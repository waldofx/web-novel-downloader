//! Markdown: the novel as a document, with the structure the format *can*
//! express written as Markdown.
//!
//! Furigana is the interesting case. Markdown has no ruby, so it degrades to the
//! same `漢字(かんじ)` the text export uses — which reads correctly in a terminal,
//! in a diff on GitHub, and in a rendered page. Emphasis, strong text, links and
//! images have real Markdown, so they use it.

use std::fmt::Write as _;

use wnd_core::{Block, Chapter, Inline, Manifest};

use crate::ExportOptions;
use crate::plain;
use crate::words::Words;

/// Render the whole novel as one Markdown document.
pub fn render(manifest: &Manifest, chapters: &[Chapter], options: &ExportOptions) -> String {
    let words = Words::for_language(&manifest.language);
    let mut out = String::new();

    let _ = writeln!(out, "# {}", escape(&manifest.title));
    if let Some(author) = &manifest.author {
        let _ = writeln!(out, "\n*{}*", escape(author));
    }
    if let Some(synopsis) = &manifest.synopsis {
        // A synopsis arrives with its line structure, and a blank line in it is
        // a paragraph break — so the lines are written as they are, rather than
        // each being given its own separating blank line.
        let _ = writeln!(out);
        for line in synopsis.lines() {
            let _ = writeln!(out, "{}", guard(&escape(line)));
        }
    }

    let mut arc: Option<&str> = None;
    for chapter in chapters {
        out.push('\n');

        let chapter_arc = manifest
            .chapter(chapter.index)
            .and_then(|meta| meta.arc.as_deref());
        if chapter_arc != arc {
            if let Some(arc) = chapter_arc {
                let _ = writeln!(out, "\n## {}", escape(arc));
            }
            arc = chapter_arc;
        }

        let _ = writeln!(out, "\n### {}", escape(&chapter.title));

        if options.include_notes && !chapter.pre_note.is_empty() {
            let body = blocks(&chapter.pre_note, options);
            let _ = write!(out, "\n{}", quote(words.preface, &body));
        }

        let _ = write!(out, "\n{}", blocks(&chapter.nodes, options));

        if options.include_notes && !chapter.post_note.is_empty() {
            let body = blocks(&chapter.post_note, options);
            let _ = write!(out, "\n{}", quote(words.afterword, &body));
        }
    }

    let trimmed = out.trim_end();
    format!("{trimmed}\n")
}

/// Render blocks, each followed by a blank line so they stay separate blocks.
fn blocks(blocks: &[Block], options: &ExportOptions) -> String {
    let mut out = String::new();
    for block in blocks {
        match block {
            Block::Paragraph { children } => {
                let _ = writeln!(out, "{}\n", guard(&inlines(children, options)));
            }
            Block::Heading { level, children } => {
                let level = (*level).clamp(1, 6);
                let _ = writeln!(
                    out,
                    "{} {}\n",
                    "#".repeat(level as usize),
                    inlines(children, options)
                );
            }
            Block::SceneBreak => out.push_str("* * *\n\n"),
            Block::HorizontalRule => out.push_str("---\n\n"),
            // The file lives in exports/, but the model's `src` is relative to
            // the novel directory, so the path has to be rewritten.
            Block::Image { src, alt } => {
                let _ = writeln!(
                    out,
                    "![{}](../{})\n",
                    escape(alt.as_deref().unwrap_or("")),
                    escape(src)
                );
            }
        }
    }
    out
}

/// An author's note as a blockquote, which is how a renderer sets it apart.
fn quote(label: &str, body: &str) -> String {
    let mut out = format!("> **{}**\n>\n", escape(label));
    for line in body.trim_end().lines() {
        if line.is_empty() {
            out.push_str(">\n");
        } else {
            let _ = writeln!(out, "> {line}");
        }
    }
    out
}

fn inlines(nodes: &[Inline], options: &ExportOptions) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            Inline::Text { value } => out.push_str(&escape(value)),
            Inline::Ruby { base, reading } => {
                out.push_str(&escape(&plain::ruby(options.ruby, base, reading)));
            }
            Inline::Emphasis { children } => {
                let _ = write!(out, "*{}*", inlines(children, options));
            }
            Inline::Strong { children } => {
                let _ = write!(out, "**{}**", inlines(children, options));
            }
            Inline::Link { href, children } => {
                let _ = write!(out, "[{}]({})", inlines(children, options), href);
            }
        }
    }
    out
}

/// Escape the characters that would otherwise be read as markup.
///
/// Deliberately short: this is prose, and backslash-escaping Japanese punctuation
/// would be worse than useless. These are the characters that actually change how
/// a paragraph renders.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        if matches!(character, '\\' | '*' | '_' | '`' | '[' | ']') {
            out.push('\\');
        }
        out.push(character);
    }
    out
}

/// A paragraph that opens with `#` or `>` would become a heading or a quote.
fn guard(line: &str) -> String {
    if line.starts_with('#') || line.starts_with('>') {
        format!("\\{line}")
    } else {
        line.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escaping_leaves_japanese_punctuation_alone() {
        assert_eq!(escape("「これは」、そのまま。"), "「これは」、そのまま。");
        assert_eq!(escape("a_b*c[d]"), r"a\_b\*c\[d\]");
    }

    #[test]
    fn a_line_that_would_be_a_heading_is_defused() {
        assert_eq!(guard("# not a heading"), r"\# not a heading");
        assert_eq!(guard("> not a quote"), r"\> not a quote");
        assert_eq!(guard("ふつうの行"), "ふつうの行");
    }
}
