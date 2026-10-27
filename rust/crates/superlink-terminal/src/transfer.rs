//! Cooperative cancellation and bounded streaming copies.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::watch;

use crate::error::{Result, TerminalError};

/// Default ceiling for a single file transfer (8 GiB).
pub const DEFAULT_MAX_TRANSFER_BYTES: u64 = 8 << 30;

/// Chunk size used for every streamed transfer.
pub(crate) const COPY_BUFFER_BYTES: usize = 64 << 10;

/// Progress callback receiving the bytes transferred so far.
pub type ProgressFn = Arc<dyn Fn(u64) + Send + Sync>;

#[derive(Debug)]
struct CancelInner {
    flag: AtomicBool,
    sender: watch::Sender<bool>,
}

/// Shared cancellation flag that also wakes asynchronous waiters.
#[derive(Clone, Debug)]
pub struct CancelToken {
    inner: Arc<CancelInner>,
}

impl Default for CancelToken {
    fn default() -> Self {
        Self::new()
    }
}

impl CancelToken {
    pub fn new() -> Self {
        let (sender, _receiver) = watch::channel(false);
        Self {
            inner: Arc::new(CancelInner {
                flag: AtomicBool::new(false),
                sender,
            }),
        }
    }

    /// Requests cancellation. Idempotent and safe from any thread.
    pub fn cancel(&self) {
        self.inner.flag.store(true, Ordering::SeqCst);
        self.inner.sender.send_replace(true);
    }

    pub fn is_cancelled(&self) -> bool {
        self.inner.flag.load(Ordering::SeqCst)
    }

    /// Resolves once [Self::cancel] has been called.
    pub async fn cancelled(&self) {
        if self.is_cancelled() {
            return;
        }
        let mut receiver = self.inner.sender.subscribe();
        if *receiver.borrow() {
            return;
        }
        while receiver.changed().await.is_ok() {
            if *receiver.borrow() {
                return;
            }
        }
    }

    pub(crate) fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(TerminalError::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// Streaming limits shared by uploads and downloads.
#[derive(Clone)]
pub struct TransferOptions {
    pub max_bytes: u64,
    pub cancel: CancelToken,
    pub progress: Option<ProgressFn>,
}

impl Default for TransferOptions {
    fn default() -> Self {
        Self {
            max_bytes: DEFAULT_MAX_TRANSFER_BYTES,
            cancel: CancelToken::new(),
            progress: None,
        }
    }
}

impl fmt::Debug for TransferOptions {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TransferOptions")
            .field("max_bytes", &self.max_bytes)
            .field("cancelled", &self.cancel.is_cancelled())
            .field("progress", &self.progress.is_some())
            .finish()
    }
}

impl TransferOptions {
    pub fn with_progress(mut self, progress: impl Fn(u64) + Send + Sync + 'static) -> Self {
        self.progress = Some(Arc::new(progress));
        self
    }

    fn report(&self, total: u64) {
        if let Some(progress) = &self.progress {
            progress(total);
        }
    }
}

/// Copies at most max_bytes, checking cancellation between chunks and
/// reporting monotonically increasing totals.
pub(crate) async fn copy_limited<R, W>(
    reader: &mut R,
    writer: &mut W,
    options: &TransferOptions,
) -> Result<u64>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut buffer = vec![0u8; COPY_BUFFER_BYTES];
    let mut total: u64 = 0;
    loop {
        let read = tokio::select! {
            biased;
            _ = options.cancel.cancelled() => return Err(TerminalError::Cancelled),
            result = reader.read(&mut buffer) => result?,
        };
        if read == 0 {
            writer.flush().await?;
            return Ok(total);
        }
        total = total.saturating_add(read as u64);
        if total > options.max_bytes {
            return Err(TerminalError::TransferTooLarge {
                limit: options.max_bytes,
            });
        }
        tokio::select! {
            biased;
            _ = options.cancel.cancelled() => return Err(TerminalError::Cancelled),
            result = writer.write_all(&buffer[..read]) => result?,
        }
        options.report(total);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[tokio::test]
    async fn copies_and_reports_monotonic_totals() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        let options = TransferOptions::default().with_progress(move |total| {
            sink.lock().unwrap().push(total);
        });
        let data = vec![7u8; COPY_BUFFER_BYTES + 10];
        let mut source: &[u8] = &data;
        let mut destination: Vec<u8> = Vec::new();
        let total = copy_limited(&mut source, &mut destination, &options)
            .await
            .unwrap();
        assert_eq!(total as usize, COPY_BUFFER_BYTES + 10);
        assert_eq!(destination.len(), COPY_BUFFER_BYTES + 10);
        let seen = seen.lock().unwrap();
        assert_eq!(
            *seen,
            vec![COPY_BUFFER_BYTES as u64, (COPY_BUFFER_BYTES + 10) as u64]
        );
    }

    #[tokio::test]
    async fn stops_at_the_size_limit() {
        let options = TransferOptions {
            max_bytes: 4,
            ..TransferOptions::default()
        };
        let mut source: &[u8] = b"0123456789";
        let mut destination: Vec<u8> = Vec::new();
        let error = copy_limited(&mut source, &mut destination, &options)
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            TerminalError::TransferTooLarge { limit: 4 }
        ));
    }

    #[tokio::test]
    async fn cancelled_copies_do_nothing() {
        let options = TransferOptions::default();
        options.cancel.cancel();
        let mut source: &[u8] = b"data";
        let mut destination: Vec<u8> = Vec::new();
        let error = copy_limited(&mut source, &mut destination, &options)
            .await
            .unwrap_err();
        assert!(matches!(error, TerminalError::Cancelled));
        assert!(destination.is_empty());
    }

    #[test]
    fn cancellation_is_shared_across_clones() {
        let token = CancelToken::new();
        let clone = token.clone();
        assert!(!clone.is_cancelled());
        token.cancel();
        assert!(clone.is_cancelled());
        assert!(clone.check().is_err());
    }
}
