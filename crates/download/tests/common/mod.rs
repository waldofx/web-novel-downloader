#![allow(dead_code)] // every test binary compiles this module; not all use everything

//! Test doubles shared by this crate's integration tests.
//!
//! `tests/` subfolders are modules, not test targets, so this file is never
//! compiled as a test binary of its own.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use url::Url;
use wnd_core::store::storage::chapter_file_name;
use wnd_core::{
    Block, Chapter, ChapterMeta, ContentRating, FetchPolicy, HttpClient, Inline, Manifest,
    SourceInfo,
};
use wnd_sources::{Source, SourceError};

/// One chapter, as the fake site serves it right now.
///
/// Mutable on purpose: `nvd update` exists to notice what changed between two
/// runs, and a test can only prove that by changing what the "site" says.
#[derive(Debug, Clone)]
pub struct FakeChapter {
    /// What the chapter is, independent of where it sits in the index. Kakuyomu's
    /// url carries an opaque id, so a chapter inserted in the middle moves every
    /// later chapter's *position* without changing their identity — the case a
    /// hash-only comparison gets wrong.
    pub id: String,
    pub title: String,
    /// What the index says about revisions, when the site says anything.
    pub revised_at: Option<String>,
    /// The body text, so a test can change what the site serves without touching
    /// any timestamp: an edit the index cannot announce.
    pub body: String,
    /// Image URLs the body references, in order. An image is an illustration the
    /// *fetcher* has to store; the adapter only knows where it is.
    pub images: Vec<String>,
}

/// An adapter that serves a novel from memory.
///
/// It never touches the network: the [`HttpClient`] handed to it is what a real
/// adapter would fetch with, but this one ignores it. Fetch counts are exposed
/// so a test can prove that a chapter was — or was not — fetched again.
#[derive(Debug)]
pub struct FakeSource {
    /// Identifies the novel. A download into a directory holding a manifest for
    /// a *different* `novel_url` must not resume against it.
    pub url: String,
    pub title: String,
    pub rating: ContentRating,
    /// The index, as the site serves it now. Behind a lock because [`Source`]
    /// takes `&self` and a test changes the site between two runs.
    chapters: Mutex<Vec<FakeChapter>>,
    /// Incremented once per table-of-contents read.
    pub toc_fetches: AtomicUsize,
    /// Incremented once per chapter fetch attempt, successful or not.
    pub chapter_fetches: AtomicUsize,
    /// When set, `chapter()` fails for this index. Used to check that an
    /// interrupted run leaves a directory that resumes.
    pub fail_index: Option<u32>,
    /// The cover URL the index states. `None` for a site with no covers.
    pub cover_url: Mutex<Option<String>>,
    /// The synopsis the work page states, if it states one. Pictures in it are
    /// the work's rather than a chapter's.
    pub synopsis: Mutex<Vec<Block>>,
    /// What this adapter says its images must be requested with.
    pub referer: Option<&'static str>,
}

fn chapter_with_id(id: u32) -> FakeChapter {
    FakeChapter {
        id: id.to_string(),
        title: format!("第{id}話"),
        revised_at: None,
        body: format!("body of chapter {id}"),
        images: Vec::new(),
    }
}

impl FakeSource {
    pub fn new(chapter_count: u32) -> Self {
        Self {
            url: "https://fake.test/novel/".to_string(),
            title: "偽の小説".to_string(),
            rating: ContentRating::General,
            chapters: Mutex::new((1..=chapter_count).map(chapter_with_id).collect()),
            toc_fetches: AtomicUsize::new(0),
            chapter_fetches: AtomicUsize::new(0),
            fail_index: None,
            cover_url: Mutex::new(None),
            synopsis: Mutex::new(Vec::new()),
            referer: None,
        }
    }

    pub fn adult(chapter_count: u32) -> Self {
        Self {
            rating: ContentRating::Adult,
            ..Self::new(chapter_count)
        }
    }

    /// State a cover, the way an adapter reads one off a work page.
    pub fn with_cover(self, url: &str) -> Self {
        *self.cover_url.lock().unwrap() = Some(url.to_string());
        self
    }

    /// State a synopsis, the way a work page states one.
    pub fn with_synopsis(self, blocks: Vec<Block>) -> Self {
        *self.synopsis.lock().unwrap() = blocks;
        self
    }

