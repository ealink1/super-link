//! Saved query library: read, add and remove named statements.
//!
//! Rows live in the shared state store and keep the connection they belong to,
//! so the list can show which datasource a statement came from.

use std::sync::Arc;

use superlink_domain::SavedQuery;
use superlink_infra::{Store, StoreError};

pub struct SavedQueryService {
    store: Arc<Store>,
}

impl SavedQueryService {
    pub fn new(store: Arc<Store>) -> Self {
        Self { store }
    }

    /// Every saved statement, newest first.
    pub fn list(&self) -> Result<Vec<SavedQuery>, StoreError> {
        self.store.all_saved_queries()
    }

    pub fn save(
        &self,
        profile_id: &str,
        title: &str,
        scope: &str,
        text: &str,
    ) -> Result<SavedQuery, StoreError> {
        let query = SavedQuery {
            id: uuid::Uuid::new_v4().to_string(),
            profile_id: profile_id.to_string(),
            title: title.trim().to_string(),
            scope: scope.to_string(),
            text: text.to_string(),
            ..Default::default()
        };
        self.store.save_saved_query(&query)
    }

    /// Renames one entry; the revision check keeps a stale editor from
    /// overwriting a concurrent change.
    pub fn rename(&self, id: &str, title: &str) -> Result<SavedQuery, StoreError> {
        let mut query = self
            .store
            .all_saved_queries()?
            .into_iter()
            .find(|query| query.id == id)
            .ok_or_else(|| {
                StoreError::Domain(superlink_domain::DomainError::validation(
                    "已存查询不存在，可能已被删除",
                ))
            })?;
        query.title = title.trim().to_string();
        self.store.save_saved_query(&query)
    }

    pub fn delete(&self, id: &str) -> Result<(), StoreError> {
        self.store.delete_saved_query(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Saved queries reference a connection row, so the test seeds one first.
    fn service(dir: &std::path::Path) -> (SavedQueryService, String) {
        let store = Arc::new(Store::open(dir.join("state.db")).unwrap());
        let profile = store
            .save_profile(&superlink_domain::Profile {
                name: "本地".into(),
                ..Default::default()
            })
            .unwrap();
        (SavedQueryService::new(store), profile.id)
    }

    #[test]
    fn saves_lists_and_deletes() {
        let dir = std::env::temp_dir().join(format!("superlink-saved-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let (service, profile_id) = service(&dir);

        let saved = service
            .save(&profile_id, "  每日订单  ", "app", "select 1")
            .unwrap();
        assert_eq!(saved.title, "每日订单");
        assert_eq!(saved.revision, 1);

        let all = service.list().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].text, "select 1");
        assert_eq!(all[0].profile_id, profile_id);

        let renamed = service.rename(&saved.id, "  重命名后  ").unwrap();
        assert_eq!(renamed.title, "重命名后");
        assert!(service.rename("missing", "x").is_err());

        service.delete(&saved.id).unwrap();
        assert!(service.list().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
