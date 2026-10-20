//! The plain-text projection: what furigana becomes when the format cannot draw
//! it, and how inline styling is dropped without losing the words.

use wnd_core::{Block, Inline};

use crate::RubyStyle;

/// `漢字(かんじ)`, or just `漢字` when the reading has been turned off.
///
/// Parentheses rather than `｜漢字《かんじ》`: these formats are read as prose,
/// and the notation the *source* used is an implementation detail of the site.
pub fn ruby(style: RubyStyle, base: &str, reading: &str) -> String {
    match style {
        RubyStyle::Parenthesised if !reading.is_empty() => format!("{base}({reading})"),
        _ => base.to_string(),
    }
}

/// Flatten inline nodes to text: the words survive, the styling does not.
///
/// A link keeps its text and loses its target, which is the point of a plain
/// text projection. The Markdown and EPUB exports keep both.
pub fn inlines(nodes: &[Inline], style: RubyStyle) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            Inline::Text { value } => out.push_str(value),
            Inline::Ruby { base, reading } => out.push_str(&ruby(style, base, reading)),
            Inline::Emphasis { children } | Inline::Strong { children } => {
                out.push_str(&inlines(children, style));
            }
            Inline::Link { children, .. } => out.push_str(&inlines(children, style)),
        }
    }
    out
}

/// A block list as one run of prose: paragraphs and headings keep their words,
/// separated by a blank line; pictures and rules are left out.
///
/// For metadata — the EPUB's `dc:description` — which a reader shows as text. A
/// picture has no place in a description, and its alt text is a caption rather
/// than the prose around it.
pub fn blocks(blocks: &[Block], style: RubyStyle) -> String {
    let paragraphs: Vec<String> = blocks
        .iter()
        .filter_map(|block| match block {
            Block::Paragraph { children } | Block::Heading { children, .. } => {
                Some(inlines(children, style))
            }
            Block::SceneBreak | Block::HorizontalRule | Block::Image { .. } => None,
        })
        .collect();
    paragraphs.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(value: &str) -> Inline {
        Inline::Text {
            value: value.to_string(),
        }
    }

    #[test]
    fn ruby_without_a_reading_is_just_the_base() {
        assert_eq!(ruby(RubyStyle::Parenthesised, "頁", ""), "頁");
        assert_eq!(ruby(RubyStyle::BaseOnly, "頁", "ページ"), "頁");
        assert_eq!(ruby(RubyStyle::Parenthesised, "頁", "ページ"), "頁(ページ)");
    }

    #[test]
    fn nested_emphasis_keeps_its_words() {
        let nodes = vec![
            text("前"),
            Inline::Strong {
                children: vec![text("中"), text("身")],
            },
            text("後"),
        ];

        assert_eq!(inlines(&nodes, RubyStyle::Parenthesised), "前中身後");
    }
}
