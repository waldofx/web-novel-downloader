//! Orchestration: a novel URL in, a novel directory out.
//!
//! This crate is deliberately thin. Site knowledge lives in `wnd-sources`, and
//! the format and the network live in `wnd-core`; what happens here is the
//! sequence — read the table of contents, work out what is already up to date,
//! fetch what is missing, and record a hash after each chapter so the next run
//! does less work.
//!
//! It is its own crate rather than part of `wnd-core` because it has to resolve
//! a URL to an adapter, which means depending on `wnd-sources`; putting it in
//! `wnd-core` would close the cycle `core → sources → core`.
//!
//! # The sequence, and why it is in this order
//!
//! 1. Read the table of contents. Nothing is written before this succeeds.
//! 2. Refuse adult content, unless allowed — *now*, because the per-work rating
//!    is only known once the index has been read.
//! 3. Decide what to skip. That decision is reported, so a front end can show
//!    "2 of 89" before the first request rather than after it.
//! 4. Write the index, then fetch. The manifest is durable before the first
//!    chapter and rewritten after each one, so an interrupted run leaves a
//!    valid directory that resumes — and a chapter is only skipped when the
//!    file on disk still hashes to what the manifest recorded.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use url::Url;
use wnd_core::store::storage::hash_json;
use wnd_core::{ContentRating, HttpClient, Manifest, NovelDir};
use wnd_sources::{Source, SourceError, find};

/// What to fetch, and how careful to be.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DownloadOptions {
    /// Allow adult works. Refused otherwise — *after* the table of contents is
    /// read, because only then is the real rating known.
    pub allow_adult: bool,
    /// Fetch at most this many chapters, counting from the first.
    pub max_chapters: Option<usize>,
    /// Re-fetch every chapter, ignoring the hashes already on disk.
    pub force: bool,
}

/// Progress, reported once per step so a front end can show something.
///
/// The events carry the chapter titles as well as the indices: a progress line
/// that says only "chapter 43" is not something a person can use, and a front end
/// should not have to read the directory it is writing to label its own output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    /// The table of contents has been read. `to_fetch` is how many chapters this
    /// run expects to fetch: `chapters` minus those already up to date.
    Planned {
        title: String,
        chapters: usize,
        to_fetch: usize,
    },
    /// One chapter was fetched and written.
    Fetched {
        index: u32,
        title: String,
        bytes: u64,
    },
    /// One chapter was already present and up to date.
    Skipped { index: u32, title: String },
}

