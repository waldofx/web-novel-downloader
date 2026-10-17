//! Narou-family adapter: `ncode.syosetu.com` and `novel18.syosetu.com`.
//!
//! The R18 sibling runs the same backend and markup, so one adapter serves both;
//! only the content rating differs, and that is derived from the host rather
//! than from the page.
//!
//! The adapter is split by job, because the markup is fiddly and each part has
//! its own traps:
//!
//! - [`toc`] — the table of contents: metadata, arcs, chapter rows, dates, `?p=` pagination
//! - [`chapter`] — one chapter page: notes and body, and Narou's blank-line rule
//! - [`inline`] — inline content: ruby, emphasis, links, and the plain-text
//!   notation `｜漢字《かんじ》` / `漢字《かんじ》` / `《《強調》》`
//! - [`dom`] — the mechanical DOM helpers the three above share
//!
//! Three things here are easy to undo by accident, and all three lose data:
//!
//! - Narou indents with U+3000. That is content, so every trim is ASCII-only.
//! - A `｜` or `《` with no usable base stays literal rather than being dropped.
//! - A chapter page with no text container is a parse error, not an empty
//!   chapter — silence there would hide adapter rot.

mod chapter;
mod inline;
mod toc;

use crate::selectors::selector;
use crate::{Source, SourceError};
use scraper::Selector;
use std::collections::HashSet;
use std::sync::LazyLock;
use url::Url;
use wnd_core::store::storage::chapter_file_name;
use wnd_core::{Chapter, ChapterMeta, ContentRating, HttpClient, Manifest, SourceInfo};

use toc::parse_toc_page;

pub struct Narou;

/// Hosts this adapter is responsible for.
pub const HOSTS: [&str; 2] = ["ncode.syosetu.com", "novel18.syosetu.com"];

/// The R18 host: works served from it are adult content.
pub const ADULT_HOST: &str = "novel18.syosetu.com";

/// Stop following `?p=` links after this many pages, so a misread pager or a
/// site bug cannot spin forever.
const MAX_TOC_PAGES: usize = 100;

// Selectors for Narou's markup, parsed once.
selector!(SEL_TITLE, "h1.p-novel__title");
selector!(SEL_AUTHOR, ".p-novel__author");
selector!(SEL_SUMMARY, ".p-novel__summary");
selector!(
    SEL_TOC_ENTRY,
    ".p-eplist__chapter-title, .p-eplist__subtitle"
);
selector!(SEL_UPDATE, ".p-eplist__update");
selector!(SEL_PAGER_NEXT, ".c-pager__item--next");
selector!(SEL_TEXT_BLOCK, "div.p-novel__text");
selector!(SEL_PARAGRAPH, "p");
selector!(SEL_ANCHOR, "a");

impl Narou {
    fn host_of(url: &Url) -> Option<String> {
        url.host_str().map(|h| h.to_ascii_lowercase())
    }

    /// The canonical table-of-contents URL for a novel, from any URL on it.
    ///
    /// `…/n0608gd/5/` and `…/n0608gd/?p=2` both reduce to `…/n0608gd/`, so a
    /// user can paste whatever they have open.
    fn canonical_novel_url(url: &Url) -> Option<Url> {
        if !matches!(url.scheme(), "http" | "https") {
            return None;
        }
        let host = url.host_str()?.to_ascii_lowercase();
        if !HOSTS.contains(&host.as_str()) {
            return None;
        }
        let ncode = url.path_segments()?.find(|segment| !segment.is_empty())?;
        Url::parse(&format!("{}://{host}/{ncode}/", url.scheme())).ok()
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
        // per-host, so it is resolved when the manifest is built.
        ContentRating::General
    }

