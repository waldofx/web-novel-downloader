//! Inline content: the HTML shapes every site shares.
//!
//! HTML is the same everywhere: `<ruby>` is furigana, `<strong>` and `<em>` are
//! emphasis, `<a>` is a link, `<br>` is a line break, and anything else
//! (`span`, `u`, `s`, `font`) is a transparent wrapper. What differs is the
//! *text between* those elements: Narou encodes furigana there as plain
//! `｜漢字《かんじ》`, Kakuyomu does not use notation at all. So the walk is shared
//! and the text decoder is passed in.
//!
//! A picture is the one thing that does not fit in a run of inline nodes: the
//! model has a picture at block level and nothing inside a paragraph, so an
//! `<img>` ends the run it sits in and a line holding one comes back as several
//! [`Piece`]s. What a line *means* is the adapter's business; what is in it is
//! this module's.
//!
//! Adapters bring two rules of their own that this module must not undo:
//!
//! - markup that is recognised is decoded here, never left as characters;
//! - a text run is handed to the decoder whole, so a decoder that needs to see
//!   `｜` and `《` together is not given them in pieces.

use scraper::{ElementRef, Node};
use url::Url;
use wnd_core::Inline;

/// How an adapter turns a run of plain text into inline nodes.
pub(crate) type TextDecoder = fn(&str) -> Vec<Inline>;

/// One piece of a line: a run of inline content, or a picture.
#[derive(Debug, PartialEq)]
pub(crate) enum Piece {
    /// Inline content: the text, ruby, emphasis and links a line is made of.
    Inline(Vec<Inline>),
    /// A picture, wherever a line has one.
    ///
    /// `src` is **absolute**, because the downloader fetches a block with no
    /// page to complete a relative address against; the walk resolves it
    /// against the page it was found on. `alt` is what the page said the
    /// picture was, when it said anything at all.
    Image { src: String, alt: Option<String> },
}

impl Piece {
    /// Whether this piece is a picture. A line holding one is not a blank line,
    /// however empty its text is — an `alt` is an attribute, not text.
    pub(crate) fn is_image(&self) -> bool {
        matches!(self, Piece::Image { .. })
    }
}

/// The content of one element: its runs of inline nodes, and the pictures
/// between them, in the order the page has them.
///
/// `page` is the address the content was found at, and the only thing that can
/// complete a picture's `src`.
pub(crate) fn pieces_of(element: &ElementRef, decode: TextDecoder, page: &str) -> Vec<Piece> {
    let mut pieces = Pieces {
        runs: Vec::new(),
        nodes: Vec::new(),
        pending: String::new(),
        decode,
        page,
    };
    walk(element, &mut pieces, None);
    pieces.finish()
}

/// The inline content of one element, with `decode` applied to its text runs.
///
/// Pictures are dropped, because there is nowhere here to put one: this is what
/// a *paragraph* is made of, and a picture is a block beside it rather than a
/// character in it. A caller that can hold a picture asks for [`pieces_of`].
pub(crate) fn of(element: &ElementRef, decode: TextDecoder) -> Vec<Inline> {
    normalize(
        pieces_of(element, decode, "")
            .into_iter()
            .filter_map(|piece| match piece {
                Piece::Inline(nodes) => Some(nodes),
                Piece::Image { .. } => None,
            })
            .flatten()
            .collect(),
    )
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

/// Collects a line: runs of inline nodes, and the pictures between them.
struct Pieces<'a> {
    runs: Vec<Piece>,
    nodes: Vec<Inline>,
    pending: String,
    decode: TextDecoder,
    page: &'a str,
}

impl Pieces<'_> {
    fn push_text(&mut self, text: &str) {
        self.pending.push_str(text);
    }

    fn push_inline(&mut self, node: Inline) {
        self.flush_text();
        self.nodes.push(node);
    }

    /// A picture ends the run it sits in: it is not inline content, and a block
    /// cannot live inside a paragraph.
    fn push_picture(&mut self, src: &str, alt: Option<String>) {
        self.flush_text();
        self.flush_run();
        self.runs.push(Piece::Image {
            src: absolute(self.page, src),
            alt,
        });
    }

    fn finish(mut self) -> Vec<Piece> {
        self.flush_text();
        self.flush_run();
        self.runs
    }

    fn flush_text(&mut self) {
        if !self.pending.is_empty() {
            let text = std::mem::take(&mut self.pending);
            let decoded = (self.decode)(&text);
            self.nodes.extend(decoded);
        }
    }

    fn flush_run(&mut self) {
        if !self.nodes.is_empty() {
            let nodes = std::mem::take(&mut self.nodes);
            self.runs.push(Piece::Inline(normalize(nodes)));
        }
    }
}

