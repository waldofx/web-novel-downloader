//! Network tests for the Kakuyomu adapter. They are `#[ignore]`d because a test
//! suite must not depend on someone else's server being reachable — or on it
//! tolerating us.
//!
//! The works they hit are taken from the gitignored `fixtures.local.json` (see
//! `fixtures.local.example.json`), never hard-coded: this repository holds no
//! real novel identifier. With nothing configured for a test, it skips and says
//! so rather than failing.
//!
//! Run them explicitly:
//!
//! ```text
//! cargo test -p wnd-sources --test kakuyomu_live -- --ignored --nocapture
//! ```

mod common;

use std::collections::HashSet;
use wnd_sources::{find, registry};

#[test]
#[ignore = "hits the network"]
fn a_work_is_downloaded_and_its_chapter_parsed() {
    let Some(target) = common::targets("kakuyomu").into_iter().next() else {
        common::note_no_target("kakuyomu");
        return;
    };

    let sources = registry();
    let source = find(&sources, &target).expect("the kakuyomu adapter handles this");
    let client = common::client();
    let manifest = source.novel(&client, &target).expect("work page");

    println!("title:    {}", manifest.title);
    println!("author:   {:?}", manifest.author);
    println!("tags:     {:?}", manifest.tags);
    println!("chapters: {}", manifest.chapters.len());

    assert_eq!(manifest.content_rating, source.content_rating());
    assert_eq!(manifest.language, "ja");
    assert!(
        manifest
            .source
            .novel_url
            .starts_with("https://kakuyomu.jp/works/")
            && !manifest.source.novel_url.ends_with('/'),
        "the canonical work URL, with no trailing slash: {}",
        manifest.source.novel_url
    );

    // The work's author, tags and synopsis are its own, and are not pinned here;
    // neither is the exact episode id, which changes with the work.

    // The index is the adapter's own contract, whichever work is configured:
    // one-based, gapless, each chapter its own page under the work.
    let indices: Vec<u32> = manifest.chapters.iter().map(|c| c.index).collect();
    assert_eq!(
        indices,
        (1..=manifest.chapters.len() as u32).collect::<Vec<u32>>(),
        "episodes are numbered from one, with no gaps"
    );
    let urls: HashSet<&str> = manifest.chapters.iter().map(|c| c.url.as_str()).collect();
    assert_eq!(
        urls.len(),
        manifest.chapters.len(),
        "every episode has its own page"
    );
    assert!(
        manifest
            .chapters
            .iter()
            .all(|c| c.url.starts_with(&manifest.source.novel_url)),
        "every episode URL belongs to the work: {}",
        manifest.source.novel_url
    );

    let first = &manifest.chapters[0];
    println!("first:    {first:?}");
    assert_eq!(first.index, 1);
    if let Some(at) = first.published_at.as_deref() {
        assert!(
            at.ends_with('Z'),
            "Kakuyomu states an instant in UTC: {at:?}"
        );
    }

    let chapter = source.chapter(&client, first).expect("chapter one");
    println!("chapter 1: {} blocks", chapter.nodes.len());
    assert!(
        !chapter.nodes.is_empty(),
        "the body is the whole episode, not an empty chapter"
    );
    assert!(
        chapter.pre_note.is_empty(),
        "Kakuyomu has no separate preface"
    );
}

/// The regression test for this adapter's central decision.
///
/// The work page renders a handful of episode links into its HTML and lazy-loads
/// the rest; its snapshot lists every episode. This runs only against a work
/// long enough for that to matter — one with more than a hundred episodes — and
/// skips when nothing that long is configured. If the adapter ever goes back to
/// reading the HTML, the fit check finds nothing and this test says so rather
/// than passing on a short work.
#[test]
#[ignore = "hits the network"]
fn a_long_work_lists_every_episode_not_just_the_rendered_ones() {
    let sources = registry();
    let client = common::client();

    let targets = common::targets("kakuyomu");
    if targets.is_empty() {
        common::note_no_target("kakuyomu");
        return;
    }

    const RENDERED_HTML_EPISODES: usize = 7;
    const A_HUNDRED: usize = 100;

    let mut chosen = None;
    for target in targets {
        let source = find(&sources, &target).expect("the kakuyomu adapter handles this");
        let manifest = source.novel(&client, &target).expect("work page");
        if manifest.chapters.len() > A_HUNDRED {
            chosen = Some(manifest);
            break;
        }
        println!(
            "note: {} lists {} episodes — too short to prove the snapshot was read",
            manifest.source.novel_url,
            manifest.chapters.len()
        );
    }
    let Some(manifest) = chosen else {
        println!(
            "note: no `kakuyomu` target with more than a hundred episodes — skipping \
             (see fixtures.local.example.json)"
        );
        return;
    };

    println!("title:    {}", manifest.title);
    println!("chapters: {}", manifest.chapters.len());

    assert!(
        manifest.chapters.len() > A_HUNDRED,
        "the rendered HTML lists about {RENDERED_HTML_EPISODES} episodes; the snapshot lists \
         every one. Getting {} means the episode list came from the wrong place",
        manifest.chapters.len()
    );
    // The exact count and the work's arc grouping are the work's own; only that
    // the list is long enough to have come from the snapshot is the adapter's.

    let indices: Vec<u32> = manifest.chapters.iter().map(|c| c.index).collect();
    assert_eq!(
        indices,
        (1..=manifest.chapters.len() as u32).collect::<Vec<u32>>(),
        "episodes are numbered from one, with no gaps"
    );

    let urls: HashSet<&str> = manifest.chapters.iter().map(|c| c.url.as_str()).collect();
    assert_eq!(
        urls.len(),
        manifest.chapters.len(),
        "every episode has its own page"
    );
}
