//! Site adapters.
//!
//! Each site (or family of sites sharing a platform) gets one module —
//! a folder, once it has enough parts to need one — implementing [`Source`].
//! Adapters rot as sites change their HTML, so they are kept small, isolated,
//! and tested against captured pages in `tests/fixtures/`.
//!
//! - [`narou`] — `ncode.syosetu.com` and its R18 sibling `novel18.syosetu.com`,
//!   split into table-of-contents, chapter, inline, and DOM parts.
//! - [`kakuyomu`] — `kakuyomu.jp`, Kadokawa's site, read from the page's
//!   Next.js snapshot where the HTML alone would be incomplete.
//!
//! The parts every site shares are not in the adapters: [`dom`] pulls text out
//! of HTML, [`blocks`] applies the blank-line convention, [`inline`] decodes the
//! inline markup (`<ruby>`, `<strong>`, `<a>`), and [`selectors`] holds the
//! macro that keeps a selector literal out of the request path. An adapter
//! supplies only what is peculiar to its site.

pub mod kakuyomu;
pub mod narou;

mod blocks;
mod dom;
mod inline;
mod selectors;

#[cfg(test)]
mod fixtures;

use crate::kakuyomu::Kakuyomu;
use crate::narou::Narou;
use url::Url;
use wnd_core::HttpClient;
use wnd_core::{Chapter, ChapterMeta, ContentRating, Manifest};

/// A website we can download novels from.
pub trait Source: Send + Sync {
    /// Stable adapter id, e.g. `narou`. Stored in the manifest.
    fn id(&self) -> &'static str;

    /// Whether this adapter handles the given URL.
    fn matches(&self, url: &Url) -> bool;

    /// Content rating reported by the adapter, before any per-work refinement.
    ///
    /// Adult-only sites return [`ContentRating::Adult`] so the CLI can refuse
    /// them unless the user opted in.
    fn content_rating(&self) -> ContentRating;

    /// Fetch the novel's metadata and chapter index.
    fn novel(&self, fetch: &HttpClient, url: &Url) -> Result<Manifest, SourceError>;

    /// Fetch and parse a single chapter.
    fn chapter(&self, fetch: &HttpClient, chapter: &ChapterMeta) -> Result<Chapter, SourceError>;
}

/// Everything known at build time about the adapters we ship.
///
/// Order matters: the first adapter whose [`Source::matches`] returns true wins.
pub fn registry() -> Vec<Box<dyn Source>> {
    vec![Box::new(Narou), Box::new(Kakuyomu)]
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

    #[error(transparent)]
    Core(#[from] wnd_core::Error),

    #[error("{0}")]
    Other(String),
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
