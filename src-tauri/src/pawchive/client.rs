//! Small client for Pawchive's JSON API. The site (like Kemono) serves JSON
//! only to requests that ask for `text/css`, which is what its front end sends.

use std::sync::Arc;
use std::time::Duration;

use parking_lot::{Mutex, RwLock};
use serde::de::DeserializeOwned;
use tokio::time::{sleep, Instant};
use tokio_util::sync::CancellationToken;

use crate::net::limiter::{is_transient_status, site_key, Cancelled, SiteLimiter};

use super::normalize::{normalize_creator_on, normalize_post_on};
use super::relay::{parse_relay_links, RelayIndex};
use super::types::{Announcement, Comment, Creator, Post, RawPost, RawProfile};
use super::urls::{api, is_site_host, page, split_service, Site, PAGE_SIZE};

#[derive(Debug, Clone, thiserror::Error, PartialEq)]
pub enum ApiError {
    #[error("cancelled")]
    Cancelled,
    #[error("HTTP {status}")]
    Status { status: u16, url: String },
    #[error("{message}")]
    Network { message: String, url: String },
    #[error("unexpected answer from the site")]
    Format { url: String },
}

impl ApiError {
    pub fn not_found(&self) -> bool {
        matches!(self, ApiError::Status { status: 404, .. })
    }
    pub fn is_cancelled(&self) -> bool {
        matches!(self, ApiError::Cancelled)
    }
}

impl From<Cancelled> for ApiError {
    fn from(_: Cancelled) -> Self {
        ApiError::Cancelled
    }
}

#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub pages: u32,
    pub posts: u32,
    pub offset: u32,
}

/// How long and how often to wait for a lost connection to come back.
const OFFLINE_WAIT: Duration = Duration::from_secs(15);
const OFFLINE_WAITS: u32 = 240;

pub type RetryHook = Arc<dyn Fn(&str, u32, Duration, &str) + Send + Sync>;

pub struct PawchiveClient {
    http: RwLock<reqwest::Client>,
    pub limiter: Arc<SiteLimiter>,
    /// Points every pawchive.pw host at another origin (tests use a mock server).
    origin_override: Option<String>,
    max_retries: Mutex<u32>,
    max_rate_limit_retries: u32,
    min_interval: Mutex<Duration>,
    next_call_at: tokio::sync::Mutex<Instant>,
    on_retry: Option<RetryHook>,
    /// When each site last answered an API request.
    site_ok: Mutex<std::collections::HashMap<String, Instant>>,
    /// Sites whose media servers stopped answering, and until when to skip them.
    media_down: Mutex<std::collections::HashMap<String, Instant>>,
}

/// `host` or `host:port` of a URL.
pub fn host_key(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(u) => match (u.host_str(), u.port()) {
            (Some(h), Some(p)) => format!("{h}:{p}"),
            (Some(h), None) => h.to_ascii_lowercase(),
            _ => url.to_string(),
        },
        Err(_) => url.to_string(),
    }
}

impl PawchiveClient {
    /// The site answered an API request just now.
    pub fn note_site_ok(&self, url: &str) {
        self.site_ok.lock().insert(site_key(url), Instant::now());
    }

    /// Whether the site behind `url` answered an API request lately, which
    /// tells a down media server apart from a lost connection.
    pub fn site_answered_within(&self, url: &str, within: Duration) -> bool {
        self.site_ok
            .lock()
            .get(&site_key(url))
            .is_some_and(|at| at.elapsed() <= within)
    }

    /// Remembers that the media server behind `url` is not answering, so
    /// other files skip the connection timeout for a while. Keyed by host:
    /// thumbnails come from another host of the same site and still work.
    pub fn mark_media_down(&self, url: &str, for_how_long: Duration) {
        self.media_down
            .lock()
            .insert(host_key(url), Instant::now() + for_how_long);
    }

    /// The host when the media server behind `url` is known to be down.
    pub fn media_down(&self, url: &str) -> Option<String> {
        let key = host_key(url);
        let mut down = self.media_down.lock();
        match down.get(&key) {
            Some(until) if *until > Instant::now() => Some(key),
            Some(_) => {
                down.remove(&key);
                None
            }
            None => None,
        }
    }

