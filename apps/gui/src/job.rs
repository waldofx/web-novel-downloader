//! What the window does, with no window in it.
//!
//! Every function here takes ordinary values and returns ordinary values: the
//! Tauri layer next door starts them on a thread and turns their events into
//! something a webview can read. That split is the point — a GUI that can only
//! be exercised by clicking it is a GUI nobody can test, and none of this is
//! GUI-shaped: it is the same `wnd-download` and `wnd-export` the command line
//! calls, with the answers the front end needs.
//!
//! The types are serializable because that is the wire between here and
//! JavaScript, and nothing else: **no crate below this one learns that a GUI
//! exists.**

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use url::Url;
use wnd_core::session::cookie::Cookie;
use wnd_core::session::{Session, SessionStore, default_dir as default_session_dir, netscape};
use wnd_core::store::model::ContentRating;
use wnd_core::{FetchPolicy, HttpClient, NovelDir};
use wnd_download::{DownloadOptions, Progress, UpdateOptions};
use wnd_export::{ExportOptions, Format, RubyStyle};
use wnd_sources::Source;

/// Export formats the window offers, in the order it offers them.
pub const FORMATS: [&str; 3] = ["epub", "txt", "md"];

/// The export and session choices both buttons share.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// A saved session to send to the hosts it names, if any.
    #[serde(default)]
    pub session: Option<String>,
    /// Formats to write after fetching. Empty means "only fetch".
    #[serde(default)]
    pub export: Vec<String>,
    /// How furigana is written where the format has no ruby: `parenthesised`
    /// or `base_only`. Absent means the default.
    #[serde(default)]
    pub ruby: Option<String>,
    /// Include the author's foreword and afterword.
    #[serde(default = "yes")]
    pub notes: bool,
}

fn yes() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            session: None,
            export: Vec::new(),
            ruby: None,
            notes: true,
        }
    }
}

/// A novel URL, where to put it, and what to do with it afterwards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadRequest {
    pub url: String,
    pub output: String,
    #[serde(default)]
    pub max_chapters: Option<usize>,
    #[serde(default)]
    pub force: bool,
    #[serde(flatten)]
    pub settings: Settings,
}

/// A directory to bring up to date, and what to do with it afterwards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRequest {
    pub dir: String,
    /// Fetch every chapter and compare, instead of trusting the index.
    #[serde(default)]
    pub check: bool,
    #[serde(flatten)]
    pub settings: Settings,
}

/// A directory to export, with no fetching at all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRequest {
    pub dir: String,
    #[serde(flatten)]
    pub settings: Settings,
}

/// Something happened. One shape for every kind, tagged so the front end can
/// switch on it.
///
/// This is `Progress` with the parts a reader wants and the wire spelled out:
/// indices and titles travel together, because a list that says "chapter 43"
/// alone is not something a person can read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
pub enum Event {
    /// The table of contents is read and the run knows what it has to do.
    Planned {
        title: String,
        chapters: usize,
        to_fetch: usize,
    },
    Fetched {
        index: u32,
        title: String,
        bytes: u64,
    },
    Skipped {
        index: u32,
        title: String,
    },
    Checked {
        index: u32,
        title: String,
    },
    /// An image was fetched into `images/`. The file name says which.
    ImageFetched {
        file: String,
        bytes: u64,
    },
    /// An image could not be fetched. Its chapter kept the URL.
    ImageMissing {
        url: String,
        reason: String,
    },
    /// A chapter the directory has that the index no longer lists. Never
    /// deleted — said out loud, and left alone.
    Removed {
        index: u32,
        title: String,
    },
    Exported {
        format: String,
        path: String,
        bytes: u64,
    },
    /// The run finished. Everything else is a step on the way here.
    Done {
        report: Report,
    },
    /// The run stopped. Nothing after this.
    Failed {
        message: String,
    },
}

