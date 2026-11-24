//! User-initiated signed release checks and bounded downloads.
use crate::QueryCancel;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};
use superlink_update::{APP_ARTIFACT_ID, ArtifactKind, Channel, Manifest, Platform};

const API: &str = "https://api.github.com/repos/ealink1/super-link/releases/latest";
const FALLBACK: &str = "https://github.com/ealink1/super-link/releases/latest/download/";
const KEY: &str = match option_env!("SUPERLINK_RELEASE_PUBLIC_KEY") {
    Some(key) => key,
    None => "",
};

#[derive(Clone)]
pub struct Release {
    pub manifest: Manifest,
}
#[derive(Deserialize)]
struct Latest {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}
#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

fn runtime() -> Result<tokio::runtime::Runtime, String> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "无法启动更新任务".into())
}
fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent("SuperLink")
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(600))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5
                || superlink_update::trusted_url(attempt.url().as_str()).is_err()
            {
                attempt.error("更新下载重定向不受信任")
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(|_| "无法启动更新网络请求".into())
}
async fn get(client: &reqwest::Client, url: &str, limit: usize) -> Result<Vec<u8>, String> {
    superlink_update::trusted_url(url).map_err(|e| e.to_string())?;
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|_| "更新网络请求失败")?;
    if !response.status().is_success() {
        return Err(format!(
            "更新服务器返回 HTTP {}",
            response.status().as_u16()
        ));
    }
    let mut raw = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| "读取更新信息失败")? {
        if raw.len() + chunk.len() > limit {
            return Err("更新信息超过大小限制".into());
        }
        raw.extend_from_slice(&chunk);
    }
    Ok(raw)
}
fn cancellable<T>(
    cancel: &QueryCancel,
    future: impl std::future::Future<Output = Result<T, String>>,
) -> Result<T, String> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    cancel.on_cancel(move || {
        let _ = sender.send(());
    });
    let result = runtime()?.block_on(async { tokio::select! { biased; _=receiver => Err("更新任务已取消".into()), result=future => result } });
    cancel.clear();
    result
}

pub fn check(cancel: &QueryCancel) -> Result<Option<Release>, String> {
    check_repo(API, FALLBACK, cancel)
}
pub fn check_drivers(cancel: &QueryCancel) -> Result<Option<Release>, String> {
    check_repo(
        "https://api.github.com/repos/ealink1/SuperLink-DriverAgents/releases/latest",
        "https://github.com/ealink1/SuperLink-DriverAgents/releases/latest/download/",
        cancel,
    )
}
fn check_repo(api: &str, fallback: &str, cancel: &QueryCancel) -> Result<Option<Release>, String> {
    if KEY.is_empty() {
        return Err("当前构建未配置发布签名公钥，无法验证更新；请使用正式发布包".into());
    }
    let client = client()?;
    cancellable(cancel, async {
        let response = client
            .get(api)
            .send()
            .await
            .map_err(|_| "无法连接发布服务器")?;
        let (manifest_url, signature_url, tag) = match response.status().as_u16() {
            404 => return Ok(None),
            403 | 429 => (
                format!("{fallback}manifest.json"),
                format!("{fallback}manifest.json.sig"),
                None,
            ),
            200 => {
                // Read with a limit even when Content-Length is absent.
                let mut response = response;
                let mut raw = Vec::new();
                while let Some(chunk) = response.chunk().await.map_err(|_| "读取发布信息失败")?
                {
                    if raw.len() + chunk.len() > 1 << 20 {
                        return Err("发布信息超过大小限制".into());
                    }
                    raw.extend_from_slice(&chunk);
                }
                let latest: Latest =
                    serde_json::from_slice(&raw).map_err(|_| "发布信息格式无效")?;
                if latest.draft || latest.prerelease {
                    return Err("没有可用的稳定版本".into());
                }
                let find = |name| {
                    latest
                        .assets
                        .iter()
                        .find(|a| a.name == name)
                        .map(|a| a.browser_download_url.clone())
                        .ok_or("发布缺少签名清单")
                };
                (
                    find("manifest.json")?,
                    find("manifest.json.sig")?,
                    Some(latest.tag_name),
                )
            }
            code => return Err(format!("发布服务器返回 HTTP {code}")),
        };
        let raw = get(&client, &manifest_url, 1 << 20).await?;
        let signature = get(&client, &signature_url, 1024).await?;
        let manifest =
            superlink_update::verify_manifest(&raw, &signature, KEY).map_err(|e| e.to_string())?;
        if manifest.channel != Channel::Stable
            || tag.is_some_and(|t| {
                t.trim_start_matches('v') != manifest.version.trim_start_matches('v')
            })
        {
            return Err("发布版本与签名清单不一致".into());
        }
        Ok(Some(Release { manifest }))
    })
}

