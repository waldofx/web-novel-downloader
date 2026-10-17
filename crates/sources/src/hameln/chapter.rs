//! One chapter page: title and body.
//!
//! The body lives in `#honbun` as a run of `<p id="N">` lines, one per
//! paragraph; a paragraph whose text is only an ideographic space (or empty) is a
//! blank line, and runs of those are scene breaks via the shared rule.

use crate::SourceError;
use crate::blocks;
use crate::dom::{element_text, first_text};
use crate::inline::{self, plain_text};
use scraper::{Html, Selector};
use wnd_core::{Chapter, ChapterMeta, Inline};

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

    let lines: Vec<Option<Vec<Inline>>> = container
        .select(&para_selector)
        .map(|paragraph| {
            if element_text(&paragraph).trim().is_empty() {
                None
            } else {
                Some(inline::of(&paragraph, plain_text))
            }
        })
        .collect();

    chapter.nodes = blocks::from_lines(lines);
    Ok(chapter)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::HAMELN_CHAPTER;
    use wnd_core::Block;

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
