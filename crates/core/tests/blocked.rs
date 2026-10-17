//! What the fetch layer does with a page that is not the site.
//!
//! Three things are checked here, and they are the whole of the dynamic rule:
//!
//! - Cloudflare's challenge is caught from its `Cf-Mitigated` **header**, and no
//!   page text is matched anywhere — a challenge page that arrives without the
//!   header, or a filter page, is just a page;
//! - a page that is not the site is described by the facts of the answer and the
//!   text that arrived ([`Page::not_the_site`]), because the page names itself in
//!   whatever language the reader's network uses;
//! - a host that answers **every** URL with the same page is caught by asking it
//!   for a path that cannot exist, which is the one signal that needs no
//!   vocabulary at all.
//!
//! A real one-shot socket, not a mock: the whole point is what `HttpClient` does
//! with an actual response, and a mock would not exercise the client at all.
//! Nothing here touches the network beyond `127.0.0.1`.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use wnd_core::{BlockKind, Error, FetchPolicy, HttpClient};

/// Cloudflare's challenge, captured from `https://syosetu.org/novel/288118/`
/// on 2026-10-17.
const CLOUDFLARE_CHALLENGE: &str = include_str!("fixtures/blocked/cloudflare-challenge.html");

/// First Media's "Internet Sehat" page, captured from `http://syosetu.org/` on
/// 2026-10-17 after the DNS answer had been hijacked.
const INTERNET_SEHAT: &str = include_str!("fixtures/blocked/internet-sehat.html");

/// The Komdigi/Trustpositif notice, as pasted from the page the user's network
/// served. The filter box handed a *different* template to a VPN exit, so this
/// one could not be captured from this side.
const KOMDIGI_NOTICE: &str = concat!(
    "<html><head><title>Trustpositif</title></head><body>",
    "<h1>SITUS DIBLOKIR / WEBSITE BLOCKED</h1>",
    "<p>MAAF, AKSES KE SITUS INI DIBLOKIR OLEH PEMERINTAH INDONESIA KARENA ",
    "MENGANDUNG KONTEN NEGATIF</p>",
    "<p>aduanpseprivat@komdigi.go.id</p></body></html>",
);

/// A client for a local server: no `robots.txt` to consult, no politeness delay,
/// and no retries — a test must not sleep its way through a backoff.
fn client() -> HttpClient {
    HttpClient::new(FetchPolicy {
        respect_robots: false,
        min_delay: Duration::ZERO,
        max_retries: 1,
        ..FetchPolicy::default()
    })
    .expect("a client")
}

/// One canned HTTP response.
fn response(status: &str, headers: &[(&str, &str)], body: &str) -> String {
    let mut out = format!(
        "HTTP/1.1 {status}\r\n\
         Content-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n",
        body.len()
    );
    for (name, value) in headers {
        out.push_str(&format!("{name}: {value}\r\n"));
    }
    out.push_str("\r\n");
    out.push_str(body);
    out
}

/// Serve `responses` on a fresh port, one per connection, in order.
///
/// Returns the URL of a fixed path on that server, so a second server can be
/// built first and named in a `Location` header.
fn serve(responses: Vec<String>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
    let address = listener.local_addr().expect("the bound address");

    std::thread::spawn(move || {
        for response in responses {
            let Ok((stream, _)) = listener.accept() else {
                return;
            };
            // Read the request head first, so the client is not writing into a
            // socket we are about to close.
            read_request(&stream);
            let mut stream = stream;
            stream.write_all(response.as_bytes()).ok();
            stream.flush().ok();
        }
    });

    format!("http://{address}/chapter")
}

/// Read a request head, and nothing beyond it.
fn read_request(stream: &TcpStream) {
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
}

#[test]
fn a_cloudflare_challenge_is_reported_as_a_challenge_not_as_a_status() {
    let url = serve(vec![response(
        "403 Forbidden",
        &[("Cf-Mitigated", "challenge")],
        CLOUDFLARE_CHALLENGE,
    )]);

    match client()
        .get_page(&url)
        .expect_err("a challenge must not pass as a page")
    {
        Error::Blocked { status, kind, .. } => {
            assert_eq!(status, 403);
            assert_eq!(kind, BlockKind::BotChallenge);
        }
        other => panic!("expected Error::Blocked, got {other:?}"),
    }
}

#[test]
fn the_challenge_message_names_the_status_and_the_header() {
    let url = serve(vec![response(
        "403 Forbidden",
        &[("Cf-Mitigated", "challenge")],
        CLOUDFLARE_CHALLENGE,
    )]);

    let message = client()
        .get_page(&url)
        .expect_err("a challenge must not pass as a page")
        .to_string();

    // The exact sentence a user reads, `error: could not download <url>: `
    // being the CLI's own prefix.
    assert_eq!(
        message,
        "Cloudflare served its browser challenge instead of the site (HTTP 403)\n  \
         the response header Cf-Mitigated: challenge"
    );
}

