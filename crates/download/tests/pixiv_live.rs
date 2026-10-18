//! The whole sequence against a real illustrated work.
//!
//! `#[ignore]`d, because it hits pixiv's servers. It is here because the one
//! thing the offline tests cannot show is the thing that made this work
//! necessary: that a picture on `i.pximg.net` — which answers `403` to a request
//! that does not say where it came from — is fetched, stored and pointed at.
//!
//! ```text
//! cargo test -p wnd-download --test pixiv_live -- --ignored --nocapture
//! ```
//!
//! The work is all-ages and was on pixiv's daily novel ranking on 2026-10-18. It
//! is pinned rather than looked up at run time, so that a red test means the code
//! rather than the ranking. It is a series with a cover and illustrations inside
//! its chapters, and the whole of it is downloaded: a picture that is not fetched
//! is only visible in a chapter that is, and one chapter proves nothing about the
//! naming, which counts per chapter.

mod common;

use common::TempDir;
use url::Url;
use wnd_core::store::storage::hash_bytes;
use wnd_core::{Block, FetchPolicy, HttpClient, NovelDir};
use wnd_download::{DownloadOptions, UpdateOptions, download_from, update_from};

/// A series whose chapters carry illustrations of their own as well as a cover.
const ILLUSTRATED: &str = "https://www.pixiv.net/novel/show.php?id=29400600";

#[test]
#[ignore = "hits the network"]
fn pixiv_illustrations_are_fetched_stored_and_pointed_at() {
    let target = Url::parse(ILLUSTRATED).unwrap();
    let sources = wnd_sources::registry();
    let source = wnd_sources::find(&sources, &target).expect("the pixiv adapter handles this");
    let client = HttpClient::new(FetchPolicy::default()).expect("client");
    let dir = TempDir::new("pixiv-illustrated");

    let options = DownloadOptions::default();
    let summary = download_from(source, &client, &target, dir.path(), &options, |_| {})
        .expect("the download");

    println!("title:  {}", summary.title);
    println!(
        "images: {} fetched, {} missing",
        summary.images_fetched, summary.images_missing
    );

    assert!(
        summary.chapters_fetched > 1,
        "this work is pinned because it is a series, not a single chapter"
    );
    assert_eq!(
        summary.images_missing, 0,
        "nothing was refused: a `403` here is exactly what a missing referer looks like"
    );

    let novel = NovelDir::at(dir.path());
    let manifest = novel.read_manifest().unwrap();
    assert_eq!(
        manifest.images.len(),
        summary.images_fetched,
        "every picture fetched is recorded exactly once"
    );

    // Every recorded picture is on disk and hashes to what was recorded — the
    // check a resume makes, run here against bytes this test did not write.
    for image in &manifest.images {
        let bytes = novel
            .read_image(&image.file)
            .unwrap_or_else(|error| panic!("{}: {error}", image.file));
        assert_eq!(hash_bytes(&bytes), image.hash, "{}", image.file);
        println!("  {} ({} bytes)", image.file, bytes.len());
    }

    // A cover, which the source states as a URL and the fetcher turns into a path.
    let cover = manifest
        .cover
        .as_deref()
        .expect("a source that names a cover has it fetched");
    assert!(
        novel.read_image(cover).unwrap().starts_with(&[0xFF, 0xD8]),
        "a JPEG, as the site serves pictures: {cover}"
    );

    // And the bodies point at files rather than at `i.pximg.net`.
    let mut in_text = 0;
    for meta in &manifest.chapters {
        let chapter = novel.read_chapter(meta.index).unwrap();
        for block in &chapter.nodes {
            if let Block::Image { src, .. } = block {
                assert!(
                    src.starts_with("images/"),
                    "a stored chapter names a file, not a URL: chapter {} says {src}",
                    meta.index
                );
                assert!(dir.path().join(src).is_file(), "{src} is missing");
                in_text += 1;
            }
        }
    }
    assert!(
        in_text > 1,
        "this work is pinned because its chapters have illustrations"
    );
    println!("in-text: {in_text} in {} chapters", manifest.chapters.len());

    // A second run has nothing to do. The resume rule is the recorded hash, and
    // it holds against bytes a real server sent rather than against a copy this
    // test made.
    let again =
        download_from(source, &client, &target, dir.path(), &options, |_| {}).expect("again");
    assert_eq!(again.chapters_fetched, 0);
    assert_eq!(again.chapters_skipped, summary.chapters_fetched);
    assert_eq!(again.images_fetched, 0);
    assert_eq!(again.bytes_written, 0);

    // And a check agrees, which is the same rule asked a different way: the
    // chapter it compares holds paths, and the URLs the site restates must not
    // make it look edited.
    let checked = update_from(
        source,
        &client,
        dir.path(),
        &UpdateOptions { check: true },
        |_| {},
    )
    .expect("the check");
    assert_eq!(checked.chapters_checked, summary.chapters_fetched);
    assert_eq!(checked.chapters_fetched, 0);
    assert_eq!(checked.images_fetched, 0);
    assert!(checked.up_to_date());
}
