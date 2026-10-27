//! SFTP transfers against the in-process fixture.

mod support;

use std::os::unix::fs::PermissionsExt;

use superlink_terminal::{
    CancelToken, HostKeyPolicy, SshClient, TerminalError, TerminalGeometry, TransferOptions,
};
use support::FixtureServer;

fn connect(fixture: &FixtureServer) -> superlink_terminal::ConnectOutcome {
    SshClient::connect(
        &fixture.host(),
        HostKeyPolicy::TrustOnce(FixtureServer::pin()),
        TerminalGeometry::default(),
        &CancelToken::new(),
        None,
    )
    .expect("fixture accepts the connection")
}

#[test]
fn uploads_lists_downloads_and_manages_files() {
    let fixture = FixtureServer::start_with_sftp();
    let outcome = connect(&fixture);
    let sftp = outcome.client.open_sftp().expect("sftp opens");
    let cancel = CancelToken::new();
    let options = TransferOptions::default();

    let source = fixture.root().join("source.bin");
    std::fs::write(&source, b"payload").expect("local source");
    assert_eq!(
        sftp.upload(&source, "/uploaded.bin", &options)
            .expect("upload"),
        7
    );
    let stored = fixture.root().join("uploaded.bin");
    assert_eq!(std::fs::read(&stored).expect("stored file"), b"payload");

    match sftp.upload(&source, "/uploaded.bin", &options) {
        Err(TerminalError::RemoteExists) => {}
        Err(error) => panic!("unexpected error: {error}"),
        Ok(_) => panic!("remote files must never be overwritten"),
    }

    let files = sftp.list("/", &cancel).expect("list");
    let entry = files
        .iter()
        .find(|file| file.name == "uploaded.bin")
        .expect("uploaded file is listed");
    assert_eq!(entry.size, 7);
    assert!(!entry.directory);
    assert!(entry.mode.starts_with('-'));

    let stat = sftp.stat("/uploaded.bin", &cancel).expect("stat");
    assert_eq!(stat.size, 7);
    assert!(!stat.directory);

    let destination = fixture.root().join("downloaded.bin");
    assert_eq!(
        sftp.download("/uploaded.bin", &destination, &options)
            .expect("download"),
        7
    );
    assert_eq!(
        std::fs::read(&destination).expect("downloaded file"),
        b"payload"
    );
    match sftp.download("/uploaded.bin", &destination, &options) {
        Err(TerminalError::LocalExists) => {}
        Err(error) => panic!("unexpected error: {error}"),
        Ok(_) => panic!("local files must never be overwritten"),
    }

    sftp.set_permissions("/uploaded.bin", 0o640, &cancel)
        .expect("chmod");
    let mode = std::fs::metadata(&stored)
        .expect("stored metadata")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o640);
    assert!(
        sftp.set_permissions("/uploaded.bin", 0o1000, &cancel)
            .is_err()
    );

    std::fs::write(fixture.root().join("other.bin"), b"other").expect("other file");
    assert!(
        sftp.rename_file("/uploaded.bin", "/other.bin", &cancel)
            .is_err(),
        "rename must refuse an existing target"
    );
    sftp.rename_file("/uploaded.bin", "/moved.bin", &cancel)
        .expect("rename");
    assert!(!stored.exists());
    assert!(fixture.root().join("moved.bin").exists());

    sftp.remove("/moved.bin", &cancel).expect("remove");
    assert!(!fixture.root().join("moved.bin").exists());

    let absolute = sftp.absolute_path("/", &cancel).expect("realpath");
    assert!(absolute.starts_with('/'));
    assert!(matches!(
        sftp.remove("/missing.bin", &cancel),
        Err(TerminalError::Sftp(_))
    ));

    sftp.close().expect("close sftp");
    outcome.client.disconnect().expect("disconnect");
}

#[test]
fn transfer_limits_and_cancellation_leave_no_partial_files() {
    let fixture = FixtureServer::start_with_sftp();
    let outcome = connect(&fixture);
    let sftp = outcome.client.open_sftp().expect("sftp opens");
    let cancel = CancelToken::new();

    let source = fixture.root().join("large.bin");
    std::fs::write(&source, vec![9u8; 4096]).expect("local source");
    let limited = TransferOptions {
        max_bytes: 16,
        ..TransferOptions::default()
    };
    match sftp.upload(&source, "/limited.bin", &limited) {
        Err(TerminalError::TransferTooLarge { limit: 16 }) => {}
        Err(error) => panic!("unexpected error: {error}"),
        Ok(_) => panic!("an oversized upload must be refused"),
    }
    assert!(!fixture.root().join("limited.bin").exists());
    assert!(
        sftp.list("/", &cancel)
            .expect("list")
            .iter()
            .all(|file| !file.name.starts_with(".superlink-upload-")),
        "an interrupted upload must not leave a staging file"
    );

    std::fs::write(fixture.root().join("remote.bin"), vec![1u8; 4096]).expect("remote file");
    let destination = fixture.root().join("blocked.bin");
    let cancelled = TransferOptions {
        cancel: CancelToken::new(),
        ..TransferOptions::default()
    };
    cancelled.cancel.cancel();
    match sftp.download("/remote.bin", &destination, &cancelled) {
        Err(TerminalError::Cancelled) => {}
        Err(error) => panic!("unexpected error: {error}"),
        Ok(_) => panic!("a cancelled download must not run"),
    }
    assert!(!destination.exists());

    sftp.close().expect("close sftp");
    outcome.client.disconnect().expect("disconnect");
}
