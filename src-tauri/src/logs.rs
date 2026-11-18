//! The activity log: kept in memory for the Logs page, mirrored to a rotating
//! file in the app's log folder, and streamed to the UI as it happens.

use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Debug,
    Info,
    Warn,
    Error,
}

impl Level {
    pub fn parse(value: &str) -> Level {
        match value {
            "debug" => Level::Debug,
            "warn" => Level::Warn,
            "error" => Level::Error,
            _ => Level::Info,
        }
    }
    fn as_str(self) -> &'static str {
        match self {
            Level::Debug => "DEBUG",
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub id: u64,
    pub at: i64,
    pub level: Level,
    pub message: String,
    #[serde(default)]
    pub detail: Option<String>,
}

pub const MAX_ENTRIES: usize = 3000;
const MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;

type Sink = Arc<dyn Fn(&LogEntry) + Send + Sync>;

pub struct Logger {
    entries: Mutex<VecDeque<LogEntry>>,
    next_id: AtomicU64,
    min_level: AtomicU8,
    file: Mutex<Option<(PathBuf, File)>>,
    sink: RwLock<Option<Sink>>,
}

impl Default for Logger {
    fn default() -> Self {
        Self {
            entries: Mutex::new(VecDeque::new()),
            next_id: AtomicU64::new(1),
            min_level: AtomicU8::new(Level::Info as u8),
            file: Mutex::new(None),
            sink: RwLock::new(None),
        }
    }
}

impl Logger {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn set_file(&self, path: &Path) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(file) = OpenOptions::new().create(true).append(true).open(path) {
            *self.file.lock() = Some((path.to_path_buf(), file));
        }
    }

    pub fn file_path(&self) -> Option<PathBuf> {
        self.file.lock().as_ref().map(|(p, _)| p.clone())
    }

    pub fn set_sink(&self, sink: Sink) {
        *self.sink.write() = Some(sink);
    }

    pub fn set_level(&self, level: Level) {
        self.min_level.store(level as u8, Ordering::Relaxed);
    }

    pub fn log(&self, level: Level, message: impl Into<String>, detail: Option<String>) {
        if (level as u8) < self.min_level.load(Ordering::Relaxed) {
            return;
        }
        let entry = LogEntry {
            id: self.next_id.fetch_add(1, Ordering::Relaxed),
            at: chrono::Utc::now().timestamp_millis(),
            level,
            message: message.into(),
            detail,
        };
        self.write_file(&entry);
        {
            let mut entries = self.entries.lock();
            entries.push_back(entry.clone());
            while entries.len() > MAX_ENTRIES {
                entries.pop_front();
            }
        }
        if let Some(sink) = self.sink.read().clone() {
            sink(&entry);
        }
    }

    fn write_file(&self, entry: &LogEntry) {
        let mut guard = self.file.lock();
        let Some((path, file)) = guard.as_mut() else {
            return;
        };
        if file
            .metadata()
            .map(|m| m.len() > MAX_FILE_BYTES)
            .unwrap_or(false)
        {
            let old = path.with_extension("1.log");
            let _ = std::fs::remove_file(&old);
            let _ = std::fs::rename(&*path, &old);
            if let Ok(f) = OpenOptions::new().create(true).append(true).open(&*path) {
                *file = f;
            }
        }
        let at = chrono::DateTime::from_timestamp_millis(entry.at)
            .map(|d| {
                d.with_timezone(&chrono::Local)
                    .format("%Y-%m-%d %H:%M:%S%.3f")
                    .to_string()
            })
            .unwrap_or_default();
        let detail = entry
            .detail
            .as_deref()
            .map(|d| format!(" | {d}"))
            .unwrap_or_default();
        let _ = writeln!(
            file,
            "{at} {:<5} {}{detail}",
            entry.level.as_str(),
            entry.message
        );
    }

    pub fn debug(&self, message: impl Into<String>) {
        self.log(Level::Debug, message, None);
    }
    pub fn info(&self, message: impl Into<String>) {
        self.log(Level::Info, message, None);
    }
    pub fn warn(&self, message: impl Into<String>, detail: Option<String>) {
        self.log(Level::Warn, message, detail);
    }
    pub fn error(&self, message: impl Into<String>, detail: Option<String>) {
        self.log(Level::Error, message, detail);
    }

    pub fn entries(&self) -> Vec<LogEntry> {
        self.entries.lock().iter().cloned().collect()
    }

    pub fn clear(&self) {
        self.entries.lock().clear();
    }

    /// Plain-text export of what is in memory.
    pub fn export_text(&self) -> String {
        self.entries
            .lock()
            .iter()
            .map(|e| {
                let at = chrono::DateTime::from_timestamp_millis(e.at)
                    .map(|d| {
                        d.with_timezone(&chrono::Local)
                            .format("%Y-%m-%d %H:%M:%S")
                            .to_string()
                    })
                    .unwrap_or_default();
                let detail = e
                    .detail
                    .as_deref()
                    .map(|d| format!("\n    {d}"))
                    .unwrap_or_default();
                format!("{at} [{}] {}{detail}", e.level.as_str(), e.message)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_buffers_and_streams() {
        let log = Logger::new();
        let seen = Arc::new(Mutex::new(0));
        let s = Arc::clone(&seen);
        log.set_sink(Arc::new(move |_| *s.lock() += 1));
        log.debug("hidden");
        log.info("shown");
        log.set_level(Level::Debug);
        log.debug("now shown");
        log.error("bad", Some("why".into()));
        assert_eq!(log.entries().len(), 3);
        assert_eq!(*seen.lock(), 3);
        assert!(log.export_text().contains("[ERROR] bad\n    why"));
        log.clear();
        assert!(log.entries().is_empty());
        assert_eq!(Level::parse("warn"), Level::Warn);
    }

    #[test]
    fn writes_a_log_file() {
        let dir = tempfile::tempdir().unwrap();
        let log = Logger::new();
        log.set_file(&dir.path().join("logs/app.log"));
        log.info("hello file");
        let text = std::fs::read_to_string(dir.path().join("logs/app.log")).unwrap();
        assert!(text.contains("INFO  hello file"));
        assert!(log.file_path().is_some());
    }
}
