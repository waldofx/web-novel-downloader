//! Narou-family adapter: `ncode.syosetu.com` and `novel18.syosetu.com`.
//!
//! The R18 sibling (`novel18.syosetu.com`) runs the same backend and markup, so
//! one adapter serves both; only the content rating differs, and it is derived
//! from the host rather than from the page.
//!
//! ## Ruby notation
//!
//! Narou encodes furigana in *plain text* rather than HTML:
//!
//! - `｜漢字《かんじ》` — explicit ruby, with `｜` marking the base's start
//! - `漢字《かんじ》` — implicit ruby, base auto-detected from the preceding kanji
//! - `《《強調》》` — the site's own emphasis
//!
//! These characters are markup, not punctuation, so they are decoded into the
//! node model here. A `｜` or `《` that has no usable base is kept as literal
//! text: dropping characters would be worse than showing a stray mark.

use crate::{Source, SourceError};
use scraper::{ElementRef, Html, Node, Selector};
use std::collections::HashSet;
use std::sync::LazyLock;
use url::Url;
use wnd_core::HttpClient;
use wnd_core::model::{Block, Chapter, ChapterMeta, ContentRating, Inline, Manifest, SourceInfo};
use wnd_core::storage::chapter_file_name;

pub struct Narou;

/// Hosts this adapter is responsible for.
pub const HOSTS: [&str; 2] = ["ncode.syosetu.com", "novel18.syosetu.com"];

/// The R18 host: works served from it are adult content.
pub const ADULT_HOST: &str = "novel18.syosetu.com";

/// Stop following `?p=` links after this many pages, so a misread pager or a
/// site bug cannot spin forever.
const MAX_TOC_PAGES: usize = 100;

macro_rules! selector {
    ($name:ident, $css:literal) => {
        static $name: LazyLock<Selector> =
            LazyLock::new(|| Selector::parse($css).expect("selector literals are valid"));
    };
}

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

/// One parsed table-of-contents page.
#[derive(Debug, Default)]
struct TocPage {
    title: Option<String>,
    author: Option<String>,
    synopsis: Option<String>,
    entries: Vec<ChapterMeta>,
    /// Arc in effect when the page ended, carried into the next page.
    end_arc: Option<String>,
    /// Index of the last chapter seen, so pagination keeps counting up.
    end_index: u32,
    next_page: Option<String>,
}

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
        parse_chapter(&html, meta)
    }
}

/// Parse one table-of-contents page.
///
/// `start_arc` and `start_index` carry state across pagination: arcs continue
/// from the previous page, and chapter numbering never restarts.
fn parse_toc_page(html: &str, base: &Url, start_arc: Option<String>, start_index: u32) -> TocPage {
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
            file: format!("chapters/{}", chapter_file_name(index)),
        });
    }

    page.end_arc = arc;
    page.end_index = index;
    page.next_page = pager_next(&document, base);
    page
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
        file: format!("chapters/{}", chapter_file_name(1)),
    }]
}

/// Parse a chapter page into the node model.
fn parse_chapter(html: &str, meta: &ChapterMeta) -> Result<Chapter, SourceError> {
    let document = Html::parse_document(html);
    let mut chapter = Chapter::new(meta.index, meta.title.clone());
    let mut containers = 0usize;

    for container in document.select(&SEL_TEXT_BLOCK) {
        containers += 1;
        let blocks = blocks_from_lines(container_lines(&container));
        if is_class(&container, "p-novel__text--preface") {
            chapter.pre_note = blocks;
        } else if is_class(&container, "p-novel__text--afterword") {
            chapter.post_note = blocks;
        } else {
            chapter.nodes = blocks;
        }
    }

    if containers == 0 {
        return Err(SourceError::Parse {
            url: meta.url.clone(),
            message: "no chapter text found; the site markup may have changed".to_string(),
        });
    }
    Ok(chapter)
}

