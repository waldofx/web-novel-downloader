//! Kakuyomu (`kakuyomu.jp`) — Kadokawa's site.
//!
//! A different platform from Narou in every way that matters, which is what
//! makes it the test of whether the node model is site-independent or merely
//! Narou-shaped. So far the model has held: Kakuyomu's furigana arrives as real
//! `<ruby>` markup rather than Narou's `｜漢字《かんじ》`, its arcs are real chapter
//! dividers rather than inferred headings, and neither difference reaches the
//! model.
//!
//! Three things about this site shape the adapter:
//!
//! - **The work page's HTML is not the work.** It renders the first chapter's
//!   episodes and lazy-loads the rest, so the episode list is read from the
//!   page's `__NEXT_DATA__` snapshot instead ([`work`]). Getting this wrong means
//!   downloading one chapter of a thousand-episode novel without complaint.
//! - **Episodes have no JSON at all**, so the body is read from HTML ([`chapter`]).
//! - **Most class names are build-hashed** (`WorkTocSection_title__H2007`) and
//!   change on every deploy. The adapter needs none of them: the body container
//!   carries a plain `widget-episodeBody`/`js-episode-body` hook, and everything
//!   else comes from the snapshot.
//!
//! Kakuyomu is an all-ages site: there is no adult sibling host like
//! `novel18.syosetu.com`, so the rating is general and per-work content flags
//! (性描写, 暴力, 残酷) are content warnings rather than an age gate. They are
//! therefore not mapped to `ContentRating::Adult`, and not yet carried anywhere —
//! see `docs/engineering/sources.md`.

mod apollo;
mod chapter;
mod work;

use crate::{Source, SourceError};
use apollo::Apollo;
use url::Url;
use wnd_core::{Block, Chapter, ChapterMeta, ContentRating, HttpClient, Manifest, SourceInfo};

pub struct Kakuyomu;

/// The only host this adapter is responsible for.
pub const HOST: &str = "kakuyomu.jp";

impl Kakuyomu {
    /// `…/works/<id>` — the canonical work URL, from any URL on the work.
    ///
    /// `…/works/<id>/episodes/<episode-id>` reduces to it, so a reader can paste
    /// the page they have open.
    fn canonical_work_url(url: &Url) -> Option<Url> {
        if !matches!(url.scheme(), "http" | "https") {
            return None;
        }
        if url.host_str()?.to_ascii_lowercase() != HOST {
            return None;
        }
        let id = Self::work_id(url)?;
        Url::parse(&format!("https://{HOST}/works/{id}")).ok()
    }

    /// The id in a `/works/<id>/…` path.
    ///
    /// Ids are digits; requiring that keeps `/works/`, `/rankings/all` and a
    /// stray `/works/index.html` from being treated as works.
    fn work_id(url: &Url) -> Option<&str> {
        let mut segments = url.path_segments()?;
        if segments.next()? != "works" {
            return None;
        }
        let id = segments.next()?;
        if id.is_empty() || !id.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        Some(id)
    }
}

