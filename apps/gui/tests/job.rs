//! What the window asks the library for, and what comes back.
//!
//! The fetching itself is tested in `wnd-download`; what matters here is that
//! the window gets the answers it renders — the validation it does before it
//! starts a thread, the shape of every event, and the report at the end.

mod common;

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use common::{FakeSource, TempDir, client};
use serde_json::json;
use wnd_core::ContentRating;
use wnd_core::session::cookie::Cookie;
use wnd_core::session::{Session, SessionStore};
use wnd_download::Progress;
use wnd_gui::job::{self, DownloadRequest, Event, ExportRequest, Settings, UpdateRequest};
use wnd_sources::Source;

/// The fake's URL, so that its own `matches` recognises what the request says.
const FAKE_URL: &str = "https://fake.example/novel/1";

fn download_request(output: &str) -> DownloadRequest {
    DownloadRequest {
        url: FAKE_URL.to_string(),
        output: output.to_string(),
        max_chapters: None,
        force: false,
        settings: Settings {
            export: vec!["epub".to_string()],
            ..Settings::default()
        },
    }
}

/// The same request against a real site's URL, for the checks that go through
/// the real registry rather than a fake adapter.
fn narou_request(output: &str) -> DownloadRequest {
    DownloadRequest {
        url: "https://ncode.syosetu.com/n0000aa/".to_string(),
        ..download_request(output)
    }
}

/// Download the fake novel into a fresh directory, returning the report and the
/// events it produced.
fn run_download(source: FakeSource, dir: &TempDir) -> (job::Report, Vec<Event>) {
    let fetches = source.fetches();
    let sources: Vec<Box<dyn Source>> = vec![Box::new(source)];
    let request = download_request(&dir.path().display().to_string());
    let prepared = job::prepare_download_with(&sources, &request).expect("the fake handles it");

    let mut events = Vec::new();
    let report = job::download(&sources, &client(), &prepared, |event| events.push(event))
        .expect("the download");
    assert_eq!(fetches.load(Ordering::SeqCst), report.chapters_fetched);
    (report, events)
}

#[test]
fn a_url_without_a_scheme_is_still_a_url() {
    assert_eq!(
        job::url_from_input("https://ncode.syosetu.com/n0000aa/")
            .unwrap()
            .as_str(),
        "https://ncode.syosetu.com/n0000aa/"
    );
    // What someone copies out of an address bar's text, or types from memory.
    assert_eq!(
        job::url_from_input("  ncode.syosetu.com/n0000aa/  ")
            .unwrap()
            .as_str(),
        "https://ncode.syosetu.com/n0000aa/"
    );

    assert!(
        job::url_from_input("   ")
            .unwrap_err()
            .contains("paste a novel URL")
    );
    assert!(job::url_from_input("two words").is_err());
}

#[test]
fn a_download_is_checked_before_a_thread_starts() {
    let good = job::prepare_download(&narou_request("some/where")).expect("narou handles this");
    assert_eq!(good.export.len(), 1);
    assert_eq!(good.download.max_chapters, None);

    // Nothing here handles it, and saying so now beats a thread that fails.
    let unknown = job::prepare_download(&DownloadRequest {
        url: "https://example.com/novel/1".to_string(),
        ..narou_request("some/where")
    })
    .unwrap_err();
    assert!(unknown.contains("no source adapter handles"), "{unknown}");

    let nowhere = job::prepare_download(&narou_request("   ")).unwrap_err();
    assert!(nowhere.contains("choose where"), "{nowhere}");

    // "at most none" is a request to do nothing, and is refused as one.
    let none = job::prepare_download(&DownloadRequest {
        max_chapters: Some(0),
        ..narou_request("some/where")
    })
    .unwrap_err();
    assert!(none.contains("nothing to do"), "{none}");

    let odd_format = job::prepare_download(&DownloadRequest {
        settings: Settings {
            export: vec!["pdf".to_string()],
            ..Settings::default()
        },
        ..narou_request("some/where")
    })
    .unwrap_err();
    assert!(odd_format.contains("not an export format"), "{odd_format}");

    let odd_ruby = job::prepare_download(&DownloadRequest {
        settings: Settings {
            ruby: Some("furigana please".to_string()),
            ..Settings::default()
        },
        ..narou_request("some/where")
    })
    .unwrap_err();
    assert!(odd_ruby.contains("not a furigana style"), "{odd_ruby}");
}

