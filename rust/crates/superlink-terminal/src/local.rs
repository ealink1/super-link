//! Local shell sessions backed by a real pseudo-terminal.
//!
//! Reads are pumped by a dedicated thread into a bounded queue, so closing the
//! session wakes a blocked reader with EOF instead of leaving it stuck on a
//! file descriptor that never becomes readable again.

use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use tokio::sync::mpsc;

use crate::error::{Result, TerminalError};
use crate::handle::SessionHandle;
use crate::session::{OutputQueue, Session, SessionState};
use crate::ssh::TerminalGeometry;

const OUTPUT_QUEUE: usize = 64;
const READ_BUFFER_BYTES: usize = 8 << 10;

/// A login shell running in a local PTY.
pub struct LocalSession {
    master: Arc<Mutex<Option<Box<dyn MasterPty + Send>>>>,
    writer: Arc<Mutex<Option<Box<dyn Write + Send>>>>,
    child: Arc<Mutex<Option<Box<dyn Child + Send>>>>,
    output: OutputQueue,
    state: SessionState,
    geometry: TerminalGeometry,
}

impl LocalSession {
    /// Starts a login shell. An empty executable means $SHELL, else /bin/sh.
    pub fn open(executable: Option<&str>, geometry: TerminalGeometry) -> Result<Self> {
        let executable = resolve_executable(executable)?;
        let pair = native_pty_system()
            .openpty(pty_size(geometry))
            .map_err(pty_error)?;
        let mut command = CommandBuilder::new(executable.as_os_str());
        command.arg("-l");
        if let Some(home) = std::env::var_os("HOME") {
            command.cwd(home);
        }
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        let child = pair.slave.spawn_command(command).map_err(pty_error)?;
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().map_err(pty_error)?;
        let writer = pair.master.take_writer().map_err(pty_error)?;
        let (output_tx, output_rx) = mpsc::channel(OUTPUT_QUEUE);
        thread::Builder::new()
            .name("superlink-local-pty".into())
            .spawn(move || pump(reader.as_mut(), output_tx))
            .map_err(TerminalError::Io)?;
        Ok(Self {
            master: Arc::new(Mutex::new(Some(pair.master))),
            writer: Arc::new(Mutex::new(Some(writer))),
            child: Arc::new(Mutex::new(Some(child))),
            output: OutputQueue::new(output_rx),
            state: SessionState::Open,
            geometry,
        })
    }

    pub fn geometry(&self) -> TerminalGeometry {
        self.geometry
    }

    pub fn is_open(&self) -> bool {
        self.state == SessionState::Open
    }

    /// Write side that can be cloned and used while another thread blocks in
    /// read. This is what the UI needs: reading must never hold the writer.
    pub fn handle(&self) -> SessionHandle {
        let writer = Arc::clone(&self.writer);
        let master = Arc::clone(&self.master);
        let child = Arc::clone(&self.child);
        SessionHandle::new(
            move |data| {
                let mut guard = writer.lock().map_err(|_| closed())?;
                let Some(writer) = guard.as_mut() else {
                    return Err(closed());
                };
                writer.write_all(data).map_err(TerminalError::Io)
            },
            move |cols, rows| {
                let geometry = TerminalGeometry::new(cols, rows);
                let guard = master.lock().map_err(|_| closed())?;
                let Some(master) = guard.as_ref() else {
                    return Err(closed());
                };
                master.resize(pty_size(geometry)).map_err(pty_error)
            },
            move || {
                let mut guard = child.lock().map_err(|_| closed())?;
                if let Some(mut child) = guard.take() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                Ok(())
            },
        )
    }
}

impl Session for LocalSession {
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize> {
        self.output.read(buffer)
    }