/// Each line of a text container, as `None` for a blank line.
///
/// Narou emits one `<p>` per line; a blank line is a `<p>` whose only content
/// is a `<br>`.
fn container_lines(container: &ElementRef) -> Vec<Option<Vec<Inline>>> {
    container
        .select(&SEL_PARAGRAPH)
        .map(|paragraph| {
            if element_text(&paragraph).trim_ascii().is_empty() {
                None
            } else {
                Some(inlines_of(&paragraph))
            }
        })
        .collect()
}

/// Turn lines into blocks, applying Narou's blank-line convention:
/// a single blank is a paragraph separator and disappears, a run of two or more
/// becomes one [`Block::SceneBreak`], and leading/trailing blanks are dropped.
fn blocks_from_lines(lines: Vec<Option<Vec<Inline>>>) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut blanks = 0usize;

    for line in lines {
        match line {
            None => blanks += 1,
            Some(children) => {
                // `!blocks.is_empty()` drops a leading run of blanks.
                if blanks >= 2 && !blocks.is_empty() {
                    blocks.push(Block::SceneBreak);
                }
                blanks = 0;
                if !children.is_empty() {
                    blocks.push(Block::Paragraph { children });
                }
            }
        }
    }
    blocks
}

/// The inline content of one line.
fn inlines_of(element: &ElementRef) -> Vec<Inline> {
    let mut builder = InlineBuilder::default();
    walk_children(element, &mut builder);
    builder.finish()
}

/// Collects inline nodes, decoding Narou's plain-text notation in each run of
/// text between elements.
#[derive(Default)]
struct InlineBuilder {
    nodes: Vec<Inline>,
    pending: String,
}

impl InlineBuilder {
    fn push_text(&mut self, text: &str) {
        self.pending.push_str(text);
    }

    fn push_inline(&mut self, node: Inline) {
        self.flush();
        self.nodes.push(node);
    }

    fn finish(mut self) -> Vec<Inline> {
        self.flush();
        normalize(self.nodes)
    }

    fn flush(&mut self) {
        if !self.pending.is_empty() {
            let text = std::mem::take(&mut self.pending);
            self.nodes.extend(parse_notation(&text));
        }
    }
}

/// Walk the child nodes of an element, because text and markup are siblings.
fn walk_children(element: &ElementRef, builder: &mut InlineBuilder) {
    let mut child = element.first_child();
    while let Some(node) = child {
        // Advance the cursor before using `node`; the nodes are reference counted.
        child = node.next_sibling();
        match node.value() {
            Node::Text(text) => builder.push_text(text),
            Node::Element(el) if el.name() == "br" => builder.push_text("\n"),
            Node::Element(el) if el.name() == "ruby" => {
                if let Some(element) = ElementRef::wrap(node) {
                    builder.push_inline(ruby_of(&element));
                }
            }
            Node::Element(el) => {
                if let Some(element) = ElementRef::wrap(node) {
                    match el.name() {
                        "b" | "strong" => {
                            let children = inlines_of(&element);
                            builder.push_inline(Inline::Strong { children });
                        }
                        "em" | "i" => {
                            let children = inlines_of(&element);
                            builder.push_inline(Inline::Emphasis { children });
                        }
                        "a" => {
                            let href = element.value().attr("href").unwrap_or("").to_string();
                            let children = inlines_of(&element);
                            builder.push_inline(Inline::Link { href, children });
                        }
                        // Inline images do not occur in Narou bodies; anything
                        // else (span, u, s, font, …) is a transparent wrapper.
                        "img" => {}
                        _ => walk_children(&element, builder),
                    }
                }
            }
            _ => {}
        }
    }
}

/// `<ruby>漢字<rt>かんじ</rt></ruby>` → [`Inline::Ruby`].
fn ruby_of(element: &ElementRef) -> Inline {
    let mut base = String::new();
    let mut reading = String::new();
    collect_ruby(element, RubyPart::Base, &mut base, &mut reading);
    Inline::Ruby {
        base: base.trim_ascii().to_string(),
        reading: reading.trim_ascii().to_string(),
    }
}

#[derive(Clone, Copy)]
enum RubyPart {
    Base,
    Reading,
    Ignore,
}