/// What a download did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub title: String,
    /// Chapters in the recorded manifest — i.e. after `max_chapters`, so that
    /// `chapters_fetched + chapters_skipped == chapters_total`.
    pub chapters_total: usize,
    pub chapters_fetched: usize,
    pub chapters_skipped: usize,
    /// Bytes of chapter content written. Manifest bookkeeping is not counted, so
    /// a run that only skipped reports zero.
    pub bytes_written: u64,
    pub output_dir: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error(transparent)]
    Source(#[from] SourceError),

    #[error(transparent)]
    Core(#[from] wnd_core::Error),

    /// The work is adult content and `allow_adult` was not set.
    #[error("{url} is adult content; re-run with --allow-adult if that is intended")]
    AdultRefused { url: String },

    /// No adapter claims the URL.
    #[error("no source adapter handles {url}")]
    Unsupported { url: String },
}

/// Find the adapter for `url` and download the novel it names.
pub fn download(
    sources: &[Box<dyn Source>],
    fetch: &HttpClient,
    url: &Url,
    output: &Path,
    options: &DownloadOptions,
    on_progress: impl FnMut(Progress),
) -> Result<Summary, DownloadError> {
    let source = find(sources, url).ok_or_else(|| DownloadError::Unsupported {
        url: url.to_string(),
    })?;
    download_from(source, fetch, url, output, options, on_progress)
}

/// Download using an adapter that has already been resolved.
///
/// Public so a caller that has resolved the adapter (or a test with a fake one)
/// does not have to resolve it twice.
pub fn download_from(
    source: &dyn Source,
    fetch: &HttpClient,
    url: &Url,
    output: &Path,
    options: &DownloadOptions,
    mut on_progress: impl FnMut(Progress),
) -> Result<Summary, DownloadError> {
    let mut manifest = source.novel(fetch, url)?;

    // The rating is a property of the work, so it cannot be checked until the
    // index has been read. Nothing has been written by this point.
    if manifest.content_rating == ContentRating::Adult && !options.allow_adult {
        return Err(DownloadError::AdultRefused {
            url: url.to_string(),
        });
    }
    if let Some(max) = options.max_chapters {
        manifest.chapters.truncate(max);
    }

    let dir = NovelDir::at(output);
    dir.ensure_layout()?;
    let previous = previous_hashes(&dir, &manifest);

    // Decide before fetching, so the plan is honest and can be reported first.
    let skips: Vec<bool> = manifest
        .chapters
        .iter()
        .map(|chapter| {
            !options.force
                && previous
                    .get(&chapter.index)
                    .is_some_and(|expected| is_unchanged(&dir, chapter.index, expected))
        })
        .collect();

    // Carry the trusted hashes over, so a resumed directory keeps a complete
    // index even though those chapters are not fetched again.
    for (position, chapter) in manifest.chapters.iter_mut().enumerate() {
        if skips[position] {
            chapter.hash = previous.get(&chapter.index).cloned();
        }
    }

    on_progress(Progress::Planned {
        title: manifest.title.clone(),
        chapters: manifest.chapters.len(),
        to_fetch: skips.iter().filter(|skip| !**skip).count(),
    });

    // Durable from here on: an interrupted run resumes rather than restarts.
    dir.write_manifest(&manifest)?;

    let mut fetched = 0usize;
    let mut skipped = 0usize;
    let mut bytes_written = 0u64;

    for (position, skip) in skips.iter().enumerate() {
        let index = manifest.chapters[position].index;
        let title = manifest.chapters[position].title.clone();

        if *skip {
            skipped += 1;
            on_progress(Progress::Skipped { index, title });
            continue;
        }

        let chapter = source.chapter(fetch, &manifest.chapters[position])?;
        let written = dir.write_chapter(&chapter)?;
        bytes_written += written.bytes;

        // Record the hash before moving on, so a failure on the *next* chapter
        // does not throw this one away.
        manifest.chapters[position].hash = Some(written.hash);
        dir.write_manifest(&manifest)?;

        fetched += 1;
        on_progress(Progress::Fetched {
            index,
            title,
            bytes: written.bytes,
        });
    }

    Ok(Summary {
        title: manifest.title,
        chapters_total: manifest.chapters.len(),
        chapters_fetched: fetched,
        chapters_skipped: skipped,
        bytes_written,
        output_dir: output.to_path_buf(),
    })
}

/// The hashes a previous run recorded, if that manifest describes *this* novel.
///
/// A manifest for a different `novel_url` is ignored rather than trusted: the
/// chapter files in a directory may belong to something else entirely, and their
/// indices would line up misleadingly.
fn previous_hashes(dir: &NovelDir, manifest: &Manifest) -> HashMap<u32, String> {
    let Ok(previous) = dir.read_manifest() else {
        return HashMap::new();
    };
    if previous.source.novel_url != manifest.source.novel_url {
        return HashMap::new();
    }
    previous
        .chapters
        .into_iter()
        .filter_map(|chapter| chapter.hash.map(|hash| (chapter.index, hash)))
        .collect()
}

/// Whether the file on disk is still exactly what `expected` describes.
///
/// File existence is not enough. A truncated, corrupted, or hand-edited file
/// must be fetched again, and only the content hash can tell the difference.
fn is_unchanged(dir: &NovelDir, index: u32, expected: &str) -> bool {
    dir.read_chapter(index)
        .ok()
        .and_then(|chapter| hash_json(&chapter).ok())
        .is_some_and(|found| found == expected)
}
