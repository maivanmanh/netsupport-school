//! The Teacher's single local database.
//!
//! Every setting, remembered selection, policy and journal that the Teacher
//! keeps between runs lives in one SQLite file (`classroom.db`). Nothing else
//! (JSON, TOML, preference caches) is authoritative any more; the legacy files
//! are imported once by [`Store::import_legacy`] and set aside.
//!
//! Layout: one `documents` table addressed by `(namespace, key)` holding JSON.
//! Namespaces are fixed by [`ns`]. Documents were chosen over one table per
//! feature because the features are small, independently evolving and mostly
//! read whole; a single access path keeps durability, migration and backup
//! uniform. The Rust agent is the only process that opens the file; Flutter
//! windows reach it through the agent's IPC, so there is one writer.
#![allow(clippy::missing_errors_doc)]

mod legacy;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

pub use legacy::ImportReport;

pub const FILE_NAME: &str = "classroom.db";
/// Largest single document (the recording journal is the biggest user).
pub const MAX_DOCUMENT_BYTES: usize = 32 * 1024 * 1024;
const MAX_KEY_BYTES: usize = 256;

/// Namespaces of the single `documents` table.
pub mod ns {
    /// Console preferences: view mode, grid size, launch items, website
    /// lists, Internet lists, daily rule selections, recent websites, ports…
    pub const PREFS: &str = "prefs";
    /// Teacher configuration overlay (`config/overlay`: sections → values).
    pub const CONFIG: &str = "config";
    /// Students seen in this room (`__room` plus one document per Student).
    pub const ROSTER: &str = "roster";
    /// Cached application inventories.
    pub const INVENTORY: &str = "inventory";
    /// Class-wide rules for the current local day.
    pub const RULES: &str = "rules";
    /// Monitoring-badge choices per Student.
    pub const BADGES: &str = "badges";
    /// Screen-recording session journal.
    pub const RECORDINGS: &str = "recordings";
    /// Hardware-control policy.
    pub const HARDWARE: &str = "hardware";
    /// Small console flags (first-run notices and similar).
    pub const UI: &str = "ui";
    /// The Teacher's list of AI/chatbot websites (`ai_sites/list`).
    pub const AI_SITES: &str = "ai_sites";
    /// Last Activity report per Student, kept for the day it was recorded.
    pub const ACTIVITY: &str = "activity";
    /// Per-profile-directory state of every role: launch selection, device
    /// identity, Student settings overlay, badge/activity switches, Guard lease.
    pub const PROFILE: &str = "profile";
    pub(crate) const META: &str = "meta";
    pub const ALL: &[&str] = &[
        PREFS, CONFIG, ROSTER, INVENTORY, RULES, BADGES, RECORDINGS, HARDWARE, UI, ACTIVITY, PROFILE,
    ];
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("document is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Invalid(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("database schema {found} is newer than this build supports ({supported})")]
    TooNew { found: usize, supported: usize },
}
pub type Result<T> = std::result::Result<T, StoreError>;

/// One connection guarded by a mutex; share it as [`Shared`].
pub struct Store {
    conn: Mutex<Connection>,
    path: Option<PathBuf>,
}
pub type Shared = Arc<Store>;
/// `(namespace, key, value)`; `None` deletes.
pub type Change = (String, String, Option<Value>);

/// Ordered, append-only schema steps. `PRAGMA user_version` records how many
/// have been applied, so adding a step here migrates existing databases.
const MIGRATIONS: &[&str] = &["
    CREATE TABLE documents (
        namespace  TEXT    NOT NULL,
        key        TEXT    NOT NULL,
        value      TEXT    NOT NULL,
        updated_at INTEGER NOT NULL,
        PRIMARY KEY (namespace, key)
    ) WITHOUT ROWID;
"];

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

fn check_name(namespace: &str, key: &str) -> Result<()> {
    if namespace.is_empty() || namespace.len() > 32 || key.is_empty() || key.len() > MAX_KEY_BYTES {
        return Err(StoreError::Invalid("invalid namespace or key".into()));
    }
    Ok(())
}

impl Store {
    /// Opens (creating and migrating if needed) the database at `path`. A file
    /// that is not a usable database is moved aside and replaced, never
    /// silently overwritten.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        match Self::try_open(path) {
            Ok(store) => Ok(store),
            // A database written by a newer build is intact: never move it aside
            // (that would discard a user's settings after a rollback).
            Err(first @ StoreError::TooNew { .. }) => Err(first),
            Err(first) => {
                if !path.exists() {
                    return Err(first);
                }
                let stamp = now_ms();
                let aside = path.with_extension(format!("db.corrupt-{stamp}"));
                tracing::error!(error = %first, aside = %aside.display(),
                    "Settings database unusable; moving it aside and starting fresh");
                std::fs::rename(path, &aside)?;
                for suffix in ["-wal", "-shm"] {
                    let side = PathBuf::from(format!("{}{suffix}", path.display()));
                    let _ = std::fs::remove_file(side);
                }
                Self::try_open(path)
            }
        }
    }

