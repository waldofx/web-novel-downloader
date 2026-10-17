//! `nvd` — the command-line front end.

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;
use url::Url;
use wnd_core::NovelDir;

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
    /// Fetch a novel (and its chapters) into a directory
    Download {
        /// URL of the novel's table of contents
        url: String,
        /// Directory to write into
        #[arg(long, short)]
        out: PathBuf,
        /// Allow adult sources and works
        #[arg(long)]
        allow_adult: bool,
    },

    /// Fetch only new or changed chapters of an already-downloaded novel
    Update {
        /// Directory previously written by `nvd download`
        dir: PathBuf,
        /// Allow adult sources and works
        #[arg(long)]
        allow_adult: bool,
    },

    /// Export a downloaded novel (EPUB, TXT, or Markdown)
    Export {
        /// Directory previously written by `nvd download`
        dir: PathBuf,
        /// Output format
        #[arg(long, default_value = "epub")]
        format: String,
    },

    /// Show what a downloaded novel directory contains
    Info {
        /// Directory previously written by `nvd download`
        dir: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
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
            allow_adult,
        } => {
            let url = Url::parse(&url).map_err(|e| format!("invalid URL: {e}"))?;
            let sources = wnd_sources::registry();
            let source = wnd_sources::find(&sources, &url)
                .ok_or_else(|| format!("no adapter knows how to handle {url}"))?;

            if source.content_rating() == wnd_core::ContentRating::Adult && !allow_adult {
                return Err(format!(
                    "{} is an adult source; re-run with --allow-adult if that is intended",
                    source.id()
                ));
            }

            Err(format!(
                "download into `{}` is not implemented yet (adapter `{}` is wired up; fetching lands in milestone M2)",
                out.display(),
                source.id()
            ))
        }

        Command::Update {
            dir,
            allow_adult: _,
        } => {
            let dir = NovelDir::at(dir);
            // Reading the manifest is real: it proves the directory is valid.
            dir.read_manifest()
                .map_err(|e| format!("{}: {e}", dir.root().display()))?;
            Err("update is not implemented yet (milestone M6)".to_string())
        }

        Command::Export { dir, format } => {
            let dir = NovelDir::at(dir);
            dir.read_manifest()
                .map_err(|e| format!("{}: {e}", dir.root().display()))?;
            Err(format!(
                "export to `{format}` is not implemented yet (milestone M2)"
            ))
        }
    }
}

/// Summarize a novel directory. Implemented for real: it reads the canonical
/// format through `wnd-core`, so it doubles as a check that a download is valid.
fn info(dir: &PathBuf) -> Result<(), String> {
    let dir = NovelDir::at(dir);
    let manifest = dir
        .read_manifest()
        .map_err(|e| format!("{}: {e}", dir.root().display()))?;

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