/// Walk the child nodes of an element, because text and markup are siblings.
///
/// `anchor_alt` is the `alt` of the `<a>` this content sits inside, if it sits
/// inside one: Hameln states a picture's `alt` there, which is not where HTML
/// puts it and is where the site does.
fn walk(element: &ElementRef, pieces: &mut Pieces, anchor_alt: Option<&str>) {
    let mut child = element.first_child();
    while let Some(node) = child {
        // Advance the cursor before using `node`; the nodes are reference counted.
        child = node.next_sibling();
        match node.value() {
            Node::Text(text) => pieces.push_text(text),
            Node::Element(el) if el.name() == "br" => pieces.push_text("\n"),
            Node::Element(el) if el.name() == "ruby" => {
                if let Some(element) = ElementRef::wrap(node) {
                    pieces.push_inline(ruby_of(&element));
                }
            }
            Node::Element(el) => {
                if let Some(element) = ElementRef::wrap(node) {
                    match el.name() {
                        "b" | "strong" => {
                            let children = of(&element, pieces.decode);
                            pieces.push_inline(Inline::Strong { children });
                        }
                        "em" | "i" => {
                            let children = of(&element, pieces.decode);
                            pieces.push_inline(Inline::Emphasis { children });
                        }
                        "a" => link(&element, pieces),
                        "img" => picture(&element, pieces, anchor_alt),
                        // Anything else (span, u, s, font, …) is a transparent
                        // wrapper.
                        _ => walk(&element, pieces, anchor_alt),
                    }
                }
            }
            _ => {}
        }
    }
}

/// An `<a>`: a link around a run of text, or the way both sites attach a
/// picture.
///
/// A link is inline and a picture is a block, so the two cannot be one node. A
/// picture says everything the link around it said — the address it offers is
/// the same picture, one size up — so the picture is kept and the link around it
/// is not.
fn link(element: &ElementRef, pieces: &mut Pieces) {
    if holds_picture(element) {
        walk(element, pieces, element.value().attr("alt"));
        return;
    }
    let href = element.value().attr("href").unwrap_or("").to_string();
    let children = of(element, pieces.decode);
    pieces.push_inline(Inline::Link { href, children });
}

/// An `<img>`: a picture, or nothing when the page states no address for one.
///
/// A page is full of `<img>` that are not the author's — logos, spacers,
/// tracking pixels — which is why an adapter hands this walk a body and not a
/// page, and why a picture with no address is dropped rather than guessed at.
fn picture(element: &ElementRef, pieces: &mut Pieces, anchor_alt: Option<&str>) {
    let Some(src) = address(element) else {
        return;
    };
    let alt = element
        .value()
        .attr("alt")
        .or(anchor_alt)
        .map(str::trim_ascii)
        .filter(|alt| !alt.is_empty())
        .map(str::to_string);
    pieces.push_picture(src, alt);
}

/// An `<img>`'s address, when the page states one that can be used.
fn address<'a>(element: &'a ElementRef<'_>) -> Option<&'a str> {
    let src = element.value().attr("src")?;
    (!src.trim_ascii().is_empty()).then_some(src)
}

/// Whether an element has a usable picture anywhere inside it.
///
/// A link around an `<img>` with no address is still a link around text, so this
/// asks the same question [`picture`] does.
fn holds_picture(element: &ElementRef) -> bool {
    let mut child = element.first_child();
    while let Some(node) = child {
        child = node.next_sibling();
        if let Node::Element(el) = node.value() {
            let nested = ElementRef::wrap(node);
            if el.name() == "img" && nested.as_ref().and_then(address).is_some() {
                return true;
            }
            if nested.is_some_and(|nested| holds_picture(&nested)) {
                return true;
            }
        }
    }
    false
}