    fn try_open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::prepare(&conn, true)?;
        let check: String = conn.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
        if check != "ok" {
            return Err(StoreError::Invalid(format!("integrity check failed: {check}")));
        }
        Ok(Self { conn: Mutex::new(conn), path: Some(path.to_path_buf()) })
    }

    /// A private in-memory database (tests, tools).
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::prepare(&conn, false)?;
        Ok(Self { conn: Mutex::new(conn), path: None })
    }

    fn prepare(conn: &Connection, wal: bool) -> Result<()> {
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        if wal {
            conn.pragma_update(None, "journal_mode", "WAL")?;
        }
        // Settings are written rarely; prefer durability over speed.
        conn.pragma_update(None, "synchronous", "FULL")?;
        let version = usize::try_from(conn.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))?)
            .unwrap_or(usize::MAX);
        if version > MIGRATIONS.len() {
            return Err(StoreError::TooNew { found: version, supported: MIGRATIONS.len() });
        }
        for (index, step) in MIGRATIONS.iter().enumerate().skip(version) {
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(step)?;
            tx.pragma_update(None, "user_version", i64::try_from(index + 1).unwrap_or(i64::MAX))?;
            tx.commit()?;
        }
        Ok(())
    }

    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn get(&self, namespace: &str, key: &str) -> Result<Option<Value>> {
        check_name(namespace, key)?;
        let text: Option<String> = self
            .lock()
            .query_row(
                "SELECT value FROM documents WHERE namespace = ?1 AND key = ?2",
                params![namespace, key],
                |row| row.get(0),
            )
            .optional()?;
        Ok(text.map(|t| serde_json::from_str(&t)).transpose()?)
    }

    pub fn get_as<T: DeserializeOwned>(&self, namespace: &str, key: &str) -> Result<Option<T>> {
        Ok(self.get(namespace, key)?.map(serde_json::from_value).transpose()?)
    }

    pub fn put(&self, namespace: &str, key: &str, value: &Value) -> Result<()> {
        self.apply(namespace, &[(key.to_owned(), Some(value.clone()))])
    }

    pub fn put_as<T: Serialize>(&self, namespace: &str, key: &str, value: &T) -> Result<()> {
        self.put(namespace, key, &serde_json::to_value(value)?)
    }

    pub fn delete(&self, namespace: &str, key: &str) -> Result<()> {
        self.apply(namespace, &[(key.to_owned(), None)])
    }

    /// Every document of a namespace, ordered by key.
    pub fn list(&self, namespace: &str) -> Result<BTreeMap<String, Value>> {
        check_name(namespace, "-")?;
        let conn = self.lock();
        let mut statement =
            conn.prepare("SELECT key, value FROM documents WHERE namespace = ?1 ORDER BY key")?;
        let rows = statement.query_map(params![namespace], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut out = BTreeMap::new();
        for row in rows {
            let (key, text) = row?;
            out.insert(key, serde_json::from_str(&text)?);
        }
        Ok(out)
    }

    /// Writes (`Some`) or removes (`None`) several documents of one namespace
    /// atomically.
    pub fn apply(&self, namespace: &str, changes: &[(String, Option<Value>)]) -> Result<()> {
        let all: Vec<Change> = changes
            .iter()
            .map(|(key, value)| (namespace.to_owned(), key.clone(), value.clone()))
            .collect();
        self.apply_many(&all)
    }

    /// Like [`Store::apply`] across namespaces, in one transaction.
    pub fn apply_many(&self, changes: &[Change]) -> Result<()> {
        for (namespace, key, value) in changes {
            check_name(namespace, key)?;
            if let Some(value) = value {
                if serde_json::to_vec(value)?.len() > MAX_DOCUMENT_BYTES {
                    return Err(StoreError::Invalid(format!("document '{key}' is too large")));
                }
            }
        }
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        for (namespace, key, value) in changes {
            match value {
                Some(value) => {
                    tx.execute(
                        "INSERT INTO documents (namespace, key, value, updated_at)
                         VALUES (?1, ?2, ?3, ?4)
                         ON CONFLICT (namespace, key) DO UPDATE
                         SET value = excluded.value, updated_at = excluded.updated_at",
                        params![namespace, key, serde_json::to_string(value)?, now_ms()],
                    )?;
                }
                None => {
                    tx.execute(
                        "DELETE FROM documents WHERE namespace = ?1 AND key = ?2",
                        params![namespace, key],
                    )?;
                }
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Stores `value` only if the key is absent. Returns whether it was stored.
    pub fn insert_if_absent(&self, namespace: &str, key: &str, value: &Value) -> Result<bool> {
        check_name(namespace, key)?;
        let inserted = self.lock().execute(
            "INSERT OR IGNORE INTO documents (namespace, key, value, updated_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![namespace, key, serde_json::to_string(value)?, now_ms()],
        )?;
        Ok(inserted == 1)
    }

    /// Replaces a whole namespace atomically.
    pub fn replace_namespace(&self, namespace: &str, entries: &BTreeMap<String, Value>) -> Result<()> {
        check_name(namespace, "-")?;
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM documents WHERE namespace = ?1", params![namespace])?;
        for (key, value) in entries {
            check_name(namespace, key)?;
            tx.execute(
                "INSERT INTO documents (namespace, key, value, updated_at) VALUES (?1, ?2, ?3, ?4)",
                params![namespace, key, serde_json::to_string(value)?, now_ms()],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// A handle to one document, for components that persist a single value.
    #[must_use]
    pub fn doc(self: &Arc<Self>, namespace: &str, key: &str) -> Doc {
        Doc { store: Arc::clone(self), namespace: namespace.to_owned(), key: key.to_owned() }
    }

    /// Imports the pre-database files found in `dir` once; see [`legacy`].
    pub fn import_legacy(&self, dir: &Path) -> Result<ImportReport> {
        legacy::import(self, dir)
    }

    pub(crate) fn meta_flag(&self, name: &str) -> Result<bool> {
        Ok(self.get(ns::META, name)?.is_some())
    }
}

/// The database of one profile directory, opened once per process and shared
/// by every module (and test) that asks for the same directory. The first open
/// also imports that directory's legacy files; see [`Store::import_legacy`].
pub fn open_shared(directory: &Path) -> Result<Shared> {
    static REGISTRY: std::sync::OnceLock<Mutex<std::collections::HashMap<PathBuf, Shared>>> =
        std::sync::OnceLock::new();
    let directory = if directory.as_os_str().is_empty() { Path::new(".") } else { directory };
    let mut registry = REGISTRY
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(existing) = registry.get(directory) {
        return Ok(Arc::clone(existing));
    }
    let store = Arc::new(Store::open(&directory.join(FILE_NAME))?);
    store.import_legacy(directory)?;
    registry.insert(directory.to_path_buf(), Arc::clone(&store));
    Ok(store)
}

/// One `(namespace, key)` document of a shared store.
#[derive(Clone)]
pub struct Doc {
    store: Shared,
    namespace: String,
    key: String,
}
impl Doc {
    pub fn read<T: DeserializeOwned>(&self) -> Result<Option<T>> {
        self.store.get_as(&self.namespace, &self.key)
    }
    pub fn write<T: Serialize>(&self, value: &T) -> Result<()> {
        self.store.put_as(&self.namespace, &self.key, value)
    }
    pub fn clear(&self) -> Result<()> {
        self.store.delete(&self.namespace, &self.key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn documents_round_trip_and_delete() {
        let store = Store::open_in_memory().unwrap();
        assert!(store.get(ns::PREFS, "view").unwrap().is_none());
        store.put(ns::PREFS, "view", &json!("grid")).unwrap();
        assert_eq!(store.get(ns::PREFS, "view").unwrap(), Some(json!("grid")));
        store.put(ns::PREFS, "view", &json!("list")).unwrap();
        assert_eq!(store.list(ns::PREFS).unwrap().len(), 1);
        store.delete(ns::PREFS, "view").unwrap();
        assert!(store.list(ns::PREFS).unwrap().is_empty());
    }

    #[test]
    fn apply_is_atomic_and_namespaces_are_isolated() {
        let store = Store::open_in_memory().unwrap();
        store.put(ns::PREFS, "a", &json!(1)).unwrap();
        store
            .apply(
                ns::PREFS,
                &[("b".into(), Some(json!(2))), ("a".into(), None)],
            )
            .unwrap();
        assert_eq!(store.list(ns::PREFS).unwrap().keys().collect::<Vec<_>>(), ["b"]);
        assert!(store.list(ns::RULES).unwrap().is_empty());
        // An invalid key rejects the whole batch.
        let bad = store.apply(
            ns::PREFS,
            &[("c".into(), Some(json!(3))), (String::new(), Some(json!(4)))],
        );
        assert!(bad.is_err());
        assert!(store.get(ns::PREFS, "c").unwrap().is_none());
    }

    #[test]
    fn replace_namespace_swaps_everything() {
        let store = Store::open_in_memory().unwrap();
        store.put(ns::ROSTER, "old", &json!({})).unwrap();
        let mut next = BTreeMap::new();
        next.insert("new".to_owned(), json!({"name": "PC"}));
        store.replace_namespace(ns::ROSTER, &next).unwrap();
        assert_eq!(store.list(ns::ROSTER).unwrap().keys().collect::<Vec<_>>(), ["new"]);
    }

    #[test]
    fn survives_reopen_and_corrupt_file_is_set_aside() {
        let dir = std::env::temp_dir().join(format!("store-{}", now_ms()));
        let path = dir.join(FILE_NAME);
        {
            let store = Store::open(&path).unwrap();
            store.put(ns::PREFS, "x", &json!(7)).unwrap();
        }
        assert_eq!(Store::open(&path).unwrap().get(ns::PREFS, "x").unwrap(), Some(json!(7)));
        std::fs::write(&path, b"this is not a database, just some bytes of text").unwrap();
        let fresh = Store::open(&path).unwrap();
        assert!(fresh.get(ns::PREFS, "x").unwrap().is_none());
        let aside = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(std::result::Result::ok)
            .any(|e| e.file_name().to_string_lossy().contains("corrupt"));
        assert!(aside, "corrupt database must be preserved, not deleted");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn newer_schema_is_refused_not_downgraded() {
        let dir = std::env::temp_dir().join(format!("store-new-{}", now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE_NAME);
        {
            let conn = Connection::open(&path).unwrap();
            conn.pragma_update(None, "user_version", 99_i64).unwrap();
        }
        // Reported, and the intact file is left exactly where it is.
        assert!(matches!(Store::open(&path), Err(StoreError::TooNew { .. })));
        assert!(path.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn insert_if_absent_keeps_the_first_value() {
        let store = Store::open_in_memory().unwrap();
        assert!(store.insert_if_absent(ns::PROFILE, "device_id", &json!("a")).unwrap());
        assert!(!store.insert_if_absent(ns::PROFILE, "device_id", &json!("b")).unwrap());
        assert_eq!(store.get(ns::PROFILE, "device_id").unwrap(), Some(json!("a")));
    }

    #[test]
    fn open_shared_returns_one_store_per_directory() {
        let dir = std::env::temp_dir().join(format!("shared-{}", now_ms()));
        let a = open_shared(&dir).unwrap();
        let b = open_shared(&dir).unwrap();
        assert!(Arc::ptr_eq(&a, &b));
        a.put(ns::PROFILE, "k", &json!(1)).unwrap();
        assert_eq!(b.get(ns::PROFILE, "k").unwrap(), Some(json!(1)));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn doc_handle_reads_and_writes_typed_values() {
        let store: Shared = Arc::new(Store::open_in_memory().unwrap());
        let doc = store.doc(ns::BADGES, "enabled");
        assert!(doc.read::<BTreeMap<String, bool>>().unwrap().is_none());
        let mut value = BTreeMap::new();
        value.insert("a".to_owned(), true);
        doc.write(&value).unwrap();
        assert_eq!(doc.read::<BTreeMap<String, bool>>().unwrap(), Some(value));
        doc.clear().unwrap();
        assert!(doc.read::<BTreeMap<String, bool>>().unwrap().is_none());
    }
}
