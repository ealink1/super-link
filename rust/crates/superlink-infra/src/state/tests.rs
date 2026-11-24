use super::*;
use superlink_domain::ConnectionConfig;

struct TestDir(std::path::PathBuf);

impl TestDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("superlink-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp_store() -> (TestDir, Store) {
    let dir = TestDir::new();
    let store = Store::open(dir.path().join("state.db")).unwrap();
    (dir, store)
}

#[test]
fn shared_workspace_reads_and_updates_original_database_instead_of_snapshot() {
    let dir = TestDir::new();
    let original = Store::open(dir.path().join("state.sqlite")).unwrap();
    let original_profile = original
        .save_profile(&Profile {
            id: "shared".into(),
            name: "original connection".into(),
            ..Default::default()
        })
        .unwrap();
    let snapshot = Store::open(dir.path().join("state.db")).unwrap();
    let selected = workspace_path(dir.path());
    assert_eq!(selected, original.path());
    let current = Store::open(selected).unwrap();
    assert_eq!(
        current.profile("shared").unwrap().name,
        original_profile.name
    );
    let mut edited = current.profile("shared").unwrap();
    edited.name = "edited in Rust".into();
    current.save_profile(&edited).unwrap();
    assert_eq!(original.profile("shared").unwrap().name, "edited in Rust");
    assert!(snapshot.profiles().unwrap().is_empty());
}

#[test]
fn workspace_without_original_database_keeps_rust_database() {
    let dir = TestDir::new();
    assert_eq!(workspace_path(dir.path()), dir.path().join("state.db"));
    // A directory with the legacy filename is not a database.
    std::fs::create_dir(dir.path().join("state.sqlite")).unwrap();
    assert_eq!(workspace_path(dir.path()), dir.path().join("state.db"));
}

#[test]
fn profiles_round_trip_with_revisions() {
    let (_dir, store) = temp_store();
    let profile = Profile {
        id: "p1".into(),
        name: "本地".into(),
        config: ConnectionConfig {
            kind: "mysql".into(),
            ..Default::default()
        },
        ..Default::default()
    };
    let saved = store.save_profile(&profile).unwrap();
    assert_eq!(saved.revision, 1);
    assert!(saved.secret_ref.starts_with("local:v1:"));
    let loaded = store.profile("p1").unwrap();
    assert_eq!(loaded.name, "本地");
    let updated = store.save_profile(&loaded).unwrap();
    assert_eq!(updated.revision, 2);
    assert!(store.delete_profile("p1", 1).is_err());
    store.delete_profile("p1", 2).unwrap();
}

#[test]
fn stale_revision_is_rejected() {
    let (_dir, store) = temp_store();
    let mut profile = Profile {
        id: "p1".into(),
        ..Default::default()
    };
    profile.config.kind = "postgres".into();
    let saved = store.save_profile(&profile).unwrap();
    let stale = Profile {
        revision: saved.revision - 1,
        ..saved.clone()
    };
    assert!(matches!(
        store.save_profile(&stale),
        Err(StoreError::Domain(DomainError::Conflict))
    ));
}

