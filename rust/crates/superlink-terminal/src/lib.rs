//! SSH terminal transport, SFTP transfers and server monitoring.
//!
//! Ported from the Go implementation in internal/infra/shell. Nothing here
//! depends on a UI toolkit: the public API blocks on a shared Tokio runtime
//! and the UI is responsible for marshalling calls onto its own thread.
//!
//! Security posture:
//! - an unknown host key is reported, never silently trusted;
//! - a changed pinned fingerprint aborts the handshake;
//! - passwords, passphrases and key material are never logged or formatted;
//! - probes and file operations validate paths and bound all server output.
//!
//! Threading contract: every public method blocks on a shared Tokio runtime.
//! Call them off the UI thread and outside any other runtime, and drain
//! [session::Session::read] on a dedicated thread: writes apply backpressure
//! when the terminal output queue is full.
//!
//! Coverage notes: remote monitoring targets Linux /proc and NVIDIA GPUs via
//! nvidia-smi; other hosts report [error::TerminalError::Unsupported]. Local
//! monitoring uses sysinfo and reports CPU totals only.

#![forbid(unsafe_code)]

pub mod auth;
pub mod connect_progress;
pub mod error;
pub mod files;
pub mod fingerprint;
pub mod host_key;
pub mod local;
pub mod monitor;
pub mod runtime;
pub mod session;
pub mod sftp;
pub mod ssh;
pub mod transfer;

pub use connect_progress::{ConnectEvent, ConnectStage, ConnectState};
pub use error::{Result, TerminalError};
pub use files::{RemoteFile, format_mode, normalize_remote_path, validate_remote_path};
pub use fingerprint::{
    fingerprint_from_bytes, fingerprint_from_openssh, fingerprint_sha256, fingerprints_match,
};
pub use host_key::{HostKeyDecision, HostKeyPolicy};
pub use local::LocalSession;
pub use monitor::{
    CpuCounter, DiskCounter, Gpu, LargeFile, Listener, LocalMonitor, Metrics, MonitorDetails,
    MonitorRates, MonitorSnapshot, NetworkCounter, Partition, ProcessMetric,
};
pub use session::{Session, SessionState, ShellSession};
pub use sftp::{MAX_DIRECTORY_ENTRIES, SftpClient};
pub use ssh::{ConnectOutcome, ProbeOutcome, SshClient, TerminalGeometry};
pub use transfer::{CancelToken, DEFAULT_MAX_TRANSFER_BYTES, ProgressFn, TransferOptions};
