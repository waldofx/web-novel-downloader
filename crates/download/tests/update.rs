//! `nvd update`, driven by an in-memory adapter.
//!
//! The rule these pin down: a chapter is fetched when it is **new**, when the
//! index says it was **revised**, or — with `--check` — when its content really
//! does differ; and never otherwise.

mod common;

use common::{FakeSource, TempDir, client};
use url::Url;
use wnd_core::{Manifest, NovelDir, SourceInfo};
use wnd_download::{
    DownloadError, DownloadOptions, Progress, UpdateOptions, UpdateSummary, download_from,
    update_from,
};

fn options() -> UpdateOptions {
    UpdateOptions::default()
}

fn checking() -> UpdateOptions {
    UpdateOptions {
        check: true,
        ..UpdateOptions::default()
    }
}

/// Download the whole novel once, the way a user would have.
fn download_first(source: &FakeSource, dir: &TempDir) {
    download_from(
        source,
        &client(),
        &Url::parse(&source.url).unwrap(),
        dir.path(),
        &DownloadOptions::default(),
        |_| {},
    )
    .expect("the first download succeeds");
}

/// Update, collecting progress, and require success.
fn run(
    source: &FakeSource,
    dir: &TempDir,
    options: &UpdateOptions,
) -> (UpdateSummary, Vec<Progress>) {
    let mut progress = Vec::new();
    let summary = update_from(source, &client(), dir.path(), options, |event| {
        progress.push(event)
    })
    .expect("the update succeeds");
    (summary, progress)
}

fn indices(progress: &[Progress], wanted: fn(&Progress) -> Option<u32>) -> Vec<u32> {
    progress.iter().filter_map(wanted).collect()
}

fn fetched(progress: &[Progress]) -> Vec<u32> {
    indices(progress, |event| match event {
        Progress::Fetched { index, .. } => Some(*index),
        _ => None,
    })
}

fn skipped(progress: &[Progress]) -> Vec<u32> {
    indices(progress, |event| match event {
        Progress::Skipped { index, .. } => Some(*index),
        _ => None,
    })
}

fn checked(progress: &[Progress]) -> Vec<u32> {
    indices(progress, |event| match event {
        Progress::Checked { index, .. } => Some(*index),
        _ => None,
    })
}

#[test]
fn an_up_to_date_directory_fetches_nothing() {
    let dir = TempDir::new("update-current");
    let source = FakeSource::new(3);
    download_first(&source, &dir);

    let before = source.chapter_fetches();
    let (summary, progress) = run(&source, &dir, &options());

    assert_eq!(summary.chapters_total, 3);
    assert_eq!(summary.chapters_fetched, 0);
    assert_eq!(summary.chapters_skipped, 3);
    assert_eq!(summary.chapters_checked, 0);
    assert_eq!(summary.bytes_written, 0, "nothing was written this time");
    assert!(summary.removed.is_empty());
    assert!(summary.up_to_date(), "so a scheduled run can tell");
    assert_eq!(
        source.chapter_fetches(),
        before,
        "an up-to-date update must not touch a chapter page"
    );
    assert_eq!(source.toc_fetches(), 2, "the index is always re-read");

    assert!(
        fetched(&progress).is_empty(),
        "nothing to fetch: {progress:?}"
    );
    assert_eq!(skipped(&progress), [1, 2, 3]);
}

#[test]
fn a_new_chapter_is_fetched_and_the_rest_are_left_alone() {
    let dir = TempDir::new("update-new");
    let source = FakeSource::new(3);
    download_first(&source, &dir);
    source.add_chapter(4);

    let before = source.chapter_fetches();
    let (summary, progress) = run(&source, &dir, &options());

    assert_eq!(summary.chapters_total, 4);
    assert_eq!(summary.chapters_fetched, 1);
    assert_eq!(summary.chapters_skipped, 3);
    assert_eq!(fetched(&progress), [4]);
    assert_eq!(skipped(&progress), [1, 2, 3]);
    assert_eq!(source.chapter_fetches() - before, 1);

    let manifest = NovelDir::at(dir.path()).read_manifest().unwrap();
    assert_eq!(manifest.chapters.len(), 4);
    assert!(
        manifest.chapters[3].hash.is_some(),
        "the new hash is recorded"
    );
}

