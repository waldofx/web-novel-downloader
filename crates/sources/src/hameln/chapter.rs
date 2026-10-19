//! One chapter page: the title, the body, and the author's afterword.
//!
//! The title is the site's big-text span just above the body. The header states
//! the *work's* title in the same shape and comes first, so the first match is
//! not the chapter's (#51).
//!
//! The body lives in `#honbun` as a run of `<p id="N">` lines, one per
//! paragraph; a paragraph whose text is only an ideographic space (or empty) is a
//! blank line, and runs of those are scene breaks via the shared rule.
//!
//! The author's 後書き is `#atogaki` — `<br>`-separated lines rather than `<p>`s,
//! with the site's own toggle link in the `#atogaki_open` element beside it, which
//! is why the container is selected exactly. A Hameln chapter has **no** 前書き:
//! no page here carries a container for one, so `pre_note` stays empty for this
//! site.
//!
//! Hameln wraps an illustration in an `<a>` and writes the `alt` on that `<a>`
//! rather than on the `<img>` — which is not where HTML says it goes, and is
//! where the site puts it, so the picture keeps the description the author gave
//! it and the link around it, which points at the same picture, is not kept.

use super::{SEL_AFTERWORD, SEL_TITLE_SPAN};
use crate::SourceError;
use crate::blocks;
use crate::dom::element_text;
use crate::inline::{self, Piece, plain_text};
use scraper::{ElementRef, Html, Selector};
use wnd_core::{Block, Chapter, ChapterMeta, Inline};

/// Parse a chapter page into the node model.
pub fn parse_chapter(html: &str, meta: &ChapterMeta) -> Result<Chapter, SourceError> {
    let document = Html::parse_document(html);

    let body_selector = Selector::parse("#honbun").expect("selector");
    let container =
        document
            .select(&body_selector)
            .next()
            .ok_or_else(|| SourceError::NotTheSite {
                url: meta.url.clone(),
                expected: "a chapter body".to_string(),
            })?;

    let para_selector = Selector::parse("p").expect("selector");
    if container.select(&para_selector).next().is_none() {
        return Err(SourceError::NotTheSite {
            url: meta.url.clone(),
            expected: "a chapter body".to_string(),
        });
    }

    let mut chapter = Chapter::new(meta.index, meta.title.clone());

    if let Some(title) = title_of(&container) {
        let title = title.trim().to_string();
        if !title.is_empty() {
            chapter.title = title;
        }
    }

    let lines: Vec<Option<Vec<Block>>> = container
        .select(&para_selector)
        .map(|paragraph| {
            let pieces = inline::pieces_of(&paragraph, plain_text, &meta.url);
            // A paragraph holding a picture is a line however little text it
            // has: the `alt` is an attribute, so an illustration on its own
            // would otherwise read as a blank and be dropped.
            if element_text(&paragraph).trim().is_empty() && !pieces.iter().any(Piece::is_image) {
                None
            } else {
                Some(blocks::from_pieces(pieces))
            }
        })
        .collect();

    chapter.nodes = blocks::from_line_blocks(lines);

    if let Some(afterword) = document.select(&SEL_AFTERWORD).next() {
        chapter.post_note = blocks::from_line_blocks(note_lines(&afterword, &meta.url));
    }

    Ok(chapter)
}

/// The chapter's own title: the big-text span just above the body.
///
/// The page states the *work's* title in the same shape, in the header, and the
/// header comes first — so the first match is the work and the chapter's is the
/// one before the body (#51). A span holding `arc title<br>chapter title` keeps
/// the chapter's line. A page with no such span keeps the title the table of
/// contents gave, which names the chapter either way.
fn title_of(body: &ElementRef) -> Option<String> {
    let mut node = body.prev_sibling();
    while let Some(current) = node {
        node = current.prev_sibling();
        if let Some(element) = ElementRef::wrap(current) {
            if SEL_TITLE_SPAN.matches(&element) {
                let text = element_text(&element);
                let title = text.rsplit('\n').next().unwrap_or(&text);
                return Some(title.to_string());
            }
        }
    }
    None
}

