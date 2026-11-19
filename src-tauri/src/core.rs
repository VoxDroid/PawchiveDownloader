//! Everything the app runs on, wired together without Tauri, so integration
//! tests can drive the real engine against a mock server.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::RwLock;
use tokio_util::sync::CancellationToken;

use crate::jobs::engine::{now_ms, Engine, Hold};
use crate::logs::{Level, Logger};
use crate::net::http::build_client;
use crate::net::limiter::{RateLimitPolicy, SiteLimiter};
use crate::net::throttle::Throttle;
use crate::pawchive::client::PawchiveClient;
use crate::pawchive::directory::Directory;
use crate::power::{self, KeepAwake, WhenDone};
use crate::settings::{self, Settings, MAX_CONCURRENCY};
use crate::store::Store;
use crate::watch::Watcher;

pub struct Core {
    pub settings: Arc<RwLock<Settings>>,
    pub store: Arc<Store>,
    pub log: Arc<Logger>,
    pub client: Arc<PawchiveClient>,
    pub throttle: Arc<Throttle>,
    pub engine: Arc<Engine>,
    pub watcher: Arc<Watcher>,
    /// Creator lists for search, one per site.
    directories: Vec<Arc<Directory>>,
    pub data_dir: PathBuf,
    pub cancel: CancellationToken,
    fast_timing: bool,
    pub awake: KeepAwake,
    pub when_done: parking_lot::Mutex<WhenDone>,
    /// When the when-done action fires (epoch ms), and how to call it off.
    countdown: parking_lot::Mutex<Option<(i64, CancellationToken)>>,
    /// Told whenever the queue status changes (the app emits it).
    on_queue: RwLock<Option<OnQueue>>,
    /// Quits the app (for the when-done action).
    on_quit: RwLock<Option<OnQuit>>,
}

pub type OnQueue = Arc<dyn Fn(QueueStatus) + Send + Sync>;
pub type OnQuit = Arc<dyn Fn() + Send + Sync>;

/// What the queue is waiting for, and what happens when it is done.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueStatus {
    pub hold: Option<Hold>,
    pub when_done: WhenDone,
    /// When the when-done action fires (epoch ms), during its countdown.
    pub action_at: Option<i64>,
    pub awake: bool,
    /// Sites whose media servers are not answering (only thumbnails work).
    pub down_sites: Vec<String>,
}

/// Seconds between the queue finishing and the when-done action.
pub const WHEN_DONE_DELAY_MS: i64 = 60_000;

/// When the download window opens next (epoch ms), or None while it is open.
/// The window may cross midnight; equal start and end mean always open.
pub fn closed_until(now: chrono::DateTime<chrono::Local>, start: &str, end: &str) -> Option<i64> {
    use chrono::{Duration as Days, Timelike};
    let (Some((sh, sm)), Some((eh, em))) =
        (settings::parse_clock(start), settings::parse_clock(end))
    else {
        return None;
    };
    let (start, end) = (sh * 60 + sm, eh * 60 + em);
    let minute = now.hour() * 60 + now.minute();
    let open = if start == end {
        true
    } else if start < end {
        minute >= start && minute < end
    } else {
        minute >= start || minute < end
    };
    if open {
        return None;
    }
    let today = now.date_naive().and_hms_opt(sh, sm, 0)?;
    let opens = if minute < start {
        today
    } else {
        today + Days::days(1)
    };
    opens
        .and_local_timezone(chrono::Local)
        .earliest()
        .map(|t| t.timestamp_millis())
}

pub struct CoreOptions {
    pub data_dir: PathBuf,
    pub log_file: Option<PathBuf>,
    /// Sends every pawchive.pw request to this origin instead (tests).
    pub origin_override: Option<String>,
    pub in_memory: bool,
    /// Millisecond pauses instead of seconds (tests only).
    pub fast_timing: bool,
}

