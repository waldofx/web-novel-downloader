//! The table of contents: novel metadata, arcs, chapter rows, published dates,
//! and `?p=` pagination.
//!
//! Arc headings and chapter rows share one document-ordered selection
//! (`.p-eplist__chapter-title, .p-eplist__subtitle`), which is what makes "the
//! arc a chapter belongs to" simply *the heading seen most recently*. Pagination
//! carries that arc across pages along with the running chapter index.
//!
//! The published date is the row's own text. A *revision* date lives in a
//! `title` attribute on a child `<span>` and is deliberately not used: it is a
//! different fact, and taking it would rewrite history on every edit.

use super::chapter_file;
use super::dom::{element_text, first_text, is_class};
use super::{
    SEL_ANCHOR, SEL_AUTHOR, SEL_PAGER_NEXT, SEL_SUMMARY, SEL_TITLE, SEL_TOC_ENTRY, SEL_UPDATE,
};
use scraper::{ElementRef, Html};
use url::Url;
use wnd_core::ChapterMeta;

/// One parsed table-of-contents page.
#[derive(Debug, Default)]
pub(super) struct TocPage {
    pub(super) title: Option<String>,
    pub(super) author: Option<String>,
    pub(super) synopsis: Option<String>,
    pub(super) entries: Vec<ChapterMeta>,
    /// Arc in effect when the page ended, carried into the next page.
    pub(super) end_arc: Option<String>,
    /// Index of the last chapter seen, so pagination keeps counting up.
    pub(super) end_index: u32,
    pub(super) next_page: Option<String>,
}

/// Parse one table-of-contents page.
///
/// `start_arc` and `start_index` carry state across pagination: arcs continue
/// from the previous page, and chapter numbering never restarts.
pub(super) fn parse_toc_page(
    html: &str,
    base: &Url,
    start_arc: Option<String>,
    start_index: u32,
) -> TocPage {
    let document = Html::parse_document(html);

    let mut page = TocPage {
        title: first_text(&document, &SEL_TITLE),
        author: first_text(&document, &SEL_AUTHOR).map(|text| strip_author_prefix(&text)),
        synopsis: first_text(&document, &SEL_SUMMARY),
        end_arc: start_arc.clone(),
        end_index: start_index,
        ..TocPage::default()
    };

    let mut arc = start_arc;
    let mut index = start_index;

    // Arc headings and chapter rows share one document-ordered selection, so
    // "the arc a chapter belongs to" is simply the heading seen most recently.
    for entry in document.select(&SEL_TOC_ENTRY) {
        if is_class(&entry, "p-eplist__chapter-title") {
            arc = Some(element_text(&entry));
            continue;
        }

        let href = entry.value().attr("href");
        index = href.and_then(chapter_index_from_href).unwrap_or(index + 1);
        let url = href
            .and_then(|href| base.join(href).ok())
            .unwrap_or_else(|| {
                base.join(&format!("{index}/"))
                    .expect("a relative join cannot fail")
            });
        // The published date is the row's own text; a revision date lives in a
        // `title` attribute and is deliberately not used.
        let published_at = entry
            .parent()
            .and_then(ElementRef::wrap)
            .and_then(|row| row.select(&SEL_UPDATE).next())
            .and_then(|cell| parse_datetime(&element_text(&cell)));

        page.entries.push(ChapterMeta {
            index,
            title: element_text(&entry),
            url: url.to_string(),
            arc: arc.clone(),
            published_at,
            hash: None,
            file: chapter_file(index),
        });
    }

    page.end_arc = arc;
    page.end_index = index;
    page.next_page = pager_next(&document, base);
    page
}

/// `作者：ウスバー（正直者）` → `ウスバー（正直者）`.
fn strip_author_prefix(text: &str) -> String {
    let text = text.trim_ascii();
    text.strip_prefix("作者：")
        .or_else(|| text.strip_prefix("作者:"))
        .unwrap_or(text)
        .trim_ascii()
        .to_string()
}

/// `…/n0608gd/12/` → `12`.
fn chapter_index_from_href(href: &str) -> Option<u32> {
    href.trim_end_matches('/').rsplit('/').next()?.parse().ok()
}

/// The `?p=` link to the next table-of-contents page, if any.
fn pager_next(document: &Html, base: &Url) -> Option<String> {
    let element = document.select(&SEL_PAGER_NEXT).next()?;
    let href = element
        .value()
        .attr("href")
        .or_else(|| element.select(&SEL_ANCHOR).next()?.value().attr("href"))?;
    base.join(href).ok().map(|url| url.to_string())
}

/// Narou prints timestamps as `2020/04/01 23:23` in Japan Standard Time, which
/// has no daylight saving, so the offset is always `+09:00`.
///
/// A date with no time is midnight; anything unparseable is dropped rather than
/// guessed at.
fn parse_datetime(text: &str) -> Option<String> {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    for (position, token) in tokens.iter().enumerate() {
        let Some(date) = parse_date(token) else {
            continue;
        };
        // A token that looks like a time must parse: substituting midnight for
        // `25:00` would invent a timestamp rather than admit the data is bad.
        let (hour, minute) = match tokens.get(position + 1) {
            Some(token) if token.contains(':') => parse_time(token)?,
            _ => (0, 0),
        };
        return Some(format!("{date}T{hour:02}:{minute:02}:00+09:00"));
    }
    None
}

