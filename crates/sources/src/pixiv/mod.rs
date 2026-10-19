//! Pixiv adapter: `pixiv.net`.
//!
//! Pixiv is the site that broke the mould for this repository. Every other
//! adapter reads HTML, and their comments are full of class names, blank-line
//! conventions and hashed selectors. Pixiv's novel pages are a JavaScript app
//! over a JSON API, so the adapter reads the API ([`api`]) and never the page:
//! the answers are the site's real interface, they hold facts the rendered page
//! does not (tags, embedded images, the description as written), and they cost
//! one request per novel instead of a page render plus a snapshot.
//!
//! Two of the site's shapes need saying plainly, because getting either wrong
//! loses text without complaining:
//!
//! - **a pixiv *work* is the novel, or the series it belongs to.** A novel URL
//!   that names one novel of a series names the series — the same rule Narou,
//!   Kakuyomu and Hameln follow, and for the same reason: any URL into a work
//!   downloads the work. `--max-chapters` is how to ask for less.
//! - **the reader's pages are not chapters.** A long novel arrives whole, with
//!   `[newpage]` where the reader slices it; those pages are joined ([`content`]).
//!   A five-page chapter that stops at page one is a fifth of a chapter that
//!   nothing reports as missing.
//!
//! Where the body comes from is worth one more word, because the manifest is
//! involved. A chapter's `url` in the manifest is the API URL the body is read
//! from, not the page a human would open: the manifest's URL is what gets
//! fetched, and keeping that true is what lets `chapter` be driven against a
//! socket (`crates/sources/tests/blocked.rs`). The work's own `novel_url` is
//! the human page, because that is what `nvd update` asks the adapter to
//! resolve again.

pub mod api;
mod content;

pub use content::{Images, parse as parse_content, pixiv_images};

use crate::dom;
use crate::{Source, SourceError};
use scraper::Html;
use serde::de::DeserializeOwned;
use url::Url;
use wnd_core::store::storage::chapter_file_name;
use wnd_core::{Chapter, ChapterMeta, ContentRating, HttpClient, Manifest, Page, SourceInfo};

pub struct Pixiv;

/// Hosts this adapter is responsible for.
///
/// `i.pximg.net`, where the images live, is deliberately not one of them: an
/// image URL is not a novel, and only the adapter that *found* one fetches it.
pub const HOSTS: [&str; 2] = ["www.pixiv.net", "pixiv.net"];

/// Where every request goes, whichever host named the work.
const API: &str = "https://www.pixiv.net/ajax";

/// How many entries one page of a series listing asks for.
///
/// The site rejects 100 and accepts 10, and the number is also the pager's stop
/// condition — a page shorter than this is the last one — so it lives in one
/// place.
const SERIES_PAGE: usize = 10;

/// A stop for a listing that never says "no more". Real series are far below
/// this; a page that repeats itself is caught separately.
const SERIES_PAGES_MAX: usize = 200;