    /// Media hosts that are down right now.
    pub fn down_sites(&self) -> Vec<String> {
        let now = Instant::now();
        let mut sites: Vec<String> = self
            .media_down
            .lock()
            .iter()
            .filter(|(_, until)| **until > now)
            .map(|(k, _)| k.clone())
            .collect();
        sites.sort();
        sites
    }

    pub fn new(http: reqwest::Client, limiter: Arc<SiteLimiter>) -> Self {
        Self {
            http: RwLock::new(http),
            limiter,
            origin_override: None,
            max_retries: Mutex::new(4),
            max_rate_limit_retries: 8,
            min_interval: Mutex::new(Duration::from_millis(1000)),
            next_call_at: tokio::sync::Mutex::new(Instant::now()),
            on_retry: None,
            site_ok: Mutex::new(Default::default()),
            media_down: Mutex::new(Default::default()),
        }
    }

    pub fn with_origin_override(mut self, origin: impl Into<String>) -> Self {
        self.origin_override = Some(origin.into().trim_end_matches('/').to_string());
        self
    }

    pub fn with_retry_hook(mut self, hook: RetryHook) -> Self {
        self.on_retry = Some(hook);
        self
    }

    pub fn set_http(&self, http: reqwest::Client) {
        *self.http.write() = http;
    }

    pub fn http(&self) -> reqwest::Client {
        self.http.read().clone()
    }

    pub fn configure(&self, max_retries: u32, min_interval: Duration) {
        *self.max_retries.lock() = max_retries;
        *self.min_interval.lock() = min_interval;
    }

    /// The URL actually requested for a site URL (identity outside tests).
    pub fn target(&self, url: &str) -> String {
        let Some(origin) = &self.origin_override else {
            return url.to_string();
        };
        match url::Url::parse(url) {
            Ok(parsed) if parsed.host_str().map(is_site_host).unwrap_or(false) => {
                let rest = &url[parsed.origin().ascii_serialization().len()..];
                format!("{origin}{rest}")
            }
            _ => url.to_string(),
        }
    }

    pub async fn get_post(
        &self,
        service: &str,
        creator_id: &str,
        post_id: &str,
        cancel: &CancellationToken,
    ) -> Result<Post, ApiError> {
        let url = api::post(service, creator_id, post_id);
        let value: serde_json::Value = self.json(&url, cancel).await?;
        // Kemono wraps the post in { post }, Pawchive returns it bare; accept both.
        let raw = match value.get("post") {
            Some(inner) if inner.is_object() => inner.clone(),
            _ => value,
        };
        let raw: RawPost =
            serde_json::from_value(raw).map_err(|_| ApiError::Format { url: url.clone() })?;
        Ok(normalize_post_on(split_service(service).0, raw))
    }

    pub async fn get_creator(
        &self,
        service: &str,
        creator_id: &str,
        cancel: &CancellationToken,
    ) -> Result<Creator, ApiError> {
        let raw: RawProfile = self
            .json(&api::profile(service, creator_id), cancel)
            .await?;
        Ok(normalize_creator_on(split_service(service).0, raw))
    }

    /// Comments under a post, oldest first. A post without comments may
    /// answer 404, which counts as none.
    pub async fn get_comments(
        &self,
        service: &str,
        creator_id: &str,
        post_id: &str,
        cancel: &CancellationToken,
    ) -> Result<Vec<Comment>, ApiError> {
        match self
            .json::<Vec<Comment>>(&api::comments(service, creator_id, post_id), cancel)
            .await
        {
            Err(e) if e.not_found() => Ok(Vec::new()),
            other => other,
        }
    }

    /// Every creator on the site (a large list; see `directory`).
    pub async fn get_creator_list(
        &self,
        site: Site,
        cancel: &CancellationToken,
    ) -> Result<Vec<super::directory::Entry>, ApiError> {
        self.json(&api::creators(site), cancel).await
    }

    /// Other accounts of the same creator (Patreon and Fanbox, for example).
    pub async fn get_linked(
        &self,
        service: &str,
        creator_id: &str,
        cancel: &CancellationToken,
    ) -> Result<Vec<Creator>, ApiError> {
        match self
            .json::<Vec<RawProfile>>(&api::linked(service, creator_id), cancel)
            .await
        {
            Ok(list) => {
                let site = split_service(service).0;
                Ok(list
                    .into_iter()
                    .map(|c| normalize_creator_on(site, c))
                    .filter(|c| !(c.service == service && c.id == creator_id))
                    .collect())
            }
            Err(e) if e.not_found() => Ok(Vec::new()),
            Err(e) => Err(e),
        }
    }