#[test]
fn a_folder_that_holds_no_novel_is_refused() {
    let dir = TempDir::new("empty-folder");
    let where_ = dir.path().display().to_string();

    let update = job::prepare_update(&UpdateRequest {
        dir: where_.clone(),
        check: false,
        settings: Settings::default(),
    })
    .unwrap_err();
    assert!(update.contains("manifest not found"), "{update}");

    let export = job::prepare_export(&ExportRequest {
        dir: "  ".to_string(),
        settings: Settings {
            export: vec!["epub".to_string()],
            ..Settings::default()
        },
    })
    .unwrap_err();
    assert!(export.contains("choose a directory"), "{export}");
}

#[test]
fn the_events_are_the_shape_the_window_reads() {
    let events = [
        Event::from_progress(Progress::Planned {
            title: "挿絵の本".to_string(),
            chapters: 10,
            to_fetch: 4,
        }),
        Event::from_progress(Progress::Fetched {
            index: 7,
            title: "第七話".to_string(),
            bytes: 1234,
        }),
        Event::from_progress(Progress::Skipped {
            index: 8,
            title: "第八話".to_string(),
        }),
        Event::from_progress(Progress::Checked {
            index: 9,
            title: "第九話".to_string(),
        }),
        Event::from_progress(Progress::ImageFetched {
            file: "images/0007-1.png".to_string(),
            bytes: 99,
        }),
        Event::from_progress(Progress::ImageMissing {
            url: "https://i.pximg.net/1.png".to_string(),
            reason: "http 404".to_string(),
        }),
        Event::Removed {
            index: 3,
            title: "第三話".to_string(),
        },
        Event::Exported {
            format: "epub".to_string(),
            path: "exports/本.epub".to_string(),
            bytes: 4096,
        },
        Event::Failed {
            message: "no source adapter handles https://x/".to_string(),
        },
    ];

    let shapes: Vec<serde_json::Value> = events
        .iter()
        .map(|event| serde_json::to_value(event).expect("an event serializes"))
        .collect();

    assert_eq!(
        shapes,
        vec![
            json!({"kind": "planned", "title": "挿絵の本", "chapters": 10, "toFetch": 4}),
            json!({"kind": "fetched", "index": 7, "title": "第七話", "bytes": 1234}),
            json!({"kind": "skipped", "index": 8, "title": "第八話"}),
            json!({"kind": "checked", "index": 9, "title": "第九話"}),
            json!({"kind": "imageFetched", "file": "images/0007-1.png", "bytes": 99}),
            json!({"kind": "imageMissing", "url": "https://i.pximg.net/1.png", "reason": "http 404"}),
            json!({"kind": "removed", "index": 3, "title": "第三話"}),
            json!({"kind": "exported", "format": "epub", "path": "exports/本.epub", "bytes": 4096}),
            json!({"kind": "failed", "message": "no source adapter handles https://x/"}),
        ]
    );
}