impl Event {
    /// The event `Progress` means, or `None` for one the window has no use for.
    ///
    /// `Progress::ImageMissing` is worth showing: a picture that did not come
    /// down is exactly the kind of thing a reader would otherwise discover as a
    /// gap in the book.
    pub fn from_progress(progress: Progress) -> Event {
        match progress {
            Progress::Planned {
                title,
                chapters,
                to_fetch,
            } => Event::Planned {
                title,
                chapters,
                to_fetch,
            },
            Progress::Fetched {
                index,
                title,
                bytes,
            } => Event::Fetched {
                index,
                title,
                bytes,
            },
            Progress::Skipped { index, title } => Event::Skipped { index, title },
            Progress::Checked { index, title } => Event::Checked { index, title },
            Progress::ImageFetched { file, bytes } => Event::ImageFetched { file, bytes },
            Progress::ImageMissing { url, reason } => Event::ImageMissing { url, reason },
        }
    }
}

/// A file an export wrote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportedFile {
    pub format: String,
    pub path: String,
    pub bytes: u64,
    pub chapters: usize,
}

/// A chapter the index has dropped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Gone {
    pub index: u32,
    pub title: String,
}

/// What a run did, in the numbers a window would show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub title: String,
    pub author: Option<String>,
    /// Read back from the manifest rather than carried through the run: the
    /// rating is a property of the work, and `None` here would mean the
    /// manifest could not be read after a run that just wrote it.
    pub rating: Option<ContentRating>,
    pub chapters_total: usize,
    pub chapters_fetched: usize,
    pub chapters_skipped: usize,
    pub chapters_checked: usize,
    pub images_fetched: usize,
    pub images_missing: usize,
    pub bytes_written: u64,
    pub output_dir: String,
    pub exported: Vec<ExportedFile>,
    pub removed: Vec<Gone>,
    /// Nothing new, nothing changed, nothing gone. An update that says so is
    /// the good case, not the empty one.
    pub up_to_date: bool,
}

/// What a directory holds, without touching the network.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NovelInfo {
    pub title: String,
    pub author: Option<String>,
    pub rating: ContentRating,
    pub source: String,
    pub novel_url: String,
    pub language: String,
    pub tags: Vec<String>,
    pub chapters: usize,
    pub images: usize,
    /// Chapter files the manifest lists that are not on disk.
    pub chapters_missing: usize,
    /// Image files the manifest lists that are not on disk (`nvd update` fetches
    /// them back).
    pub images_missing: usize,
    pub output_dir: String,
}

/// A session the user has saved, as the window needs to show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub name: String,
    pub hosts: Vec<String>,
    pub cookies: usize,
    /// Unix seconds.
    pub saved_at: u64,
    /// Unix seconds, or `None` when nothing in the session expires.
    pub expires_at: Option<u64>,
}

/// One work in a library folder, as the window lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryItem {
    pub title: String,
    pub author: Option<String>,
    pub chapters: usize,
    pub images: usize,
    /// `None` when the manifest could not be read.
    pub rating: Option<ContentRating>,
    /// RFC 3339 from the manifest, when it says.
    pub fetched_at: Option<String>,
    pub output_dir: String,
    /// The root itself is never removable: it is the library.
    pub removable: bool,
    /// Why the manifest could not be read; the row is still shown.
    pub problem: Option<String>,
}

/// A request that has been checked over, with the strings turned into types.
///
/// Both entry points below start here, so an unsupported URL or a directory with
/// nothing in it is refused *before* a thread is started and the window has to
/// deal with a run that was never going to work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prepared {
    pub url: Option<Url>,
    pub output: PathBuf,
    pub session: Option<String>,
    pub export: Vec<Format>,
    pub export_options: ExportOptions,
    pub download: DownloadOptions,
    pub check: bool,
}

/// The policy the app fetches under.
///
/// Deliberately not configurable, and deliberately not `--no-robots`: a window
/// with a checkbox that asks a site to be treated worse is a worse window, and
/// nothing here needs it.
pub fn policy() -> FetchPolicy {
    FetchPolicy::default()
}

/// A client for a session, or for nobody.
pub fn client(session: Option<Session>) -> Result<HttpClient, String> {
    let mut client = HttpClient::new(policy())
        .map_err(|error| format!("could not build the HTTP client: {error}"))?;
    if let Some(session) = session {
        client = client.with_session(session);
    }
    Ok(client)
}

/// Where sessions live unless the user says otherwise.
pub fn session_dir() -> Result<PathBuf, String> {
    default_session_dir().map_err(|error| error.to_string())
}

