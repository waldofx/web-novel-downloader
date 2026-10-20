//! Plain text: the novel as something you could paste into an email.
//!
//! There is no markup to use, so structure is carried by whitespace alone: two
//! blank lines between chapters, one between a heading and its text, and a
//! marked line for a scene break.

use std::fmt::Write as _;

use wnd_core::{Block, Chapter, Manifest};

use crate::ExportOptions;
use crate::plain;
use crate::words::Words;

/// Render the whole novel as one text document, in reading order.
pub fn render(manifest: &Manifest, chapters: &[Chapter], options: &ExportOptions) -> String {
    let words = Words::for_language(&manifest.language);
    let mut out = String::new();

    let _ = writeln!(out, "{}", manifest.title);
    if let Some(author) = &manifest.author {
        let _ = writeln!(out, "{author}");
    }
    if !manifest.synopsis.is_empty() {
        out.push('\n');
        blocks(&mut out, &manifest.synopsis, options, &words);
    }

    let mut arc: Option<&str> = None;
    for chapter in chapters {
        out.push_str("\n\n");

        // An arc heading belongs above the first chapter of the arc, and only
        // there — repeating it on every chapter would be noise in a book.
        let chapter_arc = manifest
            .chapter(chapter.index)
            .and_then(|meta| meta.arc.as_deref());
        if chapter_arc != arc {
            if let Some(arc) = chapter_arc {
                let _ = writeln!(out, "【{arc}】\n");
            }
            arc = chapter_arc;
        }

        let _ = writeln!(out, "{}", chapter.title);
        out.push('\n');

        if options.include_notes && !chapter.pre_note.is_empty() {
            let _ = writeln!(out, "［{}］", words.preface);
            blocks(&mut out, &chapter.pre_note, options, &words);
            out.push('\n');
        }

        blocks(&mut out, &chapter.nodes, options, &words);

        if options.include_notes && !chapter.post_note.is_empty() {
            out.push('\n');
            let _ = writeln!(out, "［{}］", words.afterword);
            blocks(&mut out, &chapter.post_note, options, &words);
        }
    }

    // A text file ends with exactly one newline, however many the layout left.
    let trimmed = out.trim_end();
    format!("{trimmed}\n")
}

/// Append the rendered blocks.
///
/// A paragraph is one line. A scene break needs blank lines around it, because
/// a marker glued to the prose above it does not read as a break.
fn blocks(out: &mut String, blocks: &[Block], options: &ExportOptions, words: &Words) {
    for block in blocks {
        match block {
            Block::Paragraph { children } | Block::Heading { children, .. } => {
                let _ = writeln!(out, "{}", plain::inlines(children, options.ruby));
            }
            Block::SceneBreak => {
                let _ = writeln!(out, "\n{}\n", words.scene_break);
            }
            Block::HorizontalRule => {
                let _ = writeln!(out, "\n――――――――\n");
            }
            // The image itself cannot be shown, but its existence and its
            // description can be, so a reader is not silently misled.
            Block::Image { src, alt } => {
                let _ = writeln!(out, "[画像: {}]", alt.as_deref().unwrap_or(src));
            }
        }
    }
}
