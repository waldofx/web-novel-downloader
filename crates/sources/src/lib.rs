//! Site adapters.
//!
//! Each site (or family of sites sharing a platform) gets one module —
//! a folder, once it has enough parts to need one — implementing [`Source`].
//! Adapters rot as sites change their HTML, so they are kept small, isolated,
//! and tested against captured pages in `tests/fixtures/`.
//!
//! - [`narou`] — `ncode.syosetu.com` and its R18 sibling `novel18.syosetu.com`,
//!   split into table-of-contents, chapter, inline, and DOM parts.
//! - [`hameln`] — `syosetu.org` and its R18 sibling `h.syosetu.org`, a different
//!   platform from Narou.
//! - [`kakuyomu`] — `kakuyomu.jp`, Kadokawa's site, read from the page's
//!   Next.js snapshot where the HTML alone would be incomplete.
//! - [`pixiv`] — `pixiv.net`, read from the site's JSON API rather than from its
//!   pages, which are a JavaScript app over that API.
//!
//! The parts every site shares are not in the adapters: [`dom`] pulls text out
//! of HTML, [`blocks`] applies the blank-line convention, [`inline`] decodes the
//! inline markup (`<ruby>`, `<strong>`, `<a>`), and [`selectors`] holds the
//! macro that keeps a selector literal out of the request path. An adapter
//! supplies only what is peculiar to its site.
//!
//! When the shape an adapter needs is not there at all, the parser says so with
//! [`SourceError::NotTheSite`] and the adapter's `novel`/`chapter` asks the fetch
//! layer what actually answered ([`SourceError::with_evidence`]). Nothing in
//! either crate matches block-page text: the page is quoted, not recognised.

pub mod hameln;
pub mod kakuyomu;
pub mod narou;
pub mod pixiv;

mod blocks;
mod dom;
mod inline;
mod selectors;

#[cfg(test)]
mod fixtures;

use crate::hameln::Hameln;
use crate::kakuyomu::Kakuyomu;
use crate::narou::Narou;
use crate::pixiv::Pixiv;
use url::Url;
use wnd_core::HttpClient;
use wnd_core::{Chapter, ChapterMeta, ContentRating, Manifest, Page};

/// A website we can download novels from.
pub trait Source: Send + Sync {
    /// Stable adapter id, e.g. `narou`. Stored in the manifest.
    fn id(&self) -> &'static str;

    /// Whether this adapter handles the given URL.
    fn matches(&self, url: &Url) -> bool;

    /// Content rating reported by the adapter, before any per-work refinement.
    ///
    /// Recorded for information; it does not gate anything. Each site enforces
    /// its own age gate, and the tool does not bypass it.
    fn content_rating(&self) -> ContentRating;

    /// The `Referer` this site's images have to be requested with, if any.
    ///
    /// Pixiv serves its illustrations from `i.pximg.net`, which answers `403` to
    /// a request that does not say which page it came from. That is not a
    /// disguise — it is the page a browser would have been on — and the request
    /// is otherwise the same one the policy already allows, `robots.txt` and the
    /// per-host delay included. Every other adapter leaves this alone.
    fn image_referer(&self) -> Option<&'static str> {
        None
    }

    /// Fetch the novel's metadata and chapter index.
    fn novel(&self, fetch: &HttpClient, url: &Url) -> Result<Manifest, SourceError>;

    /// Fetch and parse a single chapter.
    fn chapter(&self, fetch: &HttpClient, chapter: &ChapterMeta) -> Result<Chapter, SourceError>;
}

/// Everything known at build time about the adapters we ship.
///
/// Order matters: the first adapter whose [`Source::matches`] returns true wins.
pub fn registry() -> Vec<Box<dyn Source>> {
    vec![
        Box::new(Narou),
        Box::new(Kakuyomu),
        Box::new(Hameln),
        Box::new(Pixiv),
    ]
}

/// Find the adapter responsible for a URL.
pub fn find<'a>(sources: &'a [Box<dyn Source>], url: &Url) -> Option<&'a dyn Source> {
    sources
        .iter()
        .map(|s| s.as_ref())
        .find(|source| source.matches(url))
}

#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("this source is not implemented yet")]
    NotImplemented,

    /// The page loaded, but its shape was not what the adapter expects.
    ///
    /// Almost always means the site changed its HTML and the adapter needs a
    /// fix, not that the user did anything wrong.
    #[error("could not parse {url}: {message}")]
    Parse { url: String, message: String },

    /// The page that arrived is not this site's shape at all.
    ///
    /// Either something else answered in the site's place — a filter, a captive
    /// portal, a proxy — or the site has changed its markup. A parser raises this
    /// without the evidence, because a parser is a pure function over a body; the
    /// caller that has the [`Page`] turns it into what the user reads with
    /// [`SourceError::with_evidence`]. `expected` names what was being looked for,
    /// article included: `a table of contents`, `an episode body`.
    #[error("the page that arrived is not a novel page: {expected} was expected")]
    NotTheSite { url: String, expected: String },

    #[error(transparent)]
    Core(#[from] wnd_core::Error),

    #[error("{0}")]
    Other(String),
}