    fn write(&mut self, data: &[u8]) -> Result<()> {
        if self.state == SessionState::Closed {
            return Err(TerminalError::Io(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "终端已关闭",
            )));
        }
        let mut guard = self.writer.lock().map_err(|_| closed())?;
        let Some(writer) = guard.as_mut() else {
            return Err(closed());
        };
        writer.write_all(data).map_err(TerminalError::Io)
    }

    fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        if self.state == SessionState::Closed {
            return Err(TerminalError::Io(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "终端已关闭",
            )));
        }
        let geometry = TerminalGeometry::new(cols, rows);
        if geometry == self.geometry {
            return Ok(());
        }
        let guard = self.master.lock().map_err(|_| closed())?;
        let Some(master) = guard.as_ref() else {
            return Err(closed());
        };
        master.resize(pty_size(geometry)).map_err(pty_error)?;
        self.geometry = geometry;
        Ok(())
    }

    fn close(&mut self) -> Result<()> {
        if self.state == SessionState::Closed {
            return Ok(());
        }
        self.state = SessionState::Closed;
        self.output.close();
        if let Ok(mut guard) = self.child.lock()
            && let Some(mut child) = guard.take()
        {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Ok(mut guard) = self.writer.lock() {
            guard.take();
        }
        if let Ok(mut guard) = self.master.lock() {
            guard.take();
        }
        Ok(())
    }

    fn state(&self) -> SessionState {
        self.state
    }
}

impl Drop for LocalSession {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

fn closed() -> TerminalError {
    TerminalError::Io(io::Error::new(io::ErrorKind::BrokenPipe, "终端已关闭"))
}

fn pump(reader: &mut dyn Read, output: mpsc::Sender<Vec<u8>>) {
    let mut buffer = [0u8; READ_BUFFER_BYTES];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                if output.blocking_send(buffer[..read].to_vec()).is_err() {
                    break;
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            // EIO signals that the PTY closed; it is the normal EOF here.
            Err(_) => break,
        }
    }
}

fn resolve_executable(explicit: Option<&str>) -> Result<PathBuf> {
    let candidate = explicit
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("SHELL")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        })
        .unwrap_or_else(|| PathBuf::from("/bin/sh"));
    if !candidate.is_absolute() {
        return Err(TerminalError::Validation(
            "本地壳必须是可执行文件的绝对路径".into(),
        ));
    }
    Ok(candidate)
}

fn pty_size(geometry: TerminalGeometry) -> PtySize {
    PtySize {
        rows: geometry.rows,
        cols: geometry.cols,
        pixel_width: 0,
        pixel_height: 0,
    }
}

fn pty_error(error: impl std::fmt::Display) -> TerminalError {
    TerminalError::Io(io::Error::other(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_absolute_executables_are_accepted() {
        assert!(resolve_executable(Some("sh")).is_err());
        assert!(resolve_executable(Some("/bin/sh")).is_ok());
        assert!(resolve_executable(None).is_ok());
    }

    #[test]
    fn geometry_maps_to_pty_dimensions() {
        let size = pty_size(TerminalGeometry::new(120, 40));
        assert_eq!((size.cols, size.rows), (120, 40));
        assert_eq!(pty_size(TerminalGeometry::new(0, 0)).cols, 2);
    }

    #[test]
    #[ignore = "requires an interactive shell on the host"]
    fn local_shell_echoes_and_closes_with_eof() {
        let mut session = LocalSession::open(Some("/bin/sh"), TerminalGeometry::default()).unwrap();
        session.write(b"echo superlink-ready\n").unwrap();
        let mut collected = Vec::new();
        let mut buffer = [0u8; 1024];
        while collected.len() < 1024 {
            let read = session.read(&mut buffer).unwrap();
            if read == 0 {
                break;
            }
            collected.extend_from_slice(&buffer[..read]);
            if String::from_utf8_lossy(&collected).contains("superlink-ready") {
                break;
            }
        }
        assert!(String::from_utf8_lossy(&collected).contains("superlink-ready"));
        session.close().unwrap();
        assert_eq!(session.read(&mut buffer).unwrap(), 0);
    }
}