    fn novel(&self, fetch: &HttpClient, url: &Url) -> Result<Manifest, SourceError> {
        let base = Self::canonical_novel_url(url).ok_or_else(|| SourceError::Parse {
            url: url.to_string(),
            message: "not a Narou novel URL".to_string(),
        })?;

        let mut title = None;
        let mut author = None;
        let mut synopsis = None;
        let mut chapters: Vec<ChapterMeta> = Vec::new();
        let mut arc = None;
        let mut index = 0u32;
        let mut visited: HashSet<String> = HashSet::new();
        let mut next = Some(base.to_string());

        // Novels over 100 chapters paginate the TOC with `?p=2`, `?p=3`, …
        while let Some(page_url) = next.take() {
            if !visited.insert(page_url.clone()) || visited.len() > MAX_TOC_PAGES {
                break;
            }
            let html = fetch.get_text(&page_url)?;
            let page_base = Url::parse(&page_url).unwrap_or_else(|_| base.clone());
            let page = parse_toc_page(&html, &page_base, arc, index);

            title = title.or(page.title);
            author = author.or(page.author);
            synopsis = synopsis.or(page.synopsis);
            arc = page.end_arc;
            index = page.end_index;
            chapters.extend(page.entries);
            next = page.next_page;
        }

        let title = title.ok_or_else(|| SourceError::Parse {
            url: base.to_string(),
            message: "no novel title found; is this a Narou table of contents?".to_string(),
        })?;

        let mut manifest = Manifest::new(
            SourceInfo {
                id: self.id().to_string(),
                novel_url: base.to_string(),
                site_title: None,
            },
            title,
            "ja",
        );
        manifest.author = author;
        manifest.synopsis = synopsis;
        manifest.content_rating = rating_for(&base);
        manifest.chapters = chapter_metas(chapters, &manifest.title, &base);
        Ok(manifest)
    }

    fn chapter(&self, fetch: &HttpClient, meta: &ChapterMeta) -> Result<Chapter, SourceError> {
        let html = fetch.get_text(&meta.url)?;
        chapter::parse_chapter(&html, meta)
    }
}

/// The chapter file path recorded in the manifest, relative to the novel dir.
fn chapter_file(index: u32) -> String {
    format!("chapters/{}", chapter_file_name(index))
}

/// The chapter list, or the single inline chapter a short novel carries on its
/// own table-of-contents page.
fn chapter_metas(rows: Vec<ChapterMeta>, title: &str, base: &Url) -> Vec<ChapterMeta> {
    if !rows.is_empty() {
        return rows;
    }
    // A single-chapter novel renders its body inline on the TOC page and `/1/`
    // would 404, so the chapter's URL is the TOC URL itself.
    vec![ChapterMeta {
        index: 1,
        title: title.to_string(),
        url: base.to_string(),
        arc: None,
        published_at: None,
        hash: None,
        file: chapter_file(1),
    }]
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

    fn matches(raw: &str) -> bool {
        Narou.matches(&Url::parse(raw).unwrap())
    }

    fn url(raw: &str) -> Url {
        Url::parse(raw).unwrap()
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

    #[test]
    fn any_url_on_a_novel_reduces_to_its_table_of_contents() {
        for (input, want) in [
            (
                "https://ncode.syosetu.com/n0608gd/",
                "https://ncode.syosetu.com/n0608gd/",
            ),
            (
                "https://ncode.syosetu.com/n0608gd",
                "https://ncode.syosetu.com/n0608gd/",
            ),
            (
                "https://ncode.syosetu.com/n0608gd/5/",
                "https://ncode.syosetu.com/n0608gd/",
            ),
            (
                "https://ncode.syosetu.com/n0608gd/?p=2",
                "https://ncode.syosetu.com/n0608gd/",
            ),
            (
                "https://novel18.syosetu.com/n8309id/3/",
                "https://novel18.syosetu.com/n8309id/",
            ),
        ] {
            assert_eq!(
                Narou::canonical_novel_url(&url(input)).unwrap().as_str(),
                want,
                "input: {input}"
            );
        }
    }

    #[test]
    fn urls_without_an_ncode_are_rejected() {
        assert!(Narou::canonical_novel_url(&url("https://ncode.syosetu.com/")).is_none());
        assert!(Narou::canonical_novel_url(&url("https://example.com/n0608gd/")).is_none());
    }

    #[test]
    fn adult_host_selects_the_adult_rating() {
        assert_eq!(
            rating_for(&url("https://novel18.syosetu.com/n8309id/")),
            ContentRating::Adult
        );
        assert_eq!(
            rating_for(&url("https://ncode.syosetu.com/n0608gd/")),
            ContentRating::General
        );
    }

    #[test]
    fn a_novel_with_no_rows_gets_one_inline_chapter() {
        let base = url("https://ncode.syosetu.com/n5589cg/");
        let chapters = chapter_metas(Vec::new(), "短編のタイトル", &base);

        assert_eq!(chapters.len(), 1);
        assert_eq!(chapters[0].index, 1);
        assert_eq!(chapters[0].title, "短編のタイトル");
        assert_eq!(
            chapters[0].url,
            base.as_str(),
            "the body is on the TOC page"
        );
        assert_eq!(chapters[0].arc, None);
        assert_eq!(chapters[0].file, "chapters/0001.json");
    }
}
