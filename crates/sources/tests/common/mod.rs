#![allow(dead_code)] // every test binary compiles this module; not all use everything

//! Helpers shared by this crate's integration tests.
//!
//! `tests/` subfolders are modules, not test targets, so this file is never
//! compiled as a test binary of its own — it is included by the test files that
//! need it.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use url::Url;
use wnd_core::{FetchPolicy, HttpClient};

/// A client using the default fetch policy, for the live tests.
pub fn client() -> HttpClient {
    HttpClient::new(FetchPolicy::default()).expect("client")
}

/// A client for a local, one-shot server: no `robots.txt` to consult, no
/// politeness delay, and no retries — a test must not sleep its way through a
/// backoff.
pub fn local_client() -> HttpClient {
    HttpClient::new(FetchPolicy {
        respect_robots: false,
        min_delay: Duration::ZERO,
        max_retries: 1,
        ..FetchPolicy::default()
    })
    .expect("client")
}

/// The works this machine is allowed to hit: the gitignored `fixtures.local.json`
/// at the workspace root. An absent file, an absent site, or an entry that does
/// not fit yields nothing — a live test skips rather than fails, and says so.
///
/// The file is the user's own and never enters the repository; the committed
/// `fixtures.local.example.json` is only a template. The root is found by walking
/// up from this crate until a directory holds that example beside a `Cargo.toml`,
/// rather than counting `../..`, so moving the crate cannot point the tests at
/// the wrong file. A file that exists but is not JSON is a broken local file,
/// not an empty one: it panics rather than looking like "nothing configured".
pub fn targets(site: &str) -> Vec<Url> {
    let Some(path) = fixtures_file() else {
        return Vec::new();
    };

    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        // Absent is not broken: nothing is configured, so a live test skips.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(error) => panic!("cannot read {}: {error}", path.display()),
    };

    let value: serde_json::Value = serde_json::from_str(&text).unwrap_or_else(|error| {
        panic!(
            "{} is not valid JSON: {error}; it must match fixtures.local.example.json",
            path.display()
        )
    });

    let Some(entries) = value.get(site).and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| entry.get("url")?.as_str())
        .filter_map(|raw| Url::parse(raw).ok())
        .collect()
}

/// Print, into the output a human running the test sees, that this machine has
/// nothing configured for `site`.
pub fn note_no_target(site: &str) {
    println!(
        "note: no `{site}` target in fixtures.local.json — skipping \
         (see fixtures.local.example.json)"
    );
}

/// The gitignored fixtures file, if this machine has one.
///
/// The committed example beside a `Cargo.toml` marks the workspace root — a
/// crate directory has a `Cargo.toml` but never the example — so the walk stops
/// there, whatever a future crate's depth.
fn fixtures_file() -> Option<std::path::PathBuf> {
    let mut dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    loop {
        if dir.join("Cargo.toml").is_file() && dir.join("fixtures.local.example.json").is_file() {
            let local = dir.join("fixtures.local.json");
            return local.is_file().then_some(local);
        }
        dir = dir.parent()?;
    }
}

/// One canned HTTP response.
pub fn response(status: &str, headers: &[(&str, &str)], body: &str) -> String {
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
/// Returns the URL of a fixed path on that server. A test that serves two
/// responses is saying what a second request — a host probe, a redirect — will
/// be given.
pub fn serve(responses: Vec<String>) -> String {
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
