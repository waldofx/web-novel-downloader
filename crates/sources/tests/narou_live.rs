//! Network tests for the Narou adapter. They are `#[ignore]`d because a test
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
//! cargo test -p wnd-sources --test narou_live -- --ignored --nocapture
//! ```

mod common;

use std::collections::HashSet;
use wnd_sources::{find, registry};

#[test]
#[ignore = "hits the network"]
fn narou_multi_chapter_novel_lists_every_chapter() {
    let sources = registry();
    let client = common::client();

    let targets = common::targets("narou");
    if targets.is_empty() {
        common::note_no_target("narou");
        return;
    }

    // A work of more than one chapter: the chapter list is the point of this
    // test, and a one-chapter work is the other test's job. The first configured
    // target shaped that way is the one this runs against.
    let mut chosen = None;
    for target in targets {
        let source = find(&sources, &target).expect("the narou adapter handles this");
        let manifest = source.novel(&client, &target).expect("table of contents");
        if manifest.chapters.len() > 1 {
            chosen = Some((target, manifest));
            break;
        }
        println!(
            "note: {} has one chapter — trying the next `narou` target",
            manifest.source.novel_url
        );
    }
    let Some((target, manifest)) = chosen else {
        println!(
            "note: no `narou` target with more than one chapter — skipping \
             (see fixtures.local.example.json)"
        );
        return;
    };
    let source = find(&sources, &target).expect("the narou adapter handles this");

    println!("title:    {}", manifest.title);
    println!("author:   {:?}", manifest.author);
    println!("synopsis: {:?}", manifest.synopsis);
    println!("chapters: {}", manifest.chapters.len());

    // The `narou` bucket is the general-audience host; an adult work lives under
    // `novel18`, and the adapter resolves that host's rating separately.
    assert_eq!(manifest.content_rating, source.content_rating());
    assert_eq!(manifest.language, "ja");

    // Not asserted: the work's own title, its arc names, and its exact chapter
    // count. Each is a fact of whichever work is configured, and pinning one
    // would make a red test say "the fixture changed", not "the adapter broke".

    // The index is the adapter's own contract, whichever work is configured:
    // one-based, gapless, each chapter its own page under the work.
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
            "Narou states an instant at +09:00: {at:?}"
        );
    }

    let chapter = source.chapter(&client, first).expect("chapter one");
    println!(
        "chapter 1: {} body blocks, {} preface blocks",
        chapter.nodes.len(),
        chapter.pre_note.len()
    );
    assert!(
        !chapter.nodes.is_empty(),
        "a chapter that parses to nothing is not an empty chapter"
    );
}

#[test]
#[ignore = "hits the network"]
fn narou_single_chapter_novel_is_read_from_its_table_of_contents() {
    let sources = registry();
    let client = common::client();

    let targets = common::targets("narou");
    if targets.is_empty() {
        common::note_no_target("narou");
        return;
    }

    // A short story: one chapter whose body is on the table-of-contents page.
    // The first configured target shaped that way is the one this runs against;
    // a longer work is the other test's job.
    let mut chosen = None;
    for target in targets {
        let source = find(&sources, &target).expect("the narou adapter handles this");
        let manifest = source.novel(&client, &target).expect("table of contents");
        if manifest.chapters.len() == 1 {
            chosen = Some((target, manifest));
            break;
        }
        println!(
            "note: {} has {} chapters — trying the next `narou` target",
            manifest.source.novel_url,
            manifest.chapters.len()
        );
    }
    let Some((target, manifest)) = chosen else {
        println!(
            "note: no `narou` target with exactly one chapter — skipping \
             (see fixtures.local.example.json)"
        );
        return;
    };
    let source = find(&sources, &target).expect("the narou adapter handles this");

    println!("title: {}", manifest.title);

    assert_eq!(manifest.chapters.len(), 1, "a short story is one chapter");
    let only = &manifest.chapters[0];
    assert_eq!(
        only.title, manifest.title,
        "the chapter takes the novel's title"
    );
    assert_eq!(
        only.url, manifest.source.novel_url,
        "the body lives on the TOC page"
    );

    let chapter = source.chapter(&client, only).expect("inline chapter");
    println!("body blocks: {}", chapter.nodes.len());
    // The body's length is the work's; only that it parsed to something is a
    // fact about the adapter, so the old "more than a hundred blocks" is gone.
    assert!(
        !chapter.nodes.is_empty(),
        "the whole story should be here, not an empty chapter"
    );
}
