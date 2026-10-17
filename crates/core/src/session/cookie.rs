//! What a cookie is, and where it may be sent.
//!
//! The rules that matter here are the ones a browser applies before it puts a
//! cookie on a request, written out so they can be tested without a browser and
//! without a socket:
//!
//! - **host** — a cookie for `pixiv.net` may go to `www.pixiv.net`, but a suffix
//!   is not a domain (`notpixiv.net`), and an IP address only matches itself;
//! - **path** — `/novel` covers `/novel/1` but not `/novels`;
//! - **secure** — a `Secure` cookie never travels over `http://`;
//! - **expiry** — a cookie past its expiry is not sent, and one with no expiry at
//!   all (a session cookie) is sent until it goes.
//!
//! Nothing here ever prints a value: [`Cookie`] has a hand-written [`Debug`] that
//! redacts it, because a cookie is a credential and `{:?}` shows up in more
//! places than anyone intends.

use reqwest::Url;
use std::fmt;
use std::net::IpAddr;

/// One cookie, as a browser export writes it.
#[derive(Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Cookie {
    /// The domain the cookie applies to, without a leading dot.
    pub domain: String,
    /// True when the cookie was set for one host and must not follow a
    /// subdomain — the `FALSE` in the export's "include subdomains" column.
    pub host_only: bool,
    pub path: String,
    pub secure: bool,
    /// Unix seconds at which it expires, or `None` for a session cookie.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u64>,
    pub name: String,
    pub value: String,
}

impl Cookie {
    /// Whether this cookie may be sent to `host`.
    pub fn matches_host(&self, host: &str) -> bool {
        let domain = self.domain.trim_start_matches('.');
        if self.host_only {
            host.trim_end_matches('.')
                .eq_ignore_ascii_case(domain.trim_end_matches('.'))
        } else {
            host_is_covered(domain, host)
        }
    }

    /// Whether this cookie may be sent with a request for `path`.
    pub fn matches_path(&self, path: &str) -> bool {
        if self.path == path {
            return true;
        }
        if !path.starts_with(&self.path) {
            return false;
        }
        // `/novel` must not match `/novels`: the prefix has to end at a boundary.
        self.path.ends_with('/') || path.as_bytes().get(self.path.len()) == Some(&b'/')
    }

    /// Whether this cookie is still good at `now` (Unix seconds).
    pub fn is_alive_at(&self, now: u64) -> bool {
        match self.expires_at {
            // An expiry *at* `now` is reached, so the cookie is not sent again.
            Some(at) => now < at,
            None => true,
        }
    }
}

/// Whether the host `named` — a session's host, or a cookie's domain — covers
/// the host `host`.
///
/// The same rule as a cookie's domain, and shared on purpose: a session for
/// `pixiv.net` is not offered to `i.pximg.net`, which is a different site that
/// merely happens to serve the images.
pub fn host_is_covered(named: &str, host: &str) -> bool {
    let named = named.trim_matches('.');
    let host = host.trim_end_matches('.');
    if named.is_empty() || host.is_empty() {
        return false;
    }
    if host.eq_ignore_ascii_case(named) {
        return true;
    }
    // A suffix is not a domain, and an IP address has no subdomains at all.
    if named.parse::<IpAddr>().is_ok() {
        return false;
    }
    let start = match host.len().checked_sub(named.len()) {
        Some(start) if start > 0 => start,
        _ => return false,
    };
    match host.get(start..) {
        Some(suffix) if suffix.eq_ignore_ascii_case(named) => {
            host.as_bytes().get(start - 1) == Some(&b'.')
        }
        _ => false,
    }
}

/// The value of the `Cookie` header for a request to `url`, or `None` when no
/// cookie applies.
///
/// Cookies are ordered longest path first, as a browser sends them, so a more
/// specific cookie wins the tie when a server reads only the first.
pub fn cookie_header(cookies: &[Cookie], url: &Url, now: u64) -> Option<String> {
    let host = url.host_str()?;
    let over_tls = matches!(url.scheme(), "https" | "wss");
    let path = url.path();

    let mut matched: Vec<&Cookie> = cookies
        .iter()
        .filter(|cookie| {
            cookie.matches_host(host)
                && cookie.matches_path(path)
                && cookie.is_alive_at(now)
                && (!cookie.secure || over_tls)
        })
        .collect();
    if matched.is_empty() {
        return None;
    }

    matched.sort_by_key(|cookie| std::cmp::Reverse(cookie.path.len()));
    Some(
        matched
            .iter()
            .map(|cookie| format!("{}={}", cookie.name, cookie.value))
            .collect::<Vec<_>>()
            .join("; "),
    )
}

impl fmt::Debug for Cookie {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Cookie")
            .field("domain", &self.domain)
            .field("host_only", &self.host_only)
            .field("path", &self.path)
            .field("secure", &self.secure)
            .field("expires_at", &self.expires_at)
            .field("name", &self.name)
            .field("value", &"<redacted>")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fixed clock, so no test depends on the wall clock.
    const NOW: u64 = 1_700_000_000;

    fn a_cookie(name: &str, value: &str) -> Cookie {
        Cookie {
            domain: "pixiv.net".to_string(),
            host_only: false,
            path: "/".to_string(),
            secure: true,
            expires_at: None,
            name: name.to_string(),
            value: value.to_string(),
        }
    }

    fn at(url: &str) -> Url {
        Url::parse(url).expect("a URL")
    }

