//! Network tests for the Pixiv adapter. They are `#[ignore]`d because a test
//! suite must not depend on someone else's server being reachable — or on it
//! tolerating us.
//!
//! ```text
//! cargo test -p wnd-sources --test pixiv_live -- --ignored --nocapture
//! ```
//!
//! The works they hit are taken from the gitignored `fixtures.local.json` (see
//! `fixtures.local.example.json`), never hard-coded: this repository holds no
//! real novel identifier. Each test says which shape of work it needs — a series
//! of more than one novel, a standalone novel the reader pages — takes the first
//! configured target that fits, and skips with a note when none does.
//!
//! Each test also re-reads the API answer the adapter itself holds a URL for and
//! checks the manifest against it. That is deliberate: the numbers a live test
//! can pin by hand go stale, and the two ways this adapter can be wrong without
//! saying so — a series downloaded as one chapter, and a chapter's pages cut
//! short at the first one — are both *counts*.

mod common;

use std::collections::HashSet;
use url::Url;
use wnd_core::ContentRating;
use wnd_sources::pixiv::api;
use wnd_sources::{find, registry};

#[test]
#[ignore = "hits the network"]
fn a_url_inside_a_series_downloads_the_whole_series() {
    let sources = registry();
    let client = common::client();

    let targets = common::targets("pixiv");
    if targets.is_empty() {
        common::note_no_target("pixiv");
        return;
    }

    // A URL that names one novel of a series; the work the adapter must return is
    // the series. The first configured target shaped that way is the one this
    // runs against.
    let mut chosen = None;
    for target in targets {
        let source = find(&sources, &target).expect("the pixiv adapter handles this");
        let manifest = source.novel(&client, &target).expect("the work");
        if manifest.source.novel_url.contains("/novel/series/") && manifest.chapters.len() > 1 {
            chosen = Some((target, manifest));
            break;
        }
        println!(
            "note: {} is not a series of more than one novel — trying the next",
            manifest.source.novel_url
        );
    }
    let Some((target, manifest)) = chosen else {
        println!(
            "note: no `pixiv` target that is a series of more than one novel — skipping \
             (see fixtures.local.example.json)"
        );
        return;
    };
    let source = find(&sources, &target).expect("the pixiv adapter handles this");

    println!("title:    {}", manifest.title);
    println!("author:   {:?}", manifest.author);
    println!("tags:     {:?}", manifest.tags);
    println!("cover:    {:?}", manifest.cover_url);
    println!("work:     {}", manifest.source.novel_url);
    println!("chapters: {}", manifest.chapters.len());
    for chapter in manifest.chapters.iter().take(3) {
        println!("  {} {}", chapter.index, chapter.title);
    }

    assert_eq!(manifest.source.id, "pixiv");

    // Re-read the work's own answer and check the counts and the rating against
    // it, rather than against a number pinned here.
    let answer = wnd_sources::pixiv::answer_url(&Url::parse(&manifest.source.novel_url).unwrap())
        .expect("the work's own answer");
    let series: api::Series = read(&client, &answer);

    assert_eq!(
        manifest.content_rating,
        if series.x_restrict >= 1 {
            ContentRating::Adult
        } else {
            ContentRating::General
        },
        "the rating is read from the work's xRestrict"
    );
    assert_eq!(
        manifest.language,
        if series.language.trim_ascii().is_empty() {
            "ja".to_string()
        } else {
            series.language.clone()
        },
        "the language is the work's, or Japanese when it states none"
    );

    // The URL named one novel. If the adapter had read it as the work, this
    // would be one chapter and nothing would have complained.
    assert_eq!(
        manifest.chapters.len() as i64,
        series.published_content_count,
        "every novel the series states must be listed, not just the one the URL named"
    );
    assert_eq!(
        manifest
            .chapters
            .iter()
            .map(|chapter| chapter.index)
            .collect::<Vec<_>>(),
        (1..=manifest.chapters.len() as u32).collect::<Vec<_>>(),
        "chapters are numbered in reading order, from one"
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
            .all(|c| c.url.starts_with("https://www.pixiv.net/ajax/novel/")),
        "a pixiv chapter's URL is the API address its body is read from"
    );

    let first = manifest.chapters.first().expect("a first chapter");
    let chapter = source.chapter(&client, first).expect("the first chapter");
    println!("first chapter: {} blocks", chapter.nodes.len());
    assert!(
        !chapter.nodes.is_empty(),
        "a chapter that parses to nothing is not an empty chapter"
    );
}

