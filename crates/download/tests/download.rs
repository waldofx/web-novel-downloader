//! The download orchestrator, driven by an in-memory adapter.
//!
//! These are the tests that pin down the *sequence*: what gets fetched, what
//! gets skipped, what lands on disk, and what happens when a run is interrupted
//! or refused. No network is involved.

mod common;

use common::{FakeSource, TempDir, client};
use url::Url;
use wnd_core::{ContentRating, NovelDir};
use wnd_download::{DownloadError, DownloadOptions, Progress, Summary, download_from};

fn url() -> Url {
    Url::parse("https://fake.test/novel/").unwrap()
}

fn options() -> DownloadOptions {
    DownloadOptions::default()
}

/// Run the orchestrator, collecting progress, and require success.
fn run(source: &FakeSource, dir: &TempDir, options: &DownloadOptions) -> (Summary, Vec<Progress>) {
    let mut progress = Vec::new();
    let summary = download_from(
        source,
        &client(),
        &Url::parse(&source.url).unwrap(),
        dir.path(),
        options,
        |event| progress.push(event),
    )
    .expect("download succeeds");
    (summary, progress)
}

fn fetched_indices(progress: &[Progress]) -> Vec<u32> {
    progress
        .iter()
        .filter_map(|event| match event {
            Progress::Fetched { index, .. } => Some(*index),
            _ => None,
        })
        .collect()
}

fn skipped_indices(progress: &[Progress]) -> Vec<u32> {
    progress
        .iter()
        .filter_map(|event| match event {
            Progress::Skipped { index } => Some(*index),
            _ => None,
        })
        .collect()
}

#[test]
fn a_fresh_download_fetches_every_chapter_and_records_a_hash() {
    let dir = TempDir::new("fresh");
    let source = FakeSource::new(3);

    let (summary, progress) = run(&source, &dir, &options());

    assert_eq!(summary.title, "偽の小説");
    assert_eq!(summary.chapters_total, 3);
    assert_eq!(summary.chapters_fetched, 3);
    assert_eq!(summary.chapters_skipped, 0);
    assert!(summary.bytes_written > 0);
    assert_eq!(summary.output_dir.as_path(), dir.path());

    assert_eq!(source.toc_fetches(), 1);
    assert_eq!(source.chapter_fetches(), 3);

    // The plan is announced first, then one event per chapter, in reading order.
    assert_eq!(
        progress[0],
        Progress::Planned {
            title: "偽の小説".to_string(),
            chapters: 3,
            to_fetch: 3,
        }
    );
    assert_eq!(fetched_indices(&progress), [1, 2, 3]);
    assert!(skipped_indices(&progress).is_empty());

    // Every chapter is on disk, and every hash is recorded — that is what makes
    // the next run cheap.
    let novel = NovelDir::at(dir.path());
    let manifest = novel.read_manifest().unwrap();
    assert_eq!(manifest.chapters.len(), 3);
    for chapter in &manifest.chapters {
        assert!(
            chapter
                .hash
                .as_deref()
                .is_some_and(|h| h.starts_with("sha256:")),
            "chapter {} has no recorded hash",
            chapter.index
        );
        assert!(novel.has_chapter(chapter.index));
    }
}

#[test]
fn a_second_run_skips_everything_already_downloaded() {
    let dir = TempDir::new("resume");
    let source = FakeSource::new(3);
    run(&source, &dir, &options());

    let (summary, progress) = run(&source, &dir, &options());

    assert_eq!(summary.chapters_fetched, 0);
    assert_eq!(summary.chapters_skipped, 3);
    assert_eq!(summary.bytes_written, 0, "nothing was written this time");
    assert_eq!(
        source.chapter_fetches(),
        3,
        "no chapter may be fetched twice"
    );
    assert_eq!(source.toc_fetches(), 2, "the index is always re-read");
    assert_eq!(
        progress[0],
        Progress::Planned {
            title: "偽の小説".to_string(),
            chapters: 3,
            to_fetch: 0,
        }
    );
    assert_eq!(skipped_indices(&progress), [1, 2, 3]);

    // Skipping must not lose the hashes the first run recorded.
    let manifest = NovelDir::at(dir.path()).read_manifest().unwrap();
    assert!(manifest.chapters.iter().all(|c| c.hash.is_some()));
}

#[test]
fn a_chapter_file_that_changed_on_disk_is_fetched_again() {
    let dir = TempDir::new("changed");
    let source = FakeSource::new(3);
    run(&source, &dir, &options());

    // Someone rewrote chapter 2 in place. It is still valid JSON, so only the
    // hash can tell that it is no longer what was fetched.
    let novel = NovelDir::at(dir.path());
    std::fs::write(
        novel.chapter_path(2),
        r#"{
  "format_version": 1,
  "index": 2,
  "title": "edited outside the tool",
  "nodes": [],
  "pre_note": [],
  "post_note": []
}
"#,
    )
    .unwrap();

    let before = source.chapter_fetches();
    let (summary, progress) = run(&source, &dir, &options());

    assert_eq!(summary.chapters_fetched, 1);
    assert_eq!(summary.chapters_skipped, 2);
    assert_eq!(fetched_indices(&progress), [2]);
    assert_eq!(source.chapter_fetches() - before, 1);

    let manifest = novel.read_manifest().unwrap();
    assert_eq!(manifest.chapters[1].title, "第2話");
    assert_eq!(
        novel.read_chapter(2).unwrap().title,
        "第2話",
        "the edited file is replaced by what the source serves"
    );
}

