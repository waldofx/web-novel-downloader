#![allow(dead_code)] // every test binary compiles this module; not all use everything

//! Helpers shared by this crate's integration tests.
//!
//! `tests/` subfolders are modules, not test targets, so this file is never
//! compiled as a test binary of its own — it is included by the test files that
//! need it.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use reqwest::Url;
use wnd_core::{FetchPolicy, HttpClient};

/// A client using the default fetch policy.
pub fn client() -> HttpClient {
    HttpClient::new(FetchPolicy::default()).expect("client")
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
///
/// `Url` is `reqwest`'s re-export of the `url` crate's type, so this crate needs
/// no test-only dependency to name it.
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

/// A fresh directory under the system temporary directory.
pub fn temp_dir(tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("wnd-{tag}-{}-{nanos}", std::process::id()))
}

/// Serve one response on a fresh port and capture the `Cookie` header the
/// client sent, if any.
pub fn serve_one(response: &str) -> (String, Arc<Mutex<Option<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
    let address = listener.local_addr().expect("the bound address");
    let captured = Arc::new(Mutex::new(None));
    let captured_clone = Arc::clone(&captured);
    let response = response.to_string();

    std::thread::spawn(move || {
        if let Ok((stream, _)) = listener.accept() {
            let reading = stream.try_clone().expect("a second handle for reading");
            let mut reader = BufReader::new(reading);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) if line == "\r\n" || line == "\n" => break,
                    Ok(_) => {
                        if line.to_ascii_lowercase().starts_with("cookie:") {
                            *captured_clone.lock().expect("lock") =
                                Some(line.trim_end_matches(['\r', '\n']).to_string());
                        }
                    }
                }
            }
            let mut stream = stream;
            stream.write_all(response.as_bytes()).ok();
            stream.flush().ok();
        }
    });

    (format!("http://{address}/chapter"), captured)
}