/// The afterword's lines.
///
/// The site separates the lines of a note with `<br>`, where a body's lines are
/// `<p>`s, so the runs of inline content are split where those breaks fall and
/// the shared rule decides what a run of empty lines means. The `<br><hr><br>`
/// the site writes before the note is a separator — it drops as leading blanks —
/// and the toggle link that follows the note is a different element this
/// container does not cover.
fn note_lines(container: &ElementRef, page: &str) -> Vec<Option<Vec<Block>>> {
    let mut lines = vec![Line::default()];

    for piece in inline::pieces_of(container, plain_text, page) {
        match piece {
            Piece::Image { src, alt } => {
                let line = lines.last_mut().expect("a line is always open");
                line.content = true;
                line.blocks.push(Block::Image { src, alt });
            }
            Piece::Inline(nodes) => {
                for (position, part) in split_lines(nodes).into_iter().enumerate() {
                    if position > 0 {
                        lines.push(Line::default());
                    }
                    let line = lines.last_mut().expect("a line is always open");
                    line.content |= has_content(&part);
                    if !part.is_empty() {
                        line.blocks.push(Block::Paragraph { children: part });
                    }
                }
            }
        }
    }

    lines
        .into_iter()
        .map(|line| line.content.then_some(line.blocks))
        .collect()
}

/// One line being built: its blocks, and whether anything but whitespace is in
/// it. A line of just the paragraph indent is blank, as it is in a body.
#[derive(Default)]
struct Line {
    content: bool,
    blocks: Vec<Block>,
}

/// A run of inline nodes, split where the text says a `<br>` was.
fn split_lines(nodes: Vec<Inline>) -> Vec<Vec<Inline>> {
    let mut lines = vec![Vec::new()];

    for node in nodes {
        match node {
            Inline::Text { value } => {
                let mut parts = value.split('\n');
                push_text(
                    lines.last_mut().expect("a line is always open"),
                    parts.next().unwrap_or_default(),
                );
                for part in parts {
                    lines.push(Vec::new());
                    push_text(lines.last_mut().expect("a line is always open"), part);
                }
            }
            other => lines.last_mut().expect("a line is always open").push(other),
        }
    }

    lines
}

/// A non-empty run of text, as a node.
fn push_text(line: &mut Vec<Inline>, value: &str) {
    if !value.is_empty() {
        line.push(Inline::Text {
            value: value.to_string(),
        });
    }
}

