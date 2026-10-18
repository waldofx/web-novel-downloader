//! The front end, checked against the layer it talks to.
//!
//! A window is the one thing in this repository a test cannot click, and the two
//! files on either side of the IPC are one rename apart from a blank window with
//! an error nobody sees. So the agreement between them — the command names, the
//! channel, the element ids — is asserted here by reading both, which is a poor
//! substitute for clicking and a much better one than hoping.

use std::collections::BTreeSet;
use std::path::Path;

/// A file of the front end, next to this crate.
fn ui(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("ui").join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// The command layer's own source.
fn layer() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("lib.rs");
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// Every quoted word that follows `after`, over whitespace and newlines:
/// `el("url")` gives `url`, and so does `start(\n  "update",`.
fn quoted_after(source: &str, after: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut rest = source;
    while let Some(at) = rest.find(after) {
        rest = &rest[at + after.len()..];
        let Some(body) = rest.trim_start().strip_prefix('"') else {
            continue;
        };
        let Some(end) = body.find('"') else { continue };
        found.insert(body[..end].to_string());
        rest = &body[end + 1..];
    }
    found
}

/// The words inside a bracketed literal, `NAME = [ "a", "b" ]` or a macro call.
fn bracketed(source: &str, after: &str) -> BTreeSet<String> {
    let Some(at) = source.find(after) else {
        panic!("`{after}` is not in the file any more");
    };
    let rest = &source[at + after.len()..];
    let end = rest.find(']').expect("a closing bracket");
    rest[..end]
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
        .filter(|word| !word.is_empty() && *word != "generate_handler")
        .map(str::to_string)
        .collect()
}

/// Every `invoke("name", …)` and `start("name", …)` in the front end.
fn asked_for(js: &str) -> BTreeSet<String> {
    let mut commands = quoted_after(js, "invoke(");
    commands.extend(quoted_after(js, "start("));
    commands
}

#[test]
fn the_window_can_only_ask_for_commands_the_app_has() {
    let asked = asked_for(&ui("app.js"));
    let registered = bracketed(&layer(), "generate_handler![");

    assert!(
        asked.len() > 1,
        "the front end's own calls were not found in it: {asked:?}"
    );
    assert_eq!(
        asked, registered,
        "every command the window asks for is one the app registers, and every \
         command the app registers is one the window asks for"
    );
}

#[test]
fn the_channel_the_front_end_listens_to_is_the_one_the_job_reports_on() {
    // The one string both sides have to spell the same way: a typo in either
    // would leave the window silent with the work going on without it.
    let js = ui("app.js");
    let channel = format!("\"{}\"", wnd_gui::EVENT);
    assert!(
        js.contains(&channel),
        "the front end has to listen on {channel}"
    );
}

#[test]
fn every_element_the_front_end_looks_for_is_in_the_page() {
    let js = ui("app.js");
    let html = ui("index.html");

    // `el("url")`, and the buttons that are reached through their own list.
    let mut wanted = quoted_after(&js, "el(");
    wanted.extend(bracketed(&js, "BUTTONS = ["));
    assert!(
        wanted.len() > 5,
        "no element lookups were found: {wanted:?}"
    );

    let page = quoted_after(&html, "id=");
    let missing: Vec<&String> = wanted.difference(&page).collect();
    assert!(
        missing.is_empty(),
        "the page has no {missing:?}; the front end would throw where nobody can see it"
    );
}

#[test]
fn the_page_loads_what_the_app_embeds() {
    // `frontendDist` in `tauri.conf.json` is this directory, so a file that is
    // never loaded is a file nobody will notice is broken.
    let html = ui("index.html");
    for name in ["app.js", "style.css"] {
        assert!(html.contains(name), "the page does not load {name}");
        assert!(!ui(name).is_empty(), "{name} is empty");
    }

    let config = {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tauri.conf.json");
        std::fs::read_to_string(path).expect("the app's configuration")
    };
    assert!(
        config.contains("\"frontendDist\": \"ui\""),
        "the embedded front end is the directory these tests read"
    );
}
