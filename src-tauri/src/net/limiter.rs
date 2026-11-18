//! Responsible request pacing for Pawchive, ported from KemonoDownloader's
//! download_policy.py and the browser extension:
//!
//! - at most `max_active` file transfers at once (Pawchive asks for five),
//! - at least `min_interval` between starting two requests (one a second),
//! - when the site answers 429 (or 503 with Retry-After) every request to it
//!   pauses until the cooldown ends: the server's Retry-After when given,
//!   otherwise an exponential, jittered backoff.
//!
//! Turns are served first come, first served. Time comes from tokio, so tests
//! run with a paused clock instead of waiting.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use tokio::sync::Notify;
use tokio::time::{sleep, Instant};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("cancelled")]
pub struct Cancelled;

#[derive(Debug, Clone, Copy)]
pub struct RateLimitPolicy {
    pub min_cooldown: Duration,
    pub max_cooldown: Duration,
    pub backoff_base: Duration,
    pub backoff_cap: Duration,
    /// Spread restarts after a cooldown so waiting workers do not stampede.
    pub resume_spread: Duration,
}

impl Default for RateLimitPolicy {
    fn default() -> Self {
        Self {
            min_cooldown: Duration::from_secs(1),
            max_cooldown: Duration::from_secs(300),
            backoff_base: Duration::from_secs(2),
            backoff_cap: Duration::from_secs(60),
            resume_spread: Duration::from_secs(1),
        }
    }
}

/// Wait to apply from a Retry-After header (seconds or HTTP date).
pub fn parse_retry_after(value: Option<&str>, wall_now_ms: i64) -> Option<Duration> {
    let text = value?.trim();
    if text.is_empty() || text.starts_with('-') {
        return None;
    }
    if let Ok(secs) = text.parse::<f64>() {
        return (secs.is_finite() && secs >= 0.0)
            .then(|| Duration::from_millis((secs * 1000.0).round() as u64));
    }
    let when = chrono::DateTime::parse_from_rfc2822(text)
        .ok()?
        .timestamp_millis();
    Some(Duration::from_millis((when - wall_now_ms).max(0) as u64))
}

pub fn is_rate_limited(status: u16, retry_after: Option<&str>) -> bool {
    status == 429 || (status == 503 && parse_retry_after(retry_after, 0).is_some())
}

/// Statuses worth retrying later: timeouts, rate limits and server errors.
pub fn is_transient_status(status: u16) -> bool {
    matches!(status, 408 | 425 | 429) || status >= 500
}

/// `n1.pawchive.pw` and `pawchive.pw` are one site behind one front end.
pub fn site_key(url: &str) -> String {
    let Ok(parsed) = url::Url::parse(url) else {
        return String::new();
    };
    let Some(host) = parsed.host_str() else {
        return String::new();
    };
    let host = host.to_ascii_lowercase();
    if host.chars().all(|c| c.is_ascii_digit() || c == '.') || host.contains(':') {
        return host;
    }
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() > 2 {
        labels[labels.len() - 2..].join(".")
    } else {
        host
    }
}

fn wall_now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

struct State {
    max_active: usize,
    min_interval: Duration,
    active: usize,
    next_start_at: Instant,
    cooldown_until: HashMap<String, Instant>,
    strikes: HashMap<String, u32>,
}

pub struct SiteLimiter {
    pub policy: RateLimitPolicy,
    state: Mutex<State>,
    /// Serialises turns so they are handed out in arrival order.
    turnstile: tokio::sync::Mutex<()>,
    slot_freed: Notify,
    random: Mutex<Box<dyn FnMut() -> f64 + Send>>,
}

/// A transfer slot. Released when dropped.
pub struct Permit {
    limiter: Arc<SiteLimiter>,
}

impl Drop for Permit {
    fn drop(&mut self) {
        {
            let mut state = self.limiter.state.lock();
            state.active = state.active.saturating_sub(1);
        }
        self.limiter.slot_freed.notify_waiters();
    }
}

