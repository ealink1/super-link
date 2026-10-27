//! SSH transport: bounded setup, host key verification and authentication.
//!
//! The public API is blocking. Internally a shared Tokio runtime drives the
//! russh connection so a terminal channel can stream while SFTP and probe
//! channels share the same verified transport.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::Disconnect;
use russh::client::{self, AuthResult, Handle};
use russh::keys::PrivateKeyWithHashAlg;
use russh::keys::PublicKeyOrCertificate;
use superlink_domain::ShellHost;
use tokio::net::TcpStream;
use tokio::time::Instant;

use crate::auth::{AuthMethod, load_private_key};
use crate::connect_progress::{ConnectEvent, ConnectProgress, ConnectStage};
use crate::error::{Result, TerminalError};
use crate::fingerprint::fingerprint_sha256;
use crate::host_key::HostKeyPolicy;
use crate::runtime::runtime;
use crate::session::ShellSession;
use crate::transfer::CancelToken;

/// Total setup budget, matching the Go implementation.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
/// TCP-only budget before the handshake starts.
const TCP_TIMEOUT: Duration = Duration::from_secs(15);

/// Terminal geometry clamped to a sane, allocatable range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalGeometry {
    pub cols: u16,
    pub rows: u16,
}

impl Default for TerminalGeometry {
    fn default() -> Self {
        Self { cols: 80, rows: 24 }
    }
}

impl TerminalGeometry {
    pub fn new(cols: u16, rows: u16) -> Self {
        Self {
            cols: cols.clamp(2, 500),
            rows: rows.clamp(2, 200),
        }
    }
}

/// Successful interactive connection, including its first PTY.
pub struct ConnectOutcome {
    pub client: SshClient,
    pub session: ShellSession,
    pub fingerprint: String,
    /// The observed fingerprint should be persisted by the caller.
    pub remember: bool,
}

/// Result of a credential probe that never opens a shell.
#[derive(Debug)]
pub struct ProbeOutcome {
    pub fingerprint: String,
    pub remember: bool,
}

/// A verified SSH transport. Cheap to clone through an Arc internally; the
/// connection stays alive while any session or SFTP client references it.
pub struct SshClient {
    handle: Arc<Handle<ClientHandler>>,
    fingerprint: String,
    remember: bool,
}

impl SshClient {
    /// Connects, authenticates and opens an interactive PTY.
    ///
    /// An unknown host key is reported as [TerminalError::HostKeyUnknown] so
    /// the caller can ask the user; a changed pin is [TerminalError::HostKeyChanged].
    pub fn connect(
        host: &ShellHost,
        policy: HostKeyPolicy,
        geometry: TerminalGeometry,
        cancel: &CancelToken,
        observer: Option<&mut dyn FnMut(ConnectEvent)>,
    ) -> Result<ConnectOutcome> {
        let mut progress = ConnectProgress::new(observer);
        progress.started(ConnectStage::Tcp);
        match Self::connect_inner(host, policy, geometry, cancel, &mut progress) {
            Ok(outcome) => Ok(outcome),
            Err(error) => {
                progress.failed();
                Err(error)
            }
        }
    }

    fn connect_inner(
        host: &ShellHost,
        policy: HostKeyPolicy,
        geometry: TerminalGeometry,
        cancel: &CancelToken,
        progress: &mut ConnectProgress<'_>,
    ) -> Result<ConnectOutcome> {
        host.validate()?;
        let auth = PreparedAuth::new(host)?;
        let runtime = runtime()?;
        let address = socket_address(host);
        let (handle, fingerprint, remember, session) = runtime.block_on(async {
            let deadline = Instant::now() + CONNECT_TIMEOUT;
            let socket = connect_tcp(&address, cancel).await?;
            progress.completed(ConnectStage::Tcp);
            progress.started(ConnectStage::Auth);
            let (mut handle, observed, remember) =
                handshake(socket, policy.clone(), cancel, deadline).await?;
            authenticate(&mut handle, host, auth, cancel, deadline).await?;
            progress.completed(ConnectStage::Auth);
            let handle = Arc::new(handle);
            progress.started(ConnectStage::Channel);
            let session = ShellSession::open(Arc::clone(&handle), geometry, cancel).await?;
            progress.completed(ConnectStage::Channel);
            progress.started(ConnectStage::Ready);
            cancel.check()?;
            progress.completed(ConnectStage::Ready);
            Ok::<_, TerminalError>((handle, observed, remember, session))
        })?;
        Ok(ConnectOutcome {
            client: Self {
                handle,
                fingerprint: fingerprint.clone(),
                remember,
            },
            session,
            fingerprint,
            remember,
        })
    }