impl Source for Pixiv {
    fn id(&self) -> &'static str {
        "pixiv"
    }

    fn matches(&self, url: &Url) -> bool {
        Self::host(url).is_some()
    }

    /// General until a work says otherwise: `xRestrict` is per work, so
    /// [`novel`](Source::novel) refines it from what each work states.
    fn content_rating(&self) -> ContentRating {
        ContentRating::General
    }

    /// Illustrations and covers live on `i.pximg.net`, which answers `403` to a
    /// request that does not say it came from the site. Nothing else we ask for
    /// needs it, and the referer is not a disguise: it is the page the picture
    /// is shown on.
    fn image_referer(&self) -> Option<&'static str> {
        Some("https://www.pixiv.net/")
    }

    fn novel(&self, fetch: &HttpClient, url: &Url) -> Result<Manifest, SourceError> {
        let target = Self::target(url).ok_or_else(|| SourceError::Parse {
            url: url.to_string(),
            message: "not a Pixiv novel: expected \
                      https://www.pixiv.net/novel/show.php?id=… or \
                      https://www.pixiv.net/novel/series/…"
                .to_string(),
        })?;

        match target {
            Target::Series(id) => self.series(fetch, &id),
            Target::Novel(id) => {
                let novel = self.work(fetch, &id)?;

                // A novel that belongs to a series is one chapter of the work,
                // and the work is the series.
                match novel.series_id() {
                    Some(series_id) => self.series(fetch, series_id),
                    None => Ok(standalone(&novel)),
                }
            }
        }
    }

    fn chapter(&self, fetch: &HttpClient, meta: &ChapterMeta) -> Result<Chapter, SourceError> {
        // The manifest's URL *is* the address the body comes from, so it is
        // fetched as it stands.
        let page = fetch.get_page(&meta.url)?;
        let novel: api::Novel = body_or_error(fetch, &page, &meta.url, "a chapter body")?;

        if novel.content.trim_ascii().is_empty() {
            return Err(match novel.is_login_only {
                // Not an error the user made: the work is published to signed-in
                // accounts only, and using access they already have is the
                // whole point of a session.
                true => SourceError::Other(format!(
                    "{} is only visible to a signed-in account: \
                     save a session with `nvd session save` and pass `--session`",
                    meta.url
                )),
                // A novel with nothing in it is not an empty chapter: the body
                // did not arrive. Saying nothing would write an empty chapter
                // and call it a success.
                false => SourceError::NotTheSite {
                    url: meta.url.clone(),
                    expected: "a chapter body".to_string(),
                }
                .with_evidence(fetch, &page),
            });
        }

        let images = self.resolve(fetch, &novel);
        let mut chapter = Chapter::new(meta.index, meta.title.clone());
        chapter.nodes = content::parse(&novel.content, &images);
        Ok(chapter)
    }
}

/// What a URL of this site points at.
#[derive(Debug, PartialEq, Eq)]
enum Target {
    /// One novel, whether or not it belongs to a series.
    Novel(String),
    /// A series, whose novels are its chapters.
    Series(String),
}

/// Where the site's own answer for a work lives.
///
/// This is the URL the adapter reads the work through: a chapter's `url` in the
/// manifest holds exactly this for a novel, and a series is read the same way.
/// It is public because it is the one address a *caller* can re-read a manifest
/// against — the live tests do exactly that, because the two ways this adapter
/// goes wrong quietly are both counts (a series read as one chapter, a listing
/// truncated at its first page) and a count needs a second opinion.
///
/// `None` when the URL names no work, the same as [`Source::novel`] would say.
pub fn answer_url(url: &Url) -> Option<String> {
    match Pixiv::target(url)? {
        Target::Novel(id) => Some(novel_api(&id)),
        Target::Series(id) => Some(series_api(&id)),
    }
}

impl Pixiv {
    fn host(url: &Url) -> Option<String> {
        let host = url.host_str()?.to_ascii_lowercase();
        HOSTS.contains(&host.as_str()).then_some(host)
    }

    /// What a URL names, if it names a work at all.
    ///
    /// Only the two shapes that do are accepted: `/novel/show.php?id=…` and
    /// `/novel/series/…`. The site serves more than one language from one host
    /// and puts the language in front (`/en/novel/series/…`), so one segment
    /// before `novel` is allowed and dropped — the API answers in the language
    /// the request asks for, and a downloaded novel's language is the work's,
    /// not the reader's. Anything after the path (`#page=3`, `?lang=en`) is
    /// ignored.
    fn target(url: &Url) -> Option<Target> {
        Self::host(url)?;
        let segments: Vec<&str> = url
            .path_segments()?
            .filter(|segment| !segment.is_empty())
            .collect();

        // An exact shape rather than a search for `novel`: a URL is read from
        // its end, so `/users/novel/show.php` cannot be read as a work.
        match segments.as_slice() {
            ["novel", "show.php"] | [_, "novel", "show.php"] => novel_id(url).map(Target::Novel),
            ["novel", "series", id] | [_, "novel", "series", id] => {
                is_id(id).then(|| Target::Series((*id).to_string()))
            }
            _ => None,
        }
    }

    /// One novel's answer.
    fn work(&self, fetch: &HttpClient, id: &str) -> Result<api::Novel, SourceError> {
        let url = novel_api(id);
        let page = fetch.get_page(&url)?;
        body_or_error(fetch, &page, &url, "a novel")
    }

