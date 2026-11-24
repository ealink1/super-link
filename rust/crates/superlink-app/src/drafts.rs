//! Coalesced, ordered autosave. The UI reads its snapshot and never waits for SQLite.
use crate::profiles::ServiceResult;
use std::{
    collections::HashMap,
    sync::{Arc, Condvar, Mutex, RwLock},
};
use superlink_domain::Draft;
use superlink_infra::{Store, StoreError};
#[derive(Default)]
struct Pending {
    items: HashMap<String, Option<Draft>>,
    stopping: bool,
    busy: bool,
    error: Option<String>,
}
pub struct DraftService {
    cache: RwLock<HashMap<String, Draft>>,
    pending: Arc<(Mutex<Pending>, Condvar)>,
    worker: Mutex<Option<std::thread::JoinHandle<()>>>,
}
impl DraftService {
    pub fn new(store: Arc<Store>) -> Self {
        let cache = RwLock::new(
            store
                .drafts()
                .unwrap_or_default()
                .into_iter()
                .map(|d| (d.id.clone(), d))
                .collect(),
        );
        let pending = Arc::new((Mutex::new(Pending::default()), Condvar::new()));
        let work = pending.clone();
        let worker = std::thread::spawn(move || {
            loop {
                let (lock, ready) = &*work;
                let mut pending = lock.lock().unwrap_or_else(|e| e.into_inner());
                while pending.items.is_empty() && !pending.stopping {
                    pending = ready.wait(pending).unwrap_or_else(|e| e.into_inner());
                }
                let items = std::mem::take(&mut pending.items);
                pending.busy = true;
                pending.error = None;
                let stop = pending.stopping;
                drop(pending);
                for (id, draft) in items {
                    let result = match draft {
                        Some(draft) => store.save_draft(&draft).map(|_| ()),
                        None => store.delete_draft(&id),
                    };
                    if let Err(error) = result {
                        lock.lock().unwrap_or_else(|e| e.into_inner()).error =
                            Some(error.to_string());
                    }
                }
                {
                    let mut pending = lock.lock().unwrap_or_else(|e| e.into_inner());
                    pending.busy = false;
                    ready.notify_all();
                }
                if stop {
                    break;
                }
            }
        });
        Self {
            cache,
            pending,
            worker: Mutex::new(Some(worker)),
        }
    }
    pub fn list(&self) -> ServiceResult<Vec<Draft>> {
        let mut drafts = self
            .cache
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .cloned()
            .collect::<Vec<_>>();
        drafts.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(drafts)
    }
    pub fn save(&self, draft: &Draft) -> ServiceResult<Draft> {
        if draft.text.len() > 1 << 20 {
            return Err(StoreError::DraftTooLarge.into());
        }
        let mut saved = draft.clone();
        saved.updated_at = Some(chrono::Utc::now());
        self.cache
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(saved.id.clone(), saved.clone());
        let (lock, ready) = &*self.pending;
        lock.lock()
            .unwrap_or_else(|e| e.into_inner())
            .items
            .insert(saved.id.clone(), Some(saved.clone()));
        ready.notify_one();
        Ok(saved)
    }
    pub fn delete(&self, id: &str) -> ServiceResult<()> {
        self.cache
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id);
        let (lock, ready) = &*self.pending;
        lock.lock()
            .unwrap_or_else(|e| e.into_inner())
            .items
            .insert(id.into(), None);
        ready.notify_one();
        Ok(())
    }
    pub fn flush(&self) -> Result<(), String> {
        let (lock, ready) = &*self.pending;
        let mut state = lock.lock().unwrap_or_else(|e| e.into_inner());
        while state.busy || !state.items.is_empty() {
            state = ready.wait(state).unwrap_or_else(|e| e.into_inner());
        }
        if let Some(error) = state.error.clone() {
            Err(error)
        } else {
            Ok(())
        }
    }

    pub fn persistence_error(&self) -> Option<String> {
        self.pending
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .error
            .clone()
    }
}
impl Drop for DraftService {
    fn drop(&mut self) {
        let (lock, ready) = &*self.pending;
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
    #[test]
    fn quitting_flushes_the_latest_buffer_and_deleted_tabs_stay_deleted() {
        let dir = std::env::temp_dir().join(format!("sl-drafts-{}", uuid::Uuid::new_v4()));
        let store = Arc::new(Store::open(dir.join("state.db")).unwrap());
        store
            .save_profile(&superlink_domain::Profile {
                id: "profile".into(),
                config: superlink_domain::ConnectionConfig {
                    kind: "sqlite".into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .unwrap();
        {
            let service = DraftService::new(store.clone());
            for i in 0..100 {
                service
                    .save(&Draft {
                        id: "one".into(),
                        profile_id: "profile".into(),
                        text: i.to_string(),
                        ..Default::default()
                    })
                    .unwrap();
            }
            service
                .save(&Draft {
                    id: "deleted".into(),
                    profile_id: "profile".into(),
                    ..Default::default()
                })
                .unwrap();
            service.delete("deleted").unwrap();
        }
        let drafts = store.drafts().unwrap();
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].text, "99");
        drop(store);
        let _ = std::fs::remove_dir_all(dir);
    }
}
