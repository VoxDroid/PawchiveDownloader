//! The watchlist: followed creators are checked for new posts on a schedule.
//! New posts raise a notification and can be queued automatically.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::{Mutex, RwLock};
use serde::Serialize;
use tokio_util::sync::CancellationToken;

use crate::jobs::engine::{now_ms, Engine};
use crate::jobs::model::{CreatorInfo, JobOverrides, JobRequest, PostRef};
use crate::logs::Logger;
use crate::pawchive::client::{ApiError, PawchiveClient};
use crate::settings::Settings;
use crate::store::records::{total_new, NewPost, WatchEntry};
use crate::store::Store;

/// Shows a notification: (count of new posts, creator names).
pub type Notify = Arc<dyn Fn(usize, String) + Send + Sync>;
pub type OnChange = Arc<dyn Fn(usize) + Send + Sync>;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckReport {
    pub checked: u32,
    pub found: u32,
    pub failed: u32,
}

pub struct Watcher {
    store: Arc<Store>,
    client: Arc<PawchiveClient>,
    engine: Arc<Engine>,
    settings: Arc<RwLock<Settings>>,
    log: Arc<Logger>,
    notify: RwLock<Option<Notify>>,
    on_change: RwLock<Option<OnChange>>,
    running: tokio::sync::Mutex<()>,
    /// Serialises edits so a check cannot overwrite a concurrent change.
    edit: Mutex<()>,
    pub checking: AtomicBool,
}

impl Watcher {
    pub fn new(
        store: Arc<Store>,
        client: Arc<PawchiveClient>,
        engine: Arc<Engine>,
        settings: Arc<RwLock<Settings>>,
        log: Arc<Logger>,
    ) -> Arc<Self> {
        Arc::new(Self {
            store,
            client,
            engine,
            settings,
            log,
            notify: RwLock::new(None),
            on_change: RwLock::new(None),
            running: tokio::sync::Mutex::new(()),
            edit: Mutex::new(()),
            checking: AtomicBool::new(false),
        })
    }

    pub fn set_notify(&self, f: Notify) {
        *self.notify.write() = Some(f);
    }

    pub fn set_on_change(&self, f: OnChange) {
        *self.on_change.write() = Some(f);
    }

    pub fn new_count(&self) -> usize {
        total_new(&self.store.list_watch())
    }

    fn changed(&self) {
        let count = self.new_count();
        if let Some(f) = self.on_change.read().clone() {
            f(count);
        }
    }

    pub fn list(&self) -> Vec<WatchEntry> {
        self.store.list_watch()
    }

    /// Changes the watchlist under the edit lock.
    pub fn update<R>(&self, f: impl FnOnce(&mut Vec<WatchEntry>) -> R) -> Result<R, String> {
        let result = {
            let _guard = self.edit.lock();
            let mut entries = self.store.list_watch();
            let result = f(&mut entries);
            self.store.save_watch(&entries)?;
            result
        };
        self.changed();
        Ok(result)
    }

    pub fn add(&self, service: &str, creator_id: &str, name: &str) -> Result<bool, String> {
        self.update(|entries| {
            if entries
                .iter()
                .any(|e| e.service == service && e.creator_id == creator_id)
            {
                return false;
            }
            entries.push(WatchEntry::new(service, creator_id, name, now_ms()));
            true
        })
    }

