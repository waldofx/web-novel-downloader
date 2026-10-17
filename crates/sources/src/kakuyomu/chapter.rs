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
//! Not handled yet: illustrations inside a body, and episodes behind Kakuyomu's
//! paywall — the first is an image pipeline (see #18), the second would surface
//! as the parse error below rather than as a partial chapter.

use crate::SourceError;
use crate::blocks;
use crate::dom::{element_text, is_class};
use crate::inline;
use crate::selectors::selector;
use scraper::{ElementRef, Html, Node, Selector};
use std::sync::LazyLock;
use wnd_core::{Chapter, ChapterMeta, Inline};

selector!(SEL_BODY, "div.widget-episodeBody");
selector!(SEL_PARAGRAPH, "p");

/// Parse an episode page into the node model.
pub(super) fn parse_chapter(html: &str, meta: &ChapterMeta) -> Result<Chapter, SourceError> {
    let document = Html::parse_document(html);
    let mut chapter = Chapter::new(meta.index, meta.title.clone());
    let mut containers = 0usize;

    for container in document.select(&SEL_BODY) {
        containers += 1;
        chapter.nodes = blocks::from_lines(body_lines(&container));
    }

    if containers == 0 {
        return Err(SourceError::Parse {
            url: meta.url.clone(),
            message: "no episode body found; the site markup may have changed".to_string(),
        });
    }
    Ok(chapter)
}

/// Each line of the body, as `None` for a blank line.
fn body_lines(container: &ElementRef) -> Vec<Option<Vec<Inline>>> {
    let mut lines = Vec::new();

    for paragraph in container.select(&SEL_PARAGRAPH) {
        // The class is the site's own statement that the paragraph is blank; a
        // paragraph whose text is empty is blank by inspection. Both are needed:
        // a blank line holding nothing but an ideographic space looks like text.
        if is_class(&paragraph, "blank") || element_text(&paragraph).trim_ascii().is_empty() {
            lines.extend(std::iter::repeat_n(None, blank_lines(&paragraph)));
        } else {
            lines.push(Some(inline::of(&paragraph, inline::plain_text)));
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
    use wnd_core::Block;

    fn text(value: &str) -> Inline {
        Inline::Text {
            value: value.to_string(),
        }
    }

    fn meta() -> ChapterMeta {
        ChapterMeta {
            index: 1,
            title: "第1話　勇者、魔王の軍門に下る".to_string(),
            url: "https://kakuyomu.jp/works/11054822663954717597/episodes/11054822663955326556"
                .to_string(),
            arc: None,
            published_at: None,
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

        // The fixture is twelve real paragraphs then a real ruby paragraph. One
        // blank between paragraphs disappears; the run of two before the
        // dialogue becomes a scene break; the leading run of three drops.
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
                children: vec![text(
                    "「今回は発生して三日のダンジョンだから攻略も難しくないと聞いていたのですが……まさか、全滅するとは……」"
                )]
            }
        );
    }

    #[test]
    fn the_ideographic_space_indent_is_content() {
        let chapter = chapter();
        assert_eq!(
            chapter.nodes[0],
            Block::Paragraph {
                children: vec![text("　勇者とは。")]
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
                    text("　このスマホ一台で"),
                    Inline::Ruby {
                        base: "魔王".into(),
                        reading: "俺".into()
                    },
                    text("の強化やダンジョンの拡張、魔物の召喚や罠の設置ができる。"),
                ]
            }
        );
    }

    #[test]
    fn page_chrome_is_not_story_text() {
        let chapter = chapter();
        let rendered = format!("{:?}", chapter.nodes);

        for chrome in ["感想を書く", "ブラック待遇の勇者一行", "カクヨム"] {
            assert!(
                !rendered.contains(chrome),
                "`{chrome}` is outside the body container, got: {rendered}"
            );
        }
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
    fn an_episode_page_without_a_body_is_reported() {
        let error =
            parse_chapter("<html><body><p>nothing here</p></body></html>", &meta()).unwrap_err();

        assert!(matches!(error, SourceError::Parse { .. }), "got {error:?}");
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
}
