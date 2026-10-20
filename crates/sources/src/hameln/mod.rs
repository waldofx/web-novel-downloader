//! Hameln adapter: `syosetu.org` and its R18 sibling `h.syosetu.org`.
//!
//! A different platform from Narou (different host, different HTML), but the
//! same shape of job: a work page with metadata, arcs and an episode list, and
//! one page per chapter. The R18 sibling runs the same backend, so one adapter
//! serves both; only the content rating differs, and that is derived from the
//! host rather than from the page.

mod chapter;
mod lines;
mod toc;

pub use chapter::parse_chapter;
pub use toc::Toc;
pub use toc::parse_toc;

use crate::selectors::selector;
use crate::{Source, SourceError};
use scraper::Selector;
use std::sync::LazyLock;
use url::Url;
use wnd_core::{Chapter, ChapterMeta, ContentRating, HttpClient, Manifest, SourceInfo};

pub struct Hameln;

/// Hosts this adapter is responsible for.
pub const HOSTS: [&str; 2] = ["syosetu.org", "h.syosetu.org"];

/// The R18 host: works served from it are adult content.
pub const ADULT_HOST: &str = "h.syosetu.org";

selector!(SEL_TITLE, r#"span[itemprop="name"]"#);
// The site's big-text span: the work's title in the page header, and the
// chapter's own title just above the body. Which one is the chapter's is
// `chapter`'s question to answer.
selector!(SEL_TITLE_SPAN, r#"span[style*="font-size:120%"]"#);
// The author's 後書き. The site's toggle link for it is a *different* element
// (`#atogaki_open`), so this is selected exactly: a container that covered the
// link would put the site's words in the author's note. A chapter page carries
// no container for a 前書き at all.
selector!(SEL_AFTERWORD, "#atogaki");
selector!(SEL_AUTHOR, r#"span[itemprop="author"]"#);
selector!(SEL_TAGS, r#"a[href*="mode=search"]"#);
selector!(SEL_SS, "div.ss");
selector!(SEL_EPISODE_LIST, "ul.episode-list__items");
selector!(SEL_EPISODE_LINK, "a.episode-list__link");
selector!(SEL_CHAPTER_TITLE, "div.episode-list__chapter-title");
selector!(SEL_EPISODE_TITLE, "span.episode-list__title");
selector!(SEL_EPISODE_DATE, "time.episode-list__date");
selector!(SEL_EPISODE_REVISION, "span.episode-list__revision");

impl Hameln {
    fn host_of(url: &Url) -> Option<String> {
        url.host_str().map(|h| h.to_ascii_lowercase())
    }

    /// The canonical table-of-contents URL for a novel, from any URL on it.
    fn canonical_novel_url(url: &Url) -> Option<Url> {
        if !matches!(url.scheme(), "http" | "https") {
            return None;
        }
        let host = url.host_str()?.to_ascii_lowercase();
        if !HOSTS.contains(&host.as_str()) {
            return None;
        }
        let segments: Vec<&str> = url.path_segments()?.filter(|s| !s.is_empty()).collect();
        // Expect `/novel/<id>/...`; the id is the segment after "novel".
        let novel_pos = segments.iter().position(|s| *s == "novel")?;
        let novel_id = segments.get(novel_pos + 1)?;
        Url::parse(&format!("{}://{host}/novel/{novel_id}/", url.scheme())).ok()
    }
}

impl Source for Hameln {
    fn id(&self) -> &'static str {
        "hameln"
    }

    fn matches(&self, url: &Url) -> bool {
        matches!(url.scheme(), "http" | "https")
            && Self::host_of(url).is_some_and(|host| HOSTS.contains(&host.as_str()))
    }

    fn content_rating(&self) -> ContentRating {
        ContentRating::General
    }

    fn novel(&self, fetch: &HttpClient, url: &Url) -> Result<Manifest, SourceError> {
        let base = Self::canonical_novel_url(url).ok_or_else(|| SourceError::Parse {
            url: url.to_string(),
            message: "not a Hameln novel URL".to_string(),
        })?;

        let page = fetch.get_page(base.as_str())?;
        let parsed = parse_toc(&page.body, &base);

        if parsed.title.is_none() {
            return Err(SourceError::NotTheSite {
                url: base.to_string(),
                expected: "a table of contents".to_string(),
            }
            .with_evidence(fetch, &page));
        }

        let title = parsed.title.expect("checked above");
        let mut manifest = Manifest::new(
            SourceInfo {
                id: self.id().to_string(),
                novel_url: base.to_string(),
                site_title: None,
            },
            title,
            "ja",
        );
        manifest.author = parsed.author;
        manifest.synopsis = parsed.synopsis;
        manifest.tags = parsed.tags;
        manifest.content_rating = rating_for(&base);
        manifest.chapters = parsed.entries;
        Ok(manifest)
    }

    fn chapter(&self, fetch: &HttpClient, meta: &ChapterMeta) -> Result<Chapter, SourceError> {
        let page = fetch.get_page(&meta.url)?;
        chapter::parse_chapter(&page.body, meta).map_err(|error| error.with_evidence(fetch, &page))
    }
}

fn rating_for(url: &Url) -> ContentRating {
    match url.host_str() {
        Some(host) if host.eq_ignore_ascii_case(ADULT_HOST) => ContentRating::Adult,
        _ => ContentRating::General,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use url::Url;

    fn matches(raw: &str) -> bool {
        Hameln.matches(&Url::parse(raw).unwrap())
    }

    #[test]
    fn both_hosts_are_matched() {
        assert!(matches("https://syosetu.org/novel/000000/"));
        assert!(matches("https://syosetu.org/novel/000000/1.html"));
        assert!(matches("https://h.syosetu.org/novel/000001/"));
    }

    #[test]
    fn host_matching_is_exact_and_case_insensitive() {
        assert!(matches("https://SYOSETU.ORG/novel/000000/"));
        assert!(!matches("https://syosetu.org.evil.example/novel/000000/"));
        assert!(!matches("https://ncode.syosetu.com/n0000aa/"));
        assert!(!matches("https://kakuyomu.jp/works/1234"));
    }

    #[test]
    fn other_schemes_are_not_matched() {
        assert!(!matches("ftp://syosetu.org/novel/000000/"));
    }

    #[test]
    fn adult_host_selects_the_adult_rating() {
        assert_eq!(
            rating_for(&Url::parse("https://h.syosetu.org/novel/000001/").unwrap()),
            ContentRating::Adult
        );
        assert_eq!(
            rating_for(&Url::parse("https://syosetu.org/novel/000000/").unwrap()),
            ContentRating::General
        );
    }

    #[test]
    fn canonical_url_reduces_a_chapter_to_its_work() {
        assert_eq!(
            Hameln::canonical_novel_url(
                &Url::parse("https://syosetu.org/novel/000000/1.html").unwrap()
            )
            .unwrap()
            .as_str(),
            "https://syosetu.org/novel/000000/"
        );
        assert_eq!(
            Hameln::canonical_novel_url(&Url::parse("https://syosetu.org/novel/000000/").unwrap())
                .unwrap()
                .as_str(),
            "https://syosetu.org/novel/000000/"
        );
    }
}