/// The saved sessions, for a picker.
pub fn sessions() -> Result<Vec<SessionInfo>, String> {
    sessions_in(&session_dir()?)
}

/// The saved sessions under a store directory. Split out so it can be tested
/// against a directory that is not the user's own.
pub fn sessions_in(dir: &Path) -> Result<Vec<SessionInfo>, String> {
    SessionStore::at(dir)
        .list()
        .map(|list| list.iter().map(session_info).collect())
        .map_err(|error| error.to_string())
}

fn session_info(summary: &wnd_core::session::SessionSummary) -> SessionInfo {
    SessionInfo {
        name: summary.name.clone(),
        hosts: summary.hosts.clone(),
        cookies: summary.cookies,
        saved_at: summary.saved_at,
        expires_at: summary.expires_at,
    }
}

/// Load a session by name from the user's store.
pub fn load_session(name: &str) -> Result<Session, String> {
    SessionStore::at(session_dir()?)
        .load(name)
        .map_err(|error| error.to_string())
}

/// Save a session from a browser's `cookies.txt`, as `nvd session save` does.
///
/// A name that is already there is replaced: that is the refresh gesture, and
/// the commonest reason to come back to this panel.
pub fn save_session_file(name: &str, file: &Path) -> Result<(), String> {
    save_session_file_in(&session_dir()?, name, file)
}

/// The same, against a store directory a test can own.
pub fn save_session_file_in(dir: &Path, name: &str, file: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(file)
        .map_err(|error| format!("could not read {}: {error}", file.display()))?;
    let cookies = netscape::parse(&text)
        .map_err(|error| format!("could not read {}: {error}", file.display()))?;
    if cookies.is_empty() {
        return Err(format!("{} holds no cookies", file.display()));
    }
    save_cookies(dir, name, cookies)
}

/// Save a session from a raw `Cookie:` header, for the one site it names.
///
/// The site may be pasted as an address — `https://novel18.syosetu.com/…` is
/// what a person has on the clipboard — but a cookie domain has to be a host,
/// so only the host is kept.
pub fn save_session_header(name: &str, host: &str, header: &str) -> Result<(), String> {
    save_session_header_in(&session_dir()?, name, host, header)
}

/// The same, against a store directory a test can own.
pub fn save_session_header_in(
    dir: &Path,
    name: &str,
    host: &str,
    header: &str,
) -> Result<(), String> {
    let host = host_from_input(host)?;
    let cookies = netscape::parse_cookie_header(header, &host);
    if cookies.is_empty() {
        return Err("the cookie header holds no cookies".to_string());
    }
    save_cookies(dir, name, cookies)
}

/// Forget a saved session.
pub fn forget_session(name: &str) -> Result<(), String> {
    forget_session_in(&session_dir()?, name)
}

/// The same, against a store directory a test can own.
pub fn forget_session_in(dir: &Path, name: &str) -> Result<(), String> {
    SessionStore::at(dir)
        .delete(name)
        .map_err(|error| error.to_string())
}

/// Write a jar under a name, whichever way it arrived.
fn save_cookies(dir: &Path, name: &str, cookies: Vec<Cookie>) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("give the session a name".to_string());
    }
    let session = Session::from_cookies(name, cookies);
    SessionStore::at(dir)
        .save(&session)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// The host in what a person typed: a host, or the address they copied.
fn host_from_input(text: &str) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("the cookies need a site: paste its address".to_string());
    }
    let host = match Url::parse(text) {
        Ok(url) => url.host_str().map(str::to_string),
        Err(url::ParseError::RelativeUrlWithoutBase) => {
            // `novel18.syosetu.com/` and `novel18.syosetu.com` are one site.
            let host = text.split('/').next().unwrap_or(text);
            (!host.is_empty()).then(|| host.to_string())
        }
        Err(error) => return Err(format!("`{text}` is not a site: {error}")),
    };
    match host {
        Some(host) if !host.is_empty() => Ok(host.to_ascii_lowercase()),
        _ => Err(format!("`{text}` has no site name")),
    }
}

