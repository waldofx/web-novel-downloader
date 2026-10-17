#![allow(dead_code)] // every test binary compiles this module; not all use everything

//! Test helpers shared by this crate's integration tests.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

use wnd_core::store::storage::chapter_file_name;
use wnd_core::{Block, Chapter, ChapterMeta, Inline, Manifest, NovelDir, SourceInfo};

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