/// Whether a run holds anything a reader would see.
fn has_content(nodes: &[Inline]) -> bool {
    nodes.iter().any(|node| match node {
        Inline::Text { value } => !value.trim().is_empty(),
        _ => true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{HAMELN_CHAPTER, HAMELN_CHAPTER_ILLUSTRATED};
    use wnd_core::{Block, Inline};

    fn text(value: &str) -> Inline {
        Inline::Text {
            value: value.to_string(),
        }
    }

    fn chapter_meta(index: u32, title: &str) -> ChapterMeta {
        ChapterMeta {
            index,
            title: title.to_string(),
            url: format!("https://syosetu.org/novel/000000/{index}.html"),
            arc: None,
            published_at: None,
            revised_at: None,
            hash: None,
            file: format!("chapters/{:04}.json", index),
        }
    }

    #[test]
    fn a_chapter_takes_its_own_title_and_not_the_work_s() {
        // The header states the work's title in the same `font-size:120%` span a
        // chapter's title uses, and the header comes first (#51). `meta.title`
        // is the work's title here on purpose: a parser that read the header's
        // span would match it and the test would catch that.
        let meta = chapter_meta(1, "見本の作品　～サンプル～");
        let chapter = parse_chapter(HAMELN_CHAPTER, &meta).unwrap();

        assert_eq!(chapter.index, 1);
        assert_eq!(
            chapter.title, "第一話　見本の一話",
            "the span above the body holds `arc<br>chapter title`; the title is the chapter's"
        );
        assert!(!chapter.nodes.is_empty(), "body must not be empty");
    }

    #[test]
    fn a_chapter_title_with_no_arc_is_the_whole_span() {
        // Three of the five captured chapter pages have no arc: their span holds
        // the chapter's title and nothing else, and the `<br>` split leaves it
        // be.
        let meta = chapter_meta(1, "見本の作品");
        let html = r#"<html><body>
            <p><span style="font-size:120%"><a href="https://syosetu.org/novel/000000/">見本の作品</a></span> 　 作：<a href="https://syosetu.org/user/000000/">見本作者</a><br></p>
            <span style="font-size:120%"> 合鍵と昼食の見本</span>
            <div id="honbun"><p id="1"> 本文の見本です。</p></div>
        </body></html>"#;

        let chapter = parse_chapter(html, &meta).unwrap();

        assert_eq!(chapter.title, "合鍵と昼食の見本");
    }

    #[test]
    fn a_page_with_no_title_of_its_own_keeps_the_toc_title() {
        // The header's span is all this page offers, and it is the work's title:
        // the table of contents already said what the chapter is called.
        let meta = chapter_meta(1, "目次の見本の一話");
        let html = r#"<html><body>
            <p><span style="font-size:120%"><a href="https://syosetu.org/novel/000000/">見本の作品</a></span> 　 作：<a href="https://syosetu.org/user/000000/">見本作者</a><br></p>
            <div id="honbun"><p id="1"> 本文の見本です。</p></div>
        </body></html>"#;

        let chapter = parse_chapter(html, &meta).unwrap();

        assert_eq!(chapter.title, "目次の見本の一話");
    }

    #[test]
    fn the_afterword_becomes_the_post_note() {
        let meta = chapter_meta(1, "見本の作品　～サンプル～");
        let chapter = parse_chapter(HAMELN_CHAPTER, &meta).unwrap();

        assert_eq!(
            chapter.post_note,
            vec![
                Block::Paragraph {
                    children: vec![text("後書きの一行目の見本です。")]
                },
                Block::Paragraph {
                    children: vec![text("後書きの二行目の見本です。")]
                },
                Block::SceneBreak,
                Block::Paragraph {
                    children: vec![text("後書きの三行目の見本です。")]
                },
            ],
            "the site's `<br><hr><br>` before the note is a separator and drops, one blank \
             line separates paragraphs, two are a scene break"
        );
    }

    #[test]
    fn the_toggle_link_beside_the_afterword_is_the_site_s_and_not_the_author_s() {
        let meta = chapter_meta(1, "見本の作品　～サンプル～");
        let chapter = parse_chapter(HAMELN_CHAPTER, &meta).unwrap();

        assert!(
            !chapter
                .translatable_texts()
                .any(|text| text.contains("表示する")),
            "`#atogaki_open` is the site's toggle link, and it sits right after the note: {:?}",
            chapter.post_note
        );
    }

    #[test]
    fn an_illustration_in_a_body_becomes_a_picture_with_the_alt_the_link_states() {
        let meta = chapter_meta(1, "placeholder");
        let chapter = parse_chapter(HAMELN_CHAPTER_ILLUSTRATED, &meta).unwrap();

        assert_eq!(
            chapter.nodes,
            vec![
                Block::Paragraph {
                    children: vec![text("　本文の一行目のプレースホルダー。")]
                },
                Block::Paragraph {
                    children: vec![text("　本文の二行目のプレースホルダー。")]
                },
                // `<p id="4"></p><hr><p></p>` and then a blank `<p>`: three blank
                // lines to the reader, which is one scene break and not three.
                Block::SceneBreak,
                Block::Paragraph {
                    children: vec![text("挿絵の説明のような一行")]
                },
                Block::SceneBreak,
                Block::Image {
                    src: "http://img.example.test/user/0000/000/000001.jpg".to_string(),
                    // Hameln writes the `alt` on the `<a>` around the picture,
                    // which is not where HTML says it goes — and is where the
                    // site puts it.
                    alt: Some("挿絵".to_string()),
                },
                Block::SceneBreak,
                Block::Paragraph {
                    children: vec![text("　挿絵のあとの本文のプレースホルダー。")]
                },
                Block::SceneBreak,
                Block::Image {
                    src: "http://img.example.test/user/0000/000/000002.png".to_string(),
                    alt: Some("挿絵".to_string()),
                },
                Block::Paragraph {
                    children: vec![text("　本文の最後の行のプレースホルダー。")]
                },
            ],
            "a picture is a block and the link around it is not; a line holding one is \
             not blank; the blank runs around it still separate scenes"
        );
    }

    #[test]
    fn ruby_becomes_an_inline_ruby_node() {
        let meta = chapter_meta(1, "t");
        let chapter = parse_chapter(HAMELN_CHAPTER, &meta).unwrap();
        let has_ruby = chapter.nodes.iter().any(|b| match b {
            Block::Paragraph { children } => {
                children.iter().any(|i| matches!(i, Inline::Ruby { .. }))
            }
            _ => false,
        });
        assert!(has_ruby, "the captured chapter contains ruby");
    }

    #[test]
    fn a_page_with_no_body_is_not_a_hameln_chapter() {
        let meta = chapter_meta(1, "t");
        let error = parse_chapter("<html><body><p>nothing</p></body></html>", &meta).unwrap_err();
        match error {
            SourceError::NotTheSite { expected, .. } => {
                assert_eq!(expected, "a chapter body");
            }
            other => panic!("expected NotTheSite, got {other:?}"),
        }
    }
}
