//! Network tests for the Pixiv adapter. They are `#[ignore]`d because a test
//! suite must not depend on someone else's server being reachable — or on it
//! tolerating us.
//!
//! ```text
//! cargo test -p wnd-sources --test pixiv_live -- --ignored --nocapture
//! ```
//!
//! Both works below are all-ages and were on pixiv's daily novel ranking on
//! 2026-10-18, when this adapter was written. They are pinned rather than looked
//! up at run time so that a red test means the adapter, not the ranking.
//!
//! Each test also re-reads the API answer the adapter itself holds a URL for and
//! checks the manifest against it. That is deliberate: the numbers a live test
//! can pin by hand go stale, and the two ways this adapter can be wrong without
//! saying so — a series downloaded as one chapter, and a chapter's pages cut
//! short at the first one — are both *counts*.

mod common;

use url::Url;
use wnd_core::ContentRating;
use wnd_sources::pixiv::api;
use wnd_sources::{find, registry};

/// A novel that belongs to a series, and a URL that names only this novel of it.
const IN_A_SERIES: &str = "https://www.pixiv.net/novel/show.php?id=29392114";

/// A novel in no series, shown by the site's reader in five pages.
const STANDALONE: &str = "https://www.pixiv.net/novel/show.php?id=29391720";

#[test]
#[ignore = "hits the network"]
fn a_url_inside_a_series_downloads_the_whole_series() {
    let target = Url::parse(IN_A_SERIES).unwrap();
    let sources = registry();
    let source = find(&sources, &target).expect("the pixiv adapter handles this");
    let client = common::client();

    let manifest = source.novel(&client, &target).expect("the series");

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
    assert_eq!(
        manifest.content_rating,
        ContentRating::General,
        "this work is all-ages, and the rating is read from the work"
    );
    assert!(
        manifest.source.novel_url.contains("/novel/series/"),
        "the work is the series, so that is what the manifest points at: {}",
        manifest.source.novel_url
    );
    assert!(
        !manifest.title.is_empty() && manifest.cover_url.is_some(),
        "the series answer is where a series' own title and cover live"
    );

    // The URL named one novel. If the adapter had read it as the work, this
    // would be one chapter and nothing would have complained.
    let answer = wnd_sources::pixiv::answer_url(&Url::parse(&manifest.source.novel_url).unwrap())
        .expect("the work's own answer");
    let series: api::Series = read(&client, &answer);
    assert_eq!(
        manifest.chapters.len() as i64,
        series.published_content_count,
        "every novel the series states must be listed, not just the one the URL named"
    );
    assert!(
        manifest.chapters.len() > 1,
        "a series with one novel would make this test prove nothing"
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
    let target = Url::parse(STANDALONE).unwrap();
    let sources = registry();
    let source = find(&sources, &target).expect("the pixiv adapter handles this");
    let client = common::client();

    let manifest = source.novel(&client, &target).expect("the novel");

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
    assert!(
        meta.published_at.is_some(),
        "the site states when a chapter appeared, so the manifest keeps it"
    );

    let chapter = source.chapter(&client, meta).expect("the chapter");
    println!("blocks:   {}", chapter.nodes.len());

    // The site shows a long novel to a reader in pages, and hands over the whole
    // text with `[newpage]` where the slices meet. Read the answer again and
    // count what was there: an adapter that stopped at the first page would
    // come back with a handful of blocks and no complaint.
    let novel: api::Novel = read(&client, &meta.url);
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
        novel.content.contains("[newpage]"),
        "this work is pinned because its text has reader pages"
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
