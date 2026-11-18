//! Everything the app runs on, wired together without Tauri, so integration
//! tests can drive the real engine against a mock server.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::RwLock;
use tokio_util::sync::CancellationToken;

use crate::jobs::engine::Engine;
use crate::logs::{Level, Logger};
use crate::net::http::build_client;
use crate::net::limiter::{RateLimitPolicy, SiteLimiter};
use crate::net::throttle::Throttle;
use crate::pawchive::client::PawchiveClient;
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
    pub data_dir: PathBuf,
    pub cancel: CancellationToken,
    fast_timing: bool,
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
            data_dir: options.data_dir,
            cancel: CancellationToken::new(),
            fast_timing: options.fast_timing,
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
