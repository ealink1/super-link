//! Transport behaviour against the in-process SSH fixture.

mod support;

use superlink_terminal::{
    CancelToken, ConnectEvent, ConnectStage, ConnectState, HostKeyPolicy, LargeFile, ProcessMetric,
    Session as _, SshClient, TerminalError, TerminalGeometry,
};
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

fn success_events() -> Vec<ConnectEvent> {
    let mut events = Vec::new();
    for stage in [
        ConnectStage::Tcp,
        ConnectStage::Auth,
        ConnectStage::Channel,
        ConnectStage::Ready,
    ] {
        events.push(ConnectEvent {
            stage,
            state: ConnectState::Started,
        });
        events.push(ConnectEvent {
            stage,
            state: ConnectState::Completed,
        });
    }
    events
}

#[test]
fn unknown_and_changed_host_keys_are_refused() {
    let fixture = FixtureServer::start();
    let host = fixture.host();
    let cancel = CancelToken::new();
    let pin = FixtureServer::pin();

    match SshClient::connect(
        &host,
        HostKeyPolicy::Pinned(String::new()),
        TerminalGeometry::default(),
        &cancel,
        None,
    ) {
        Err(TerminalError::HostKeyUnknown { fingerprint }) => assert_eq!(fingerprint, pin),
        Err(error) => panic!("unexpected error: {error}"),
        Ok(_) => panic!("an unknown host key must not be trusted"),
    }

    match SshClient::connect(
        &host,
        HostKeyPolicy::Pinned("SHA256:changed".to_owned()),
        TerminalGeometry::default(),
        &cancel,
        None,
    ) {
        Err(TerminalError::HostKeyChanged { expected, observed }) => {
            assert_eq!(expected, "SHA256:changed");
            assert_eq!(observed, pin);
        }
        Err(error) => panic!("unexpected error: {error}"),
        Ok(_) => panic!("a changed host key must abort the connection"),
    }
}

#[test]
fn rejects_a_wrong_password_after_the_host_key_is_trusted() {
    let fixture = FixtureServer::start();
    let mut host = fixture.host();
    host.password = "wrong".to_owned();
    let result = SshClient::connect(
        &host,
        HostKeyPolicy::TrustOnce(FixtureServer::pin()),
        TerminalGeometry::default(),
        &CancelToken::new(),
        None,
    );
    match result {
        Err(TerminalError::Authentication) => {}
        Err(error) => panic!("unexpected error: {error}"),
        Ok(_) => panic!("a wrong password must be rejected"),
    }
}

#[test]
fn a_rejected_pty_fails_at_the_channel_stage() {
    let fixture = FixtureServer::start_with_rejecting_pty();
    let mut events = Vec::new();
    let mut observer = |event| events.push(event);
    let result = SshClient::connect(
        &fixture.host(),
        HostKeyPolicy::TrustOnce(FixtureServer::pin()),
        TerminalGeometry::default(),
        &CancelToken::new(),
        Some(&mut observer),
    );
    match result {
        Err(TerminalError::Channel(message)) => {
            assert_eq!(message, "远程服务器拒绝了终端请求");
        }
        Err(error) => panic!("unexpected error: {error}"),
        Ok(_) => panic!("a rejected PTY must not produce a session"),
    }
    let mut expected = success_events();
    expected.truncate(5);
    expected.push(ConnectEvent {
        stage: ConnectStage::Channel,
        state: ConnectState::Failed,
    });
    assert_eq!(events, expected);
}

#[test]
fn streams_a_terminal_echoes_resizes_and_reports_progress() {
    let fixture = FixtureServer::start();
    let host = fixture.host();
    let cancel = CancelToken::new();
    let pin = FixtureServer::pin();
    let mut events = Vec::new();
    let mut observer = |event| events.push(event);
    let outcome = SshClient::connect(
        &host,
        HostKeyPolicy::TrustOnce(pin.clone()),
        TerminalGeometry::new(91, 31),
        &cancel,
        Some(&mut observer),
    )
    .expect("fixture accepts the connection");
    assert_eq!(events, success_events());
    assert_eq!(outcome.fingerprint, pin);
    assert!(!outcome.remember);
    assert_eq!(
        fixture.shared.lock().unwrap().terminals,
        vec!["xterm-256color".to_owned()]
    );

    let mut session = outcome.session;
    session.write("你好\n".as_bytes()).expect("write terminal");
    read_expected(&mut session, "你好\n".as_bytes());

    session.resize(120, 40).expect("resize terminal");
    fixture.wait_for_resize((120, 40));

    let mut second_session = outcome
        .client
        .spawn_shell(100, 30, &cancel)
        .expect("second shell opens");
    second_session.write(b"again").expect("write second shell");
    read_expected(&mut second_session, b"again");
    second_session.close().expect("close second shell");
    let mut buffer = [0u8; 4];
    assert_eq!(
        second_session.read(&mut buffer).expect("EOF expected"),
        0,
        "closed sessions must report EOF"
    );

    session.close().expect("close terminal");
    assert_eq!(
        session.read(&mut buffer).expect("EOF expected"),
        0,
        "closed sessions must report EOF"
    );
    outcome.client.disconnect().expect("disconnect");
}

#[test]
fn runs_probes_and_scans_over_one_shot_channels() {
    let fixture = FixtureServer::start();
    let host = fixture.host();
    let cancel = CancelToken::new();
    let outcome = SshClient::connect(
        &host,
        HostKeyPolicy::TrustOnce(FixtureServer::pin()),
        TerminalGeometry::default(),
        &cancel,
        None,
    )
    .expect("fixture accepts the connection");

    let metrics = outcome.client.monitor(&cancel).expect("monitor works");
    assert_eq!(metrics.system, "Linux");
    assert_eq!(metrics.memory_total, 1_024_000);
    assert_eq!(metrics.memory_used, 614_400);

    let files = outcome
        .client
        .large_files(&["/tmp".to_owned()], 100 << 20, 20, &cancel)
        .expect("scan works");
    assert_eq!(
        files,
        vec![LargeFile {
            path: "/tmp/big".to_owned(),
            size: 10
        }]
    );

    let process = ProcessMetric {
        pid: 42,
        start: 7,
        ..ProcessMetric::default()
    };
    outcome
        .client
        .stop_process(&process, &cancel)
        .expect("stop works");
    outcome.client.disconnect().expect("disconnect");
}

#[test]
fn remembering_trust_marks_the_fingerprint_for_storage() {
    let fixture = FixtureServer::start();
    let outcome = SshClient::connect(
        &fixture.host(),
        HostKeyPolicy::TrustAndRemember(FixtureServer::pin()),
        TerminalGeometry::default(),
        &CancelToken::new(),
        None,
    )
    .expect("fixture accepts the connection");
    assert!(outcome.remember);
    outcome.client.disconnect().expect("disconnect");
}
