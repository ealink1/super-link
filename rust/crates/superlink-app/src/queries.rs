use std::sync::Arc;

use superlink_domain::{Draft, SavedQuery};
use superlink_infra::{Store, StoreError};

use crate::profiles::ServiceResult;

/// Autosaved editor buffers. One draft per tab, bounded at 1 MiB.
pub struct DraftService {
    store: Arc<Store>,
}

impl DraftService {
    pub fn new(store: Arc<Store>) -> Self {
        Self { store }
    }

    pub fn list(&self) -> ServiceResult<Vec<Draft>> {
        Ok(self.store.drafts()?)
    }

    pub fn save(&self, draft: &Draft) -> ServiceResult<Draft> {
        Ok(self.store.save_draft(draft)?)
    }

    pub fn delete(&self, id: &str) -> ServiceResult<()> {
        Ok(self.store.delete_draft(id)?)
    }
}

/// Explicitly named SQL documents, separate from drafts.
pub struct QueryLibrary {
    store: Arc<Store>,
}

impl QueryLibrary {
    pub fn new(store: Arc<Store>) -> Self {
        Self { store }
    }

    pub fn list(&self, profile_id: &str) -> ServiceResult<Vec<SavedQuery>> {
        Ok(self.store.saved_queries(profile_id)?)
    }

    pub fn save(&self, query: &SavedQuery) -> ServiceResult<SavedQuery> {
        if query.title.trim().is_empty() {
            return Err(
                StoreError::Domain(superlink_domain::DomainError::validation("请输入查询名称"))
                    .into(),
            );
        }
        if query.text.len() > 1 << 20 {
            return Err(
                StoreError::Domain(superlink_domain::DomainError::validation("查询超过 1 MiB"))
                    .into(),
            );
        }
        Ok(self.store.save_saved_query(query)?)
    }

    pub fn delete(&self, id: &str) -> ServiceResult<()> {
        Ok(self.store.delete_saved_query(id)?)
    }
}
