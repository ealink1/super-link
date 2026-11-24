//! Adds non-networked protocol profiles only to the known parity fixture.
use std::{path::PathBuf, sync::Arc};
use superlink_app::{Credentials, ProfileService};
use superlink_domain::{ConnectionConfig, Profile, resolve};
use superlink_infra::{LocalVault, Store};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("supply the parity fixture directory")?,
    );
    if !root.is_absolute() || !root.join("demo.db").is_file() {
        return Err("not the isolated parity fixture".into());
    }
    let store = Arc::new(Store::open(root.join("state.db"))?);
    let service = ProfileService::new(store, Arc::new(LocalVault::new(&root)));
    let fixture = service.get("parity-sqlite")?;
    if fixture.config.kind != "sqlite"
        || PathBuf::from(&fixture.config.database).parent() != Some(root.as_path())
    {
        return Err("fixture identity mismatch".into());
    }
    for kind in [
        "redis",
        "mongodb",
        "qdrant",
        "kafka",
        "nacos",
        "elasticsearch",
    ] {
        let id = format!("parity-{kind}");
        if service.get(&id).is_ok() {
            continue;
        }
        let descriptor = resolve(kind)?;
        service.save(
            Profile {
                id,
                name: format!("{} 协议验收", descriptor.name),
                group: "协议验收".into(),
                config: ConnectionConfig {
                    kind: kind.into(),
                    host: "127.0.0.1".into(),
                    port: descriptor.port.unwrap_or(0),
                    query_timeout: 5,
                    ..Default::default()
                },
                ..Default::default()
            },
            &Credentials::default(),
        )?;
    }
    println!("Protocol fixture profiles ready; no network requests performed.");
    Ok(())
}
