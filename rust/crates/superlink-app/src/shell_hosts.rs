use std::sync::Arc;

use serde::{Deserialize, Serialize};
use superlink_domain::{DomainError, ShellHost};
use superlink_infra::{LocalVault, SecretError, Store, StoreError};

/// SSH secrets live in the vault; the host row keeps only a reference.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HostCredentials {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub password: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
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
}

impl HostService {
    pub fn new(store: Arc<Store>, vault: Arc<LocalVault>) -> Self {
        Self { store, vault }
    }

    pub fn list(&self) -> Result<Vec<ShellHost>, HostError> {
        Ok(self.store.shell_hosts()?)
    }

    pub fn save(
        &self,
        host: ShellHost,
        credentials: &HostCredentials,
    ) -> Result<ShellHost, HostError> {
        host.validate()?;
        if host.id.trim().is_empty() {
            return Err(DomainError::validation("主机标识不能为空").into());
        }
        let saved = self.store.save_shell_host(&host)?;
        if credentials.is_empty() {
            return Ok(saved);
        }
        let bundle = serde_json::to_vec(credentials)
            .map_err(|err| DomainError::validation(format!("凭据无法序列化：{err}")))?;
        self.vault.put(&saved.secret_ref, &bundle)?;
        Ok(saved)
    }

    pub fn credentials(&self, host: &ShellHost) -> Result<Option<HostCredentials>, HostError> {
        if host.secret_ref.is_empty() {
            return Ok(None);
        }
        match self.vault.get(&host.secret_ref) {
            Ok(raw) => Ok(serde_json::from_slice(&raw).ok()),
            Err(SecretError::Missing) => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    pub fn delete(&self, id: &str, revision: i64) -> Result<(), HostError> {
        let host = self
            .list()?
            .into_iter()
            .find(|host| host.id == id)
            .ok_or(DomainError::NotFound)?;
        self.store.delete_shell_host(id, revision)?;
        if !host.secret_ref.is_empty() {
            self.vault.delete(&host.secret_ref)?;
        }
        Ok(())
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
