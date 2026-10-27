//! Icon assets and the small helper used by every panel.
//!
//! The SVGs are the reviewed assets the Go build already ships; they are read
//! from disk during development and from beside the executable in a bundle.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use gpui::{Hsla, Result, SharedString, Styled, px, svg};

const ASSET_ROOT_CANDIDATES: [&str; 3] = [
    "../../../internal/ui/assets",
    "assets",
    "../Resources/assets",
];

pub struct Assets {
    root: PathBuf,
}

impl Assets {
    pub fn discover() -> Self {
        if let Some(root) = std::env::var_os("SUPERLINK_ASSETS_DIR") {
            return Self {
                root: PathBuf::from(root),
            };
        }
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        for candidate in ASSET_ROOT_CANDIDATES {
            let path = manifest.join(candidate);
            if path.is_dir() {
                return Self { root: path };
            }
        }
        if let Ok(executable) = std::env::current_exe()
            && let Some(dir) = executable.parent()
        {
            for candidate in ASSET_ROOT_CANDIDATES {
                let path = dir.join(candidate);
                if path.is_dir() {
                    return Self { root: path };
                }
            }
        }
        Self {
            root: PathBuf::from("internal/ui/assets"),
        }
    }

    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

impl gpui::AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        match std::fs::read(self.root.join(path)) {
            Ok(bytes) => Ok(Some(Cow::Owned(bytes))),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut names = Vec::new();
        if let Ok(entries) = std::fs::read_dir(self.root.join(path)) {
            for entry in entries.flatten() {
                names.push(SharedString::from(
                    entry.path().to_string_lossy().to_string(),
                ));
            }
        }
        Ok(names)
    }
}

/// Data source icon. Falls back to the generic database glyph for engines
/// whose artwork only exists as PNG.
pub fn datasource_icon(kind: &str) -> &'static str {
    match kind {
        "mysql" => "superlink/db-mysql.svg",
        "mariadb" => "superlink/db-mariadb.svg",
        "postgres" => "superlink/db-postgres.svg",
        "sqlite" => "superlink/db-sqlite.svg",
        "sqlserver" => "superlink/db-sqlserver.svg",
        "mongodb" => "superlink/db-mongodb.svg",
        "redis" => "superlink/db-redis.svg",
        "clickhouse" => "superlink/db-clickhouse.svg",
        "elasticsearch" => "superlink/db-elasticsearch.svg",
        "diros" => "superlink/db-diros.svg",
        "duckdb" => "superlink/db-duckdb.svg",
        "iotdb" => "superlink/db-iotdb.svg",
        "vastbase" => "superlink/db-vastbase.svg",
        "sphinx" => "superlink/db-sphinx.svg",
        "chroma" => "superlink/db-chroma.svg",
        "milvus" => "superlink/db-milvus.svg",
        "qdrant" => "superlink/db-qdrant.svg",
        "nacos" => "superlink/db-nacos.svg",
        "mqtt" => "superlink/db-mqtt.svg",
        "pulsar" => "superlink/db-pulsar.svg",
        "rabbitmq" => "superlink/db-rabbitmq.svg",
        _ => "superlink/database.svg",
    }
}

/// Monochrome icon tinted by the caller. Sizing is square.
pub fn icon(path: &'static str, size: f32, color: Hsla) -> impl gpui::IntoElement {
    svg()
        .path(path)
        .w(px(size))
        .h(px(size))
        .flex_shrink_0()
        .text_color(color)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_the_repository_assets() {
        let assets = Assets::discover();
        assert!(
            assets.root().join("superlink/refresh.svg").is_file(),
            "assets root: {}",
            assets.root().display()
        );
    }

    #[test]
    fn every_mapped_icon_exists() {
        for kind in [
            "mysql",
            "mariadb",
            "postgres",
            "sqlite",
            "sqlserver",
            "mongodb",
            "redis",
            "clickhouse",
            "elasticsearch",
            "diros",
            "duckdb",
            "iotdb",
            "vastbase",
            "sphinx",
            "chroma",
            "milvus",
            "qdrant",
            "nacos",
            "mqtt",
            "pulsar",
            "rabbitmq",
            "oracle",
        ] {
            let path = datasource_icon(kind);
            let root = Assets::discover();
            assert!(
                root.root().join(path).is_file(),
                "{kind} maps to missing {path}"
            );
        }
    }

    #[test]
    fn loads_a_known_icon_and_misses_unknown_paths() {
        use gpui::AssetSource as _;
        let assets = Assets::discover();
        let bytes = assets.load("superlink/settings.svg").unwrap();
        assert!(bytes.is_some_and(|data| !data.is_empty()));
        assert!(
            assets
                .load("superlink/does-not-exist.svg")
                .unwrap()
                .is_none()
        );
    }
}
