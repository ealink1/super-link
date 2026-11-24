//! Multiple keys on one server must not invalidate a pin during UI migration.
mod support;
use russh::keys::{HashAlg, PrivateKey};
use superlink_terminal::{CancelToken, HostKeyPolicy, SshClient, TerminalError};
use support::{FixtureServer, HOST_KEY, Options};

fn keys() -> (PrivateKey, PrivateKey) {
    (
        PrivateKey::from_openssh(include_str!("fixtures/ecdsa_host_key")).unwrap(),
        PrivateKey::from_openssh(HOST_KEY).unwrap(),
    )
}

#[test]
fn old_fyne_ecdsa_and_rust_ed25519_pins_both_verify_on_a_multi_key_server() {
    let (ecdsa, ed25519) = keys();
    let ecdsa_pin = ecdsa.public_key().fingerprint(HashAlg::Sha256).to_string();
    let ed25519_pin = ed25519
        .public_key()
        .fingerprint(HashAlg::Sha256)
        .to_string();
    let fixture = FixtureServer::start_with(Options {
        host_keys: vec![ed25519, ecdsa],
        ..Default::default()
    });
    for pin in [ecdsa_pin, ed25519_pin] {
        let outcome = SshClient::probe(
            &fixture.host(),
            HostKeyPolicy::Pinned(pin.clone()),
            &CancelToken::new(),
        )
        .unwrap();
        assert_eq!(outcome.fingerprint, pin);
    }
    assert_eq!(fixture.shared.lock().unwrap().auth_attempts, 2);
}

#[test]
fn a_real_key_change_never_sends_credentials_and_unknown_servers_do_not_retry_trust() {
    let (ecdsa, ed25519) = keys();
    let fixture = FixtureServer::start_with(Options {
        host_keys: vec![ecdsa, ed25519],
        ..Default::default()
    });
    let cancel = CancelToken::new();
    assert!(matches!(
        SshClient::probe(
            &fixture.host(),
            HostKeyPolicy::Pinned("SHA256:changed".into()),
            &cancel
        ),
        Err(TerminalError::HostKeyChanged { .. })
    ));
    assert!(matches!(
        SshClient::probe(&fixture.host(), HostKeyPolicy::Ask, &cancel),
        Err(TerminalError::HostKeyUnknown { .. })
    ));
    assert_eq!(fixture.shared.lock().unwrap().auth_attempts, 0);
}
