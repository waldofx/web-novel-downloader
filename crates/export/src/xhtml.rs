//! XHTML rendering, shared by the EPUB writer.
//!
//! The EPUB is the one format that can express what the model captured — real
//! ruby, real emphasis, real links — so nothing is flattened here.

use std::fmt::Write as _;

use wnd_core::{Block, Inline};

/// Escape text for XHTML content or an attribute value.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(character),
        }
    }
    out
}

/// Render inline nodes as XHTML, structure intact.
pub fn inlines(nodes: &[Inline]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            Inline::Text { value } => out.push_str(&escape(value)),
            Inline::Ruby { base, reading } => {
                if reading.is_empty() {
                    out.push_str(&escape(base));
                } else {
                    // <rp> is the fallback for readers that do not render ruby;
                    // ignoring it would show the reading run into the base text.
                    let _ = write!(
                        out,
                        "<ruby>{}<rp>(</rp><rt>{}</rt><rp>)</rp></ruby>",
                        escape(base),
                        escape(reading)
                    );
                }
            }
            Inline::Emphasis { children } => {
                let _ = write!(out, "<em>{}</em>", inlines(children));
            }
            Inline::Strong { children } => {
                let _ = write!(out, "<strong>{}</strong>", inlines(children));
            }
            Inline::Link { href, children } => {
                let _ = write!(
                    out,
                    "<a href=\"{}\">{}</a>",
                    escape(href),
                    inlines(children)
                );
            }
        }
    }
    out
}

/// Inline content with its line structure kept.
///
/// A synopsis arrives with newlines in it; in XHTML a newline is only whitespace,
/// so it has to become a `<br />` or the lines would run together.
fn text_with_breaks(nodes: &[Inline]) -> String {
    inlines(nodes).replace('\n', "<br />\n")
}

/// Block content, as the inside of a `<body>` or a `<section>`.
///
/// `image_prefix` is the relative path from the document being written to the
/// archive root, because that is what the model's `src` is relative to.
pub fn blocks(blocks: &[Block], image_prefix: &str) -> String {
    let mut out = String::new();
    for block in blocks {
        match block {
            Block::Paragraph { children } => {
                let _ = writeln!(out, "<p>{}</p>", text_with_breaks(children));
            }
            Block::Heading { level, children } => {
                // The model allows any level; XHTML has six.
                let level = (*level).clamp(1, 6);
                let _ = writeln!(out, "<h{level}>{}</h{level}>", inlines(children));
            }
            // A rule is what a scene break *is*; no text marker is needed.
            Block::SceneBreak => out.push_str("<hr class=\"scene-break\" />\n"),
            Block::HorizontalRule => out.push_str("<hr />\n"),
            Block::Image { src, alt } => {
                let _ = writeln!(
                    out,
                    "<img src=\"{image_prefix}{}\" alt=\"{}\" />",
                    escape(src),
                    escape(alt.as_deref().unwrap_or(""))
                );
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_escaped() {
        assert_eq!(escape("a <b> & \"c\""), "a &lt;b&gt; &amp; &quot;c&quot;");
    }

    #[test]
    fn ruby_keeps_its_base_and_reading_apart() {
        let nodes = vec![Inline::Ruby {
            base: "漢字".to_string(),
            reading: "かんじ".to_string(),
        }];

        assert_eq!(
            inlines(&nodes),
            "<ruby>漢字<rp>(</rp><rt>かんじ</rt><rp>)</rp></ruby>"
        );
    }

    #[test]
    fn newlines_become_line_breaks() {
        let nodes = vec![Inline::Text {
            value: "one\ntwo".to_string(),
        }];

        assert_eq!(text_with_breaks(&nodes), "one<br />\ntwo");
    }
}
