//! One episode page: the body, and how Kakuyomu spells a blank line.
//!
//! The body lives in `div.widget-episodeBody` and is one `<p id="pN">` per line.
//! Scoping to that container is not a nicety: the page around it is full of
//! `<img>`, `<a>` and `<strong>` — the header, the share buttons, the ad slot —
//! and a whole-page reader would pull all of it into the novel.
//!
//! A blank line is a `<p class="blank">` containing **one `<br>` per blank
//! line**:
//!
//! ```html
//! <p id="p3" class="blank"><br /></p>              <!-- one blank line -->
//! <p id="p11" class="blank"><br /><br /></p>       <!-- two: a scene break -->
//! ```
//!
//! So the count is meaning. Narou writes a separate `<p>` for each blank line;
//! Kakuyomu packs a run into one. Reading the class as "one blank line" would
//! throw away every scene break in every chapter on the site, quietly.
//!
//! The body is HTML only: an episode page carries no `__NEXT_DATA__` snapshot,
//! which is why this adapter needs both idioms.
//!
//! Kakuyomu does not allow a picture in an episode at all — its own help centre
//! says images cannot be placed on an episode page, and every `<img>` on one is
//! the header, a promotion or the ad slot. The body container is therefore the
//! boundary, and a picture that ever appears inside it is handled by the shared
//! walk rather than by a rule of this adapter's own.
//!
//! Not handled yet: episodes behind Kakuyomu's paywall, which surface as the
//! missing body below rather than as a partial chapter.

use crate::SourceError;
use crate::blocks;
use crate::dom::{element_text, is_class};
use crate::inline::{self, Piece};
use crate::selectors::selector;
use scraper::{ElementRef, Html, Node, Selector};
use std::sync::LazyLock;
use wnd_core::{Block, Chapter, ChapterMeta};

selector!(SEL_BODY, "div.widget-episodeBody");
selector!(SEL_PARAGRAPH, "p");

/// Parse an episode page into the node model.
pub(super) fn parse_chapter(html: &str, meta: &ChapterMeta) -> Result<Chapter, SourceError> {
    let document = Html::parse_document(html);
    let mut chapter = Chapter::new(meta.index, meta.title.clone());
    let mut containers = 0usize;

    for container in document.select(&SEL_BODY) {
        containers += 1;
        chapter.nodes = blocks::from_line_blocks(body_lines(&container, &meta.url));
    }

    if containers == 0 {
        return Err(SourceError::NotTheSite {
            url: meta.url.clone(),
            expected: "an episode body".to_string(),
        });
    }
    Ok(chapter)
}

/// Each line of the body, as `None` for a blank line.
fn body_lines(container: &ElementRef, page: &str) -> Vec<Option<Vec<Block>>> {
    let mut lines = Vec::new();

    for paragraph in container.select(&SEL_PARAGRAPH) {
        let pieces = inline::pieces_of(&paragraph, inline::plain_text, page);
        // The class is the site's own statement that the paragraph is blank; a
        // paragraph whose text is empty is blank by inspection. Both are needed:
        // a blank line holding nothing but an ideographic space looks like text.
        // A paragraph holding a picture is neither, however little text it has,
        // or the picture would read as a blank and be dropped.
        if is_class(&paragraph, "blank")
            || (element_text(&paragraph).trim_ascii().is_empty()
                && !pieces.iter().any(Piece::is_image))
        {
            lines.extend(std::iter::repeat_n(None, blank_lines(&paragraph)));
        } else {
            lines.push(Some(blocks::from_pieces(pieces)));
        }
    }
    lines
}

