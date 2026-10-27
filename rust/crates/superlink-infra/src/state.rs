use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;
use superlink_domain::{DomainError, Draft, History, Profile, SavedQuery};

const SCHEMA_VERSION: i64 = 2;
const MAX_DRAFT_BYTES: usize = 1 << 20;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("state path must be absolute")]
    RelativePath,
    #[error("state schema {0} is newer than this application")]
    NewerSchema(i64),
    #[error("draft exceeds 1 MiB")]
    DraftTooLarge,
    #[error("credential reference is missing")]
    MissingSecretRef,
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub type StoreResult<T> = Result<T, StoreError>;

/// Application metadata. Credentials live in the vault, never here.
#[derive(Debug)]
pub struct Store {
    path: PathBuf,
    db: Arc<Mutex<Connection>>,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> StoreResult<Self> {
        let path = path.as_ref();
        if !path.is_absolute() {
            return Err(StoreError::RelativePath);
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
            harden_dir(parent)?;
        }
        OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(path)?;
        harden_file(path)?;

        let connection = Connection::open(path)?;
        connection.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000; PRAGMA foreign_keys=ON;",
        )?;
        migrate(&connection)?;
        ensure_optional_tables(&connection)?;
        Ok(Self {
            path: path.to_path_buf(),
            db: Arc::new(Mutex::new(connection)),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn profiles(&self) -> StoreResult<Vec<Profile>> {
        let db = self.lock();
        let mut statement = db.prepare(
            "SELECT metadata,secret_ref,revision FROM profiles ORDER BY name COLLATE NOCASE,id",
        )?;
        let rows = statement.query_map([], read_profile_row)?;
        let mut profiles = Vec::new();
        for row in rows {
            profiles.push(row?);
        }
        Ok(profiles)
    }

    pub fn profile(&self, id: &str) -> StoreResult<Profile> {
        let db = self.lock();
        let found = db
            .query_row(
                "SELECT metadata,secret_ref,revision FROM profiles WHERE id=?",
                [id],
                read_profile_row,
            )
            .optional()?;
        found.ok_or(StoreError::Domain(DomainError::NotFound))
    }

    /// Compares revisions so two editors cannot overwrite each other.
    pub fn save_profile(&self, profile: &Profile) -> StoreResult<Profile> {
        let mut saved = profile.clone();
        saved.config = saved.config.clone().without_secrets();
        if saved.revision == 0 {
            saved.secret_ref = new_secret_ref();
        }
        if saved.secret_ref.is_empty() {
            return Err(StoreError::MissingSecretRef);
        }
        let metadata = serde_json::to_string(&saved)?;
        let db = self.lock();
        let changed = if saved.revision == 0 {
            db.execute(
                "INSERT INTO profiles(id,name,metadata,secret_ref,revision) VALUES(?,?,?,?,1)
                 ON CONFLICT(id) DO NOTHING",
                params![saved.id, saved.name, metadata, saved.secret_ref],
            )?
        } else {
            db.execute(
                "UPDATE profiles SET name=?,metadata=?,secret_ref=?,revision=revision+1
                 WHERE id=? AND revision=?",
                params![
                    saved.name,
                    metadata,
                    saved.secret_ref,
                    saved.id,
                    saved.revision
                ],
            )?
        };
        if changed != 1 {
            return Err(StoreError::Domain(DomainError::Conflict));
        }
        saved.revision += 1;
        Ok(saved)
    }

    pub fn delete_profile(&self, id: &str, revision: i64) -> StoreResult<()> {
        let db = self.lock();
        let changed = db.execute(
            "DELETE FROM profiles WHERE id=? AND revision=?",
            params![id, revision],
        )?;
        if changed != 1 {
            return Err(StoreError::Domain(DomainError::Conflict));
        }
        Ok(())
    }

    pub fn drafts(&self) -> StoreResult<Vec<Draft>> {
        let db = self.lock();
        let mut statement = db.prepare("SELECT metadata FROM drafts ORDER BY rowid")?;
        let rows = statement.query_map([], |row| {
            let raw: String = row.get(0)?;
            Ok(raw)
        })?;
        let mut drafts = Vec::new();
        for raw in rows {
            drafts.push(serde_json::from_str(&raw?)?);
        }
        Ok(drafts)
    }

    pub fn save_draft(&self, draft: &Draft) -> StoreResult<Draft> {
        if draft.text.len() > MAX_DRAFT_BYTES {
            return Err(StoreError::DraftTooLarge);
        }
        let mut saved = draft.clone();
        saved.updated_at = Some(chrono::Utc::now());
        let metadata = serde_json::to_string(&saved)?;
        let db = self.lock();
        db.execute(
            "INSERT INTO drafts(id,profile_id,metadata) VALUES(?,?,?)
             ON CONFLICT(id) DO UPDATE SET metadata=excluded.metadata,profile_id=excluded.profile_id",
            params![saved.id, saved.profile_id, metadata],
        )?;
        Ok(saved)
    }

    pub fn delete_draft(&self, id: &str) -> StoreResult<()> {
        let db = self.lock();
        db.execute("DELETE FROM drafts WHERE id=?", [id])?;
        Ok(())
    }

    /// History stores redacted query text supplied by the caller; never results.
    pub fn add_history(&self, history: &History) -> StoreResult<()> {
        let db = self.lock();
        db.execute(
            "INSERT INTO history(profile_id,text,success,rows_count,duration_ns,created_at)
             VALUES(?,?,?,?,?,?)",
            params![
                history.profile_id,
                history.text,
                history.success,
                history.rows,
                history.duration_ns as i64,
                chrono::Utc::now().timestamp_millis()
            ],
        )?;
        Ok(())
    }

    pub fn saved_queries(&self, profile_id: &str) -> StoreResult<Vec<SavedQuery>> {
        let db = self.lock();
        let mut statement = db.prepare(
            "SELECT id,profile_id,title,scope,schema_name,text,revision,updated_at
             FROM saved_queries WHERE profile_id=? ORDER BY title COLLATE NOCASE,id",
        )?;
        let rows = statement.query_map([profile_id], |row| {
            Ok(SavedQuery {
                id: row.get(0)?,
                profile_id: row.get(1)?,
                title: row.get(2)?,
                scope: row.get(3)?,
                schema: row.get(4)?,
                text: row.get(5)?,
                revision: row.get(6)?,
                updated_at: chrono::DateTime::from_timestamp_millis(row.get(7)?),
            })
        })?;
        let mut queries = Vec::new();
        for row in rows {
            queries.push(row?);
        }
        Ok(queries)
    }

    /// Optimistic concurrency keeps two editors from clobbering each other.
    pub fn save_saved_query(&self, query: &SavedQuery) -> StoreResult<SavedQuery> {
        let mut saved = query.clone();
        let now = chrono::Utc::now();
        let db = self.lock();
        let changed = if saved.revision == 0 {
            db.execute(
                "INSERT INTO saved_queries
                 (id,profile_id,title,scope,schema_name,text,revision,updated_at)
                 VALUES(?,?,?,?,?,?,1,?)",
                params![
                    saved.id,
                    saved.profile_id,
                    saved.title,
                    saved.scope,
                    saved.schema,
                    saved.text,
                    now.timestamp_millis()
                ],
            )?
        } else {
            db.execute(
                "UPDATE saved_queries SET title=?,scope=?,schema_name=?,text=?,revision=revision+1,
                 updated_at=? WHERE id=? AND revision=?",
                params![
                    saved.title,
                    saved.scope,
                    saved.schema,
                    saved.text,
                    now.timestamp_millis(),
                    saved.id,
                    saved.revision
                ],
            )?
        };
        if changed != 1 {
            return Err(StoreError::Domain(DomainError::QueryConflict));
        }
        saved.revision += 1;
        saved.updated_at = Some(now);
        Ok(saved)
    }

    pub fn delete_saved_query(&self, id: &str) -> StoreResult<()> {
        let db = self.lock();
        db.execute("DELETE FROM saved_queries WHERE id=?", [id])?;
        Ok(())
    }

    pub fn shell_hosts(&self) -> StoreResult<Vec<superlink_domain::ShellHost>> {
        let db = self.lock();
        let mut statement =
            db.prepare("SELECT metadata,secret_ref,revision FROM shell_hosts ORDER BY rowid")?;
        let rows = statement.query_map([], read_shell_host_row)?;
        let mut hosts = Vec::new();
        for row in rows {
            hosts.push(row?);
        }
        Ok(hosts)
    }

    /// Creates or updates a host under the same revision discipline as profiles.
    pub fn save_shell_host(
        &self,
        host: &superlink_domain::ShellHost,
    ) -> StoreResult<superlink_domain::ShellHost> {
        let mut saved = host.clone();
        if saved.revision == 0 {
            saved.secret_ref = new_secret_ref();
        }
        if saved.secret_ref.is_empty() {
            return Err(StoreError::MissingSecretRef);
        }
        let metadata = serde_json::to_string(&saved)?;
        let db = self.lock();
        let changed = if saved.revision == 0 {
            db.execute(
                "INSERT INTO shell_hosts(id,metadata,secret_ref,revision) VALUES(?,?,?,1)
                 ON CONFLICT(id) DO NOTHING",
                params![saved.id, metadata, saved.secret_ref],
            )?
        } else {
            db.execute(
                "UPDATE shell_hosts SET metadata=?,secret_ref=?,revision=revision+1
                 WHERE id=? AND revision=?",
                params![metadata, saved.secret_ref, saved.id, saved.revision],
            )?
        };
        if changed != 1 {
            return Err(StoreError::Domain(DomainError::Conflict));
        }
        saved.revision += 1;
        Ok(saved)
    }

    pub fn delete_shell_host(&self, id: &str, revision: i64) -> StoreResult<()> {
        let db = self.lock();
        let changed = db.execute(
            "DELETE FROM shell_hosts WHERE id=? AND revision=?",
            params![id, revision],
        )?;
        if changed != 1 {
            return Err(StoreError::Domain(DomainError::Conflict));
        }
        Ok(())
    }

    /// The notebook is one encrypted snapshot; only its reference and revision
    /// live in the metadata database.
    pub fn notebook_ref(&self) -> StoreResult<Option<(String, i64)>> {
        let db = self.lock();
        db.query_row(
            "SELECT secret_ref,revision FROM notebook WHERE id=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(StoreError::from)
    }

    /// Publishes a new snapshot reference only when the caller saw the latest
    /// revision, so two windows cannot silently drop notes.
    pub fn save_notebook_ref(&self, secret_ref: &str, expected_revision: i64) -> StoreResult<i64> {
        let db = self.lock();
        let changed = if expected_revision == 0 {
            db.execute(
                "INSERT INTO notebook(id,secret_ref,revision) VALUES(1,?,1)
                 ON CONFLICT(id) DO NOTHING",
                [secret_ref],
            )?
        } else {
            db.execute(
                "UPDATE notebook SET secret_ref=?,revision=revision+1 WHERE id=1 AND revision=?",
                params![secret_ref, expected_revision],
            )?
        };
        if changed != 1 {
            return Err(StoreError::Domain(DomainError::Conflict));
        }
        Ok(expected_revision + 1)
    }

    pub fn setting(&self, key: &str) -> StoreResult<Option<Value>> {
        let db = self.lock();
        let raw: Option<String> = db
            .query_row("SELECT value FROM settings WHERE key=?", [key], |row| {
                row.get(0)
            })
            .optional()?;
        Ok(raw.map(|text| serde_json::from_str(&text)).transpose()?)
    }

    pub fn set_setting(&self, key: &str, value: &Value) -> StoreResult<()> {
        let db = self.lock();
        db.execute(
            "INSERT INTO settings(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, serde_json::to_string(value)?],
        )?;
        Ok(())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.db
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn read_shell_host_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<superlink_domain::ShellHost> {
    let raw: String = row.get(0)?;
    let secret_ref: String = row.get(1)?;
    let revision: i64 = row.get(2)?;
    let mut host: superlink_domain::ShellHost = serde_json::from_str(&raw).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(err))
    })?;
    host.secret_ref = secret_ref;
    host.revision = revision;
    Ok(host)
}

fn read_profile_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Profile> {
    let raw: String = row.get(0)?;
    let secret_ref: String = row.get(1)?;
    let revision: i64 = row.get(2)?;
    let mut profile: Profile = serde_json::from_str(&raw).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(err))
    })?;
    profile.secret_ref = secret_ref;
    profile.revision = revision;
    Ok(profile)
}