async fn sleep_or_cancel(duration: Duration, cancel: &CancellationToken) -> Result<(), Cancelled> {
    if duration.is_zero() {
        return if cancel.is_cancelled() {
            Err(Cancelled)
        } else {
            Ok(())
        };
    }
    tokio::select! {
        _ = sleep(duration) => Ok(()),
        _ = cancel.cancelled() => Err(Cancelled),
    }
}

impl SiteLimiter {
    pub fn new(max_active: usize, min_interval: Duration) -> Arc<Self> {
        Self::with_policy(max_active, min_interval, RateLimitPolicy::default())
    }

    pub fn with_policy(
        max_active: usize,
        min_interval: Duration,
        policy: RateLimitPolicy,
    ) -> Arc<Self> {
        Arc::new(Self {
            policy,
            state: Mutex::new(State {
                max_active: max_active.max(1),
                min_interval,
                active: 0,
                next_start_at: Instant::now(),
                cooldown_until: HashMap::new(),
                strikes: HashMap::new(),
            }),
            turnstile: tokio::sync::Mutex::new(()),
            slot_freed: Notify::new(),
            random: Mutex::new(Box::new(rand::random::<f64>)),
        })
    }

    /// Replaces the randomness source (tests use a fixed value).
    pub fn set_random(&self, f: impl FnMut() -> f64 + Send + 'static) {
        *self.random.lock() = Box::new(f);
    }

    fn rand(&self) -> f64 {
        (self.random.lock())()
    }

    pub fn active_count(&self) -> usize {
        self.state.lock().active
    }

    pub fn configure(&self, max_active: usize, min_interval: Duration) {
        {
            let mut state = self.state.lock();
            state.max_active = max_active.max(1);
            state.min_interval = min_interval;
        }
        self.slot_freed.notify_waiters();
    }

    /// Time until the site of `url` may be contacted again.
    pub fn remaining(&self, url: &str) -> Duration {
        let key = site_key(url);
        let state = self.state.lock();
        state
            .cooldown_until
            .get(&key)
            .map(|until| until.saturating_duration_since(Instant::now()))
            .unwrap_or_default()
    }

    /// Waits for a transfer slot, the start interval and any cooldown.
    pub async fn acquire(
        self: &Arc<Self>,
        url: &str,
        cancel: &CancellationToken,
    ) -> Result<Permit, Cancelled> {
        let _turn = tokio::select! {
            guard = self.turnstile.lock() => guard,
            _ = cancel.cancelled() => return Err(Cancelled),
        };
        self.wait_for_turn(url, cancel, true).await?;
        Ok(Permit {
            limiter: Arc::clone(self),
        })
    }

    /// Waits for the cooldown and the start interval without taking a slot,
    /// for work that already holds one and makes several requests in a row.
    pub async fn pace(&self, url: &str, cancel: &CancellationToken) -> Result<(), Cancelled> {
        let _turn = tokio::select! {
            guard = self.turnstile.lock() => guard,
            _ = cancel.cancelled() => return Err(Cancelled),
        };
        self.wait_for_turn(url, cancel, false).await
    }

    /// Waits out a cooldown only.
    pub async fn wait_for_site(
        &self,
        url: &str,
        cancel: &CancellationToken,
    ) -> Result<(), Cancelled> {
        let mut waited = false;
        loop {
            let left = self.remaining(url);
            if left.is_zero() {
                break;
            }
            sleep_or_cancel(left, cancel).await?;
            waited = true;
        }
        if waited && !self.policy.resume_spread.is_zero() {
            let spread = self.policy.resume_spread.mul_f64(self.rand());
            sleep_or_cancel(spread, cancel).await?;
        }
        Ok(())
    }