fn collect_ruby(element: &ElementRef, part: RubyPart, base: &mut String, reading: &mut String) {
    let mut child = element.first_child();
    while let Some(node) = child {
        child = node.next_sibling();
        match node.value() {
            Node::Text(text) => match part {
                RubyPart::Base => base.push_str(text),
                RubyPart::Reading => reading.push_str(text),
                RubyPart::Ignore => {}
            },
            Node::Element(el) => {
                let part = match el.name() {
                    "rt" => RubyPart::Reading,
                    "rp" => RubyPart::Ignore,
                    _ => part,
                };
                if let Some(element) = ElementRef::wrap(node) {
                    collect_ruby(&element, part, base, reading);
                }
            }
            _ => {}
        }
    }
}

/// All text under an element, with `<br>` becoming a newline.
///
/// Whitespace touching a line break is dropped so `<br /> <br />` reads as two
/// breaks, and each line is trimmed of ASCII padding. U+3000 (Narou's
/// indentation) is content and survives.
fn element_text(element: &ElementRef) -> String {
    let mut out = String::new();
    collect_text(element, &mut out);
    if !out.contains('\n') {
        return out.trim_ascii().to_string();
    }
    out.split('\n')
        .map(|line| line.trim_ascii())
        .collect::<Vec<_>>()
        .join("\n")
        .trim_ascii()
        .to_string()
}

fn collect_text(element: &ElementRef, out: &mut String) {
    let mut child = element.first_child();
    while let Some(node) = child {
        child = node.next_sibling();
        match node.value() {
            Node::Text(text) => out.push_str(text),
            Node::Element(el) if el.name() == "br" => out.push('\n'),
            Node::Element(_) => {
                if let Some(element) = ElementRef::wrap(node) {
                    collect_text(&element, out);
                }
            }
            _ => {}
        }
    }
}

/// Decode Narou's plain-text ruby and emphasis notation into inline nodes.
///
/// Text without markup is returned as a single [`Inline::Text`].
fn parse_notation(text: &str) -> Vec<Inline> {
    let chars: Vec<char> = text.chars().collect();
    let mut nodes: Vec<Inline> = Vec::new();
    let mut plain = String::new();
    let mut i = 0usize;

    while i < chars.len() {
        // Emphasis: 《《 … 》》
        if chars[i] == '《' && chars.get(i + 1) == Some(&'《') {
            if let Some(end) = find_index(&chars, i + 2, '》') {
                if chars.get(end + 1) == Some(&'》') {
                    push_text(&mut nodes, &plain);
                    plain.clear();
                    let inner: String = chars[i + 2..end].iter().collect();
                    nodes.push(Inline::Emphasis {
                        children: parse_notation(&inner),
                    });
                    i = end + 2;
                    continue;
                }
            }
        }

        // Ruby: base《reading》. The base is whatever precedes the 《 in the
        // pending text — after a ｜ if one is present, else the trailing kanji.
        if chars[i] == '《' {
            if let Some(end) = find_index(&chars, i + 1, '》') {
                let reading: String = chars[i + 1..end].iter().collect();
                if let Some((keep, base)) = split_ruby_base(&plain) {
                    let keep = keep.to_string();
                    let base = base.to_string();
                    plain.clear();
                    push_text(&mut nodes, &keep);
                    nodes.push(Inline::Ruby { base, reading });
                    i = end + 1;
                    continue;
                }
            }
        }

        plain.push(chars[i]);
        i += 1;
    }

    push_text(&mut nodes, &plain);
    normalize(nodes)
}

/// First index at or after `from` holding `close`.
fn find_index(chars: &[char], from: usize, close: char) -> Option<usize> {
    chars
        .iter()
        .enumerate()
        .skip(from)
        .find(|(_, c)| **c == close)
        .map(|(index, _)| index)
}

