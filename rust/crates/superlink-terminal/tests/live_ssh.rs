//! Opt-in checks against a real SSH server.
//!
//! These tests never run on their own. Provide a reachable server and run:
//!
//! ```text
//! SUPERLINK_TEST_SSH_HOST=127.0.0.1 \
//! SUPERLINK_TEST_SSH_PORT=2222 \
//! SUPERLINK_TEST_SSH_USER=tester \
//! SUPERLINK_TEST_SSH_PASSWORD=secret \
//! cargo test -p superlink-terminal --test live_ssh -- --ignored
//! ```

use superlink_domain::ShellHost;
use superlink_terminal::{
    CancelToken, HostKeyPolicy, Session, SshClient, TerminalError, TerminalGeometry,
};

fn fixture_host() -> Option<ShellHost> {
    let host = std::env::var("SUPERLINK_TEST_SSH_HOST").ok()?;
    let user = std::env::var("SUPERLINK_TEST_SSH_USER").ok()?;
    Some(ShellHost {
        name: "live".into(),
        host,
        user,
        port: std::env::var("SUPERLINK_TEST_SSH_PORT")
            .ok()
            .and_then(|port| port.parse().ok())
            .unwrap_or(22),
        password: std::env::var("SUPERLINK_TEST_SSH_PASSWORD").unwrap_or_default(),
        key_path: std::env::var("SUPERLINK_TEST_SSH_KEY").unwrap_or_default(),
        passphrase: std::env::var("SUPERLINK_TEST_SSH_PASSPHRASE").unwrap_or_default(),
        fingerprint: std::env::var("SUPERLINK_TEST_SSH_FINGERPRINT").unwrap_or_default(),
        ..ShellHost::default()
    })
}

#[test]
#[ignore = "requires SUPERLINK_TEST_SSH_* and a reachable server"]
fn probe_then_open_and_close_a_terminal() {
    let Some(host) = fixture_host() else {
        return;
    };
    let cancel = CancelToken::new();
    let policy = HostKeyPolicy::Pinned(host.fingerprint.clone());
    let outcome =
        match SshClient::connect(&host, policy, TerminalGeometry::new(100, 30), &cancel, None) {
            Ok(outcome) => outcome,
            Err(TerminalError::HostKeyUnknown { fingerprint }) => {
                let mut client = SshClient::connect(
                    &host,
                    HostKeyPolicy::TrustOnce(fingerprint),
                    TerminalGeometry::new(100, 30),
                    &cancel,
                    None,
                )
                .expect("explicitly trusted host must connect");
                assert!(client.client.fingerprint().starts_with("SHA256:"));
                let mut session = std::mem::replace(
                    &mut client.session,
                    // The first session is replaced so the original is closed by Drop.
                    client
                        .client
                        .spawn_shell(80, 24, &cancel)
                        .expect("second shell must open"),
                );
                session.close().expect("close must not block");
                let mut buffer = [0u8; 8];
                assert_eq!(session.read(&mut buffer).expect("EOF expected"), 0);
                client.client.disconnect().expect("disconnect must succeed");
                return;
            }
            Err(error) => panic!("unexpected connection error: {error}"),
        };
    let mut session = outcome.session;
    session.close().expect("close must not block");
    let mut buffer = [0u8; 8];
    assert_eq!(session.read(&mut buffer).expect("EOF expected"), 0);
    outcome
        .client
        .disconnect()
        .expect("disconnect must succeed");
}