/// A URL typed by a person, which may be missing its scheme.
///
/// `ncode.syosetu.com/n0000aa/` is what someone copies out of an address bar's
/// text, and refusing it because it has no `https://` would be the tool being
/// pedantic about a thing it can work out.
pub fn url_from_input(text: &str) -> Result<Url, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("paste a novel URL first".to_string());
    }
    match Url::parse(text) {
        Ok(url) => Ok(url),
        Err(url::ParseError::RelativeUrlWithoutBase) => Url::parse(&format!("https://{text}"))
            .map_err(|error| format!("`{text}` is not a URL: {error}")),
        Err(error) => Err(format!("`{text}` is not a URL: {error}")),
    }
}

/// A format name from the front end.
pub fn parse_format(name: &str) -> Result<Format, String> {
    match name {
        "epub" => Ok(Format::Epub),
        "txt" => Ok(Format::Txt),
        "md" => Ok(Format::Markdown),
        other => Err(format!(
            "`{other}` is not an export format ({} are)",
            FORMATS.join(", ")
        )),
    }
}

/// How furigana is written where a format has none.
pub fn parse_ruby(name: &str) -> Result<RubyStyle, String> {
    match name {
        "parenthesised" => Ok(RubyStyle::Parenthesised),
        "base_only" => Ok(RubyStyle::BaseOnly),
        other => Err(format!(
            "`{other}` is not a furigana style (parenthesised, base_only)"
        )),
    }
}

/// Everything both requests have in common, checked.
fn prepare(settings: &Settings, output: PathBuf) -> Result<Prepared, String> {
    let mut export = Vec::new();
    for name in &settings.export {
        export.push(parse_format(name)?);
    }

    let ruby = match &settings.ruby {
        Some(name) => parse_ruby(name)?,
        None => RubyStyle::default(),
    };

    Ok(Prepared {
        url: None,
        output,
        session: settings.session.clone(),
        export,
        export_options: ExportOptions {
            ruby,
            include_notes: settings.notes,
        },
        download: DownloadOptions::default(),
        check: false,
    })
}

/// Check a download over, without fetching anything.
pub fn prepare_download(request: &DownloadRequest) -> Result<Prepared, String> {
    prepare_download_with(&wnd_sources::registry(), request)
}

/// Check a download over against a particular set of adapters.
///
/// The registry is a parameter so that this — the validation a window does
/// before it starts a thread — can be tested without the network and without
/// inventing a corner of a real site for a fake to pretend to be.
pub fn prepare_download_with(
    sources: &[Box<dyn Source>],
    request: &DownloadRequest,
) -> Result<Prepared, String> {
    let url = url_from_input(&request.url)?;
    if wnd_sources::find(sources, &url).is_none() {
        return Err(format!("no source adapter handles {url}"));
    }

    let output = request.output.trim();
    if output.is_empty() {
        return Err("choose where to save the novel".to_string());
    }
    if request.max_chapters == Some(0) {
        return Err("at most 0 chapters is nothing to do".to_string());
    }

    let mut prepared = prepare(&request.settings, PathBuf::from(output))?;
    prepared.url = Some(url);
    prepared.download = DownloadOptions {
        max_chapters: request.max_chapters,
        force: request.force,
    };
    Ok(prepared)
}

/// Check an update over: the directory has to hold a manifest, since that is
/// where the novel's address comes from.
pub fn prepare_update(request: &UpdateRequest) -> Result<Prepared, String> {
    let dir = request.dir.trim();
    if dir.is_empty() {
        return Err("choose the directory to update".to_string());
    }
    let output = PathBuf::from(dir);
    // Reading it is the check: an update cannot start without the novel's URL,
    // and the manifest is the only place it is written down.
    NovelDir::at(&output)
        .read_manifest()
        .map_err(|error| format!("{}: {error}", output.display()))?;

    let mut prepared = prepare(&request.settings, output)?;
    prepared.check = request.check;
    Ok(prepared)
}

/// Check an export over.
pub fn prepare_export(request: &ExportRequest) -> Result<Prepared, String> {
    let dir = request.dir.trim();
    if dir.is_empty() {
        return Err("choose a directory to export".to_string());
    }
    let output = PathBuf::from(dir);
    NovelDir::at(&output)
        .read_manifest()
        .map_err(|error| format!("{}: {error}", output.display()))?;

    let prepared = prepare(&request.settings, output)?;
    if prepared.export.is_empty() {
        return Err("pick at least one format to export".to_string());
    }
    Ok(prepared)
}

