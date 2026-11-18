//! Small client for Pawchive's JSON API. The site (like Kemono) serves JSON
//! only to requests that ask for `text/css`, which is what its front end sends.

use std::sync::Arc;
use std::time::Duration;

use parking_lot::{Mutex, RwLock};
use serde::de::DeserializeOwned;
use tokio::time::{sleep, Instant};
use tokio_util::sync::CancellationToken;

use crate::net::limiter::{is_transient_status, Cancelled, SiteLimiter};

use super::normalize::{normalize_creator, normalize_post};
use super::relay::{parse_relay_links, RelayIndex};
use super::types::{Creator, Post, RawPost, RawProfile};
use super::urls::{api, is_site_host, page, PAGE_SIZE};

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
}

impl PawchiveClient {
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
        Ok(normalize_post(raw))
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
        Ok(normalize_creator(raw))
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
        Ok(raw.into_iter().map(normalize_post).collect())
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
        offset: u32,
        query: Option<&str>,
        cancel: &CancellationToken,
    ) -> Result<Vec<Post>, ApiError> {
        let raw: serde_json::Value = self.json(&api::recent_posts(offset, query), cancel).await?;
        // The listing may come bare or wrapped as { posts: [...] }.
        let list = match raw {
            serde_json::Value::Array(_) => raw,
            serde_json::Value::Object(mut map) => map
                .remove("posts")
                .unwrap_or(serde_json::Value::Array(vec![])),
            _ => serde_json::Value::Array(vec![]),
        };
        let raw: Vec<RawPost> = serde_json::from_value(list).unwrap_or_default();
        Ok(raw.into_iter().map(normalize_post).collect())
    }

    /// Temporary relay links for the post's preview-only files.
    pub async fn get_relay_index(
        &self,
        service: &str,
        creator_id: &str,
        post_id: &str,
        cancel: &CancellationToken,
    ) -> Result<RelayIndex, ApiError> {
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
                    Ok(body) => return Ok(body),
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