#[test]
#[ignore = "hits the network"]
fn a_novel_in_no_series_is_one_chapter_with_its_reader_pages_joined() {
    let sources = registry();
    let client = common::client();

    let targets = common::targets("pixiv");
    if targets.is_empty() {
        common::note_no_target("pixiv");
        return;
    }

    // A standalone novel — one chapter — whose text the site shows in more than
    // one reader page, so that joining those pages is actually exercised. The
    // first configured target shaped that way is the one this runs against.
    let mut chosen = None;
    for target in targets {
        let source = find(&sources, &target).expect("the pixiv adapter handles this");
        let manifest = source.novel(&client, &target).expect("the novel");
        if manifest.chapters.len() != 1 {
            println!(
                "note: {} is a work of {} chapters, not a standalone novel — trying the next",
                manifest.source.novel_url,
                manifest.chapters.len()
            );
            continue;
        }
        let novel: api::Novel = read(&client, &manifest.chapters[0].url);
        if !novel.content.contains("[newpage]") {
            println!(
                "note: {} has no reader pages — trying the next",
                manifest.source.novel_url
            );
            continue;
        }
        chosen = Some((target, manifest, novel));
        break;
    }
    let Some((target, manifest, novel)) = chosen else {
        println!(
            "note: no `pixiv` standalone novel with reader pages — skipping \
             (see fixtures.local.example.json)"
        );
        return;
    };
    let source = find(&sources, &target).expect("the pixiv adapter handles this");

    println!("title:    {}", manifest.title);
    println!("author:   {:?}", manifest.author);
    println!("chapters: {}", manifest.chapters.len());

    assert_eq!(
        manifest.chapters.len(),
        1,
        "a novel in no series is a work of one chapter"
    );

    let meta = &manifest.chapters[0];
    assert!(meta.url.starts_with("https://www.pixiv.net/ajax/novel/"));
    if !novel.create_date.trim_ascii().is_empty() {
        assert!(
            meta.published_at.is_some(),
            "the site states when a chapter appeared, so the manifest keeps it"
        );
    }

    let chapter = source.chapter(&client, meta).expect("the chapter");
    println!("blocks:   {}", chapter.nodes.len());

    // The site shows a long novel to a reader in pages, and hands over the whole
    // text with `[newpage]` where the slices meet. Read the answer again and
    // count what was there: an adapter that stopped at the first page would
    // come back with a handful of blocks and no complaint.
    let new_page_lines = novel
        .content
        .lines()
        .filter(|line| line.trim() == "[newpage]")
        .count();
    let prose_lines = novel
        .content
        .lines()
        .filter(|line| !line.trim_ascii().is_empty())
        .count()
        - new_page_lines;

    println!(
        "pages:    {} ({} prose lines, {} page breaks)",
        novel.content.split("[newpage]").count(),
        prose_lines,
        novel.content.matches("[newpage]").count()
    );

    assert!(
        chapter.nodes.len() >= prose_lines,
        "every line the site sent must reach the chapter: {} blocks for {prose_lines} lines",
        chapter.nodes.len()
    );
}

/// Read the API answer a URL of the adapter's own index points at.
fn read<T: serde::de::DeserializeOwned>(client: &wnd_core::HttpClient, url: &str) -> T {
    let page = client.get_page(url).expect("the answer");
    api::body_of(&page.body).expect("the payload")
}
