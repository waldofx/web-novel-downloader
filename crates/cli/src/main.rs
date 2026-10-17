//! `nvd` — the command-line front end.
//!
//! Two conventions make this scriptable, and both are deliberate:
//!
//! - **Progress goes to stderr, results to stdout.** `nvd download … >
//!   summary.txt` captures the summary rather than the meter.
//! - **Exit codes are meaningful**: `0` on success, `1` on any failure, and `2`
//!   for a usage error (which is clap's own convention).
//!
//! Errors are written as a sentence that names the thing that failed — the URL
//! that was being fetched, or the directory being read — because "error:
//! unexpected end of file" is not something anyone can act on.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use url::Url;
use wnd_core::{FetchPolicy, HttpClient, NovelDir};
use wnd_download::{DownloadOptions, Progress};
use wnd_export::{ExportOptions, ExportSummary, Format};

#[derive(Debug, Parser)]
#[command(
    name = "nvd",
    version,
    about = "Download web novels into a structured offline format",
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Fetch a novel and its chapters into a directory
    Download {
        /// URL of the novel's table of contents
        url: String,
        /// Directory to write into
        #[arg(long, short)]
        out: PathBuf,
        /// Also export the result. Repeatable or comma-separated: epub, txt, md
        #[arg(long, value_delimiter = ',')]
        format: Vec<FormatArg>,
        /// Allow adult works (otherwise refused once the index has been read)
        #[arg(long)]
        allow_adult: bool,
        /// Fetch at most this many chapters, counting from the first
        #[arg(long)]
        max_chapters: Option<usize>,
        /// Re-fetch every chapter, ignoring what is already on disk
        #[arg(long)]
        force: bool,
        /// Do not consult robots.txt. The escape hatch for a site whose file is
        /// wrong; not a licence to ignore a site's wishes
        #[arg(long)]
        no_robots: bool,
    },

    /// Fetch only new or changed chapters of an already-downloaded novel
    Update {
        /// Directory previously written by `nvd download`
        dir: PathBuf,
        /// Allow adult works
        #[arg(long)]
        allow_adult: bool,
    },

    /// Export a downloaded novel (EPUB, TXT, or Markdown)
    Export {
        /// Directory previously written by `nvd download`
        dir: PathBuf,
        /// Output format(s). Repeatable or comma-separated: epub, txt, md
        #[arg(long, value_delimiter = ',', default_value = "epub")]
        format: Vec<FormatArg>,
    },

    /// Show what a downloaded novel directory contains
    Info {
        /// Directory previously written by `nvd download`
        dir: PathBuf,
    },
}

/// The values `--format` accepts, named the way a user would type them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum FormatArg {
    /// EPUB 3, for an e-reader
    Epub,
    /// Plain text
    Txt,
    /// Markdown
    Md,
}

