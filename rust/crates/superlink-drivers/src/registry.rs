use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use object::{Architecture, Object};
use serde::Deserialize;
use sha2::{Digest, Sha256};

/// The only protocol this build understands.
pub const PROTOCOL: &str = "json-lines-v2";
const BUNDLE_SCHEMA: i64 = 1;

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("bundled drivers target another platform")]
    WrongPlatform,
    #[error("driver bundle schema {0} is not supported")]
    BundleSchema(i64),
    #[error("driver {kind} failed SHA256 verification")]
    ChecksumMismatch { kind: String },
    #[error("driver {kind} targets another CPU architecture")]
    WrongArchitecture { kind: String },
    #[error("driver {kind} speaks {actual}, expected {PROTOCOL}")]
    Protocol { kind: String, actual: String },
}

#[derive(Debug, Clone, Deserialize)]
pub struct BundleRecord {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub revision: String,
    #[serde(default)]
    pub protocol: String,
    #[serde(default)]
    pub source: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Bundle {
    schema: i64,
    os: String,
    arch: String,
    #[serde(default)]
    drivers: Vec<BundleRecord>,
}

/// One binary found on disk together with what could be verified about it.
#[derive(Debug, Clone)]
pub struct InstalledDriver {
    pub kind: String,
    pub path: PathBuf,
    pub revision: String,
    pub protocol: String,
    pub verified: bool,
    pub problem: Option<String>,
}

impl InstalledDriver {
    pub fn is_usable(&self) -> bool {
        self.verified && self.protocol == PROTOCOL
    }
}

/// Reads bundle.json and verifies every record. A missing bundle is not an
/// error: it means no driver was installed for this application build.
pub fn scan(directory: impl AsRef<Path>) -> Result<Vec<InstalledDriver>, RegistryError> {
    let directory = directory.as_ref();
    let raw = match fs::read(directory.join("bundle.json")) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(err.into()),
    };
    let bundle: Bundle = serde_json::from_slice(&raw)?;
    if bundle.schema != BUNDLE_SCHEMA {
        return Err(RegistryError::BundleSchema(bundle.schema));
    }
    if bundle.os != std::env::consts::OS || bundle.arch != std::env::consts::ARCH {
        return Err(RegistryError::WrongPlatform);
    }

    let mut drivers = Vec::with_capacity(bundle.drivers.len());
    for record in &bundle.drivers {
        let path = directory.join(agent_file_name(&record.kind));
        let mut driver = InstalledDriver {
            kind: record.kind.clone(),
            path,
            revision: record.revision.clone(),
            protocol: record.protocol.clone(),
            verified: false,
            problem: None,
        };
        if !driver.path.is_file() {
            driver.problem = Some("缺少驱动可执行文件".into());
            drivers.push(driver);
            continue;
        }
        if record.protocol != PROTOCOL {
            return Err(RegistryError::Protocol {
                kind: record.kind.clone(),
                actual: record.protocol.clone(),
            });
        }
        if !record.sha256.is_empty() {
            let actual = sha256_file(&driver.path)?;
            if !actual.eq_ignore_ascii_case(&record.sha256) {
                return Err(RegistryError::ChecksumMismatch {
                    kind: record.kind.clone(),
                });
            }
        }
        if !matches_architecture(&driver.path)? {
            return Err(RegistryError::WrongArchitecture {
                kind: record.kind.clone(),
            });
        }
        driver.verified = true;
        drivers.push(driver);
    }
    Ok(drivers)
}

fn agent_file_name(kind: &str) -> String {
    let suffix = if cfg!(target_os = "windows") {
        ".exe"
    } else {
        ""
    };
    format!("{kind}-driver-agent{suffix}")
}

fn sha256_file(path: &Path) -> Result<String, RegistryError> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Rejects a binary built for another CPU before it is ever executed.
fn matches_architecture(path: &Path) -> Result<bool, RegistryError> {
    let bytes = fs::read(path)?;
    let file = match object::File::parse(&*bytes) {
        Ok(file) => file,
        Err(_) => return Ok(false),
    };
    let expected = match std::env::consts::ARCH {
        "aarch64" => Architecture::Aarch64,
        "x86_64" => Architecture::X86_64,
        "x86" => Architecture::I386,
        _ => return Ok(true),
    };
    Ok(file.architecture() == expected)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "superlink-drivers-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::SeqCst)
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn missing_bundle_means_no_drivers() {
        let dir = TestDir::new();
        assert!(scan(&dir.0).unwrap().is_empty());
    }

    #[test]
    fn rejects_another_platform() {
        let dir = TestDir::new();
        fs::write(
            dir.0.join("bundle.json"),
            r#"{"schema":1,"os":"plan9","arch":"mips","drivers":[]}"#,
        )
        .unwrap();
        assert!(matches!(scan(&dir.0), Err(RegistryError::WrongPlatform)));
    }

    #[test]
    fn rejects_a_bundle_from_the_future() {
        let dir = TestDir::new();
        fs::write(
            dir.0.join("bundle.json"),
            r#"{"schema":9,"os":"darwin","arch":"arm64","drivers":[]}"#,
        )
        .unwrap();
        assert!(matches!(scan(&dir.0), Err(RegistryError::BundleSchema(9))));
    }

    #[test]
    fn reports_a_missing_binary_without_panicking() {
        let dir = TestDir::new();
        let bundle = format!(
            r#"{{"schema":1,"os":"{}","arch":"{}","drivers":[{{"type":"sqlite","sha256":"","revision":"r1","protocol":"json-lines-v2"}}]}}"#,
            std::env::consts::OS,
            std::env::consts::ARCH
        );
        fs::write(dir.0.join("bundle.json"), bundle).unwrap();
        let drivers = scan(&dir.0).unwrap();
        assert_eq!(drivers.len(), 1);
        assert!(!drivers[0].verified);
        assert!(drivers[0].problem.is_some());
    }

    #[test]
    fn detects_a_checksum_mismatch() {
        let dir = TestDir::new();
        fs::write(dir.0.join(agent_file_name("sqlite")), b"not the real agent").unwrap();
        let bundle = format!(
            r#"{{"schema":1,"os":"{}","arch":"{}","drivers":[{{"type":"sqlite","sha256":"{}","revision":"r1","protocol":"json-lines-v2"}}]}}"#,
            std::env::consts::OS,
            std::env::consts::ARCH,
            "0".repeat(64)
        );
        fs::write(dir.0.join("bundle.json"), bundle).unwrap();
        assert!(matches!(
            scan(&dir.0),
            Err(RegistryError::ChecksumMismatch { .. })
        ));
    }
}
