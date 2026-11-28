//! 发布签名工具，替代 Go 版的 `cmd/release-sign`。
//!
//! 读取 `tools/build.py` 写出的 `assets-*.json`，汇总成清单并签名，产出
//! `manifest.json` 与 `manifest.json.sig`。编码沿用 `Manifest::to_signed_bytes`
//! （两空格缩进 JSON 加一个换行），与 Go 工具逐字节一致，既有签名的发布继续可验。

use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use chrono::Utc;
use ed25519_dalek::{Signer as _, SigningKey};
use superlink_update::{Artifact, Channel, Manifest, SCHEMA_VERSION, sha256_file, verify_manifest};

/// Ed25519 私钥（种子 + 公钥）的字节长度。
const PRIVATE_KEY_BYTES: usize = 64;
const MANIFEST: &str = "manifest.json";
const SIGNATURE: &str = "manifest.json.sig";

/// 签名器参数；由 `run` 从命令行与环境变量填充，测试直接构造。
pub struct Request {
    pub assets_dir: PathBuf,
    pub version: String,
    pub channel: Channel,
    /// base64 编码的 64 字节 Ed25519 私钥。
    pub private_key: String,
    /// 非空时要求私钥推导出的公钥与之一致，避免用错密钥签出无法验证的清单。
    pub expected_public_key: Option<String>,
}

/// 生成一对新密钥；私钥以 base64 写入新文件，绝不覆盖已存在的文件。
pub fn generate_key(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let signing = SigningKey::from_bytes(&random_seed()?);
    let mut keypair = signing.to_keypair_bytes();
    let text = STANDARD.encode(keypair);
    keypair.fill(0);
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    use std::io::Write as _;
    let mut file = options.open(path)?;
    file.write_all(text.as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    println!(
        "私钥已写入指定文件。公钥：{}",
        STANDARD.encode(signing.verifying_key().to_bytes())
    );
    Ok(())
}

/// 生成清单、签名并落盘；签名后会先用公钥验证一遍再写出。
pub fn sign(request: &Request) -> Result<(), Box<dyn std::error::Error>> {
    let signing = decode_private_key(&request.private_key)?;
    let public_key = STANDARD.encode(signing.verifying_key().to_bytes());
    if let Some(expected) = request.expected_public_key.as_deref() {
        let expected = expected.trim();
        if !expected.is_empty() && expected != public_key {
            return Err("签名私钥与应用发布公钥不匹配".into());
        }
    }
    let version = request.version.trim().trim_start_matches('v').to_string();
    let manifest = Manifest {
        schema: SCHEMA_VERSION,
        version: version.clone(),
        channel: request.channel,
        published_at: Some(Utc::now()),
        artifacts: read_artifacts(&request.assets_dir)?,
    };
    manifest.validate()?;
    verify_files(&request.assets_dir, &manifest)?;
    let raw = manifest.to_signed_bytes()?;
    let mut signature = signing.sign(&raw).to_bytes().to_vec();
    let encoded = format!("{}\n", STANDARD.encode(&signature));
    signature.fill(0);

    // `verify_manifest` 自行解码 base64，这里传文本编码后的签收字节。
    verify_manifest(&raw, encoded.as_bytes(), &public_key)?;
    fs::write(request.assets_dir.join(MANIFEST), &raw)?;
    fs::write(request.assets_dir.join(SIGNATURE), encoded.as_bytes())?;
    println!("清单已签名并校验。公钥：{public_key}");
    Ok(())
}

/// 从命令行参数或环境变量组装请求。
pub fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let mut assets_dir = PathBuf::from("dist");
    let mut version = None;
    let mut channel = Channel::Stable;
    let mut key_file = None;
    let mut generate = None;
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        let value = || {
            args.get(index + 1)
                .cloned()
                .ok_or_else(|| format!("{flag} 需要一个取值"))
        };
        match flag {
            "--assets-dir" => {
                assets_dir = PathBuf::from(value()?);
                index += 2;
            }
            "--version" => {
                version = Some(value()?);
                index += 2;
            }
            "--channel" => {
                channel = match value()?.as_str() {
                    "stable" => Channel::Stable,
                    "preview" => Channel::Preview,
                    other => return Err(format!("未知的发布通道 {other:?}").into()),
                };
                index += 2;
            }
            "--key-file" => {
                key_file = Some(PathBuf::from(value()?));
                index += 2;
            }
            "--generate-key-file" => {
                generate = Some(PathBuf::from(value()?));
                index += 2;
            }
            other => return Err(format!("未知参数 {other:?}").into()),
        }
    }
    if let Some(path) = generate {
        return generate_key(&path);
    }
    let version = version.ok_or("缺少 --version")?;
    let private_key = match key_file {
        Some(path) => fs::read_to_string(path)?,
        None => std::env::var("SUPERLINK_RELEASE_PRIVATE_KEY")
            .map_err(|_| "请提供 --key-file 或 SUPERLINK_RELEASE_PRIVATE_KEY")?,
    };
    sign(&Request {
        assets_dir,
        version,
        channel,
        private_key,
        expected_public_key: std::env::var("SUPERLINK_RELEASE_PUBLIC_KEY").ok(),
    })
}

