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
//! 2. Nothing is gated on rating — each site enforces its own age gate, so an
//!    adult work downloads like any other.
//! 3. Decide what to skip. That decision is reported, so a front end can show
//!    "2 of 89" before the first request rather than after it.
//! 4. Write the index, then fetch. The manifest is durable before the first
//!    chapter and rewritten after each one, so an interrupted run leaves a
//!    valid directory that resumes — and a chapter is only skipped when the
//!    file on disk still hashes to what the manifest recorded.
//!
//! # Updating, which is the same sequence with a memory
//!
//! [`update`] reads the novel's URL out of the directory's own manifest, so it
//! is a thing done *to a directory* rather than to a URL. It then re-reads the
//! index and diffs it against what the last run recorded:
//!
//! - a **new** chapter is fetched;
//! - a chapter the index says was **revised** is fetched, and so is one whose
//!   revision stamp cannot be compared (`--check` covers the rest, at one
//!   request per chapter: some sites publish no revision stamp at all);
//! - a chapter that **disappeared** from the index is reported and left on disk;
//! - everything else is left alone.
//!
//! The comparison is by chapter **url**, not by number, because Kakuyomu numbers
//! episodes by position: an episode inserted in the middle moves every later
//! number while none of them is a different chapter.

use std::path::{Path, PathBuf};

use url::Url;
use wnd_core::store::storage::hash_json;
use wnd_core::{HttpClient, Manifest, NovelDir};
use wnd_sources::{Source, SourceError, find};

/// What to fetch, and how careful to be.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DownloadOptions {
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
    /// One chapter was fetched to be compared and turned out to be unchanged
    /// (`nvd update --check`). Nothing was written.
    Checked { index: u32, title: String },
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

    /// No adapter claims the URL.
    #[error("no source adapter handles {url}")]
    Unsupported { url: String },
}

/// What an update is allowed to do.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpdateOptions {
    /// Fetch every chapter and compare it with what is on disk, rewriting only
    /// the ones that really differ.
    ///
    /// This costs one request per chapter, so it is opt-in. It is the only way
    /// to find an edit on a site that publishes no revision stamp: Kakuyomu's
    /// work page gives `{id, title, publishedAt}` per episode and nothing that
    /// changes when the author edits one.
    pub check: bool,
}

/// A chapter the directory has and the index no longer lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovedChapter {
    pub index: u32,
    pub title: String,
}

/// What an update did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateSummary {
    pub title: String,
    /// Chapters the index now lists.
    pub chapters_total: usize,
    /// Chapters written: new, revised, or (with `--check`) edited.
    pub chapters_fetched: usize,
    /// Chapters that were correct as far as the index could tell, and were not
    /// requested at all.
    pub chapters_skipped: usize,
    /// Chapters fetched only to compare, and found unchanged — so
    /// `chapters_fetched + chapters_skipped + chapters_checked ==
    /// chapters_total`.
    pub chapters_checked: usize,
    /// Chapters in the directory that the index no longer lists. Never deleted.
    pub removed: Vec<RemovedChapter>,
    /// Bytes of chapter content written. Manifest bookkeeping is not counted.
    pub bytes_written: u64,
    pub output_dir: PathBuf,
}

impl UpdateSummary {
    /// Whether the run found nothing at all: nothing to fetch, nothing gone.
    ///
    /// A scheduled run can tell this apart from a failure without reading the
    /// output — it is what `nvd update` exits `3` for.
    pub fn up_to_date(&self) -> bool {
        self.chapters_fetched == 0 && self.removed.is_empty()
    }
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

    if let Some(max) = options.max_chapters {
        manifest.chapters.truncate(max);
    }

    // `force` means "ignore what is on disk", so the previous manifest is simply
    // not consulted: the plan then fetches every chapter.
    let dir = NovelDir::at(output);
    let previous = if options.force {
        None
    } else {
        dir.read_manifest().ok()
    };

    let outcome = run(
        source,
        fetch,
        output,
        manifest,
        previous.as_ref(),
        false,
        &mut on_progress,
    )?;

