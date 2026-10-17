//! Exporters: the canonical JSON, re-rendered for humans and e-readers.
//!
//! Every format here is a **projection** of `chapters/*.json`. They never touch
//! the network and never invent content, so `exports/` can always be deleted and
//! rebuilt. The JSON stays the source of truth.
//!
//! Determinism is a requirement, not a nicety: the same directory must produce
//! byte-identical output, so re-exports diff cleanly and a build is reproducible.
//! That means no clock is consulted — the EPUB's modification date comes from the
//! manifest, and the ZIP entries carry a fixed timestamp.

use std::fs;
use std::path::{Path, PathBuf};

use wnd_core::{Chapter, Manifest, NovelDir};

mod epub;
mod markdown;
mod plain;
mod txt;
mod words;
mod xhtml;

/// A format an export can take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// EPUB 3: ruby as real `<ruby>`, one XHTML file per chapter.
    Epub,
    /// Plain text: ruby falls back to parentheses.
    Txt,
    /// Markdown: headings and emphasis as markup, ruby as plain text.
    Markdown,
}

impl Format {
    /// File extension, without the dot.
    pub fn extension(self) -> &'static str {
        match self {
            Format::Epub => "epub",
            Format::Txt => "txt",
            Format::Markdown => "md",
        }
    }
}

/// How furigana is written where the format has no ruby.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RubyStyle {
    /// `漢字(かんじ)` — keeps the reading, and reads naturally as prose.
    #[default]
    Parenthesised,
    /// `漢字` — drops the reading, for readers who do not want it.
    BaseOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportOptions {
    /// How to write furigana in the formats that cannot express it. The EPUB has
    /// real ruby, so it ignores this.
    pub ruby: RubyStyle,
    /// Include the author's 前書き and 後書き notes.
    pub include_notes: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        // Notes are the author's own words about the chapter, so they are part
        // of the work by default rather than an opt-in extra.
        Self {
            ruby: RubyStyle::default(),
            include_notes: true,
        }
    }
}

/// What an export produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportSummary {
    pub format: Format,
    /// The file that was written, inside the novel's `exports/` directory.
    pub path: PathBuf,
    pub chapters: usize,
    pub bytes: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error(transparent)]
    Core(#[from] wnd_core::Error),

    #[error("archive error: {0}")]
    Archive(#[from] zip::result::ZipError),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("nothing to export: {0} has no chapters")]
    Empty(PathBuf),
}

/// Render a downloaded novel into `dir`'s `exports/` directory.
///
/// The whole novel is read before anything is written, so a broken chapter fails
/// the export instead of producing a half-exported file that looks complete.
pub fn export(
    dir: &NovelDir,
    format: Format,
    options: &ExportOptions,
) -> Result<ExportSummary, ExportError> {
    let manifest = dir.read_manifest()?;
    if manifest.chapters.is_empty() {
        return Err(ExportError::Empty(dir.root().to_path_buf()));
    }

    let chapters = manifest
        .chapters
        .iter()
        .map(|meta| dir.read_chapter(meta.index))
        .collect::<Result<Vec<Chapter>, _>>()?;

    fs::create_dir_all(dir.exports_dir())?;
    let path = dir
        .exports_dir()
        .join(format!("{}.{}", file_stem(&manifest), format.extension()));

    let bytes = match format {
        Format::Txt => write_file(&path, &txt::render(&manifest, &chapters, options))?,
        Format::Markdown => write_file(&path, &markdown::render(&manifest, &chapters, options))?,
        Format::Epub => epub::write(dir, &manifest, &chapters, options, &path)?,
    };

    Ok(ExportSummary {
        format,
        path,
        chapters: chapters.len(),
        bytes,
    })
}

fn write_file(path: &Path, text: &str) -> Result<u64, ExportError> {
    fs::write(path, text)?;
    Ok(text.len() as u64)
}

/// A file name derived from the title.
///
/// The title is what a human recognises in a file manager, so it beats a URL or
/// an index — but a title is free text and a file name is not. The characters
/// Windows forbids anywhere in a name are replaced, and the ones it forbids at
/// the end of a name are trimmed. Everything else, including the ideographic
/// space a Japanese title may contain, is left exactly as written.
///
/// This is a pure function of the manifest, which is what keeps re-exports from
/// scattering renamed files around the directory.
fn file_stem(manifest: &Manifest) -> String {
    /// Long enough for a real light-novel title, short enough for a file system.
    const MAX_CHARS: usize = 80;

    let sanitised: String = manifest
        .title
        .trim()
        .chars()
        .map(|character| match character {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            character if character.is_control() => '_',
            character => character,
        })
        .collect();

    let trimmed = sanitised.trim().trim_end_matches(['.', ' ']);
    let capped: String = trimmed.chars().take(MAX_CHARS).collect();
    let capped = capped.trim_end_matches(['.', ' ']);

    if capped.is_empty() {
        // Nothing usable survived — a title of punctuation, say. A file still has
        // to have a name, and the adapter id is at least honest about the source.
        manifest.source.id.clone()
    } else {
        capped.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wnd_core::SourceInfo;

    fn manifest_titled(title: &str) -> Manifest {
        Manifest::new(
            SourceInfo {
                id: "fake".to_string(),
                novel_url: "https://fake.test/novel/".to_string(),
                site_title: None,
            },
            title,
            "ja",
        )
    }

    #[test]
    fn a_very_long_title_is_capped() {
        let stem = file_stem(&manifest_titled(&"あ".repeat(200)));

        assert_eq!(stem.chars().count(), 80);
    }

    #[test]
    fn a_title_ending_in_a_dot_or_a_space_loses_it() {
        // Windows silently strips these, which would make the file that gets
        // written differ from the path we reported.
        assert_eq!(file_stem(&manifest_titled("題名.")), "題名");
        assert_eq!(file_stem(&manifest_titled("題名  ")), "題名");
    }

    #[test]
    fn an_ideographic_space_inside_a_title_survives() {
        assert_eq!(file_stem(&manifest_titled("題名　二")), "題名　二");
    }
}