#[test]
fn a_chapter_the_index_says_was_revised_is_fetched_again() {
    let dir = TempDir::new("update-revised");
    let source = FakeSource::new(3);
    download_first(&source, &dir);
    source.revise_chapter(2, "2024-05-06T07:08:00+09:00");

    let before = source.chapter_fetches();
    let (summary, progress) = run(&source, &dir, &options());

    assert_eq!(summary.chapters_fetched, 1);
    assert_eq!(fetched(&progress), [2], "only the revised chapter");
    assert_eq!(skipped(&progress), [1, 3]);
    assert_eq!(source.chapter_fetches() - before, 1);

    // The stamp is recorded, so the *next* run has nothing to do.
    let manifest = NovelDir::at(dir.path()).read_manifest().unwrap();
    assert_eq!(
        manifest.chapters[1].revised_at.as_deref(),
        Some("2024-05-06T07:08:00+09:00")
    );

    let (summary, _) = run(&source, &dir, &options());
    assert!(summary.up_to_date(), "a recorded stamp is stable");
    assert_eq!(source.chapter_fetches() - before, 1);
}

#[test]
fn a_revision_stamp_that_arrives_later_on_an_older_directory_is_not_trusted() {
    // A directory downloaded before the tool recorded revision stamps has none.
    // An index that now says "revised" cannot be compared against nothing, and
    // "cannot tell" is not "unchanged": the chapter is fetched once, and the
    // stamp is recorded so that it is the last time.
    let dir = TempDir::new("update-old-directory");
    let source = FakeSource::new(2);
    download_first(&source, &dir);

    let novel = NovelDir::at(dir.path());
    let mut manifest = novel.read_manifest().unwrap();
    for chapter in &mut manifest.chapters {
        chapter.revised_at = None;
    }
    novel.write_manifest(&manifest).unwrap();
    source.revise_chapter(1, "2024-05-06T07:08:00+09:00");

    let (summary, progress) = run(&source, &dir, &options());

    assert_eq!(summary.chapters_fetched, 1);
    assert_eq!(fetched(&progress), [1]);
    assert_eq!(
        novel.read_manifest().unwrap().chapters[0]
            .revised_at
            .as_deref(),
        Some("2024-05-06T07:08:00+09:00")
    );

    let (summary, _) = run(&source, &dir, &options());
    assert!(summary.up_to_date());
}

#[test]
fn an_edit_the_index_cannot_announce_is_found_only_with_check() {
    let dir = TempDir::new("update-check");
    let source = FakeSource::new(3);
    download_first(&source, &dir);
    // The author edits a chapter and the site says nothing about it — which is
    // every edit on Kakuyomu.
    source.edit_chapter(2, "rewritten, and no timestamp moved");

    let before = source.chapter_fetches();
    let (summary, _) = run(&source, &dir, &options());
    assert!(
        summary.up_to_date(),
        "the index says there is nothing to do"
    );
    assert_eq!(source.chapter_fetches(), before, "so nothing is fetched");

    let (summary, progress) = run(&source, &dir, &checking());

    assert_eq!(summary.chapters_checked, 2, "fetched, compared, unchanged");
    assert_eq!(summary.chapters_fetched, 1, "fetched, compared, changed");
    assert_eq!(
        summary.chapters_skipped, 0,
        "--check asks the site about all"
    );
    assert_eq!(checked(&progress), [1, 3]);
    assert_eq!(fetched(&progress), [2]);
    assert_eq!(
        source.chapter_fetches() - before,
        3,
        "--check fetches every chapter once"
    );

    // The rewritten chapter is on disk, and the two that matched were not
    // rewritten at all.
    let novel = NovelDir::at(dir.path());
    assert_eq!(
        novel.read_chapter(2).unwrap().translatable_texts().next(),
        Some("rewritten, and no timestamp moved")
    );
    assert_eq!(
        summary.bytes_written,
        std::fs::metadata(novel.chapter_path(2)).unwrap().len(),
        "only the chapter that changed was written"
    );

    let (summary, _) = run(&source, &dir, &checking());
    assert!(summary.up_to_date(), "and then it is up to date again");
    assert_eq!(summary.chapters_checked, 3);
}

#[test]
fn a_chapter_dropped_from_the_index_is_reported_and_left_on_disk() {
    let dir = TempDir::new("update-removed");
    let source = FakeSource::new(3);
    download_first(&source, &dir);
    // The last chapter, so nothing after it shifts: the chapter that is gone is
    // the only thing this run has to say.
    source.remove_chapter(3);

    let (summary, progress) = run(&source, &dir, &options());

    assert_eq!(summary.chapters_total, 2);
    assert_eq!(skipped(&progress), [1, 2]);
    assert_eq!(
        summary
            .removed
            .iter()
            .map(|gone| (gone.index, gone.title.as_str()))
            .collect::<Vec<_>>(),
        [(3, "第3話")],
        "the chapter that is gone from the index, named"
    );
    assert_eq!(summary.chapters_fetched, 0, "nothing new to fetch");
    assert!(
        !summary.up_to_date(),
        "the index changed: the chapters that disappeared were a change too"
    );
    assert!(
        NovelDir::at(dir.path()).has_chapter(2),
        "an update never deletes what the user already has"
    );
    assert!(
        NovelDir::at(dir.path())
            .read_manifest()
            .unwrap()
            .chapter(3)
            .is_none(),
        "but the index no longer describes it"
    );
}