    /// State the `Referer` this "site" demands for its images.
    pub fn with_referer(self, referer: &'static str) -> Self {
        let mut source = self;
        source.referer = Some(referer);
        source
    }

    /// Add an image to a chapter's body.
    pub fn image(&self, position: usize, url: &str) {
        self.chapters()[position - 1].images.push(url.to_string());
    }

    pub fn toc_fetches(&self) -> usize {
        self.toc_fetches.load(Ordering::SeqCst)
    }

    pub fn chapter_fetches(&self) -> usize {
        self.chapter_fetches.load(Ordering::SeqCst)
    }

    fn chapters(&self) -> std::sync::MutexGuard<'_, Vec<FakeChapter>> {
        self.chapters
            .lock()
            .expect("the fake index is not poisoned")
    }

    /// Publish one more chapter at the end.
    pub fn add_chapter(&self, id: u32) {
        self.chapters().push(chapter_with_id(id));
    }

    /// Drop a chapter from the index, the way an author deletes one.
    pub fn remove_chapter(&self, position: usize) {
        self.chapters().remove(position - 1);
    }

    /// Insert a chapter at `position` (1-based), shifting the ones after it.
    pub fn insert_chapter(&self, position: usize, id: u32) {
        self.chapters().insert(position - 1, chapter_with_id(id));
    }

    /// Mark a chapter as revised, the way an index announces one.
    pub fn revise_chapter(&self, position: usize, revised_at: &str) {
        self.chapters()[position - 1].revised_at = Some(revised_at.to_string());
    }

    /// Change what a chapter says without touching any timestamp.
    pub fn edit_chapter(&self, position: usize, body: &str) {
        self.chapters()[position - 1].body = body.to_string();
    }
}

impl Source for FakeSource {
    fn id(&self) -> &'static str {
        "fake"
    }

    fn matches(&self, url: &Url) -> bool {
        url.as_str().starts_with(&self.url)
    }

    fn content_rating(&self) -> ContentRating {
        self.rating
    }

    fn image_referer(&self) -> Option<&'static str> {
        self.referer
    }

    fn novel(&self, _fetch: &HttpClient, _url: &Url) -> Result<Manifest, SourceError> {
        self.toc_fetches.fetch_add(1, Ordering::SeqCst);

        let mut manifest = Manifest::new(
            SourceInfo {
                id: "fake".to_string(),
                novel_url: self.url.clone(),
                site_title: None,
            },
            self.title.clone(),
            "ja",
        );
        manifest.content_rating = self.rating;
        manifest.cover_url = self.cover_url.lock().unwrap().clone();
        manifest.synopsis = self.synopsis.lock().unwrap().clone();
        manifest.chapters = self
            .chapters()
            .iter()
            .enumerate()
            .map(|(position, chapter)| {
                let index = position as u32 + 1;
                ChapterMeta {
                    index,
                    title: chapter.title.clone(),
                    url: format!("{}{}/", self.url, chapter.id),
                    arc: None,
                    published_at: None,
                    revised_at: chapter.revised_at.clone(),
                    hash: None,
                    file: format!("chapters/{}", chapter_file_name(index)),
                }
            })
            .collect();
        Ok(manifest)
    }

    fn chapter(&self, _fetch: &HttpClient, meta: &ChapterMeta) -> Result<Chapter, SourceError> {
        self.chapter_fetches.fetch_add(1, Ordering::SeqCst);

        if self.fail_index == Some(meta.index) {
            return Err(SourceError::Other(format!(
                "chapter {} is unavailable",
                meta.index
            )));
        }

        let chapters = self.chapters();
        let chapter = chapters
            .iter()
            .find(|chapter| format!("{}{}/", self.url, chapter.id) == meta.url)
            .ok_or_else(|| {
                SourceError::Other(format!("chapter {} is not in the index", meta.index))
            })?;

        let mut parsed = Chapter::new(meta.index, meta.title.clone());
        parsed.nodes.push(Block::Paragraph {
            children: vec![Inline::Text {
                value: chapter.body.clone(),
            }],
        });
        for url in &chapter.images {
            parsed.nodes.push(Block::Image {
                src: url.clone(),
                alt: None,
            });
        }
        Ok(parsed)
    }
}

/// A client that will never be asked to make a request.
pub fn client() -> HttpClient {
    HttpClient::new(FetchPolicy::default()).expect("client")
}