fn migrate(connection: &Connection) -> StoreResult<()> {
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version > SCHEMA_VERSION {
        return Err(StoreError::NewerSchema(version));
    }
    if version == SCHEMA_VERSION {
        return Ok(());
    }
    connection.execute_batch("BEGIN")?;
    let result = (|| -> StoreResult<()> {
        if version == 0 {
            connection.execute_batch(
                "CREATE TABLE profiles(id TEXT PRIMARY KEY, name TEXT NOT NULL, metadata TEXT NOT NULL, secret_ref TEXT NOT NULL, revision INTEGER NOT NULL);
                 CREATE TABLE drafts(id TEXT PRIMARY KEY, profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE, metadata TEXT NOT NULL);
                 CREATE TABLE history(id INTEGER PRIMARY KEY AUTOINCREMENT, profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE, text TEXT NOT NULL, success INTEGER NOT NULL, rows_count INTEGER NOT NULL, duration_ns INTEGER NOT NULL, created_at INTEGER NOT NULL);
                 CREATE TABLE settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);
                 PRAGMA user_version=1;",
            )?;
        }
        connection.execute_batch(
            "CREATE TABLE saved_queries(id TEXT PRIMARY KEY, profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE, title TEXT NOT NULL, scope TEXT NOT NULL, schema_name TEXT NOT NULL, text TEXT NOT NULL, revision INTEGER NOT NULL, updated_at INTEGER NOT NULL);
             PRAGMA user_version=2;",
        )?;
        Ok(())
    })();
    match result {
        Ok(()) => {
            connection.execute_batch("COMMIT")?;
            Ok(())
        }
        Err(err) => {
            let _ = connection.execute_batch("ROLLBACK");
            Err(err)
        }
    }
}

/// Tables the Go build creates on first use. Keeping them idempotent lets both
/// implementations share one workspace without a migration bump.
fn ensure_optional_tables(connection: &Connection) -> StoreResult<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS notebook(id INTEGER PRIMARY KEY CHECK(id=1), secret_ref TEXT NOT NULL, revision INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS shell_hosts(id TEXT PRIMARY KEY, metadata TEXT NOT NULL, secret_ref TEXT NOT NULL, revision INTEGER NOT NULL);",
    )?;
    Ok(())
}

fn new_secret_ref() -> String {
    format!("local:v1:{}", uuid::Uuid::new_v4())
}

#[cfg(unix)]
fn harden_dir(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn harden_dir(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn harden_file(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn harden_file(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use superlink_domain::ConnectionConfig;

    struct TestDir(std::path::PathBuf);

    impl TestDir {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("superlink-test-{}", uuid::Uuid::new_v4()));
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
}
