#![allow(dead_code)] // every test binary compiles this module; not all use everything

//! Helpers shared by this crate's integration tests.
//!
//! `tests/` subfolders are modules, not test targets, so this file is never
//! compiled as a test binary of its own — it is included by the test files that
//! need it.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use wnd_core::{FetchPolicy, HttpClient};

/// A client using the default fetch policy.
pub fn client() -> HttpClient {
    HttpClient::new(FetchPolicy::default()).expect("client")
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