#[test]
fn a_download_reports_every_step_and_writes_what_was_asked_for() {
    let dir = TempDir::new("download");
    let (report, events) = run_download(FakeSource::new("挿絵の本").with_chapters(3), &dir);

    // The story the window tells, in order: the plan, three chapters, the
    // export. `Done` is not here — the command layer adds it around the report
    // this returns, so that a run that failed cannot also say it finished.
    let kinds: Vec<&str> = events
        .iter()
        .map(|event| match event {
            Event::Planned { .. } => "planned",
            Event::Fetched { .. } => "fetched",
            Event::Skipped { .. } => "skipped",
            Event::Checked { .. } => "checked",
            Event::ImageFetched { .. } => "imageFetched",
            Event::ImageMissing { .. } => "imageMissing",
            Event::Removed { .. } => "removed",
            Event::Exported { .. } => "exported",
            Event::Done { .. } => "done",
            Event::Failed { .. } => "failed",
        })
        .collect();
    assert_eq!(
        kinds,
        ["planned", "fetched", "fetched", "fetched", "exported"],
        "{events:#?}"
    );

    match &events[0] {
        Event::Planned {
            title,
            chapters,
            to_fetch,
        } => {
            assert_eq!(title, "挿絵の本");
            assert_eq!((*chapters, *to_fetch), (3, 3));
        }
        other => panic!("the first event is the plan: {other:?}"),
    }

    assert_eq!(report.title, "挿絵の本");
    assert_eq!(report.author.as_deref(), Some("Someone"));
    assert_eq!(report.rating, Some(ContentRating::General));
    assert_eq!(report.chapters_total, 3);
    assert_eq!(report.chapters_fetched, 3);
    assert_eq!(report.chapters_skipped, 0);
    assert_eq!((report.images_fetched, report.images_missing), (0, 0));
    assert!(report.bytes_written > 0);
    assert!(!report.up_to_date);
    assert!(report.removed.is_empty());

    // The chapters are on disk, in the folder the report names.
    let novel = PathBuf::from(&report.output_dir);
    assert!(novel.join("chapters/0001.json").is_file(), "{novel:?}");
    assert!(novel.join("chapters/0003.json").is_file(), "{novel:?}");

    // And so is the export that was asked for.
    assert_eq!(report.exported.len(), 1);
    assert_eq!(report.exported[0].format, "epub");
    assert_eq!(report.exported[0].chapters, 3);
    assert!(Path::new(&report.exported[0].path).is_file());

    // The report the command layer wraps in `Done`, as the window reads it.
    let done = serde_json::to_value(Event::Done {
        report: report.clone(),
    })
    .expect("a done event");
    assert_eq!(done["kind"], "done");
    assert_eq!(done["report"]["title"], "挿絵の本");
    assert_eq!(done["report"]["chaptersFetched"], 3);
    assert_eq!(done["report"]["rating"], "general");
    assert_eq!(done["report"]["upToDate"], false);
    assert_eq!(done["report"]["exported"][0]["format"], "epub");
    assert_eq!(done["report"]["exported"][0]["chapters"], 3);
}

#[test]
fn a_second_download_fetches_nothing_and_says_so() {
    let dir = TempDir::new("resume");
    let (first, _) = run_download(FakeSource::new("挿絵の本").with_chapters(3), &dir);
    let novel = first.output_dir.clone();

    let source = FakeSource::new("挿絵の本").with_chapters(3);
    let fetches = source.fetches();
    let sources: Vec<Box<dyn Source>> = vec![Box::new(source)];
    let prepared = job::prepare_download_with(
        &sources,
        &download_request(&dir.path().display().to_string()),
    )
    .expect("the fake handles it");

    let mut events = Vec::new();
    let again = job::download(&sources, &client(), &prepared, |event| events.push(event))
        .expect("the second download");

    assert_eq!(again.output_dir, novel, "the same novel, the same folder");
    assert_eq!(again.chapters_fetched, 0);
    assert_eq!(again.chapters_skipped, 3);
    assert!(
        again.up_to_date,
        "the novel was already complete; an export is not a fetch"
    );
    // Bytes of content: the export is not chapter content and is not counted.
    assert_eq!(again.bytes_written, 0);
    assert_eq!(
        fetches.load(Ordering::SeqCst),
        0,
        "a resume does not ask the site for a chapter it already has"
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::Skipped { .. })),
        "the window is told what was skipped: {events:#?}"
    );
}

#[test]
fn an_update_says_when_there_was_nothing_to_do() {
    let dir = TempDir::new("update");
    let (first, _) = run_download(FakeSource::new("挿絵の本").with_chapters(2), &dir);

    let source = FakeSource::new("挿絵の本").with_chapters(2);
    let fetches = source.fetches();
    let sources: Vec<Box<dyn Source>> = vec![Box::new(source)];

    // Without --check nothing is fetched: the index is all there is to go on.
    let prepared = job::prepare_update(&UpdateRequest {
        dir: first.output_dir.clone(),
        check: false,
        settings: Settings::default(),
    })
    .expect("the folder holds a novel");
    let report = job::update(&sources, &client(), &prepared, |_event| {}).expect("the update");

    assert!(report.up_to_date);
    assert_eq!(report.chapters_skipped, 2);
    assert_eq!(report.chapters_fetched, 0);
    assert_eq!(report.chapters_checked, 0);
    assert!(report.exported.is_empty());
    assert_eq!(fetches.load(Ordering::SeqCst), 0);

    // With `check`, every chapter is fetched to compare it, and none of them
    // turns out to have changed.
    let prepared = job::prepare_update(&UpdateRequest {
        dir: first.output_dir.clone(),
        check: true,
        settings: Settings::default(),
    })
    .expect("the folder holds a novel");
    let report = job::update(&sources, &client(), &prepared, |_event| {}).expect("the check");

    assert!(
        report.up_to_date,
        "a check that finds nothing is up to date"
    );
    assert_eq!(report.chapters_checked, 2);
    assert_eq!(report.chapters_fetched, 0);
    assert_eq!(
        fetches.load(Ordering::SeqCst),
        2,
        "the check exactly is the requests it promises"
    );
}

