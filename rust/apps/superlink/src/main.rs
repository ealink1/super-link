use std::path::PathBuf;
use std::sync::Arc;

use superlink_app::{DraftService, HostService, NotebookService, ProfileService, QueryService};
use superlink_infra::{LocalVault, Store};

fn main() {
    let root = data_root();
    if let Err(err) = std::fs::create_dir_all(&root) {
        eprintln!("无法创建本地工作区 {}: {err}", root.display());
        std::process::exit(1);
    }
    let store = match Store::open(root.join("state.db")) {
        Ok(store) => Arc::new(store),
        Err(err) => {
            eprintln!("无法打开本地工作区 {}: {err}", root.display());
            std::process::exit(1);
        }
    };
    let vault = Arc::new(LocalVault::new(&root));
    let service = Arc::new(ProfileService::new(store.clone(), vault.clone()));
    let queries = Arc::new(QueryService::new(drivers_dir()));
    let drafts = Arc::new(DraftService::new(store.clone()));
    let hosts = Arc::new(HostService::new(store.clone(), vault.clone()));
    let notebook = Arc::new(NotebookService::new(store, vault));
    superlink_ui::run(service, queries, drafts, notebook, hosts);
}

/// Agent binaries live beside the application in release builds and in the
/// repository during development.
fn drivers_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("SUPERLINK_DRIVERS_DIR") {
        return PathBuf::from(dir);
    }
    if let Ok(exe) = std::env::current_exe() {
        for candidate in ["../../../bin/drivers", "../../bin/drivers"] {
            let dir = exe.join(candidate);
            if dir.is_dir() {
                return dir;
            }
        }
    }
    PathBuf::from("bin/drivers")
}

/// Same precedence as the Go build: an explicit root wins, otherwise the
/// platform application-data directory.
fn data_root() -> PathBuf {
    if let Some(root) = std::env::var_os("SUPERLINK_DATA_ROOT") {
        return PathBuf::from(root);
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/SuperLink")
    } else if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or(home)
            .join("SuperLink")
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/share"))
            .join("superlink")
    }
}
