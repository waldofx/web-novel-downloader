//! Narou's plain-text notation, decoded on top of the shared inline walk.
//!
//! Narou writes furigana in *plain text* as well as in HTML:
//!
//! - `｜漢字《かんじ》` — explicit ruby, `｜` marks where the base starts
//! - `漢字《かんじ》` — implicit ruby, the base is the trailing run of kanji
//! - `《《強調》》` — the site's own emphasis
//!
//! These characters are markup, not punctuation, so they are decoded here. A
//! `｜` or `《` with no usable base stays literal text: showing a stray mark is
//! better than dropping a character the author typed.
//!
//! The HTML around the text — `<ruby>`, `<strong>`, `<em>`, `<a>` — is the same
//! as everywhere else and is handled by [`crate::inline`].

use crate::inline::{self, TextDecoder};
use scraper::ElementRef;
use wnd_core::Inline;

/// The inline content of one element, decoding Narou's notation too.
pub(super) fn inlines_of(element: &ElementRef) -> Vec<Inline> {
    let decode: TextDecoder = parse_notation;
    inline::of(element, decode)
}

/// Decode Narou's plain-text ruby and emphasis notation into inline nodes.
///
/// Text without markup is returned as a single [`Inline::Text`].
fn parse_notation(text: &str) -> Vec<Inline> {
    let chars: Vec<char> = text.chars().collect();
    let mut nodes: Vec<Inline> = Vec::new();
    let mut plain = String::new();
    let mut i = 0usize;

    while i < chars.len() {
        // Emphasis: 《《 … 》》
        if chars[i] == '《' && chars.get(i + 1) == Some(&'《') {
            if let Some(end) = find_index(&chars, i + 2, '》') {
                if chars.get(end + 1) == Some(&'》') {
                    push_text(&mut nodes, &plain);
                    plain.clear();
                    let inner: String = chars[i + 2..end].iter().collect();
                    nodes.push(Inline::Emphasis {
                        children: parse_notation(&inner),
                    });
                    i = end + 2;
                    continue;
                }
            }
        }

        // Ruby: base《reading》. The base is whatever precedes the 《 in the
        // pending text — after a ｜ if one is present, else the trailing kanji.
        if chars[i] == '《' {
            if let Some(end) = find_index(&chars, i + 1, '》') {
                let reading: String = chars[i + 1..end].iter().collect();
                if let Some((keep, base)) = split_ruby_base(&plain) {
                    let keep = keep.to_string();
                    let base = base.to_string();
                    plain.clear();
                    push_text(&mut nodes, &keep);
                    nodes.push(Inline::Ruby { base, reading });
                    i = end + 1;
                    continue;
                }
            }
        }

        plain.push(chars[i]);
        i += 1;
    }

    push_text(&mut nodes, &plain);
    inline::normalize(nodes)
}

/// First index at or after `from` holding `close`.
fn find_index(chars: &[char], from: usize, close: char) -> Option<usize> {
    chars
        .iter()
        .enumerate()
        .skip(from)
        .find(|(_, c)| **c == close)
        .map(|(index, _)| index)
}

/// Split pending text into the part to keep and the ruby base.
///
/// `None` means the 《 has no base and is therefore literal prose.
fn split_ruby_base(before: &str) -> Option<(&str, &str)> {
    if let Some(marker) = before.rfind('｜') {
        // Explicit: everything after ｜ is the base, even if it is kana.
        let base = &before[marker + '｜'.len_utf8()..];
        if base.is_empty() {
            return None;
        }
        return Some((&before[..marker], base));
    }

    // Implicit: the base is the longest run of kanji at the end.
    let start = before
        .char_indices()
        .rev()
        .take_while(|(_, c)| is_kanji(*c))
        .last()
        .map(|(index, _)| index)?;
    Some((&before[..start], &before[start..]))
}

/// Whether a character can implicitly start a ruby base.
fn is_kanji(c: char) -> bool {
    matches!(c,
        '\u{3005}'                  // 々, the iteration mark
        | '\u{3400}'..='\u{4DBF}'   // CJK Unified Ideographs Extension A
        | '\u{4E00}'..='\u{9FFF}'   // CJK Unified Ideographs
        | '\u{F900}'..='\u{FAFF}'   // CJK Compatibility Ideographs
    )
}

