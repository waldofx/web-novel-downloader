//! Network tests for the Narou adapter. They are `#[ignore]`d because a test
//! suite must not depend on someone else's server being reachable — or on it
//! tolerating us.
//!
//! Run them explicitly:
//!
//! ```text
//! cargo test -p wnd-sources --test narou_live -- --ignored --nocapture
//! ```

mod common;

use url::Url;
use wnd_core::ContentRating;
use wnd_sources::{find, registry};

#[test]
#[ignore = "hits the network"]
fn narou_multi_chapter_novel_lists_every_chapter_and_arc() {
    let target = Url::parse("https://ncode.syosetu.com/n0608gd/").unwrap();
    let sources = registry();
    let source = find(&sources, &target).expect("narou adapter handles this");
    let client = common::client();

    let manifest = source.novel(&client, &target).expect("table of contents");

    println!("title:    {}", manifest.title);
    println!("author:   {:?}", manifest.author);
    println!("synopsis: {:?}", manifest.synopsis);
    println!("chapters: {}", manifest.chapters.len());

    assert_eq!(manifest.content_rating, ContentRating::General);
    assert_eq!(manifest.language, "ja");
    assert_eq!(manifest.chapters.len(), 89);
    assert!(
        manifest.title.contains('\u{3000}'),
        "an ideographic space in the title is content, not padding: {:?}",
        manifest.title
    );

    let first = &manifest.chapters[0];
    println!("first:    {first:?}");
    assert_eq!(first.index, 1);
    assert_eq!(first.arc.as_deref(), Some("第一章　勇者、日本に帰る"));
    assert_eq!(
        first.published_at.as_deref(),
        Some("2020-04-01T23:23:00+09:00")
    );
    assert!(first.url.ends_with("/n0608gd/1/"));

    let chapter = source.chapter(&client, first).expect("chapter one");
    println!(
        "chapter 1: {} body blocks, {} preface blocks",
        chapter.nodes.len(),
        chapter.pre_note.len()
    );
    assert!(!chapter.nodes.is_empty());
    assert!(!chapter.pre_note.is_empty(), "this novel has a preface");
}

#[test]
#[ignore = "hits the network"]
fn narou_single_chapter_novel_is_read_from_its_table_of_contents() {
    let target = Url::parse("https://ncode.syosetu.com/n5589cg/").unwrap();
    let sources = registry();
    let source = find(&sources, &target).expect("narou adapter handles this");
    let client = common::client();

    let manifest = source.novel(&client, &target).expect("table of contents");
    println!("title: {}", manifest.title);

    assert_eq!(manifest.chapters.len(), 1, "a short story is one chapter");
    let only = &manifest.chapters[0];
    assert_eq!(
        only.title, manifest.title,
        "the chapter takes the novel's title"
    );
    assert_eq!(only.url, target.as_str(), "the body lives on the TOC page");

    let chapter = source.chapter(&client, only).expect("inline chapter");
    println!("body blocks: {}", chapter.nodes.len());
    assert!(
        chapter.nodes.len() > 100,
        "the whole story should be here, got {} blocks",
        chapter.nodes.len()
    );
}
