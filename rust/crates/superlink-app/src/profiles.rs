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
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Credentials {
    pub fn is_empty(&self) -> bool {
        self.password.is_empty()
            && self.passphrase.is_empty()
            && self.private_key.is_empty()
            && self.extra.is_empty()
    }
    pub fn apply(
        &self,
        config: &superlink_domain::ConnectionConfig,
    ) -> superlink_domain::ConnectionConfig {
        let mut value = serde_json::to_value(config).unwrap_or_default();
        if let Some(public) = value.as_object_mut() {
            superlink_domain::secrets::merge(public, &self.extra);
            public.insert(
                "password".into(),
                serde_json::Value::String(self.password.clone()),
            );
        }
        serde_json::from_value(value).unwrap_or_else(|_| config.clone())
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
    temporary: std::sync::Mutex<std::collections::HashMap<String, Credentials>>,
}

impl ProfileService {
    pub fn chat_service(&self) -> crate::chat::ChatService {
        crate::chat::ChatService::new(self.store.clone(), self.vault.clone())
    }
    pub fn new(store: Arc<Store>, vault: Arc<LocalVault>) -> Self {
        Self {
            store,
            vault,
            temporary: Default::default(),
        }
    }

    pub fn list(&self) -> ServiceResult<Vec<Profile>> {
        Ok(self.store.profiles()?)
    }

    pub fn get(&self, id: &str) -> ServiceResult<Profile> {
        Ok(self.store.profile(id)?)
    }

    /// Validates presentation, persists public metadata, then the vault bundle.
    pub fn save(&self, profile: Profile, credentials: &Credentials) -> ServiceResult<Profile> {
        self.save_with_remember(profile, credentials, true)
    }

    pub fn save_with_remember(
        &self,
        mut profile: Profile,
        credentials: &Credentials,
        remember: bool,
    ) -> ServiceResult<Profile> {
        profile.validate_presentation()?;
        let (public, secret) = superlink_domain::secrets::split_config(&profile.config)?;
        let old_reference = profile.secret_ref.clone();
        let mut bundle = if profile.revision > 0 {
            self.credentials(&profile)?.unwrap_or_default()
        } else {
            Credentials::default()
        };
        superlink_domain::secrets::merge(&mut bundle.extra, &secret);
        superlink_domain::secrets::merge(&mut bundle.extra, &credentials.extra);
        if let Some(password) = bundle
            .extra
            .remove("password")
            .and_then(|v| v.as_str().map(str::to_owned))
        {
            bundle.password = password;
        }
        if !credentials.password.is_empty() {
            bundle.password = credentials.password.clone();
        }
        if !credentials.passphrase.is_empty() {
            bundle.passphrase = credentials.passphrase.clone();
        }
        if !credentials.private_key.is_empty() {
            bundle.private_key = credentials.private_key.clone();
        }
        profile.config = public;
        profile.config.save_password = remember;
        profile.secret_ref = format!("local:v1:{}", uuid::Uuid::new_v4());
        if remember && !bundle.is_empty() {
            let raw = serde_json::to_vec(&bundle)
                .map_err(|_| ServiceError::Domain(DomainError::validation("凭据无法编码")))?;
            self.vault.put(&profile.secret_ref, &raw)?;
        }
        let saved = match self.store.save_profile(&profile) {
            Ok(saved) => saved,
            Err(error) => {
                let _ = self.vault.delete(&profile.secret_ref);
                return Err(error.into());
            }
        };
        let mut temporary = self.temporary.lock().unwrap_or_else(|e| e.into_inner());
        if remember {
            temporary.remove(&saved.id);
        } else {
            if temporary.len() >= 256
                && !temporary.contains_key(&saved.id)
                && let Some(first) = temporary.keys().next().cloned()
            {
                temporary.remove(&first);
            }
            temporary.insert(saved.id.clone(), bundle);
        }
        drop(temporary);
        if !old_reference.is_empty() && old_reference != saved.secret_ref {
            let _ = self.vault.delete(&old_reference);
        }
        Ok(saved)
    }

    pub fn credentials(&self, profile: &Profile) -> ServiceResult<Option<Credentials>> {
        if let Some(credentials) = self
            .temporary
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&profile.id)
            .cloned()
        {
            return Ok(Some(credentials));
        }
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
        self.temporary
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id);
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