    async fn wait_for_turn(
        &self,
        url: &str,
        cancel: &CancellationToken,
        take_slot: bool,
    ) -> Result<(), Cancelled> {
        loop {
            if cancel.is_cancelled() {
                return Err(Cancelled);
            }
            if !self.remaining(url).is_zero() {
                self.wait_for_site(url, cancel).await?;
                continue;
            }
            // Register interest before checking, so a release in between is not missed.
            let freed = self.slot_freed.notified();
            let (full, wait) = {
                let state = self.state.lock();
                (
                    take_slot && state.active >= state.max_active,
                    state
                        .next_start_at
                        .saturating_duration_since(Instant::now()),
                )
            };
            if full {
                tokio::select! {
                    _ = freed => {},
                    _ = cancel.cancelled() => return Err(Cancelled),
                }
                continue;
            }
            if !wait.is_zero() {
                sleep_or_cancel(wait, cancel).await?;
                continue;
            }
            let mut state = self.state.lock();
            if take_slot {
                state.active += 1;
            }
            state.next_start_at = Instant::now() + state.min_interval;
            return Ok(());
        }
    }

    /// Records an HTTP answer from `url`. Returns the cooldown when it was a
    /// rate limit. A success after the cooldown resets the backoff.
    pub fn note(&self, url: &str, status: u16, retry_after: Option<&str>) -> Option<Duration> {
        let key = site_key(url);
        if key.is_empty() {
            return None;
        }
        let now = Instant::now();
        let random = self.rand();
        let mut state = self.state.lock();
        let until = state.cooldown_until.get(&key).copied().unwrap_or(now);
        if !is_rate_limited(status, retry_after) {
            if status > 0 && status < 400 && now >= until {
                state.strikes.remove(&key);
            }
            return None;
        }
        let cooling = now < until;
        // Answers to requests sent before the cooldown began are one strike, not many.
        let strikes = (state.strikes.get(&key).copied().unwrap_or(0) + u32::from(!cooling)).max(1);
        state.strikes.insert(key.clone(), strikes);
        let delay = match parse_retry_after(retry_after, wall_now_ms()) {
            Some(d) => d,
            None if cooling => Duration::ZERO,
            None => self.jittered(strikes - 1, random),
        };
        let delay = delay.clamp(self.policy.min_cooldown, self.policy.max_cooldown);
        let next = until.max(now + delay);
        state.cooldown_until.insert(key, next);
        Some(next - now)
    }

    /// Puts the whole site on cooldown, e.g. after a failure without a status.
    pub fn penalize(&self, url: &str, attempt: u32) -> Duration {
        let key = site_key(url);
        let now = Instant::now();
        let delay = self
            .jittered(attempt, self.rand())
            .clamp(self.policy.min_cooldown, self.policy.max_cooldown);
        let mut state = self.state.lock();
        let next = state
            .cooldown_until
            .get(&key)
            .copied()
            .unwrap_or(now)
            .max(now + delay);
        state.cooldown_until.insert(key, next);
        next - now
    }

    /// Jittered exponential backoff for retry `attempt` (0-based).
    pub fn backoff(&self, attempt: u32) -> Duration {
        self.jittered(attempt, self.rand())
    }

    fn jittered(&self, attempt: u32, random: f64) -> Duration {
        let full = self
            .policy
            .backoff_base
            .saturating_mul(1u32 << attempt.min(20))
            .min(self.policy.backoff_cap);
        full / 2 + full.mul_f64(random / 2.0)
    }

