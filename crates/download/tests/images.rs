//! Fetching the pictures a body and a manifest point at.
//!
//! These live behind a real socket, because that is the only place the questions
//! can be asked: whether the fetcher sends what the site demands, whether it
//! believes the media type over the URL, and whether a picture that is not there
//! costs the chapter it belongs to. Everything *around* the fetch — the naming,
//! the manifest, the resume rule — is checked on what lands on disk.

mod common;

use common::{FakeSource, Image, ImageServer, TempDir, local_client};
use url::Url;
use wnd_core::{Block, Chapter, NovelDir};
use wnd_download::{DownloadOptions, Progress, UpdateOptions, download_from, update_from};

fn download(source: &FakeSource, dir: &TempDir) -> (wnd_download::Summary, Vec<Progress>) {
    let mut progress = Vec::new();
    let summary = download_from(
        source,
        &local_client(),
        &Url::parse(&source.url).unwrap(),
        dir.path(),
        &DownloadOptions::default(),
        |event| progress.push(event),
    )
    .expect("download succeeds");
    (summary, progress)
}

fn update(source: &FakeSource, dir: &TempDir, check: bool) -> wnd_download::UpdateSummary {
    update_from(
        source,
        &local_client(),
        dir.path(),
        &UpdateOptions { check },
        |_| {},
    )
    .expect("update succeeds")
}

