//! Small key-value settings owned by the application layer.

use std::sync::Arc;

use serde_json::Value;
use superlink_infra::Store;

pub struct SettingsService {
    store: Arc<Store>,
}

impl SettingsService {
    pub fn new(store: Arc<Store>) -> Self {
        Self { store }
    }

    pub fn get(&self, key: &str) -> Option<Value> {
        self.store.setting(key).ok().flatten()
    }

    pub fn set(&self, key: &str, value: Value) {
        let _ = self.store.set_setting(key, &value);
    }

    pub fn theme_mode(&self) -> String {
        self.get("theme")
            .and_then(|value| value.as_str().map(str::to_string))
            .unwrap_or_else(|| "system".into())
    }

    pub fn set_theme_mode(&self, mode: &str) {
        self.set("theme", Value::String(mode.to_string()));
    }

    pub fn set_row_limit(&self, limit: u32) {
        let limit = limit.clamp(100, 100_000);
        self.set("rowLimit", Value::from(limit));
    }

    pub fn row_limit(&self) -> u32 {
        self.get("rowLimit")
            .and_then(|value| value.as_u64())
            .map(|value| value.clamp(100, 100_000) as u32)
            .unwrap_or(5_000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service() -> (std::path::PathBuf, SettingsService) {
        let dir = std::env::temp_dir().join(format!("superlink-settings-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Arc::new(Store::open(dir.join("state.db")).unwrap());
        (dir, SettingsService::new(store))
    }

    #[test]
    fn defaults_then_round_trips() {
        let (dir, settings) = service();
        assert_eq!(settings.theme_mode(), "system");
        settings.set_theme_mode("dark");
        assert_eq!(settings.theme_mode(), "dark");
        assert_eq!(settings.row_limit(), 5_000);
        let _ = std::fs::remove_dir_all(dir);
    }
}
