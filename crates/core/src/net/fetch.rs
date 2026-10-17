//! Polite, retrying HTTP access.
//!
//! Every network read goes through [`HttpClient`], so the rules that keep this
//! tool a good citizen live in exactly one place:
//!
//! - a minimum delay between requests to the same host,
//! - `robots.txt`, parsed once per origin and honored,
//! - bounded retries with exponential backoff that respects `Retry-After`,
//! - an optional cache of raw responses, so a parser change can be re-run
//!   without touching the site again,
//! - and [`net::blocked`](crate::net::blocked), which catches Cloudflare's
//!   challenge from its header, and describes an answer that cannot be read as
//!   the site whenever an adapter says its markup is not there.
//!
//! Requests are issued one at a time. The per-host delay is the real bottleneck
//! when downloading a single novel, so concurrency would buy almost nothing
//! while making us look like a scraper. [`FetchPolicy::max_concurrency`] is
//! reserved for the multi-host case (illustrations on a CDN) and is unused here.
//!
//! Setting [`FetchPolicy::respect_robots`] to `false` is the documented escape
//! hatch for a site whose published rules are stale or broken.

use crate::error::{Error, Result};
use crate::net::blocked;
use crate::net::policy::FetchPolicy;
use crate::net::rate_limit::RateLimiter;
use crate::net::robots::RobotsRules;
use reqwest::Url;
use reqwest::header::{HeaderMap, RETRY_AFTER};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

/// How long to wait for a connection to be established.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a whole request may take. Novel chapters are small.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// First backoff step; it doubles per attempt.
const BACKOFF_BASE: Duration = Duration::from_secs(1);
/// Ceiling on any single wait, so a hostile `Retry-After` cannot stall us for hours.
const BACKOFF_CAP: Duration = Duration::from_secs(60);

/// Statuses worth another attempt: rate limiting and transient server faults.
///
/// Everything else (404, 410, 403, …) is final — retrying a missing chapter
/// helps nobody and hammers the site.
fn is_retryable_status(status: u16) -> bool {
    matches!(status, 408 | 425 | 429 | 500 | 502 | 503 | 504)
}

/// How long to wait before retry `attempt` (0-based).
///
/// Exponential from [`BACKOFF_BASE`], never shorter than what the server asked
/// for, and never longer than [`BACKOFF_CAP`].
fn backoff_delay(attempt: u32, retry_after: Option<Duration>) -> Duration {
    let exponential = BACKOFF_BASE.saturating_mul(1u32 << attempt.min(6));
    exponential
        .max(retry_after.unwrap_or(Duration::ZERO))
        .min(BACKOFF_CAP)
}

/// `Retry-After` in delta-seconds form.
///
/// The HTTP-date form is deliberately ignored: our own backoff is a safe
/// fallback, and parsing dates correctly would need a calendar we otherwise
/// have no use for.
fn parse_retry_after(headers: &HeaderMap) -> Option<Duration> {
    let value = headers.get(RETRY_AFTER)?.to_str().ok()?.trim();
    value.parse::<u64>().ok().map(Duration::from_secs)
}

/// Raw responses kept on disk under a caller-chosen name.
///
/// This is what makes a parser fix cheap: re-run against the cached HTML
/// instead of re-fetching the whole novel.
#[derive(Debug, Clone)]
pub struct RawCache {
    dir: PathBuf,
}

impl RawCache {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn path_for(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    pub fn has(&self, name: &str) -> bool {
        self.path_for(name).is_file()
    }

    pub fn get(&self, name: &str) -> Option<String> {
        std::fs::read_to_string(self.path_for(name)).ok()
    }

    pub fn put(&self, name: &str, body: &str) -> Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        std::fs::write(self.path_for(name), body)?;
        Ok(())
    }
}

/// One answer from a host: the body, and the facts about who sent it.
///
/// A body on its own cannot say *what* answered — a filter page and a chapter are
/// both text. `Page` keeps the status, the media type, and the URL that finally
/// answered, so a page that is not the site can be reported with what actually
/// arrived rather than guessed at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    /// Status of the final response, after any redirects.
    pub status: u16,
    /// The URL that was asked for.
    pub url: String,
    /// The URL that answered, after redirects. Differs from `url` when the site
    /// redirects to a canonical form — or when something that is not the site
    /// answers in its place.
    pub final_url: String,
    /// Content type as sent, parameters and all.
    pub content_type: Option<String>,
    pub body: String,
}

