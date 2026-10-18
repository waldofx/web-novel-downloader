#![allow(dead_code)] // every test binary compiles this module; not all use everything

//! A fake site and a temporary directory, for the app's own tests.
//!
//! The fetching is `wnd-download`'s and is tested there; what is checked here is
//! that the app asks for the right thing and reports it in the shape the window
//! reads.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use url::Url;
use wnd_core::store::storage::chapter_file_name;
use wnd_core::{
    Block, Chapter, ChapterMeta, ContentRating, HttpClient, Inline, Manifest, SourceInfo,
};
use wnd_sources::{Source, SourceError};

/// A directory under the system temp that removes itself when the test ends.
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub fn new(tag: &str) -> TempDir {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("wnd-gui-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&path).expect("a temporary directory");
        TempDir { path }
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

/// One chapter the fake site serves.
pub struct FakeChapter {
    pub index: u32,
    pub title: String,
    pub body: String,
}

/// An adapter that serves a novel from memory and never touches the network.
///
/// It counts chapter fetches so that a test can prove a resume did not fetch
/// anything again — the one promise the app must not break, since every
/// re-fetch is a request to somebody else's server.
pub struct FakeSource {
    pub url: String,
    pub title: String,
    pub author: Option<String>,
    pub chapters: Vec<FakeChapter>,
    /// Shared so that the count outlives the adapter, which the downloader takes
    /// ownership of.
    pub chapter_fetches: Arc<AtomicUsize>,
    pub toc_fetches: Arc<AtomicUsize>,
}

impl FakeSource {
    pub fn new(title: &str) -> FakeSource {
        FakeSource {
            url: "https://fake.example/novel/1".to_string(),
            title: title.to_string(),
            author: Some("Someone".to_string()),
            chapters: Vec::new(),
            chapter_fetches: Arc::new(AtomicUsize::new(0)),
            toc_fetches: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// A novel of `count` chapters, numbered from one.
    pub fn with_chapters(mut self, count: u32) -> FakeSource {
        for index in 1..=count {
            self.chapters.push(FakeChapter {
                index,
                title: format!("第{index}話"),
                body: format!("これは {index} 番目の章です。"),
            });
        }
        self
    }

    /// A handle on the chapter-fetch count, for after the adapter is moved.
    pub fn fetches(&self) -> Arc<AtomicUsize> {
        Arc::clone(&self.chapter_fetches)
    }
}

impl Source for FakeSource {
    fn id(&self) -> &'static str {
        "fake"
    }

    fn matches(&self, url: &Url) -> bool {
        url.as_str().starts_with(self.url.as_str())
    }

    fn content_rating(&self) -> ContentRating {
        ContentRating::General
    }

    fn novel(&self, _fetch: &HttpClient, _url: &Url) -> Result<Manifest, SourceError> {
        self.toc_fetches.fetch_add(1, Ordering::SeqCst);
        let mut manifest = Manifest::new(
            SourceInfo {
                id: self.id().to_string(),
                novel_url: self.url.clone(),
                site_title: None,
            },
            self.title.clone(),
            "ja",
        );
        manifest.author = self.author.clone();
        manifest.chapters = self
            .chapters
            .iter()
            .map(|chapter| ChapterMeta {
                index: chapter.index,
                title: chapter.title.clone(),
                url: format!("{}/{}", self.url, chapter.index),
                arc: None,
                published_at: None,
                revised_at: None,
                hash: None,
                file: chapter_file_name(chapter.index),
            })
            .collect();
        Ok(manifest)
    }

    fn chapter(&self, _fetch: &HttpClient, meta: &ChapterMeta) -> Result<Chapter, SourceError> {
        self.chapter_fetches.fetch_add(1, Ordering::SeqCst);
        let body = self
            .chapters
            .iter()
            .find(|chapter| chapter.index == meta.index)
            .map(|chapter| chapter.body.clone())
            .unwrap_or_default();

        let mut chapter = Chapter::new(meta.index, meta.title.clone());
        chapter.nodes = vec![Block::Paragraph {
            children: vec![Inline::Text { value: body }],
        }];
        Ok(chapter)
    }
}

/// A client that is never used, for a job whose adapter ignores it.
pub fn client() -> HttpClient {
    HttpClient::new(wnd_core::FetchPolicy::default()).expect("a client")
}