#[test]
fn a_corrupt_chapter_file_is_fetched_again() {
    let dir = TempDir::new("corrupt");
    let source = FakeSource::new(3);
    run(&source, &dir, &options());

    let novel = NovelDir::at(dir.path());
    std::fs::write(novel.chapter_path(2), b"this is not json").unwrap();

    let (summary, _) = run(&source, &dir, &options());

    assert_eq!(summary.chapters_fetched, 1);
    assert_eq!(summary.chapters_skipped, 2);
    assert_eq!(novel.read_chapter(2).unwrap().index, 2);
}

#[test]
fn force_re_fetches_chapters_that_are_already_present() {
    let dir = TempDir::new("force");
    let source = FakeSource::new(3);
    run(&source, &dir, &options());

    let before = source.chapter_fetches();
    let options = DownloadOptions {
        force: true,
        ..DownloadOptions::default()
    };
    let (summary, _) = run(&source, &dir, &options);

    assert_eq!(summary.chapters_fetched, 3);
    assert_eq!(summary.chapters_skipped, 0);
    assert_eq!(source.chapter_fetches() - before, 3);
}

#[test]
fn max_chapters_truncates_the_manifest_as_well_as_the_work() {
    let dir = TempDir::new("max");
    let source = FakeSource::new(5);
    let options = DownloadOptions {
        max_chapters: Some(2),
        ..DownloadOptions::default()
    };

    let (summary, _) = run(&source, &dir, &options);

    assert_eq!(summary.chapters_total, 2);
    assert_eq!(summary.chapters_fetched, 2);
    assert_eq!(summary.chapters_skipped, 0);

    let novel = NovelDir::at(dir.path());
    let manifest = novel.read_manifest().unwrap();
    assert_eq!(
        manifest.chapters.len(),
        2,
        "the truncated index is what gets recorded, so the next run does not resurrect it"
    );
    assert_eq!(manifest.chapters.last().unwrap().index, 2);
    assert!(!novel.has_chapter(3), "chapter 3 must never be fetched");
}

#[test]
fn an_interrupted_run_leaves_a_directory_that_resumes() {
    let dir = TempDir::new("interrupted");
    let mut source = FakeSource::new(5);
    source.fail_index = Some(4);

    let error =
        download_from(&source, &client(), &url(), dir.path(), &options(), |_| {}).unwrap_err();
    assert!(matches!(error, DownloadError::Source(_)), "got {error:?}");

    // The index is written before the first chapter, so what was fetched before
    // the failure is already recorded and will not be fetched again.
    let novel = NovelDir::at(dir.path());
    let manifest = novel.read_manifest().unwrap();
    assert_eq!(manifest.chapters.len(), 5);
    assert_eq!(
        manifest
            .chapters
            .iter()
            .filter(|chapter| chapter.hash.is_some())
            .count(),
        3,
        "chapters 1-3 are recorded; 4 failed and 5 was never attempted"
    );

    source.fail_index = None;
    let before = source.chapter_fetches();
    let (summary, progress) = run(&source, &dir, &options());

    assert_eq!(summary.chapters_fetched, 2, "only 4 and 5 are missing");
    assert_eq!(summary.chapters_skipped, 3);
    assert_eq!(fetched_indices(&progress), [4, 5]);
    assert_eq!(skipped_indices(&progress), [1, 2, 3]);
    assert_eq!(source.chapter_fetches() - before, 2);
}

#[test]
fn an_adult_novel_is_refused_before_anything_is_written() {
    let dir = TempDir::new("adult-refused");
    let source = FakeSource::adult(2);

    let error =
        download_from(&source, &client(), &url(), dir.path(), &options(), |_| {}).unwrap_err();

    assert!(
        matches!(error, DownloadError::AdultRefused { .. }),
        "got {error:?}"
    );
    assert_eq!(source.chapter_fetches(), 0);
    assert_eq!(
        source.toc_fetches(),
        1,
        "the per-work rating is only known after the table of contents is read"
    );
    assert!(
        !NovelDir::at(dir.path()).manifest_path().exists(),
        "a refused download must leave nothing behind"
    );
}

#[test]
fn an_adult_novel_is_accepted_when_explicitly_allowed() {
    let dir = TempDir::new("adult-allowed");
    let source = FakeSource::adult(2);
    let options = DownloadOptions {
        allow_adult: true,
        ..DownloadOptions::default()
    };

    let (summary, _) = run(&source, &dir, &options);

    assert_eq!(summary.chapters_fetched, 2);
    assert_eq!(
        NovelDir::at(dir.path())
            .read_manifest()
            .unwrap()
            .content_rating,
        ContentRating::Adult,
        "the rating is preserved in the manifest"
    );
}

#[test]
fn a_manifest_for_a_different_novel_does_not_make_chapters_look_present() {
    let dir = TempDir::new("other-novel");
    let first = FakeSource::new(2);
    run(&first, &dir, &options());

    // A different novel reusing the same output directory. Its chapter files
    // share the same paths, but its manifest does not describe them.
    let mut second = FakeSource::new(2);
    second.url = "https://fake.test/other/".to_string();

    let (summary, progress) = run(&second, &dir, &options());

    assert_eq!(
        summary.chapters_skipped, 0,
        "another novel's hashes must not count"
    );
    assert_eq!(fetched_indices(&progress), [1, 2]);
}

#[test]
fn a_url_no_adapter_claims_is_unsupported() {
    let dir = TempDir::new("unsupported");
    let target = Url::parse("https://example.invalid/novel/1").unwrap();

    let error = wnd_download::download(
        &wnd_sources::registry(),
        &client(),
        &target,
        dir.path(),
        &options(),
        |_| {},
    )
    .unwrap_err();

    assert!(
        matches!(error, DownloadError::Unsupported { .. }),
        "got {error:?}"
    );
}
