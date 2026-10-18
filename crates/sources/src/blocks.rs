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
//! with a `<p>` whose sole content is a `<br>`, Kakuyomu with `class="blank"`,
//! Pixiv with a genuinely empty line. The rule itself lives here because it is
//! the same rule, and it is applied to lines that are already blocks as well as
//! to lines that are a paragraph each ([`from_line_blocks`]).

use wnd_core::{Block, Inline};

/// Turn lines into blocks. `None` is a blank line; `Some` is a line's content,
/// which may itself be empty and is then dropped without counting as a blank.
pub(crate) fn from_lines(lines: Vec<Option<Vec<Inline>>>) -> Vec<Block> {
    from_line_blocks(
        lines
            .into_iter()
            .map(|line| {
                line.map(|children| {
                    if children.is_empty() {
                        Vec::new()
                    } else {
                        vec![Block::Paragraph { children }]
                    }
                })
            })
            .collect(),
    )
}

/// The same rule, for an adapter whose line already knows what blocks it is.
///
/// Pixiv is why this exists: its `[uploadedimage:…]` marker is a block of its
/// own sharing a line with prose, where every other site's line is exactly one
/// paragraph. Whether a line becomes one paragraph or several is the adapter's
/// business; what a blank line means is this module's, and it is the same rule
/// either way.
pub(crate) fn from_line_blocks(lines: Vec<Option<Vec<Block>>>) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut blanks = 0usize;

    for line in lines {
        match line {
            None => blanks += 1,
            Some(built) => {
                // `!blocks.is_empty()` drops a leading run of blanks.
                if blanks >= 2 && !blocks.is_empty() {
                    blocks.push(Block::SceneBreak);
                }
                blanks = 0;
                blocks.extend(built);
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

    #[test]
    fn a_line_that_is_several_blocks_does_not_change_what_a_blank_line_means() {
        // An image is a block of its own and shares a line with a paragraph;
        // pixiv is the site that does this. A blank line still separates, and
        // two still make a scene break, whether a line holds one block or two.
        let prose = |value: &str| {
            vec![Block::Paragraph {
                children: vec![text(value)],
            }]
        };
        let image = || {
            vec![Block::Image {
                src: "images/1.png".to_string(),
                alt: None,
            }]
        };

        let blocks = from_line_blocks(vec![
            None,
            Some(prose("a")),
            Some(image()),
            None,
            Some(prose("b")),
            None,
            None,
            Some(image()),
        ]);

        assert_eq!(
            blocks,
            vec![
                Block::Paragraph {
                    children: vec![text("a")]
                },
                Block::Image {
                    src: "images/1.png".to_string(),
                    alt: None
                },
                Block::Paragraph {
                    children: vec![text("b")]
                },
                Block::SceneBreak,
                Block::Image {
                    src: "images/1.png".to_string(),
                    alt: None
                },
            ]
        );
    }

    #[test]
    fn from_lines_and_from_line_blocks_agree_on_the_simple_case() {
        // `from_lines` is the one-line-one-paragraph shorthand for the same
        // rule; if the two ever disagree, every site gets a different rule.
        let lines = vec![
            None,
            line("a"),
            None,
            line("b"),
            None,
            None,
            line("c"),
            None,
        ];
        let shorthand = from_lines(lines);
        let explicit = from_line_blocks(vec![
            None,
            Some(vec![Block::Paragraph {
                children: vec![text("a")],
            }]),
            None,
            Some(vec![Block::Paragraph {
                children: vec![text("b")],
            }]),
            None,
            None,
            Some(vec![Block::Paragraph {
                children: vec![text("c")],
            }]),
            None,
        ]);

        assert_eq!(shorthand, explicit);
    }
}