#[test]
fn a_challenge_page_without_the_header_is_only_a_page() {
    // The rule is the header. Matching `Just a moment...` in a body was how this
    // was done before, and a phrase list is exactly what this no longer keeps.
    let url = serve(vec![response("200 OK", &[], CLOUDFLARE_CHALLENGE)]);

    let page = client()
        .get_page(&url)
        .expect("without the header this is just a page");

    assert!(page.body.contains("Just a moment"), "the body is untouched");
}

#[test]
fn a_filter_page_served_with_200_is_only_a_page() {
    // Nothing in this layer knows the words `SITUS DIBLOKIR` any more. The
    // adapter that expected markup is what reports this — see
    // `crates/sources/src/lib.rs`, `with_evidence`.
    let url = serve(vec![response("200 OK", &[], INTERNET_SEHAT)]);

    let page = client().get_page(&url).expect("a filter says 200");

    assert_eq!(page.status, 200, "the filter says 200, and so do we");
    assert_eq!(page.body, INTERNET_SEHAT);
}

#[test]
fn the_facts_of_the_answer_travel_with_the_body() {
    let url = serve(vec![response(
        "200 OK",
        &[],
        "<html><p>a chapter</p></html>",
    )]);
    let page = client().get_page(&url).expect("a normal page");

    assert_eq!(page.status, 200);
    assert_eq!(page.url, url);
    assert_eq!(page.final_url, url, "nothing redirected");
    assert_eq!(
        page.content_type.as_deref(),
        Some("text/html; charset=utf-8"),
        "the media type is kept as sent; it is tidied where it is printed"
    );
}

#[test]
fn a_redirect_is_remembered_as_the_url_that_answered() {
    let final_url = serve(vec![response("200 OK", &[], "<p>moved</p>")]);
    let start = serve(vec![response("302 Found", &[("Location", &final_url)], "")]);

    let page = client().get_page(&start).expect("a redirect is followed");

    assert_eq!(page.url, start, "the URL asked for is kept");
    assert_eq!(page.final_url, final_url, "and so is the one that answered");
}

#[test]
fn a_page_that_is_not_the_site_is_reported_with_what_arrived() {
    let url = serve(vec![response("200 OK", &[], KOMDIGI_NOTICE)]);
    let page = client().get_page(&url).expect("the filter says 200");

    let message = page.not_the_site("a table of contents", false).to_string();

    assert_eq!(
        message,
        format!(
            "the page that arrived is not a novel page (HTTP 200)\n  \
             a table of contents was expected; {} bytes of text/html arrived instead\n  \
             it begins: \"Trustpositif SITUS DIBLOKIR / WEBSITE BLOCKED MAAF, AKSES KE \
             SITUS INI DIBLOKIR OLEH PEMERINTAH INDONESIA KARENA MENGANDUNG KONTEN \
             NEGATIF aduanpseprivat@komdigi.go.id\"",
            KOMDIGI_NOTICE.len()
        ),
        "the reader sees the page's own words, whatever language they are in"
    );
}

#[test]
fn a_host_that_answers_every_url_the_same_is_caught() {
    // Both the request and the probe get the same bytes: a DNS answer hijacked to
    // a filter box, a captive portal, and most content filters all behave this
    // way, and it needs no vocabulary to see.
    let body = KOMDIGI_NOTICE;
    let url = serve(vec![
        response("200 OK", &[], body),
        response("200 OK", &[], body),
    ]);

    assert!(
        client().answers_every_url_the_same(&url, body),
        "the same page for a URL that cannot exist is interception"
    );
}

#[test]
fn a_host_that_answers_the_probe_differently_is_not_intercepting() {
    // The probe is the only request this makes, and here the host answers it the
    // way a site answers a path that does not exist.
    let url = serve(vec![response("404 Not Found", &[], "<p>no such page</p>")]);

    assert!(
        !client().answers_every_url_the_same(&url, KOMDIGI_NOTICE),
        "a site answering its own 404 has not been hijacked"
    );
}

#[test]
fn a_host_that_cannot_be_reached_is_not_intercepting() {
    // A dead host is not evidence of interception: the probe failing must answer
    // "no", not panic or hang.
    let client = client();
    assert!(!client.answers_every_url_the_same("http://127.0.0.1:1/chapter", "anything"));
}

#[test]
fn an_ordinary_page_still_arrives_as_text() {
    let url = serve(vec![response(
        "200 OK",
        &[],
        "<html><body><p>a chapter</p></body></html>",
    )]);

    let page = client()
        .get_page(&url)
        .expect("a normal page must come back untouched");

    assert!(page.body.contains("a chapter"));
}

#[test]
fn an_ordinary_404_is_still_an_http_status() {
    let url = serve(vec![response(
        "404 Not Found",
        &[],
        "<html><body>no such chapter</body></html>",
    )]);

    match client().get_page(&url).expect_err("a 404 is an error") {
        Error::Http { status, .. } => assert_eq!(status, 404),
        other => panic!("expected Error::Http, got {other:?}"),
    }
}
