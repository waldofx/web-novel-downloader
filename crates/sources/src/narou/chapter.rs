//! One chapter page: the preface, the body, and the afterword.
//!
//! Narou emits one `<p>` per line. A blank line is a `<p>` whose only content is
//! a `<br>`, and a run of blank lines is how authors mark a scene change. The
//! rule for what a run means is shared with the other adapters — it is the same
//! rule everywhere — and lives in [`crate::blocks`].

use super::inline::inlines_of;
use super::{SEL_PARAGRAPH, SEL_TEXT_BLOCK};
use crate::SourceError;
use crate::blocks;
use crate::dom::{element_text, is_class};
use scraper::{ElementRef, Html};
use wnd_core::{Chapter, ChapterMeta, Inline};

/// Parse a chapter page into the node model.
pub(super) fn parse_chapter(html: &str, meta: &ChapterMeta) -> Result<Chapter, SourceError> {
    let document = Html::parse_document(html);
    let mut chapter = Chapter::new(meta.index, meta.title.clone());
    let mut containers = 0usize;

    for container in document.select(&SEL_TEXT_BLOCK) {
        containers += 1;
        let blocks = blocks::from_lines(container_lines(&container));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{NAROU_CHAPTER, NAROU_SHORT_STORY};
    use wnd_core::Block;

    fn text(value: &str) -> Inline {
        Inline::Text {
            value: value.to_string(),
        }
    }

    fn chapter_meta(index: u32) -> ChapterMeta {
        ChapterMeta {
            index,
            title: format!("第{index}話"),
            url: format!("https://ncode.syosetu.com/n0608gd/{index}/"),
            arc: None,
            published_at: None,
            hash: None,
            file: crate::narou::chapter_file(index),
        }
    }

    #[test]
    fn chapter_notes_and_body_are_split() {
        let chapter = parse_chapter(NAROU_CHAPTER, &chapter_meta(1)).unwrap();

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
        let chapter = parse_chapter(NAROU_CHAPTER, &chapter_meta(1)).unwrap();

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
        let chapter = parse_chapter(NAROU_CHAPTER, &chapter_meta(1)).unwrap();
        match &chapter.nodes[0] {
            Block::Paragraph { children } => assert_eq!(children, &vec![text("　最初の行。")]),
            other => panic!("expected a paragraph, got {other:?}"),
        }
    }

    #[test]
    fn html_ruby_becomes_an_inline_ruby_node() {
        let chapter = parse_chapter(NAROU_CHAPTER, &chapter_meta(1)).unwrap();
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
        let chapter = parse_chapter(NAROU_CHAPTER, &chapter_meta(1)).unwrap();
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
    fn a_short_story_page_is_its_own_chapter() {
        let meta = ChapterMeta {
            index: 1,
            title: "短編のタイトル".to_string(),
            url: "https://ncode.syosetu.com/n5589cg/".to_string(),
            arc: None,
            published_at: None,
            hash: None,
            file: crate::narou::chapter_file(1),
        };
        let chapter = parse_chapter(NAROU_SHORT_STORY, &meta).unwrap();

        assert!(chapter.pre_note.is_empty(), "a short story has no preface");
        assert_eq!(
            chapter.nodes.len(),
            3,
            "one blank drops, two become a scene break: {:?}",
            chapter.nodes
        );
        assert_eq!(chapter.nodes[1], Block::SceneBreak);
        assert_eq!(
            chapter.post_note.len(),
            1,
            "the afterword is still kept separate"
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
}
