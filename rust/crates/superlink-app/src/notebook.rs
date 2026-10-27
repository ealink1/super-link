use std::sync::Arc;

use superlink_domain::{DomainError, Notebook};
use superlink_infra::{LocalVault, SecretError, Store, StoreError};

#[derive(Debug, thiserror::Error)]
pub enum NotebookError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Secret(#[from] SecretError),
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error("笔记本快照损坏")]
    Corrupt,
}

pub type NotebookResult<T> = Result<T, NotebookError>;

/// Owns the encrypted notebook snapshot: validate, seal, then publish the
/// reference under a revision check.
pub struct NotebookService {
    store: Arc<Store>,
    vault: Arc<LocalVault>,
}

impl NotebookService {
    pub fn new(store: Arc<Store>, vault: Arc<LocalVault>) -> Self {
        Self { store, vault }
    }

    pub fn load(&self) -> NotebookResult<Notebook> {
        let Some((reference, revision)) = self.store.notebook_ref()? else {
            return Ok(Notebook::default());
        };
        let raw = match self.vault.get(&reference) {
            Ok(raw) => raw,
            Err(SecretError::Missing) => return Ok(Notebook::default()),
            Err(err) => return Err(err.into()),
        };
        let mut notebook: Notebook =
            serde_json::from_slice(&raw).map_err(|_| NotebookError::Corrupt)?;
        notebook.revision = revision;
        Ok(notebook)
    }

    pub fn save(&self, notebook: &Notebook) -> NotebookResult<Notebook> {
        notebook.validate()?;
        let revision = notebook.revision;
        let reference = match self.store.notebook_ref()? {
            Some((reference, _)) if revision > 0 => reference,
            _ => format!("local:v1:{}", uuid::Uuid::new_v4()),
        };
        let raw = serde_json::to_vec(notebook)
            .map_err(|err| DomainError::validation(format!("笔记本无法序列化：{err}")))?;
        self.vault.put(&reference, &raw)?;
        let saved = self.store.save_notebook_ref(&reference, revision)?;
        let mut snapshot = notebook.clone();
        snapshot.revision = saved;
        Ok(snapshot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use superlink_domain::{Note, NoteGroup};

    struct Fixture {
        dir: std::path::PathBuf,
        service: NotebookService,
    }

    impl Fixture {
        fn new() -> Self {
            let dir =
                std::env::temp_dir().join(format!("superlink-notes-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&dir).unwrap();
            let store = Arc::new(Store::open(dir.join("state.db")).unwrap());
            let vault = Arc::new(LocalVault::new(&dir));
            Self {
                dir,
                service: NotebookService::new(store, vault),
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn starts_empty_then_round_trips() {
        let fixture = Fixture::new();
        assert!(fixture.service.load().unwrap().notes.is_empty());
        let notebook = Notebook {
            notes: vec![Note {
                id: "n1".into(),
                title: "周报".into(),
                body: "# 本周".into(),
                ..Default::default()
            }],
            groups: vec![NoteGroup {
                id: "g1".into(),
                name: "工作".into(),
            }],
            revision: 0,
        };
        let saved = fixture.service.save(&notebook).unwrap();
        assert_eq!(saved.revision, 1);
        let loaded = fixture.service.load().unwrap();
        assert_eq!(loaded.notes.len(), 1);
        assert_eq!(loaded.notes[0].title, "周报");
        assert_eq!(loaded.groups[0].name, "工作");
    }

    #[test]
    fn rejects_invalid_notebooks_before_encrypting() {
        let fixture = Fixture::new();
        let notebook = Notebook {
            notes: vec![Note {
                id: "n1".into(),
                body: "x".repeat(300 << 10),
                ..Default::default()
            }],
            ..Default::default()
        };
        assert!(fixture.service.save(&notebook).is_err());
    }

    #[test]
    fn stale_snapshots_conflict() {
        let fixture = Fixture::new();
        let notebook = Notebook {
            notes: vec![Note {
                id: "n1".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let first = fixture.service.save(&notebook).unwrap();
        let stale = Notebook {
            revision: 0,
            ..notebook.clone()
        };
        assert!(fixture.service.save(&stale).is_err());
        let next = Notebook {
            revision: first.revision,
            ..notebook
        };
        assert_eq!(fixture.service.save(&next).unwrap().revision, 2);
    }
}