    pub async fn get_announcements(
        &self,
        service: &str,
        creator_id: &str,
        cancel: &CancellationToken,
    ) -> Result<Vec<Announcement>, ApiError> {
        match self
            .json::<Vec<Announcement>>(&api::announcements(service, creator_id), cancel)
            .await
        {
            Err(e) if e.not_found() => Ok(Vec::new()),
            other => other,
        }
    }

    pub async fn get_creator_page(
        &self,
        service: &str,
        creator_id: &str,
        offset: u32,
        query: Option<&str>,
        cancel: &CancellationToken,
    ) -> Result<Vec<Post>, ApiError> {
        let raw: Vec<RawPost> = self
            .json(
                &api::creator_posts(service, creator_id, offset, query),
                cancel,
            )
            .await?;
        let site = split_service(service).0;
        Ok(raw
            .into_iter()
            .map(|p| normalize_post_on(site, p))
            .collect())
    }

    /// Every post of a creator, newest first, fetched page by page.
    #[allow(clippy::too_many_arguments)]
    pub async fn get_all_creator_posts(
        &self,
        service: &str,
        creator_id: &str,
        query: Option<&str>,
        max_posts: Option<usize>,
        cancel: &CancellationToken,
        mut on_progress: impl FnMut(ScanProgress, &[Post]),
        mut stop_when: impl FnMut(&[Post]) -> bool,
    ) -> Result<Vec<Post>, ApiError> {
        let mut posts = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let max = max_posts.unwrap_or(usize::MAX);
        let mut offset = 0;
        let mut pages = 0;
        loop {
            let page = self
                .get_creator_page(service, creator_id, offset, query, cancel)
                .await?;
            pages += 1;
            let full = page.len() as u32 >= PAGE_SIZE;
            for post in &page {
                if seen.insert(post.id.clone()) {
                    posts.push(post.clone());
                }
            }
            on_progress(
                ScanProgress {
                    pages,
                    posts: posts.len() as u32,
                    offset,
                },
                &page,
            );
            if !full || posts.len() >= max || stop_when(&page) {
                break;
            }
            offset += PAGE_SIZE;
        }
        posts.truncate(max);
        Ok(posts)
    }

    pub async fn get_recent_posts(
        &self,
        site: Site,
        offset: u32,
        query: Option<&str>,
        cancel: &CancellationToken,
    ) -> Result<Vec<Post>, ApiError> {
        let raw: serde_json::Value = self
            .json(&api::recent_posts(site, offset, query), cancel)
            .await?;
        // The listing may come bare or wrapped as { posts: [...] }.
        let list = match raw {
            serde_json::Value::Array(_) => raw,
            serde_json::Value::Object(mut map) => map
                .remove("posts")
                .unwrap_or(serde_json::Value::Array(vec![])),
            _ => serde_json::Value::Array(vec![]),
        };
        // One odd entry must not empty the whole page.
        let items = match list {
            serde_json::Value::Array(items) => items,
            _ => Vec::new(),
        };
        Ok(items
            .into_iter()
            .filter_map(|v| serde_json::from_value::<RawPost>(v).ok())
            .map(|p| normalize_post_on(site, p))
            .collect())
    }

    /// Temporary relay links for the post's preview-only files.
    pub async fn get_relay_index(
        &self,
        service: &str,
        creator_id: &str,
        post_id: &str,
        cancel: &CancellationToken,
    ) -> Result<RelayIndex, ApiError> {
        // Only Pawchive offers relay links for preview-only files.
        if split_service(service).0 != Site::Pawchive {
            return Ok(RelayIndex::default());
        }
        let html = self
            .text(
                &page::post(service, creator_id, post_id),
                "text/html",
                cancel,
            )
            .await?;
        Ok(parse_relay_links(&html))
    }

    async fn json<T: DeserializeOwned>(
        &self,
        url: &str,
        cancel: &CancellationToken,
    ) -> Result<T, ApiError> {
        let body = self.text(url, "text/css", cancel).await?;
        serde_json::from_str(&body).map_err(|_| ApiError::Format {
            url: url.to_string(),
        })
    }

