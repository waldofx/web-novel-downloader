//! One chapter page: the preface, the body, and the afterword.
//!
//! Narou emits one `<p>` per line. A blank line is a `<p>` whose only content is
//! a `<br>`, and a run of blank lines is how authors mark a scene change. The
//! rule for what a run means is shared with the other adapters — it is the same
//! rule everywhere — and lives in [`crate::blocks`].
//!
//! An author's illustration gets a `<p>` of its own, holding an `<a>` around an
//! `<img>` whose address the site writes protocol-relative
//! (`//img.example.test/…`). The walk resolves it against the chapter, so the
//! picture reaches `images/` and an export rather than vanishing.

use super::inline::pieces_of;
use super::{SEL_PARAGRAPH, SEL_TEXT_BLOCK};
use crate::SourceError;
use crate::blocks;
use crate::dom::{element_text, is_class};
use crate::inline::Piece;
use scraper::{ElementRef, Html};
use wnd_core::{Block, Chapter, ChapterMeta};

/// Parse a chapter page into the node model.
pub(super) fn parse_chapter(html: &str, meta: &ChapterMeta) -> Result<Chapter, SourceError> {
    let document = Html::parse_document(html);
    let mut chapter = Chapter::new(meta.index, meta.title.clone());
    let mut containers = 0usize;

    for container in document.select(&SEL_TEXT_BLOCK) {
        containers += 1;
        let blocks = blocks::from_line_blocks(container_lines(&container, &meta.url));
        if is_class(&container, "p-novel__text--preface") {
            chapter.pre_note = blocks;
        } else if is_class(&container, "p-novel__text--afterword") {
            chapter.post_note = blocks;
        } else {
            chapter.nodes = blocks;
        }
    }

    if containers == 0 {
        return Err(SourceError::NotTheSite {
            url: meta.url.clone(),
            expected: "a chapter body".to_string(),
        });
    }
    Ok(chapter)
}

/// Each line of a text container, as `None` for a blank line.
///
/// Narou emits one `<p>` per line; a blank line is a `<p>` whose only content
/// is a `<br>`. A line holding a picture is a line however little text it has —
/// an `alt` is an attribute, not text — or the picture would read as a blank and
/// be dropped.
fn container_lines(container: &ElementRef, page: &str) -> Vec<Option<Vec<Block>>> {
    container
        .select(&SEL_PARAGRAPH)
        .map(|paragraph| {
            let pieces = pieces_of(&paragraph, page);
            if element_text(&paragraph).trim_ascii().is_empty()
                && !pieces.iter().any(Piece::is_image)
            {
                None
            } else {
                Some(blocks::from_pieces(pieces))
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{NAROU_CHAPTER, NAROU_CHAPTER_ILLUSTRATED, NAROU_SHORT_STORY};
    use wnd_core::Inline;

    fn text(value: &str) -> Inline {
        Inline::Text {
            value: value.to_string(),
        }
    }

    fn chapter_meta(index: u32) -> ChapterMeta {
        ChapterMeta {
            index,
            title: format!("第{index}話"),
            url: format!("https://ncode.syosetu.com/n0000aa/{index}/"),
            arc: None,
            published_at: None,
            revised_at: None,
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

    /// The chapter the illustrated capture came from, so a resolved
    /// protocol-relative `src` is resolved against the page it really sat on.
    fn illustrated_meta() -> ChapterMeta {
        ChapterMeta {
            index: 288,
            title: "第288話".to_string(),
            url: "https://ncode.syosetu.com/n0000bb/288/".to_string(),
            arc: None,
            published_at: None,
            revised_at: None,
            hash: None,
            file: crate::narou::chapter_file(288),
        }
    }

    #[test]
    fn an_illustration_in_a_body_becomes_a_picture_the_fetcher_can_find() {
        let chapter = parse_chapter(NAROU_CHAPTER_ILLUSTRATED, &illustrated_meta()).unwrap();

        assert_eq!(
            chapter.nodes,
            vec![
                Block::Paragraph {
                    children: vec![text("　挿絵の前の本文の見本です。")]
                },
                Block::Paragraph {
                    children: vec![text("　挿絵の直前の行の見本です。")]
                },
                Block::Image {
                    // Protocol-relative on the page, absolute once it is a block:
                    // the fetcher has no page to complete it against.
                    src: "https://img.example.test/view/00000/".to_string(),
                    alt: Some("挿絵(By サンプル)".to_string()),
                },
                Block::SceneBreak,
                Block::Paragraph {
                    children: vec![text("　挿絵のあとの本文の見本です。")]
                },
            ],
            "the picture is a block beside the prose, not a line of characters in it"
        );

        assert!(
            !chapter.pre_note.is_empty(),
            "the preface of the same capture still parses: {:?}",
            chapter.pre_note
        );
    }

    #[test]
    fn a_short_story_page_is_its_own_chapter() {
        let meta = ChapterMeta {
            index: 1,
            title: "短編のタイトル".to_string(),
            url: "https://ncode.syosetu.com/n0000cc/".to_string(),
            arc: None,
            published_at: None,
            revised_at: None,
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
    fn a_page_with_no_chapter_text_is_not_a_narou_chapter_page() {
        // A chapter is a text container full of paragraphs. No container at all
        // is not ordinary adapter rot: it is a page that is not this site's
        // chapter page, and the caller reports it with the evidence.
        let error = parse_chapter(
            "<html><body><p>nothing here</p></body></html>",
            &chapter_meta(1),
        )
        .unwrap_err();

        match error {
            SourceError::NotTheSite { url, expected } => {
                assert_eq!(url, "https://ncode.syosetu.com/n0000aa/1/");
                assert_eq!(expected, "a chapter body");
            }
            other => panic!("expected a page that is not the site, got {other:?}"),
        }
    }
}