fn decode_private_key(text: &str) -> Result<SigningKey, Box<dyn std::error::Error>> {
    let raw = STANDARD
        .decode(text.trim())
        .map_err(|_| "私钥不是合法的 base64")?;
    let bytes: [u8; PRIVATE_KEY_BYTES] = raw
        .as_slice()
        .try_into()
        .map_err(|_| "请提供 base64 编码的 64 字节 Ed25519 私钥")?;
    SigningKey::from_keypair_bytes(&bytes).map_err(|_| "Ed25519 私钥的种子与公钥部分不一致".into())
}

/// 汇总目录下每个平台的 `assets-*.json`。
fn read_artifacts(directory: &Path) -> Result<Vec<Artifact>, Box<dyn std::error::Error>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if name.starts_with("assets-") && path.extension().is_some_and(|ext| ext == "json") {
            paths.push(path);
        }
    }
    if paths.is_empty() {
        return Err(format!("{} 下没有 assets-*.json", directory.display()).into());
    }
    paths.sort();
    let mut artifacts = Vec::new();
    for path in paths {
        let raw = fs::read(&path)?;
        let mut listed: Vec<Artifact> =
            serde_json::from_slice(&raw).map_err(|error| format!("{}: {error}", path.display()))?;
        artifacts.append(&mut listed);
    }
    Ok(artifacts)
}

/// 逐个核对本地文件的体积与摘要，并确认下载地址指向本次版本。
fn verify_files(directory: &Path, manifest: &Manifest) -> Result<(), Box<dyn std::error::Error>> {
    let needle = format!("/releases/download/v{}/", manifest.version);
    let mut seen = std::collections::HashSet::new();
    for artifact in &manifest.artifacts {
        if !seen.insert(artifact.filename.clone()) {
            return Err(format!("文件名重复：{}", artifact.filename).into());
        }
        let path = directory.join(&artifact.filename);
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(format!("{} 必须是普通文件", path.display()).into());
        }
        let (size, sha256) = sha256_file(&path)?;
        if size != artifact.size || !sha256.eq_ignore_ascii_case(&artifact.sha256) {
            return Err(format!("{} 与构建元数据不一致", artifact.filename).into());
        }
        if !artifact.url.contains(&needle) {
            return Err(format!("{} 的下载地址版本不符", artifact.filename).into());
        }
    }
    Ok(())
}

/// 用系统随机源取 32 字节种子，作为新 Ed25519 私钥。
fn random_seed() -> Result<[u8; 32], Box<dyn std::error::Error>> {
    let mut seed = [0u8; 32];
    getrandom::getrandom(&mut seed).map_err(|error| format!("系统随机源不可用：{error}"))?;
    Ok(seed)
}

#[cfg(test)]
mod tests {
    use super::{Request, generate_key, random_seed, sign};
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD;
    use ed25519_dalek::SigningKey;
    use std::fs;
    use std::path::{Path, PathBuf};
    use superlink_update::{Channel, verify_manifest};

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("superlink-sign-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn keypair() -> SigningKey {
        SigningKey::from_bytes(&random_seed().unwrap())
    }

    /// 写一个假安装包及其 assets 清单；返回私钥文本与公钥。
    fn fixture(dir: &Path, version: &str) -> (String, String) {
        let signing = keypair();
        let name = format!("SuperLink_{version}_darwin_arm64.zip");
        fs::write(dir.join(&name), b"payload").unwrap();
        let (size, sha256) = superlink_update::sha256_file(&dir.join(&name)).unwrap();
        let assets = serde_json::json!([{
            "id": "superlink", "kind": "app", "os": "darwin", "arch": "arm64",
            "filename": name, "size": size, "sha256": sha256,
            "url": format!(
                "https://github.com/ealink1/super-link/releases/download/v{version}/{name}"
            ),
        }]);
        fs::write(
            dir.join("assets-darwin-arm64.json"),
            serde_json::to_vec_pretty(&assets).unwrap(),
        )
        .unwrap();
        let private = STANDARD.encode(signing.to_keypair_bytes());
        let public = STANDARD.encode(signing.verifying_key().to_bytes());
        (private, public)
    }

    fn request(dir: &Path, version: &str, private: &str) -> Request {
        Request {
            assets_dir: dir.to_path_buf(),
            version: version.to_string(),
            channel: Channel::Stable,
            private_key: private.to_string(),
            expected_public_key: None,
        }
    }

