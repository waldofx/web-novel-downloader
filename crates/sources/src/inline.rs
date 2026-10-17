//! Inline content: the HTML shapes every site shares.
//!
//! HTML is the same everywhere: `<ruby>` is furigana, `<strong>` and `<em>` are
//! emphasis, `<a>` is a link, `<br>` is a line break, and anything else
//! (`span`, `u`, `s`, `font`) is a transparent wrapper. What differs is the
//! *text between* those elements: Narou encodes furigana there as plain
//! `｜漢字《かんじ》`, Kakuyomu does not use notation at all. So the walk is shared
//! and the text decoder is passed in.
//!
//! Adapters bring two rules of their own that this module must not undo:
//!
//! - markup that is recognised is decoded here, never left as characters;
//! - a text run is handed to the decoder whole, so a decoder that needs to see
//!   `｜` and `《` together is not given them in pieces.

use scraper::{ElementRef, Node};
use wnd_core::Inline;

/// How an adapter turns a run of plain text into inline nodes.
pub(crate) type TextDecoder = fn(&str) -> Vec<Inline>;

/// The inline content of one element, with `decode` applied to its text runs.
pub(crate) fn of(element: &ElementRef, decode: TextDecoder) -> Vec<Inline> {
    let mut builder = InlineBuilder {
        nodes: Vec::new(),
        pending: String::new(),
        decode,
    };
    walk_children(element, &mut builder);
    builder.finish()
}

/// Text as-is: no notation to decode.
///
/// What every site gets except the Narou family, whose plain text is markup.
pub(crate) fn plain_text(text: &str) -> Vec<Inline> {
    if text.is_empty() {
        Vec::new()
    } else {
        vec![Inline::Text {
            value: text.to_string(),
        }]
    }
}

/// Merge adjacent text nodes and drop empty ones.
pub(crate) fn normalize(nodes: Vec<Inline>) -> Vec<Inline> {
    let mut out: Vec<Inline> = Vec::new();
    for node in nodes {
        match node {
            Inline::Text { value } if value.is_empty() => {}
            Inline::Text { value } => match out.last_mut() {
                Some(Inline::Text { value: previous }) => previous.push_str(&value),
                _ => out.push(Inline::Text { value }),
            },
            other => out.push(other),
        }
    }
    out
}

/// Collects inline nodes, decoding each run of text between elements.
struct InlineBuilder {
    nodes: Vec<Inline>,
    pending: String,
    decode: TextDecoder,
}

impl InlineBuilder {
    fn push_text(&mut self, text: &str) {
        self.pending.push_str(text);
    }

    fn push_inline(&mut self, node: Inline) {
        self.flush();
        self.nodes.push(node);
    }

    fn finish(mut self) -> Vec<Inline> {
        self.flush();
        normalize(self.nodes)
    }

    fn flush(&mut self) {
        if !self.pending.is_empty() {
            let text = std::mem::take(&mut self.pending);
            let decoded = (self.decode)(&text);
            self.nodes.extend(decoded);
        }
    }
}

/// Walk the child nodes of an element, because text and markup are siblings.
fn walk_children(element: &ElementRef, builder: &mut InlineBuilder) {
    let mut child = element.first_child();
    while let Some(node) = child {
        // Advance the cursor before using `node`; the nodes are reference counted.
        child = node.next_sibling();
        match node.value() {
            Node::Text(text) => builder.push_text(text),
            Node::Element(el) if el.name() == "br" => builder.push_text("\n"),
            Node::Element(el) if el.name() == "ruby" => {
                if let Some(element) = ElementRef::wrap(node) {
                    builder.push_inline(ruby_of(&element));
                }
            }
            Node::Element(el) => {
                if let Some(element) = ElementRef::wrap(node) {
                    match el.name() {
                        "b" | "strong" => {
                            let children = of(&element, builder.decode);
                            builder.push_inline(Inline::Strong { children });
                        }
                        "em" | "i" => {
                            let children = of(&element, builder.decode);
                            builder.push_inline(Inline::Emphasis { children });
                        }
                        "a" => {
                            let href = element.value().attr("href").unwrap_or("").to_string();
                            let children = of(&element, builder.decode);
                            builder.push_inline(Inline::Link { href, children });
                        }
                        // Inline images are a block-level concern and are not
                        // handled here; anything else (span, u, s, font, …) is a
                        // transparent wrapper.
                        "img" => {}
                        _ => walk_children(&element, builder),
                    }
                }
            }
            _ => {}
        }
    }
}

