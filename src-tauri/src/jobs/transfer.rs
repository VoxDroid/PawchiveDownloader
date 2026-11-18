//! Moves bytes from Pawchive to disk. Transfers write to a `.part` file next to
//! the target and resume it with an HTTP Range request after a pause, a dropped
//! connection or a restart of the app. Finished files are checked against the
//! sha256 in their storage path before they get their real name.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use crate::net::limiter::is_transient_status;
use crate::net::throttle::Throttle;
use crate::pawchive::client::{describe, PawchiveClient};
use crate::settings::ConflictAction;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum FetchError {
    #[error("cancelled")]
    Cancelled,
    /// The site asked us to slow down; does not count as an attempt.
    #[error("rate limited")]
    RateLimited(Duration),
    #[error("HTTP {0}")]
    Status(u16),
    #[error("{0}")]
    Network(String),
    #[error("{0}")]
    Disk(String),
    #[error("the file does not match its checksum")]
    Checksum,
    /// A bug in the app; never retried, always reported.
    #[error("{0}")]
    Internal(String),
    /// No connection at all (DNS, refused, unreachable): wait for the
    /// network without using up the item's attempts.
    #[error("no connection")]
    Offline,
    /// The server answered with a web page instead of the file (an error
    /// page, a login wall or a captive portal).
    #[error("got a web page instead of the file")]
    NotAFile,
}

impl FetchError {
    /// Whether another attempt may succeed.
    pub fn retryable(&self) -> bool {
        match self {
            FetchError::Cancelled | FetchError::Disk(_) | FetchError::Internal(_) => false,
            FetchError::Status(s) => is_transient_status(*s),
            _ => true,
        }
    }

    /// Whether the full file is simply not on the server (thumbnail fallback).
    pub fn missing(&self) -> bool {
        matches!(self, FetchError::Status(403 | 404 | 410))
    }
}

/// Live byte counters shared between a transfer and the engine.
#[derive(Debug, Default)]
pub struct Progress {
    pub bytes: AtomicU64,
    pub total: AtomicU64,
    /// Bytes of earlier pieces (archive entries already fetched).
    pub base: AtomicU64,
    /// All pieces are fetched and the archive is being written.
    pub building: std::sync::atomic::AtomicBool,
}

impl Progress {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }
    /// (bytes, total) including earlier pieces.
    pub fn get(&self) -> (u64, u64) {
        let base = self.base.load(Ordering::Relaxed);
        (
            base + self.bytes.load(Ordering::Relaxed),
            base + self.total.load(Ordering::Relaxed),
        )
    }
}

pub fn part_path(target: &Path) -> PathBuf {
    let mut name = target
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(".part");
    target.with_file_name(name)
}

/// `name.ext`, `name (1).ext`, `name (2).ext`, ...
pub fn numbered(path: &Path, n: u32) -> PathBuf {
    if n == 0 {
        return path.to_path_buf();
    }
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = match path.extension() {
        Some(ext) => format!("{stem} ({n}).{}", ext.to_string_lossy()),
        None => format!("{stem} ({n})"),
    };
    path.with_file_name(name)
}

/// Decides which file an item writes to. `taken` reports paths another
/// transfer of this app already claimed. None means skip (it exists).
pub fn choose_target(
    wanted: &Path,
    conflict: ConflictAction,
    taken: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    let busy = |p: &Path| p.exists() || part_path(p).exists() || taken(p);
    if !busy(wanted) {
        return Some(wanted.to_path_buf());
    }
    match conflict {
        // Replacing a file on disk is fine, but never one another running
        // transfer of this app is writing: that would mix two files.
        ConflictAction::Overwrite if !taken(wanted) => Some(wanted.to_path_buf()),
        ConflictAction::Overwrite => (1..10_000).map(|n| numbered(wanted, n)).find(|p| !busy(p)),
        ConflictAction::Skip => None,
        ConflictAction::Uniquify => (1..10_000).map(|n| numbered(wanted, n)).find(|p| !busy(p)),
    }
}

pub async fn sha256_file(path: PathBuf) -> std::io::Result<String> {
    tokio::task::spawn_blocking(move || {
        let mut file = std::fs::File::open(&path)?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 1 << 20];
        loop {
            let n = file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        Ok(hex::encode(hasher.finalize()))
    })
    .await
    .map_err(std::io::Error::other)?
}

fn disk(e: std::io::Error) -> FetchError {
    FetchError::Disk(match e.kind() {
        std::io::ErrorKind::PermissionDenied => "permission denied".into(),
        std::io::ErrorKind::StorageFull => "the disk is full".into(),
        _ => e.to_string(),
    })
}