impl Core {
    pub fn new(options: CoreOptions) -> Result<Arc<Self>, String> {
        let store = Arc::new(if options.in_memory {
            Store::in_memory()
        } else {
            Store::open(&options.data_dir.join("library.db"))?
        });
        let stored = store.get_raw("settings").unwrap_or(serde_json::Value::Null);
        let settings = settings::coerce(&stored);
        let log = Logger::new();
        if let Some(file) = &options.log_file {
            log.set_file(file);
        }
        log.set_level(Level::parse(&settings.log_level));
        let http = build_client(&settings.proxy).or_else(|e| {
            log.error(
                "The proxy settings are not usable; connecting directly",
                Some(e),
            );
            build_client(&Default::default())
        })?;
        let interval = if options.fast_timing {
            Duration::ZERO
        } else {
            Duration::from_millis(u64::from(settings.request_interval_ms))
        };
        let policy = if options.fast_timing {
            RateLimitPolicy {
                min_cooldown: Duration::from_millis(5),
                max_cooldown: Duration::from_millis(50),
                backoff_base: Duration::from_millis(5),
                backoff_cap: Duration::from_millis(20),
                resume_spread: Duration::ZERO,
            }
        } else {
            RateLimitPolicy::default()
        };
        let limiter = SiteLimiter::with_policy(MAX_CONCURRENCY as usize, interval, policy);
        let retry_log = Arc::clone(&log);
        let mut client = PawchiveClient::new(http, limiter).with_retry_hook(Arc::new(
            move |url, attempt, delay, reason| {
                if reason == "no connection" && attempt == 1 {
                    retry_log.warn(
                        "No connection to Pawchive, waiting for the network",
                        Some(url.to_string()),
                    );
                }
                retry_log.debug(format!(
                    "Retry {attempt} in {:.1}s ({reason}): {url}",
                    delay.as_secs_f64()
                ));
            },
        ));
        if let Some(origin) = &options.origin_override {
            client = client.with_origin_override(origin.clone());
        }
        client.configure(settings.max_retries.min(8), interval);
        let client = Arc::new(client);
        let throttle = Arc::new(Throttle::new(u64::from(settings.speed_limit_kbps) * 1024));
        let settings = Arc::new(RwLock::new(settings));
        let engine = Engine::new(
            Arc::clone(&client),
            Arc::clone(&throttle),
            Arc::clone(&store),
            Arc::clone(&settings),
            Arc::clone(&log),
        );
        let watcher = Watcher::new(
            Arc::clone(&store),
            Arc::clone(&client),
            Arc::clone(&engine),
            Arc::clone(&settings),
            Arc::clone(&log),
        );
        Ok(Arc::new(Self {
            settings,
            store,
            log,
            client,
            throttle,
            engine,
            watcher,
            directories: crate::pawchive::urls::Site::ALL
                .iter()
                .map(|s| Arc::new(Directory::for_site(options.data_dir.clone(), *s)))
                .collect(),
            data_dir: options.data_dir,
            cancel: CancellationToken::new(),
            fast_timing: options.fast_timing,
            awake: KeepAwake::new(),
            when_done: parking_lot::Mutex::new(WhenDone::Nothing),
            countdown: parking_lot::Mutex::new(None),
            on_queue: RwLock::new(None),
            on_quit: RwLock::new(None),
        }))
    }

    /// Restores the queue and starts the scheduler and the watchlist timer.
    /// Must be called inside the async runtime.
    pub fn start(self: &Arc<Self>) {
        self.engine
            .attach_runtime(tokio::runtime::Handle::current());
        self.engine.restore();
        tokio::spawn(Arc::clone(&self.engine).run());
        tokio::spawn(Arc::clone(&self.watcher).run(self.cancel.clone()));
        tokio::spawn(Arc::clone(self).guard());
    }

    pub fn set_on_queue(&self, f: OnQueue) {
        *self.on_queue.write() = Some(f);
    }

    pub fn set_on_quit(&self, f: OnQuit) {
        *self.on_quit.write() = Some(f);
    }

    pub fn queue_status(&self) -> QueueStatus {
        QueueStatus {
            hold: self.engine.hold(),
            when_done: *self.when_done.lock(),
            action_at: self.countdown.lock().as_ref().map(|(at, _)| *at),
            awake: self.awake.is_held(),
            down_sites: self.client.down_sites(),
        }
    }

    fn queue_changed(&self) {
        if let Some(f) = self.on_queue.read().clone() {
            f(self.queue_status());
        }
    }