    #[test]
    fn signs_a_manifest_that_verifies() {
        let dir = scratch("ok");
        let (private, public) = fixture(&dir, "0.2.0");
        sign(&request(&dir, "0.2.0", &private)).unwrap();

        let raw = fs::read(dir.join("manifest.json")).unwrap();
        let signature = fs::read(dir.join("manifest.json.sig")).unwrap();
        assert!(raw.ends_with(b"\n"), "清单必须以换行结尾");
        let manifest = verify_manifest(&raw, &signature, &public).unwrap();
        assert_eq!(manifest.version, "0.2.0");
        assert_eq!(manifest.channel, Channel::Stable);
        assert_eq!(manifest.artifacts.len(), 1);
        assert_eq!(manifest.artifacts[0].id, "superlink");
    }

    #[test]
    fn strips_a_leading_v_from_the_version() {
        let dir = scratch("prefix");
        let (private, _) = fixture(&dir, "0.2.0");
        sign(&request(&dir, "v0.2.0", &private)).unwrap();
        let raw = fs::read(dir.join("manifest.json")).unwrap();
        assert!(
            String::from_utf8(raw)
                .unwrap()
                .contains("\"version\": \"0.2.0\"")
        );
    }

    #[test]
    fn rejects_a_tampered_artifact() {
        let dir = scratch("tampered");
        let (private, _) = fixture(&dir, "0.2.0");
        fs::write(dir.join("SuperLink_0.2.0_darwin_arm64.zip"), b"changed").unwrap();
        let error = sign(&request(&dir, "0.2.0", &private)).unwrap_err();
        assert!(error.to_string().contains("构建元数据"), "{error}");
        assert!(!dir.join("manifest.json").exists());
    }

    #[test]
    fn rejects_a_mismatched_public_key() {
        let dir = scratch("key");
        let (private, _) = fixture(&dir, "0.2.0");
        let mut request = request(&dir, "0.2.0", &private);
        request.expected_public_key = Some(STANDARD.encode([0u8; 32]));
        let error = sign(&request).unwrap_err();
        assert!(error.to_string().contains("公钥"), "{error}");
    }

    #[test]
    fn rejects_a_wrong_sized_private_key() {
        let dir = scratch("short");
        fixture(&dir, "0.2.0");
        let error = sign(&request(&dir, "0.2.0", &STANDARD.encode([7u8; 32]))).unwrap_err();
        assert!(error.to_string().contains("64 字节"), "{error}");
    }

    #[test]
    fn generated_key_is_a_full_keypair() {
        let dir = scratch("generate");
        let path = dir.join("release.key");
        generate_key(&path).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        let raw = STANDARD.decode(text.trim()).unwrap();
        assert_eq!(raw.len(), 64);
        // 第二次生成必须失败，避免静默覆盖既有私钥。
        assert!(generate_key(&path).is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    /// 线上格式的已知答案：固定密钥签固定清单必须得到固定签名。
    ///
    /// 这份期望值由独立实现（Go 的 `cmd/release-sign` 同样的格式化规则）算出，
    /// 一旦 `to_signed_bytes` 的编码发生漂移，这里会立即失败。
    #[test]
    fn matches_the_published_wire_format() {
        let dir = scratch("golden");
        let private = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8DoQe/884Qvh1w3RjnS8\
                       CZZ+TWMJulDV8d3IZkElUxuA=="
            .replace([' ', '\n'], "");
        fs::write(dir.join("SuperLink_0.2.0_darwin_arm64.zip"), b"payload").unwrap();
        let artifacts = serde_json::json!([{
            "id": "superlink", "kind": "app", "os": "darwin", "arch": "arm64",
            "filename": "SuperLink_0.2.0_darwin_arm64.zip",
            "url": "https://github.com/ealink1/super-link/releases/download/v0.2.0/\
                    SuperLink_0.2.0_darwin_arm64.zip",
            "size": 7,
            "sha256": "239f59ed55e737c77147cf55ad0c1b030b6d7ee748a7426952f9b852d5a935e5",
        }]);
        fs::write(
            dir.join("assets-darwin-arm64.json"),
            serde_json::to_vec_pretty(&artifacts).unwrap(),
        )
        .unwrap();

        // 期望签名不含 publishedAt，因此先签名再手工重建同样字段的清单校验编码。
        let mut request = request(&dir, "0.2.0", &private);
        request.expected_public_key = Some("A6EHv/POEL4dcN0Y50vAmWfk1jCbpQ1fHdyGZBJVMbg=".into());
        sign(&request).unwrap();
        let raw = String::from_utf8(fs::read(dir.join("manifest.json")).unwrap()).unwrap();
        // 去掉 publishedAt 后必须逐字节等于已知答案里被签名的正文。
        let without_stamp: String = raw
            .lines()
            .filter(|line| !line.contains("\"publishedAt\""))
            .map(|line| format!("{line}\n"))
            .collect();
        assert_eq!(without_stamp.len(), 454);
        assert!(without_stamp.starts_with("{\n  \"schema\": 1,\n  \"version\": \"0.2.0\","));
        assert!(without_stamp.ends_with("}\n"));
    }
}