/// A client for a local server: no `robots.txt` to consult, no politeness
/// delay, and no retries — a test must not sleep its way through a backoff, and
/// a `404` it is asserting on must not be asked for twice.
pub fn local_client() -> HttpClient {
    HttpClient::new(FetchPolicy {
        respect_robots: false,
        min_delay: std::time::Duration::ZERO,
        max_retries: 1,
        ..FetchPolicy::default()
    })
    .expect("client")
}

/// The works this machine is allowed to hit: the gitignored `fixtures.local.json`
/// at the workspace root. An absent file, an absent site, or an entry that does
/// not fit yields nothing — a live test skips rather than fails, and says so.
///
/// The file is the user's own and never enters the repository; the committed
/// `fixtures.local.example.json` is only a template. The root is found by walking
/// up from this crate until a directory holds that example beside a `Cargo.toml`,
/// rather than counting `../..`, so moving the crate cannot point the tests at
/// the wrong file. A file that exists but is not JSON is a broken local file,
/// not an empty one: it panics rather than looking like "nothing configured".
pub fn targets(site: &str) -> Vec<Url> {
    let Some(path) = fixtures_file() else {
        return Vec::new();
    };

    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        // Absent is not broken: nothing is configured, so a live test skips.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(error) => panic!("cannot read {}: {error}", path.display()),
    };

    let value: serde_json::Value = serde_json::from_str(&text).unwrap_or_else(|error| {
        panic!(
            "{} is not valid JSON: {error}; it must match fixtures.local.example.json",
            path.display()
        )
    });

    let Some(entries) = value.get(site).and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| entry.get("url")?.as_str())
        .filter_map(|raw| Url::parse(raw).ok())
        .collect()
}

/// Print, into the output a human running the test sees, that this machine has
/// nothing configured for `site`.
pub fn note_no_target(site: &str) {
    println!(
        "note: no `{site}` target in fixtures.local.json — skipping \
         (see fixtures.local.example.json)"
    );
}

/// The gitignored fixtures file, if this machine has one.
///
/// The committed example beside a `Cargo.toml` marks the workspace root — a
/// crate directory has a `Cargo.toml` but never the example — so the walk stops
/// there, whatever a future crate's depth.
fn fixtures_file() -> Option<std::path::PathBuf> {
    let mut dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    loop {
        if dir.join("Cargo.toml").is_file() && dir.join("fixtures.local.example.json").is_file() {
            let local = dir.join("fixtures.local.json");
            return local.is_file().then_some(local);
        }
        dir = dir.parent()?;
    }
}

/// One canned image answer.
#[derive(Debug, Clone)]
pub struct Image {
    pub status: u16,
    /// `None` sends no `Content-Type` header at all, which some hosts do.
    pub content_type: Option<String>,
    pub bytes: Vec<u8>,
}

impl Image {
    pub fn png(bytes: &str) -> Self {
        Self {
            status: 200,
            content_type: Some("image/png".to_string()),
            bytes: bytes.as_bytes().to_vec(),
        }
    }

    /// A worthwhile picture served with no type at all, so the URL decides.
    pub fn untyped(bytes: &str) -> Self {
        Self {
            status: 200,
            content_type: None,
            bytes: bytes.as_bytes().to_vec(),
        }
    }

    /// What a host answers when it has nothing to give.
    pub fn not_found() -> Self {
        Self {
            status: 404,
            content_type: Some("text/html; charset=utf-8".to_string()),
            bytes: b"<html><body>not found</body></html>".to_vec(),
        }
    }

    /// What a filter answers: a page, with a success status.
    pub fn a_page() -> Self {
        Self {
            status: 200,
            content_type: Some("text/html; charset=utf-8".to_string()),
            bytes: "<html><body>BU SİTEYE ERİŞİM ENGELLENMİŞTİR</body></html>"
                .as_bytes()
                .to_vec(),
        }
    }
}

/// A request the image server saw.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Seen {
    pub path: String,
    pub referer: Option<String>,
}

