//! The session store: what it writes, what it lists, and what it never says.
//!
//! A session is a credential the user handed over on purpose, so the promises
//! checked here are the ones that rot silently if nothing watches them: no value
//! in a listing, in `Debug`, or in an error; a name that cannot escape its
//! directory; a host outside the session that never sees the jar; and a file only
//! its owner can read.

mod common;

use std::path::PathBuf;

use common::temp_dir;
use wnd_core::Error;
use wnd_core::session::cookie::Cookie;
use wnd_core::session::{Session, SessionStore};

/// A session named `name` holding one cookie for `pixiv.net`.
fn session(name: &str, value: &str) -> Session {
    Session::from_cookies(
        name,
        vec![Cookie {
            domain: "pixiv.net".to_string(),
            host_only: false,
            path: "/".to_string(),
            secure: true,
            expires_at: Some(1_800_000_000),
            name: "PHPSESSID".to_string(),
            value: value.to_string(),
        }],
    )
}

/// A store in a fresh directory, and that directory to clean up.
fn store(tag: &str) -> (SessionStore, PathBuf) {
    let dir = temp_dir(tag);
    (SessionStore::at(&dir), dir)
}

#[test]
fn a_session_round_trips_through_the_store() {
    let (store, dir) = store("session-round-trip");
    let saved = session("pixiv", "hunter2");

    let path = store.save(&saved).expect("a save");
    assert!(path.is_file(), "the file was written: {path:?}");

    let loaded = store.load("pixiv").expect("a load");
    assert_eq!(loaded, saved, "the cookies came back as they went in");
    assert_eq!(loaded.hosts(), ["pixiv.net"]);

    let listed = store.list().expect("a listing");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "pixiv");
    assert_eq!(listed[0].cookies, 1);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn saving_a_name_again_replaces_that_session() {
    let (store, dir) = store("session-refresh");

    store.save(&session("pixiv", "old")).expect("a first save");
    store.save(&session("pixiv", "new")).expect("a refresh");

    let loaded = store.load("pixiv").expect("a load");
    assert_eq!(
        loaded.cookies()[0].value,
        "new",
        "a refresh is the same gesture as a save"
    );
    assert_eq!(store.list().expect("a listing").len(), 1);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_listing_never_contains_a_cookie_value() {
    let (store, dir) = store("session-quiet");
    store.save(&session("pixiv", "hunter2")).expect("a save");

    let listed = store.list().expect("a listing");
    let printed = format!("{listed:#?}");
    assert!(
        printed.contains("pixiv.net"),
        "the host is not a secret: {printed}"
    );
    assert!(
        !printed.contains("hunter2"),
        "a listing is a summary and never a jar: {printed}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_earliest_expiry_is_the_one_a_listing_shows() {
    let (store, dir) = store("session-expiry");

    let mut cookies = session("mixed", "v").cookies().to_vec();
    cookies[0].expires_at = Some(1_800_000_000);
    cookies.push(Cookie {
        expires_at: Some(1_700_000_000),
        name: "later".to_string(),
        ..cookies[0].clone()
    });
    // A session cookie has no expiry, and must not hide the ones that do.
    cookies.push(Cookie {
        expires_at: None,
        name: "forever".to_string(),
        ..cookies[0].clone()
    });
    store
        .save(&Session::from_cookies("mixed", cookies))
        .expect("a save");

    assert_eq!(
        store.list().expect("a listing")[0].expires_at,
        Some(1_700_000_000),
        "the first cookie to expire is the one worth knowing about"
    );

    let none = Session::from_cookies(
        "none",
        vec![Cookie {
            expires_at: None,
            ..session("x", "v").cookies()[0].clone()
        }],
    );
    assert_eq!(none.expires_at(), None, "no expiry at all is not the epoch");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_session_is_only_offered_to_the_hosts_it_names() {
    let session = session("pixiv", "v");

    assert!(session.covers("pixiv.net"));
    assert!(
        session.covers("www.pixiv.net"),
        "the site is not always at the apex"
    );
    assert!(
        !session.covers("i.pximg.net"),
        "another site's images are not pixiv"
    );
    assert!(!session.covers("example.com"));

    let nowhere = Session::new("empty", Vec::new(), session.cookies().to_vec());
    assert!(
        !nowhere.covers("pixiv.net"),
        "a session with no hosts goes nowhere, whatever it holds"
    );
}

#[test]
fn a_name_that_could_escape_the_directory_is_refused() {
    let (store, dir) = store("session-names");
    std::fs::create_dir_all(&dir).expect("a directory");
    let outside = dir.parent().expect("a parent").join("escaped.json");

    for name in ["../escaped", "a/b", "a\\b", "", ".", "..", ".hidden", "a b"] {
        let error = store
            .save(&session(name, "v"))
            .expect_err(&format!("`{name}` is not a session name"));
        assert!(
            matches!(error, Error::InvalidSessionName { .. }),
            "`{name}` gave {error}"
        );

        // The same name is refused on the way in and on the way out, so a
        // hand-written file cannot be reached by a name either.
        assert!(matches!(
            store.load(name),
            Err(Error::InvalidSessionName { .. })
        ));
        assert!(matches!(
            store.delete(name),
            Err(Error::InvalidSessionName { .. })
        ));
    }

    let long = "a".repeat(65);
    assert!(matches!(
        store.save(&session(&long, "v")),
        Err(Error::InvalidSessionName { .. })
    ));
    assert!(
        !outside.exists(),
        "nothing was written outside the directory"
    );

    // The characters that are allowed, including the ones a site names itself.
    store
        .save(&session("h.syosetu-org_2", "v"))
        .expect("a reasonable name saves");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn deleting_a_session_that_is_not_there_names_it() {
    let (store, dir) = store("session-missing");
    std::fs::create_dir_all(&dir).expect("a directory");

    match store.delete("pixiv") {
        Err(Error::SessionNotFound(name)) => assert_eq!(name, "pixiv"),
        other => panic!("expected a missing session, got {other:?}"),
    }
    match store.load("pixiv") {
        Err(Error::SessionNotFound(name)) => assert_eq!(name, "pixiv"),
        other => panic!("expected a missing session, got {other:?}"),
    }

    store.save(&session("pixiv", "v")).expect("a save");
    store.delete("pixiv").expect("a delete");
    assert!(store.list().expect("a listing").is_empty(), "it is gone");
    assert!(!store.dir().join("pixiv.json").exists());

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_directory_that_does_not_exist_yet_lists_as_nothing() {
    let (store, dir) = store("session-absent");
    assert!(store.list().expect("a listing").is_empty());
    assert!(!dir.exists(), "listing does not create the directory");
}

#[test]
fn a_file_that_is_not_a_session_is_named_in_the_error() {
    let (store, dir) = store("session-foreign");
    std::fs::create_dir_all(&dir).expect("a directory");
    std::fs::write(dir.join("stray.json"), "{\"not\": \"a session\"}").expect("a stray file");

    let error = store.list().expect_err("a foreign file stops the listing");
    let message = error.to_string();
    assert!(message.contains("stray.json"), "{message}");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_session_never_prints_its_cookie_values() {
    let session = session("pixiv", "hunter2");

    let printed = format!("{session:?}");
    assert!(
        printed.contains("pixiv"),
        "the name is not a secret: {printed}"
    );
    assert!(printed.contains("pixiv.net"), "nor is the host: {printed}");
    assert!(!printed.contains("hunter2"), "the jar is: {printed}");
    assert!(
        !printed.contains("PHPSESSID"),
        "a session is summarised, not described: {printed}"
    );
}

#[cfg(unix)]
#[test]
fn the_session_file_is_readable_only_by_its_owner() {
    use std::os::unix::fs::PermissionsExt;

    let (store, dir) = store("session-permissions");
    let path = store.save(&session("pixiv", "v")).expect("a save");

    let mode = std::fs::metadata(&path)
        .expect("metadata")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600, "a credential is its owner's business");

    std::fs::remove_dir_all(&dir).ok();
}