/// Fetch a novel, then write the exports that were asked for.
pub fn download(
    sources: &[Box<dyn Source>],
    fetch: &HttpClient,
    prepared: &Prepared,
    mut emit: impl FnMut(Event),
) -> Result<Report, String> {
    let url = prepared
        .url
        .as_ref()
        .ok_or_else(|| "a download needs a URL".to_string())?;

    let summary = wnd_download::download(
        sources,
        fetch,
        url,
        &prepared.output,
        &prepared.download,
        |event| emit(Event::from_progress(event)),
    )
    .map_err(|error| error.to_string())?;

    let dir = NovelDir::at(&summary.output_dir);
    let exported = write_exports(&dir, &prepared.export, &prepared.export_options, &mut emit)?;

    Ok(report(
        &dir,
        &summary.title,
        Counts {
            total: summary.chapters_total,
            fetched: summary.chapters_fetched,
            skipped: summary.chapters_skipped,
            checked: 0,
        },
        summary.images_fetched,
        summary.images_missing,
        summary.bytes_written,
        summary.output_dir.clone(),
        exported,
        Vec::new(),
        summary.chapters_fetched == 0 && summary.images_fetched == 0,
    ))
}

/// Bring a directory up to date, then write the exports that were asked for.
pub fn update(
    sources: &[Box<dyn Source>],
    fetch: &HttpClient,
    prepared: &Prepared,
    mut emit: impl FnMut(Event),
) -> Result<Report, String> {
    let options = UpdateOptions {
        check: prepared.check,
    };
    let summary = wnd_download::update(sources, fetch, &prepared.output, &options, |event| {
        emit(Event::from_progress(event))
    })
    .map_err(|error| error.to_string())?;

    if !summary.removed.is_empty() {
        for gone in &summary.removed {
            emit(Event::Removed {
                index: gone.index,
                title: gone.title.clone(),
            });
        }
    }

    let dir = NovelDir::at(&summary.output_dir);
    let exported = write_exports(&dir, &prepared.export, &prepared.export_options, &mut emit)?;

    Ok(report(
        &dir,
        &summary.title,
        Counts {
            total: summary.chapters_total,
            fetched: summary.chapters_fetched,
            skipped: summary.chapters_skipped,
            checked: summary.chapters_checked,
        },
        summary.images_fetched,
        summary.images_missing,
        summary.bytes_written,
        summary.output_dir.clone(),
        exported,
        summary
            .removed
            .iter()
            .map(|gone| Gone {
                index: gone.index,
                title: gone.title.clone(),
            })
            .collect(),
        summary.up_to_date(),
    ))
}

/// Write the chosen exports, reporting each one as it lands.
pub fn write_exports(
    dir: &NovelDir,
    formats: &[Format],
    options: &ExportOptions,
    emit: &mut impl FnMut(Event),
) -> Result<Vec<ExportedFile>, String> {
    let mut written = Vec::new();
    for format in formats {
        let summary = wnd_export::export(dir, *format, options)
            .map_err(|error| format!("could not export {}: {error}", dir.root().display()))?;
        let file = ExportedFile {
            format: summary.format.extension().to_string(),
            path: summary.path.display().to_string(),
            bytes: summary.bytes,
            chapters: summary.chapters,
        };
        emit(Event::Exported {
            format: file.format.clone(),
            path: file.path.clone(),
            bytes: file.bytes,
        });
        written.push(file);
    }
    Ok(written)
}

/// Export a directory that is already downloaded.
pub fn export(
    prepared: &Prepared,
    emit: &mut impl FnMut(Event),
) -> Result<Vec<ExportedFile>, String> {
    let dir = NovelDir::at(&prepared.output);
    write_exports(&dir, &prepared.export, &prepared.export_options, emit)
}

