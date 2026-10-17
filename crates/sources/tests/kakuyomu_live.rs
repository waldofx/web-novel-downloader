//! Network tests for the Kakuyomu adapter. They are `#[ignore]`d because a test
//! suite must not depend on someone else's server being reachable — or on it
//! tolerating us.
//!
//! Run them explicitly:
//!
//! ```text
//! cargo test -p wnd-sources --test kakuyomu_live -- --ignored --nocapture
//! ```

mod common;

use std::collections::HashSet;
use url::Url;
use wnd_core::ContentRating;
use wnd_sources::{find, registry};

#[test]
#[ignore = "hits the network"]
fn a_work_without_arcs_is_downloaded_and_its_chapter_parsed() {
    let target = Url::parse("https://kakuyomu.jp/works/11054822663954717597").unwrap();
    let sources = registry();
    let source = find(&sources, &target).expect("the kakuyomu adapter handles this");
    let client = common::client();
    let manifest = source.novel(&client, &target).expect("work page");

    println!("title:    {}", manifest.title);
    println!("author:   {:?}", manifest.author);
    println!("tags:     {:?}", manifest.tags);
    println!("chapters: {}", manifest.chapters.len());

    assert_eq!(manifest.content_rating, ContentRating::General);
    assert_eq!(manifest.language, "ja");
    assert_eq!(
        manifest.source.novel_url,
        target.as_str(),
        "the canonical work URL, with no trailing slash"
    );
    assert!(manifest.author.is_some(), "this work has an author");
    assert!(!manifest.tags.is_empty(), "this work has tags");
    assert!(
        manifest.synopsis.is_some_and(|s| s.contains('\n')),
        "the introduction is kept, paragraphs and all"
    );
    assert!(manifest.chapters.len() >= 2, "an 11-episode work");
    assert_eq!(
        manifest.chapters[0].arc, None,
        "this work has no arcs, so there is no arc name to invent"
    );

    let first = &manifest.chapters[0];
    println!("first:    {first:?}");
    assert_eq!(first.index, 1);
    assert_eq!(
        first.url,
        format!("{}/episodes/11054822663955326556", target.as_str())
    );
    assert!(
        first
            .published_at
            .as_deref()
            .is_some_and(|at| at.ends_with('Z')),
        "Kakuyomu states an instant in UTC: {:?}",
        first.published_at
    );

    let chapter = source.chapter(&client, first).expect("chapter one");
    println!("chapter 1: {} blocks", chapter.nodes.len());
    assert!(
        chapter.nodes.len() > 100,
        "the body is the whole episode, got {} blocks",
        chapter.nodes.len()
    );
    assert!(
        chapter.pre_note.is_empty(),
        "Kakuyomu has no separate preface"
    );
}

/// The regression test for this adapter's central decision.
///
/// The work page renders seven episode links into its HTML and lazy-loads the
/// rest; its snapshot lists every episode. If the adapter ever goes back to
/// reading the HTML, this test fails — with a number, not a shrug.
#[test]
#[ignore = "hits the network"]
fn a_thousand_episode_work_lists_every_episode_not_just_the_rendered_ones() {
    let target = Url::parse("https://kakuyomu.jp/works/16816452221336015212").unwrap();
    let sources = registry();
    let source = find(&sources, &target).expect("the kakuyomu adapter handles this");
    let client = common::client();
    let manifest = source.novel(&client, &target).expect("work page");

    println!("title:    {}", manifest.title);
    println!("chapters: {}", manifest.chapters.len());

    assert!(
        manifest.chapters.len() > 500,
        "the rendered HTML lists 7 episodes of this work; the snapshot lists over a thousand. \
         Getting {} means the episode list came from the wrong place",
        manifest.chapters.len()
    );

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

    let arcs: HashSet<&str> = manifest
        .chapters
        .iter()
        .filter_map(|c| c.arc.as_deref())
        .collect();
    println!("arcs:     {}", arcs.len());
    assert!(
        arcs.len() > 5,
        "this work is grouped into arcs (章), which must be kept: {arcs:?}"
    );
    assert!(
        manifest.chapters.iter().all(|c| c.arc.is_some()),
        "every episode here belongs to an arc"
    );
}
