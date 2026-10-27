//! Cloneable write side of a terminal session.
//!
//! A session cannot be shared behind a mutex: its read call blocks, so any
//! concurrent write would starve. The handle only carries the cheap write
//! path, which lets one thread drain output while another types.

use std::sync::Arc;

use crate::error::Result;

type WriteFn = Arc<dyn Fn(&[u8]) -> Result<()> + Send + Sync>;
type ResizeFn = Arc<dyn Fn(u16, u16) -> Result<()> + Send + Sync>;
type CloseFn = Arc<dyn Fn() -> Result<()> + Send + Sync>;

#[derive(Clone)]
pub struct SessionHandle {
    write: WriteFn,
    resize: ResizeFn,
    close: CloseFn,
}

impl SessionHandle {
    pub fn new(
        write: impl Fn(&[u8]) -> Result<()> + Send + Sync + 'static,
        resize: impl Fn(u16, u16) -> Result<()> + Send + Sync + 'static,
        close: impl Fn() -> Result<()> + Send + Sync + 'static,
    ) -> Self {
        Self {
            write: Arc::new(write),
            resize: Arc::new(resize),
            close: Arc::new(close),
        }
    }

    pub fn write(&self, data: &[u8]) -> Result<()> {
        (self.write)(data)
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        (self.resize)(cols, rows)
    }

    /// Idempotent for both transports.
    pub fn close(&self) -> Result<()> {
        (self.close)()
    }
}