impl Page {
    /// The error for a page that is not the site's shape.
    ///
    /// `expected` names what was being looked for, article included — `a table of
    /// contents`, `an episode body`. `intercepted` is the finding from
    /// [`HttpClient::answers_every_url_the_same`], which the caller asks for,
    /// because the caller is what knows the page should have been the site.
    pub fn not_the_site(&self, expected: &str, intercepted: bool) -> Error {
        blocked::not_the_site(self, expected, intercepted)
    }
}

/// A polite HTTP client.
#[derive(Debug)]
pub struct HttpClient {
    client: reqwest::blocking::Client,
    policy: FetchPolicy,
    limiter: RateLimiter,
    /// Parsed rules per origin. A failure to fetch is cached too, so a broken
    /// `robots.txt` costs one attempt rather than one per chapter.
    robots: Mutex<HashMap<String, RobotsRules>>,
    raw: Option<RawCache>,
}

impl HttpClient {
    /// A client using `policy`, with no raw cache.
    pub fn new(policy: FetchPolicy) -> Result<Self> {
        Self::with_raw_cache(policy, None)
    }

    /// A client that stores every fetched body under `raw_dir` when given one.
    pub fn with_raw_cache(policy: FetchPolicy, raw_dir: Option<PathBuf>) -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .user_agent(policy.user_agent.clone())
            .gzip(true)
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(Error::ClientBuild)?;
        let limiter = policy.limiter();
        Ok(Self {
            client,
            policy,
            limiter,
            robots: Mutex::new(HashMap::new()),
            raw: raw_dir.map(RawCache::new),
        })
    }

    pub fn policy(&self) -> &FetchPolicy {
        &self.policy
    }

    /// Fetch `url`, and keep the facts that say who answered.
    ///
    /// A page that is not the site's shape still comes back `Ok`: only the
    /// adapter knows what the site's markup should look like, so that judgement
    /// belongs to the adapter — see [`Page::not_the_site`]. Cloudflare's browser
    /// challenge is the one answer refused here, because a header says so and
    /// nothing else sends it.
    pub fn get_page(&self, url: &str) -> Result<Page> {
        let response = self.send(url)?;
        let status = response.status().as_u16();
        let final_url = response.url().to_string();
        let content_type = content_type(&response);

        if let Some(page) = blocked::challenge(cf_mitigated(&response).as_deref()) {
            return Err(blocked_error(status, url, page));
        }

        let body = response.text().map_err(|source| Error::Network {
            url: url.to_string(),
            source,
        })?;

        Ok(Page {
            status,
            url: url.to_string(),
            final_url,
            content_type,
            body,
        })
    }

    /// Whether this host answers every URL with the same page.
    ///
    /// This is the one signal that tells "your network intercepted this" from
    /// "the site changed its markup" without knowing a word of any language. A
    /// site answers a path that cannot exist with its own not-found answer; a
    /// hijacked DNS answer, a captive portal, and most content filters answer
    /// every path with the same bytes.
    ///
    /// It costs a request, so it is asked only after a page has already failed to
    /// look like the site. `body` is that failed answer, and the comparison is
    /// byte for byte on purpose: a near-match would also match the soft-404 page
    /// many sites serve with `200`, which echoes the URL it was asked for and
    /// nothing else. A probe that cannot be made — a dead host, a disallowed
    /// path — answers `false`, never an error: it is evidence, not a fetch.
    pub fn answers_every_url_the_same(&self, url: &str, body: &str) -> bool {
        let Ok(parsed) = Url::parse(url) else {
            return false;
        };
        let probe = format!("{}{}", origin_of(&parsed), probe_path());
        match self.get_page(&probe) {
            Ok(answer) => answer.body == body,
            Err(_) => false,
        }
    }

    /// Fetch `url` as raw bytes, for images and other binary assets.
    pub fn get_bytes(&self, url: &str) -> Result<Vec<u8>> {
        let response = self.send(url)?;
        let bytes = response.bytes().map_err(|source| Error::Network {
            url: url.to_string(),
            source,
        })?;
        Ok(bytes.to_vec())
    }

    /// Like [`HttpClient::get_page`], but reuse a body already cached under
    /// `name` instead of going to the network.
    ///
    /// The body only: a cached page remembers no status, no headers and no
    /// redirects, because none of those were written down.
    pub fn get_text_cached(&self, url: &str, name: &str) -> Result<String> {
        if let Some(cache) = &self.raw {
            if let Some(body) = cache.get(name) {
                return Ok(body);
            }
        }
        let page = self.get_page(url)?;
        if let Some(cache) = &self.raw {
            cache.put(name, &page.body)?;
        }
        Ok(page.body)
    }

    /// One GET: rate limited, robots-checked, and retried.
    fn send(&self, url: &str) -> Result<reqwest::blocking::Response> {
        let parsed =
            Url::parse(url).map_err(|error| Error::InvalidUrl(format!("{url}: {error}")))?;
        let host = parsed.host_str().unwrap_or_default().to_string();

        let rules = self.rules_for(&parsed, &host);
        if !rules.is_allowed(&self.policy.user_agent, &path_and_query(&parsed)) {
            return Err(Error::RobotsDisallowed(url.to_string()));
        }
        // A site may ask for a longer delay than our own policy does; honor it.
        let delay = rules
            .crawl_delay(&self.policy.user_agent)
            .unwrap_or(Duration::ZERO)
            .max(self.policy.min_delay);

        let attempts = self.policy.max_retries.max(1);
        let mut last: Option<Error> = None;
        let mut retry_after: Option<Duration> = None;

        for attempt in 0..attempts {
            if attempt > 0 {
                std::thread::sleep(backoff_delay(attempt - 1, retry_after));
            }
            self.limiter.acquire_with(&host, delay);

            match self.client.get(url).send() {
                Ok(response) if response.status().is_success() => return Ok(response),
                Ok(response) => {
                    let status = response.status().as_u16();
                    if !is_retryable_status(status) {
                        // A `403` is usually Cloudflare rather than the site
                        // refusing us, and Cloudflare says so in a header. The
                        // body is not read: no phrase in it is trusted.
                        if let Some(page) = blocked::challenge(cf_mitigated(&response).as_deref()) {
                            return Err(blocked_error(status, url, page));
                        }
                        return Err(Error::Http {
                            status,
                            url: url.to_string(),
                        });
                    }
                    retry_after = parse_retry_after(response.headers());
                    last = Some(Error::Http {
                        status,
                        url: url.to_string(),
                    });
                }
                Err(source) => {
                    retry_after = None;
                    let transient = source.is_timeout()
                        || source.is_connect()
                        || source.is_request()
                        || source.is_body();
                    if !transient {
                        return Err(Error::Network {
                            url: url.to_string(),
                            source,
                        });
                    }
                    last = Some(Error::Network {
                        url: url.to_string(),
                        source,
                    });
                }
            }
        }

        Err(last.unwrap_or(Error::Http {
            status: 0,
            url: url.to_string(),
        }))
    }

    /// `robots.txt` rules for this URL's origin, fetched at most once per run.
    ///
    /// A site with no readable `robots.txt` is treated as unrestricted.
    fn rules_for(&self, url: &Url, host: &str) -> RobotsRules {
        if !self.policy.respect_robots {
            return RobotsRules::allow_all();
        }
        let origin = origin_of(url);
        if let Some(rules) = self
            .robots
            .lock()
            .expect("robots cache lock poisoned")
            .get(&origin)
            .cloned()
        {
            return rules;
        }

        // Be polite about the file itself, then remember the answer either way.
        self.limiter.acquire(host);
        let rules = match self.client.get(format!("{origin}/robots.txt")).send() {
            Ok(response) if response.status().is_success() => {
                RobotsRules::parse(&response.text().unwrap_or_default())
            }
            _ => RobotsRules::allow_all(),
        };
        self.robots
            .lock()
            .expect("robots cache lock poisoned")
            .insert(origin, rules.clone());
        rules
    }
}

