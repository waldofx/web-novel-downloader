//! `nvd` — the command-line front end.
//!
//! Two conventions make this scriptable, and both are deliberate:
//!
//! - **Progress goes to stderr, results to stdout.** `nvd download … >
//!   summary.txt` captures the summary rather than the meter.
//! - **Exit codes are meaningful**: `0` on success, `1` on any failure, and `2`
//!   for a usage error (which is clap's own convention). `nvd update` adds `3`
//!   for "there was nothing to do", so a scheduled run can tell that apart from
//!   a failure without reading anything.
//!
//! Errors are written as a sentence that names the thing that failed — the URL
//! that was being fetched, or the directory being read — because "error:
//! unexpected end of file" is not something anyone can act on.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use url::Url;
use wnd_core::session::netscape;
use wnd_core::session::{Session, SessionStore};
use wnd_core::{FetchPolicy, HttpClient, NovelDir};
use wnd_download::{DownloadOptions, Progress, UpdateOptions, UpdateSummary};
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

    /// Where sessions live. Defaults to the user's own data directory.
    #[arg(long, global = true)]
    session_dir: Option<PathBuf>,
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
        /// Use a saved session (by name) for the hosts it names
        #[arg(long)]
        session: Option<String>,
    },

    /// Fetch only new or changed chapters of an already-downloaded novel
    ///
    /// The novel comes from the directory's own manifest, so no URL is needed.
    /// A chapter that is new, or that the site says was revised, is fetched;
    /// one that disappeared from the index is reported and left on disk; the
    /// rest are left alone.
    ///
    /// Exits 3 when there was nothing to do, so a scheduled run can tell
    /// "nothing new" apart from "it broke" (1) without reading the output. Exit
    /// 0 means something was fetched, or a chapter had gone from the index.
    Update {
        /// Directory previously written by `nvd download`
        dir: PathBuf,
        /// Allow adult works. An adult directory is refused before anything is
        /// requested, because the rating is already recorded in its manifest
        #[arg(long)]
        allow_adult: bool,
        /// Fetch every chapter and compare it, rewriting only the ones that
        /// really differ. Costs one request per chapter, and is the only way to
        /// find an edit on a site that publishes no revision (Kakuyomu)
        #[arg(long)]
        check: bool,
        /// Use a saved session (by name) for the hosts it names
        #[arg(long)]
        session: Option<String>,
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

    /// Save, list or forget a session the user handed over
    Session {
        #[command(subcommand)]
        action: SessionAction,
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

#[derive(Debug, Subcommand)]
enum SessionAction {
    /// Import a browser's cookie export under a name
    Save {
        /// Name to give the session (e.g. "pixiv")
        name: String,
        /// A Netscape `cookies.txt` file, as any browser export extension writes
        file: PathBuf,
        /// Restrict the session to these hosts instead of the file's own domains
        #[arg(long)]
        host: Vec<String>,
    },
    /// List the sessions that are saved
    List,
    /// Forget a saved session
    Delete {
        /// Name of the session to remove
        name: String,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => ExitCode::from(code),
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

/// Nothing went wrong, and the run did what it was asked.
const EXIT_OK: u8 = 0;
/// `nvd update` found nothing new or changed. Distinct from `0` so a scheduled
/// run can tell "nothing to do" from "something happened" without parsing, and
/// from `1` so it can tell both from a failure.
const EXIT_NOTHING_UPDATED: u8 = 3;

fn run(cli: Cli) -> Result<u8, String> {
    let session_dir = cli.session_dir.clone();
    match cli.command {
        Command::Info { dir } => info(&dir).map(|()| EXIT_OK),

        Command::Download {
            url,
            out,
            format,
            allow_adult,
            max_chapters,
            force,
            no_robots,
            session,
        } => {
            let session = load_session(session.as_deref(), session_dir.as_deref())?;
            download(
                &url,
                &out,
                &format,
                allow_adult,
                max_chapters,
                force,
                no_robots,
                session,
            )
            .map(|()| EXIT_OK)
        }

        Command::Export { dir, format } => export(&dir, &format).map(|()| EXIT_OK),

        Command::Update {
            dir,
            allow_adult,
            check,
            session,
        } => {
            let session = load_session(session.as_deref(), session_dir.as_deref())?;
            update(&dir, allow_adult, check, session)
        }

        Command::Session { action } => session_cmd(action, session_dir.as_deref()),
    }
}

#[allow(clippy::too_many_arguments)]
fn download(
    url: &str,
    out: &Path,
    formats: &[FormatArg],
    allow_adult: bool,
    max_chapters: Option<usize>,
    force: bool,
    no_robots: bool,
    session: Option<Session>,
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
    let mut client = HttpClient::new(policy)
        .map_err(|error| format!("could not build the HTTP client: {error}"))?;
    if let Some(session) = session {
        client = client.with_session(session);
    }
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

fn update(
    dir: &Path,
    allow_adult: bool,
    check: bool,
    session: Option<Session>,
) -> Result<u8, String> {
    let mut client = HttpClient::new(FetchPolicy::default())
        .map_err(|error| format!("could not build the HTTP client: {error}"))?;
    if let Some(session) = session {
        client = client.with_session(session);
    }
    let options = UpdateOptions { allow_adult, check };

    let mut reporter = Reporter::default();
    let summary = wnd_download::update(&wnd_sources::registry(), &client, dir, &options, |event| {
        reporter.report(&event)
    })
    // Name the directory: an update is a thing done to a directory, and "which
    // one" is the question a failure leaves open.
    .map_err(|error| format!("could not update {}: {error}", dir.display()))?;

    println!("{}", summary.title);
    if summary.up_to_date() {
        println!(
            "  up to date: {} chapters, nothing new or changed",
            summary.chapters_total
        );
    } else {
        println!(
            "  chapters: {} fetched, {} skipped, {} total",
            summary.chapters_fetched, summary.chapters_skipped, summary.chapters_total
        );
    }
    if summary.chapters_checked > 0 {
        println!(
            "  checked:  {} compared, {} changed",
            summary.chapters_checked, summary.chapters_fetched
        );
    }
    println!("  written:  {}", human_bytes(summary.bytes_written));
    for gone in &summary.removed {
        // Said out loud, and never acted on: deleting a file the user already has
        // is not this tool's business.
        println!(
            "  gone:     {} (chapter {} is no longer in the index; nothing was deleted)",
            gone.title, gone.index
        );
    }
    println!("  output:   {}", summary.output_dir.display());

    Ok(update_exit_code(&summary))
}

/// The exit code an update's outcome deserves.
fn update_exit_code(summary: &UpdateSummary) -> u8 {
    if summary.up_to_date() {
        EXIT_NOTHING_UPDATED
    } else {
        EXIT_OK
    }
}

fn session_cmd(action: SessionAction, session_dir: Option<&Path>) -> Result<u8, String> {
    let dir = resolve_session_dir(session_dir)?;
    let store = SessionStore::at(dir);

    match action {
        SessionAction::Save { name, file, host } => {
            let text = std::fs::read_to_string(&file)
                .map_err(|error| format!("could not read {}: {error}", file.display()))?;
            let cookies = netscape::parse(&text)
                .map_err(|error| format!("could not read {}: {error}", file.display()))?;
            if cookies.is_empty() {
                return Err(format!("{} holds no cookies", file.display()));
            }
            let hosts = if host.is_empty() {
                wnd_core::session::hosts_of(&cookies)
            } else {
                host
            };
            let session = Session::new(name, hosts, cookies);
            let path = store.save(&session).map_err(|error| error.to_string())?;
            println!(
                "saved session `{}` ({} cookies) to {}",
                session.name(),
                session.cookies().len(),
                path.display()
            );
            Ok(EXIT_OK)
        }
        SessionAction::List => {
            let sessions = store.list().map_err(|error| error.to_string())?;
            if sessions.is_empty() {
                println!("no sessions saved");
                return Ok(EXIT_OK);
            }
            for summary in sessions {
                println!(
                    "{}  hosts: {}  cookies: {}  saved: {}  expiry: {}",
                    summary.name,
                    if summary.hosts.is_empty() {
                        "(none)".to_string()
                    } else {
                        summary.hosts.join(", ")
                    },
                    summary.cookies,
                    human_age(now_secs().saturating_sub(summary.saved_at)),
                    expiry_text(summary.expires_at)
                );
            }
            Ok(EXIT_OK)
        }
        SessionAction::Delete { name } => {
            store.delete(&name).map_err(|error| error.to_string())?;
            println!("forgot session `{name}`");
            Ok(EXIT_OK)
        }
    }
}

fn load_session(name: Option<&str>, session_dir: Option<&Path>) -> Result<Option<Session>, String> {
    let Some(name) = name else {
        return Ok(None);
    };
    let dir = resolve_session_dir(session_dir)?;
    let store = SessionStore::at(dir);
    let session = store.load(name).map_err(|error| error.to_string())?;
    Ok(Some(session))
}

fn resolve_session_dir(session_dir: Option<&Path>) -> Result<PathBuf, String> {
    match session_dir {
        Some(dir) => Ok(dir.to_path_buf()),
        None => wnd_core::session::default_dir().map_err(|error| error.to_string()),
    }
}

fn expiry_text(expires_at: Option<u64>) -> String {
    match expires_at {
        None => "no expiry".to_string(),
        Some(at) => {
            let now = now_secs();
            if at > now {
                format!("expires in {}", human_duration(at - now))
            } else {
                format!("expired {}", human_duration(now - at))
            }
        }
    }
}

fn human_age(seconds: u64) -> String {
    if seconds < 60 {
        "just now".to_string()
    } else if seconds < 3600 {
        format!("{} minutes ago", seconds / 60)
    } else if seconds < 86400 {
        format!("{} hours ago", seconds / 3600)
    } else {
        format!("{} days ago", seconds / 86400)
    }
}

fn human_duration(seconds: u64) -> String {
    if seconds < 60 {
        format!("{seconds} seconds")
    } else if seconds < 3600 {
        format!("{} minutes", seconds / 60)
    } else if seconds < 86400 {
        format!("{} hours", seconds / 3600)
    } else {
        format!("{} days", seconds / 86400)
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0)
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
            Progress::Checked { title, .. } => {
                // Fetched and compared, and nothing was written: `update --check`.
                self.seen += 1;
                eprintln!("  {} check  {title}", self.counter());
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
    fn an_update_that_found_nothing_exits_three() {
        // A scheduled run must be able to tell "nothing new" from "it broke"
        // (1), and both from "something happened" (0).
        let nothing = UpdateSummary {
            title: "小説".to_string(),
            chapters_total: 3,
            chapters_fetched: 0,
            chapters_skipped: 3,
            chapters_checked: 0,
            removed: Vec::new(),
            bytes_written: 0,
            output_dir: PathBuf::from("novel"),
        };
        assert_eq!(update_exit_code(&nothing), 3);

        let fetched = UpdateSummary {
            chapters_fetched: 1,
            chapters_skipped: 2,
            ..nothing.clone()
        };
        assert_eq!(update_exit_code(&fetched), 0);

        // A chapter that vanished from the index is news too, even though
        // nothing needed fetching: the run is not "up to date".
        let gone = UpdateSummary {
            removed: vec![wnd_download::RemovedChapter {
                index: 4,
                title: "第四章".to_string(),
            }],
            ..nothing.clone()
        };
        assert_eq!(update_exit_code(&gone), 0);
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
