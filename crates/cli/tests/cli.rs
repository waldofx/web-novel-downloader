//! The `nvd` binary, driven as a user would drive it.
//!
//! These run the real executable, so they cover the parts the library tests
//! cannot: argument parsing, exit codes, which stream output goes to, and the
//! wording a user actually sees. Nothing here needs the network — the tests that
//! do are `#[ignore]`d.

mod common;

use std::fs;

use common::{TempDir, exported, novel, nvd, stderr, stdout};

#[test]
fn info_describes_a_downloaded_directory() {
    let (_guard, novel) = novel("info");

    let output = nvd(&["info", novel.root().to_str().unwrap()]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("偽の小説"), "got:\n{text}");
    assert!(text.contains("author:   著者"), "got:\n{text}");
    assert!(text.contains("chapters: 2"), "got:\n{text}");
}

#[test]
fn export_writes_the_requested_formats() {
    let (_guard, novel) = novel("export");

    let output = nvd(&[
        "export",
        novel.root().to_str().unwrap(),
        "--format",
        "txt,md",
    ]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(
        exported(novel.root(), "txt"),
        novel.exports_dir().join("偽の小説.txt")
    );
    assert_eq!(
        exported(novel.root(), "md"),
        novel.exports_dir().join("偽の小説.md")
    );

    // The result goes to stdout, so it can be captured.
    let text = stdout(&output);
    assert!(text.contains("偽の小説.txt"), "got:\n{text}");
    assert!(text.contains("偽の小説.md"), "got:\n{text}");
    assert!(text.contains("2 chapters"), "got:\n{text}");
}

#[test]
fn export_defaults_to_epub() {
    let (_guard, novel) = novel("export-default");

    let output = nvd(&["export", novel.root().to_str().unwrap()]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let epub = exported(novel.root(), "epub");
    assert!(
        !fs::read(&epub).unwrap().is_empty(),
        "the epub must not be empty"
    );
}

#[test]
fn a_directory_that_is_not_a_novel_is_an_error() {
    let guard = TempDir::new("not-a-novel");
    let dir = guard.path().to_str().unwrap();

    let output = nvd(&["export", dir, "--format", "txt"]);

    assert_eq!(output.status.code(), Some(1), "a failure must not exit 0");
    // The message names the directory that was tried, not just "file not found".
    assert!(stderr(&output).contains(dir), "got:\n{}", stderr(&output));
    assert!(stdout(&output).is_empty(), "nothing was produced");
}

#[test]
fn an_unknown_format_is_a_usage_error() {
    let (_guard, novel) = novel("bad-format");

    let output = nvd(&["export", novel.root().to_str().unwrap(), "--format", "pdf"]);

    assert_eq!(
        output.status.code(),
        Some(2),
        "clap reports usage errors as 2"
    );
    assert!(stderr(&output).contains("pdf"), "got:\n{}", stderr(&output));
}

#[test]
fn a_url_no_adapter_claims_fails_and_names_the_url() {
    let guard = TempDir::new("unsupported");
    let out = guard.path().join("novel");
    let url = "https://example.invalid/novel/1";

    let output = nvd(&["download", url, "--out", out.to_str().unwrap()]);

    assert_eq!(output.status.code(), Some(1));
    let text = stderr(&output);
    assert!(text.contains(url), "the error must name the URL:\n{text}");
    assert!(
        !text.contains("panicked") && !text.contains("RUST_BACKTRACE"),
        "no backtraces:\n{text}"
    );
    assert!(
        !out.exists(),
        "a refused download leaves no directory behind"
    );
}

#[test]
fn no_robots_is_announced_out_loud() {
    let guard = TempDir::new("no-robots");
    let out = guard.path().join("novel");

    let output = nvd(&[
        "download",
        "https://example.invalid/novel/1",
        "--out",
        out.to_str().unwrap(),
        "--no-robots",
    ]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("robots.txt is being ignored"),
        "overriding a site's wishes must not be silent:\n{}",
        stderr(&output)
    );
}

#[test]
fn help_lists_every_command() {
    let output = nvd(&["--help"]);

    assert_eq!(output.status.code(), Some(0));
    let text = stdout(&output);
    for command in ["download", "update", "export", "info", "session"] {
        assert!(
            text.contains(command),
            "help should mention {command}:\n{text}"
        );
    }
}

#[test]
fn update_on_a_directory_that_is_not_a_novel_is_an_error() {
    let guard = TempDir::new("update-not-a-novel");
    let dir = guard.path().to_str().unwrap();

    let output = nvd(&["update", dir]);

    assert_eq!(output.status.code(), Some(1), "a failure must not exit 0");
    // The message names the directory that was tried, not just "file not found".
    assert!(stderr(&output).contains(dir), "got:\n{}", stderr(&output));
    assert!(stdout(&output).is_empty(), "nothing was produced");
}

#[test]
fn update_on_a_novel_no_adapter_handles_names_the_url() {
    // The directory is a valid one; the site it came from is not ours to fetch.
    let (_guard, novel) = novel("update-unsupported");

    let output = nvd(&["update", novel.root().to_str().unwrap()]);

    assert_eq!(output.status.code(), Some(1));
    let text = stderr(&output);
    assert!(
        text.contains("https://fake.test/novel/"),
        "the error must name the novel's URL:\n{text}"
    );
    assert!(
        !text.contains("not implemented"),
        "update is implemented:\n{text}"
    );
}

#[test]
fn update_help_documents_its_flags_and_its_exit_code() {
    let output = nvd(&["update", "--help"]);

    assert_eq!(output.status.code(), Some(0));
    let text = stdout(&output);
    assert!(text.contains("--check"), "got:\n{text}");
    assert!(text.contains("--allow-adult"), "got:\n{text}");
    assert!(
        text.contains("Exits 3"),
        "an exit code nobody expects must be said out loud:\n{text}"
    );
}

/// The whole pipeline against the real site, and the only test that needs the
/// network. Run it with `cargo test -p wnd-cli -- --ignored`.
///
/// `n0608gd` has 89 chapters across 8 arcs, which is why it is the reference
/// novel: two of them is enough to see the meter count, the truncation work, and
/// the resume on the second run.
#[test]
#[ignore = "needs the network and a live site"]
fn downloads_a_real_novel_end_to_end() {
    const URL: &str = "https://ncode.syosetu.com/n0608gd/";

    let guard = TempDir::new("live");
    let out = guard.path().join("novel");

    let output = nvd(&[
        "download",
        URL,
        "--out",
        out.to_str().unwrap(),
        "--max-chapters",
        "2",
        "--format",
        "txt,md",
    ]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));

    // The meter is live: one line per chapter, counted against the total.
    let meter = stderr(&output);
    assert!(meter.contains("[1/2] fetch"), "got:\n{meter}");
    assert!(meter.contains("[2/2] fetch"), "got:\n{meter}");

    // The summary is on stdout, and it counts what actually happened.
    let summary = stdout(&output);
    assert!(
        summary.contains("2 fetched, 0 skipped, 2 total"),
        "got:\n{summary}"
    );
    assert!(summary.contains("exported:"), "got:\n{summary}");

    // The exports exist and contain prose, not just headings.
    let text = fs::read_to_string(exported(&out, "txt")).unwrap();
    assert!(text.lines().count() > 5, "got:\n{text}");

    let manifest = wnd_core::NovelDir::at(&out).read_manifest().unwrap();
    assert_eq!(
        manifest.chapters.len(),
        2,
        "--max-chapters truncated the index"
    );
    assert_eq!(manifest.chapters[0].index, 1);
    assert!(
        manifest.chapters[0].arc.is_some(),
        "the arc survives the export"
    );

    // Running it again must fetch nothing and change nothing.
    let again = nvd(&[
        "download",
        URL,
        "--out",
        out.to_str().unwrap(),
        "--max-chapters",
        "2",
    ]);
    assert_eq!(again.status.code(), Some(0), "stderr: {}", stderr(&again));
    assert!(
        stdout(&again).contains("0 fetched, 2 skipped, 2 total"),
        "the second run must resume:\n{}",
        stdout(&again)
    );
}

/// The same pipeline, against the other site — driven from a URL a reader would
/// actually paste: the **episode page they have open**, not the work page.
///
/// Kakuyomu numbers its episodes nowhere a URL can be derived from, so this also
/// checks that an episode URL reduces to the work it belongs to.
#[test]
#[ignore = "needs the network and a live site"]
fn downloads_a_real_kakuyomu_novel_from_an_episode_url() {
    const WORK: &str = "https://kakuyomu.jp/works/11054822663954717597";
    const EPISODE: &str =
        "https://kakuyomu.jp/works/11054822663954717597/episodes/11054822663955326556";

    let guard = TempDir::new("live-kakuyomu");
    let out = guard.path().join("novel");

    let output = nvd(&[
        "download",
        EPISODE,
        "--out",
        out.to_str().unwrap(),
        "--max-chapters",
        "2",
        "--format",
        "txt",
    ]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(
        stdout(&output).contains("2 fetched, 0 skipped, 2 total"),
        "got:\n{}",
        stdout(&output)
    );

    let manifest = wnd_core::NovelDir::at(&out).read_manifest().unwrap();
    assert_eq!(manifest.source.id, "kakuyomu");
    assert_eq!(
        manifest.source.novel_url, WORK,
        "an episode URL resolves to its work, not to itself"
    );
    assert!(manifest.author.is_some());
    assert!(
        !manifest.tags.is_empty(),
        "tags come from the page snapshot"
    );
    assert!(
        manifest
            .synopsis
            .as_deref()
            .is_some_and(|s| s.contains('\n')),
        "the synopsis keeps its paragraph breaks"
    );
    assert_eq!(
        manifest.chapters.len(),
        2,
        "--max-chapters truncated the index"
    );

    let first = &manifest.chapters[0];
    assert_eq!(first.title, "第1話　勇者、魔王の軍門に下る");
    assert_eq!(
        first.arc, None,
        "this work has no arcs, and none is invented"
    );
    assert!(
        first
            .published_at
            .as_deref()
            .is_some_and(|at| at.ends_with('Z')),
        "Kakuyomu states an instant in UTC: {:?}",
        first.published_at
    );

    let text = fs::read_to_string(exported(&out, "txt")).unwrap();
    assert!(text.lines().count() > 5, "got:\n{text}");
}

/// `nvd update` against the real site. A single-chapter short story, because the
/// point is the exit code and the wording, not the volume — and because an
/// update against a capped download legitimately fetches everything the cap left
/// behind, which would be 87 chapters here.
#[test]
#[ignore = "needs the network and a live site"]
fn updates_a_real_novel_and_then_has_nothing_to_do() {
    const URL: &str = "https://ncode.syosetu.com/n5589cg/";

    let guard = TempDir::new("live-update");
    let out = guard.path().join("novel");

    let output = nvd(&["download", URL, "--out", out.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));

    // A directory that is already current exits 3 rather than 0 — the whole
    // point of the code — and says so instead of printing an empty summary.
    let output = nvd(&["update", out.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(3), "stderr: {}", stderr(&output));
    assert!(
        stdout(&output).contains("up to date"),
        "got:\n{}",
        stdout(&output)
    );

    // `--check` asks the site about the chapter, finds it unchanged, writes
    // nothing, and still exits 3: fetching in order to compare is not updating.
    let output = nvd(&["update", out.to_str().unwrap(), "--check"]);
    assert_eq!(output.status.code(), Some(3), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("1 compared, 0 changed"), "got:\n{text}");
    assert!(
        stderr(&output).contains("check"),
        "the meter says what it is doing:\n{}",
        stderr(&output)
    );
}

#[test]
fn session_save_list_delete_round_trips() {
    let guard = TempDir::new("session-round-trip");
    let session_dir = guard.path().join("sessions");
    let cookies = guard.path().join("cookies.txt");
    std::fs::write(&cookies, ".example.com\tTRUE\t/\tTRUE\t0\tSESS\tabc123\n").unwrap();

    let save = nvd(&[
        "--session-dir",
        session_dir.to_str().unwrap(),
        "session",
        "save",
        "demo",
        cookies.to_str().unwrap(),
    ]);
    assert_eq!(save.status.code(), Some(0), "stderr: {}", stderr(&save));
    assert!(stdout(&save).contains("saved session `demo`"));

    let list = nvd(&[
        "--session-dir",
        session_dir.to_str().unwrap(),
        "session",
        "list",
    ]);
    assert_eq!(list.status.code(), Some(0), "stderr: {}", stderr(&list));
    let text = stdout(&list);
    assert!(text.contains("demo"), "got:\n{text}");
    assert!(text.contains("example.com"), "got:\n{text}");
    assert!(!text.contains("abc123"), "a listing never carries a value: got:\n{text}");

    let delete = nvd(&[
        "--session-dir",
        session_dir.to_str().unwrap(),
        "session",
        "delete",
        "demo",
    ]);
    assert_eq!(delete.status.code(), Some(0), "stderr: {}", stderr(&delete));

    let list = nvd(&[
        "--session-dir",
        session_dir.to_str().unwrap(),
        "session",
        "list",
    ]);
    assert!(stdout(&list).contains("no sessions saved"));
}

#[test]
fn session_save_rejects_an_empty_export() {
    let guard = TempDir::new("session-empty");
    let session_dir = guard.path().join("sessions");
    let cookies = guard.path().join("cookies.txt");
    std::fs::write(&cookies, "# Netscape HTTP Cookie File\n").unwrap();

    let output = nvd(&[
        "--session-dir",
        session_dir.to_str().unwrap(),
        "session",
        "save",
        "demo",
        cookies.to_str().unwrap(),
    ]);
    assert_ne!(output.status.code(), Some(0));
    assert!(stderr(&output).contains("holds no cookies"));
}

#[test]
fn download_with_a_missing_session_names_it() {
    let guard = TempDir::new("session-missing");
    let session_dir = guard.path().join("sessions");
    let output = nvd(&[
        "download",
        "https://example.com/novel/1",
        "--out",
        guard.path().join("out").to_str().unwrap(),
        "--session",
        "nope",
        "--session-dir",
        session_dir.to_str().unwrap(),
    ]);
    assert_ne!(output.status.code(), Some(0));
    assert!(stderr(&output).contains("no session named `nope`"));
}