fn content_range_start(value: &str) -> Option<u64> {
    // "bytes 100-199/200"
    value
        .strip_prefix("bytes ")?
        .split('-')
        .next()?
        .trim()
        .parse()
        .ok()
}

fn content_range_total(value: &str) -> Option<u64> {
    value.rsplit('/').next()?.trim().parse().ok()
}

pub struct Fetcher {
    pub client: Arc<PawchiveClient>,
    pub throttle: Arc<Throttle>,
}

impl Fetcher {
    /// Downloads `url` into `part`, resuming what is already there. Returns
    /// the file's size once complete.
    pub async fn fetch(
        &self,
        url: &str,
        part: &Path,
        // The file's real name, to tell an HTML file from an error page.
        name: &str,
        progress: &Progress,
        cancel: &CancellationToken,
    ) -> Result<u64, FetchError> {
        if let Some(dir) = part.parent() {
            tokio::fs::create_dir_all(dir).await.map_err(disk)?;
        }
        let mut existing = tokio::fs::metadata(part)
            .await
            .map(|m| m.len())
            .unwrap_or(0);
        let mut request = self
            .client
            .http()
            .get(self.client.target(url))
            .header("Accept", "*/*")
            // Byte ranges must refer to the file itself, never a compressed copy.
            .header("Accept-Encoding", "identity");
        if existing > 0 {
            request = request.header("Range", format!("bytes={existing}-"));
        }
        let response = tokio::select! {
            r = request.send() => r.map_err(|e| {
                if e.is_connect() { FetchError::Offline } else { FetchError::Network(describe(&e)) }
            })?,
            _ = cancel.cancelled() => return Err(FetchError::Cancelled),
        };
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let header = |name: &str| {
            headers
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        };
        if let Some(cooldown) =
            self.client
                .limiter
                .note(url, status, header("retry-after").as_deref())
        {
            return Err(FetchError::RateLimited(cooldown));
        }
        // Media servers never send HTML for a file; when one does it is an
        // error page or a login wall, which must not be saved as the file.
        let html =
            header("content-type").is_some_and(|t| t.to_ascii_lowercase().starts_with("text/html"));
        let lower = name.to_ascii_lowercase();
        let wants_html = lower.ends_with(".html") || lower.ends_with(".htm");
        if html && !wants_html && (200..300).contains(&status) {
            return Err(FetchError::NotAFile);
        }
        let mut append = false;
        match status {
            206 => {
                let start = header("content-range")
                    .as_deref()
                    .and_then(content_range_start);
                if start == Some(existing) {
                    append = true;
                } else {
                    // The server sent a different piece than asked: start over.
                    return self.restart(part).await;
                }
            }
            200 => existing = 0,
            416 => {
                let total = header("content-range")
                    .as_deref()
                    .and_then(content_range_total);
                if existing > 0 && total == Some(existing) {
                    progress.total.store(existing, Ordering::Relaxed);
                    progress.bytes.store(existing, Ordering::Relaxed);
                    return Ok(existing);
                }
                return self.restart(part).await;
            }
            s => return Err(FetchError::Status(s)),
        }
        let length = response.content_length();
        let total = length.map(|l| l + existing).unwrap_or(0);
        progress.total.store(total, Ordering::Relaxed);
        progress.bytes.store(existing, Ordering::Relaxed);

        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(append)
            .truncate(!append)
            .open(part)
            .await
            .map_err(disk)?;
        let mut written = existing;
        let mut stream = response.bytes_stream();
        loop {
            let next = tokio::select! {
                chunk = stream.next() => chunk,
                _ = cancel.cancelled() => {
                    let _ = file.flush().await;
                    return Err(FetchError::Cancelled);
                }
            };
            let Some(chunk) = next else { break };
            let chunk = match chunk {
                Ok(c) => c,
                Err(e) => {
                    let _ = file.flush().await;
                    return Err(FetchError::Network(describe(&e)));
                }
            };
            file.write_all(&chunk).await.map_err(disk)?;
            written += chunk.len() as u64;
            progress.bytes.store(written, Ordering::Relaxed);
            tokio::select! {
                _ = self.throttle.consume(chunk.len()) => {},
                _ = cancel.cancelled() => {
                    let _ = file.flush().await;
                    return Err(FetchError::Cancelled);
                }
            }
        }
        file.flush().await.map_err(disk)?;
        file.sync_all().await.ok();
        drop(file);
        if total > 0 && written < total {
            // Keep the part: the next attempt resumes from here.
            return Err(FetchError::Network("connection dropped".into()));
        }
        progress.total.store(written, Ordering::Relaxed);
        Ok(written)
    }