/// `2020/04/01` → `2020-04-01`.
fn parse_date(token: &str) -> Option<String> {
    let mut parts = token.split('/');
    let year = parts.next()?;
    let month = parts.next()?;
    let day = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    if year.len() != 4 || month.len() != 2 || day.len() != 2 {
        return None;
    }
    let month_number: u32 = month.parse().ok()?;
    let day_number: u32 = day.parse().ok()?;
    if !(1..=12).contains(&month_number) || !(1..=31).contains(&day_number) {
        return None;
    }
    Some(format!("{year}-{month}-{day}"))
}

/// `23:23` → `(23, 23)`.
fn parse_time(token: &str) -> Option<(u32, u32)> {
    let mut parts = token.split(':');
    let hour = parts.next()?;
    let minute = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    if hour.len() != 2 || minute.len() != 2 {
        return None;
    }
    let hour_number: u32 = hour.parse().ok()?;
    let minute_number: u32 = minute.parse().ok()?;
    if hour_number > 23 || minute_number > 59 {
        return None;
    }
    Some((hour_number, minute_number))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::NAROU_TOC;

    fn url(raw: &str) -> Url {
        Url::parse(raw).unwrap()
    }

    #[test]
    fn toc_metadata_arcs_and_chapters_are_parsed() {
        let page = parse_toc_page(
            NAROU_TOC,
            &url("https://ncode.syosetu.com/n0608gd/"),
            None,
            0,
        );

        assert_eq!(
            page.title.as_deref(),
            Some("二重勇者はすごいです！　～テスト～")
        );
        assert_eq!(page.author.as_deref(), Some("ウスバー（正直者）"));
        assert_eq!(
            page.synopsis.as_deref(),
            Some("――ただ、強い！！\n\n二年前に勇者として召喚された。"),
            "the <br />s become newlines"
        );

        assert_eq!(page.entries.len(), 3);
        let first = &page.entries[0];
        assert_eq!(first.index, 1);
        assert_eq!(first.title, "１．勇者の帰還");
        assert_eq!(first.url, "https://ncode.syosetu.com/n0608gd/1/");
        assert_eq!(first.arc.as_deref(), Some("第一章　勇者、日本に帰る"));
        assert_eq!(
            first.published_at.as_deref(),
            Some("2020-04-01T23:23:00+09:00")
        );
        assert_eq!(first.hash, None, "the adapter cannot know a hash yet");
        assert_eq!(first.file, "chapters/0001.json");

        let third = &page.entries[2];
        assert_eq!(third.arc.as_deref(), Some("第二章　再会"));

        assert_eq!(
            page.next_page.as_deref(),
            Some("https://ncode.syosetu.com/n0608gd/?p=2")
        );
        assert_eq!(page.end_index, 3);
    }

    #[test]
    fn the_first_page_of_a_paginated_toc_reports_its_state() {
        let page = parse_toc_page(
            NAROU_TOC,
            &url("https://ncode.syosetu.com/n0608gd/"),
            None,
            0,
        );
        // The next page must continue numbering and keep the arc in effect.
        assert_eq!(page.end_index, 3);
        assert_eq!(page.end_arc.as_deref(), Some("第二章　再会"));
    }

    #[test]
    fn narou_timestamps_become_rfc_3339_in_japan_standard_time() {
        assert_eq!(
            parse_datetime("2020/04/01 23:23 （改）").as_deref(),
            Some("2020-04-01T23:23:00+09:00")
        );
        assert_eq!(
            parse_datetime(" 2021/05/01 12:34 ").as_deref(),
            Some("2021-05-01T12:34:00+09:00")
        );
        assert_eq!(
            parse_datetime("2021/05/01").as_deref(),
            Some("2021-05-01T00:00:00+09:00"),
            "a date with no time is midnight"
        );
        assert_eq!(parse_datetime("not a date"), None);
        assert_eq!(
            parse_datetime("2020/13/01 00:00"),
            None,
            "month 13 is not a month"
        );
        assert_eq!(
            parse_datetime("2020/01/01 25:00"),
            None,
            "hour 25 is not an hour"
        );
    }

    #[test]
    fn chapter_indices_come_from_the_url() {
        assert_eq!(chapter_index_from_href("/n0608gd/1/"), Some(1));
        assert_eq!(chapter_index_from_href("/n0608gd/42"), Some(42));
        assert_eq!(chapter_index_from_href("/n0608gd/"), None);
    }

    #[test]
    fn the_author_label_is_removed_when_present() {
        assert_eq!(
            strip_author_prefix("作者：ウスバー（正直者） "),
            "ウスバー（正直者）"
        );
        assert_eq!(strip_author_prefix("作者: かずは"), "かずは");
        assert_eq!(strip_author_prefix(" 名前だけ "), "名前だけ");
    }
}
