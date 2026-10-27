use std::io;

use superlink_domain::DomainError;

/// Result type used across the terminal transport.
pub type Result<T> = std::result::Result<T, TerminalError>;

/// Transport failures. Messages never carry credentials, key material or
/// command output, so they are safe to show and log.
#[derive(Debug, thiserror::Error)]
pub enum TerminalError {
    #[error("{0}")]
    Validation(String),
    #[error("连接 SSH：{0}")]
    Connect(String),
    #[error("SSH 握手：{0}")]
    Handshake(String),
    #[error("认证失败，请检查用户名、密码或私钥")]
    Authentication,
    #[error("尚未信任的 SSH 主机指纹：{fingerprint}")]
    HostKeyUnknown { fingerprint: String },
    #[error("SSH 主机指纹发生变化，连接已阻止，请核实服务器身份")]
    HostKeyChanged { expected: String, observed: String },
    #[error("操作已取消")]
    Cancelled,
    #[error("连接超时")]
    Timeout,
    #[error("SSH 通道：{0}")]
    Channel(String),
    #[error("SFTP：{0}")]
    Sftp(String),
    #[error("远程路径无效")]
    InvalidRemotePath,
    #[error("传输超过 {limit} 字节上限")]
    TransferTooLarge { limit: u64 },
    #[error("远程文件已存在，请使用不同名称")]
    RemoteExists,
    #[error("本地文件已存在，请使用不同名称")]
    LocalExists,
    #[error("监控输出超过预算")]
    MonitorOutputTooLarge,
    #[error("服务器监控响应格式无效")]
    MonitorFormat,
    #[error("{0}")]
    Unsupported(String),
    #[error("输入输出：{0}")]
    Io(#[from] io::Error),
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error("SSH：{0}")]
    Russh(#[from] russh::Error),
    #[error("{0}")]
    Other(String),
}

impl TerminalError {
    pub(crate) fn channel(error: impl std::fmt::Display) -> Self {
        Self::Channel(error.to_string())
    }

    pub(crate) fn sftp(error: impl std::fmt::Display) -> Self {
        Self::Sftp(error.to_string())
    }
}