/// What a stored chapter says its pictures are.
fn image_sources(chapter: &Chapter) -> Vec<String> {
    chapter
        .nodes
        .iter()
        .filter_map(|block| match block {
            Block::Image { src, .. } => Some(src.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_cover_and_a_body_image_become_files_the_chapter_points_at() {
    let dir = TempDir::new("images-store");
    let server = ImageServer::start(Image::png("PNG-illustration"));

    let cover = server.url("cover.png");
    let illustration = server.url("illust/1/a.png");
    let source = FakeSource::new(1)
        .with_cover(&cover)
        .with_referer("https://fake.test/");
    source.image(1, &illustration);

    let (summary, progress) = download(&source, &dir);

    assert_eq!(summary.images_fetched, 2);
    assert_eq!(summary.images_missing, 0);

    let novel = NovelDir::at(dir.path());
    let manifest = novel.read_manifest().unwrap();

    // The bytes are on disk under names that say what they are…
    assert_eq!(
        novel.read_image("images/cover.png").unwrap(),
        b"PNG-illustration"
    );
    assert_eq!(
        novel.read_image("images/0001-1.png").unwrap(),
        b"PNG-illustration"
    );

    // …the manifest records where each came from and what it hashes to…
    assert_eq!(manifest.cover.as_deref(), Some("images/cover.png"));
    assert_eq!(manifest.cover_url.as_deref(), Some(cover.as_str()));
    let recorded = manifest
        .image(&illustration)
        .expect("the illustration is recorded");
    assert_eq!(recorded.file, "images/0001-1.png");
    assert_eq!(
        recorded.hash,
        wnd_core::store::storage::hash_bytes(b"PNG-illustration")
    );

    // …and the chapter, which is what a renderer reads, holds a path, not a URL.
    let chapter = novel.read_chapter(1).unwrap();
    assert_eq!(image_sources(&chapter), ["images/0001-1.png"]);

    // Every request said where it came from. `i.pximg.net` answers 403 without
    // it, and this is the only thing the orchestrator adds to a request.
    let seen = server.requests();
    assert_eq!(seen.len(), 2);
    assert!(
        seen.iter()
            .all(|request| request.referer.as_deref() == Some("https://fake.test/")),
        "the site's images were asked for without a referer: {seen:?}"
    );

    // Both are reported, cover first, because it is fetched before the chapters.
    let stored: Vec<&str> = progress
        .iter()
        .filter_map(|event| match event {
            Progress::ImageFetched { file, .. } => Some(file.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(stored, ["images/cover.png", "images/0001-1.png"]);
}

#[test]
fn an_image_already_stored_is_not_fetched_again() {
    let dir = TempDir::new("images-resume");
    let server = ImageServer::start(Image::png("PNG"));
    let source = FakeSource::new(1).with_cover(&server.url("cover.png"));
    source.image(1, &server.url("a.png"));

    download(&source, &dir);
    assert_eq!(server.request_count(), 2);

    // A second run has nothing to do: the chapter is skipped, and the cover —
    // the one image an otherwise up-to-date directory can lack — is already
    // there. Not one request is made.
    let (summary, _) = download(&source, &dir);
    assert_eq!(summary.chapters_skipped, 1);
    assert_eq!(summary.images_fetched, 0);
    assert_eq!(summary.bytes_written, 0);
    assert_eq!(server.request_count(), 2, "nothing was asked for twice");

    // An update agrees, and says so with the exit code it is written for.
    let summary = update(&source, &dir, false);
    assert!(summary.up_to_date());
    assert_eq!(summary.images_fetched, 0);
    assert_eq!(server.request_count(), 2);
}

#[test]
fn a_checked_chapter_compares_the_stored_paths_so_nothing_looks_changed() {
    let dir = TempDir::new("images-check");
    let server = ImageServer::start(Image::png("PNG"));
    let source = FakeSource::new(1);
    source.image(1, &server.url("a.png"));
    download(&source, &dir);
    let after_download = server.request_count();

    let summary = update(&source, &dir, true);

    // Fetched and compared against a chapter that says `images/0001-1.png`
    // because it was stored that way. The comparison has to be like for like, or
    // every illustration would look like an edit on every run.
    assert_eq!(summary.chapters_checked, 1);
    assert_eq!(summary.chapters_fetched, 0);
    assert_eq!(summary.images_fetched, 0);
    assert!(summary.up_to_date());
    assert_eq!(server.request_count(), after_download);
}

#[test]
fn an_image_deleted_by_hand_is_fetched_again() {
    let dir = TempDir::new("images-repair");
    let server = ImageServer::start(Image::png("PNG"));
    let source = FakeSource::new(1);
    source.image(1, &server.url("a.png"));
    download(&source, &dir);
    let after_download = server.request_count();

    // The chapter still matches its hash, so the plan skips it and would never
    // look at the picture again. The repair pass is what notices.
    std::fs::remove_file(dir.path().join("images/0001-1.png")).unwrap();

    let summary = update(&source, &dir, false);

    assert_eq!(summary.images_fetched, 1);
    assert_eq!(
        summary.chapters_fetched, 0,
        "the chapter itself was untouched"
    );
    assert!(
        !summary.up_to_date(),
        "a picture came back, and that is news"
    );
    assert_eq!(server.request_count(), after_download + 1);
    assert_eq!(
        NovelDir::at(dir.path())
            .read_image("images/0001-1.png")
            .unwrap(),
        b"PNG"
    );
}

#[test]
fn an_image_the_server_does_not_have_is_reported_and_the_chapter_survives() {
    let dir = TempDir::new("images-404");
    let server = ImageServer::start(Image::not_found());
    let source = FakeSource::new(1).with_cover(&server.url("cover.png"));
    source.image(1, &server.url("gone.png"));

    let (summary, progress) = download(&source, &dir);

    assert_eq!(summary.images_missing, 2, "the cover and the illustration");
    assert_eq!(summary.images_fetched, 0);
    assert_eq!(
        summary.chapters_fetched, 1,
        "a missing picture must not cost the chapter it belongs to"
    );

    let novel = NovelDir::at(dir.path());
    let manifest = novel.read_manifest().unwrap();
    assert!(manifest.images.is_empty());
    assert!(
        manifest.cover.is_none(),
        "no path to a file that is not there"
    );

    // What is left in the chapter is the URL — the only record of what the site
    // offered, and what a later run tries again from.
    let chapter = novel.read_chapter(1).unwrap();
    assert_eq!(image_sources(&chapter), [server.url("gone.png")]);

    let missing: Vec<&str> = progress
        .iter()
        .filter_map(|event| match event {
            Progress::ImageMissing { url, .. } => Some(url.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(missing.len(), 2);
    for event in &progress {
        if let Progress::ImageMissing { reason, .. } = event {
            assert!(
                reason.contains("404"),
                "the reason names what happened: {reason}"
            );
        }
    }
}

#[test]
fn a_page_where_a_picture_belongs_is_not_stored() {
    // A filter can answer an image URL with a success status and a page. Storing
    // that under `images/` would put a filter notice inside someone's EPUB.
    let dir = TempDir::new("images-page");
    let server = ImageServer::start(Image::a_page());
    let source = FakeSource::new(1);
    source.image(1, &server.url("picture.png"));

    let (summary, progress) = download(&source, &dir);

    assert_eq!(summary.images_missing, 1);
    assert_eq!(summary.images_fetched, 0);
    assert!(!dir.path().join("images/0001-1.png").exists());
    assert!(
        NovelDir::at(dir.path())
            .read_manifest()
            .unwrap()
            .images
            .is_empty()
    );

    let reason = progress
        .iter()
        .find_map(|event| match event {
            Progress::ImageMissing { reason, .. } => Some(reason.as_str()),
            _ => None,
        })
        .expect("the page is reported");
    assert!(reason.contains("text/html"), "{reason}");
}

#[test]
fn the_extension_follows_the_media_type_and_falls_back_to_the_url() {
    let dir = TempDir::new("images-extension");
    let server = ImageServer::start(Image::untyped("NO-TYPE"));
    // The server's word beats the URL's: this is a png under a jpeg's name, and
    // a reader told `image/jpeg` would refuse it.
    server.serve("photo.jpg", Image::png("PNG"));

    let source = FakeSource::new(3);
    // No type and no extension anywhere: nothing to go on but `bin`.
    source.image(1, &server.url("illustration"));
    // A query string is not an extension.
    source.image(2, &server.url("a.jpg?token=abc"));
    source.image(3, &server.url("photo.jpg"));

    download(&source, &dir);

    assert!(dir.path().join("images/0001-1.bin").exists());
    assert!(dir.path().join("images/0002-1.jpg").exists());
    assert!(dir.path().join("images/0003-1.png").exists());
}

#[test]
fn two_images_of_one_chapter_are_numbered_and_a_second_run_keeps_them() {
    let dir = TempDir::new("images-two");
    let server = ImageServer::start(Image::png("PNG"));
    let source = FakeSource::new(1);
    source.image(1, &server.url("one.png"));
    source.image(1, &server.url("two.png"));

    download(&source, &dir);
    let novel = NovelDir::at(dir.path());
    let chapter = novel.read_chapter(1).unwrap();
    assert_eq!(
        image_sources(&chapter),
        ["images/0001-1.png", "images/0001-2.png"]
    );
    assert_eq!(novel.read_image("images/0001-2.png").unwrap(), b"PNG");

    // The same chapter with an extra paragraph is still the same chapter: the
    // names count the images, not the blocks around them, so `update --check`
    // finds nothing changed.
    source.edit_chapter(1, "body of chapter 1, revised");
    let summary = update(&source, &dir, true);
    assert_eq!(summary.chapters_fetched, 1, "the text really did change");
    let chapter = NovelDir::at(dir.path()).read_chapter(1).unwrap();
    assert_eq!(
        image_sources(&chapter),
        ["images/0001-1.png", "images/0001-2.png"]
    );
}