/// Cloudflare's `Cf-Mitigated` header, when a response carries one.
fn cf_mitigated(response: &reqwest::blocking::Response) -> Option<String> {
    response
        .headers()
        .get("cf-mitigated")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

/// A response's content type, when it sent one.
fn content_type(response: &reqwest::blocking::Response) -> Option<String> {
    response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

/// The path asked for to see whether a host answers everything the same.
///
/// It cannot name a real page, and the nanosecond suffix keeps a proxy or a cache
/// from answering it out of its own memory.
fn probe_path() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.subsec_nanos())
        .unwrap_or(0);
    format!("/web-novel-downloader-does-not-exist-{nanos:x}")
}

/// The error a recognised block page becomes.
fn blocked_error(status: u16, url: &str, page: blocked::BlockPage) -> Error {
    Error::Blocked {
        status,
        url: url.to_string(),
        kind: page.kind,
        evidence: page.evidence,
    }
}

/// Scheme, host, and port of a URL — the key for robots rules.
fn origin_of(url: &Url) -> String {
    let host = url.host_str().unwrap_or_default();
    match url.port() {
        Some(port) => format!("{}://{host}:{port}", url.scheme()),
        None => format!("{}://{host}", url.scheme()),
    }
}

/// Path plus query, which is what robots rules are matched against.
fn path_and_query(url: &Url) -> String {
    match url.query() {
        Some(query) => format!("{}?{query}", url.path()),
        None => url.path().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("wnd-fetch-{tag}-{}-{nanos}", std::process::id()))
    }

    #[test]
    fn retryable_statuses_are_only_the_transient_ones() {
        for status in [408, 425, 429, 500, 502, 503, 504] {
            assert!(is_retryable_status(status), "{status} should be retried");
        }
        for status in [200, 204, 301, 400, 401, 403, 404, 410, 418] {
            assert!(!is_retryable_status(status), "{status} must not be retried");
        }
    }

    #[test]
    fn backoff_grows_exponentially_and_is_capped() {
        assert_eq!(backoff_delay(0, None), Duration::from_secs(1));
        assert_eq!(backoff_delay(1, None), Duration::from_secs(2));
        assert_eq!(backoff_delay(2, None), Duration::from_secs(4));
        assert_eq!(backoff_delay(9, None), BACKOFF_CAP);
    }

    #[test]
    fn retry_after_wins_when_it_is_longer_than_the_backoff() {
        assert_eq!(
            backoff_delay(0, Some(Duration::from_secs(10))),
            Duration::from_secs(10)
        );
        assert_eq!(
            backoff_delay(4, Some(Duration::from_secs(2))),
            Duration::from_secs(16),
            "the exponential step still applies when it is longer"
        );
        assert_eq!(
            backoff_delay(0, Some(Duration::from_secs(3600))),
            BACKOFF_CAP,
            "a hostile Retry-After must not stall us for an hour"
        );
    }

    #[test]
    fn retry_after_header_is_read_in_seconds_only() {
        let mut headers = HeaderMap::new();
        assert_eq!(parse_retry_after(&headers), None);

        headers.insert(RETRY_AFTER, reqwest::header::HeaderValue::from_static("7"));
        assert_eq!(parse_retry_after(&headers), Some(Duration::from_secs(7)));

        headers.insert(
            RETRY_AFTER,
            reqwest::header::HeaderValue::from_static("Wed, 21 Oct 2026 07:28:00 GMT"),
        );
        assert_eq!(
            parse_retry_after(&headers),
            None,
            "the HTTP-date form falls back to our own backoff"
        );
    }

    #[test]
    fn raw_cache_round_trips_and_misses_cleanly() {
        let dir = temp_dir("cache");
        let cache = RawCache::new(&dir);
        assert_eq!(cache.get("0001.html"), None);
        assert!(!cache.has("0001.html"));

        cache.put("0001.html", "<p>こんにちは</p>").unwrap();
        assert!(cache.has("0001.html"));
        assert_eq!(cache.get("0001.html").as_deref(), Some("<p>こんにちは</p>"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn origin_drops_path_and_query_but_keeps_the_port() {
        let url = Url::parse("https://ncode.syosetu.com/n0608gd/1/?p=2").unwrap();
        assert_eq!(origin_of(&url), "https://ncode.syosetu.com");
        assert_eq!(path_and_query(&url), "/n0608gd/1/?p=2");

        let url = Url::parse("http://example.com:8080/x").unwrap();
        assert_eq!(origin_of(&url), "http://example.com:8080");
        assert_eq!(path_and_query(&url), "/x");
    }

    #[test]
    fn the_probe_path_cannot_name_a_real_page() {
        let path = probe_path();

        assert!(
            path.starts_with("/web-novel-downloader-does-not-exist-"),
            "{path}"
        );
        assert!(
            !path.ends_with('-'),
            "the suffix must be there, so the path is never the same twice: {path}"
        );
    }

    #[test]
    fn an_unparseable_url_fails_before_any_request() {
        let policy = FetchPolicy {
            respect_robots: false,
            ..FetchPolicy::default()
        };
        let client = HttpClient::new(policy).unwrap();
        assert!(matches!(
            client.get_page("not a url"),
            Err(Error::InvalidUrl(_))
        ));
    }
}
