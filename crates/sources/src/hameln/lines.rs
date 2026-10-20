//! A `<br>`-separated run of Hameln content, as blocks.
//!
//! Two containers on this site use this shape and neither is a body: the
//! author's 後書き (`#atogaki`) and the synopsis (the second `div.ss`). A body is
//! one `<p>` per line (`chapter`), so it is read paragraph by paragraph; these
//! are one *line* per `<br>`, so the lines are cut out of the running text first
//! and the shared blank-line rule applied afterwards
//! ([`crate::blocks::from_line_blocks`]).
//!
//! Two details belong to the page rather than to the model:
//!
//! - whitespace that touches a `<br>` is padding, as [`crate::dom::element_text`]
//!   says, so it is dropped — and a line that is only padding is a blank line.
//!   U+3000 is the paragraph indent and survives;
//! - a picture shares its line with the words beside it and becomes a block of
//!   its own, because a block cannot live inside a paragraph. Hameln states the
//!   picture's `alt` on the `<a>` around it, which the shared walk already
//!   reads.

use scraper::ElementRef;
use wnd_core::{Block, Inline};

use crate::blocks::from_line_blocks;
use crate::inline::{self, Piece, plain_text};

/// The blocks a `<br>`-separated container holds, in the order the page has them.
pub(super) fn blocks_of(container: &ElementRef, page: &str) -> Vec<Block> {
    from_line_blocks(lines_of(container, page))
}

/// The container's lines: each one's blocks, and whether it holds anything.
fn lines_of(container: &ElementRef, page: &str) -> Vec<Option<Vec<Block>>> {
    let mut lines = vec![Line::default()];

    for piece in inline::pieces_of(container, plain_text, page) {
        match piece {
            Piece::Image { src, alt } => {
                let line = lines.last_mut().expect("a line is always open");
                line.content = true;
                line.blocks.push(Block::Image { src, alt });
            }
            Piece::Inline(nodes) => {
                for (position, part) in split_lines(nodes).into_iter().enumerate() {
                    if position > 0 {
                        lines.push(Line::default());
                    }
                    let line = lines.last_mut().expect("a line is always open");
                    line.content |= has_content(&part);
                    if !part.is_empty() {
                        line.blocks.push(Block::Paragraph { children: part });
                    }
                }
            }
        }
    }

    lines
        .into_iter()
        .map(|line| line.content.then_some(line.blocks))
        .collect()
}

/// One line being built: its blocks, and whether anything but whitespace is in
/// it. A line of just the paragraph indent is blank, as it is in a body.
#[derive(Default)]
struct Line {
    content: bool,
    blocks: Vec<Block>,
}

/// A run of inline nodes, split where the text says a `<br>` was, with the
/// padding at each line's edges dropped.
fn split_lines(nodes: Vec<Inline>) -> Vec<Vec<Inline>> {
    let mut lines = vec![Vec::new()];

    for node in nodes {
        match node {
            Inline::Text { value } => {
                let mut parts = value.split('\n');
                push_text(
                    lines.last_mut().expect("a line is always open"),
                    parts.next().unwrap_or_default(),
                );
                for part in parts {
                    lines.push(Vec::new());
                    push_text(lines.last_mut().expect("a line is always open"), part);
                }
            }
            other => lines.last_mut().expect("a line is always open").push(other),
        }
    }

    lines.into_iter().map(trim_edges).collect()
}

/// A non-empty run of text, as a node.
fn push_text(line: &mut Vec<Inline>, value: &str) {
    if !value.is_empty() {
        line.push(Inline::Text {
            value: value.to_string(),
        });
    }
}

/// Drop the ASCII padding at a line's edges, and any node that was only that.
///
/// Only ASCII: a line may open with U+3000, which is the paragraph indent and
/// therefore content.
fn trim_edges(mut nodes: Vec<Inline>) -> Vec<Inline> {
    if let Some(Inline::Text { value }) = nodes.first_mut() {
        *value = value.trim_ascii_start().to_string();
    }
    if let Some(Inline::Text { value }) = nodes.last_mut() {
        *value = value.trim_ascii_end().to_string();
    }
    inline::normalize(nodes)
}

/// Whether a run holds anything a reader would see.
fn has_content(nodes: &[Inline]) -> bool {
    nodes.iter().any(|node| match node {
        Inline::Text { value } => !value.trim().is_empty(),
        _ => true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use scraper::{Html, Selector};

    fn text(value: &str) -> Inline {
        Inline::Text {
            value: value.to_string(),
        }
    }

    /// The blocks of a fragment, found on a page at `https://syosetu.org/`.
    fn blocks(html: &str) -> Vec<Block> {
        let document = Html::parse_document(&format!(
            r#"<!DOCTYPE html><html><body><div id="f">{html}</div></body></html>"#
        ));
        let selector = Selector::parse("#f").expect("selector");
        let element = document.select(&selector).next().expect("wrapper");
        blocks_of(&element, "https://syosetu.org/novel/000000/")
    }

    #[test]
    fn padding_that_touches_a_break_is_padding() {
        // The site pads its `<br>` runs with an ASCII space; the text walk
        // (`dom::element_text`) drops it, and so does this one.
        assert_eq!(
            blocks("<br> 一行目の見本です。 <br><br> 　二行目の見本です。<br>"),
            vec![
                Block::Paragraph {
                    children: vec![text("一行目の見本です。")]
                },
                Block::Paragraph {
                    children: vec![text("　二行目の見本です。")]
                },
            ],
            "the padding goes; the ideographic indent is content; one blank line separates"
        );
    }

    #[test]
    fn a_line_with_only_a_space_in_it_is_a_blank_line() {
        assert_eq!(
            blocks("一行目<br> <br>二行目"),
            vec![
                Block::Paragraph {
                    children: vec![text("一行目")]
                },
                Block::Paragraph {
                    children: vec![text("二行目")]
                },
            ],
            "two `<br>`s with padding between them are still one blank line"
        );
    }

    #[test]
    fn a_picture_is_a_block_of_its_own_and_the_link_around_it_is_not() {
        assert_eq!(
            blocks(
                r#"前の文<br><a href="/img/1.png" alt="挿絵" name="img"><img src="/img/1.png"></a><br>後の文"#
            ),
            vec![
                Block::Paragraph {
                    children: vec![text("前の文")]
                },
                Block::Image {
                    src: "https://syosetu.org/img/1.png".to_string(),
                    alt: Some("挿絵".to_string()),
                },
                Block::Paragraph {
                    children: vec![text("後の文")]
                },
            ]
        );
    }
}