    /// Checks the given creators (all when None).
    pub async fn check(
        &self,
        keys: Option<Vec<String>>,
        cancel: &CancellationToken,
    ) -> CheckReport {
        let _running = self.running.lock().await;
        self.checking.store(true, Ordering::Relaxed);
        self.changed();
        let targets: Vec<WatchEntry> = self
            .store
            .list_watch()
            .into_iter()
            .filter(|e| keys.as_ref().is_none_or(|k| k.contains(&e.key)))
            .collect();
        let mut report = CheckReport::default();
        let mut found_all: Vec<(WatchEntry, Vec<NewPost>)> = Vec::new();
        for target in targets {
            if cancel.is_cancelled() {
                break;
            }
            let page = self
                .client
                .get_creator_page(&target.service, &target.creator_id, 0, None, cancel)
                .await;
            if matches!(page, Err(ApiError::Cancelled)) {
                break;
            }
            report.checked += 1;
            let outcome = self.update(|entries| {
                let entry = entries.iter_mut().find(|e| e.key == target.key)?;
                Some(match &page {
                    Ok(posts) => Ok((entry.clone(), entry.apply_check(posts, now_ms()))),
                    Err(e) => {
                        entry.error = Some(e.to_string());
                        entry.last_checked_at = Some(now_ms());
                        Err(e.to_string())
                    }
                })
            });
            match outcome {
                Ok(Some(Ok((entry, found)))) if !found.is_empty() => {
                    report.found += found.len() as u32;
                    found_all.push((entry, found));
                }
                Ok(Some(Err(e))) => {
                    report.failed += 1;
                    self.log
                        .warn(format!("Could not check {}", target.name), Some(e));
                }
                _ => {}
            }
        }
        for (entry, found) in &found_all {
            self.log
                .info(format!("{} new posts from {}", found.len(), entry.name));
            if entry.auto_download {
                let ids: Vec<String> = found.iter().map(|p| p.id.clone()).collect();
                self.queue(entry, ids.clone());
                let _ = self.update(|entries| {
                    if let Some(e) = entries.iter_mut().find(|e| e.key == entry.key) {
                        e.mark_seen(Some(&ids));
                    }
                });
            }
        }
        if !found_all.is_empty() && self.settings.read().notifications == "all" {
            if let Some(notify) = self.notify.read().clone() {
                let total: usize = found_all.iter().map(|(_, f)| f.len()).sum();
                let names: Vec<String> = found_all
                    .iter()
                    .map(|(e, _)| e.name.clone())
                    .take(3)
                    .collect();
                notify(total, names.join(", "));
            }
        }
        let _ = self.store.set("watch.lastRun", &now_ms());
        self.checking.store(false, Ordering::Relaxed);
        self.changed();
        report
    }

    /// Queues posts of a watched creator for download.
    pub fn queue(&self, entry: &WatchEntry, post_ids: Vec<String>) -> Option<String> {
        if post_ids.is_empty() {
            return None;
        }
        let count = post_ids.len();
        let refs: Vec<PostRef> = post_ids
            .into_iter()
            .map(|post_id| PostRef {
                service: entry.service.clone(),
                creator_id: entry.creator_id.clone(),
                post_id,
            })
            .collect();
        Some(self.engine.create(JobRequest::Refs {
            title: Some(format!("{} ({count} new)", entry.name)),
            creator: Some(CreatorInfo {
                service: entry.service.clone(),
                id: entry.creator_id.clone(),
                name: entry.name.clone(),
            }),
            refs,
            options: JobOverrides {
                download_dir: entry.options.download_dir.clone(),
                mode: entry.options.mode,
                apply_post_filter: entry.options.filter.as_ref().map(|_| true),
                filter: entry.options.filter.clone(),
                ..Default::default()
            },
        }))
    }

    /// Checks on the schedule set in Settings until shutdown.
    pub async fn run(self: Arc<Self>, cancel: CancellationToken) {
        // Give the app a moment to settle before the first check.
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(20)) => {},
            _ = cancel.cancelled() => return,
        }
        loop {
            let minutes = self.settings.read().watch_interval_minutes;
            if minutes > 0 && !self.store.list_watch().is_empty() {
                let last: i64 = self.store.get("watch.lastRun").unwrap_or(0);
                if now_ms() - last >= i64::from(minutes) * 60_000 {
                    let report = self.check(None, &cancel).await;
                    self.log.debug(format!(
                        "Watchlist check: {} checked, {} new",
                        report.checked, report.found
                    ));
                }
            }
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(60)) => {},
                _ = cancel.cancelled() => return,
            }
        }
    }
}