/// Describe a directory.
pub fn info(dir: &Path) -> Result<NovelInfo, String> {
    let novel = NovelDir::at(dir);
    let manifest = novel
        .read_manifest()
        .map_err(|error| format!("{}: {error}", novel.root().display()))?;

    let chapters_missing = manifest
        .chapters
        .iter()
        .filter(|chapter| !novel.has_chapter(chapter.index))
        .count();
    let images_missing = manifest
        .images
        .iter()
        .filter(|image| !novel.image_path(&image.file).is_file())
        .count();

    Ok(NovelInfo {
        title: manifest.title,
        author: manifest.author,
        rating: manifest.content_rating,
        source: manifest.source.id,
        novel_url: manifest.source.novel_url,
        language: manifest.language,
        tags: manifest.tags,
        chapters: manifest.chapters.len(),
        images: manifest.images.len(),
        chapters_missing,
        images_missing,
        output_dir: novel.root().display().to_string(),
    })
}

/// Read the novels a folder holds: the folder itself, and its direct children.
///
/// A novel is a folder with a `manifest.json`. One whose manifest cannot be read
/// is still listed, with the problem, so it can be opened and removed rather
/// than silently hidden. Nothing below the direct children is read: a library
/// is a shelf, not a walk of the disk.
pub fn library(root: &Path) -> Result<Vec<LibraryItem>, String> {
    if root.as_os_str().is_empty() {
        return Err("choose a folder to list".to_string());
    }
    let metadata =
        std::fs::metadata(root).map_err(|error| format!("{}: {error}", root.display()))?;
    if !metadata.is_dir() {
        return Err(format!("{} is not a folder", root.display()));
    }

    let mut items = Vec::new();
    if root.join("manifest.json").is_file() {
        items.push(library_item(root, false));
    }
    let entries = std::fs::read_dir(root)
        .map_err(|error| format!("could not read {}: {error}", root.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("could not read {}: {error}", root.display()))?;
        let path = entry.path();
        if path.is_dir() && path.join("manifest.json").is_file() {
            items.push(library_item(&path, true));
        }
    }

    items.sort_by(|left, right| {
        left.title
            .to_lowercase()
            .cmp(&right.title.to_lowercase())
            .then_with(|| left.output_dir.cmp(&right.output_dir))
    });
    Ok(items)
}

/// One row, read from its manifest; a broken manifest is the row's problem.
fn library_item(dir: &Path, removable: bool) -> LibraryItem {
    let folder = dir
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| dir.display().to_string());

    match NovelDir::at(dir).read_manifest() {
        Ok(manifest) => LibraryItem {
            title: manifest.title,
            author: manifest.author,
            chapters: manifest.chapters.len(),
            images: manifest.images.len(),
            rating: Some(manifest.content_rating),
            fetched_at: manifest.fetched_at,
            output_dir: dir.display().to_string(),
            removable,
            problem: None,
        },
        Err(error) => LibraryItem {
            title: folder,
            author: None,
            chapters: 0,
            images: 0,
            rating: None,
            fetched_at: None,
            output_dir: dir.display().to_string(),
            removable,
            problem: Some(error.to_string()),
        },
    }
}

/// The folder a removal would take, if it is a novel and really is in `root`.
///
/// The check is the whole safety of Remove: only a direct child of the library
/// folder, only a folder with a manifest, and never the library folder itself.
/// It is a separate step so that the decision can be tested without moving
/// anything to the trash.
pub fn removal_target(root: &Path, novel: &Path) -> Result<PathBuf, String> {
    let root_canonical =
        std::fs::canonicalize(root).map_err(|error| format!("{}: {error}", root.display()))?;
    if !root_canonical.is_dir() {
        return Err(format!("{} is not a folder", root.display()));
    }
    let novel_canonical =
        std::fs::canonicalize(novel).map_err(|error| format!("{}: {error}", novel.display()))?;

    if novel_canonical == root_canonical {
        return Err("that is the library folder itself, not a novel in it".to_string());
    }
    if novel_canonical.parent() != Some(root_canonical.as_path()) {
        return Err(format!(
            "{} is not directly inside {}",
            novel.display(),
            root.display()
        ));
    }
    if !novel_canonical.join("manifest.json").is_file() {
        return Err(format!(
            "{} has no manifest.json, so it is not a novel",
            novel.display()
        ));
    }

    // The caller's own path, not the canonical one: it is the path the window
    // shows, and on Windows the canonical form carries a `\\?\` prefix the
    // trash does not need to be given.
    Ok(novel.to_path_buf())
}

