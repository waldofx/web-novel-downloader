//! Network tests for the Hameln adapter. They are `#[ignore]`d because a test
//! suite must not depend on someone else's server being reachable.
//!
//! The work they hit is taken from the gitignored `fixtures.local.json` (see
//! `fixtures.local.example.json`), never hard-coded: this repository holds no
//! real novel identifier. With nothing configured, the test skips and says so.
//!
//! Run them explicitly:
//!
//! ```text
//! cargo test -p wnd-sources --test hameln_live -- --ignored --nocapture
//! ```

mod common;

use std::collections::HashSet;
use wnd_sources::{find, registry};

#[test]
#[ignore = "hits the network"]
fn a_hameln_work_is_downloaded_and_its_chapter_parsed() {
    let Some(target) = common::targets("hameln").into_iter().next() else {
        common::note_no_target("hameln");
        return;
    };

    let sources = registry();
    let source = find(&sources, &target).expect("the hameln adapter handles this");
    let client = common::client();
    let manifest = source.novel(&client, &target).expect("work page");

    println!("title:    {}", manifest.title);
    println!("author:   {:?}", manifest.author);
    println!("tags:     {:?}", manifest.tags);
    println!("chapters: {}", manifest.chapters.len());

    // The `hameln` bucket is the general-audience host; an adult work lives under
    // `h_syosetu`, and the adapter resolves that host's rating separately.
    assert_eq!(manifest.content_rating, source.content_rating());
    assert_eq!(manifest.language, "ja");

    // The index is the adapter's own contract, whichever work is configured:
    // one-based, gapless, each chapter its own page under the work.
    assert!(
        !manifest.chapters.is_empty(),
        "a work page that lists no chapters did not parse"
    );
    let indices: Vec<u32> = manifest.chapters.iter().map(|c| c.index).collect();
    assert_eq!(
        indices,
        (1..=manifest.chapters.len() as u32).collect::<Vec<u32>>(),
        "chapters are numbered from one, with no gaps"
    );
    let urls: HashSet<&str> = manifest.chapters.iter().map(|c| c.url.as_str()).collect();
    assert_eq!(
        urls.len(),
        manifest.chapters.len(),
        "every chapter has its own page"
    );
    assert!(
        manifest
            .chapters
            .iter()
            .all(|c| c.url.starts_with(&manifest.source.novel_url)),
        "every chapter URL belongs to the work: {}",
        manifest.source.novel_url
    );

    let first = &manifest.chapters[0];
    println!("first:    {first:?}");
    assert_eq!(first.index, 1);
    if let Some(at) = first.published_at.as_deref() {
        assert!(
            at.ends_with("+09:00"),
            "Hameln states an instant at +09:00: {at:?}"
        );
    }

    let chapter = source.chapter(&client, first).expect("chapter one");
    println!("chapter 1: {} blocks", chapter.nodes.len());
    assert!(
        !chapter.nodes.is_empty(),
        "the body is the whole chapter, not an empty one"
    );
}
