//! How we are allowed to talk to other people's servers.

use crate::rate_limit::RateLimiter;
use std::time::Duration;

/// Politeness settings for the fetcher.
///
/// Defaults are deliberately conservative: a single site can be large, and
/// getting blocked helps nobody.
#[derive(Debug, Clone)]
pub struct FetchPolicy {
    /// Minimum delay between two requests to the same host.
    pub min_delay: Duration,
    /// Maximum simultaneous in-flight requests across all hosts.
    pub max_concurrency: usize,
    /// Attempts per request before giving up (429/5xx/network errors).
    pub max_retries: u32,
    /// Sent as `User-Agent`, so sites can identify and rate-limit us properly.
    pub user_agent: String,
    /// Honor `robots.txt` before fetching.
    pub respect_robots: bool,
}

impl Default for FetchPolicy {
    fn default() -> Self {
        Self {
            min_delay: Duration::from_secs(1),
            max_concurrency: 3,
            max_retries: 3,
            user_agent: format!(
                "web-novel-downloader/{} (+https://github.com/waldofx/web-novel-downloader)",
                env!("CARGO_PKG_VERSION")
            ),
            respect_robots: true,
        }
    }
}

impl FetchPolicy {
    /// A limiter configured from this policy.
    pub fn limiter(&self) -> RateLimiter {
        RateLimiter::new(self.min_delay)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_conservative() {
        let policy = FetchPolicy::default();
        assert!(policy.min_delay >= Duration::from_secs(1));
        assert!(policy.max_concurrency <= 4);
        assert!(policy.respect_robots);
        assert!(policy.user_agent.contains("web-novel-downloader"));
    }
}
