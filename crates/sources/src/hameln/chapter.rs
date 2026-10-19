//! One chapter page: title and body.
//!
//! The title is the site's big-text span just above the body. The header states
//! the *work's* title in the same shape and comes first, so the first match is
//! not the chapter's (#51).
//!
//! The body lives in `#honbun` as a run of `<p id="N">` lines, one per
//! paragraph; a paragraph whose text is only an ideographic space (or empty) is a
//! blank line, and runs of those are scene breaks via the shared rule.
//!
//! Hameln wraps an illustration in an `<a>` and writes the `alt` on that `<a>`
//! rather than on the `<img>` — which is not where HTML says it goes, and is
//! where the site puts it, so the picture keeps the description the author gave
//! it and the link around it, which points at the same picture, is not kept.

use super::SEL_TITLE_SPAN;
use crate::SourceError;
use crate::blocks;
use crate::dom::element_text;
use crate::inline::{self, Piece, plain_text};
use scraper::{ElementRef, Html, Selector};
use wnd_core::{Block, Chapter, ChapterMeta};

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