    Ok(Summary {
        title: outcome.title,
        chapters_total: outcome.total,
        chapters_fetched: outcome.fetched,
        chapters_skipped: outcome.skipped,
        bytes_written: outcome.bytes_written,
        output_dir: output.to_path_buf(),
    })
}

/// Bring a directory that `nvd download` wrote up to date with the site.
///
/// The novel comes from the directory's own manifest, so no URL is needed: an
/// update is a thing you do *to a directory*.
pub fn update(
    sources: &[Box<dyn Source>],
    fetch: &HttpClient,
    dir: &Path,
    options: &UpdateOptions,
    on_progress: impl FnMut(Progress),
) -> Result<UpdateSummary, DownloadError> {
    let stored = NovelDir::at(dir).read_manifest()?;
    let url = novel_url(&stored)?;
    let source = find(sources, &url).ok_or_else(|| DownloadError::Unsupported {
        url: url.to_string(),
    })?;
    update_with(source, fetch, dir, stored, options, on_progress)
}

/// Update using an adapter that has already been resolved.
pub fn update_from(
    source: &dyn Source,
    fetch: &HttpClient,
    dir: &Path,
    options: &UpdateOptions,
    on_progress: impl FnMut(Progress),
) -> Result<UpdateSummary, DownloadError> {
    let stored = NovelDir::at(dir).read_manifest()?;
    update_with(source, fetch, dir, stored, options, on_progress)
}

fn update_with(
    source: &dyn Source,
    fetch: &HttpClient,
    dir: &Path,
    stored: Manifest,
    options: &UpdateOptions,
    mut on_progress: impl FnMut(Progress),
) -> Result<UpdateSummary, DownloadError> {
    let url = novel_url(&stored)?;

    let mut manifest = source.novel(fetch, &url)?;

    // What the index cannot describe: `cover` is a path to a file inside the
    // directory, which no adapter can know. Dropping it would erase a fact the
    // directory has, so an adapter's silence on it is not a removal.
    if manifest.cover.is_none() {
        manifest.cover = stored.cover.clone();
    }
    let removed = removed_chapters(&stored, &manifest);

    let outcome = run(
        source,
        fetch,
        dir,
        manifest,
        Some(&stored),
        options.check,
        &mut on_progress,
    )?;

    Ok(UpdateSummary {
        title: outcome.title,
        chapters_total: outcome.total,
        chapters_fetched: outcome.fetched,
        chapters_skipped: outcome.skipped,
        chapters_checked: outcome.checked,
        removed,
        bytes_written: outcome.bytes_written,
        output_dir: dir.to_path_buf(),
    })
}

/// The novel a directory belongs to.
fn novel_url(manifest: &Manifest) -> Result<Url, DownloadError> {
    Url::parse(&manifest.source.novel_url).map_err(|error| {
        DownloadError::Core(wnd_core::Error::InvalidUrl(format!(
            "{}: {error}",
            manifest.source.novel_url
        )))
    })
}

/// Chapters the directory has that the index no longer lists.
///
/// Matched by **url**, not by number: a chapter inserted in the middle of a
/// Kakuyomu work moves every later number without any chapter being gone, and
/// calling those "removed" would be a lie that a scheduled run acts on.
fn removed_chapters(stored: &Manifest, fresh: &Manifest) -> Vec<RemovedChapter> {
    stored
        .chapters
        .iter()
        .filter(|chapter| !fresh.chapters.iter().any(|now| now.url == chapter.url))
        .map(|chapter| RemovedChapter {
            index: chapter.index,
            title: chapter.title.clone(),
        })
        .collect()
}

/// What is to be done with one chapter of the index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Plan {
    /// Not on disk, changed since it was written, or the file no longer hashes
    /// to what the manifest recorded. It is fetched and written.
    Fetch,
    /// On disk and correct as far as anything knows, but the caller asked for
    /// every chapter to be compared. It is fetched and written **only** if what
    /// arrives differs.
    Check,
    /// On disk, correct, and nothing says otherwise: not requested at all.
    Skip,
}