    pub fn set_when_done(&self, action: WhenDone) {
        *self.when_done.lock() = action;
        if action == WhenDone::Nothing {
            self.cancel_when_done();
        }
        self.queue_changed();
    }

    /// Calls off a running when-done countdown and forgets the action.
    pub fn cancel_when_done(&self) {
        if let Some((_, token)) = self.countdown.lock().take() {
            token.cancel();
            *self.when_done.lock() = WhenDone::Nothing;
            self.log
                .info("The action after the downloads was called off");
        }
        self.queue_changed();
    }

    fn start_countdown(self: &Arc<Self>, action: WhenDone) {
        let delay = if self.fast_timing {
            50
        } else {
            WHEN_DONE_DELAY_MS
        };
        let token = self.cancel.child_token();
        *self.countdown.lock() = Some((now_ms() + delay, token.clone()));
        self.log.info(format!(
            "Downloads finished; {action:?} in {} s",
            delay / 1000
        ));
        self.queue_changed();
        let core = Arc::clone(self);
        tokio::spawn(async move {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(delay as u64)) => {}
                _ = token.cancelled() => return,
            }
            core.countdown.lock().take();
            *core.when_done.lock() = WhenDone::Nothing;
            core.queue_changed();
            match action {
                WhenDone::Quit => {
                    if let Some(quit) = core.on_quit.read().clone() {
                        quit();
                    }
                }
                other => {
                    if let Err(e) = power::perform(other) {
                        core.log.error("Could not sleep or shut down", Some(e));
                    }
                }
            }
        });
    }

    /// Every few seconds: holds downloads outside the download window or when
    /// the drive is nearly full, keeps the computer awake while they run, and
    /// starts the when-done countdown once the queue is empty.
    async fn guard(self: Arc<Self>) {
        let period = if self.fast_timing {
            Duration::from_millis(20)
        } else {
            Duration::from_secs(3)
        };
        let mut tick = tokio::time::interval(period);
        let mut last_status = self.queue_status();
        let mut was_busy = false;
        let mut finished_seen = self.engine.finished_total();
        let mut disk_checked: Option<(std::time::Instant, Option<Hold>)> = None;
        loop {
            tokio::select! {
                _ = tick.tick() => {}
                _ = self.cancel.cancelled() => break,
            }
            let s = self.settings();
            let folders = self.engine.pending_folders();
            let mut hold = None;
            if s.schedule_enabled {
                if let Some(opens_at) =
                    closed_until(chrono::Local::now(), &s.schedule_start, &s.schedule_end)
                {
                    hold = Some(Hold::Schedule { opens_at });
                }
            }
            if hold.is_none() && s.min_free_space_mb > 0 {
                // Checked even when idle, so a new job never starts on a full
                // drive before the next check.
                let stale = disk_checked
                    .as_ref()
                    .is_none_or(|(at, _)| at.elapsed() >= period * 5);
                if stale {
                    let need = u64::from(s.min_free_space_mb) * 1024 * 1024;
                    let mut check = vec![s.download_dir.clone()];
                    check.extend(folders.iter().filter(|f| **f != s.download_dir).cloned());
                    let low = check.iter().find_map(|folder| {
                        let free = power::free_space(Path::new(folder))?;
                        (free < need).then(|| Hold::Disk {
                            free,
                            need,
                            folder: folder.clone(),
                        })
                    });
                    disk_checked = Some((std::time::Instant::now(), low));
                }
                hold = disk_checked.as_ref().and_then(|(_, h)| h.clone());
            } else {
                disk_checked = None;
            }
            let previous = self.engine.hold();
            if self.engine.set_hold(hold.clone()) {
                match (&previous, &hold) {
                    (_, Some(Hold::Disk { free, folder, .. })) => self.log.warn(
                        format!(
                            "Downloads wait: only {} MB free in {folder}",
                            free / 1024 / 1024
                        ),
                        None,
                    ),
                    (_, Some(Hold::Schedule { .. })) => {
                        self.log.info("Downloads wait for the download window")
                    }
                    (Some(_), None) => self.log.info("Downloads continue"),
                    _ => {}
                }
            }
            if s.auto_retry_minutes > 0 {
                let after = if self.fast_timing {
                    i64::from(s.auto_retry_minutes)
                } else {
                    i64::from(s.auto_retry_minutes) * 60_000
                };
                self.engine.auto_retry(after, 3);
            }
            let busy = !folders.is_empty();
            let transferring = self.engine.active_count() > 0 || (busy && hold.is_none());
            self.awake.set(s.keep_awake && transferring);
            // A job finished since the last check (even one too quick to be
            // seen running) and nothing is left: the queue is done.
            let finished = self.engine.finished_total();
            let finished_any = finished != finished_seen;
            finished_seen = finished;
            // Paused jobs are not done: pausing everything never shuts down.
            if (was_busy || finished_any) && !busy && !self.engine.has_unfinished() {
                let action = *self.when_done.lock();
                if action != WhenDone::Nothing && self.countdown.lock().is_none() {
                    self.start_countdown(action);
                }
            }
            was_busy = busy;
            let status = self.queue_status();
            if status != last_status {
                last_status = status;
                self.queue_changed();
            }
        }
        self.awake.set(false);
    }

    pub fn directory(&self, site: crate::pawchive::urls::Site) -> &Arc<Directory> {
        let index = crate::pawchive::urls::Site::ALL
            .iter()
            .position(|s| *s == site)
            .unwrap_or(0);
        &self.directories[index]
    }

    pub fn settings(&self) -> Settings {
        self.settings.read().clone()
    }

    /// Saves new settings and applies the parts that affect running services.
    pub fn apply_settings(&self, next: Settings) -> Result<Settings, String> {
        let previous = self.settings();
        self.store.set("settings", &next)?;
        *self.settings.write() = next.clone();
        let interval = if self.fast_timing {
            Duration::ZERO
        } else {
            Duration::from_millis(u64::from(next.request_interval_ms))
        };
        self.client
            .limiter
            .configure(MAX_CONCURRENCY as usize, interval);
        self.client.configure(next.max_retries.min(8), interval);
        self.throttle
            .set_rate(u64::from(next.speed_limit_kbps) * 1024);
        self.log.set_level(Level::parse(&next.log_level));
        if previous.proxy != next.proxy {
            match build_client(&next.proxy) {
                Ok(http) => {
                    self.client.set_http(http);
                    self.log.info("Network settings changed");
                }
                Err(e) => self.log.error("The proxy settings are not usable", Some(e)),
            }
        }
        Ok(next)
    }

    pub fn shutdown(&self) {
        self.cancel.cancel();
        self.engine.shutdown();
    }
}