    async fn restart(&self, part: &Path) -> Result<u64, FetchError> {
        let _ = tokio::fs::remove_file(part).await;
        Err(FetchError::Network("resume refused, starting over".into()))
    }
}

/// Checks a finished part file against the expected sha256.
pub async fn verify(part: &Path, expected: Option<&str>) -> Result<(), FetchError> {
    let Some(expected) = expected else {
        return Ok(());
    };
    let actual = sha256_file(part.to_path_buf()).await.map_err(disk)?;
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        let _ = tokio::fs::remove_file(part).await;
        Err(FetchError::Checksum)
    }
}

/// Gives a finished part file its real name.
pub async fn finish(part: &Path, target: &Path) -> Result<(), FetchError> {
    tokio::fs::rename(part, target).await.map_err(disk)
}

/// Writes a small text file in one go (via a part file, so it is never half written).
pub async fn write_text(target: &Path, text: &str) -> Result<u64, FetchError> {
    if let Some(dir) = target.parent() {
        tokio::fs::create_dir_all(dir).await.map_err(disk)?;
    }
    let part = part_path(target);
    tokio::fs::write(&part, text.as_bytes())
        .await
        .map_err(disk)?;
    finish(&part, target).await?;
    Ok(text.len() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_conflicting_names() {
        assert_eq!(
            numbered(Path::new("/a/b.png"), 2),
            PathBuf::from("/a/b (2).png")
        );
        assert_eq!(numbered(Path::new("/a/b"), 1), PathBuf::from("/a/b (1)"));
        assert_eq!(
            part_path(Path::new("/a/b.png")),
            PathBuf::from("/a/b.png.part")
        );
    }

    #[test]
    fn chooses_targets() {
        let dir = tempfile::tempdir().unwrap();
        let wanted = dir.path().join("x.txt");
        assert_eq!(
            choose_target(&wanted, ConflictAction::Skip, |_| false),
            Some(wanted.clone())
        );
        std::fs::write(&wanted, "1").unwrap();
        assert_eq!(
            choose_target(&wanted, ConflictAction::Skip, |_| false),
            None
        );
        assert_eq!(
            choose_target(&wanted, ConflictAction::Overwrite, |_| false),
            Some(wanted.clone())
        );
        let first = dir.path().join("x (1).txt");
        assert_eq!(
            choose_target(&wanted, ConflictAction::Uniquify, |_| false),
            Some(first.clone())
        );
        let taken = first.clone();
        assert_eq!(
            choose_target(&wanted, ConflictAction::Uniquify, move |p| p == taken),
            Some(dir.path().join("x (2).txt"))
        );
    }

    #[test]
    fn classifies_errors() {
        assert!(FetchError::Status(503).retryable());
        assert!(!FetchError::Status(404).retryable());
        assert!(FetchError::Status(404).missing());
        assert!(FetchError::Network("x".into()).retryable());
        assert!(!FetchError::Disk("x".into()).retryable());
        assert!(FetchError::Checksum.retryable());
        assert_eq!(content_range_start("bytes 100-199/200"), Some(100));
        assert_eq!(content_range_total("bytes */200"), Some(200));
    }

    #[tokio::test]
    async fn an_unreachable_server_is_offline_not_a_failure() {
        let client = PawchiveClient::new(
            reqwest::Client::new(),
            crate::net::limiter::SiteLimiter::new(5, Duration::ZERO),
        );
        let fetcher = Fetcher {
            client: Arc::new(client),
            throttle: Arc::new(Throttle::new(0)),
        };
        let dir = tempfile::tempdir().unwrap();
        let result = fetcher
            .fetch(
                "http://127.0.0.1:9/file.bin",
                &dir.path().join("f.part"),
                "file.bin",
                &Progress::default(),
                &CancellationToken::new(),
            )
            .await;
        assert_eq!(result, Err(FetchError::Offline));
        assert!(FetchError::Offline.retryable());
        assert!(FetchError::NotAFile.retryable());
        assert!(!FetchError::Internal("x".into()).retryable());
    }

    #[tokio::test]
    async fn hashes_and_writes_files() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("sub/t.txt");
        assert_eq!(write_text(&target, "abc").await.unwrap(), 3);
        let hash = sha256_file(target.clone()).await.unwrap();
        assert_eq!(
            hash,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(verify(&target, Some(&hash.to_uppercase())).await.is_ok());
        assert_eq!(verify(&target, Some("00")).await, Err(FetchError::Checksum));
        assert!(!target.exists());
    }
}
