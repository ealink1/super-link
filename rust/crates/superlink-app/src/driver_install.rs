//! User-initiated driver installation into the writable workspace, never the app bundle.
use crate::{QueryCancel, QueryService, updates::Release};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::Path,
    sync::Mutex,
};
static INSTALL: Mutex<()> = Mutex::new(());
impl QueryService {
    pub fn install_release_driver(
        &self,
        release: &Release,
        kind: &str,
        cancel: &QueryCancel,
    ) -> Result<(), String> {
        let platform = superlink_update::Platform::current().ok_or("当前平台不受支持")?;
        let artifact = release
            .manifest
            .artifact(
                superlink_update::ArtifactKind::Driver,
                kind,
                platform.os,
                platform.arch,
            )
            .map_err(|e| e.to_string())?;
        let download = self.drivers_dir().join("downloads");
        let path = crate::updates::download_artifact(artifact, &download, cancel)?;
        let result = self.install_local_driver(
            &path,
            kind,
            &artifact.sha256,
            Some(&artifact.revision),
            cancel,
        );
        let _ = std::fs::remove_file(path);
        result
    }
    pub fn install_local_driver(
        &self,
        source: &Path,
        kind: &str,
        expected: &str,
        revision: Option<&str>,
        cancel: &QueryCancel,
    ) -> Result<(), String> {
        superlink_domain::resolve(kind).map_err(|_| "数据源类型无效")?;
        if expected.len() != 64 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("请填写发布文件的 64 位 SHA256 校验和".into());
        }
        let _gate = INSTALL.lock().unwrap_or_else(|e| e.into_inner());
        let root = self.drivers_dir();
        std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
        let scratch = root.join(format!(".install-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&scratch).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&scratch, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
        }
        let result = (|| {
            let name = format!(
                "{kind}-driver-agent{}",
                if cfg!(windows) { ".exe" } else { "" }
            );
            let staged = scratch.join(&name);
            copy_verified(source, &staged, expected, cancel)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o700))
                    .map_err(|e| e.to_string())?;
            }
            let platform = superlink_update::Platform::current().ok_or("当前平台不受支持")?;
            let mut record = json!({"type":kind,"sha256":expected.to_lowercase(),"revision":"pending","protocol":superlink_drivers::registry::PROTOCOL,"source":"user-install"});
            let bundle = |records: Value| json!({"schema":1,"os":platform.os.as_str(),"arch":platform.arch.as_str(),"drivers":records});
            std::fs::write(
                scratch.join("bundle.json"),
                bundle(json!([record])).to_string(),
            )
            .map_err(|e| e.to_string())?;
            superlink_drivers::registry::scan_one(&scratch, kind).map_err(|e| e.to_string())?;
            let _operation = crate::execution::Operation::new(cancel, 15);
            let mut session =
                superlink_drivers::DriverSession::open_with_cancel(&staged, kind, cancel)
                    .map_err(|e| e.to_string())?;
            session.set_cancellation(cancel);
            if revision.is_some_and(|expected| expected != session.revision()) {
                return Err("驱动版本与签名清单不一致".into());
            }
            record["revision"] = json!(session.revision());
            session.close().map_err(|e| e.to_string())?;
            drop(session);
            if cancel.is_cancelled() {
                return Err("驱动安装已停止".into());
            }
            let marker = root.join("bundle.json");
            let mut records = if marker.exists() {
                if std::fs::metadata(&marker).map_err(|e| e.to_string())?.len() > 1 << 20 {
                    return Err("驱动清单超过大小限制".into());
                }
                serde_json::from_slice::<Value>(&std::fs::read(&marker).map_err(|e| e.to_string())?)
                    .map_err(|_| "驱动清单无效")?["drivers"]
                    .as_array()
                    .cloned()
                    .ok_or("驱动清单无效")?
            } else {
                Vec::new()
            };
            records.retain(|r| r["type"] != kind);
            records.push(record);
            let staged_marker = scratch.join("bundle-next.json");
            let mut file = std::fs::File::create(&staged_marker).map_err(|e| e.to_string())?;
            file.write_all(bundle(json!(records)).to_string().as_bytes())
                .map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
            drop(file);
            let destination = root.join(&name);
            let backup = scratch.join("previous-driver");
            let previous = destination.exists();
            if previous {
                std::fs::rename(&destination, &backup).map_err(|e| e.to_string())?;
            }
            if let Err(error) = std::fs::rename(&staged, &destination)
                .and_then(|_| std::fs::rename(&staged_marker, &marker))
            {
                let _ = std::fs::remove_file(&destination);
                if previous {
                    let _ = std::fs::rename(&backup, &destination);
                }
                return Err(error.to_string());
            }
            self.refresh_drivers()
        })();
        let _ = std::fs::remove_dir_all(scratch);
        result
    }
}
fn copy_verified(
    source: &Path,
    target: &Path,
    expected: &str,
    cancel: &QueryCancel,
) -> Result<(), String> {
    let mut input = std::fs::File::open(source).map_err(|e| e.to_string())?;
    let metadata = input.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() > 1 << 30 {
        return Err("驱动文件必须是 1 GiB 以内的普通文件".into());
    }
    let mut output = std::fs::File::create(target).map_err(|e| e.to_string())?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 64 << 10];
    let mut total = 0;
    loop {
        if cancel.is_cancelled() {
            return Err("驱动安装已停止".into());
        }
        let n = input.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        total += n;
        if total > 1 << 30 {
            return Err("驱动文件超过 1 GiB".into());
        }
        digest.update(&buffer[..n]);
        output.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
    }
    output.sync_all().map_err(|e| e.to_string())?;
    if hex::encode(digest.finalize()) != expected.to_lowercase() {
        return Err("驱动 SHA256 不匹配，未安装".into());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn installs_and_executes_the_verified_sqlite_agent_in_a_private_workspace() {
        let source =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../bin/drivers/sqlite-driver-agent");
        if !source.is_file() {
            return;
        }
        let root =
            std::env::temp_dir().join(format!("sl-driver-roundtrip-{}", uuid::Uuid::new_v4()));
        let service = QueryService::new(&root);
        let digest = hex::encode(Sha256::digest(std::fs::read(&source).unwrap()));
        service
            .install_local_driver(&source, "sqlite", &digest, None, &QueryCancel::new())
            .unwrap();
        let mut driver =
            superlink_drivers::DriverSession::open(service.agent_path("sqlite").unwrap(), "sqlite")
                .unwrap();
        driver
            .connect(&superlink_domain::ConnectionConfig {
                kind: "sqlite".into(),
                database: ":memory:".into(),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(
            driver.query("SELECT 7", &[], 1).unwrap().rows[0][0],
            json!(7)
        );
        drop(driver);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn rejected_binary_preserves_the_existing_inventory() {
        let root = std::env::temp_dir().join(format!("sl-driver-install-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("bundle.json"), "original inventory").unwrap();
        std::fs::write(root.join("source"), "invalid binary").unwrap();
        let service = QueryService::new(&root);
        assert!(
            service
                .install_local_driver(
                    &root.join("source"),
                    "sqlite",
                    &"0".repeat(64),
                    None,
                    &QueryCancel::new()
                )
                .is_err()
        );
        assert_eq!(
            std::fs::read_to_string(root.join("bundle.json")).unwrap(),
            "original inventory"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