    /// Verifies credentials and reports the server identity without opening a
    /// shell, so probing an unknown host leaves no remote session behind.
    pub fn probe(
        host: &ShellHost,
        policy: HostKeyPolicy,
        cancel: &CancelToken,
    ) -> Result<ProbeOutcome> {
        host.validate()?;
        let auth = PreparedAuth::new(host)?;
        let runtime = runtime()?;
        let address = socket_address(host);
        let (fingerprint, remember) = runtime.block_on(async {
            let deadline = Instant::now() + CONNECT_TIMEOUT;
            let socket = connect_tcp(&address, cancel).await?;
            let (mut handle, observed, remember) =
                handshake(socket, policy, cancel, deadline).await?;
            authenticate(&mut handle, host, auth, cancel, deadline).await?;
            let _ = handle.disconnect(Disconnect::ByApplication, "", "en").await;
            Ok::<_, TerminalError>((observed, remember))
        })?;
        Ok(ProbeOutcome {
            fingerprint,
            remember,
        })
    }

    /// Opens an additional interactive PTY over the verified transport.
    pub fn spawn_shell(&self, cols: u16, rows: u16, cancel: &CancelToken) -> Result<ShellSession> {
        let geometry = TerminalGeometry::new(cols, rows);
        runtime()?.block_on(ShellSession::open(
            Arc::clone(&self.handle),
            geometry,
            cancel,
        ))
    }

    /// Pinned fingerprint observed on this connection.
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    /// Whether the caller asked to persist the observed fingerprint.
    pub fn remember_fingerprint(&self) -> bool {
        self.remember
    }

    pub fn is_closed(&self) -> bool {
        self.handle.is_closed()
    }

    /// Closes the transport; live sessions and SFTP clients observe EOF.
    pub fn disconnect(&self) -> Result<()> {
        runtime()?.block_on(async {
            match self
                .handle
                .disconnect(Disconnect::ByApplication, "", "en")
                .await
            {
                Ok(()) => Ok(()),
                Err(_) if self.handle.is_closed() => Ok(()),
                Err(error) => Err(TerminalError::Russh(error)),
            }
        })
    }

    pub(crate) fn handle(&self) -> &Arc<Handle<ClientHandler>> {
        &self.handle
    }
}

/// Credentials prepared before dialing so key errors surface immediately.
enum PreparedAuth {
    Password(String),
    Key(PrivateKeyWithHashAlg),
}

impl PreparedAuth {
    fn new(host: &ShellHost) -> Result<Self> {
        match AuthMethod::for_host(host)? {
            AuthMethod::Password(password) => Ok(Self::Password(password)),
            AuthMethod::PrivateKey { path, passphrase } => {
                Ok(Self::Key(load_private_key(&path, passphrase.as_deref())?))
            }
        }
    }
}

/// Host key check performed inside the handshake.
pub(crate) struct ClientHandler {
    policy: HostKeyPolicy,
    observed: Arc<Mutex<Option<String>>>,
    remember: Arc<AtomicBool>,
}

impl client::Handler for ClientHandler {
    type Error = TerminalError;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKeyOrCertificate,
    ) -> std::result::Result<bool, Self::Error> {
        let observed = fingerprint_sha256(&server_public_key.public_key());
        if let Ok(mut slot) = self.observed.lock() {
            *slot = Some(observed.clone());
        }
        let decision = self.policy.decide(&observed)?;
        self.remember.store(decision.remember, Ordering::SeqCst);
        Ok(true)
    }
}

fn socket_address(host: &ShellHost) -> String {
    if host.host.contains(':') && !host.host.starts_with('[') {
        format!("[{}]:{}", host.host, host.port)
    } else {
        format!("{}:{}", host.host, host.port)
    }
}