impl Source for Kakuyomu {
    fn id(&self) -> &'static str {
        "kakuyomu"
    }

    fn matches(&self, url: &Url) -> bool {
        matches!(url.scheme(), "http" | "https")
            && url
                .host_str()
                .is_some_and(|host| host.eq_ignore_ascii_case(HOST))
    }

    fn content_rating(&self) -> ContentRating {
        ContentRating::General
    }

    fn novel(&self, fetch: &HttpClient, url: &Url) -> Result<Manifest, SourceError> {
        let base = Self::canonical_work_url(url).ok_or_else(|| SourceError::Parse {
            url: url.to_string(),
            message: "not a Kakuyomu work URL".to_string(),
        })?;
        let work_id = Self::work_id(&base).expect("a canonical work URL has a work id");

        let fetched = fetch.get_page(base.as_str())?;
        let apollo = Apollo::from_html(&fetched.body)
            .ok_or_else(|| SourceError::NotTheSite {
                url: base.to_string(),
                expected: "a page snapshot".to_string(),
            })
            .map_err(|error| error.with_evidence(fetch, &fetched))?;
        let page = work::parse(&apollo, work_id, &base)?;

        let mut manifest = Manifest::new(
            SourceInfo {
                id: self.id().to_string(),
                novel_url: base.to_string(),
                site_title: None,
            },
            page.title,
            "ja",
        );
        manifest.author = page.author;
        manifest.synopsis = page
            .synopsis
            .map(|text| Block::paragraphs(&text))
            .unwrap_or_default();
        manifest.tags = page.tags;
        manifest.content_rating = self.content_rating();
        manifest.chapters = page.chapters;
        manifest.catchphrase = page.catchphrase;
        manifest.cover_url = page.cover_url;
        manifest.warnings = page.warnings;
        Ok(manifest)
    }

    fn chapter(&self, fetch: &HttpClient, meta: &ChapterMeta) -> Result<Chapter, SourceError> {
        let page = fetch.get_page(&meta.url)?;
        chapter::parse_chapter(&page.body, meta).map_err(|error| error.with_evidence(fetch, &page))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(raw: &str) -> Url {
        Url::parse(raw).unwrap()
    }

    fn matches(raw: &str) -> bool {
        Kakuyomu.matches(&url(raw))
    }

    #[test]
    fn work_and_episode_urls_are_matched() {
        assert!(matches("https://kakuyomu.jp/works/10000000000000000002"));
        assert!(matches(
            "https://kakuyomu.jp/works/10000000000000000002/episodes/10000000000000000004"
        ));
        assert!(matches("https://KAKUYOMU.jp/works/1"));
    }

    #[test]
    fn lookalike_hosts_and_other_schemes_are_not_matched() {
        assert!(!matches("https://kakuyomu.jp.evil.example/works/1"));
        assert!(!matches("https://example.com/works/1"));
        assert!(!matches("ftp://kakuyomu.jp/works/1"));
        assert!(!matches("https://kakuyomu.com/works/1"));
    }

    #[test]
    fn any_url_on_a_work_reduces_to_its_work_page() {
        for (input, want) in [
            (
                "https://kakuyomu.jp/works/10000000000000000001",
                "https://kakuyomu.jp/works/10000000000000000001",
            ),
            (
                "https://kakuyomu.jp/works/10000000000000000001/",
                "https://kakuyomu.jp/works/10000000000000000001",
            ),
            (
                "https://kakuyomu.jp/works/10000000000000000001/episodes/10000000000000000003",
                "https://kakuyomu.jp/works/10000000000000000001",
            ),
            (
                "https://kakuyomu.jp/works/10000000000000000001?utm_source=x#p12",
                "https://kakuyomu.jp/works/10000000000000000001",
            ),
        ] {
            assert_eq!(
                Kakuyomu::canonical_work_url(&url(input)).unwrap().as_str(),
                want,
                "input: {input}"
            );
        }
    }

    #[test]
    fn urls_that_are_not_works_are_rejected() {
        for input in [
            "https://kakuyomu.jp/",
            "https://kakuyomu.jp/rankings/all/weekly",
            "https://kakuyomu.jp/works/",
            "https://kakuyomu.jp/works/not-a-number",
            "https://kakuyomu.jp/users/sample_writer",
        ] {
            assert!(
                Kakuyomu::canonical_work_url(&url(input)).is_none(),
                "input: {input}"
            );
        }
    }

    #[test]
    fn kakuyomu_is_not_adult_gated() {
        // There is no adult sibling host; the per-work flags are warnings, and
        // mapping them here would refuse to fetch most of the catalogue.
        assert_eq!(Kakuyomu.content_rating(), ContentRating::General);
        assert_eq!(Kakuyomu.id(), "kakuyomu");
    }
}