    /// A series and every novel in it, as one manifest.
    fn series(&self, fetch: &HttpClient, id: &str) -> Result<Manifest, SourceError> {
        let url = series_api(id);
        let page = fetch.get_page(&url)?;
        let series: api::Series = body_or_error(fetch, &page, &url, "a series")?;
        let entries = self.series_entries(fetch, id)?;

        if entries.is_empty() {
            // A series with no novels in it is either a listing that did not
            // arrive or a series the site has emptied; either way a manifest
            // with no chapters would download nothing and report success.
            return Err(SourceError::NotTheSite {
                url,
                expected: "a chapter list".to_string(),
            }
            .with_evidence(fetch, &page));
        }

        Ok(series_manifest(&series, &entries, &series_page(id)))
    }

    /// Every novel in a series, in reading order.
    ///
    /// The listing is answered newest-first and pages by `last_order`, which
    /// means "give me the entries below this one", so a series is walked from
    /// its end back to chapter one and put in order at the end. The first page
    /// must not carry `last_order` at all — an empty value is an HTTP 400 — and
    /// a page that is not full is the last one.
    fn series_entries(
        &self,
        fetch: &HttpClient,
        id: &str,
    ) -> Result<Vec<api::SeriesEntry>, SourceError> {
        let mut entries: Vec<api::SeriesEntry> = Vec::new();
        let mut last_order: Option<i64> = None;

        for _ in 0..SERIES_PAGES_MAX {
            let url = series_content_api(id, last_order);
            let page = fetch.get_page(&url)?;
            let content: api::SeriesContent = body_or_error(fetch, &page, &url, "a chapter list")?;
            let batch = content.thumbnails.novel;

            let lowest = batch.iter().map(|entry| entry.series_content_order).min();
            let full_page = batch.len() >= SERIES_PAGE;
            entries.extend(batch);

            // Continue below the lowest order this page gave. A page that does
            // not fill up, or that repeats the order we asked below, is the end
            // of the list.
            match lowest {
                Some(order) if full_page && Some(order) != last_order => {
                    last_order = Some(order);
                }
                _ => break,
            }
        }

        entries.sort_by_key(|entry| entry.series_content_order);
        Ok(entries)
    }

    /// The URL every image marker in a novel resolves to.
    ///
    /// The images the author uploaded are already in the novel's own answer. A
    /// `[pixivimage:…]` points at someone else's illustration and takes one
    /// request; when that request fails, the reference is simply left out of
    /// the map, and [`content::parse`] then leaves the marker as the author
    /// wrote it. A broken reference must not cost the reader the chapter it
    /// appears in.
    fn resolve(&self, fetch: &HttpClient, novel: &api::Novel) -> Images {
        let mut images = Images::new();

        for (reference, embedded) in &novel.text_embedded_images {
            if let Some(url) = api::best(&embedded.urls) {
                images.insert(reference.clone(), url.to_string());
            }
        }

        for reference in content::pixiv_images(&novel.content) {
            if images.contains_key(&reference) {
                continue;
            }
            if let Some(url) = self.illust(fetch, &reference) {
                images.insert(reference, url);
            }
        }

        images
    }

    /// The original image a `[pixivimage:…]` reference names, if the site still
    /// has it and this adapter can make sense of the reference.
    fn illust(&self, fetch: &HttpClient, reference: &str) -> Option<String> {
        let (id, page) = illust_reference(reference)?;
        let answer = fetch.get_page(&illust_api(id)).ok()?;
        let illust: api::Illust = api::body_of(&answer.body).ok()?;
        illust.original_at(page).map(str::to_string)
    }
}

/// The illust id and zero-based page a `[pixivimage:…]` reference names.
///
/// The site writes `[pixivimage:12345]` for the first page of an illustration
/// and `[pixivimage:12345-2]` for the second. Anything else — a missing id, a
/// page that is not a number, a reference to a whole series — is not something
/// this adapter can resolve, and is left as written.
fn illust_reference(reference: &str) -> Option<(&str, usize)> {
    let (id, page) = match reference.split_once('-') {
        Some((id, page)) => (id, page.parse::<usize>().ok()?),
        None => (reference, 1),
    };

    if id.is_empty() || !is_id(id) {
        return None;
    }

    Some((id, page.saturating_sub(1)))
}

