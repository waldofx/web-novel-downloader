//! Per-host request spacing.
//!
//! Being slow on purpose is a feature: it keeps us from being blocked, and it
//! respects the sites we depend on.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Enforces a minimum delay between requests to the same host.
///
/// Overlapping callers push a host's next permitted time further out, so a
/// concurrent fetcher cannot accidentally hammer a site.
#[derive(Debug)]
pub struct RateLimiter {
    min_delay: Duration,
    next_allowed: Mutex<HashMap<String, Instant>>,
}

impl RateLimiter {
    pub fn new(min_delay: Duration) -> Self {
        Self {
            min_delay,
            next_allowed: Mutex::new(HashMap::new()),
        }
    }

    pub fn min_delay(&self) -> Duration {
        self.min_delay
    }

    /// How long a caller would have to wait to hit `host` right now.
    ///
    /// Split out from [`RateLimiter::acquire`] so the policy is testable
    /// without sleeping.
    pub fn delay_for(&self, host: &str) -> Duration {
        let map = self
            .next_allowed
            .lock()
            .expect("rate limiter lock poisoned");
        map.get(host)
            .map(|at| at.saturating_duration_since(Instant::now()))
            .unwrap_or(Duration::ZERO)
    }

    /// Block until a request to `host` is permitted, then reserve the slot.
    pub fn acquire(&self, host: &str) {
        self.acquire_with(host, self.min_delay);
    }

    /// Like [`RateLimiter::acquire`], but reserving `min_delay` for this host
    /// instead of the limiter's own.
    ///
    /// A site can ask for more patience than our policy's default through
    /// `robots.txt` (`Crawl-delay`); this is how that request is honored.
    pub fn acquire_with(&self, host: &str, min_delay: Duration) {
        let wait = self.delay_for(host);
        if !wait.is_zero() {
            std::thread::sleep(wait);
        }

        let mut map = self
            .next_allowed
            .lock()
            .expect("rate limiter lock poisoned");
        let now = Instant::now();
        let base = map.get(host).copied().filter(|at| *at > now).unwrap_or(now);
        map.insert(host.to_string(), base + min_delay);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unseen_host_is_never_delayed() {
        let limiter = RateLimiter::new(Duration::from_secs(5));
        assert_eq!(limiter.delay_for("example.com"), Duration::ZERO);
    }

    #[test]
    fn the_first_request_reserves_the_slot_but_does_not_sleep() {
        let limiter = RateLimiter::new(Duration::from_secs(30));
        let started = Instant::now();
        limiter.acquire("example.com");
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "the first request to a host must not block"
        );
        assert!(limiter.delay_for("example.com") > Duration::ZERO);
    }

    #[test]
    fn hosts_are_tracked_independently() {
        let limiter = RateLimiter::new(Duration::from_secs(30));
        limiter.acquire("slow.example.com");
        assert_eq!(limiter.delay_for("other.example.com"), Duration::ZERO);
    }

    #[test]
    fn a_per_call_delay_can_be_longer_than_the_default() {
        let limiter = RateLimiter::new(Duration::from_secs(1));
        limiter.acquire_with("example.com", Duration::from_secs(4));
        let wait = limiter.delay_for("example.com");
        assert!(
            wait > Duration::from_secs(2),
            "the longer per-host delay must be reserved, got {wait:?}"
        );
    }

    #[test]
    fn a_zero_delay_limiter_never_blocks() {
        let limiter = RateLimiter::new(Duration::ZERO);
        limiter.acquire("example.com");
        limiter.acquire("example.com");
        assert_eq!(limiter.delay_for("example.com"), Duration::ZERO);
    }
}