impl From<FormatArg> for Format {
    fn from(value: FormatArg) -> Self {
        match value {
            FormatArg::Epub => Format::Epub,
            FormatArg::Txt => Format::Txt,
            FormatArg::Md => Format::Markdown,
        }
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), String> {
    match cli.command {
        Command::Info { dir } => info(&dir),

        Command::Download {
            url,
            out,
            format,
            allow_adult,
            max_chapters,
            force,
            no_robots,
        } => download(
            &url,
            &out,
            &format,
            allow_adult,
            max_chapters,
            force,
            no_robots,
        ),

        Command::Export { dir, format } => export(&dir, &format),

        Command::Update {
            dir,
            allow_adult: _,
        } => {
            let dir = NovelDir::at(dir);
            // Reading the manifest is real: it proves the directory is valid, so
            // the answer for a typo is not "not implemented".
            dir.read_manifest().map_err(|error| named(&dir, &error))?;
            Err("update is not implemented yet (milestone M6)".to_string())
        }
    }
}

fn download(
    url: &str,
    out: &Path,
    formats: &[FormatArg],
    allow_adult: bool,
    max_chapters: Option<usize>,
    force: bool,
    no_robots: bool,
) -> Result<(), String> {
    let url = Url::parse(url).map_err(|error| format!("invalid URL `{url}`: {error}"))?;
    let policy = FetchPolicy {
        respect_robots: !no_robots,
        ..FetchPolicy::default()
    };
    if no_robots {
        // Said out loud, because it is the one flag that changes what we ask of
        // a site rather than what we do with the answer.
        eprintln!("warning: robots.txt is being ignored (--no-robots)");
    }
    let client = HttpClient::new(policy)
        .map_err(|error| format!("could not build the HTTP client: {error}"))?;
    let options = DownloadOptions {
        allow_adult,
        max_chapters,
        force,
    };

    let mut reporter = Reporter::default();
    let summary = wnd_download::download(
        &wnd_sources::registry(),
        &client,
        &url,
        out,
        &options,
        |event| reporter.report(&event),
    )
    // Name the URL: a failure several chapters in is otherwise hard to place.
    .map_err(|error| format!("could not download {url}: {error}"))?;

    println!("{}", summary.title);
    println!(
        "  chapters: {} fetched, {} skipped, {} total",
        summary.chapters_fetched, summary.chapters_skipped, summary.chapters_total
    );
    println!("  written:  {}", human_bytes(summary.bytes_written));
    println!("  output:   {}", summary.output_dir.display());

    for export in write_exports(&NovelDir::at(&summary.output_dir), formats)? {
        println!(
            "  exported: {} ({})",
            export.path.display(),
            human_bytes(export.bytes)
        );
    }

    Ok(())
}

fn export(dir: &Path, formats: &[FormatArg]) -> Result<(), String> {
    let novel = NovelDir::at(dir);
    // Check the directory before writing anything: "this is not a novel
    // directory" should not arrive after a half-finished export.
    novel
        .read_manifest()
        .map_err(|error| named(&novel, &error))?;

    for summary in write_exports(&novel, formats)? {
        println!(
            "{} ({}, {} chapters)",
            summary.path.display(),
            human_bytes(summary.bytes),
            summary.chapters
        );
    }
    Ok(())
}

fn write_exports(novel: &NovelDir, formats: &[FormatArg]) -> Result<Vec<ExportSummary>, String> {
    formats
        .iter()
        .map(|format| {
            wnd_export::export(novel, Format::from(*format), &ExportOptions::default())
                .map_err(|error| named(novel, &error))
        })
        .collect()
}

/// Summarize a novel directory. It reads the canonical format through
/// `wnd-core`, so it doubles as a check that a download is valid.
fn info(dir: &Path) -> Result<(), String> {
    let dir = NovelDir::at(dir);
    let manifest = dir.read_manifest().map_err(|error| named(&dir, &error))?;

    println!("{}", manifest.title);
    if let Some(author) = &manifest.author {
        println!("  author:   {author}");
    }
    println!(
        "  source:   {} ({})",
        manifest.source.id, manifest.source.novel_url
    );
    println!("  language: {}", manifest.language);
    println!("  rating:   {:?}", manifest.content_rating);
    if !manifest.tags.is_empty() {
        println!("  tags:     {}", manifest.tags.join(", "));
    }
    println!("  chapters: {}", manifest.chapters.len());

    let missing = manifest
        .chapters
        .iter()
        .filter(|c| !dir.has_chapter(c.index))
        .count();
    if missing > 0 {
        println!("  missing:  {missing} chapter file(s) not present yet");
    }
    Ok(())
}

/// Prefix an error with the directory it happened in.
fn named(dir: &NovelDir, error: &dyn std::fmt::Display) -> String {
    format!("{}: {error}", dir.root().display())
}

/// Prints the progress meter to stderr.
///
/// It counts for itself rather than trusting the index, because what a reader
/// wants to see is where they are in *this* run — which, on a resume, is not the
/// same as the chapter number.
#[derive(Default)]
struct Reporter {
    total: usize,
    seen: usize,
}

impl Reporter {
    fn report(&mut self, event: &Progress) {
        match event {
            Progress::Planned {
                title,
                chapters,
                to_fetch,
            } => {
                self.total = *chapters;
                eprintln!("{title}: {chapters} chapters, {to_fetch} to fetch");
            }
            Progress::Fetched { title, bytes, .. } => {
                self.seen += 1;
                eprintln!(
                    "  {} fetch  {title} ({})",
                    self.counter(),
                    human_bytes(*bytes)
                );
            }
            Progress::Skipped { title, .. } => {
                self.seen += 1;
                eprintln!("  {} skip   {title}", self.counter());
            }
        }
    }

    fn counter(&self) -> String {
        let width = self.total.to_string().len();
        format!("[{:>width$}/{}]", self.seen, self.total)
    }
}

/// A byte count a person can read.
fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];

    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_counts_are_readable() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(999), "999 B");
        assert_eq!(human_bytes(1024), "1.0 KiB");
        assert_eq!(human_bytes(1536), "1.5 KiB");
        assert_eq!(human_bytes(5 * 1024 * 1024), "5.0 MiB");
    }

    #[test]
    fn the_meter_counts_from_one() {
        let mut reporter = Reporter::default();
        reporter.report(&Progress::Planned {
            title: "t".to_string(),
            chapters: 12,
            to_fetch: 12,
        });
        reporter.report(&Progress::Skipped {
            index: 1,
            title: "one".to_string(),
        });

        // Padded to the width of the total, so the column does not jump at 10.
        assert_eq!(reporter.counter(), "[ 1/12]");
    }
}