/// A novel that belongs to no series: the work is this one novel, and this one
/// novel is its only chapter.
fn standalone(novel: &api::Novel) -> Manifest {
    let mut manifest = Manifest::new(
        SourceInfo {
            id: "pixiv".to_string(),
            novel_url: novel_page(&novel.id),
            site_title: None,
        },
        &novel.title,
        language(&novel.language),
    );

    manifest.author = non_empty(&novel.user_name);
    manifest.synopsis = synopsis(&novel.description);
    manifest.tags = novel.tag_names();
    manifest.cover_url = non_empty(&novel.cover_url);
    manifest.content_rating = rating(novel.x_restrict);

    let (published_at, revised_at) = dates(&novel.create_date, &novel.upload_date);
    manifest.chapters = vec![ChapterMeta {
        index: 1,
        title: novel.title.clone(),
        url: novel_api(&novel.id),
        arc: None,
        published_at,
        revised_at,
        hash: None,
        file: chapter_file(1),
    }];

    manifest
}

/// A series, with one chapter per novel in it.
///
/// Chapters are numbered in reading order, from one — not by
/// `seriesContentOrder`, which is the site's own numbering used to page the
/// listing and which this tool promises nothing about. It is the same rule
/// Kakuyomu's position-numbered episodes get, and for the same reason: a
/// number the *site* assigns can be anything, but the manifest promises
/// `1..n` in the order the work is read.
fn series_manifest(
    series: &api::Series,
    entries: &[api::SeriesEntry],
    novel_url: &str,
) -> Manifest {
    let mut manifest = Manifest::new(
        SourceInfo {
            id: "pixiv".to_string(),
            novel_url: novel_url.to_string(),
            site_title: None,
        },
        &series.title,
        language(&series.language),
    );

    manifest.author = non_empty(&series.user_name);
    manifest.synopsis = synopsis(&series.caption);
    manifest.tags = series.tags.clone();
    manifest.cover_url = series
        .cover
        .as_ref()
        .and_then(|cover| api::best(&cover.urls))
        .map(str::to_string);
    manifest.content_rating = rating(series.x_restrict);

    manifest.chapters = entries
        .iter()
        .enumerate()
        .map(|(position, entry)| {
            let index = position as u32 + 1;
            let (published_at, revised_at) = dates(&entry.create_date, &entry.update_date);

            ChapterMeta {
                index,
                title: entry.title.clone(),
                url: novel_api(&entry.id),
                arc: None,
                published_at,
                revised_at,
                hash: None,
                file: chapter_file(index),
            }
        })
        .collect();

    manifest
}

/// A chapter's two dates: when it appeared, and when it was last revised.
///
/// The site states both for every chapter, so "revised" is read as *the update
/// differs from the publication* — otherwise every chapter would carry a
/// revision stamp that never means anything, and `nvd update` would have
/// nothing to compare. A site that states no time at all says `None` here, the
/// same as Narou does for a chapter it has never revised.
fn dates(created: &str, updated: &str) -> (Option<String>, Option<String>) {
    let published = non_empty(created);
    let revised =
        non_empty(updated).filter(|updated| Some(updated.as_str()) != published.as_deref());
    (published, revised)
}

/// The language the site states, or `ja`.
///
/// Pixiv is a Japanese site whose answers also serve other languages; the field
/// is absent on some works, and a novel with no stated language is Japanese.
fn language(stated: &str) -> String {
    non_empty(stated).unwrap_or_else(|| "ja".to_string())
}

/// `xRestrict` is `1` for adult and `0` otherwise.
fn rating(x_restrict: i64) -> ContentRating {
    if x_restrict >= 1 {
        ContentRating::Adult
    } else {
        ContentRating::General
    }
}

/// The site publishes a description as HTML: `<br />` for a line break, and
/// occasionally a link around part of it. The model's synopsis is plain text
/// with its line breaks kept, so the markup is read as markup rather than
/// pasted into the synopsis as tags.
fn synopsis(description: &str) -> Option<String> {
    let document = Html::parse_fragment(description);
    non_empty(&dom::element_text(&document.root_element()))
}

fn non_empty(value: &str) -> Option<String> {
    (!value.trim_ascii().is_empty()).then(|| value.to_string())
}

/// Where a chapter's file lives, relative to the novel directory.
fn chapter_file(index: u32) -> String {
    format!("chapters/{}", chapter_file_name(index))
}

