//! What a session does when it meets the network.
//!
//! Nothing here leaves `127.0.0.1`: the whole point is what `HttpClient` does
//! with a real response, and a mock would not exercise it.

mod common;

use common::serve_one;
use wnd_core::{FetchPolicy, HttpClient, session::Session};

fn client_with(session: Option<Session>) -> HttpClient {
    let policy = FetchPolicy {
        respect_robots: false,
        min_delay: std::time::Duration::ZERO,
        max_retries: 1,
        ..FetchPolicy::default()
    };
    let mut client = HttpClient::new(policy).expect("a client");
    if let Some(session) = session {
        client = client.with_session(session);
    }
    client
}

#[test]
fn a_session_is_sent_only_to_the_hosts_it_names() {
    let cookie = "PHPSESSID=secret";
    // A session for 127.0.0.1 covers the local server; one for example.com does
    // not, so the jar must not leave with the request.
    let covered = Session::from_cookies("local", vec![cookie_for("127.0.0.1", cookie)]);
    let uncovered = Session::from_cookies("elsewhere", vec![cookie_for("example.com", cookie)]);

    let (url, captured) =
        serve_one("HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
    let client = client_with(Some(covered));
    client.get_page(&url).expect("a page");
    assert_eq!(
        captured.lock().expect("lock").clone().as_deref(),
        Some("cookie: PHPSESSID=secret"),
        "a session that names this host is sent with the request"
    );

    let (url, captured) =
        serve_one("HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
    let client = client_with(Some(uncovered));
    client.get_page(&url).expect("a page");
    assert!(
        captured.lock().expect("lock").is_none(),
        "a session that does not name this host is never sent"
    );
}

#[test]
fn without_a_session_the_request_carries_no_cookie() {
    let (url, captured) =
        serve_one("HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
    let client = client_with(None);
    client.get_page(&url).expect("a page");
    assert!(captured.lock().expect("lock").is_none());
}

#[test]
fn a_challenge_with_a_session_in_use_blames_the_session() {
    let session = Session::from_cookies("pixiv", vec![cookie_for("127.0.0.1", "PHPSESSID=secret")]);
    let (url, _) = serve_one(
        "HTTP/1.1 403 Forbidden\r\nCf-Mitigated: challenge\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
    );
    let client = client_with(Some(session));

    let error = client
        .get_page(&url)
        .expect_err("a challenge is not a page");
    let message = error.to_string();
    assert!(
        message.contains("pixiv"),
        "the session is named, not the site: {message}"
    );
    assert!(
        message.contains("expired") || message.contains("save it again"),
        "the blame is on the session: {message}"
    );
    assert!(
        !message.contains("secret"),
        "a challenge message must not carry a cookie: {message}"
    );
}

fn cookie_for(host: &str, header_value: &str) -> wnd_core::session::cookie::Cookie {
    let (name, value) = header_value.split_once('=').expect("name=value");
    wnd_core::session::cookie::Cookie {
        domain: host.to_string(),
        host_only: true,
        path: "/".to_string(),
        secure: false,
        expires_at: None,
        name: name.to_string(),
        value: value.to_string(),
    }
}