/// The program and arguments that open `path` with the desktop's own tools.
///
/// The window never decides what an OS is called; it asks here. A folder opens
/// in the file manager and a file in its default application, because that is
/// what each of these programs does with a path.
pub fn open_command(path: &Path) -> Result<(String, Vec<String>), String> {
    if !path.exists() {
        return Err(format!("{} is not there", path.display()));
    }
    let target = path.display().to_string();

    #[cfg(target_os = "windows")]
    let program = "explorer";
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(all(unix, not(target_os = "macos")))]
    let program = "xdg-open";

    Ok((program.to_string(), vec![target]))
}

struct Counts {
    total: usize,
    fetched: usize,
    skipped: usize,
    checked: usize,
}

#[allow(clippy::too_many_arguments)]
fn report(
    dir: &NovelDir,
    title: &str,
    counts: Counts,
    images_fetched: usize,
    images_missing: usize,
    bytes_written: u64,
    output_dir: PathBuf,
    exported: Vec<ExportedFile>,
    removed: Vec<Gone>,
    up_to_date: bool,
) -> Report {
    // The rating and the author are not in any summary: they belong to the work,
    // and the manifest is where a work's own facts are kept. A read that fails
    // here cannot mean much — the run just wrote that file — so it leaves them
    // absent rather than failing a download that worked.
    let manifest = dir.read_manifest().ok();

    Report {
        title: title.to_string(),
        author: manifest.as_ref().and_then(|m| m.author.clone()),
        rating: manifest.as_ref().map(|m| m.content_rating),
        chapters_total: counts.total,
        chapters_fetched: counts.fetched,
        chapters_skipped: counts.skipped,
        chapters_checked: counts.checked,
        images_fetched,
        images_missing,
        bytes_written,
        output_dir: output_dir.display().to_string(),
        exported,
        removed,
        up_to_date,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// The window sends camelCase keys and spreads its options next to the
    /// request's own fields, because the form has one set of options and two
    /// buttons. Both sides have to agree on that shape, and nothing else would
    /// ever notice if they stopped.
    #[test]
    fn a_request_arrives_as_the_window_sends_it() {
        let sent = json!({
            "url": "https://ncode.syosetu.com/n0000aa/",
            "output": "C:\\novels",
            "maxChapters": 5,
            "force": true,
            "session": "narou",
            "export": ["epub", "md"],
            "ruby": "base_only",
            "notes": false
        });

        let request: DownloadRequest = serde_json::from_value(sent).expect("the window's own JSON");
        assert_eq!(request.url, "https://ncode.syosetu.com/n0000aa/");
        assert_eq!(request.output, "C:\\novels");
        assert_eq!(request.max_chapters, Some(5));
        assert!(request.force);
        assert_eq!(request.settings.session.as_deref(), Some("narou"));
        assert_eq!(request.settings.export, ["epub", "md"]);
        assert_eq!(request.settings.ruby.as_deref(), Some("base_only"));
        assert!(!request.settings.notes);

        // What the window leaves alone when it has no opinion.
        let minimal: DownloadRequest =
            serde_json::from_value(json!({"url": "u", "output": "o"})).expect("a bare request");
        assert_eq!(minimal.max_chapters, None);
        assert!(!minimal.force);
        assert_eq!(minimal.settings, Settings::default());
        assert!(
            minimal.settings.notes,
            "the author's notes are included unless asked otherwise"
        );
    }

    #[test]
    fn the_other_two_requests_come_in_the_same_shape() {
        let update: UpdateRequest = serde_json::from_value(json!({
            "dir": "C:\\novels\\A",
            "check": true,
            "session": null,
            "export": []
        }))
        .expect("the window's own JSON");
        assert_eq!(update.dir, "C:\\novels\\A");
        assert!(update.check);
        assert_eq!(update.settings.session, None);
        assert!(update.settings.export.is_empty());

        let export: ExportRequest = serde_json::from_value(json!({
            "dir": "C:\\novels\\A",
            "export": ["txt"]
        }))
        .expect("the window's own JSON");
        assert_eq!(export.settings.export, ["txt"]);
        assert_eq!(export.settings.ruby, None);
    }
}