/// A picture's address as an absolute one, resolved against the page it was
/// found on.
///
/// `//host/path` is common enough to matter — Narou writes its illustrations
/// that way — and is not an address on its own. A `src` that cannot be resolved
/// at all is kept exactly as written, because it is the only record of what the
/// site offered.
fn absolute(page: &str, src: &str) -> String {
    Url::parse(page)
        .and_then(|page| page.join(src))
        .map(|url| url.to_string())
        .unwrap_or_else(|_| src.to_string())
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

    /// The same fragment as pieces, found on a page at `page`.
    fn piece_list(html: &str, page: &str) -> Vec<Piece> {
        let document = Html::parse_document(&format!(
            r#"<!DOCTYPE html><html><body><div id="f">{html}</div></body></html>"#
        ));
        let selector = Selector::parse("#f").expect("selector");
        let element = document.select(&selector).next().expect("wrapper");
        pieces_of(&element, plain_text, page)
    }

    fn picture(src: &str, alt: Option<&str>) -> Piece {
        Piece::Image {
            src: src.to_string(),
            alt: alt.map(str::to_string),
        }
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

    #[test]
    fn a_picture_ends_the_run_it_sits_in() {
        assert_eq!(
            piece_list(
                r#"前の文<img src="/i/1.png" alt=" 挿絵 " />後の文"#,
                "https://site.test/novel/1"
            ),
            vec![
                Piece::Inline(vec![text("前の文")]),
                // Relative on the page, absolute once it is a piece: the
                // downloader has no page to complete it against.
                picture("https://site.test/i/1.png", Some("挿絵")),
                Piece::Inline(vec![text("後の文")]),
            ]
        );
    }

    #[test]
    fn a_protocol_relative_picture_is_resolved_against_the_page() {
        // Narou writes its illustrations this way, and `//host/path` alone is
        // not an address at all.
        assert_eq!(
            piece_list(
                r#"<img src="//8873.mitemin.net/userpageimage/viewimagebig/icode/i78408/" alt="挿絵(By みてみん)" />"#,
                "https://ncode.syosetu.com/n4890bm/288/"
            ),
            vec![picture(
                "https://8873.mitemin.net/userpageimage/viewimagebig/icode/i78408/",
                Some("挿絵(By みてみん)")
            )]
        );
    }

    #[test]
    fn a_picture_wrapped_in_a_link_is_kept_and_the_link_around_it_is_not() {
        // A picture is a block and a link is inline, so the two cannot be one
        // node; the picture is what the author put there.
        assert_eq!(
            piece_list(
                r#"<a href="//8873.mitemin.net/i78408/" target="_blank"><img src="//8873.mitemin.net/userpageimage/viewimagebig/icode/i78408/" alt="挿絵" border="0" /></a>"#,
                "https://ncode.syosetu.com/n4890bm/288/"
            ),
            vec![picture(
                "https://8873.mitemin.net/userpageimage/viewimagebig/icode/i78408/",
                Some("挿絵")
            )]
        );
    }

    #[test]
    fn the_alt_the_link_states_is_the_picture_s_alt_when_the_picture_has_none() {
        // Hameln puts the `alt` on the `<a>` around the picture, which is not
        // where HTML says it goes and is where the site puts it.
        assert_eq!(
            piece_list(
                r#"<a href="/img/user/196658/125290.png" alt="挿絵" name="img"><img src="/img/user/196658/125290.png" border="0" loading="lazy" /></a>"#,
                "https://syosetu.org/novel/316401/1.html"
            ),
            vec![picture(
                "https://syosetu.org/img/user/196658/125290.png",
                Some("挿絵")
            )]
        );
    }

    #[test]
    fn a_picture_with_no_address_is_not_a_picture() {
        assert_eq!(
            piece_list(r#"a<img />b<img src="  " />c"#, "https://site.test/x"),
            vec![Piece::Inline(vec![text("abc")])],
            "chrome images and spacers offer no address, so they contribute nothing"
        );
    }

    #[test]
    fn a_link_around_a_spacer_is_still_a_link() {
        assert_eq!(
            piece_list(
                r#"<a href="/x">link<img src="" width="0" height="0" /></a>"#,
                "https://site.test/n/1"
            ),
            vec![Piece::Inline(vec![Inline::Link {
                href: "/x".to_string(),
                children: vec![text("link")],
            }])],
            "only a usable picture takes the link with it"
        );
    }

    #[test]
    fn an_address_that_cannot_be_resolved_is_left_exactly_as_written() {
        assert_eq!(
            piece_list(r#"<img src="挿絵.png" alt="挿絵" />"#, ""),
            vec![picture("挿絵.png", Some("挿絵"))],
            "the address is the only record of what the site offered"
        );
    }

    #[test]
    fn of_keeps_the_text_and_leaves_the_picture() {
        assert_eq!(
            inlines(r#"a<img src="i.png" />b"#),
            vec![text("ab")],
            "the inline walk has nowhere to put a block"
        );
    }
}