async fn connect_tcp(address: &str, cancel: &CancelToken) -> Result<TcpStream> {
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(TerminalError::Cancelled),
        result = tokio::time::timeout(TCP_TIMEOUT, TcpStream::connect(address)) => {
            match result {
                Ok(Ok(socket)) => Ok(socket),
                Ok(Err(error)) => Err(TerminalError::Connect(error.to_string())),
                Err(_) => Err(TerminalError::Timeout),
            }
        }
    }
}

async fn handshake(
    socket: TcpStream,
    policy: HostKeyPolicy,
    cancel: &CancelToken,
    deadline: Instant,
) -> Result<(Handle<ClientHandler>, String, bool)> {
    let observed = Arc::new(Mutex::new(None));
    let remember = Arc::new(AtomicBool::new(false));
    let handler = ClientHandler {
        policy,
        observed: Arc::clone(&observed),
        remember: Arc::clone(&remember),
    };
    let config = client::Config {
        inactivity_timeout: None,
        keepalive_interval: Some(Duration::from_secs(30)),
        nodelay: true,
        ..client::Config::default()
    };
    let connect = client::connect_stream(Arc::new(config), socket, handler);
    let handle = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(TerminalError::Cancelled),
        result = tokio::time::timeout_at(deadline, connect) => {
            match result {
                Ok(Ok(handle)) => handle,
                Ok(Err(error)) => return Err(error),
                Err(_) => return Err(TerminalError::Timeout),
            }
        }
    };
    let fingerprint = observed
        .lock()
        .ok()
        .and_then(|slot| slot.clone())
        .ok_or_else(|| TerminalError::Handshake("未获得服务器主机指纹".into()))?;
    Ok((handle, fingerprint, remember.load(Ordering::SeqCst)))
}

async fn authenticate(
    handle: &mut Handle<ClientHandler>,
    host: &ShellHost,
    auth: PreparedAuth,
    cancel: &CancelToken,
    deadline: Instant,
) -> Result<()> {
    let result = match auth {
        PreparedAuth::Password(password) => {
            let future = handle.authenticate_password(&host.user, password);
            tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(TerminalError::Cancelled),
                result = tokio::time::timeout_at(deadline, future) => result,
            }
        }
        PreparedAuth::Key(key) => {
            let future = handle.authenticate_publickey(&host.user, key);
            tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(TerminalError::Cancelled),
                result = tokio::time::timeout_at(deadline, future) => result,
            }
        }
    };
    match result {
        Ok(Ok(AuthResult::Success)) => Ok(()),
        Ok(Ok(AuthResult::Failure { .. })) => Err(TerminalError::Authentication),
        Ok(Err(error)) => Err(TerminalError::Russh(error)),
        Err(_) => Err(TerminalError::Timeout),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host() -> ShellHost {
        ShellHost {
            name: "fixture".into(),
            host: "127.0.0.1".into(),
            user: "tester".into(),
            port: 22,
            ..ShellHost::default()
        }
    }

    #[test]
    fn geometry_is_clamped_to_a_usable_range() {
        assert_eq!(
            TerminalGeometry::default(),
            TerminalGeometry { cols: 80, rows: 24 }
        );
        assert_eq!(
            TerminalGeometry::new(0, 0),
            TerminalGeometry { cols: 2, rows: 2 }
        );
        assert_eq!(
            TerminalGeometry::new(u16::MAX, u16::MAX),
            TerminalGeometry {
                cols: 500,
                rows: 200
            }
        );
    }

    #[test]
    fn ipv6_endpoints_are_bracketed() {
        let mut host = host();
        host.host = "::1".into();
        assert_eq!(socket_address(&host), "[::1]:22");
        host.host = "example.test".into();
        assert_eq!(socket_address(&host), "example.test:22");
    }

    #[test]
    fn invalid_hosts_are_rejected_before_dialing() {
        let mut host = host();
        host.host = "bad host".into();
        let error = SshClient::probe(&host, HostKeyPolicy::Ask, &CancelToken::new()).unwrap_err();
        assert!(matches!(error, TerminalError::Domain(_)));
    }

    #[test]
    fn missing_credentials_fail_before_dialing() {
        let error = SshClient::probe(&host(), HostKeyPolicy::Ask, &CancelToken::new()).unwrap_err();
        assert_eq!(error.to_string(), "请输入登录密码或选择私钥");
    }
}
