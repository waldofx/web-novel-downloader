#![allow(dead_code)] // every test binary compiles this module; not all use everything

//! Test helpers shared by this crate's integration tests.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

use url::Url;
use wnd_core::store::storage::chapter_file_name;
use wnd_core::{
    Block, Chapter, ChapterMeta, FetchPolicy, HttpClient, Inline, Manifest, NovelDir, SourceInfo,
};

/// A client at the default fetch policy, for the live tests.
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

/// A directory that deletes itself when dropped, so a failing assertion cannot
/// leave debris in the system temporary directory.
#[derive(Debug)]
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub fn new(tag: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("wnd-cli-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&path).expect("create the temporary directory");
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// The built binary, run with `args`. Cargo guarantees the path.
pub fn nvd(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_nvd"))
        .args(args)
        .output()
        .expect("run nvd")
}

pub fn stdout(output: &Output) -> String {
    // The child writes UTF-8 to a pipe, whatever the console encoding is.
    String::from_utf8_lossy(&output.stdout).to_string()
}

pub fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

/// The exported file with the given extension, wherever it landed.
///
/// The name comes from the novel's title, so the test looks for the extension
/// rather than repeating the naming rule.
pub fn exported(dir: &Path, extension: &str) -> PathBuf {
    let exports = NovelDir::at(dir).exports_dir();
    let mut found: Vec<PathBuf> = std::fs::read_dir(&exports)
        .unwrap_or_else(|error| panic!("read {}: {error}", exports.display()))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|found| found == extension))
        .collect();
    found.sort();
    assert!(
        !found.is_empty(),
        "no .{extension} in {}",
        exports.display()
    );
    found.remove(0)
}

/// A two-chapter novel on disk, written through the real storage layer.
pub fn novel(tag: &str) -> (TempDir, NovelDir) {
    const URL: &str = "https://fake.test/novel/";

    let guard = TempDir::new(tag);
    let novel = NovelDir::at(guard.path());
    novel.ensure_layout().unwrap();

    let mut manifest = Manifest::new(
        SourceInfo {
            id: "fake".to_string(),
            novel_url: URL.to_string(),
            site_title: None,
        },
        "偽の小説",
        "ja",
    );
    manifest.author = Some("著者".to_string());
    manifest.chapters = (1..=2)
        .map(|index| ChapterMeta {
            index,
            title: format!("第{index}話"),
            url: format!("{URL}{index}/"),
            arc: None,
            published_at: None,
            revised_at: None,
            hash: None,
            file: format!("chapters/{}", chapter_file_name(index)),
        })
        .collect();
    novel.write_manifest(&manifest).unwrap();

    for index in 1..=2 {
        let mut chapter = Chapter::new(index, format!("第{index}話"));
        chapter.nodes.push(Block::Paragraph {
            children: vec![Inline::Text {
                value: format!("　{index}話目の本文。"),
            }],
        });
        novel.write_chapter(&chapter).unwrap();
    }

    (guard, novel)
}

/// Give chapter `index` one stored illustration, the way a fetch would.
///
/// The three facts have to agree — a file in `images/`, an entry in the manifest
/// and a block pointing at it — so this writes all three rather than leaving a
/// caller to remember them.
pub fn add_illustration(novel: &NovelDir, index: u32) {
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\nstand-in for a picture";

    let file = format!("images/{index:04}-1.png");
    novel.write_image(&file, PNG).unwrap();

    let mut manifest = novel.read_manifest().unwrap();
    manifest.images.push(wnd_core::StoredImage {
        url: format!("https://fake.test/illust/{index}.png"),
        file: file.clone(),
        hash: wnd_core::store::storage::hash_bytes(PNG),
    });
    novel.write_manifest(&manifest).unwrap();

    let mut chapter = novel.read_chapter(index).unwrap();
    chapter.nodes.push(Block::Image {
        src: file,
        alt: None,
    });
    novel.write_chapter(&chapter).unwrap();
}