/// How many blank lines a blank paragraph stands for: one `<br>` each, and at
/// least one, since an empty `<p>` is still a line.
fn blank_lines(paragraph: &ElementRef) -> usize {
    let breaks = paragraph
        .children()
        .filter(|node| matches!(node.value(), Node::Element(el) if el.name() == "br"))
        .count();
    breaks.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::KAKUYOMU_CHAPTER;
    use wnd_core::Inline;

    fn text(value: &str) -> Inline {
        Inline::Text {
            value: value.to_string(),
        }
    }

    fn meta() -> ChapterMeta {
        ChapterMeta {
            index: 1,
            title: "第1話　見本の一話".to_string(),
            url: "https://kakuyomu.jp/works/10000000000000000001/episodes/10000000000000000003"
                .to_string(),
            arc: None,
            published_at: None,
            revised_at: None,
            hash: None,
            file: "chapters/0001.json".to_string(),
        }
    }

    fn chapter() -> Chapter {
        parse_chapter(KAKUYOMU_CHAPTER, &meta()).expect("the fixture parses")
    }

    #[test]
    fn one_br_is_one_blank_line_and_two_is_a_scene_break() {
        let chapter = chapter();

        // The fixture is ten placeholder paragraphs, one line of dialogue, a
        // blank holding an ideographic space, then the ruby paragraph. One blank
        // between paragraphs disappears; the run of two before the dialogue
        // becomes a scene break; the leading run of three drops.
        assert_eq!(
            chapter.nodes.len(),
            8,
            "seven paragraphs and one scene break: {:#?}",
            chapter.nodes
        );
        assert_eq!(
            chapter.nodes[5],
            Block::SceneBreak,
            "the author pressed enter twice here"
        );
        assert_eq!(
            chapter.nodes[6],
            Block::Paragraph {
                children: vec![text("「本文の見本です。」")]
            }
        );
    }

    #[test]
    fn the_ideographic_space_indent_is_content() {
        let chapter = chapter();
        assert_eq!(
            chapter.nodes[0],
            Block::Paragraph {
                children: vec![text("　本文の見本です。")]
            },
            "the leading U+3000 is the paragraph indent, not padding"
        );
    }

    #[test]
    fn a_blank_line_holding_only_an_ideographic_space_is_still_blank() {
        let chapter = chapter();
        // The fixture has such a paragraph between the dialogue and the ruby
        // line. If the class were ignored it would become a paragraph of "　".
        assert!(
            !chapter.nodes.iter().any(|block| block
                == &Block::Paragraph {
                    children: vec![text("　")]
                }),
            "a whitespace-only paragraph must not reach the reader: {:#?}",
            chapter.nodes
        );
    }

    #[test]
    fn ruby_is_decoded_from_the_real_markup() {
        let chapter = chapter();
        assert_eq!(
            chapter.nodes.last().unwrap(),
            &Block::Paragraph {
                children: vec![
                    text("　本文の見本です。"),
                    Inline::Ruby {
                        base: "漢字".into(),
                        reading: "かんじ".into()
                    },
                    text("の見本です。"),
                ]
            }
        );
    }

    #[test]
    fn page_chrome_is_not_story_text() {
        let chapter = chapter();
        let rendered = format!("{:?}", chapter.nodes);

        for chrome in ["感想を書く", "見本の作品　～サンプル～", "カクヨム"] {
            assert!(
                !rendered.contains(chrome),
                "`{chrome}` is outside the body container, got: {rendered}"
            );
        }
        // The header's logos are `<img alt="カクヨム">`, a tracker is `<img>` in
        // a `<noscript>`, and the site allows no picture in an episode at all
        // (see the module docs) — so nothing on this page is the author's.
        assert!(
            !chapter
                .nodes
                .iter()
                .any(|block| matches!(block, Block::Image { .. })),
            "a logo is not an illustration: {rendered}"
        );
        assert!(
            chapter.pre_note.is_empty() && chapter.post_note.is_empty(),
            "Kakuyomu has no preface or afterword: the author writes those in the body"
        );
    }

    #[test]
    fn blank_counts_are_read_from_the_markup() {
        let mark = |html: &str| {
            let document = Html::parse_document(&format!("<body>{html}</body>"));
            let element = document.select(&SEL_PARAGRAPH).next().expect("p");
            blank_lines(&element)
        };

        assert_eq!(mark(r#"<p class="blank"><br /></p>"#), 1);
        assert_eq!(mark(r#"<p class="blank"><br /><br /></p>"#), 2);
        assert_eq!(mark(r#"<p class="blank"><br /><br /><br /></p>"#), 3);
        assert_eq!(
            mark(r#"<p class="blank"></p>"#),
            1,
            "an empty p is one line"
        );
    }

    #[test]
    fn an_episode_page_without_a_body_is_an_episode_page_that_is_not_there() {
        let error =
            parse_chapter("<html><body><p>nothing here</p></body></html>", &meta()).unwrap_err();

        match error {
            SourceError::NotTheSite { expected, .. } => assert_eq!(expected, "an episode body"),
            other => panic!("expected a page that is not the site, got {other:?}"),
        }
    }

    #[test]
    fn a_body_with_nothing_in_it_is_empty_rather_than_a_failure() {
        // An empty episode is possible (a placeholder the author has not filled
        // in). The container is there, so this is not adapter rot.
        let html =
            r#"<html><body><div class="widget-episodeBody js-episode-body"></div></body></html>"#;
        let chapter = parse_chapter(html, &meta()).expect("an empty body");

        assert!(chapter.nodes.is_empty());
    }

    #[test]
    fn a_picture_inside_the_body_container_is_the_author_s_picture() {
        // Kakuyomu allows no picture in an episode, so no capture holds one and
        // this fragment is written rather than captured (see the module docs).
        // The walk is shared, and the container is the boundary: a picture that
        // ever appears inside it is the author's, must not read as a blank line,
        // and takes its prose apart around it.
        let html = r#"<!DOCTYPE html><html><body>
            <div class="widget-episodeBody">
              <p id="p1">まえ<img src="//host.test/i/1.png" alt="挿絵" />あと</p>
              <p id="p2"><img src="//host.test/i/2.png" alt="" /></p>
              <p id="p3" class="blank"><br /></p>
            </div></body></html>"#;
        let chapter = parse_chapter(html, &meta()).expect("the body parses");

        assert_eq!(
            chapter.nodes,
            vec![
                Block::Paragraph {
                    children: vec![text("まえ")]
                },
                Block::Image {
                    src: "https://host.test/i/1.png".to_string(),
                    alt: Some("挿絵".to_string())
                },
                Block::Paragraph {
                    children: vec![text("あと")]
                },
                Block::Image {
                    src: "https://host.test/i/2.png".to_string(),
                    alt: None
                },
            ],
            "a picture splits the paragraph, and a picture on its own is a line"
        );
    }
}