    pub fn reset(&self) {
        let mut state = self.state.lock();
        state.cooldown_until.clear();
        state.strikes.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const URL: &str = "https://n1.pawchive.pw/data/x";

    #[test]
    fn parses_retry_after() {
        assert_eq!(
            parse_retry_after(Some("2"), 0),
            Some(Duration::from_secs(2))
        );
        assert_eq!(
            parse_retry_after(Some("1.5"), 0),
            Some(Duration::from_millis(1500))
        );
        assert_eq!(parse_retry_after(Some("-1"), 0), None);
        assert_eq!(parse_retry_after(Some(""), 0), None);
        assert_eq!(parse_retry_after(None, 0), None);
        let at = chrono::DateTime::parse_from_rfc2822("Wed, 21 Oct 2015 07:28:00 GMT")
            .unwrap()
            .timestamp_millis();
        assert_eq!(
            parse_retry_after(Some("Wed, 21 Oct 2015 07:28:00 GMT"), at - 5000),
            Some(Duration::from_secs(5))
        );
        assert_eq!(
            parse_retry_after(Some("Wed, 21 Oct 2015 07:28:00 GMT"), at + 5000),
            Some(Duration::ZERO)
        );
    }

    #[test]
    fn groups_subdomains_into_one_site() {
        assert_eq!(site_key("https://n3.pawchive.pw/a"), "pawchive.pw");
        assert_eq!(site_key("https://pawchive.pw/a"), "pawchive.pw");
        assert_eq!(site_key("http://127.0.0.1:8080/a"), "127.0.0.1");
        assert_eq!(site_key("nonsense"), "");
        assert!(is_rate_limited(429, None));
        assert!(is_rate_limited(503, Some("3")));
        assert!(!is_rate_limited(503, None));
        assert!(is_transient_status(502) && is_transient_status(408) && !is_transient_status(404));
    }

    #[tokio::test(start_paused = true)]
    async fn spaces_starts_and_caps_active_transfers() {
        let limiter = SiteLimiter::new(2, Duration::from_secs(1));
        let cancel = CancellationToken::new();
        let start = Instant::now();
        let a = limiter.acquire(URL, &cancel).await.unwrap();
        let b = limiter.acquire(URL, &cancel).await.unwrap();
        assert_eq!(start.elapsed(), Duration::from_secs(1));
        assert_eq!(limiter.active_count(), 2);
        let l2 = Arc::clone(&limiter);
        let c2 = cancel.clone();
        let third = tokio::spawn(async move { l2.acquire(URL, &c2).await.map(|_| Instant::now()) });
        sleep(Duration::from_secs(5)).await;
        assert!(!third.is_finished());
        drop(a);
        let at = third.await.unwrap().unwrap();
        assert_eq!(at - start, Duration::from_secs(6));
        drop(b);
    }

    #[tokio::test(start_paused = true)]
    async fn honours_cooldowns_and_cancellation() {
        let limiter = SiteLimiter::new(5, Duration::ZERO);
        limiter.set_random(|| 0.0);
        let cancel = CancellationToken::new();
        assert_eq!(
            limiter.note(URL, 429, Some("10")),
            Some(Duration::from_secs(10))
        );
        // A later answer from before the cooldown does not extend it.
        assert_eq!(
            limiter.note("https://pawchive.pw/api", 429, None),
            Some(Duration::from_secs(10))
        );
        assert_eq!(
            limiter.remaining("https://pawchive.pw/x"),
            Duration::from_secs(10)
        );
        let start = Instant::now();
        let _p = limiter.acquire(URL, &cancel).await.unwrap();
        assert_eq!(start.elapsed(), Duration::from_secs(10));

        limiter.note(URL, 429, Some("30"));
        let c2 = cancel.clone();
        let l2 = Arc::clone(&limiter);
        let waiting = tokio::spawn(async move { l2.pace(URL, &c2).await });
        sleep(Duration::from_secs(1)).await;
        cancel.cancel();
        assert_eq!(waiting.await.unwrap(), Err(Cancelled));
    }

    #[tokio::test(start_paused = true)]
    async fn backs_off_exponentially_without_retry_after() {
        let limiter = SiteLimiter::new(5, Duration::ZERO);
        limiter.set_random(|| 1.0);
        assert_eq!(limiter.note(URL, 429, None), Some(Duration::from_secs(2)));
        sleep(Duration::from_secs(3)).await;
        assert_eq!(limiter.note(URL, 429, None), Some(Duration::from_secs(4)));
        sleep(Duration::from_secs(5)).await;
        // Success after the cooldown resets the strikes.
        assert_eq!(limiter.note(URL, 200, None), None);
        assert_eq!(limiter.note(URL, 429, None), Some(Duration::from_secs(2)));
        assert_eq!(limiter.backoff(10), Duration::from_secs(60));
        let p = limiter.penalize("https://other.example/x", 0);
        assert_eq!(p, Duration::from_secs(2));
        limiter.reset();
        assert!(limiter.remaining(URL).is_zero());
    }
}
