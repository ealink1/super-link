use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use superlink_domain::{DomainError, ShellHost};
use superlink_infra::{LocalVault, SecretError, Store, StoreError};

/// SSH secrets live in the vault; the host row keeps only a reference.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HostCredentials {
    #[serde(
        rename = "Password",
        alias = "password",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub password: String,
    #[serde(
        rename = "Passphrase",
        alias = "passphrase",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub passphrase: String,
}

impl HostCredentials {
    pub fn is_empty(&self) -> bool {
        self.password.is_empty() && self.passphrase.is_empty()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum HostError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Secret(#[from] SecretError),
    #[error(transparent)]
    Domain(#[from] DomainError),
}

pub struct HostService {
    store: Arc<Store>,
    vault: Arc<LocalVault>,
    mutation: Mutex<()>,
    transient: Mutex<HashMap<String, HostCredentials>>,
}

impl HostService {
    pub fn new(store: Arc<Store>, vault: Arc<LocalVault>) -> Self {
        Self {
            store,
            vault,
            mutation: Mutex::new(()),
            transient: Mutex::new(HashMap::new()),
        }
    }

    pub fn list(&self) -> Result<Vec<ShellHost>, HostError> {
        Ok(self.store.shell_hosts()?)
    }

    pub fn save(
        &self,
        host: ShellHost,
        credentials: &HostCredentials,
    ) -> Result<ShellHost, HostError> {
        let _guard = self.mutation.lock().unwrap_or_else(|e| e.into_inner());
        host.validate()?;
        if host.id.trim().is_empty() {
            return Err(DomainError::validation("主机标识不能为空").into());
        }
        let mut host = host;
        let old = if host.revision > 0 {
            let old = self
                .list()?
                .into_iter()
                .find(|h| h.id == host.id)
                .ok_or(DomainError::NotFound)?;
            if old.revision != host.revision {
                return Err(DomainError::Conflict.into());
            }
            if old.host != host.host || old.port != host.port {
                host.fingerprint.clear();
            }
            Some(old)
        } else {
            None
        };
        let old_reference = old
            .as_ref()
            .map(|h| h.secret_ref.clone())
            .unwrap_or_default();
        let mut bundle = match old.as_ref() {
            Some(old) => self.credentials(old)?.unwrap_or_default(),
            None => HostCredentials::default(),
        };
        if !credentials.password.is_empty() {
            bundle.password = credentials.password.clone();
        }
        if !credentials.passphrase.is_empty() {
            bundle.passphrase = credentials.passphrase.clone();
        }
        host.secret_ref = format!("local:v1:{}", uuid::Uuid::new_v4());
        if host.remember && !bundle.is_empty() {
            let raw =
                serde_json::to_vec(&bundle).map_err(|_| DomainError::validation("凭据无法编码"))?;
            self.vault.put(&host.secret_ref, &raw)?;
        }
        let saved = match self.store.save_shell_host(&host) {
            Ok(saved) => saved,
            Err(error) => {
                let _ = self.vault.delete(&host.secret_ref);
                return Err(error.into());
            }
        };
        let mut transient = self.transient.lock().unwrap_or_else(|e| e.into_inner());
        if host.remember {
            transient.remove(&host.id);
        } else {
            transient.insert(host.id.clone(), bundle);
        }
        drop(transient);
        if !old_reference.is_empty() {
            let _ = self.vault.delete(&old_reference);
        }
        Ok(saved)
    }

    pub fn credentials(&self, host: &ShellHost) -> Result<Option<HostCredentials>, HostError> {
        if let Some(bundle) = self
            .transient
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&host.id)
        {
            return Ok(Some(bundle.clone()));
        }
        if host.secret_ref.is_empty() {
            return Ok(None);
        }
        match self.vault.get(&host.secret_ref) {
            Ok(raw) => serde_json::from_slice(&raw)
                .map(Some)
                .map_err(|_| SecretError::Corrupt.into()),
            Err(SecretError::Missing) => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    pub fn delete(&self, id: &str, revision: i64) -> Result<(), HostError> {
        let _guard = self.mutation.lock().unwrap_or_else(|e| e.into_inner());
        let host = self
            .list()?
            .into_iter()
            .find(|host| host.id == id)
            .ok_or(DomainError::NotFound)?;
        self.store.delete_shell_host(id, revision)?;
        self.transient
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id);
        if !host.secret_ref.is_empty() {
            self.vault.delete(&host.secret_ref)?;
        }
        Ok(())
    }

    /// Pin the confirmed key without rewriting or retiring credential bundles.
    pub fn trust(
        &self,
        id: &str,
        revision: i64,
        fingerprint: &str,
    ) -> Result<ShellHost, HostError> {
        let _guard = self.mutation.lock().unwrap_or_else(|e| e.into_inner());
        let mut host = self
            .list()?
            .into_iter()
            .find(|h| h.id == id)
            .ok_or(DomainError::NotFound)?;
        if host.revision != revision
            || !host.fingerprint.is_empty()
            || !fingerprint.starts_with("SHA256:")
        {
            return Err(DomainError::Conflict.into());
        }
        host.fingerprint = fingerprint.to_string();
        Ok(self.store.save_shell_host(&host)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        dir: std::path::PathBuf,
        service: HostService,
    }

    impl Fixture {
        fn new() -> Self {
            let dir =
                std::env::temp_dir().join(format!("superlink-hosts-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&dir).unwrap();
            let store = Arc::new(Store::open(dir.join("state.db")).unwrap());
            let vault = Arc::new(LocalVault::new(&dir));
            Self {
                dir,
                service: HostService::new(store, vault),
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn host() -> ShellHost {
        ShellHost {
            id: "h1".into(),
            name: "生产".into(),
            host: "10.0.0.1".into(),
            user: "root".into(),
            port: 22,
            remember: true,
            ..Default::default()
        }
    }

    #[test]
    fn saves_hosts_and_keeps_secrets_out_of_metadata() {
        let fixture = Fixture::new();
        let credentials = HostCredentials {
            password: "hunter2".into(),
            ..Default::default()
        };
        let saved = fixture.service.save(host(), &credentials).unwrap();
        assert_eq!(saved.revision, 1);
        assert!(saved.secret_ref.starts_with("local:v1:"));
        let list = fixture.service.list().unwrap();
        assert_eq!(list[0].host, "10.0.0.1");
        assert!(list[0].password.is_empty());
        let bundle = fixture.service.credentials(&list[0]).unwrap().unwrap();
        assert_eq!(bundle.password, "hunter2");
    }

    #[test]
    fn rejects_invalid_endpoints() {
        let fixture = Fixture::new();
        let mut bad = host();
        bad.host = "10.0.0.1; rm -rf /".into();
        assert!(
            fixture
                .service
                .save(bad, &HostCredentials::default())
                .is_err()
        );
    }

    #[test]
    fn restores_fyne_credentials_and_keeps_shared_encoding() {
        let fixture = Fixture::new();
        let reference = format!("local:v1:{}", uuid::Uuid::new_v4());
        fixture
            .service
            .vault
            .put(
                &reference,
                br#"{"Password":"old-password","Passphrase":"old-key"}"#,
            )
            .unwrap();
        let mut record = host();
        record.secret_ref = reference;
        let saved = fixture.service.store.save_shell_host(&record).unwrap();
        let credentials = fixture.service.credentials(&saved).unwrap().unwrap();
        assert_eq!(credentials.password, "old-password");
        assert_eq!(credentials.passphrase, "old-key");
        let updated = fixture
            .service
            .save(saved, &HostCredentials::default())
            .unwrap();
        let raw = fixture.service.vault.get(&updated.secret_ref).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        assert_eq!(value["Password"], "old-password");
        let lower: HostCredentials =
            serde_json::from_str(r#"{"password":"rust","passphrase":"key"}"#).unwrap();
        assert_eq!(lower.password, "rust");
    }

    #[test]
    fn unremembered_credentials_are_available_only_until_restart() {
        let fixture = Fixture::new();
        let mut record = host();
        record.remember = false;
        let saved = fixture
            .service
            .save(
                record,
                &HostCredentials {
                    password: "temporary".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(
            fixture
                .service
                .credentials(&saved)
                .unwrap()
                .unwrap()
                .password,
            "temporary"
        );
        assert!(matches!(
            fixture.service.vault.get(&saved.secret_ref),
            Err(SecretError::Missing)
        ));
        let restarted =
            HostService::new(fixture.service.store.clone(), fixture.service.vault.clone());
        assert!(restarted.credentials(&saved).unwrap().is_none());
    }

    #[test]
    fn changing_endpoint_resets_pin_and_trusting_keeps_credentials() {
        let fixture = Fixture::new();
        let saved = fixture
            .service
            .save(
                host(),
                &HostCredentials {
                    password: "saved".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let trusted = fixture
            .service
            .trust(&saved.id, saved.revision, "SHA256:verified")
            .unwrap();
        assert_eq!(saved.secret_ref, trusted.secret_ref);
        assert_eq!(
            fixture
                .service
                .credentials(&trusted)
                .unwrap()
                .unwrap()
                .password,
            "saved"
        );
        let mut moved = trusted.clone();
        moved.host = "10.0.0.2".into();
        assert!(
            fixture
                .service
                .save(moved, &Default::default())
                .unwrap()
                .fingerprint
                .is_empty()
        );
        assert!(fixture.service.save(trusted, &Default::default()).is_err());
    }

    #[test]
    fn deletion_requires_the_current_revision() {
        let fixture = Fixture::new();
        let saved = fixture
            .service
            .save(host(), &HostCredentials::default())
            .unwrap();
        assert!(fixture.service.delete("h1", saved.revision - 1).is_err() || saved.revision == 1);
        fixture.service.delete("h1", saved.revision).unwrap();
        assert!(fixture.service.list().unwrap().is_empty());
    }
}
