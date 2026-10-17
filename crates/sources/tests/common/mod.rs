#![allow(dead_code)] // every test binary compiles this module; not all use everything

//! Helpers shared by this crate's integration tests.
//!
//! `tests/` subfolders are modules, not test targets, so this file is never
//! compiled as a test binary of its own — it is included by the test files that
//! need it.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

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