#[test]
fn info_counts_what_the_folder_has_and_what_it_has_lost() {
    let dir = TempDir::new("info");
    let (report, _) = run_download(FakeSource::new("挿絵の本").with_chapters(2), &dir);
    let novel = PathBuf::from(&report.output_dir);

    let info = job::info(&novel).expect("the folder it just wrote");
    assert_eq!(info.title, "挿絵の本");
    assert_eq!(info.author.as_deref(), Some("Someone"));
    assert_eq!(info.rating, ContentRating::General);
    assert_eq!(info.source, "fake");
    assert_eq!(info.novel_url, FAKE_URL);
    assert_eq!(info.language, "ja");
    assert_eq!(info.chapters, 2);
    assert_eq!(info.images, 0);
    assert_eq!(info.chapters_missing, 0);
    assert_eq!(info.images_missing, 0);
    assert_eq!(info.output_dir, novel.display().to_string());

    // A chapter someone deleted is reported, not hidden: an exporter would
    // quietly leave the gap.
    std::fs::remove_file(novel.join("chapters/0001.json")).expect("the chapter file");
    let after = job::info(&novel).expect("the folder after the deletion");
    assert_eq!(after.chapters_missing, 1);
    assert_eq!(after.chapters, 2, "the index still lists both");
}

#[test]
fn the_picker_shows_the_sessions_a_store_holds() {
    let dir = TempDir::new("sessions");
    let store = SessionStore::at(dir.path());
    store
        .save(&Session::from_cookies(
            "pixiv",
            vec![Cookie {
                domain: "pixiv.net".to_string(),
                host_only: false,
                path: "/".to_string(),
                secure: true,
                expires_at: Some(1_800_000_000),
                name: "PHPSESSID".to_string(),
                value: "hunter2".to_string(),
            }],
        ))
        .expect("a saved session");

    let listed = job::sessions_in(dir.path()).expect("a listing");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "pixiv");
    assert_eq!(listed[0].hosts, ["pixiv.net"]);
    assert_eq!(listed[0].cookies, 1);
    assert_eq!(listed[0].expires_at, Some(1_800_000_000));

    // The shape the picker reads — and no cookie value in it, ever.
    let shape = serde_json::to_value(&listed[0]).expect("a session serializes");
    assert_eq!(shape["name"], "pixiv");
    assert_eq!(shape["hosts"], json!(["pixiv.net"]));
    assert_eq!(shape["cookies"], 1);
    assert_eq!(shape["expiresAt"], 1_800_000_000_u64);
    assert!(
        shape["savedAt"].as_u64().expect("a saved time") > 0,
        "the manager says how old a session is"
    );
    assert!(!shape.to_string().contains("hunter2"));
}

// ------------------------------------------------------------------ sessions

/// A store directory that is not the user's own.
fn store(dir: &TempDir) -> PathBuf {
    dir.path().join("sessions")
}

/// A Netscape export with one cookie for `domain`.
fn export(domain: &str) -> String {
    format!("# Netscape HTTP Cookie File\n{domain}\tFALSE\t/\tFALSE\t0\tPHPSESSID\tsecret\n")
}

#[test]
fn a_cookies_file_is_saved_as_a_session() {
    let dir = TempDir::new("session-file");
    let store = store(&dir);
    let file = dir.path().join("cookies.txt");
    std::fs::write(&file, export("example.com")).expect("a cookies file");

    job::save_session_file_in(&store, "example", &file).expect("the session is saved");

    let listed = job::sessions_in(&store).expect("a listing");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "example");
    assert_eq!(listed[0].hosts, ["example.com"]);
    assert_eq!(listed[0].cookies, 1);
}