#[test]
fn passwords_never_reach_the_database() {
    let (dir, store) = temp_store();
    let profile = Profile {
        id: "p1".into(),
        config: ConnectionConfig {
            kind: "mysql".into(),
            password: "hunter2".into(),
            ..Default::default()
        },
        ..Default::default()
    };
    store.save_profile(&profile).unwrap();
    drop(store);
    let raw = fs::read(dir.path().join("state.db")).unwrap();
    assert!(!contains(&raw, b"hunter2"));
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[test]
fn drafts_are_bounded() {
    let (_dir, store) = temp_store();
    let profile = seed_profile(&store);
    let draft = Draft {
        id: "d1".into(),
        profile_id: profile.id.clone(),
        text: "select 1".into(),
        ..Default::default()
    };
    let saved = store.save_draft(&draft).unwrap();
    assert!(saved.updated_at.is_some());
    let huge = Draft {
        id: "d2".into(),
        profile_id: profile.id,
        text: "x".repeat(MAX_DRAFT_BYTES + 1),
        ..Default::default()
    };
    assert!(matches!(
        store.save_draft(&huge),
        Err(StoreError::DraftTooLarge)
    ));
}

fn seed_profile(store: &Store) -> Profile {
    store
        .save_profile(&Profile {
            id: "p1".into(),
            ..Default::default()
        })
        .unwrap()
}

#[test]
fn saved_queries_use_optimistic_concurrency() {
    let (_dir, store) = temp_store();
    let profile = store
        .save_profile(&Profile {
            id: "p1".into(),
            ..Default::default()
        })
        .unwrap();
    let query = SavedQuery {
        id: "q1".into(),
        profile_id: profile.id.clone(),
        title: "top".into(),
        text: "select 1".into(),
        ..Default::default()
    };
    let saved = store.save_saved_query(&query).unwrap();
    assert_eq!(saved.revision, 1);

    let mut next = saved.clone();
    next.text = "select 2".into();
    assert_eq!(store.save_saved_query(&next).unwrap().revision, 2);

    let mut stale = saved.clone();
    stale.text = "select 3".into();
    assert!(matches!(
        store.save_saved_query(&stale),
        Err(StoreError::Domain(DomainError::QueryConflict))
    ));
    assert_eq!(store.saved_queries(&profile.id).unwrap().len(), 1);
}

#[test]
fn history_accepts_a_redacted_statement() {
    let (_dir, store) = temp_store();
    let profile = seed_profile(&store);
    store
        .add_history(&History {
            profile_id: profile.id,
            text: "select 1".into(),
            ..Default::default()
        })
        .unwrap();
}

#[test]
fn imports_legacy_workspace_once() {
    let dir = TestDir::new();
    let legacy_path = dir.path().join("state.sqlite");
    {
        let legacy = Store::open(&legacy_path).unwrap();
        let profile = legacy
            .save_profile(&Profile {
                id: "p1".into(),
                name: "旧连接".into(),
                ..Default::default()
            })
            .unwrap();
        legacy
            .add_history(&History {
                profile_id: profile.id,
                text: "select 1".into(),
                ..Default::default()
            })
            .unwrap();
        legacy
            .set_setting("ui.appearance", &Value::String("dark".into()))
            .unwrap();
        legacy
            .set_setting("connection.groups", &serde_json::json!(["旧分组"]))
            .unwrap();
    }

    let target = Store::open(dir.path().join("state.db")).unwrap();
    assert!(target.import_legacy(&legacy_path).unwrap());
    assert_eq!(target.profiles().unwrap().len(), 1);
    assert_eq!(target.profiles().unwrap()[0].name, "旧连接");
    assert_eq!(target.history("p1", 10).unwrap().len(), 1);
    assert_eq!(
        target.setting("theme").unwrap(),
        Some(Value::String("dark".into()))
    );
    assert_eq!(
        target.setting("connection.groups").unwrap(),
        Some(serde_json::json!(["旧分组"]))
    );

    // 标记写入后不再重复导入。
    assert!(!target.import_legacy(&legacy_path).unwrap());
    assert_eq!(target.profiles().unwrap().len(), 1);
}

#[test]
fn keeps_existing_workspace_untouched() {
    let dir = TestDir::new();
    let legacy_path = dir.path().join("state.sqlite");
    {
        let legacy = Store::open(&legacy_path).unwrap();
        legacy
            .save_profile(&Profile {
                id: "legacy".into(),
                ..Default::default()
            })
            .unwrap();
    }
    let target = Store::open(dir.path().join("state.db")).unwrap();
    target
        .save_profile(&Profile {
            id: "mine".into(),
            ..Default::default()
        })
        .unwrap();

    assert!(!target.import_legacy(&legacy_path).unwrap());
    let ids: Vec<String> = target
        .profiles()
        .unwrap()
        .into_iter()
        .map(|profile| profile.id)
        .collect();
    assert_eq!(ids, vec!["mine".to_string()]);
}

#[test]
fn rejects_legacy_workspace_with_unknown_schema() {
    let dir = TestDir::new();
    let legacy_path = dir.path().join("state.sqlite");
    {
        let raw = Connection::open(&legacy_path).unwrap();
        raw.execute_batch(
            "CREATE TABLE profiles(id TEXT PRIMARY KEY);
                 CREATE TABLE settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )
        .unwrap();
    }
    let target = Store::open(dir.path().join("state.db")).unwrap();
    assert!(!target.import_legacy(&legacy_path).unwrap());
    assert!(target.profiles().unwrap().is_empty());
}