impl SourceError {
    /// Give a [`SourceError::NotTheSite`] the evidence the page carries.
    ///
    /// Called from an adapter's `novel` and `chapter`, the two places where the
    /// page and the client are both in hand:
    ///
    /// ```ignore
    /// parse(&page.body, meta).map_err(|error| error.with_evidence(fetch, &page))
    /// ```
    ///
    /// Anything that is not a `NotTheSite` is passed through untouched, so this
    /// can wrap a whole parse call. The host probe runs from here — once, on the
    /// failure path — because the caller is what knows the page should have been
    /// the site.
    pub fn with_evidence(self, fetch: &HttpClient, page: &Page) -> Self {
        match self {
            Self::NotTheSite { url, expected } => Self::Core(page.not_the_site(
                &expected,
                fetch.answers_every_url_the_same(&url, &page.body),
            )),
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wnd_core::{BlockKind, Error, FetchPolicy, Page};

    /// A page that is not the site, with the facts a real fetch attaches.
    fn unexpected_page() -> Page {
        Page {
            status: 200,
            url: "http://127.0.0.1:1/chapter".to_string(),
            final_url: "http://127.0.0.1:1/chapter".to_string(),
            content_type: Some("text/html; charset=utf-8".to_string()),
            body: "<html><body><h1>BU SİTEYE ERİŞİM ENGELLENMİŞTİR</h1></body></html>".to_string(),
        }
    }

    /// A client for a port with nothing behind it: the host probe fails at once,
    /// which is what a test wants. Proving the probe itself is `crates/core`'s
    /// job, not this crate's.
    fn client() -> HttpClient {
        HttpClient::new(FetchPolicy {
            respect_robots: false,
            min_delay: std::time::Duration::ZERO,
            max_retries: 1,
            ..FetchPolicy::default()
        })
        .expect("a client")
    }

    #[test]
    fn a_not_the_site_failure_gets_the_evidence_the_page_carries() {
        let page = unexpected_page();
        let error = SourceError::NotTheSite {
            url: page.url.clone(),
            expected: "a table of contents".to_string(),
        }
        .with_evidence(&client(), &page);

        match error {
            SourceError::Core(Error::Blocked {
                status,
                kind,
                evidence,
                ..
            }) => {
                assert_eq!(status, 200, "the filter says 200, and so do we");
                assert_eq!(kind, BlockKind::NotANovelPage);
                assert!(
                    evidence.contains("a table of contents was expected"),
                    "got: {evidence}"
                );
                assert!(
                    evidence.contains("BU SİTEYE ERİŞİM ENGELLENMİŞTİR"),
                    "the page's own words are the evidence: {evidence}"
                );
                assert!(
                    !evidence.contains("cannot exist"),
                    "a host that was never reached has not been shown to intercept: {evidence}"
                );
            }
            other => panic!("expected a reported page, got {other:?}"),
        }
    }

    #[test]
    fn every_other_failure_is_passed_through_untouched() {
        let page = unexpected_page();

        let error = SourceError::NotImplemented.with_evidence(&client(), &page);

        assert!(
            matches!(error, SourceError::NotImplemented),
            "only a page that is not the site is enriched: {error:?}"
        );
    }

    #[test]
    fn registry_routes_japanese_novel_sites() {
        let sources = registry();
        for (raw, id) in [
            ("https://ncode.syosetu.com/n1234ab/", "narou"),
            ("https://novel18.syosetu.com/n8309id/", "narou"),
            ("https://kakuyomu.jp/works/16816452221336015212", "kakuyomu"),
            (
                "https://kakuyomu.jp/works/16816452221336015212/episodes/16816452221336047787",
                "kakuyomu",
            ),
            ("https://syosetu.org/novel/316401/", "hameln"),
            ("https://h.syosetu.org/novel/288118/", "hameln"),
            ("https://www.pixiv.net/novel/show.php?id=14929293", "pixiv"),
            ("https://www.pixiv.net/novel/series/11074941", "pixiv"),
        ] {
            let url = Url::parse(raw).unwrap();
            let found = find(&sources, &url).expect("adapter should handle this URL");
            assert_eq!(found.id(), id, "input: {raw}");
        }
    }

    #[test]
    fn unknown_sites_are_not_claimed() {
        let sources = registry();
        let url = Url::parse("https://example.com/novel/1").unwrap();
        assert!(find(&sources, &url).is_none());
    }

    #[test]
    fn only_the_site_that_needs_a_referer_declares_one() {
        // `i.pximg.net` answers 403 without one. Nothing else we speak to asks,
        // and inventing a referer for a host that never did would be a claim
        // about where the request came from that is not true.
        let sources = registry();
        let telling: Vec<&str> = sources
            .iter()
            .filter(|source| source.image_referer().is_some())
            .map(|source| source.id())
            .collect();
        assert_eq!(telling, vec!["pixiv"]);

        let pixiv = find(
            &sources,
            &Url::parse("https://www.pixiv.net/novel/show.php?id=1").unwrap(),
        )
        .expect("pixiv handles its own URLs");
        assert_eq!(pixiv.image_referer(), Some("https://www.pixiv.net/"));
    }
}