/// Folder of the default log file inside a data folder.
pub fn log_file_in(dir: &Path) -> PathBuf {
    dir.join("pawchive-downloader.log")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Local, TimeZone};

    fn at(h: u32, m: u32) -> chrono::DateTime<Local> {
        Local
            .with_ymd_and_hms(2026, 11, 18, h, m, 0)
            .single()
            .unwrap()
    }

    #[test]
    fn the_download_window_can_cross_midnight() {
        // 01:00-07:00: open at 03:00, closed at 12:00 until 01:00 tomorrow.
        assert_eq!(closed_until(at(3, 0), "01:00", "07:00"), None);
        let opens = closed_until(at(12, 0), "01:00", "07:00").unwrap();
        assert_eq!(
            opens,
            Local
                .with_ymd_and_hms(2026, 11, 19, 1, 0, 0)
                .unwrap()
                .timestamp_millis()
        );
        // Closed before the start on the same day.
        let opens = closed_until(at(0, 30), "01:00", "07:00").unwrap();
        assert_eq!(opens, at(1, 0).timestamp_millis());
        // 22:00-06:00 crosses midnight.
        assert_eq!(closed_until(at(23, 0), "22:00", "06:00"), None);
        assert_eq!(closed_until(at(5, 59), "22:00", "06:00"), None);
        assert_eq!(
            closed_until(at(6, 0), "22:00", "06:00"),
            Some(at(22, 0).timestamp_millis())
        );
        // Same start and end, or a broken value: always open.
        assert_eq!(closed_until(at(9, 0), "08:00", "08:00"), None);
        assert_eq!(closed_until(at(9, 0), "25:00", "08:00"), None);
    }
}
