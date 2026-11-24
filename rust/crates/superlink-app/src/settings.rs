//! Small key-value settings owned by the application layer.

use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex, RwLock};

use serde_json::Value;
use superlink_infra::Store;

pub struct SettingsService {
    values: RwLock<HashMap<String, Value>>,
    writer: Arc<(Mutex<Pending>, Condvar)>,
    worker: Mutex<Option<std::thread::JoinHandle<()>>>,
}

#[derive(Default)]
struct Pending {
    values: HashMap<String, Value>,
    stopping: bool,
    busy: bool,
    error: Option<String>,
}

impl SettingsService {
    pub fn new(store: Arc<Store>) -> Self {
        let values = RwLock::new(store.settings_snapshot().unwrap_or_default());
        let writer = Arc::new((Mutex::new(Pending::default()), Condvar::new()));
        let state = writer.clone();
        let worker = std::thread::spawn(move || {
            loop {
                let (lock, ready) = &*state;
                let mut pending = lock.lock().unwrap_or_else(|e| e.into_inner());
                while pending.values.is_empty() && !pending.stopping {
                    pending = ready.wait(pending).unwrap_or_else(|e| e.into_inner());
                }
                let values = std::mem::take(&mut pending.values);
                pending.busy = true;
                pending.error = None;
                let stopping = pending.stopping;
                drop(pending);
                for (key, value) in values {
                    if let Err(error) = store.set_setting(&key, &value) {
                        lock.lock().unwrap_or_else(|e| e.into_inner()).error =
                            Some(error.to_string());
                    }
                }
                {
                    let mut pending = lock.lock().unwrap_or_else(|e| e.into_inner());
                    pending.busy = false;
                    ready.notify_all();
                }
                if stopping {
                    break;
                }
            }
        });
        Self {
            values,
            writer,
            worker: Mutex::new(Some(worker)),
        }
    }

    pub fn get(&self, key: &str) -> Option<Value> {
        self.values
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(key)
            .cloned()
    }

    pub fn set(&self, key: &str, value: Value) {
        self.values
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(key.into(), value.clone());
        let (lock, ready) = &*self.writer;
        lock.lock()
            .unwrap_or_else(|e| e.into_inner())
            .values
            .insert(key.into(), value);
        ready.notify_one();
    }

    pub fn flush(&self) -> Result<(), String> {
        let (lock, ready) = &*self.writer;
        let mut state = lock.lock().unwrap_or_else(|e| e.into_inner());
        while state.busy || !state.values.is_empty() {
            state = ready.wait(state).unwrap_or_else(|e| e.into_inner());
        }
        if let Some(error) = state.error.clone() {
            Err(error)
        } else {
            Ok(())
        }
    }

    pub fn persistence_error(&self) -> Option<String> {
        self.writer
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .error
            .clone()
    }

    fn shell_appearance(&self) -> Value {
        match self.get("shell.appearance").unwrap_or(Value::Null) {
            Value::String(raw) => serde_json::from_str(&raw).unwrap_or(Value::Null),
            value => value,
        }
    }

    pub fn shell_font_size(&self) -> u32 {
        self.get("shell.fontSize")
            .and_then(|v| v.as_u64())
            .or_else(|| {
                self.shell_appearance()
                    .get("FontSize")
                    .and_then(Value::as_u64)
            })
            .unwrap_or(16)
            .clamp(10, 24) as u32
    }

    pub fn shell_default_program(&self) -> Option<String> {
        let program = self
            .get("shell.defaultProgram")
            .and_then(|v| v.as_str().map(str::to_owned))
            .or_else(|| {
                self.shell_appearance()
                    .get("DefaultShell")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })?;
        let allowed = if cfg!(windows) {
            ["powershell.exe", "cmd.exe"]
        } else {
            ["/bin/zsh", "/bin/bash"]
        };
        allowed.contains(&program.as_str()).then_some(program)
    }

    pub fn shell_reconnect_enabled(&self) -> bool {
        self.get("shell.autoReconnect")
            .and_then(|v| v.as_bool())
            .unwrap_or(true)
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

impl Drop for SettingsService {
    fn drop(&mut self) {
        let (lock, ready) = &*self.writer;
        lock.lock().unwrap_or_else(|e| e.into_inner()).stopping = true;
        ready.notify_one();
        if let Some(worker) = self.worker.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = worker.join();
        }
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
        settings.flush().unwrap();
        drop(settings);
        let _ = std::fs::remove_dir_all(dir);
    }
    #[test]
    fn keeps_legacy_shell_preferences_and_new_overrides() {
        let (dir, settings) = service();
        settings.set(
            "shell.appearance",
            serde_json::json!({"DefaultShell":"/bin/bash", "FontSize":15}),
        );
        assert_eq!(settings.shell_font_size(), 15);
        if !cfg!(windows) {
            assert_eq!(
                settings.shell_default_program().as_deref(),
                Some("/bin/bash")
            );
        }
        settings.set("shell.fontSize", Value::from(18));
        settings.set("shell.defaultProgram", Value::String("".into()));
        settings.set("shell.autoReconnect", Value::Bool(false));
        assert_eq!(settings.shell_font_size(), 18);
        assert!(settings.shell_default_program().is_none());
        assert!(!settings.shell_reconnect_enabled());
        settings.flush().unwrap();
        drop(settings);
        let _ = std::fs::remove_dir_all(dir);
    }
}
