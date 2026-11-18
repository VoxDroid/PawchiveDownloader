//! Everything the app remembers lives in one SQLite database (`library.db`):
//! settings, the job queue, the history, the hashes of downloaded files (to
//! skip duplicates), the watchlist and favorites.

pub mod records;

use std::collections::HashSet;
use std::path::Path;

use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};

use crate::jobs::model::Job;
use records::{Favorite, HistoryEntry, WatchEntry};

pub struct Store {
    conn: Mutex<Connection>,
}

#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    pub files: u64,
    pub posts: u64,
    pub history: u64,
    pub watch: u64,
    pub favorites: u64,
}

pub type Result<T> = std::result::Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS kv (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS jobs (id TEXT PRIMARY KEY, position INTEGER NOT NULL, data TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS files (hash TEXT PRIMARY KEY, post TEXT NOT NULL, path TEXT NOT NULL, at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS posts (key TEXT PRIMARY KEY, title TEXT NOT NULL, at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS history (id TEXT PRIMARY KEY, finished_at INTEGER NOT NULL, data TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS history_finished ON history (finished_at);
CREATE TABLE IF NOT EXISTS watch (key TEXT PRIMARY KEY, position INTEGER NOT NULL, data TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS favorites (key TEXT PRIMARY KEY, added_at INTEGER NOT NULL, data TEXT NOT NULL);
";

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(err)?;
        }
        let conn = Connection::open(path).map_err(err)?;
        Self::init(conn)
    }

    pub fn in_memory() -> Self {
        Self::init(Connection::open_in_memory().unwrap()).unwrap()
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "journal_mode", "WAL").ok();
        conn.pragma_update(None, "synchronous", "NORMAL")
            .map_err(err)?;
        conn.execute_batch(SCHEMA).map_err(err)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    // ------------------------------------------------------------ key/value

    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        let conn = self.conn.lock();
        let text: Option<String> = conn
            .query_row("SELECT value FROM kv WHERE key = ?1", [key], |r| r.get(0))
            .optional()
            .ok()
            .flatten();
        text.and_then(|t| serde_json::from_str(&t).ok())
    }

    pub fn get_raw(&self, key: &str) -> Option<serde_json::Value> {
        self.get::<serde_json::Value>(key)
    }

    pub fn set<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        let text = serde_json::to_string(value).map_err(err)?;
        self.conn
            .lock()
            .execute("INSERT INTO kv (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value", params![key, text])
            .map(|_| ())
            .map_err(err)
    }

    // ----------------------------------------------------------------- jobs

    pub fn load_jobs(&self) -> Vec<Job> {
        let conn = self.conn.lock();
        let Ok(mut stmt) = conn.prepare("SELECT data FROM jobs ORDER BY position") else {
            return vec![];
        };
        let rows = stmt.query_map([], |r| r.get::<_, String>(0));
        rows.map(|rows| {
            rows.filter_map(|r| r.ok())
                .filter_map(|t| serde_json::from_str(&t).ok())
                .collect()
        })
        .unwrap_or_default()
    }

    /// Replaces the saved queue in one transaction.
    pub fn save_jobs(&self, jobs: &[Job]) -> Result<()> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction().map_err(err)?;
        tx.execute("DELETE FROM jobs", []).map_err(err)?;
        for (i, job) in jobs.iter().enumerate() {
            let data = serde_json::to_string(job).map_err(err)?;
            tx.execute(
                "INSERT INTO jobs (id, position, data) VALUES (?1, ?2, ?3)",
                params![job.id, i as i64, data],
            )
            .map_err(err)?;
        }
        tx.commit().map_err(err)
    }

    /// Writes only the jobs that changed, keeps the order and drops the
    /// jobs no longer in the queue, in one transaction.
    pub fn sync_jobs(&self, order: &[String], changed: &[(String, String)]) -> Result<()> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction().map_err(err)?;
        {
            let mut upsert = tx
                .prepare_cached(
                    "INSERT INTO jobs (id, position, data) VALUES (?1, 0, ?2) ON CONFLICT(id) DO UPDATE SET data = excluded.data",
                )
                .map_err(err)?;
            for (id, json) in changed {
                upsert.execute(params![id, json]).map_err(err)?;
            }
            let mut position = tx
                .prepare_cached("UPDATE jobs SET position = ?2 WHERE id = ?1")
                .map_err(err)?;
            for (i, id) in order.iter().enumerate() {
                position.execute(params![id, i as i64]).map_err(err)?;
            }
            let existing: Vec<String> = {
                let mut stmt = tx.prepare("SELECT id FROM jobs").map_err(err)?;
                let rows = stmt.query_map([], |r| r.get::<_, String>(0)).map_err(err)?;
                rows.filter_map(|r| r.ok()).collect()
            };
            let keep: HashSet<&String> = order.iter().collect();
            for id in existing.iter().filter(|id| !keep.contains(id)) {
                tx.execute("DELETE FROM jobs WHERE id = ?1", [id])
                    .map_err(err)?;
            }
        }
        tx.commit().map_err(err)
    }

    // ------------------------------------------------------- known files

    pub fn known_hashes(&self, hashes: &[String]) -> HashSet<String> {
        let conn = self.conn.lock();
        let mut found = HashSet::new();
        let Ok(mut stmt) = conn.prepare_cached("SELECT 1 FROM files WHERE hash = ?1") else {
            return found;
        };
        for hash in hashes {
            if stmt.exists([hash]).unwrap_or(false) {
                found.insert(hash.clone());
            }
        }
        found
    }

    pub fn known_posts(&self, keys: &[String]) -> HashSet<String> {
        let conn = self.conn.lock();
        let mut found = HashSet::new();
        let Ok(mut stmt) = conn.prepare_cached("SELECT 1 FROM posts WHERE key = ?1") else {
            return found;
        };
        for key in keys {
            if stmt.exists([key]).unwrap_or(false) {
                found.insert(key.clone());
            }
        }
        found
    }

    pub fn record_file(&self, hash: &str, post: &str, path: &str, at: i64) -> Result<()> {
        self.conn
            .lock()
            .execute(
                "INSERT INTO files (hash, post, path, at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(hash) DO UPDATE SET post = excluded.post, path = excluded.path, at = excluded.at",
                params![hash, post, path, at],
            )
            .map(|_| ())
            .map_err(err)
    }

    pub fn record_post(&self, key: &str, title: &str, at: i64) -> Result<()> {
        self.conn
            .lock()
            .execute(
                "INSERT INTO posts (key, title, at) VALUES (?1, ?2, ?3) ON CONFLICT(key) DO UPDATE SET title = excluded.title, at = excluded.at",
                params![key, title, at],
            )
            .map(|_| ())
            .map_err(err)
    }

    pub fn forget_known(&self) -> Result<()> {
        self.conn
            .lock()
            .execute_batch("DELETE FROM files; DELETE FROM posts;")
            .map_err(err)
    }

    // -------------------------------------------------------------- history

    pub fn add_history(&self, entry: &HistoryEntry) -> Result<()> {
        let data = serde_json::to_string(entry).map_err(err)?;
        self.conn
            .lock()
            .execute(
                "INSERT INTO history (id, finished_at, data) VALUES (?1, ?2, ?3) ON CONFLICT(id) DO UPDATE SET finished_at = excluded.finished_at, data = excluded.data",
                params![entry.id, entry.finished_at, data],
            )
            .map(|_| ())
            .map_err(err)
    }

    pub fn list_history(&self) -> Vec<HistoryEntry> {
        self.list_json("SELECT data FROM history ORDER BY finished_at DESC")
    }

    pub fn get_history(&self, id: &str) -> Option<HistoryEntry> {
        let conn = self.conn.lock();
        conn.query_row("SELECT data FROM history WHERE id = ?1", [id], |r| {
            r.get::<_, String>(0)
        })
        .optional()
        .ok()
        .flatten()
        .and_then(|t| serde_json::from_str(&t).ok())
    }

    pub fn remove_history(&self, ids: &[String]) -> Result<()> {
        let conn = self.conn.lock();
        for id in ids {
            conn.execute("DELETE FROM history WHERE id = ?1", [id])
                .map_err(err)?;
        }
        Ok(())
    }

    pub fn clear_history(&self, also_known_files: bool) -> Result<()> {
        self.conn
            .lock()
            .execute("DELETE FROM history", [])
            .map_err(err)?;
        if also_known_files {
            self.forget_known()?;
        }
        Ok(())
    }

    // ------------------------------------------------------------ watchlist

    pub fn list_watch(&self) -> Vec<WatchEntry> {
        self.list_json("SELECT data FROM watch ORDER BY position")
    }

    pub fn save_watch(&self, entries: &[WatchEntry]) -> Result<()> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction().map_err(err)?;
        tx.execute("DELETE FROM watch", []).map_err(err)?;
        for (i, e) in entries.iter().enumerate() {
            let data = serde_json::to_string(e).map_err(err)?;
            tx.execute(
                "INSERT INTO watch (key, position, data) VALUES (?1, ?2, ?3)",
                params![e.key, i as i64, data],
            )
            .map_err(err)?;
        }
        tx.commit().map_err(err)
    }

    // ------------------------------------------------------------ favorites

    pub fn list_favorites(&self) -> Vec<Favorite> {
        self.list_json("SELECT data FROM favorites ORDER BY added_at DESC")
    }

    pub fn put_favorite(&self, fav: &Favorite) -> Result<()> {
        let data = serde_json::to_string(fav).map_err(err)?;
        self.conn
            .lock()
            .execute(
                "INSERT INTO favorites (key, added_at, data) VALUES (?1, ?2, ?3) ON CONFLICT(key) DO UPDATE SET data = excluded.data",
                params![fav.key, fav.added_at, data],
            )
            .map(|_| ())
            .map_err(err)
    }

    pub fn remove_favorites(&self, keys: &[String]) -> Result<()> {
        let conn = self.conn.lock();
        for key in keys {
            conn.execute("DELETE FROM favorites WHERE key = ?1", [key])
                .map_err(err)?;
        }
        Ok(())
    }

    // --------------------------------------------------------------- misc

    pub fn counts(&self) -> Counts {
        let conn = self.conn.lock();
        let count = |table: &str| -> u64 {
            conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap_or(0) as u64
        };
        Counts {
            files: count("files"),
            posts: count("posts"),
            history: count("history"),
            watch: count("watch"),
            favorites: count("favorites"),
        }
    }

    fn list_json<T: DeserializeOwned>(&self, sql: &str) -> Vec<T> {
        let conn = self.conn.lock();
        let Ok(mut stmt) = conn.prepare(sql) else {
            return vec![];
        };
        stmt.query_map([], |r| r.get::<_, String>(0))
            .map(|rows| {
                rows.filter_map(|r| r.ok())
                    .filter_map(|t| serde_json::from_str(&t).ok())
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::records::*;
    use super::*;

    #[test]
    fn remembers_files_posts_and_kv() {
        let store = Store::in_memory();
        store.record_file("h1", "p", "a/b.png", 1).unwrap();
        store.record_post("s:c:p", "Title", 1).unwrap();
        assert_eq!(
            store.known_hashes(&["h1".into(), "h2".into()]),
            ["h1".to_string()].into()
        );
        assert_eq!(store.known_posts(&["s:c:p".into()]).len(), 1);
        store.set("answer", &42).unwrap();
        assert_eq!(store.get::<i32>("answer"), Some(42));
        assert_eq!(store.get::<i32>("missing"), None);
        assert_eq!(store.counts().files, 1);
        store.forget_known().unwrap();
        assert!(store.known_hashes(&["h1".into()]).is_empty());
    }

    #[test]
    fn keeps_history_watch_and_favorites() {
        let store = Store::in_memory();
        let mut e = HistoryEntry::sample("a", 10);
        store.add_history(&e).unwrap();
        e.id = "b".into();
        e.finished_at = 20;
        store.add_history(&e).unwrap();
        let list = store.list_history();
        assert_eq!(
            list.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            ["b", "a"]
        );
        assert!(store.get_history("a").is_some());
        store.remove_history(&["a".into()]).unwrap();
        assert_eq!(store.list_history().len(), 1);
        store.clear_history(false).unwrap();
        assert!(store.list_history().is_empty());

        let w = WatchEntry::new("patreon", "1", "Cat", 0);
        store.save_watch(std::slice::from_ref(&w)).unwrap();
        assert_eq!(store.list_watch(), vec![w]);

        let f = Favorite::creator("patreon", "1", "Cat", 5);
        store.put_favorite(&f).unwrap();
        assert_eq!(store.list_favorites().len(), 1);
        store
            .remove_favorites(std::slice::from_ref(&f.key))
            .unwrap();
        assert_eq!(store.counts().favorites, 0);
    }
}