    async fn pace(&self, cancel: &CancellationToken) -> Result<(), ApiError> {
        let mut next = tokio::select! {
            guard = self.next_call_at.lock() => guard,
            _ = cancel.cancelled() => return Err(ApiError::Cancelled),
        };
        let wait = next.saturating_duration_since(Instant::now());
        if !wait.is_zero() {
            tokio::select! {
                _ = sleep(wait) => {},
                _ = cancel.cancelled() => return Err(ApiError::Cancelled),
            }
        }
        *next = Instant::now() + *self.min_interval.lock();
        Ok(())
    }

    async fn backoff_sleep(
        &self,
        delay: Duration,
        cancel: &CancellationToken,
    ) -> Result<(), ApiError> {
        tokio::select! {
            _ = sleep(delay) => Ok(()),
            _ = cancel.cancelled() => Err(ApiError::Cancelled),
        }
    }

    fn retried(&self, url: &str, attempt: u32, delay: Duration, reason: &str) {
        if let Some(hook) = &self.on_retry {
            hook(url, attempt, delay, reason);
        }
    }

    pub async fn text(
        &self,
        url: &str,
        accept: &str,
        cancel: &CancellationToken,
    ) -> Result<String, ApiError> {
        let mut failures = 0u32;
        let mut rate_limits = 0u32;
        let mut offline = 0u32;
        let max_retries = *self.max_retries.lock();
        loop {
            if cancel.is_cancelled() {
                return Err(ApiError::Cancelled);
            }
            self.limiter.wait_for_site(url, cancel).await?;
            self.pace(cancel).await?;
            let request = self
                .http()
                .get(self.target(url))
                .header("Accept", accept)
                .send();
            let response = tokio::select! {
                r = request => r,
                _ = cancel.cancelled() => return Err(ApiError::Cancelled),
            };
            let response = match response {
                Ok(r) => r,
                Err(error) => {
                    // No connection at all: wait for the network (up to about
                    // an hour) instead of failing the whole scan.
                    if error.is_connect() && offline < OFFLINE_WAITS {
                        offline += 1;
                        let delay = self.limiter.policy.backoff_cap.min(OFFLINE_WAIT);
                        self.retried(url, offline, delay, "no connection");
                        self.backoff_sleep(delay, cancel).await?;
                        continue;
                    }
                    if failures >= max_retries {
                        return Err(ApiError::Network {
                            message: describe(&error),
                            url: url.to_string(),
                        });
                    }
                    let delay = self.limiter.backoff(failures);
                    self.retried(url, failures + 1, delay, "network");
                    failures += 1;
                    self.backoff_sleep(delay, cancel).await?;
                    continue;
                }
            };
            let status = response.status().as_u16();
            let retry_after = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);
            let cooldown = self.limiter.note(url, status, retry_after.as_deref());
            if response.status().is_success() {
                let body = tokio::select! {
                    b = response.text() => b,
                    _ = cancel.cancelled() => return Err(ApiError::Cancelled),
                };
                match body {
                    Ok(body) => {
                        self.note_site_ok(url);
                        return Ok(body);
                    }
                    Err(error) if failures < max_retries => {
                        let delay = self.limiter.backoff(failures);
                        self.retried(url, failures + 1, delay, &describe(&error));
                        failures += 1;
                        self.backoff_sleep(delay, cancel).await?;
                        continue;
                    }
                    Err(error) => {
                        return Err(ApiError::Network {
                            message: describe(&error),
                            url: url.to_string(),
                        })
                    }
                }
            }
            if let Some(cooldown) = cooldown {
                if rate_limits < self.max_rate_limit_retries {
                    rate_limits += 1;
                    self.retried(url, rate_limits, cooldown, &format!("HTTP {status}"));
                    continue;
                }
            }
            if is_transient_status(status) && cooldown.is_none() && failures < max_retries {
                let delay = self.limiter.backoff(failures);
                self.retried(url, failures + 1, delay, &format!("HTTP {status}"));
                failures += 1;
                self.backoff_sleep(delay, cancel).await?;
                continue;
            }
            return Err(ApiError::Status {
                status,
                url: url.to_string(),
            });
        }
    }
}

/// A short, human readable description of a request error.
pub fn describe(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        "timed out".into()
    } else if error.is_connect() {
        "could not connect".into()
    } else if error.is_body() || error.is_decode() {
        "connection dropped".into()
    } else {
        let text = error.to_string();
        text.split(": ").last().unwrap_or(&text).to_string()
    }
}