pub fn is_newer(release: &Release) -> bool {
    superlink_update::is_newer(
        &release.manifest.version,
        option_env!("SUPERLINK_VERSION").unwrap_or(env!("CARGO_PKG_VERSION")),
    )
}

/// Runs only after the user chooses download/install. No application is replaced here.
pub fn download(
    release: &Release,
    directory: &Path,
    cancel: &QueryCancel,
) -> Result<PathBuf, String> {
    let platform = Platform::current().ok_or("当前平台没有更新包")?;
    let artifact = release
        .manifest
        .artifact(
            ArtifactKind::App,
            APP_ARTIFACT_ID,
            platform.os,
            platform.arch,
        )
        .map_err(|e| e.to_string())?;
    download_artifact(artifact, directory, cancel)
}
pub fn download_artifact(
    artifact: &superlink_update::Artifact,
    directory: &Path,
    cancel: &QueryCancel,
) -> Result<PathBuf, String> {
    artifact.validate().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let temporary = directory.join(format!(".download-{}", uuid::Uuid::new_v4()));
    let client = client()?;
    let result = cancellable(cancel, async {
        let mut response = client
            .get(&artifact.url)
            .send()
            .await
            .map_err(|_| "更新包下载失败")?;
        if !response.status().is_success() {
            return Err(format!(
                "下载服务器返回 HTTP {}",
                response.status().as_u16()
            ));
        }
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        let mut size = 0u64;
        let mut digest = Sha256::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| "更新包下载中断")? {
            size += chunk.len() as u64;
            if size > artifact.size {
                return Err("更新包大小超过签名清单".into());
            }
            digest.update(&chunk);
            file.write_all(&chunk).map_err(|e| e.to_string())?;
        }
        if size != artifact.size
            || hex::encode(digest.finalize()) != artifact.sha256.to_ascii_lowercase()
        {
            return Err("更新包大小或校验和不匹配".into());
        }
        file.sync_all().map_err(|e| e.to_string())?;
        let path = directory.join(&artifact.filename);
        std::fs::rename(&temporary, &path).map_err(|e| e.to_string())?;
        Ok(path)
    });
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result
}

pub fn prepare(
    archive: &Path,
    release: &Release,
    updates: &Path,
) -> Result<superlink_update::ApplyRequest, String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let target = superlink_update::running_root(&executable)
        .map_err(|_| "开发构建不能自动替换安装版，请手动安装下载的正式发布包")?;
    superlink_update::prepare_update(archive, updates, &target, &release.manifest.version)
        .map_err(|e| e.to_string())
}

/// Launches a copied helper after document queues have flushed, immediately before quitting.
pub fn launch_helper(request: &superlink_update::ApplyRequest, root: &Path) -> Result<(), String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let target = superlink_update::running_root(&executable).map_err(|e| e.to_string())?;
    if target != request.target {
        return Err("更新目标与当前运行应用不一致".into());
    }
    let package = superlink_update::read_package(&target).map_err(|e| e.to_string())?;
    let directory = root
        .join("updates")
        .join(format!("helper-{}", request.token));
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
    }
    let helper = directory.join(if cfg!(windows) {
        "update-helper.exe"
    } else {
        "update-helper"
    });
    std::fs::copy(target.join(&package.helper), &helper).map_err(|e| e.to_string())?;
    let path = directory.join("request.json");
    std::fs::write(
        &path,
        serde_json::to_vec(request).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    std::process::Command::new(helper)
        .arg(&path)
        .arg(std::process::id().to_string())
        .arg(root)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub type Prepared = superlink_update::ApplyRequest;
