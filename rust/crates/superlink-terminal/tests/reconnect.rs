//! Reconnecting after a transport drop, using the in-process SSH fixture.
//!
//! The UI schedules retries with a backoff; this test covers the part that
//! actually has to work underneath it: a fresh connect after the previous
//! session is gone, with the same pinned host key.

mod support;

use superlink_terminal::{CancelToken, HostKeyPolicy, Session as _, SshClient, TerminalGeometry};
use support::FixtureServer;

fn read_expected(session: &mut impl superlink_terminal::Session, expected: &[u8]) {
    let mut collected = Vec::new();
    let mut buffer = [0u8; 64];
    while collected.len() < expected.len() {
        let read = session.read(&mut buffer).expect("read terminal");
        assert!(read > 0, "EOF before the expected output arrived");
        collected.extend_from_slice(&buffer[..read]);
    }
    assert_eq!(collected, expected);
}

#[test]
fn reconnects_after_the_session_is_closed() {
    let fixture = FixtureServer::start();
    let host = fixture.host();
    let cancel = CancelToken::new();
    let policy = HostKeyPolicy::Pinned(FixtureServer::pin());

    let first = SshClient::connect(
        &host,
        policy.clone(),
        TerminalGeometry::default(),
        &cancel,
        None,
    )
    .expect("first connect");
    let mut session = first.session;
    session.write(b"first").expect("write");
    read_expected(&mut session, b"first");
    session.close().expect("close");
    drop(session);

    // Exactly what the terminal page does after an unexpected drop.
    let second = SshClient::connect(&host, policy, TerminalGeometry::default(), &cancel, None)
        .expect("reconnect");
    let mut session = second.session;
    session.write(b"second").expect("write");
    read_expected(&mut session, b"second");
}

#[test]
fn a_pinned_key_still_blocks_a_changed_server() {
    let fixture = FixtureServer::start();
    let host = fixture.host();
    let cancel = CancelToken::new();

    let error = SshClient::connect(
        &host,
        HostKeyPolicy::Pinned("SHA256:not-the-real-key".into()),
        TerminalGeometry::default(),
        &cancel,
        None,
    )
    .err()
    .expect("changed pin must fail");
    assert!(matches!(
        error,
        superlink_terminal::TerminalError::HostKeyChanged { .. }
    ));
}
