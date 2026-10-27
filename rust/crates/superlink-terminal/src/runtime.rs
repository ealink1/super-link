//! Shared Tokio runtime used by every SSH transport.
//!
//! Blocking public APIs enter this runtime with Runtime::block_on, while the
//! connection, PTY and SFTP tasks keep running on its worker threads.

use std::sync::OnceLock;

use tokio::runtime::Runtime;

use crate::error::{Result, TerminalError};

static RUNTIME: OnceLock<std::result::Result<Runtime, String>> = OnceLock::new();

pub(crate) fn runtime() -> Result<&'static Runtime> {
    match RUNTIME.get_or_init(|| Runtime::new().map_err(|error| error.to_string())) {
        Ok(runtime) => Ok(runtime),
        Err(message) => Err(TerminalError::Other(format!(
            "无法启动 SSH 运行时：{message}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_a_shared_runtime() {
        let first = runtime().unwrap();
        let second = runtime().unwrap();
        assert!(std::ptr::eq(first, second));
    }
}
