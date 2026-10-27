//! Cooperative cancellation.
//!
//! The ported code checks a caller-supplied token between records, statements
//! and byte blocks instead of threading a Go context.Context through.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::{FileError, Result};

/// Token polled by long-running readers and writers.
pub trait Cancellation {
    /// Returns true once the caller has cancelled the operation.
    fn is_cancelled(&self) -> bool;
}

/// Token that is never cancelled.
#[derive(Debug, Clone, Copy, Default)]
pub struct NeverCancelled;

impl Cancellation for NeverCancelled {
    fn is_cancelled(&self) -> bool {
        false
    }
}

/// Cloneable token flipped by another component.
#[derive(Debug, Clone, Default)]
pub struct CancelFlag(Arc<AtomicBool>);

impl CancelFlag {
    /// Creates a token that is not yet cancelled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Cancels every clone of this token.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    /// Clears the token so it can be reused for a new operation.
    pub fn reset(&self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

impl Cancellation for CancelFlag {
    fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

impl Cancellation for AtomicBool {
    fn is_cancelled(&self) -> bool {
        self.load(Ordering::SeqCst)
    }
}

impl<T: Cancellation + ?Sized> Cancellation for &T {
    fn is_cancelled(&self) -> bool {
        (**self).is_cancelled()
    }
}

/// Returns FileError::Cancelled once the token has been flipped.
pub fn check(cancel: &dyn Cancellation) -> Result<()> {
    if cancel.is_cancelled() {
        Err(FileError::Cancelled)
    } else {
        Ok(())
    }
}