/// What a run produced, before it is shaped into a [`Summary`] or an
/// [`UpdateSummary`].
struct Outcome {
    title: String,
    total: usize,
    fetched: usize,
    skipped: usize,
    checked: usize,
    bytes_written: u64,
}

/// The part every run shares: decide from the index and what is on disk, then
/// fetch what the decision says.
///
/// `previous` is the manifest of the last run *for this novel*, or `None` when
/// there is nothing to resume against. It is where the recorded hashes and
/// revision stamps come from, and the reason a second run does almost nothing.
fn run(
    source: &dyn Source,
    fetch: &HttpClient,
    output: &Path,
    mut manifest: Manifest,
    previous: Option<&Manifest>,
    check: bool,
    on_progress: &mut impl FnMut(Progress),
) -> Result<Outcome, DownloadError> {
    let dir = NovelDir::at(output);
    dir.ensure_layout()?;

    // A manifest for a *different* novel describes files that are not this
    // one's, however well the numbers happen to line up.
    let previous = previous.filter(|stored| stored.source.novel_url == manifest.source.novel_url);
    let stored_chapter =
        |chapter: &wnd_core::ChapterMeta| previous.and_then(|s| s.chapter(chapter.index));

    let plan: Vec<Plan> = manifest
        .chapters
        .iter()
        .map(|chapter| {
            match stored_chapter(chapter) {
                // The same chapter under the same number, unchanged on the site
                // and still exactly what the recorded hash describes.
                Some(stored)
                    if stored.url == chapter.url
                        && stored.revised_at == chapter.revised_at
                        && stored
                            .hash
                            .as_deref()
                            .is_some_and(|hash| is_unchanged(&dir, chapter.index, hash)) =>
                {
                    if check { Plan::Check } else { Plan::Skip }
                }
                _ => Plan::Fetch,
            }
        })
        .collect();

    // Carry the trusted hashes over, so a directory keeps a complete index even
    // though those chapters are not fetched again.
    for (position, chapter) in manifest.chapters.iter_mut().enumerate() {
        if plan[position] != Plan::Fetch {
            chapter.hash = stored_chapter(chapter).and_then(|stored| stored.hash.clone());
        }
    }

    on_progress(Progress::Planned {
        title: manifest.title.clone(),
        chapters: manifest.chapters.len(),
        to_fetch: plan.iter().filter(|step| **step != Plan::Skip).count(),
    });

    // Durable from here on: an interrupted run resumes rather than restarts.
    dir.write_manifest(&manifest)?;

    let mut outcome = Outcome {
        title: manifest.title.clone(),
        total: manifest.chapters.len(),
        fetched: 0,
        skipped: 0,
        checked: 0,
        bytes_written: 0,
    };

    for (position, step) in plan.iter().enumerate() {
        let index = manifest.chapters[position].index;
        let title = manifest.chapters[position].title.clone();

        if *step == Plan::Skip {
            outcome.skipped += 1;
            on_progress(Progress::Skipped { index, title });
            continue;
        }

        let chapter = source.chapter(fetch, &manifest.chapters[position])?;

        // A checked chapter is written only if it is not what the manifest
        // already describes.
        if *step == Plan::Check {
            let arrived = hash_json(&chapter)?;
            if manifest.chapters[position].hash.as_deref() == Some(arrived.as_str()) {
                outcome.checked += 1;
                on_progress(Progress::Checked { index, title });
                continue;
            }
        }

        let written = dir.write_chapter(&chapter)?;
        outcome.bytes_written += written.bytes;

        // Record the hash before moving on, so a failure on the *next* chapter
        // does not throw this one away.
        manifest.chapters[position].hash = Some(written.hash);
        dir.write_manifest(&manifest)?;

        outcome.fetched += 1;
        on_progress(Progress::Fetched {
            index,
            title,
            bytes: written.bytes,
        });
    }

    Ok(outcome)
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