#[test]
fn a_chapter_inserted_in_the_middle_is_not_mistaken_for_unchanged() {
    // Kakuyomu numbers episodes by position and gives each an opaque id, so an
    // episode added in the middle shifts every later chapter's number while
    // leaving the files on disk exactly as they were. A comparison that trusted
    // the hash alone would skip them and keep the wrong text under every later
    // number.
    let dir = TempDir::new("update-inserted");
    let source = FakeSource::new(3);
    download_first(&source, &dir);
    source.insert_chapter(2, 9);

    let (summary, progress) = run(&source, &dir, &options());

    assert_eq!(summary.chapters_total, 4);
    assert_eq!(fetched(&progress), [2, 3, 4]);
    assert_eq!(skipped(&progress), [1], "chapter 1 did not move");
    assert!(
        summary.removed.is_empty(),
        "nothing was deleted: the chapters only moved"
    );

    // Chapter 2 is the inserted one, and 3 and 4 are the chapters that used to
    // be 2 and 3 — content and index together, not content under a stale number.
    let novel = NovelDir::at(dir.path());
    let manifest = novel.read_manifest().unwrap();
    assert_eq!(manifest.chapters[1].url, "https://fake.test/novel/9/");
    assert_eq!(manifest.chapters[2].url, "https://fake.test/novel/2/");
    assert_eq!(
        novel.read_chapter(3).unwrap().translatable_texts().next(),
        Some("body of chapter 2")
    );
}

#[test]
fn an_adult_directory_is_refused_before_the_index_is_read() {
    let dir = TempDir::new("update-adult");
    let source = FakeSource::adult(2);
    download_from(
        &source,
        &client(),
        &Url::parse(&source.url).unwrap(),
        dir.path(),
        &DownloadOptions {
            allow_adult: true,
            ..DownloadOptions::default()
        },
        |_| {},
    )
    .expect("the first download succeeds");

    let before = source.toc_fetches();
    let error = update_from(&source, &client(), dir.path(), &options(), |_| {}).unwrap_err();

    assert!(
        matches!(error, DownloadError::AdultRefused { .. }),
        "got {error:?}"
    );
    assert_eq!(
        source.toc_fetches(),
        before,
        "the stored rating is known without asking the site anything"
    );

    let (summary, _) = run(
        &source,
        &dir,
        &UpdateOptions {
            allow_adult: true,
            ..UpdateOptions::default()
        },
    );
    assert!(summary.up_to_date());
}

#[test]
fn a_directory_that_is_not_a_novel_directory_is_named() {
    let dir = TempDir::new("update-not-a-novel");
    let source = FakeSource::new(1);

    let error = update_from(&source, &client(), dir.path(), &options(), |_| {}).unwrap_err();

    assert!(
        error.to_string().contains("manifest not found"),
        "got {error}"
    );
    assert!(
        error
            .to_string()
            .contains(&dir.path().to_string_lossy().to_string()),
        "the message must name the directory: {error}"
    );
    assert_eq!(source.toc_fetches(), 0, "and nothing was fetched");
}

#[test]
fn a_manifest_whose_url_no_adapter_handles_is_unsupported() {
    let dir = TempDir::new("update-unsupported");
    let manifest = Manifest::new(
        SourceInfo {
            id: "gone".into(),
            novel_url: "https://example.invalid/novel/1".into(),
            site_title: None,
        },
        "誰も知らない小説",
        "ja",
    );
    NovelDir::at(dir.path()).write_manifest(&manifest).unwrap();

    let error = wnd_download::update(
        &wnd_sources::registry(),
        &client(),
        dir.path(),
        &options(),
        |_| {},
    )
    .unwrap_err();

    assert!(
        matches!(error, DownloadError::Unsupported { .. }),
        "got {error:?}"
    );
    assert!(error.to_string().contains("example.invalid"), "got {error}");
}

#[test]
fn an_update_keeps_what_the_index_cannot_know() {
    // `cover` is a path to a file in the directory, not something the index can
    // describe. An update must not erase it just because the adapter does not
    // mention it.
    let dir = TempDir::new("update-cover");
    let source = FakeSource::new(2);
    download_first(&source, &dir);

    let novel = NovelDir::at(dir.path());
    let mut manifest = novel.read_manifest().unwrap();
    manifest.cover = Some("images/cover.png".into());
    novel.write_manifest(&manifest).unwrap();

    let (summary, _) = run(&source, &dir, &options());

    assert!(summary.up_to_date());
    assert_eq!(
        novel.read_manifest().unwrap().cover.as_deref(),
        Some("images/cover.png")
    );
}
