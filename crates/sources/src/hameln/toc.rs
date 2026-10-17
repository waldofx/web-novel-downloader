//! The table of contents: novel metadata, arcs, chapter rows, published dates,
//! and revision stamps.
//!
//! Hameln puts everything on one page — title, author, tags, the synopsis, and
//! every episode with its arc heading — so there is no pagination to follow.
//! Arc headings and chapter rows share one document-ordered selection, which is
//! what makes "the arc a chapter belongs to" simply *the heading seen most
//! recently*.

use super::{
    SEL_AUTHOR, SEL_CHAPTER_TITLE, SEL_EPISODE_DATE, SEL_EPISODE_LINK, SEL_EPISODE_LIST,
    SEL_EPISODE_REVISION, SEL_EPISODE_TITLE, SEL_SS, SEL_TAGS, SEL_TITLE,
};
use crate::dom::{element_text, first_text, is_class};
use scraper::{ElementRef, Html};
use url::Url;
use wnd_core::ChapterMeta;
use wnd_core::store::storage::chapter_file_name;

/// One parsed table of contents.
#[derive(Debug, Default)]
pub struct Toc {
    pub title: Option<String>,
    pub author: Option<String>,
    pub tags: Vec<String>,
    pub synopsis: Option<String>,
    pub entries: Vec<ChapterMeta>,
    /// Arc in effect when the list ended, carried for completeness.
    pub end_arc: Option<String>,
    pub end_index: u32,
}

/// Parse a table-of-contents page.
pub fn parse_toc(html: &str, base: &Url) -> Toc {
    let document = Html::parse_document(html);

    let mut toc = Toc {
        title: first_text(&document, &SEL_TITLE),
        author: first_text(&document, &SEL_AUTHOR).map(|text| strip_author_prefix(&text)),
        tags: collect_tags(&document),
        synopsis: synopsis_of(&document),
        ..Toc::default()
    };

    let mut arc: Option<String> = None;
    let mut index: u32 = 0;

    // Walk the episode list in document order: headings and rows are siblings.
    if let Some(list) = document.select(&SEL_EPISODE_LIST).next() {
        for child in list.children() {
            let Some(element) = ElementRef::wrap(child) else {
                continue;
            };
            if is_class(&element, "episode-list__chapter") {
                if let Some(title) = element
                    .select(&SEL_CHAPTER_TITLE)
                    .next()
                    .map(|e| element_text(&e))
                {
                    let title = title.trim().to_string();
                    if !title.is_empty() {
                        arc = Some(title);
                    }
                }
                continue;
            }
            if !is_class(&element, "episode-list__item") {
                continue;
            }

            let link = match element.select(&SEL_EPISODE_LINK).next() {
                Some(link) => link,
                None => continue,
            };
            let href = link.value().attr("href");
            index = href.and_then(chapter_index_from_href).unwrap_or(index + 1);

            let url = href
                .and_then(|href| base.join(href).ok())
                .unwrap_or_else(|| {
                    base.join(&format!("{index}.html"))
                        .expect("a relative join cannot fail")
                });

            let title = element
                .select(&SEL_EPISODE_TITLE)
                .next()
                .map(|e| element_text(&e))
                .unwrap_or_default();

            let published_at = element.select(&SEL_EPISODE_DATE).next().and_then(|date| {
                // Prefer the machine-readable attribute when present.
                date.value()
                    .attr("datetime")
                    .and_then(parse_datetime)
                    .or_else(|| parse_datetime(&element_text(&date)))
            });

            let revised_at = element
                .select(&SEL_EPISODE_REVISION)
                .next()
                .and_then(|revision| revision.value().attr("title"))
                .and_then(parse_datetime);

            toc.entries.push(ChapterMeta {
                index,
                title,
                url: url.to_string(),
                arc: arc.clone(),
                published_at,
                revised_at,
                hash: None,
                file: format!("chapters/{}", chapter_file_name(index)),
            });
        }
    }

    toc.end_arc = arc;
    toc.end_index = index;
    toc
}

fn collect_tags(document: &Html) -> Vec<String> {
    document
        .select(&SEL_TAGS)
        .map(|element| element_text(&element))
        .filter(|text| !text.is_empty())
        .collect()
}

fn synopsis_of(document: &Html) -> Option<String> {
    // The synopsis lives in the second `div.ss`, mixed with illustrations.
    // Take its text and keep only the leading paragraph.
    let sections: Vec<String> = document
        .select(&SEL_SS)
        .map(|element| element_text(&element))
        .collect();
    // The first section is the title/author/tags block; the second is the body.
    let body = sections.get(1)?;
    let synopsis = body
        .split("\n\n")
        .map(str::trim)
        .find(|chunk| !chunk.is_empty() && !chunk.contains("挿絵"))?;
    if synopsis.is_empty() {
        None
    } else {
        Some(synopsis.to_string())
    }
}

fn strip_author_prefix(text: &str) -> String {
    text.trim().to_string()
}