/// Split pending text into the part to keep and the ruby base.
///
/// `None` means the 《 has no base and is therefore literal prose.
fn split_ruby_base(before: &str) -> Option<(&str, &str)> {
    if let Some(marker) = before.rfind('｜') {
        // Explicit: everything after ｜ is the base, even if it is kana.
        let base = &before[marker + '｜'.len_utf8()..];
        if base.is_empty() {
            return None;
        }
        return Some((&before[..marker], base));
    }

    // Implicit: the base is the longest run of kanji at the end.
    let start = before
        .char_indices()
        .rev()
        .take_while(|(_, c)| is_kanji(*c))
        .last()
        .map(|(index, _)| index)?;
    Some((&before[..start], &before[start..]))
}

/// Whether a character can implicitly start a ruby base.
fn is_kanji(c: char) -> bool {
    matches!(c,
        '\u{3005}'                  // 々, the iteration mark
        | '\u{3400}'..='\u{4DBF}'   // CJK Unified Ideographs Extension A
        | '\u{4E00}'..='\u{9FFF}'   // CJK Unified Ideographs
        | '\u{F900}'..='\u{FAFF}'   // CJK Compatibility Ideographs
    )
}

fn push_text(nodes: &mut Vec<Inline>, text: &str) {
    if !text.is_empty() {
        nodes.push(Inline::Text {
            value: text.to_string(),
        });
    }
}

/// Merge adjacent text nodes and drop empty ones.
fn normalize(nodes: Vec<Inline>) -> Vec<Inline> {
    let mut out: Vec<Inline> = Vec::new();
    for node in nodes {
        match node {
            Inline::Text { value } if value.is_empty() => {}
            Inline::Text { value } => match out.last_mut() {
                Some(Inline::Text { value: previous }) => previous.push_str(&value),
                _ => out.push(Inline::Text { value }),
            },
            other => out.push(other),
        }
    }
    out
}

/// The first element matching `selector`, as raw text.
fn first_text(document: &Html, selector: &Selector) -> Option<String> {
    document
        .select(selector)
        .next()
        .map(|element| element_text(&element))
}

