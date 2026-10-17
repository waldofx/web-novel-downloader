#![allow(dead_code)] // every test binary compiles this module; not all use everything

//! Test helpers shared by this crate's integration tests.
//!
//! `tests/` subfolders are modules, not test targets, so this file is never
//! compiled as a test binary of its own.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use zip::ZipArchive;

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
            std::env::temp_dir().join(format!("wnd-export-{tag}-{}-{nanos}", std::process::id()));
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

/// Every entry name in an archive, in the order the archive lists them.
pub fn entry_names(path: &Path) -> Vec<String> {
    let mut archive = ZipArchive::new(File::open(path).expect("open the archive")).expect("zip");
    (0..archive.len())
        .map(|index| archive.by_index(index).expect("entry").name().to_string())
        .collect()
}

/// One entry's contents, as text.
pub fn entry(path: &Path, name: &str) -> String {
    let mut archive = ZipArchive::new(File::open(path).expect("open the archive")).expect("zip");
    let mut file = archive
        .by_name(name)
        .unwrap_or_else(|_| panic!("no {name} in the archive"));
    let mut text = String::new();
    file.read_to_string(&mut text).expect("read the entry");
    text
}

/// One entry's raw bytes.
pub fn entry_bytes(path: &Path, name: &str) -> Vec<u8> {
    let mut archive = ZipArchive::new(File::open(path).expect("open the archive")).expect("zip");
    let mut file = archive
        .by_name(name)
        .unwrap_or_else(|_| panic!("no {name} in the archive"));
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).expect("read the entry");
    bytes
}
