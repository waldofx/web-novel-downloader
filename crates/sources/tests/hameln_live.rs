//! Network tests for the Hameln adapter. They are `#[ignore]`d because a test
//! suite must not depend on someone else's server being reachable.
//!
//! Run them explicitly:
//!
//! ```text
//! cargo test -p wnd-sources --test hameln_live -- --ignored --nocapture
//! ```

mod common;

use url::Url;
use wnd_core::ContentRating;
use wnd_sources::{find, registry};

#[test]
#[ignore = "hits the network"]
fn a_hameln_work_is_downloaded_and_its_chapter_parsed() {
    let target = Url::parse("https://syosetu.org/novel/316401/").unwrap();
    let sources = registry();
    let source = find(&sources, &target).expect("the hameln adapter handles this");
    let client = common::client();
    let manifest = source.novel(&client, &target).expect("work page");

    println!("title:    {}", manifest.title);
    println!("author:   {:?}", manifest.author);
    println!("tags:     {:?}", manifest.tags);
    println!("chapters: {}", manifest.chapters.len());

    assert_eq!(manifest.content_rating, ContentRating::General);
    assert_eq!(manifest.language, "ja");
    assert!(manifest.author.is_some(), "this work has an author");
    assert!(!manifest.tags.is_empty(), "this work has tags");
    assert!(manifest.chapters.len() >= 2, "more than one chapter");

    let first = &manifest.chapters[0];
    println!("first:    {first:?}");
    assert_eq!(first.index, 1);

    let chapter = source.chapter(&client, first).expect("chapter one");
    println!("chapter 1: {} blocks", chapter.nodes.len());
    assert!(!chapter.nodes.is_empty(), "the body is the whole chapter");
}
