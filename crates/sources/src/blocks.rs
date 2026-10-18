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
//! the same rule, and every adapter arrives at it the same way
//! ([`from_line_blocks`]).
//!
//! A line is not always one paragraph. An author's picture shares its line with
//! the prose beside it, and pixiv's `[uploadedimage:…]` is the same thing typed
//! rather than marked up, so [`from_pieces`] is where a line's content becomes
//! blocks: prose, picture, prose, in the order they came.

use crate::inline::Piece;
use wnd_core::Block;

/// A line's content — runs of inline nodes, and the pictures between them — as
/// the blocks it makes.
///
/// Prose beside a picture is split around it, because a block cannot live inside
/// a paragraph and the picture goes where the author put it. This is the rule
/// pixiv's `[uploadedimage:…]` and three sites' `<img>` share, so it is written
/// once.
pub(crate) fn from_pieces(pieces: Vec<Piece>) -> Vec<Block> {
    let mut blocks = Vec::new();
    for piece in pieces {
        match piece {
            Piece::Inline(nodes) if !nodes.is_empty() => {
                blocks.push(Block::Paragraph { children: nodes });
            }
            Piece::Inline(_) => {}
            Piece::Image { src, alt } => blocks.push(Block::Image { src, alt }),
        }
    }
    blocks
}

/// The blank-line rule, for an adapter whose line already knows what blocks it
/// is.
///
/// A line that holds a picture is more than one block, and pixiv's
/// `[uploadedimage:…]` marker is the same from the other direction: whether a
/// line becomes one paragraph or several is the adapter's business. What a blank
/// line means is this module's, and it is the same rule either way.
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
    use wnd_core::Inline;

    fn text(value: &str) -> Inline {
        Inline::Text {
            value: value.to_string(),
        }
    }

    /// One line of prose.
    fn line(value: &str) -> Option<Vec<Block>> {
        Some(vec![Block::Paragraph {
            children: vec![text(value)],
        }])
    }

    fn image(src: &str) -> Block {
        Block::Image {
            src: src.to_string(),
            alt: None,
        }
    }

    /// One run of prose, as a line hands it over.
    fn run(value: &str) -> Piece {
        Piece::Inline(vec![text(value)])
    }

    fn picture(src: &str, alt: Option<&str>) -> Piece {
        Piece::Image {
            src: src.to_string(),
            alt: alt.map(str::to_string),
        }
    }

    #[test]
    fn blank_line_rules_are_applied_directly() {
        let blocks = from_line_blocks(vec![
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
        let blocks = from_line_blocks(vec![line("a"), None, None, None, line("b")]);
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
        let blocks = from_line_blocks(vec![line("a"), Some(Vec::new()), line("b")]);
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
        assert_eq!(from_line_blocks(vec![None, None, None]), Vec::new());
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
    fn prose_beside_a_picture_is_split_around_it() {
        assert_eq!(
            from_pieces(vec![
                run("前の文"),
                picture("images/1.png", Some("挿絵")),
                run("後の文"),
            ]),
            vec![
                Block::Paragraph {
                    children: vec![text("前の文")]
                },
                Block::Image {
                    src: "images/1.png".to_string(),
                    alt: Some("挿絵".to_string())
                },
                Block::Paragraph {
                    children: vec![text("後の文")]
                },
            ]
        );
    }

    #[test]
    fn a_line_that_is_only_a_picture_is_only_a_picture() {
        assert_eq!(
            from_pieces(vec![picture("images/1.png", None)]),
            vec![image("images/1.png")]
        );
    }

    #[test]
    fn a_run_with_nothing_in_it_is_not_a_paragraph() {
        assert_eq!(
            from_pieces(vec![
                Piece::Inline(Vec::new()),
                picture("images/1.png", None),
                Piece::Inline(Vec::new()),
            ]),
            vec![image("images/1.png")]
        );
        assert_eq!(from_pieces(Vec::new()), Vec::new());
    }
}
