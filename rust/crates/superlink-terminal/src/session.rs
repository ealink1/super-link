//! Interactive terminal sessions.
//!
//! A session is a small state machine: an open channel that forwards writes,
//! resizes and output, and a closed state where reads return EOF immediately
//! instead of blocking on a channel that may never answer.

use std::io;
use std::sync::Arc;
use std::time::Duration;

use russh::client::{Handle, Msg};
use russh::{ChannelMsg, ChannelReadHalf, ChannelWriteHalf, Pty};
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;

use crate::error::{Result, TerminalError};
use crate::ssh::{ClientHandler, TerminalGeometry};
use crate::transfer::CancelToken;

/// Buffered messages between the UI thread and the channel task.
const SESSION_QUEUE: usize = 64;
const TERMINAL_TYPE: &str = "xterm-256color";
const SHELL_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
/// Cap for output that arrives before the shell confirms the request.
const MAX_SETUP_OUTPUT: usize = 256 << 10;

/// Lifecycle of a terminal session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Open,
    Closed,
}

/// Transport-independent terminal session.
pub trait Session: Send {
    /// Reads terminal output. Returns 0 on EOF, including after [Session::close].
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize>;
    fn write(&mut self, data: &[u8]) -> Result<()>;
    fn resize(&mut self, cols: u16, rows: u16) -> Result<()>;
    fn close(&mut self) -> Result<()>;
    fn state(&self) -> SessionState;
}

enum SessionCommand {
    Write(Vec<u8>),
    Resize { cols: u16, rows: u16 },
    Close,
}

/// Bounded output queue that never blocks after the producer is gone.
pub(crate) struct OutputQueue {
    receiver: Option<mpsc::Receiver<Vec<u8>>>,
    chunk: Vec<u8>,
    offset: usize,
}

impl OutputQueue {
    pub(crate) fn new(receiver: mpsc::Receiver<Vec<u8>>) -> Self {
        Self {
            receiver: Some(receiver),
            chunk: Vec::new(),
            offset: 0,
        }
    }

    pub(crate) fn read(&mut self, buffer: &mut [u8]) -> Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        loop {
            if self.offset < self.chunk.len() {
                let available = &self.chunk[self.offset..];
                let take = available.len().min(buffer.len());
                buffer[..take].copy_from_slice(&available[..take]);
                self.offset += take;
                if self.offset == self.chunk.len() {
                    self.chunk.clear();
                    self.offset = 0;
                }
                return Ok(take);
            }
            let Some(receiver) = self.receiver.as_mut() else {
                return Ok(0);
            };
            match receiver.blocking_recv() {
                Some(chunk) => {
                    self.chunk = chunk;
                    self.offset = 0;
                }
                None => {
                    self.receiver = None;
                    return Ok(0);
                }
            }
        }
    }

    /// Drops the queue so a blocked reader wakes up with EOF.
    pub(crate) fn close(&mut self) {
        self.receiver = None;
        self.chunk.clear();
        self.offset = 0;
    }
}

/// A PTY channel over a verified SSH transport.
pub struct ShellSession {
    commands: Option<mpsc::Sender<SessionCommand>>,
    output: OutputQueue,
    state: SessionState,
    geometry: TerminalGeometry,
    /// Keeps the SSH transport alive for as long as the terminal is open.
    _client: Option<Arc<Handle<ClientHandler>>>,
}

impl ShellSession {
    /// Opens a PTY and waits until the server confirms both the terminal and
    /// shell requests. Must run inside the shared runtime.
    pub(crate) async fn open(
        handle: Arc<Handle<ClientHandler>>,
        geometry: TerminalGeometry,
        cancel: &CancelToken,
    ) -> Result<Self> {
        let (commands, command_rx) = mpsc::channel(SESSION_QUEUE);
        let (output_tx, output_rx) = mpsc::channel(SESSION_QUEUE);
        let (setup_tx, setup_rx) = oneshot::channel();
        tokio::spawn(run_shell(
            Arc::clone(&handle),
            geometry,
            command_rx,
            output_tx,
            setup_tx,
            cancel.clone(),
        ));
        match setup_rx.await {
            Ok(Ok(())) => Ok(Self {
                commands: Some(commands),
                output: OutputQueue::new(output_rx),
                state: SessionState::Open,
                geometry,
                _client: Some(handle),
            }),
            Ok(Err(error)) => Err(error),
            Err(_) => Err(TerminalError::Channel("终端启动任务意外结束".into())),
        }
    }

    pub fn geometry(&self) -> TerminalGeometry {
        self.geometry
    }

    /// Whether reads and writes are still forwarded.
    pub fn is_open(&self) -> bool {
        self.state == SessionState::Open
    }
}

impl Session for ShellSession {
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize> {
        self.output.read(buffer)
    }

