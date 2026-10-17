#![allow(dead_code)] // every test binary compiles this module; not all use everything

//! Test doubles shared by this crate's integration tests.
//!
//! `tests/` subfolders are modules, not test targets, so this file is never
//! compiled as a test binary of its own.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
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
}

fn chapter_with_id(id: u32) -> FakeChapter {
    FakeChapter {
        id: id.to_string(),
        title: format!("第{id}話"),
        revised_at: None,
        body: format!("body of chapter {id}"),
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
        }
    }

    pub fn adult(chapter_count: u32) -> Self {
        Self {
            rating: ContentRating::Adult,
            ..Self::new(chapter_count)
        }
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
        Ok(parsed)
    }
}

/// A client that will never be asked to make a request.
pub fn client() -> HttpClient {
    HttpClient::new(FetchPolicy::default()).expect("client")
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