#[test]
fn a_cookie_header_is_saved_for_the_site_it_names() {
    let dir = TempDir::new("session-header");
    let store = store(&dir);

    job::save_session_header_in(&store, "adult", "novel18.example", "over18=yes; sess=abc")
        .expect("the session is saved");

    let listed = job::sessions_in(&store).expect("a listing");
    assert_eq!(listed[0].hosts, ["novel18.example"]);
    assert_eq!(listed[0].cookies, 2);

    let session = SessionStore::at(&store)
        .load("adult")
        .expect("the session is there");
    assert!(session.covers("novel18.example"));
    assert!(
        !session.covers("ncode.example"),
        "a header names one site only"
    );
}

#[test]
fn a_site_pasted_as_a_url_is_reduced_to_its_host() {
    let dir = TempDir::new("session-url");
    let store = store(&dir);

    job::save_session_header_in(
        &store,
        "adult",
        "https://Novel18.Example/story/1/",
        "over18=yes",
    )
    .expect("the site is understood");

    let listed = job::sessions_in(&store).expect("a listing");
    assert_eq!(
        listed[0].hosts,
        ["novel18.example"],
        "a pasted address is a site"
    );
}

#[test]
fn a_blank_name_is_refused() {
    let dir = TempDir::new("session-blank");
    let store = store(&dir);

    let error = job::save_session_header_in(&store, "   ", "example.com", "a=b")
        .expect_err("a session needs a name");
    assert!(error.contains("name"), "{error}");
    assert!(job::sessions_in(&store).expect("a listing").is_empty());
}

#[test]
fn an_empty_export_is_refused() {
    let dir = TempDir::new("session-empty-file");
    let store = store(&dir);
    let file = dir.path().join("cookies.txt");
    std::fs::write(&file, "# Netscape HTTP Cookie File\n").expect("an empty export");

    let error =
        job::save_session_file_in(&store, "empty", &file).expect_err("no cookies is not a session");
    assert!(error.contains("no cookies"), "{error}");
    assert!(job::sessions_in(&store).expect("a listing").is_empty());
}

#[test]
fn an_empty_header_is_refused() {
    let dir = TempDir::new("session-empty-header");
    let store = store(&dir);

    let error = job::save_session_header_in(&store, "empty", "example.com", " ; ; ")
        .expect_err("no cookies is not a session");
    assert!(error.contains("no cookies"), "{error}");
}

#[test]
fn a_missing_file_says_which_one() {
    let dir = TempDir::new("session-missing-file");
    let store = store(&dir);
    let file = dir.path().join("there-is-no-cookies.txt");

    let error = job::save_session_file_in(&store, "gone", &file)
        .expect_err("a file that is not there cannot be read");
    assert!(error.contains("there-is-no-cookies.txt"), "{error}");
}

#[test]
fn saving_the_same_name_replaces_the_session() {
    let dir = TempDir::new("session-replace");
    let store = store(&dir);

    job::save_session_header_in(&store, "site", "a.example", "one=1").expect("the first save");
    job::save_session_header_in(&store, "site", "b.example", "two=2").expect("a refresh");

    let listed = job::sessions_in(&store).expect("a listing");
    assert_eq!(listed.len(), 1, "a name is one session, not two");
    assert_eq!(listed[0].hosts, ["b.example"]);
    assert_eq!(listed[0].cookies, 1);
}

#[test]
fn forgetting_a_session_removes_it() {
    let dir = TempDir::new("session-forget");
    let store = store(&dir);
    job::save_session_header_in(&store, "site", "example.com", "a=b").expect("a saved session");

    job::forget_session_in(&store, "site").expect("it is forgotten");
    assert!(job::sessions_in(&store).expect("a listing").is_empty());

    let error = job::forget_session_in(&store, "site").expect_err("it is already gone");
    assert!(error.contains("site"), "{error}");
}

#[test]
fn a_broken_export_does_not_echo_a_cookie_value() {
    let dir = TempDir::new("session-broken");
    let store = store(&dir);
    let file = dir.path().join("cookies.txt");
    std::fs::write(&file, "not a cookie line, hunter2\n").expect("a broken export");

    let error =
        job::save_session_file_in(&store, "broken", &file).expect_err("a broken export is refused");
    assert!(
        !error.contains("hunter2"),
        "a value must not travel in an error: {error}"
    );
}
