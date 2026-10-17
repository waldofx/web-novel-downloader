//! What the fetch layer does with a page that is not the site.
//!
//! The classifier itself is unit-tested next to its markers; what matters here
//! is the *wiring* — that a filter page served with `HTTP 200` never reaches an
//! adapter as if it were markup, and that Cloudflare's `403` is reported as a
//! challenge rather than as the site refusing us.
//!
//! A real one-shot socket, not a mock: the whole point is what `HttpClient` does
//! with an actual response, and a mock would not exercise the client at all.
//! Nothing here touches the network beyond `127.0.0.1`.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::time::Duration;

use wnd_core::{BlockKind, Error, FetchPolicy, HttpClient};

/// Cloudflare's challenge, captured from `https://syosetu.org/novel/288118/`
/// on 2026-10-17.
const CLOUDFLARE_CHALLENGE: &str = include_str!("fixtures/blocked/cloudflare-challenge.html");

/// First Media's "Internet Sehat" page, captured from `http://syosetu.org/` on
/// 2026-10-17 after the DNS answer had been hijacked.
const INTERNET_SEHAT: &str = include_str!("fixtures/blocked/internet-sehat.html");

/// A client for a local server: no `robots.txt` to consult, no politeness delay.
fn client() -> HttpClient {
    HttpClient::new(FetchPolicy {
        respect_robots: false,
        min_delay: Duration::ZERO,
        ..FetchPolicy::default()
    })
    .expect("a client")
}

/// Serve exactly one response on a fresh port, and return the URL for it.
fn serve_once(status: &str, headers: &[(&str, &str)], body: &str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
    let address = listener.local_addr().expect("the bound address");

    let mut response = format!(
        "HTTP/1.1 {status}\r\n\
         Content-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n",
        body.len()
    );
    for (name, value) in headers {
        response.push_str(&format!("{name}: {value}\r\n"));
    }
    response.push_str("\r\n");
    response.push_str(body);

    std::thread::spawn(move || {
        let Ok((stream, _)) = listener.accept() else {
            return;
        };
        // Read the request head first, so the client is not writing into a
        // socket we are about to close.
        let reading = stream.try_clone().expect("a second handle for reading");
        let mut reader = BufReader::new(reading);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) if line == "\r\n" || line == "\n" => break,
                Ok(_) => {}
            }
        }

        let mut writing = stream;
        writing.write_all(response.as_bytes()).ok();
        writing.flush().ok();
    });

    format!("http://{address}/chapter")
}

#[test]
fn a_cloudflare_challenge_is_reported_as_a_challenge_not_as_a_status() {
    let url = serve_once(
        "403 Forbidden",
        &[("Cf-Mitigated", "challenge")],
        CLOUDFLARE_CHALLENGE,
    );

    match client()
        .get_text(&url)
        .expect_err("a challenge must not pass as a page")
    {
        Error::Blocked {
            status,
            kind,
            evidence,
            ..
        } => {
            assert_eq!(status, 403);
            assert_eq!(kind, BlockKind::BotChallenge);
            assert!(evidence.contains("Just a moment"), "{evidence}");
        }
        other => panic!("expected Error::Blocked, got {other:?}"),
    }
}

#[test]
fn the_challenge_message_names_the_status_and_quotes_the_page() {
    let url = serve_once(
        "403 Forbidden",
        &[("Cf-Mitigated", "challenge")],
        CLOUDFLARE_CHALLENGE,
    );

    let message = client()
        .get_text(&url)
        .expect_err("a challenge must not pass as a page")
        .to_string();

    // The exact sentence a user reads, `error: could not download <url>: `
    // being the CLI's own prefix.
    assert_eq!(
        message,
        "Cloudflare served its browser challenge instead of the site \
         (HTTP 403): the page says \"Just a moment...\""
    );
}

#[test]
fn a_filter_page_served_with_200_never_reaches_an_adapter() {
    let url = serve_once("200 OK", &[], INTERNET_SEHAT);
    let error = client()
        .get_text(&url)
        .expect_err("a block notice is not a novel page");

    assert_eq!(
        error.to_string(),
        "the network served an ISP content-filter page instead of the site \
         (HTTP 200): the page says \"internetsehat.firstmedia.com\""
    );

    match error {
        Error::Blocked { status, kind, .. } => {
            assert_eq!(status, 200, "the filter says 200, and so should we");
            assert_eq!(kind, BlockKind::ContentFilter);
        }
        other => panic!("expected Error::Blocked, got {other:?}"),
    }
}

#[test]
fn an_ordinary_page_still_arrives_as_text() {
    let url = serve_once("200 OK", &[], "<html><body><p>a chapter</p></body></html>");

    let body = client()
        .get_text(&url)
        .expect("a normal page must come back untouched");

    assert!(body.contains("a chapter"), "{body}");
}

#[test]
fn an_ordinary_404_is_still_an_http_status() {
    let url = serve_once(
        "404 Not Found",
        &[],
        "<html><body>no such chapter</body></html>",
    );

    match client().get_text(&url).expect_err("a 404 is an error") {
        Error::Http { status, .. } => assert_eq!(status, 404),
        other => panic!("expected Error::Http, got {other:?}"),
    }
}