/// A local HTTP server that answers image requests, and remembers them.
///
/// Tests need a real socket here and nowhere else. What has to be proved is that
/// the fetcher *sends* what the site demands, trusts the media type the server
/// states, and gives up on a `404` without losing the chapter — and none of that
/// is visible from inside the process.
///
/// Every path answers the same image unless the test registers another one; the
/// requests are recorded in order, so a test can say "that was not asked for
/// twice" rather than only "the file is there".
#[derive(Debug)]
pub struct ImageServer {
    address: std::net::SocketAddr,
    base: String,
    default: Image,
    routes: Arc<Mutex<std::collections::HashMap<String, Image>>>,
    requests: Arc<Mutex<Vec<Seen>>>,
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl ImageServer {
    pub fn start(default: Image) -> Self {
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
        let address = listener.local_addr().expect("the bound address");
        let routes: Arc<Mutex<std::collections::HashMap<String, Image>>> =
            Arc::new(Mutex::new(std::collections::HashMap::new()));
        let requests: Arc<Mutex<Vec<Seen>>> = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));

        let served_routes = Arc::clone(&routes);
        let served_requests = Arc::clone(&requests);
        let served_stop = Arc::clone(&stop);
        let served_default = default.clone();

        let handle = std::thread::spawn(move || {
            for stream in listener.incoming() {
                if served_stop.load(Ordering::SeqCst) {
                    return;
                }
                let Ok(mut stream) = stream else { continue };
                let seen = read_head(&stream);
                let answer = served_routes
                    .lock()
                    .expect("routes")
                    .get(&seen.path)
                    .cloned()
                    .unwrap_or_else(|| served_default.clone());
                served_requests.lock().expect("requests").push(seen);
                answer_to(&mut stream, &answer);
            }
        });

        Self {
            address,
            base: format!("http://{address}"),
            default,
            routes,
            requests,
            stop,
            handle: Some(handle),
        }
    }

    /// The URL of a path on this server.
    pub fn url(&self, path: &str) -> String {
        format!("{}/{}", self.base, path.trim_start_matches('/'))
    }

    /// Answer `path` with something other than the default.
    pub fn serve(&self, path: &str, image: Image) {
        self.routes
            .lock()
            .expect("routes")
            .insert(format!("/{}", path.trim_start_matches('/')), image);
    }

    pub fn requests(&self) -> Vec<Seen> {
        self.requests.lock().expect("requests").clone()
    }

    pub fn request_count(&self) -> usize {
        self.requests().len()
    }

    pub fn served_default(&self) -> &Image {
        &self.default
    }
}

impl Drop for ImageServer {
    fn drop(&mut self) {
        use std::net::TcpStream;

        self.stop.store(true, Ordering::SeqCst);
        // Wake the accept loop so it can notice that it is finished.
        let _ = TcpStream::connect(self.address);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// Read a request head and keep the two things a test asks about.
fn read_head(stream: &std::net::TcpStream) -> Seen {
    use std::io::BufRead;

    let reading = stream.try_clone().expect("a second handle for reading");
    let mut reader = std::io::BufReader::new(reading);
    let mut seen = Seen::default();
    let mut line = String::new();
    let mut request_line = true;

    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) if line == "\r\n" || line == "\n" => break,
            Ok(_) => {}
        }
        if request_line {
            // `GET /path HTTP/1.1`
            seen.path = line
                .split_whitespace()
                .nth(1)
                .unwrap_or_default()
                .to_string();
            request_line = false;
            continue;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("referer") {
                seen.referer = Some(value.trim().to_string());
            }
        }
    }

    seen
}

/// Write one canned answer, then close.
fn answer_to(stream: &mut std::net::TcpStream, image: &Image) {
    use std::io::Write;

    let content_type = match &image.content_type {
        Some(value) => format!("Content-Type: {value}\r\n"),
        None => String::new(),
    };
    let head = format!(
        "HTTP/1.1 {} {}\r\n{content_type}Content-Length: {}\r\nConnection: close\r\n\r\n",
        image.status,
        reason(image.status),
        image.bytes.len()
    );

    if stream.write_all(head.as_bytes()).is_err() {
        return;
    }
    let _ = stream.write_all(&image.bytes);
    let _ = stream.flush();
}

/// The reason phrase for a status a test cares about.
fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        403 => "Forbidden",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "Unknown",
    }
}

/// A directory that deletes itself when dropped, so a failing assertion cannot
/// leave debris in the system temporary directory.
#[derive(Debug)]
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub fn new(tag: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("wnd-download-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&path).expect("create the temporary directory");
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
