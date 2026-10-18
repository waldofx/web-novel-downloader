//! A pixiv novel's text into the node model.
//!
//! The site hands over the body as plain text with a small bracket notation,
//! and the notation is the whole difficulty — there is no HTML to lean on:
//!
//! - `[[rb:base > reading]]` is furigana. Once decoded it is the same fact as
//!   Narou's `｜base《reading》`, which is why it lands in [`Inline::Ruby`]
//!   rather than in the text;
//! - `[[jumpuri:label > url]]` is a link;
//! - `[chapter:Title]` is a divider the **author** typed. A pixiv novel has no
//!   chapters of its own — the work is the novel, or the series it belongs to —
//!   so this is a heading, never a chapter split;
//! - `[uploadedimage:ID]` is an image the author placed in this novel, and the
//!   novel's own answer already carries its URL;
//! - `[pixivimage:ID]` / `[pixivimage:ID-N]` is *someone else's illustration*,
//!   which takes a request to resolve. This module does not make requests, so
//!   `images` arrives already resolved by the caller.
//!
//! Two rules follow from what the site does *not* do:
//!
//! - **an unresolvable marker is left exactly as written.** A `[pixivimage:…]`
//!   for an illustration that is gone, or one this adapter cannot make sense
//!   of, must not cost the reader the chapter it appears in. The marker stays
//!   visible, which is also how they will report it;
//! - indentation is U+3000 and is content, so every trim here is ASCII-only.

use crate::blocks;
use crate::inline::{self, Piece};
use std::collections::HashMap;
use wnd_core::{Block, Inline};

/// The URL each image reference in a novel's text resolves to.
///
/// Keyed by the reference exactly as the text writes it — `21674723`,
/// `12345678-2` — so a marker with no entry is one that could not be resolved
/// and is left as written.
pub type Images = HashMap<String, String>;

/// The site's marker for a page of its reader.
const NEW_PAGE: &str = "[newpage]";

/// Parse one novel's text.
pub fn parse(content: &str, images: &Images) -> Vec<Block> {
    let joined = join_pages(content);

    blocks::from_line_blocks(
        joined
            .split('\n')
            .map(|line| (!is_blank(line)).then(|| line_blocks(line, images)))
            .collect(),
    )
}

/// The `[pixivimage:…]` references a novel makes, in order and without repeats.
///
/// The caller resolves each of these — one request apiece — and hands the
/// result back to [`parse`]. Uploaded images are not listed: the novel's own
/// answer already carries them, so the caller has them without asking.
pub fn pixiv_images(content: &str) -> Vec<String> {
    let mut references: Vec<String> = Vec::new();
    let mut rest = content;

    while let Some(at) = rest.find("[pixivimage:") {
        rest = &rest[at..];
        let Some((reference, len)) = bracketed(rest, "[pixivimage:", "]") else {
            break; // nothing closes a marker from here on, so there is none
        };
        let reference = reference.trim_ascii();
        if !reference.is_empty() && !references.iter().any(|seen| seen == reference) {
            references.push(reference.to_string());
        }
        rest = &rest[len..];
    }

    references
}