    fn write(&mut self, data: &[u8]) -> Result<()> {
        if self.state == SessionState::Closed {
            return Err(closed_error());
        }
        if data.is_empty() {
            return Ok(());
        }
        let Some(commands) = self.commands.as_ref() else {
            return Err(closed_error());
        };
        match commands.blocking_send(SessionCommand::Write(data.to_vec())) {
            Ok(()) => Ok(()),
            Err(_) => {
                self.state = SessionState::Closed;
                Err(closed_error())
            }
        }
    }

    fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        if self.state == SessionState::Closed {
            return Err(closed_error());
        }
        let geometry = TerminalGeometry::new(cols, rows);
        if geometry == self.geometry {
            return Ok(());
        }
        let Some(commands) = self.commands.as_ref() else {
            return Err(closed_error());
        };
        commands
            .blocking_send(SessionCommand::Resize {
                cols: geometry.cols,
                rows: geometry.rows,
            })
            .map_err(|_| closed_error())?;
        self.geometry = geometry;
        Ok(())
    }

    fn close(&mut self) -> Result<()> {
        if self.state == SessionState::Closed {
            return Ok(());
        }
        self.state = SessionState::Closed;
        if let Some(commands) = self.commands.take() {
            let _ = commands.try_send(SessionCommand::Close);
        }
        self.output.close();
        Ok(())
    }

    fn state(&self) -> SessionState {
        self.state
    }
}

impl Drop for ShellSession {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

fn closed_error() -> TerminalError {
    TerminalError::Io(io::Error::new(io::ErrorKind::BrokenPipe, "终端已关闭"))
}

/// Opens the channel, negotiates a PTY and only then starts pumping data.
async fn run_shell(
    handle: Arc<Handle<ClientHandler>>,
    geometry: TerminalGeometry,
    mut commands: mpsc::Receiver<SessionCommand>,
    output: mpsc::Sender<Vec<u8>>,
    setup: oneshot::Sender<Result<()>>,
    cancel: CancelToken,
) {
    let channel = match tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(TerminalError::Cancelled),
        result = handle.channel_open_session() => result.map_err(TerminalError::channel),
    } {
        Ok(channel) => channel,
        Err(error) => {
            let _ = setup.send(Err(error));
            return;
        }
    };
    let (mut reader, writer) = channel.split();
    let mut buffered = Vec::new();
    let negotiated = negotiate_shell(&writer, &mut reader, &mut buffered, geometry, &cancel).await;
    if let Err(error) = negotiated {
        let _ = setup.send(Err(error));
        let _ = writer.close().await;
        return;
    }
    if setup.send(Ok(())).is_err() {
        let _ = writer.close().await;
        return;
    }
    for chunk in buffered {
        if output.send(chunk).await.is_err() {
            let _ = writer.close().await;
            return;
        }
    }
    pump(reader, writer, &mut commands, output).await;
}

async fn negotiate_shell(
    writer: &ChannelWriteHalf<Msg>,
    reader: &mut ChannelReadHalf,
    buffered: &mut Vec<Vec<u8>>,
    geometry: TerminalGeometry,
    cancel: &CancelToken,
) -> Result<()> {
    let pty = writer.request_pty(
        true,
        TERMINAL_TYPE,
        geometry.cols as u32,
        geometry.rows as u32,
        0,
        0,
        &[(Pty::ECHO, 1)],
    );
    await_request(pty, cancel).await?;
    await_reply(reader, buffered, cancel, "远程服务器拒绝了终端请求").await?;
    let shell = writer.request_shell(true);
    await_request(shell, cancel).await?;
    await_reply(reader, buffered, cancel, "远程服务器拒绝了 Shell 请求").await?;
    Ok(())
}

async fn await_request(
    request: impl std::future::Future<Output = std::result::Result<(), russh::Error>>,
    cancel: &CancelToken,
) -> Result<()> {
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(TerminalError::Cancelled),
        result = tokio::time::timeout(SHELL_REQUEST_TIMEOUT, request) => {
            match result {
                Ok(Ok(())) => Ok(()),
                Ok(Err(error)) => Err(TerminalError::channel(error)),
                Err(_) => Err(TerminalError::Timeout),
            }
        }
    }
}

/// Waits for the reply to a request with a reply flag, keeping early output.
async fn await_reply(
    reader: &mut ChannelReadHalf,
    buffered: &mut Vec<Vec<u8>>,
    cancel: &CancelToken,
    rejection: &'static str,
) -> Result<()> {
    let deadline = Instant::now() + SHELL_REQUEST_TIMEOUT;
    loop {
        let message = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(TerminalError::Cancelled),
            result = tokio::time::timeout_at(deadline, reader.wait()) => match result {
                Ok(message) => message,
                Err(_) => return Err(TerminalError::Timeout),
            },
        };
        match message {
            Some(ChannelMsg::Success) => return Ok(()),
            Some(ChannelMsg::Failure) => {
                return Err(TerminalError::Channel(rejection.to_owned()));
            }
            Some(ChannelMsg::Data { data }) | Some(ChannelMsg::ExtendedData { data, .. }) => {
                let total: usize = buffered.iter().map(Vec::len).sum();
                if total + data.len() > MAX_SETUP_OUTPUT {
                    return Err(TerminalError::Channel("终端启动输出超过预算".into()));
                }
                buffered.push(data.to_vec());
            }
            Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => {
                return Err(TerminalError::Channel("远程终端在启动时关闭".into()));
            }
            _ => {}
        }
    }
}

