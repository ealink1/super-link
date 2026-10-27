use std::sync::Arc;

use serde::{Deserialize, Serialize};
use superlink_domain::{DomainError, Profile};
use superlink_infra::{LocalVault, SecretError, Store, StoreError};

/// Credential bundle written to the vault. Never serialized to metadata.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Credentials {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub password: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub passphrase: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub private_key: String,
}

impl Credentials {
    pub fn is_empty(&self) -> bool {
        self.password.is_empty() && self.passphrase.is_empty() && self.private_key.is_empty()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Secret(#[from] SecretError),
    #[error(transparent)]
    Domain(#[from] DomainError),
}

pub type ServiceResult<T> = Result<T, ServiceError>;

impl ServiceError {
    /// Store errors wrap domain errors, so callers need one predicate.
    pub fn is_not_found(&self) -> bool {
        matches!(
            self,
            ServiceError::Domain(DomainError::NotFound)
                | ServiceError::Store(StoreError::Domain(DomainError::NotFound))
        )
    }
}

/// Owns connection profiles and keeps credentials out of their metadata.
pub struct ProfileService {
    store: Arc<Store>,
    vault: Arc<LocalVault>,
}

impl ProfileService {
    pub fn new(store: Arc<Store>, vault: Arc<LocalVault>) -> Self {
        Self { store, vault }
    }

    pub fn list(&self) -> ServiceResult<Vec<Profile>> {
        Ok(self.store.profiles()?)
    }

    pub fn get(&self, id: &str) -> ServiceResult<Profile> {
        Ok(self.store.profile(id)?)
    }

    /// Validates presentation, persists public metadata, then the vault bundle.
    pub fn save(&self, profile: Profile, credentials: &Credentials) -> ServiceResult<Profile> {
        profile.validate_presentation()?;
        let saved = self.store.save_profile(&profile)?;
        if !credentials.is_empty() {
            let bundle = serde_json::to_vec(credentials).map_err(|err| {
                ServiceError::Domain(DomainError::validation(format!("凭据无法序列化：{err}")))
            })?;
            self.vault.put(&saved.secret_ref, &bundle)?;
        }
        Ok(saved)
    }

    pub fn credentials(&self, profile: &Profile) -> ServiceResult<Option<Credentials>> {
        if profile.secret_ref.is_empty() {
            return Ok(None);
        }
        match self.vault.get(&profile.secret_ref) {
            Ok(raw) => {
                let bundle: Credentials = serde_json::from_slice(&raw)
                    .map_err(|_| ServiceError::Secret(SecretError::Corrupt))?;
                Ok(Some(bundle))
            }
            Err(SecretError::Missing) => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    pub fn delete(&self, id: &str, revision: i64) -> ServiceResult<()> {
        let profile = self.store.profile(id)?;
        self.store.delete_profile(id, revision)?;
        if !profile.secret_ref.is_empty() {
            self.vault.delete(&profile.secret_ref)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use superlink_domain::ConnectionConfig;
    use superlink_infra::{LocalVault, Store};

    struct Fixture {
        dir: std::path::PathBuf,
        service: ProfileService,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!("superlink-app-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&dir).unwrap();
            let store = Arc::new(Store::open(dir.join("state.db")).unwrap());
            let vault = Arc::new(LocalVault::new(&dir));
            Self {
                dir,
                service: ProfileService::new(store, vault),
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn profile() -> Profile {
        Profile {
            id: "p1".into(),
            name: "生产".into(),
            config: ConnectionConfig {
                kind: "mysql".into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn saves_and_reads_credentials_separately_from_metadata() {
        let fixture = Fixture::new();
        let credentials = Credentials {
            password: "hunter2".into(),
            ..Default::default()
        };
        let saved = fixture.service.save(profile(), &credentials).unwrap();
        let loaded = fixture.service.get("p1").unwrap();
        assert_eq!(loaded.config.password, "");
        let bundle = fixture.service.credentials(&loaded).unwrap().unwrap();
        assert_eq!(bundle.password, "hunter2");
        assert_eq!(saved.revision, 1);
    }

    #[test]
    fn deleting_a_profile_removes_its_bundle() {
        let fixture = Fixture::new();
        let credentials = Credentials {
            password: "hunter2".into(),
            ..Default::default()
        };
        let saved = fixture.service.save(profile(), &credentials).unwrap();
        fixture.service.delete("p1", saved.revision).unwrap();
        assert!(fixture.service.get("p1").unwrap_err().is_not_found());
    }

    #[test]
    fn invalid_presentation_is_rejected_before_persistence() {
        let fixture = Fixture::new();
        let mut bad = profile();
        bad.icon_color = "green".into();
        assert!(fixture.service.save(bad, &Credentials::default()).is_err());
        assert!(fixture.service.list().unwrap().is_empty());
    }
}