/// The page a human opens for a novel — and what the manifest's `novel_url`
/// holds, because `nvd update` asks the adapter to resolve it again.
fn novel_page(id: &str) -> String {
    format!("https://www.pixiv.net/novel/show.php?id={id}")
}

/// The page a human opens for a series.
fn series_page(id: &str) -> String {
    format!("https://www.pixiv.net/novel/series/{id}")
}

/// The API URL a novel's text is read from — and what a chapter's `url` holds.
fn novel_api(id: &str) -> String {
    format!("{API}/novel/{id}")
}

fn series_api(id: &str) -> String {
    format!("{API}/novel/series/{id}")
}

/// One page of a series listing. `last_order` is omitted on the first page: the
/// site rejects an empty value with a 400.
fn series_content_api(id: &str, last_order: Option<i64>) -> String {
    let page = match last_order {
        Some(order) => format!("last_order={order}"),
        None => "is_first_page=true".to_string(),
    };

    format!("{API}/novel/series_content/{id}?limit={SERIES_PAGE}&{page}&work_category=novel")
}

fn illust_api(id: &str) -> String {
    format!("{API}/illust/{id}")
}

/// The novel id a `/novel/show.php` URL names.
fn novel_id(url: &Url) -> Option<String> {
    let id = url
        .query_pairs()
        .find(|(key, _)| key == "id")
        .map(|(_, value)| value.into_owned())?;

    is_id(&id).then_some(id)
}

fn is_id(candidate: &str) -> bool {
    !candidate.is_empty() && candidate.bytes().all(|byte| byte.is_ascii_digit())
}

