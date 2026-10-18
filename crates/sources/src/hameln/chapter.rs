//! One chapter page: title and body.
//!
//! The body lives in `#honbun` as a run of `<p id="N">` lines, one per
//! paragraph; a paragraph whose text is only an ideographic space (or empty) is a
//! blank line, and runs of those are scene breaks via the shared rule.
//!
//! Hameln wraps an illustration in an `<a>` and writes the `alt` on that `<a>`
//! rather than on the `<img>` — which is not where HTML says it goes, and is
//! where the site puts it, so the picture keeps the description the author gave
//! it and the link around it, which points at the same picture, is not kept.

use crate::SourceError;
use crate::blocks;
use crate::dom::{element_text, first_text};
use crate::inline::{self, Piece, plain_text};
use scraper::{Html, Selector};
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

    if let Ok(title_selector) = Selector::parse(r#"span[style*="font-size:120%"]"#) {
        if let Some(title) = first_text(&document, &title_selector) {
            let title = title.trim().to_string();
            if !title.is_empty() {
                // The span holds "arc title<br>chapter title"; keep the chapter's.
                let title = title
                    .rsplit('\n')
                    .next()
                    .unwrap_or(&title)
                    .trim()
                    .to_string();
                if !title.is_empty() {
                    chapter.title = title;
                }
            }
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
            url: format!("https://syosetu.org/novel/316401/{index}.html"),
            arc: None,
            published_at: None,
            revised_at: None,
            hash: None,
            file: format!("chapters/{:04}.json", index),
        }
    }

    #[test]
    fn a_chapter_parses_a_title_and_a_body() {
        let meta = chapter_meta(1, "placeholder");
        let chapter = parse_chapter(HAMELN_CHAPTER, &meta).unwrap();

        assert_eq!(chapter.index, 1);
        assert!(!chapter.title.is_empty(), "title: {:?}", chapter.title);
        assert!(!chapter.nodes.is_empty(), "body must not be empty");
    }

    #[test]
    fn a_picture_in_a_body_keeps_the_alt_the_page_states_and_takes_its_link_with_it() {
        let meta = chapter_meta(1, "placeholder");
        let chapter = parse_chapter(HAMELN_CHAPTER_ILLUSTRATED, &meta).unwrap();

        assert_eq!(
            chapter.nodes,
            vec![
                Block::Paragraph {
                    children: vec![text(
                        " 学園都市キヴォトス、それは無数の学園と生徒により構成される巨大な都市。"
                    )]
                },
                Block::Paragraph {
                    children: vec![text(
                        " 銃声と怒号が日常となったこの街に今、一人の男子生徒が立ち上がる……!"
                    )]
                },
                Block::Image {
                    src: "http://syosetu.org/img/user/196658/125290.png".to_string(),
                    // Hameln writes the `alt` on the `<a>` around the picture,
                    // which is not where HTML says it goes — and still where the
                    // site puts it.
                    alt: Some("挿絵".to_string()),
                },
                Block::SceneBreak,
                Block::Paragraph {
                    children: vec![
                        text(
                            " 思春期男子の妄想を容赦なくぶった切った目の前の乳出し女に書類を押し付けられながら、書類仕事をこなすこの俺、"
                        ),
                        Inline::Ruby {
                            base: "折川酒泉".to_string(),
                            reading: "おりかわしゅせん".to_string(),
                        },
                        text("は今日も今日とて元気です"),
                    ]
                },
            ],
            "the link the picture was wrapped in goes with it, and the prose survives"
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
