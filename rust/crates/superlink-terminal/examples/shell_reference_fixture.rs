//! Disposable loopback server for native Fyne/Rust parity checks.
//! Never connects to user hosts. Create INFO.stop to stop it (15 minute cap).
#[path = "../tests/support/mod.rs"]
mod support;
use russh::keys::{HashAlg, PrivateKey};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use support::{FixtureServer, HOST_KEY, Options};

fn main() {
    let info = PathBuf::from(std::env::args_os().nth(1).expect("fixture info file"));
    let ecdsa = PrivateKey::from_openssh(include_str!("../tests/fixtures/ecdsa_host_key"))
        .expect("ECDSA fixture");
    let pin = ecdsa.public_key().fingerprint(HashAlg::Sha256).to_string();
    let ed25519 = PrivateKey::from_openssh(HOST_KEY).expect("Ed25519 fixture");
    let server = FixtureServer::start_with(Options {
        sftp: true,
        host_keys: vec![ed25519, ecdsa],
        ..Default::default()
    });
    std::fs::write(
        &info,
        format!(
            "{{\"port\":{},\"fingerprint\":\"{pin}\"}}",
            server.host().port
        ),
    )
    .expect("write public fixture endpoint");
    let started = Instant::now();
    while !info.with_extension("stop").exists() && started.elapsed() < Duration::from_secs(900) {
        std::thread::sleep(Duration::from_millis(250));
    }
}
