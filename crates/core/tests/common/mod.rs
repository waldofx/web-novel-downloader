#![allow(dead_code)] // every test binary compiles this module; not all use everything

//! Helpers shared by this crate's integration tests.
//!
//! `tests/` subfolders are modules, not test targets, so this file is never
//! compiled as a test binary of its own — it is included by the test files that
//! need it.

use wnd_core::{FetchPolicy, HttpClient};

/// A client using the default fetch policy.
pub fn client() -> HttpClient {
    HttpClient::new(FetchPolicy::default()).expect("client")
}
