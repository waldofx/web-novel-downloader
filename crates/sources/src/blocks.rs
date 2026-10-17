//! Turning a page's lines into blocks.
//!
//! Every Japanese web-novel site uses the same convention, and it is not
//! obvious: a blank line is not whitespace to be collapsed, it is meaning.
//!
//! - a single blank line is a paragraph separator, and disappears;
//! - two or more become one [`Block::SceneBreak`];
//! - leading and trailing blanks are dropped.
//!
//! The adapter's job is only to say which lines are blank — Narou marks them
//! with a `<p>` whose sole content is a `<br>`, Kakuyomu with `class="blank"`.
//! The rule itself lives here because it is the same rule.

use wnd_core::{Block, Inline};

/// Turn lines into blocks. `None` is a blank line; `Some` is a line's content,
/// which may itself be empty and is then dropped without counting as a blank.
pub(crate) fn from_lines(lines: Vec<Option<Vec<Inline>>>) -> Vec<Block> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn text(value: &str) -> Inline {
        Inline::Text {
            value: value.to_string(),
        }
    }

    fn line(value: &str) -> Option<Vec<Inline>> {
        Some(vec![text(value)])
    }

    #[test]
    fn blank_line_rules_are_applied_directly() {
        let blocks = from_lines(vec![
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
    fn three_blank_lines_are_still_one_scene_break() {
        let blocks = from_lines(vec![line("a"), None, None, None, line("b")]);
        assert_eq!(
            blocks,
            vec![
                Block::Paragraph {
                    children: vec![text("a")]
                },
                Block::SceneBreak,
                Block::Paragraph {
                    children: vec![text("b")]
                },
            ],
            "the length of the run is not the number of breaks"
        );
    }

    #[test]
    fn a_line_whose_content_is_empty_is_not_a_blank_line() {
        // An empty paragraph is markup noise; only an explicit blank counts, or
        // a stray `<p></p>` would silently become a scene break.
        let blocks = from_lines(vec![line("a"), Some(Vec::new()), line("b")]);
        assert_eq!(
            blocks,
            vec![
                Block::Paragraph {
                    children: vec![text("a")]
                },
                Block::Paragraph {
                    children: vec![text("b")]
                },
            ]
        );
    }

    #[test]
    fn a_page_of_nothing_but_blanks_is_empty() {
        assert_eq!(from_lines(vec![None, None, None]), Vec::new());
    }
}
