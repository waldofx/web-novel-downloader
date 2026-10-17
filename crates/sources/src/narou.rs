//! Narou-family adapter: `ncode.syosetu.com` and `novel18.syosetu.com`.
//!
//! The R18 sibling (`novel18.syosetu.com`) is believed to run the same backend
//! and markup, so one adapter serves both. Only URL routing is implemented so
//! far; fetching and parsing land in the next milestone.
//!
//! ## Ruby notation
//!
//! Narou encodes furigana in *plain text* rather than HTML:
//!
//! - `｜漢字《かんじ》` — explicit ruby, with `｜` marking the base's start
//! - `漢字《かんじ》` — implicit ruby, base auto-detected from the preceding kanji
//! - `《《強調》》` — the site's own emphasis
//!
//! These characters are markup, not punctuation, so parsing them into the node
//! model is part of this adapter's job. See `DESIGN.md`.

use crate::{Source, SourceError};
use url::Url;
use wnd_core::model::{Chapter, ChapterMeta, ContentRating, Manifest};

pub struct Narou;

/// Hosts this adapter is responsible for.
pub const HOSTS: [&str; 2] = ["ncode.syosetu.com", "novel18.syosetu.com"];

impl Narou {
    fn host_of(url: &Url) -> Option<String> {
        url.host_str().map(|h| h.to_ascii_lowercase())
    }
}

impl Source for Narou {
    fn id(&self) -> &'static str {
        "narou"
    }

    fn matches(&self, url: &Url) -> bool {
        matches!(url.scheme(), "http" | "https")
            && Self::host_of(url).is_some_and(|host| HOSTS.contains(&host.as_str()))
    }

    fn content_rating(&self) -> ContentRating {
        // `ncode` is general audience; `novel18` is adult. The distinction is
        // per-work, so it is resolved when parsing, not here.
        ContentRating::General
    }

    fn novel(&self, _url: &Url) -> Result<Manifest, SourceError> {
        Err(SourceError::NotImplemented)
    }

    fn chapter(&self, _chapter: &ChapterMeta) -> Result<Chapter, SourceError> {
        Err(SourceError::NotImplemented)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches(raw: &str) -> bool {
        Narou.matches(&Url::parse(raw).unwrap())
    }

    #[test]
    fn both_narou_family_hosts_are_matched() {
        assert!(matches("https://ncode.syosetu.com/n1234ab/"));
        assert!(matches("https://ncode.syosetu.com/n1234ab/1/"));
        assert!(matches("https://novel18.syosetu.com/n8309id/"));
    }

    #[test]
    fn host_matching_is_exact_and_case_insensitive() {
        assert!(matches("https://NCODE.SYOSETU.COM/n1234ab/"));
        // A lookalike domain must not be claimed.
        assert!(!matches("https://ncode.syosetu.com.evil.example/n1234ab/"));
        assert!(!matches("https://syosetu.com/n1234ab/"));
    }

    #[test]
    fn other_schemes_and_hosts_are_not_matched() {
        assert!(!matches("ftp://ncode.syosetu.com/n1234ab/"));
        assert!(!matches("https://syosetu.org/novel/288118/"));
        assert!(!matches("https://kakuyomu.jp/works/1234"));
    }
}
