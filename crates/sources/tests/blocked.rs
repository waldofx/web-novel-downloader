//! A page that is not the site, at the adapter level.
//!
//! The fetch layer reports the facts and the text that arrived; this is where a
//! page stops being markup. A chapter URL is fetched exactly as the manifest
//! holds it, which is what makes both adapters testable against a socket — their
//! *work* pages are matched on host and can only be exercised against captures.
//!
//! What these tests protect is the sentence a reader sees when their network
//! answers for a site: the network's own words, in its own language, with no
//! phrase list of ours in the way.

mod common;

use wnd_core::{BlockKind, ChapterMeta, Error};
use wnd_sources::kakuyomu::Kakuyomu;
use wnd_sources::narou::Narou;
use wnd_sources::{Source, SourceError};

use common::{local_client, response, serve};

/// A real Narou chapter page, for the happy path.
const NAROU_CHAPTER: &str = include_str!("fixtures/narou/chapter.html");

/// A block page in a language, and from a provider, this tool has never seen.
///
/// Not one phrase in it appears anywhere in this repository: it is the whole
/// point that a filter page is reported because of what it *says to the reader*,
/// not because we recognised it.
const UNKNOWN_FILTER: &str = concat!(
    "<html><head><title>Erişim Engellendi</title></head><body>",
    "<h1>BU SİTEYE ERİŞİM ENGELLENMİŞTİR</h1>",
    "<p>5651 sayılı kanun kapsamında bu siteye erişim engellenmiştir.</p>",
    "</body></html>",
);

fn chapter_meta(url: &str) -> ChapterMeta {
    ChapterMeta {
        index: 1,
        title: "第一章".to_string(),
        url: url.to_string(),
        arc: None,
        published_at: None,
        hash: None,
        file: "chapters/0001.json".to_string(),
    }
}

/// The error `adapter.chapter` returns for a page served twice — once as the
/// chapter, once as the answer to the host probe.
fn chapter_error(adapter: &dyn Source, body: &str) -> SourceError {
    let url = serve(vec![
        response("200 OK", &[], body),
        response("200 OK", &[], body),
    ]);

    adapter
        .chapter(&local_client(), &chapter_meta(&url))
        .expect_err("a block page is not a chapter")
}

#[test]
fn a_filter_page_at_a_narou_chapter_url_is_reported_with_its_own_words() {
    let error = chapter_error(&Narou, UNKNOWN_FILTER);
    let message = error.to_string();

    assert!(
        message.contains("Erişim Engellendi BU SİTEYE ERİŞİM ENGELLENMİŞTİR"),
        "the reader must see the page's own words: {message}"
    );
    assert!(
        message.contains("a chapter body was expected"),
        "and what was expected: {message}"
    );

    match error {
        SourceError::Core(Error::Blocked { kind, .. }) => {
            assert_eq!(kind, BlockKind::NotANovelPage)
        }
        other => panic!("expected a reported block page, got {other:?}"),
    }
}

#[test]
fn a_filter_page_at_a_kakuyomu_episode_url_is_reported_the_same_way() {
    let message = chapter_error(&Kakuyomu, UNKNOWN_FILTER).to_string();

    assert!(
        message.contains("BU SİTEYE ERİŞİM ENGELLENMİŞTİR"),
        "got: {message}"
    );
    assert!(
        message.contains("an episode body was expected"),
        "got: {message}"
    );
}

#[test]
fn a_host_that_answers_everything_the_same_is_named_as_interception() {
    let message = chapter_error(&Kakuyomu, UNKNOWN_FILTER).to_string();

    assert!(
        message.contains("answered a URL that cannot exist with this same page, byte for byte"),
        "a host that serves the probe the same page has been hijacked or filtered: {message}"
    );
}

#[test]
fn a_page_whose_shape_is_simply_gone_does_not_cry_filter() {
    // Markup the site changed, and a host that answers the probe with its own
    // 404. Neither a phrase nor a probe says "filter", and the message must not
    // pretend otherwise — it shows what arrived and leaves the judgement alone.
    let url = serve(vec![
        response(
            "200 OK",
            &[],
            "<html><head><title>第一章 - カクヨム</title></head>\
             <body><div class=\"episodeBody-new\"><p>本文</p></div></body></html>",
        ),
        response("404 Not Found", &[], "<p>no such page</p>"),
    ]);

    let message = Kakuyomu
        .chapter(&local_client(), &chapter_meta(&url))
        .expect_err("no episode body is not a chapter")
        .to_string();

    assert!(
        message.contains("it begins: \"第一章 - カクヨム 本文\""),
        "the text that arrived is what a reader needs to judge: {message}"
    );
    assert!(
        !message.contains("content filter") && !message.contains("captive portal"),
        "no interception was proven, so none may be claimed: {message}"
    );
}

#[test]
fn an_ordinary_chapter_is_still_read_as_a_chapter() {
    let url = serve(vec![response("200 OK", &[], NAROU_CHAPTER)]);

    let chapter = Narou
        .chapter(&local_client(), &chapter_meta(&url))
        .expect("a real chapter page must still parse");

    assert!(!chapter.nodes.is_empty(), "the body came through");
}