fn chapter_index_from_href(href: &str) -> Option<u32> {
    // `.../316401/1.html` or `.../316401/1/`
    let last = href.trim_end_matches('/').rsplit('/').next()?;
    let stem = last.strip_suffix(".html").unwrap_or(last);
    stem.parse().ok()
}

fn parse_datetime(text: &str) -> Option<String> {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    for (position, token) in tokens.iter().enumerate() {
        let Some(date) = parse_date(token) else {
            continue;
        };
        let (hour, minute) = match tokens.get(position + 1) {
            Some(token) if token.contains(':') => parse_time(token)?,
            _ => (0, 0),
        };
        return Some(format!("{date}T{hour:02}:{minute:02}:00+09:00"));
    }
    None
}

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

fn parse_time(token: &str) -> Option<(u32, u32)> {
    let (hour, rest) = token.split_once(':')?;
    if hour.len() != 2 {
        return None;
    }
    let hour_number: u32 = hour.parse().ok()?;
    if rest.len() < 2 {
        return None;
    }
    let minute: u32 = rest[..2].parse().ok()?;
    if hour_number > 23 || minute > 59 {
        return None;
    }
    // The minute must not be followed by another digit: `05:412` is not a time.
    if rest[2..].chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    Some((hour_number, minute))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::HAMELN_WORK;
    use url::Url;

    fn base() -> Url {
        Url::parse("https://syosetu.org/novel/316401/").unwrap()
    }

    #[test]
    fn the_work_page_yields_its_title_and_author() {
        let toc = parse_toc(HAMELN_WORK, &base());
        assert_eq!(toc.title.as_deref(), Some("〝アリウス〟潰すゾ!!!"));
        assert_eq!(toc.author.as_deref(), Some("あば茶"));
    }

    #[test]
    fn the_work_page_yields_real_tags_only() {
        let toc = parse_toc(HAMELN_WORK, &base());
        assert!(toc.tags.contains(&"ヤンデレ".to_string()));
        assert!(toc.tags.contains(&"転生".to_string()));
        assert!(
            !toc.tags
                .iter()
                .any(|t| t == "お気に入り済み" || t == "評価済み"),
            "UI markers are not tags: {:?}",
            toc.tags
        );
    }

    #[test]
    fn the_work_page_yields_a_synopsis() {
        let toc = parse_toc(HAMELN_WORK, &base());
        assert!(
            toc.synopsis.as_deref().unwrap_or("").contains("オリ主くん"),
            "got: {:?}",
            toc.synopsis
        );
    }

    #[test]
    fn the_work_page_lists_every_episode() {
        let toc = parse_toc(HAMELN_WORK, &base());
        assert_eq!(toc.entries.len(), 535);
    }

    #[test]
    fn episodes_are_numbered_from_one_without_gaps() {
        let toc = parse_toc(HAMELN_WORK, &base());
        let indices: Vec<u32> = toc.entries.iter().map(|e| e.index).collect();
        assert_eq!(indices, (1..=535).collect::<Vec<_>>());
    }

    #[test]
    fn every_episode_has_its_own_url() {
        let toc = parse_toc(HAMELN_WORK, &base());
        let urls: std::collections::HashSet<&str> =
            toc.entries.iter().map(|e| e.url.as_str()).collect();
        assert_eq!(urls.len(), toc.entries.len());
    }

    #[test]
    fn arcs_are_kept() {
        let toc = parse_toc(HAMELN_WORK, &base());
        assert!(toc.entries.iter().any(|e| e.arc.is_some()));
        assert!(toc.entries.iter().all(|e| e.arc.is_some()));
        let arcs: std::collections::HashSet<&str> = toc
            .entries
            .iter()
            .filter_map(|e| e.arc.as_deref())
            .collect();
        assert!(arcs.len() > 5, "got {} arcs", arcs.len());
    }

    #[test]
    fn published_dates_are_rfc3339() {
        let toc = parse_toc(HAMELN_WORK, &base());
        assert!(
            toc.entries[0]
                .published_at
                .as_deref()
                .unwrap_or("")
                .ends_with("+09:00"),
            "got: {:?}",
            toc.entries[0].published_at
        );
    }

    #[test]
    fn a_revised_episode_carries_a_revision_stamp() {
        let toc = parse_toc(HAMELN_WORK, &base());
        let revised = toc.entries.iter().find(|e| e.revised_at.is_some()).unwrap();
        assert!(
            revised.revised_at.as_deref().unwrap().ends_with("+09:00"),
            "got: {:?}",
            revised.revised_at
        );
        // An unrevised row carries no stamp.
        assert!(toc.entries.iter().any(|e| e.revised_at.is_none()));
    }

    #[test]
    fn a_missing_title_is_not_this_site() {
        // Sanity: the parser does not invent a title for an empty page.
        let toc = parse_toc("", &base());
        assert!(toc.title.is_none());
        assert!(toc.entries.is_empty());
    }
}