/// Join the reader's pages of one chapter into the text the author wrote.
///
/// `[newpage]` is a page of the site's reader, not a chapter and not a scene
/// break: the body arrives whole and is shown in slices. The marker sits on a
/// line of its own with the paragraph's blank line on either side, so dropping
/// the newlines at the boundary and joining with one blank line puts back
/// exactly what the page break interrupted.
///
/// The one thing this cannot express is a scene break straddling a page
/// boundary — two blank lines on the far side, minus the ones the boundary
/// needs. The naive joins are both worse: leaving the marker in invents a
/// paragraph per page, and replacing it with nothing invents a *scene break*
/// per page, because the blank lines on either side of it meet.
fn join_pages(content: &str) -> String {
    content
        .split(NEW_PAGE)
        .map(|page| page.trim_matches('\n'))
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// A blank line: nothing but ASCII padding.
///
/// U+3000 is not ASCII, so a line holding no more than the paragraph indent is
/// a paragraph of one indent — the same reading Kakuyomu's blank lines get, and
/// the reason `trim` must never be used here.
fn is_blank(line: &str) -> bool {
    line.trim_ascii().is_empty()
}

/// One line's blocks: prose, with whatever markers it holds decoded.
///
/// An image marker is a block of its own and the prose sharing the line with it
/// goes beside it, in the order the author wrote it — which is the same rule an
/// `<img>` in a body gets, so it lives in [`crate::blocks`].
fn line_blocks(line: &str, images: &Images) -> Vec<Block> {
    if let Some(title) = heading(line) {
        return vec![Block::Heading {
            level: 2,
            children: inline::plain_text(&title),
        }];
    }

    blocks::from_pieces(pieces(line, images))
}

/// The title of a `[chapter:…]` line, when that is the whole line.
///
/// An author who writes the marker *inside* a sentence is writing about the
/// notation. Reading that one as a heading would move their prose above the
/// rest of the line, so a marker that is not the whole line stays text.
fn heading(line: &str) -> Option<String> {
    let line = line.trim_ascii();
    let (title, len) = bracketed(line, "[chapter:", "]")?;

    if len != line.len() {
        return None;
    }

    let title = title.trim_ascii();
    (!title.is_empty()).then(|| title.to_string())
}

/// One line, split into the markers it holds and the text between them.
///
/// The markers that are inline content join the run they sit in; the two that
/// name an illustration end it, because a picture cannot live inside a paragraph
/// and the prose around it becomes a paragraph of its own.
fn pieces(line: &str, images: &Images) -> Vec<Piece> {
    let mut pieces = Vec::new();
    let mut run: Vec<Inline> = Vec::new();
    let mut text = String::new();
    let mut rest = line;

    while !rest.is_empty() {
        match marker(rest, images) {
            Some((Marker::Inline(node), len)) => {
                flush_text(&mut text, &mut run);
                run.push(node);
                rest = &rest[len..];
            }
            Some((Marker::Image(src), len)) => {
                flush_text(&mut text, &mut run);
                if !run.is_empty() {
                    pieces.push(Piece::Inline(std::mem::take(&mut run)));
                }
                pieces.push(Piece::Image { src, alt: None });
                rest = &rest[len..];
            }
            None => {
                // Not a marker, or one that stays as written: take one character
                // and look again from the next one.
                let next = rest.chars().next().expect("the loop keeps it non-empty");
                text.push(next);
                rest = &rest[next.len_utf8()..];
            }
        }
    }

    flush_text(&mut text, &mut run);
    if !run.is_empty() {
        pieces.push(Piece::Inline(run));
    }

    pieces
}

/// The text read so far becomes one node of the run, as the decoder would have
/// it: the site's notation has no markup *within* a run of text.
fn flush_text(text: &mut String, run: &mut Vec<Inline>) {
    if !text.is_empty() {
        run.push(Inline::Text {
            value: std::mem::take(text),
        });
    }
}

/// A marker this site's notation offers, before the line is put back together.
enum Marker {
    /// Inline content: ruby, a link.
    Inline(Inline),
    /// Somebody else's illustration, already resolved to an address.
    Image(String),
}

/// The marker at the very start of `text`, decoded, with the marker's length.
///
/// `None` covers three cases on purpose: there is no marker here, there is one
/// this adapter does not know, and there is one it knows but cannot resolve.
/// All three read literally, which is what keeps an unknown marker in the text
/// the reader sees instead of deleting it.
fn marker(text: &str, images: &Images) -> Option<(Marker, usize)> {
    if let Some((body, len)) = bracketed(text, "[[rb:", "]]") {
        let (base, reading) = body.split_once('>')?;
        let (base, reading) = (base.trim_ascii(), reading.trim_ascii());
        if base.is_empty() || reading.is_empty() {
            return None;
        }
        return Some((
            Marker::Inline(Inline::Ruby {
                base: base.to_string(),
                reading: reading.to_string(),
            }),
            len,
        ));
    }

    if let Some((body, len)) = bracketed(text, "[[jumpuri:", "]]") {
        let (label, href) = body.split_once('>')?;
        let (label, href) = (label.trim_ascii(), href.trim_ascii());
        if href.is_empty() {
            return None;
        }
        return Some((
            Marker::Inline(Inline::Link {
                href: href.to_string(),
                children: inline::plain_text(label),
            }),
            len,
        ));
    }

    if let Some((reference, len)) = bracketed(text, "[uploadedimage:", "]") {
        let url = images.get(reference.trim_ascii())?;
        return Some((Marker::Image(url.clone()), len));
    }

    if let Some((reference, len)) = bracketed(text, "[pixivimage:", "]") {
        let url = images.get(reference.trim_ascii())?;
        return Some((Marker::Image(url.clone()), len));
    }

    None
}

/// The text between `open` and the next `close`, with the whole marker's
/// length. `None` when `text` does not start with `open`, or never closes it.
fn bracketed<'a>(text: &'a str, open: &str, close: &str) -> Option<(&'a str, usize)> {
    let rest = text.strip_prefix(open)?;
    let end = rest.find(close)?;
    Some((&rest[..end], open.len() + end + close.len()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;

    fn text(value: &str) -> Inline {
        Inline::Text {
            value: value.to_string(),
        }
    }

    fn paragraph(value: &str) -> Block {
        Block::Paragraph {
            children: vec![text(value)],
        }
    }

    fn ruby(base: &str, reading: &str) -> Inline {
        Inline::Ruby {
            base: base.to_string(),
            reading: reading.to_string(),
        }
    }

    fn image(src: &str) -> Block {
        Block::Image {
            src: src.to_string(),
            alt: None,
        }
    }

    /// The indentation the site's authors use, as a literal a reader of the test
    /// can see: U+3000, not a space, and never trimmed.
    const INDENT: &str = "\u{3000}";

    #[test]
    fn the_pages_of_one_chapter_are_joined_rather_than_stopped_at() {
        // The reader shows a long chapter in slices; the API hands over the
        // whole text with `[newpage]` where the slices meet. Stopping at the
        // first one downloads a fifth of the chapter and says nothing.
        let content = format!("{INDENT}一ページ目。\n\n[newpage]\n\n{INDENT}二ページ目。");

        assert_eq!(
            parse(&content, &Images::new()),
            vec![paragraph("　一ページ目。"), paragraph("　二ページ目。")],
            "a page break is not a paragraph of its own, and not a scene break"
        );
    }

    #[test]
    fn a_page_break_where_a_scene_break_would_be_is_still_only_a_paragraph_break() {
        // The marker arrives with the blank line a paragraph separator already
        // has on either side. Replacing it with nothing would leave two blank
        // lines facing each other and invent a scene break at every page.
        let content = format!("{INDENT}前。\n\n[newpage]\n\n{INDENT}後。");

        assert_eq!(
            parse(&content, &Images::new()),
            vec![paragraph("　前。"), paragraph("　後。")]
        );
    }

    #[test]
    fn a_blank_line_separates_and_two_make_a_scene_break() {
        let content = format!("{INDENT}a\n\n{INDENT}b\n\n\n{INDENT}c");

        assert_eq!(
            parse(&content, &Images::new()),
            vec![
                paragraph("　a"),
                paragraph("　b"),
                Block::SceneBreak,
                paragraph("　c")
            ]
        );
    }

    #[test]
    fn the_ideographic_indent_is_content_and_survives() {
        // Every trim in this module is ASCII-only. A `trim()` would eat the
        // indent of every paragraph in every pixiv novel ever written.
        assert_eq!(
            parse("　本文　", &Images::new()),
            vec![paragraph("　本文　")],
            "the trailing U+3000 is the author's, not padding"
        );
    }

    #[test]
    fn furigana_is_decoded_from_the_bracket_notation() {
        let content = "　[[rb:超能力 > エスパー]]の少年。";

        assert_eq!(
            parse(content, &Images::new()),
            vec![Block::Paragraph {
                children: vec![text("　"), ruby("超能力", "エスパー"), text("の少年。")],
            }]
        );
    }

    #[test]
    fn a_broken_furigana_marker_stays_as_the_author_wrote_it() {
        for content in ["[[rb:読めない]]", "[[rb: > 空]]", "[[rb: ]]"] {
            assert_eq!(
                parse(content, &Images::new()),
                vec![paragraph(content)],
                "nothing to decode means nothing to change: {content}"
            );
        }
    }

    #[test]
    fn an_image_reference_becomes_an_image_block() {
        let mut images = Images::new();
        images.insert(
            "21674723".to_string(),
            "https://i.pximg.net/novel-cover-original/img/a.png".to_string(),
        );

        assert_eq!(
            parse("[uploadedimage:21674723]", &images),
            vec![image("https://i.pximg.net/novel-cover-original/img/a.png")]
        );
    }

    #[test]
    fn prose_sharing_a_line_with_an_image_keeps_its_order() {
        let mut images = Images::new();
        images.insert(
            "21674723".to_string(),
            "https://i.pximg.net/i.png".to_string(),
        );

        assert_eq!(
            parse("　前の文[uploadedimage:21674723]後の文", &images),
            vec![
                paragraph("　前の文"),
                image("https://i.pximg.net/i.png"),
                paragraph("後の文"),
            ]
        );
    }

    #[test]
    fn a_reference_that_could_not_be_resolved_is_left_as_written() {
        // The image is gone, or the illustration API would not answer. Either
        // way the reader keeps the chapter, and sees what was there.
        let content = "　絵[uploadedimage:99999]と[pixivimage:88888-2]と[unknown:まま]。";

        assert_eq!(
            parse(content, &Images::new()),
            vec![paragraph(
                "　絵[uploadedimage:99999]と[pixivimage:88888-2]と[unknown:まま]。"
            )]
        );
    }

    #[test]
    fn a_chapter_marker_is_a_heading_and_only_when_it_is_the_whole_line() {
        assert_eq!(
            parse("[chapter:第一章　始まり]", &Images::new()),
            vec![Block::Heading {
                level: 2,
                children: vec![text("第一章　始まり")],
            }]
        );

        assert_eq!(
            parse("　[chapter:記法]について書く。", &Images::new()),
            vec![paragraph("　[chapter:記法]について書く。")],
            "a marker inside a sentence is prose, and may not be reordered"
        );
    }

    #[test]
    fn a_jumpuri_marker_becomes_a_link() {
        assert_eq!(
            parse(
                "[[jumpuri:作者のサイト > https://example.com/novel]]へ。",
                &Images::new()
            ),
            vec![Block::Paragraph {
                children: vec![
                    Inline::Link {
                        href: "https://example.com/novel".to_string(),
                        children: vec![text("作者のサイト")],
                    },
                    text("へ。"),
                ],
            }]
        );
    }

    #[test]
    fn pixiv_images_lists_each_reference_once_in_the_order_it_appears() {
        let content = "　[pixivimage:111]と[pixivimage:222-3]とまた[pixivimage:111]。\
                       [uploadedimage:333]は自分の絵なので要らない。";

        assert_eq!(pixiv_images(content), vec!["111", "222-3"]);
        assert_eq!(
            pixiv_images("　[pixivimage:開いていない"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_whole_captured_novel_becomes_the_blocks_it_describes() {
        // The captured answer, end to end: page joining, ruby, an uploaded
        // image, a scene break, and two markers that stay as written.
        let novel: crate::pixiv::api::Novel =
            crate::pixiv::api::body_of(fixtures::PIXIV_NOVEL_IN_SERIES).expect("the fixture");

        let mut images = Images::new();
        for (reference, embedded) in &novel.text_embedded_images {
            images.insert(
                reference.clone(),
                crate::pixiv::api::best(&embedded.urls)
                    .expect("a size")
                    .to_string(),
            );
        }
        let embedded = images.get("21674723").expect("the uploaded image").clone();

        assert_eq!(
            parse(&novel.content, &images),
            vec![
                paragraph("　最初の段落。"),
                paragraph("　同じ段落の二行目。"),
                paragraph("　次の段落。"),
                paragraph("　ページをまたいだ段落。"),
                Block::Paragraph {
                    children: vec![text("　"), ruby("超能力", "エスパー"), text("を持つ少年。")],
                },
                image(&embedded),
                paragraph("　画像のあとの行。"),
                Block::SceneBreak,
                paragraph("　場面転換のあとの行。"),
                paragraph("　未解決の[uploadedimage:99999]はそのまま残る。"),
                paragraph("　文中の"),
                image(&embedded),
                paragraph("画像。"),
            ]
        );
    }

    #[test]
    fn the_captured_novel_lists_the_illustrations_it_references() {
        let novel: crate::pixiv::api::Novel =
            crate::pixiv::api::body_of(fixtures::PIXIV_NOVEL_IN_SERIES).expect("the fixture");

        assert_eq!(
            pixiv_images(&novel.content),
            Vec::<String>::new(),
            "this novel references no illustration, and an uploaded image is not a reference"
        );
    }

    #[test]
    fn a_captured_standalone_novel_reads_with_headings_and_links() {
        let novel: crate::pixiv::api::Novel =
            crate::pixiv::api::body_of(fixtures::PIXIV_NOVEL_STANDALONE).expect("the fixture");

        let mut images = Images::new();
        images.insert(
            "12345678-2".to_string(),
            "https://i.pximg.net/img-original/img/12345678_p1.png".to_string(),
        );
        images.insert(
            "12345678".to_string(),
            "https://i.pximg.net/img-original/img/12345678_p0.png".to_string(),
        );

        let blocks = parse(&novel.content, &images);

        assert_eq!(
            blocks,
            vec![
                Block::Heading {
                    level: 2,
                    children: vec![text("第一章　始まり")],
                },
                paragraph("　本文。"),
                Block::Paragraph {
                    children: vec![
                        Inline::Link {
                            href: "https://example.com/novel".to_string(),
                            children: vec![text("作者のサイト")],
                        },
                        text("へ。"),
                    ],
                },
                image("https://i.pximg.net/img-original/img/12345678_p1.png"),
                image("https://i.pximg.net/img-original/img/12345678_p0.png"),
                Block::Heading {
                    level: 2,
                    children: vec![text("第二章　終わり")],
                },
                paragraph("　最後の行。"),
            ],
            "the second illustration is a page of the same illust, not the same file"
        );
    }
}
