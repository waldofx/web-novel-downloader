//! Mechanical DOM helpers, shared by every adapter.
//!
//! HTML is the same everywhere, so pulling text out of it is the same job
//! everywhere: deeply nested elements, incidental whitespace, and `<br>` meaning
//! a line break. This module is the one place that knowledge lives.
//!
//! Two rules run through all of it:
//!
//! - a `<br>` becomes a newline, and whitespace touching it is dropped, so
//!   `<br /> <br />` reads as two line breaks and not four;
//! - trimming is **ASCII-only**, because Japanese sites indent with U+3000 and
//!   that is content, not padding.

use scraper::{ElementRef, Html, Node, Selector};

/// All text under an element, with `<br>` becoming a newline.
///
/// Lines are trimmed of ASCII padding, and leading/trailing blank lines are
/// dropped. U+3000 (the paragraph indent) survives.
pub(crate) fn element_text(element: &ElementRef) -> String {
    let mut out = String::new();
    collect_text(element, &mut out);
    if !out.contains('\n') {
        return out.trim_ascii().to_string();
    }
    out.split('\n')
        .map(|line| line.trim_ascii())
        .collect::<Vec<_>>()
        .join("\n")
        .trim_ascii()
        .to_string()
}

fn collect_text(element: &ElementRef, out: &mut String) {
    let mut child = element.first_child();
    while let Some(node) = child {
        child = node.next_sibling();
        match node.value() {
            Node::Text(text) => out.push_str(text),
            Node::Element(el) if el.name() == "br" => out.push('\n'),
            Node::Element(_) => {
                if let Some(element) = ElementRef::wrap(node) {
                    collect_text(&element, out);
                }
            }
            _ => {}
        }
    }
}

/// The first element matching `selector`, as [`element_text`].
pub(crate) fn first_text(document: &Html, selector: &Selector) -> Option<String> {
    document
        .select(selector)
        .next()
        .map(|element| element_text(&element))
}

/// Whether an element carries a class.
pub(crate) fn is_class(element: &ElementRef, class: &str) -> bool {
    element.value().classes().any(|name| name == class)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wrap a fragment in a document and extract the text of the wrapper.
    fn text_of(html: &str) -> String {
        let document = Html::parse_document(&format!(
            r#"<!DOCTYPE html><html><body><div id="f">{html}</div></body></html>"#
        ));
        let selector = Selector::parse("#f").expect("selector");
        let element = document.select(&selector).next().expect("wrapper");
        element_text(&element)
    }

    /// The first element matching `css`.
    fn element_of(html: &str, css: &str) -> String {
        let document = Html::parse_document(html);
        let selector = Selector::parse(css).expect("selector");
        let element = document.select(&selector).next().expect("element");
        element_text(&element)
    }

    #[test]
    fn a_br_becomes_a_newline() {
        assert_eq!(text_of("a<br />b"), "a\nb");
    }

    #[test]
    fn whitespace_touching_a_line_break_is_dropped() {
        assert_eq!(text_of("a <br /> <br /> b"), "a\n\nb");
    }

    #[test]
    fn ascii_padding_is_trimmed_but_ideographic_indentation_survives() {
        // The leading U+3000 is the paragraph indent and the trailing one is
        // content; only the ASCII padding around them is padding.
        assert_eq!(text_of(" 　本文　 "), "　本文　");
        assert_eq!(text_of("\t 本文 \n"), "本文");
    }

    #[test]
    fn leading_and_trailing_blank_lines_are_dropped() {
        assert_eq!(text_of("<br />a<br />"), "a");
    }

    #[test]
    fn nested_elements_contribute_their_text() {
        assert_eq!(text_of("<span>a</span><b>b</b>"), "ab");
    }

    #[test]
    fn classes_are_matched_exactly() {
        let document =
            Html::parse_document(r#"<html><body><div class="one two"></div></body></html>"#);
        let selector = Selector::parse("div").expect("selector");
        let element = document.select(&selector).next().expect("div");

        assert!(is_class(&element, "one"));
        assert!(is_class(&element, "two"));
        assert!(!is_class(&element, "on"), "class matching is exact");
        assert!(
            !is_class(&element, "one two"),
            "a class is not a substring of the attribute"
        );
    }

    #[test]
    fn a_hashed_class_is_matched_by_its_whole_name() {
        // Kakuyomu's CSS-module classes carry a build hash that changes on every
        // deploy. `is_class` is used only for the plain hooks the site keeps
        // stable (`blank`), never for those.
        let document = Html::parse_document(
            r#"<html><body><div class="widget-episodeBody js-episode-body blank"></div></body></html>"#,
        );
        let selector = Selector::parse("div").expect("selector");
        let element = document.select(&selector).next().expect("div");

        assert!(is_class(&element, "widget-episodeBody"), "a plain hook");
        assert!(is_class(&element, "js-episode-body"), "and its JS twin");
        assert!(is_class(&element, "blank"));
        assert!(!is_class(&element, "widget-episodeBody__H2007"));
    }

    #[test]
    fn the_first_match_wins() {
        let html = r#"<html><body><p>first</p><p>second</p></body></html>"#;
        assert_eq!(element_of(html, "p"), "first");
    }
}