/// `<ruby><rb>漢字</rb><rp>（</rp><rt>かんじ</rt><rp>）</rp></ruby>` →
/// [`Inline::Ruby`].
///
/// `<rb>` is optional in the HTML spec and Kakuyomu always writes it, but the
/// base is read from the ruby element's own content either way. `<rp>` is the
/// fallback text for browsers that cannot show ruby and is dropped.
fn ruby_of(element: &ElementRef) -> Inline {
    let mut base = String::new();
    let mut reading = String::new();
    collect_ruby(element, RubyPart::Base, &mut base, &mut reading);
    Inline::Ruby {
        base: base.trim_ascii().to_string(),
        reading: reading.trim_ascii().to_string(),
    }
}

#[derive(Clone, Copy)]
enum RubyPart {
    Base,
    Reading,
    Ignore,
}

fn collect_ruby(element: &ElementRef, part: RubyPart, base: &mut String, reading: &mut String) {
    let mut child = element.first_child();
    while let Some(node) = child {
        child = node.next_sibling();
        match node.value() {
            Node::Text(text) => match part {
                RubyPart::Base => base.push_str(text),
                RubyPart::Reading => reading.push_str(text),
                RubyPart::Ignore => {}
            },
            Node::Element(el) => {
                let part = match el.name() {
                    "rt" => RubyPart::Reading,
                    "rp" => RubyPart::Ignore,
                    _ => part,
                };
                if let Some(element) = ElementRef::wrap(node) {
                    collect_ruby(&element, part, base, reading);
                }
            }
            _ => {}
        }
    }
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

    /// Parse a fragment and return the inline content of its `#f` wrapper.
    fn inlines(html: &str) -> Vec<Inline> {
        let document = Html::parse_document(&format!(
            r#"<!DOCTYPE html><html><body><div id="f">{html}</div></body></html>"#
        ));
        let selector = Selector::parse("#f").expect("selector");
        let element = document.select(&selector).next().expect("wrapper");
        of(&element, plain_text)
    }

    #[test]
    fn markup_becomes_inline_nodes() {
        assert_eq!(
            inlines(r#"a<em>e</em><a href="/x">l</a><span>s</span><img src="i.png" />b"#),
            vec![
                text("a"),
                Inline::Emphasis {
                    children: vec![text("e")]
                },
                Inline::Link {
                    href: "/x".to_string(),
                    children: vec![text("l")]
                },
                // `span` is a transparent wrapper and `img` is not inline
                // content, so the text on either side of them merges.
                text("sb"),
            ]
        );
    }

    #[test]
    fn ruby_reads_the_base_from_the_ruby_and_the_reading_from_rt() {
        assert_eq!(
            inlines("　<ruby><rb>魔王</rb><rp>（</rp><rt>俺</rt><rp>）</rp></ruby>の強化"),
            vec![
                text("　"),
                Inline::Ruby {
                    base: "魔王".into(),
                    reading: "俺".into()
                },
                text("の強化"),
            ],
            "the rp fallbacks are dropped, not folded into the base"
        );
    }

    #[test]
    fn ruby_without_an_rb_still_reads_the_base() {
        assert_eq!(
            inlines("<ruby>漢字<rp>(</rp><rt>かんじ</rt><rp>)</rp></ruby>だ。"),
            vec![
                Inline::Ruby {
                    base: "漢字".into(),
                    reading: "かんじ".into()
                },
                text("だ。"),
            ]
        );
    }

    #[test]
    fn a_br_becomes_a_newline_inside_a_paragraph() {
        assert_eq!(inlines("a<br />b"), vec![text("a\nb")]);
    }

    #[test]
    fn strong_is_not_confused_with_emphasis() {
        assert_eq!(
            inlines("<strong>s</strong><em>e</em>"),
            vec![
                Inline::Strong {
                    children: vec![text("s")]
                },
                Inline::Emphasis {
                    children: vec![text("e")]
                },
            ]
        );
    }

    #[test]
    fn plain_text_is_one_node_and_no_node() {
        assert_eq!(plain_text("本文"), vec![text("本文")]);
        assert_eq!(plain_text(""), Vec::new());
    }

    #[test]
    fn adjacent_text_nodes_are_merged_and_empty_ones_dropped() {
        assert_eq!(
            normalize(vec![text("a"), text(""), text("b"), text("c")]),
            vec![text("abc")]
        );
        assert_eq!(
            normalize(vec![
                text("a"),
                Inline::Emphasis {
                    children: vec![text("e")]
                },
                text("b")
            ]),
            vec![
                text("a"),
                Inline::Emphasis {
                    children: vec![text("e")]
                },
                text("b")
            ],
            "only text merges; markup separates it"
        );
    }
}
