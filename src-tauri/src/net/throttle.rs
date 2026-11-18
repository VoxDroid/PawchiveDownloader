//! A shared token bucket that caps the total download speed of all transfers.

use std::time::Duration;

use parking_lot::Mutex;
use tokio::time::Instant;

pub struct Throttle {
    state: Mutex<Bucket>,
}

struct Bucket {
    /// Bytes per second; 0 means unlimited.
    rate: u64,
    tokens: f64,
    last: Instant,
}

impl Default for Throttle {
    fn default() -> Self {
        Self::new(0)
    }
}

impl Throttle {
    pub fn new(rate: u64) -> Self {
        Self {
            state: Mutex::new(Bucket {
                rate,
                tokens: rate as f64,
                last: Instant::now(),
            }),
        }
    }

    pub fn set_rate(&self, rate: u64) {
        let mut b = self.state.lock();
        b.rate = rate;
        b.tokens = b.tokens.min(rate as f64);
    }

    pub fn rate(&self) -> u64 {
        self.state.lock().rate
    }

    /// How long to wait before `bytes` more may be consumed. The bytes are
    /// taken immediately (the bucket may go negative), which keeps the long
    /// term average at the rate without stalling small reads.
    pub fn reserve(&self, bytes: usize) -> Duration {
        let mut b = self.state.lock();
        if b.rate == 0 {
            return Duration::ZERO;
        }
        let now = Instant::now();
        let elapsed = now.duration_since(b.last).as_secs_f64();
        b.last = now;
        // At most one second of burst.
        b.tokens = (b.tokens + elapsed * b.rate as f64).min(b.rate as f64);
        b.tokens -= bytes as f64;
        if b.tokens >= 0.0 {
            Duration::ZERO
        } else {
            Duration::from_secs_f64(-b.tokens / b.rate as f64)
        }
    }

    pub async fn consume(&self, bytes: usize) {
        let wait = self.reserve(bytes);
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn limits_the_average_rate() {
        let t = Throttle::new(1000);
        assert_eq!(t.reserve(500), Duration::ZERO);
        assert_eq!(t.reserve(1500), Duration::from_secs(1));
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert_eq!(t.reserve(0), Duration::ZERO);
        t.set_rate(0);
        assert_eq!(t.reserve(1 << 30), Duration::ZERO);
        assert_eq!(t.rate(), 0);
    }
}
