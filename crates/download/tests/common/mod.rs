#![allow(dead_code)] // every test binary compiles this module; not all use everything

//! Test doubles shared by this crate's integration tests.
//!
//! `tests/` subfolders are modules, not test targets, so this file is never
//! compiled as a test binary of its own.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use url::Url;
use wnd_core::store::storage::chapter_file_name;
use wnd_core::{
    Block, Chapter, ChapterMeta, ContentRating, FetchPolicy, HttpClient, Inline, Manifest,
    SourceInfo,
};
use wnd_sources::{Source, SourceError};

/// An adapter that serves a fixed novel from memory.
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
    pub chapter_count: u32,
    /// Incremented once per table-of-contents read.
    pub toc_fetches: AtomicUsize,
    /// Incremented once per chapter fetch attempt, successful or not.
    pub chapter_fetches: AtomicUsize,
    /// When set, `chapter()` fails for this index. Used to check that an
    /// interrupted run leaves a directory that resumes.
    pub fail_index: Option<u32>,
}

impl FakeSource {
    pub fn new(chapter_count: u32) -> Self {
        Self {
            url: "https://fake.test/novel/".to_string(),
            title: "偽の小説".to_string(),
            rating: ContentRating::General,
            chapter_count,
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
        manifest.chapters = (1..=self.chapter_count)
            .map(|index| ChapterMeta {
                index,
                title: format!("第{index}話"),
                url: format!("{}{index}/", self.url),
                arc: None,
                published_at: None,
                hash: None,
                file: format!("chapters/{}", chapter_file_name(index)),
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

        let mut chapter = Chapter::new(meta.index, meta.title.clone());
        chapter.nodes.push(Block::Paragraph {
            children: vec![Inline::Text {
                value: format!("body of chapter {}", meta.index),
            }],
        });
        Ok(chapter)
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