    #[test]
    fn a_cookie_is_only_sent_to_the_host_it_was_set_for() {
        let cookie = a_cookie("PHPSESSID", "v");

        assert!(cookie.matches_host("pixiv.net"));
        assert!(cookie.matches_host("www.pixiv.net"));
        assert!(
            cookie.matches_host("WWW.PIXIV.NET"),
            "hosts are case-insensitive"
        );

        assert!(
            !cookie.matches_host("notpixiv.net"),
            "a suffix is not a domain"
        );
        assert!(!cookie.matches_host("pixiv.net.evil.example"));
        assert!(!cookie.matches_host("pxiv.net"));
        assert!(!cookie.matches_host(""));
    }

    #[test]
    fn a_host_only_cookie_stops_at_the_exact_host() {
        let cookie = Cookie {
            host_only: true,
            ..a_cookie("PHPSESSID", "v")
        };

        assert!(cookie.matches_host("pixiv.net"));
        assert!(!cookie.matches_host("www.pixiv.net"));
        assert!(!cookie.matches_host("i.pximg.net"));
    }

    #[test]
    fn an_ip_address_gets_an_exact_match_only() {
        let cookie = Cookie {
            domain: "127.0.0.1".to_string(),
            ..a_cookie("PHPSESSID", "v")
        };

        assert!(cookie.matches_host("127.0.0.1"));
        assert!(!cookie.matches_host("www.127.0.0.1"));
        assert!(!cookie.matches_host("1.127.0.0.1"));
    }

    #[test]
    fn a_secure_cookie_never_travels_over_http() {
        let secure = a_cookie("PHPSESSID", "v");
        assert_eq!(
            cookie_header(
                std::slice::from_ref(&secure),
                &at("https://pixiv.net/"),
                NOW
            )
            .as_deref(),
            Some("PHPSESSID=v")
        );
        assert_eq!(
            cookie_header(&[secure], &at("http://pixiv.net/"), NOW),
            None,
            "the same cookie must not go out in the clear"
        );

        let plain = Cookie {
            secure: false,
            ..a_cookie("plain", "v")
        };
        assert_eq!(
            cookie_header(&[plain], &at("http://pixiv.net/"), NOW).as_deref(),
            Some("plain=v"),
            "an unmarked cookie travels both ways"
        );
    }

    #[test]
    fn an_expired_cookie_is_not_sent_again() {
        let dead = Cookie {
            expires_at: Some(NOW),
            ..a_cookie("dead", "v")
        };
        let alive = Cookie {
            expires_at: Some(NOW + 1),
            ..a_cookie("alive", "v")
        };
        let session = a_cookie("session", "v");

        assert!(!dead.is_alive_at(NOW), "expiry is reached, not passed");
        assert!(alive.is_alive_at(NOW));
        assert!(session.is_alive_at(NOW), "a session cookie has no expiry");

        assert_eq!(
            cookie_header(&[dead, alive, session], &at("https://pixiv.net/"), NOW).as_deref(),
            Some("alive=v; session=v")
        );
    }

    #[test]
    fn the_longest_path_is_sent_first() {
        let root = a_cookie("a", "1");
        let deep = Cookie {
            path: "/novel/1".to_string(),
            name: "b".to_string(),
            value: "2".to_string(),
            ..a_cookie("ignored", "ignored")
        };

        assert_eq!(
            cookie_header(&[root, deep], &at("https://pixiv.net/novel/1/x"), NOW).as_deref(),
            Some("b=2; a=1")
        );
    }

    #[test]
    fn a_cookie_for_a_sibling_path_is_not_sent() {
        let cookie = Cookie {
            path: "/novel".to_string(),
            ..a_cookie("a", "1")
        };

        assert!(cookie.matches_path("/novel"));
        assert!(cookie.matches_path("/novel/1"));
        assert!(
            !cookie.matches_path("/novels"),
            "/novel is not a prefix of /novels"
        );
        assert!(!cookie.matches_path("/"));

        let trailing = Cookie {
            path: "/novel/".to_string(),
            ..a_cookie("a", "1")
        };
        assert!(trailing.matches_path("/novel/1"));
        assert!(
            !trailing.matches_path("/novel"),
            "a trailing slash is part of the path"
        );
    }

    #[test]
    fn a_request_no_cookie_applies_to_has_no_header() {
        let cookies = [a_cookie("PHPSESSID", "v")];
        assert_eq!(
            cookie_header(&cookies, &at("https://example.com/"), NOW),
            None
        );
        assert_eq!(cookie_header(&[], &at("https://pixiv.net/"), NOW), None);
    }

    #[test]
    fn a_named_host_covers_its_subdomains_and_nothing_else() {
        assert!(host_is_covered("pixiv.net", "pixiv.net"));
        assert!(host_is_covered("pixiv.net", "www.pixiv.net"));
        assert!(host_is_covered("PIXIV.NET", "www.pixiv.net"));
        assert!(
            host_is_covered(".pixiv.net", "www.pixiv.net"),
            "an export writes the dot"
        );

        assert!(
            !host_is_covered("pixiv.net", "i.pximg.net"),
            "another site's images"
        );
        assert!(!host_is_covered("pixiv.net", "notpixiv.net"));
        assert!(!host_is_covered("pixiv.net", "pixiv.net.evil.example"));
        assert!(!host_is_covered("127.0.0.1", "www.127.0.0.1"));
    }

    #[test]
    fn a_cookie_never_prints_its_value() {
        let cookie = a_cookie("PHPSESSID", "hunter2");

        let debug = format!("{cookie:?}");
        assert!(
            debug.contains("pixiv.net"),
            "the domain is not a secret: {debug}"
        );
        assert!(
            debug.contains("PHPSESSID"),
            "the name is not a secret: {debug}"
        );
        assert!(!debug.contains("hunter2"), "the value is: {debug}");
    }
}