/// Owns both channel halves so reads and writes never borrow each other.
async fn pump(
    mut reader: ChannelReadHalf,
    writer: ChannelWriteHalf<Msg>,
    commands: &mut mpsc::Receiver<SessionCommand>,
    output: mpsc::Sender<Vec<u8>>,
) {
    loop {
        tokio::select! {
            biased;
            message = reader.wait() => match message {
                Some(ChannelMsg::Data { data })
                | Some(ChannelMsg::ExtendedData { data, .. }) => {
                    if output.send(data.to_vec()).await.is_err() {
                        let _ = writer.close().await;
                        break;
                    }
                }
                Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => break,
                _ => {}
            },
            command = commands.recv() => match command {
                Some(SessionCommand::Write(bytes)) => {
                    if writer.data_bytes(bytes).await.is_err() {
                        let _ = writer.close().await;
                        break;
                    }
                }
                Some(SessionCommand::Resize { cols, rows }) => {
                    let _ = writer.window_change(cols as u32, rows as u32, 0, 0).await;
                }
                Some(SessionCommand::Close) | None => {
                    let _ = writer.eof().await;
                    let _ = writer.close().await;
                    break;
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detached() -> (
        ShellSession,
        mpsc::Receiver<SessionCommand>,
        mpsc::Sender<Vec<u8>>,
    ) {
        let (command_tx, command_rx) = mpsc::channel(SESSION_QUEUE);
        let (output_tx, output_rx) = mpsc::channel(SESSION_QUEUE);
        let session = ShellSession {
            commands: Some(command_tx),
            output: OutputQueue::new(output_rx),
            state: SessionState::Open,
            geometry: TerminalGeometry::default(),
            _client: None,
        };
        (session, command_rx, output_tx)
    }

    #[test]
    fn reads_chunks_and_reports_eof_after_the_producer_ends() {
        let (mut session, _commands, output) = detached();
        output.blocking_send(b"hello".to_vec()).unwrap();
        let mut buffer = [0u8; 3];
        assert_eq!(session.read(&mut buffer).unwrap(), 3);
        assert_eq!(&buffer, b"hel");
        assert_eq!(session.read(&mut buffer).unwrap(), 2);
        assert_eq!(&buffer[..2], b"lo");
        drop(output);
        assert_eq!(session.read(&mut buffer).unwrap(), 0);
        assert_eq!(session.read(&mut buffer).unwrap(), 0);
    }

    #[test]
    fn close_wakes_readers_with_eof_and_blocks_writes() {
        let (mut session, _commands, _output) = detached();
        assert_eq!(session.state(), SessionState::Open);
        session.close().unwrap();
        assert_eq!(session.state(), SessionState::Closed);
        let mut buffer = [0u8; 8];
        assert_eq!(session.read(&mut buffer).unwrap(), 0);
        assert!(session.write(b"data").is_err());
        assert!(session.resize(120, 40).is_err());
        session.close().unwrap();
    }

    #[test]
    fn writes_and_resizes_are_forwarded_in_order() {
        let (mut session, mut commands, _output) = detached();
        session.write(b"abc").unwrap();
        session.resize(120, 40).unwrap();
        session.resize(120, 40).unwrap();
        session.write(b"def").unwrap();
        match commands.blocking_recv().unwrap() {
            SessionCommand::Write(bytes) => assert_eq!(bytes, b"abc"),
            _ => panic!("expected a write"),
        }
        match commands.blocking_recv().unwrap() {
            SessionCommand::Resize { cols, rows } => {
                assert_eq!((cols, rows), (120, 40));
            }
            _ => panic!("expected a resize"),
        }
        match commands.blocking_recv().unwrap() {
            SessionCommand::Write(bytes) => assert_eq!(bytes, b"def"),
            _ => panic!("expected a write"),
        }
        assert_eq!(
            session.geometry(),
            TerminalGeometry {
                cols: 120,
                rows: 40
            }
        );
    }

    #[test]
    fn resizes_stay_inside_the_supported_range() {
        let (mut session, mut commands, _output) = detached();
        session.resize(0, 0).unwrap();
        match commands.blocking_recv().unwrap() {
            SessionCommand::Resize { cols, rows } => assert_eq!((cols, rows), (2, 2)),
            _ => panic!("expected a resize"),
        }
    }
}
