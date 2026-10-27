//! Closing a local session through the cloneable write handle.
//!
//! The terminal page keeps reads on one thread and writes on another; the
//! disconnect button only has the handle, so closing it must actually end the
//! PTY instead of merely detaching.

use std::sync::mpsc;
use std::time::Duration;

use superlink_terminal::{LocalSession, Session as _, TerminalGeometry};

#[test]
fn closing_the_handle_ends_the_local_session() {
    let mut session = LocalSession::open(Some("/bin/sh"), TerminalGeometry { cols: 80, rows: 24 })
        .expect("local pty");
    let handle = session.handle();
    handle.write(b"echo ready\n").expect("write");

    let (done_tx, done_rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut buffer = [0u8; 1024];
        loop {
            match session.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
        }
        let _ = done_tx.send(());
    });

    std::thread::sleep(Duration::from_millis(200));
    handle.close().expect("close");
    assert!(
        done_rx.recv_timeout(Duration::from_secs(5)).is_ok(),
        "reader never observed EOF after close"
    );
    reader.join().expect("reader thread");
}