/// Read the payload out of an answer, turning each way it can fail into the
/// error the reader should see.
///
/// A body that is not JSON is not a parse failure: something that is not this
/// API answered, and the fetch layer knows how to say what it was with the
/// evidence in hand.
fn body_or_error<T: DeserializeOwned>(
    fetch: &HttpClient,
    page: &Page,
    url: &str,
    expected: &str,
) -> Result<T, SourceError> {
    api::body_of::<T>(&page.body).map_err(|error| {
        if error.not_json {
            SourceError::NotTheSite {
                url: url.to_string(),
                expected: expected.to_string(),
            }
            .with_evidence(fetch, page)
        } else {
            SourceError::Parse {
                url: url.to_string(),
                message: error.message,
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;

    fn target(raw: &str) -> Option<Target> {
        Pixiv::target(&Url::parse(raw).expect("a URL"))
    }

    fn novel(raw: &str) -> api::Novel {
        api::body_of(raw).expect("the fixture")
    }

    #[test]
    fn every_url_into_a_novel_names_the_same_novel() {
        for raw in [
            "https://www.pixiv.net/novel/show.php?id=10000004",
            // The language prefix the site serves other languages under.
            "https://www.pixiv.net/en/novel/show.php?id=10000004",
            "https://www.pixiv.net/zh-tw/novel/show.php?id=10000004",
            // What a browser adds when you copy the address bar.
            "https://www.pixiv.net/novel/show.php?id=10000004#page=3",
            "https://www.pixiv.net/novel/show.php?lang=en&id=10000004",
            "https://www.pixiv.net/novel/show.php?id=10000004&foo=bar",
            "https://www.pixiv.net/novel/show.php/?id=10000004",
            // The bare host redirects to `www`, and reaches the same novel.
            "https://pixiv.net/novel/show.php?id=10000004",
        ] {
            assert_eq!(
                target(raw),
                Some(Target::Novel("10000004".to_string())),
                "input: {raw}"
            );
        }
    }

    #[test]
    fn every_url_into_a_series_names_the_same_series() {
        for raw in [
            "https://www.pixiv.net/novel/series/10000006",
            "https://www.pixiv.net/novel/series/10000006/",
            "https://www.pixiv.net/en/novel/series/10000006",
            "https://www.pixiv.net/en/novel/series/10000006#page=2",
            "https://www.pixiv.net/novel/series/10000006?lang=en",
        ] {
            assert_eq!(
                target(raw),
                Some(Target::Series("10000006".to_string())),
                "input: {raw}"
            );
        }
    }

    #[test]
    fn a_pixiv_url_that_names_no_work_names_nothing() {
        for raw in [
            // The site's other content, and its other pages.
            "https://www.pixiv.net/artworks/12345678",
            "https://www.pixiv.net/users/406881",
            "https://www.pixiv.net/novel/ranking.php?mode=daily",
            "https://www.pixiv.net/novel/new",
            // A novel URL with no usable id.
            "https://www.pixiv.net/novel/show.php",
            "https://www.pixiv.net/novel/show.php?id=",
            "https://www.pixiv.net/novel/show.php?id=abc",
            "https://www.pixiv.net/novel/show.php?id=10000004abc",
            "https://www.pixiv.net/novel/series/",
            "https://www.pixiv.net/novel/series/abc",
            // A `novel` segment deeper in a path is not a work.
            "https://www.pixiv.net/users/1/novel/show.php",
        ] {
            assert_eq!(target(raw), None, "input: {raw}");
        }
    }

    #[test]
    fn the_adapter_claims_its_own_hosts_and_no_others() {
        let claimed = |raw: &str| Pixiv.matches(&Url::parse(raw).expect("a URL"));

        assert!(claimed("https://www.pixiv.net/novel/show.php?id=1"));
        assert!(claimed("https://pixiv.net/novel/show.php?id=1"));
        assert!(claimed("https://www.pixiv.net/artworks/1"));

        // The image CDN is not a novel, and `notpixiv.net` is not pixiv.
        assert!(!claimed("https://i.pximg.net/img/1.png"));
        assert!(!claimed("https://notpixiv.net/novel/show.php?id=1"));
        assert!(!claimed(
            "https://www.pixiv.net.evil.example/novel/show.php?id=1"
        ));
    }

    #[test]
    fn a_novel_in_no_series_is_a_work_of_one_chapter() {
        let manifest = standalone(&novel(fixtures::PIXIV_NOVEL_STANDALONE));

        assert_eq!(manifest.title, "短編サンプル");
        assert_eq!(manifest.author.as_deref(), Some("別の作者"));
        assert_eq!(manifest.language, "ja");
        assert_eq!(manifest.tags, vec!["ファンタジー"]);
        assert_eq!(manifest.source.id, "pixiv");
        assert_eq!(
            manifest.source.novel_url,
            "https://www.pixiv.net/novel/show.php?id=10000007"
        );
        assert_eq!(
            manifest.content_rating,
            ContentRating::Adult,
            "xRestrict is per work, and this one is 1"
        );

        assert_eq!(manifest.chapters.len(), 1, "the novel is its own chapter");
        let chapter = &manifest.chapters[0];
        assert_eq!(chapter.index, 1);
        assert_eq!(chapter.title, "短編サンプル");
        assert_eq!(
            chapter.url, "https://www.pixiv.net/ajax/novel/10000007",
            "a chapter's url is the address its body is read from"
        );
        assert_eq!(chapter.file, "chapters/0001.json");
        assert_eq!(
            chapter.published_at.as_deref(),
            Some("2023-05-18T14:58:25+00:00")
        );
        assert_eq!(
            chapter.revised_at.as_deref(),
            Some("2023-05-20T09:00:00+00:00"),
            "the site says this one was touched after it was published"
        );
    }

    #[test]
    fn a_series_is_the_work_and_its_novels_are_its_chapters_in_reading_order() {
        let series: api::Series = api::body_of(fixtures::PIXIV_SERIES).expect("the fixture");
        let listing: api::SeriesContent =
            api::body_of(fixtures::PIXIV_SERIES_CONTENT).expect("the fixture");
        let mut entries = listing.thumbnails.novel;
        entries.sort_by_key(|entry| entry.series_content_order);

        let manifest = series_manifest(
            &series,
            &entries,
            "https://www.pixiv.net/novel/series/10000006",
        );

        assert_eq!(manifest.title, "サンプルシリーズ");
        assert_eq!(
            manifest.source.novel_url, "https://www.pixiv.net/novel/series/10000006",
            "the work's url is the page `nvd update` resolves again, and this adapter can"
        );
        assert_eq!(manifest.author.as_deref(), Some("サンプル作者"));
        assert_eq!(manifest.tags, vec!["R-18", "学園", "SF"]);
        assert_eq!(manifest.content_rating, ContentRating::Adult);
        assert_eq!(
            manifest.synopsis.as_deref(),
            Some("シリーズの説明一行目。\n二行目。"),
            "the description is HTML with line breaks, and the synopsis keeps them"
        );
        assert_eq!(
            manifest.cover_url.as_deref(),
            Some(
                "https://i.pximg.net/novel-cover-original/img/2021/03/26/15/00/36/\
                 sci10000006_deadbeef.png"
            ),
            "the original cover, not one of the resized derivatives"
        );

        let titles: Vec<&str> = manifest
            .chapters
            .iter()
            .map(|chapter| chapter.title.as_str())
            .collect();
        assert_eq!(
            titles,
            vec!["第一話　出会い", "第二話　再会", "第三話　決着"]
        );

        assert_eq!(
            manifest
                .chapters
                .iter()
                .map(|chapter| chapter.index)
                .collect::<Vec<_>>(),
            vec![1, 2, 3],
            "chapter numbers are reading order, not the site's own ordering"
        );
        assert_eq!(
            manifest
                .chapters
                .iter()
                .map(|chapter| chapter.url.as_str())
                .collect::<Vec<_>>(),
            vec![
                "https://www.pixiv.net/ajax/novel/10000004",
                "https://www.pixiv.net/ajax/novel/15101193",
                "https://www.pixiv.net/ajax/novel/15482204",
            ]
        );
        assert_eq!(
            manifest
                .chapters
                .iter()
                .map(|chapter| chapter.revised_at.as_deref())
                .collect::<Vec<_>>(),
            vec![None, None, Some("2022-03-05T12:00:00+09:00")],
            "only the chapter the site says was updated carries a revision stamp"
        );
        assert_eq!(
            manifest.chapters[2].published_at.as_deref(),
            Some("2022-03-01T00:00:00+09:00"),
            "an instant the site states is kept as the site stated it"
        );
    }

    #[test]
    fn an_illust_reference_names_an_illust_and_a_page_of_it() {
        assert_eq!(illust_reference("12345678"), Some(("12345678", 0)));
        assert_eq!(illust_reference("12345678-2"), Some(("12345678", 1)));
        assert_eq!(illust_reference("12345678-1"), Some(("12345678", 0)));
        assert_eq!(
            illust_reference("12345678-0"),
            Some(("12345678", 0)),
            "a page number below one clamps rather than wrapping"
        );

        assert_eq!(illust_reference(""), None);
        assert_eq!(illust_reference("-2"), None);
        assert_eq!(illust_reference("abc"), None);
        assert_eq!(illust_reference("12345678-x"), None);
    }

    #[test]
    fn the_series_listing_is_asked_for_in_the_order_the_site_pages_by() {
        assert_eq!(
            series_content_api("10000006", None),
            "https://www.pixiv.net/ajax/novel/series_content/10000006\
             ?limit=10&is_first_page=true&work_category=novel",
            "the first page must not carry `last_order` at all: an empty one is a 400"
        );
        assert_eq!(
            series_content_api("10000006", Some(2)),
            "https://www.pixiv.net/ajax/novel/series_content/10000006\
             ?limit=10&last_order=2&work_category=novel"
        );
    }

    #[test]
    fn a_work_has_one_address_the_site_answers_at() {
        for raw in [
            "https://www.pixiv.net/novel/show.php?id=10000004",
            "https://www.pixiv.net/en/novel/show.php?id=10000004#page=3",
        ] {
            assert_eq!(
                answer_url(&Url::parse(raw).unwrap()).as_deref(),
                Some("https://www.pixiv.net/ajax/novel/10000004"),
                "input: {raw}"
            );
        }

        assert_eq!(
            answer_url(&Url::parse("https://www.pixiv.net/novel/series/10000006").unwrap())
                .as_deref(),
            Some("https://www.pixiv.net/ajax/novel/series/10000006")
        );

        assert_eq!(
            answer_url(&Url::parse("https://www.pixiv.net/artworks/1").unwrap()),
            None
        );
    }

    #[test]
    fn a_language_the_site_does_not_state_is_japanese() {
        assert_eq!(language("ja"), "ja");
        assert_eq!(language("en"), "en");
        assert_eq!(language(""), "ja");
        assert_eq!(language("   "), "ja");
    }
}