fn is_class(element: &ElementRef, class: &str) -> bool {
    element.value().classes().any(|name| name == class)
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

    fn text(value: &str) -> Inline {
        Inline::Text {
            value: value.to_string(),
        }
    }

    /// A table-of-contents page mirroring the real Narou markup: an arc heading,
    /// two chapters, dates (one with a revision note), and an ideographic space
    /// in the title that trimming must not eat.
    const TOC_HTML: &str = r#"<!DOCTYPE html><html lang="ja"><head><meta charset="utf-8"></head><body>
<div class="p-novel">
  <h1 class="p-novel__title">二重勇者はすごいです！　～テスト～</h1>
  <div class="p-novel__author">作者：ウスバー（正直者） </div>
  <div id="novel_ex" class="p-novel__summary">――ただ、強い！！<br /> <br /> 二年前に勇者として召喚された。</div>
  <div class="p-eplist">
    <div class="p-eplist__chapter-title">第一章　勇者、日本に帰る</div>
    <div class="p-eplist__sublist">
      <a href="/n0608gd/1/" class="p-eplist__subtitle"> １．勇者の帰還 </a>
      <div class="p-eplist__update"> 2020/04/01 23:23 <span title="2023/09/01 14:36 改稿">（<u>改</u>）</span> </div>
    </div>
    <div class="p-eplist__sublist">
      <a href="/n0608gd/2/" class="p-eplist__subtitle"> ２．日本 </a>
      <div class="p-eplist__update"> 2021/04/01 00:00 </div>
    </div>
    <div class="p-eplist__chapter-title">第二章　再会</div>
    <div class="p-eplist__sublist">
      <a href="/n0608gd/3/" class="p-eplist__subtitle"> ３．新たなる力 </a>
      <div class="p-eplist__update"> 2021/05/01 12:34 </div>
    </div>
  </div>
  <div class="c-pager">
    <a href="?p=2" class="c-pager__item c-pager__item--next">次へ</a>
  </div>
</div></body></html>"#;

    const CHAPTER_HTML: &str = r#"<!DOCTYPE html><html lang="ja"><head><meta charset="utf-8"></head><body>
<div class="p-novel__body">
  <div class="js-novel-text p-novel__text p-novel__text--preface">
    <p id="Lp1">いよいよ新連載始めます！</p>
    <p id="Lp2">応援よろしくお願いします！</p>
  </div>
  <div class="js-novel-text p-novel__text">
    <p id="L1">　最初の行。</p>
    <p id="L2"><br /></p>
    <p id="L3">　一つの空行は段落。</p>
    <p id="L4"><br /></p>
    <p id="L5"><br /></p>
    <p id="L6">　二つの空行は場面転換。</p>
    <p id="L7">　<ruby>漢字<rt>かんじ</rt></ruby>と<b>強調</b>です。</p>
    <p id="L8">　今日は｜漢字《かんじ》です。</p>
    <p id="L9">　かな《かな》は字《じ》です。</p>
    <p id="L10"><br /></p>
  </div>
  <div class="js-novel-text p-novel__text p-novel__text--afterword">
    <p id="La1">次回もよろしく。</p>
  </div>
</div></body></html>"#;

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
    fn toc_metadata_arcs_and_chapters_are_parsed() {
        let page = parse_toc_page(
            TOC_HTML,
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
            TOC_HTML,
            &url("https://ncode.syosetu.com/n0608gd/"),
            None,
            0,
        );
        // The next page must continue numbering and keep the arc in effect.
        assert_eq!(page.end_index, 3);
        assert_eq!(page.end_arc.as_deref(), Some("第二章　再会"));
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
    }

    fn chapter_meta(index: u32) -> ChapterMeta {
        ChapterMeta {
            index,
            title: format!("第{index}話"),
            url: format!("https://ncode.syosetu.com/n0608gd/{index}/"),
            arc: None,
            published_at: None,
            hash: None,
            file: format!("chapters/{}", chapter_file_name(index)),
        }
    }

    #[test]
    fn chapter_notes_and_body_are_split() {
        let chapter = parse_chapter(CHAPTER_HTML, &chapter_meta(1)).unwrap();

        assert_eq!(chapter.index, 1);
        assert_eq!(chapter.pre_note.len(), 2, "preface lines become paragraphs");
        assert_eq!(chapter.post_note.len(), 1, "afterword is kept separate");

        let preface = &chapter.pre_note[0];
        assert_eq!(
            *preface,
            Block::Paragraph {
                children: vec![text("いよいよ新連載始めます！")]
            }
        );
    }

    #[test]
    fn blank_line_runs_become_scene_breaks() {
        let chapter = parse_chapter(CHAPTER_HTML, &chapter_meta(1)).unwrap();

        // L1, L3 (single blank between them dropped), scene break, then L6-L9.
        assert_eq!(chapter.nodes.len(), 7, "blocks: {:?}", chapter.nodes);
        assert_eq!(
            chapter.nodes[0],
            Block::Paragraph {
                children: vec![text("　最初の行。")]
            }
        );
        assert_eq!(
            chapter.nodes[1],
            Block::Paragraph {
                children: vec![text("　一つの空行は段落。")]
            },
            "one blank line is just a paragraph break"
        );
        assert_eq!(
            chapter.nodes[2],
            Block::SceneBreak,
            "two blanks are a scene break"
        );
        assert_eq!(
            chapter.nodes[3],
            Block::Paragraph {
                children: vec![text("　二つの空行は場面転換。")]
            },
            "the paragraph that follows a scene break"
        );
    }

    #[test]
    fn ideographic_indentation_survives_but_ascii_padding_is_trimmed() {
        let chapter = parse_chapter(CHAPTER_HTML, &chapter_meta(1)).unwrap();
        match &chapter.nodes[0] {
            Block::Paragraph { children } => assert_eq!(children, &vec![text("　最初の行。")]),
            other => panic!("expected a paragraph, got {other:?}"),
        }
    }

    #[test]
    fn html_ruby_becomes_an_inline_ruby_node() {
        let chapter = parse_chapter(CHAPTER_HTML, &chapter_meta(1)).unwrap();
        // nodes: [L1, L3, SceneBreak, L6, L7, L8, L9]
        let line = &chapter.nodes[4];
        assert_eq!(
            *line,
            Block::Paragraph {
                children: vec![
                    text("　"),
                    Inline::Ruby {
                        base: "漢字".into(),
                        reading: "かんじ".into()
                    },
                    text("と"),
                    Inline::Strong {
                        children: vec![text("強調")]
                    },
                    text("です。"),
                ]
            }
        );
    }

    #[test]
    fn plain_text_ruby_notation_is_decoded() {
        let chapter = parse_chapter(CHAPTER_HTML, &chapter_meta(1)).unwrap();
        // `　今日は｜漢字《かんじ》です。` — ｜ marks an explicit base.
        assert_eq!(
            chapter.nodes[5],
            Block::Paragraph {
                children: vec![
                    text("　今日は"),
                    Inline::Ruby {
                        base: "漢字".into(),
                        reading: "かんじ".into()
                    },
                    text("です。")
                ]
            }
        );
        // `　かな《かな》は字《じ》です。` — no kanji precedes the first 《, so it
        // stays literal; the second has an implicit kanji base.
        assert_eq!(
            chapter.nodes[6],
            Block::Paragraph {
                children: vec![
                    text("　かな《かな》は"),
                    Inline::Ruby {
                        base: "字".into(),
                        reading: "じ".into()
                    },
                    text("です。")
                ]
            }
        );
    }

    #[test]
    fn explicit_ruby_accepts_a_kana_base() {
        assert_eq!(
            parse_notation("｜これ《これ》"),
            vec![Inline::Ruby {
                base: "これ".into(),
                reading: "これ".into()
            }]
        );
    }

    #[test]
    fn explicit_ruby_takes_the_longest_kanji_run_implicitly() {
        assert_eq!(
            parse_notation("今日は漢字《かんじ》"),
            vec![
                text("今日は"),
                Inline::Ruby {
                    base: "漢字".into(),
                    reading: "かんじ".into()
                }
            ]
        );
        assert_eq!(
            parse_notation("々《くりかえし》"),
            vec![Inline::Ruby {
                base: "々".into(),
                reading: "くりかえし".into()
            }]
        );
    }

    #[test]
    fn emphasis_notation_becomes_an_emphasis_node() {
        assert_eq!(
            parse_notation("あ《《強調》》い"),
            vec![
                text("あ"),
                Inline::Emphasis {
                    children: vec![text("強調")]
                },
                text("い")
            ]
        );
    }

    #[test]
    fn notation_without_markup_is_one_text_node() {
        assert_eq!(
            parse_notation("ただの文です。"),
            vec![text("ただの文です。")]
        );
        assert_eq!(parse_notation(""), vec![]);
    }

    #[test]
    fn a_lone_marker_is_kept_literally() {
        assert_eq!(parse_notation("終わり｜"), vec![text("終わり｜")]);
        assert_eq!(parse_notation("《開き》なし"), vec![text("《開き》なし")]);
    }

    #[test]
    fn blank_line_rules_are_applied_directly() {
        let line = |value: &str| {
            Some(vec![Inline::Text {
                value: value.to_string(),
            }])
        };
        let blocks = blocks_from_lines(vec![
            None,
            None,
            line("a"),
            None,
            line("b"),
            None,
            None,
            line("c"),
            None,
        ]);
        assert_eq!(
            blocks,
            vec![
                Block::Paragraph {
                    children: vec![text("a")]
                },
                Block::Paragraph {
                    children: vec![text("b")]
                },
                Block::SceneBreak,
                Block::Paragraph {
                    children: vec![text("c")]
                },
            ],
            "leading and trailing blanks drop; single blank drops; two become a scene break"
        );
    }

    #[test]
    fn chapter_pages_without_text_are_reported_as_parse_failures() {
        let error = parse_chapter(
            "<html><body><p>nothing here</p></body></html>",
            &chapter_meta(1),
        )
        .unwrap_err();
        assert!(matches!(error, SourceError::Parse { .. }), "got {error:?}");
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
