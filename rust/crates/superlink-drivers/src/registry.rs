use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};

/// The only protocol this build understands.
pub const PROTOCOL: &str = "json-lines-v3-rust";
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
    inspect(directory, None, true)
}

/// Cheap startup inventory. A record is executable only after scan_one succeeds.
pub fn inventory(directory: &Path) -> Result<Vec<InstalledDriver>, RegistryError> {
    inspect(directory, None, false)
}
pub fn scan_one(directory: &Path, kind: &str) -> Result<Vec<InstalledDriver>, RegistryError> {
    inspect(directory, Some(kind), true)
}
fn inspect(
    directory: &Path,
    only: Option<&str>,
    verify: bool,
) -> Result<Vec<InstalledDriver>, RegistryError> {
    let bundle_path = directory.join("bundle.json");
    if fs::metadata(&bundle_path).is_ok_and(|m| m.len() > 1 << 20) {
        return Err(RegistryError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "driver inventory exceeds 1 MiB",
        )));
    }
    let raw = match fs::read(&bundle_path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(err.into()),
    };
    let bundle: Bundle = serde_json::from_slice(&raw)?;
    if bundle.schema != BUNDLE_SCHEMA {
        return Err(RegistryError::BundleSchema(bundle.schema));
    }
    let os = if std::env::consts::OS == "macos" {
        "darwin"
    } else {
        std::env::consts::OS
    };
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "amd64",
        other => other,
    };
    if ![os, std::env::consts::OS].contains(&bundle.os.as_str())
        || ![arch, std::env::consts::ARCH].contains(&bundle.arch.as_str())
    {
        return Err(RegistryError::WrongPlatform);
    }

    if bundle.drivers.len() > 200 {
        return Err(RegistryError::BundleSchema(bundle.schema));
    }
    let mut drivers = Vec::with_capacity(bundle.drivers.len());
    for record in &bundle.drivers {
        if record.kind.is_empty()
            || !record
                .kind
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        {
            return Err(RegistryError::Protocol {
                kind: "invalid driver id".into(),
                actual: record.protocol.clone(),
            });
        }
        if only.is_some_and(|kind| kind != record.kind) {
            continue;
        }
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
        if record.sha256.len() != 64 || !record.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(RegistryError::ChecksumMismatch {
                kind: record.kind.clone(),
            });
        }
        if !verify {
            drivers.push(driver);
            continue;
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
    // Only executable headers are needed; large driver SDKs stay off the heap.
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(1 << 20)
        .read_to_end(&mut bytes)?;
    let expected = match std::env::consts::ARCH {
        "aarch64" => 0x0100000c,
        "x86_64" => 0x01000007,
        "x86" => 7,
        _ => return Ok(true),
    };
    let u32_at = |offset: usize, big: bool| -> Option<u32> {
        let raw: [u8; 4] = bytes.get(offset..offset + 4)?.try_into().ok()?;
        Some(if big {
            u32::from_be_bytes(raw)
        } else {
            u32::from_le_bytes(raw)
        })
    };
    if bytes.starts_with(b"\xcf\xfa\xed\xfe") || bytes.starts_with(b"\xce\xfa\xed\xfe") {
        return Ok(u32_at(4, false) == Some(expected));
    }
    if bytes.starts_with(b"\xfe\xed\xfa\xcf") || bytes.starts_with(b"\xfe\xed\xfa\xce") {
        return Ok(u32_at(4, true) == Some(expected));
    }
    if bytes.starts_with(b"\xca\xfe\xba\xbe") || bytes.starts_with(b"\xca\xfe\xba\xbf") {
        let stride = if bytes[3] == 0xbf { 32 } else { 20 };
        let count = u32_at(4, true).unwrap_or(0).min(64) as usize;
        return Ok((0..count).any(|i| u32_at(8 + i * stride, true) == Some(expected)));
    }
    if bytes.starts_with(b"\x7fELF") {
        let Some(machine) = bytes.get(18..20) else {
            return Ok(false);
        };
        let value = if bytes.get(5) == Some(&2) {
            u16::from_be_bytes([machine[0], machine[1]])
        } else {
            u16::from_le_bytes([machine[0], machine[1]])
        };
        return Ok(value
            == match expected {
                0x0100000c => 183,
                0x01000007 => 62,
                _ => 3,
            });
    }
    if bytes.starts_with(b"MZ") {
        let Some(offset) = u32_at(0x3c, false).map(|v| v as usize) else {
            return Ok(false);
        };
        if bytes.get(offset..offset + 4) != Some(b"PE\0\0") {
            return Ok(false);
        }
        let Some(machine) = bytes.get(offset + 4..offset + 6) else {
            return Ok(false);
        };
        return Ok(u16::from_le_bytes([machine[0], machine[1]])
            == match expected {
                0x0100000c => 0xaa64,
                0x01000007 => 0x8664,
                _ => 0x14c,
            });
    }
    Ok(false)
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
            r#"{{"schema":1,"os":"{}","arch":"{}","drivers":[{{"type":"sqlite","sha256":"","revision":"r1","protocol":"json-lines-v3-rust"}}]}}"#,
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
            r#"{{"schema":1,"os":"{}","arch":"{}","drivers":[{{"type":"sqlite","sha256":"{}","revision":"r1","protocol":"json-lines-v3-rust"}}]}}"#,
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

#[cfg(test)]
mod header_tests {
    use super::*;
    #[test]
    fn reads_only_headers_and_matches_the_current_executable() {
        assert!(matches_architecture(&std::env::current_exe().unwrap()).unwrap());
        let path = std::env::temp_dir().join(format!("sl-invalid-header-{}", std::process::id()));
        fs::write(&path, b"not an executable").unwrap();
        assert!(!matches_architecture(&path).unwrap());
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn inventory_does_not_authorize_an_unverified_driver() {
        let directory = std::env::temp_dir().join(format!("sl-inventory-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let executable = directory.join(agent_file_name("sqlite"));
        fs::write(&executable, b"corrupted").unwrap();
        let os = if cfg!(target_os = "macos") {
            "darwin"
        } else {
            std::env::consts::OS
        };
        let arch = if cfg!(target_arch = "aarch64") {
            "arm64"
        } else {
            "amd64"
        };
        fs::write(directory.join("bundle.json"),serde_json::json!({"schema":1,"os":os,"arch":arch,"drivers":[{"type":"sqlite","protocol":PROTOCOL,"sha256":"0".repeat(64)}]}).to_string()).unwrap();
        assert!(!inventory(&directory).unwrap()[0].is_usable());
        assert!(scan_one(&directory, "sqlite").is_err());
        fs::remove_dir_all(directory).unwrap();
    }
}