fn push_text(nodes: &mut Vec<Inline>, text: &str) {
    if !text.is_empty() {
        nodes.push(Inline::Text {
            value: text.to_string(),
        });
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

    #[test]
    fn explicit_ruby_accepts_a_kana_base() {
        assert_eq!(
            parse_notation("｜これ《これ》"),
            vec![Inline::Ruby {
                base: "これ".into(),
                reading: "これ".into()
            }]
        );
    }

    #[test]
    fn explicit_ruby_takes_the_longest_kanji_run_implicitly() {
        assert_eq!(
            parse_notation("今日は漢字《かんじ》"),
            vec![
                text("今日は"),
                Inline::Ruby {
                    base: "漢字".into(),
                    reading: "かんじ".into()
                }
            ]
        );
        assert_eq!(
            parse_notation("々《くりかえし》"),
            vec![Inline::Ruby {
                base: "々".into(),
                reading: "くりかえし".into()
            }]
        );
    }

    #[test]
    fn emphasis_notation_becomes_an_emphasis_node() {
        assert_eq!(
            parse_notation("あ《《強調》》い"),
            vec![
                text("あ"),
                Inline::Emphasis {
                    children: vec![text("強調")]
                },
                text("い")
            ]
        );
    }

    #[test]
    fn notation_without_markup_is_one_text_node() {
        assert_eq!(
            parse_notation("ただの文です。"),
            vec![text("ただの文です。")]
        );
        assert_eq!(parse_notation(""), vec![]);
    }

    #[test]
    fn a_lone_marker_is_kept_literally() {
        assert_eq!(parse_notation("終わり｜"), vec![text("終わり｜")]);
        assert_eq!(parse_notation("《開き》なし"), vec![text("《開き》なし")]);
    }

    /// The HTML walk is shared, but these two tests go through *this* adapter's
    /// entry point, so they fail if Narou ever stops decoding its own notation.
    fn chapter_text(html: &str) -> Vec<Inline> {
        let document = Html::parse_document(&format!(
            r#"<!DOCTYPE html><html><body><div id="f">{html}</div></body></html>"#
        ));
        let selector = Selector::parse("#f").expect("selector");
        let element = document.select(&selector).next().expect("wrapper");
        inlines_of(&element)
    }

    #[test]
    fn html_markup_becomes_inline_nodes() {
        assert_eq!(
            chapter_text(r#"a<em>e</em><a href="/x">l</a><span>s</span><img src="i.png" />b"#),
            vec![
                text("a"),
                Inline::Emphasis {
                    children: vec![text("e")]
                },
                Inline::Link {
                    href: "/x".to_string(),
                    children: vec![text("l")]
                },
                // `span` is a transparent wrapper and `img` contributes nothing,
                // so the text on either side of them merges.
                text("sb"),
            ]
        );
    }

    #[test]
    fn html_ruby_is_read_from_rt() {
        assert_eq!(
            chapter_text("　<ruby>漢字<rp>(</rp><rt>かんじ</rt><rp>)</rp></ruby>だ。"),
            vec![
                text("　"),
                Inline::Ruby {
                    base: "漢字".into(),
                    reading: "かんじ".into()
                },
                text("だ。"),
            ],
            "rp fallbacks are ignored so they do not leak into the text"
        );
    }

    #[test]
    fn notation_inside_html_ruby_is_still_decoded_around_it() {
        // Narou has chapters that mix both encodings in one line.
        assert_eq!(
            chapter_text("　今日は<ruby>漢字<rt>かんじ</rt></ruby>と｜単語《たんご》です。"),
            vec![
                text("　今日は"),
                Inline::Ruby {
                    base: "漢字".into(),
                    reading: "かんじ".into()
                },
                text("と"),
                Inline::Ruby {
                    base: "単語".into(),
                    reading: "たんご".into()
                },
                text("です。"),
            ]
        );
    }
}
