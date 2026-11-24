//! Cancellation belongs to one operation and never to a reusable connection.
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicBool, Ordering},
};

type Interrupt = Box<dyn FnOnce() + Send>;

#[derive(Clone, Default)]
pub struct QueryCancel(Arc<State>);

#[derive(Default)]
struct State {
    cancelled: AtomicBool,
    interrupt: Mutex<Option<Interrupt>>,
    active: Mutex<bool>,
    completed: Condvar,
}

impl QueryCancel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::Acquire)
    }

    /// Call on a worker: a remote driver's cancellation may itself do I/O.
    pub fn cancel(&self) {
        self.0.cancelled.store(true, Ordering::Release);
        let interrupt = self
            .0
            .interrupt
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        if let Some(interrupt) = interrupt {
            interrupt();
        }
    }

    pub fn on_cancel(&self, interrupt: impl FnOnce() + Send + 'static) {
        let mut slot = self.0.interrupt.lock().unwrap_or_else(|e| e.into_inner());
        if self.is_cancelled() {
            drop(slot);
            interrupt();
        } else {
            *slot = Some(Box::new(interrupt));
        }
    }

    pub fn begin(&self) {
        *self.0.active.lock().unwrap_or_else(|e| e.into_inner()) = true;
    }
    pub fn finish(&self) {
        *self.0.active.lock().unwrap_or_else(|e| e.into_inner()) = false;
        self.0.completed.notify_all();
    }
    pub fn wait_finished(&self, timeout: std::time::Duration) -> bool {
        let active = self.0.active.lock().unwrap_or_else(|e| e.into_inner());
        let (active, _) = self
            .0
            .completed
            .wait_timeout_while(active, timeout, |active| *active)
            .unwrap_or_else(|e| e.into_inner());
        !*active
    }
    pub fn clear(&self) {
        self.0
            .interrupt
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
    }
}

impl std::fmt::Debug for QueryCancel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QueryCancel")
            .field("cancelled", &self.is_cancelled())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shutdown_waits_until_the_operation_has_released_resources() {
        let cancel = QueryCancel::new();
        cancel.begin();
        assert!(!cancel.wait_finished(std::time::Duration::from_millis(1)));
        let worker = cancel.clone();
        let task = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(10));
            worker.finish();
        });
        cancel.cancel();
        assert!(cancel.wait_finished(std::time::Duration::from_secs(1)));
        task.join().unwrap();
    }
    #[test]
    fn cancellation_before_registration_still_interrupts() {
        let cancel = QueryCancel::new();
        cancel.cancel();
        let interrupted = Arc::new(AtomicBool::new(false));
        let flag = interrupted.clone();
        cancel.on_cancel(move || {
            flag.store(true, Ordering::Release);
        });
        assert!(interrupted.load(Ordering::Acquire));
    }
    #[test]
    fn only_the_current_operation_can_be_interrupted() {
        let cancel = QueryCancel::new();
        cancel.on_cancel(|| panic!("retired operation"));
        cancel.clear();
        cancel.cancel();
        assert!(cancel.is_cancelled());
    }
}
