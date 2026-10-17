//! Tests that touch the network. They are `#[ignore]`d because a test suite
//! must not depend on someone else's server being reachable — or on it
//! tolerating us.
//!
//! Run them explicitly:
//!
//! ```text
//! cargo test -p wnd-core --test live -- --ignored --nocapture
//! ```

use std::time::{Duration, Instant};
use wnd_core::{Error, FetchPolicy, HttpClient, RobotsRules};

#[test]
#[ignore = "hits the network"]
fn a_real_narou_page_is_fetched_and_decoded() {
    let client = HttpClient::new(FetchPolicy::default()).expect("client");
    let body = client
        .get_text("https://ncode.syosetu.com/n5589cg/")
        .expect("narou page");

    assert!(
        body.contains("p-novel__title"),
        "expected Narou markup, got {} bytes",
        body.len()
    );
    assert!(
        body.contains("乙女ゲーム"),
        "the body must decode as UTF-8, not mojibake"
    );
}

#[test]
#[ignore = "hits the network"]
fn narou_robots_txt_parses_and_permits_novel_pages() {
    let client = HttpClient::new(FetchPolicy::default()).expect("client");
    let body = client
        .get_text("https://ncode.syosetu.com/robots.txt")
        .expect("robots.txt");
    let rules = RobotsRules::parse(&body);
    let user_agent = &FetchPolicy::default().user_agent;

    println!("crawl delay: {:?}", rules.crawl_delay(user_agent));
    assert!(
        rules.is_allowed(user_agent, "/n0608gd/1/"),
        "Narou must permit novel pages for our crawler"
    );
}

#[test]
#[ignore = "hits the network"]
fn a_missing_page_is_an_error_and_is_not_retried() {
    let client = HttpClient::new(FetchPolicy::default()).expect("client");
    let started = Instant::now();
    let error = client
        .get_text("https://ncode.syosetu.com/n0000zz/")
        .expect_err("a nonexistent novel must not succeed");

    assert!(
        matches!(error, Error::Http { status: 404, .. }),
        "expected a 404, got {error:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "a final status must not be retried in a loop"
    );
}
