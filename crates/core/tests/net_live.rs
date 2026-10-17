//! Network tests for the fetch layer. They are `#[ignore]`d because a test suite
//! must not depend on someone else's server being reachable — or on it
//! tolerating us.
//!
//! Run them explicitly:
//!
//! ```text
//! cargo test -p wnd-core --test net_live -- --ignored --nocapture
//! ```

mod common;

use std::time::{Duration, Instant};
use wnd_core::{Error, FetchPolicy, RobotsRules};

#[test]
#[ignore = "hits the network"]
fn a_real_narou_page_is_fetched_and_decoded() {
    let page = common::client()
        .get_page("https://ncode.syosetu.com/n5589cg/")
        .expect("narou page");

    assert!(
        page.body.contains("p-novel__title"),
        "expected Narou markup, got {} bytes",
        page.body.len()
    );
    assert!(
        page.body.contains("乙女ゲーム"),
        "the body must decode as UTF-8, not mojibake"
    );
    assert_eq!(
        page.final_url, page.url,
        "narou must not redirect us somewhere we did not ask for"
    );
}

#[test]
#[ignore = "hits the network"]
fn narou_robots_txt_parses_and_permits_novel_pages() {
    let body = common::client()
        .get_page("https://ncode.syosetu.com/robots.txt")
        .expect("robots.txt")
        .body;
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
    let started = Instant::now();
    let error = common::client()
        .get_page("https://ncode.syosetu.com/n0000zz/")
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
