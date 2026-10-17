//! Reading and writing a novel directory.
//!
//! Layout (see `docs/engineering/format.md`):
//!
//! ```text
//! my-novel/
//! ├── manifest.json
//! ├── chapters/0001.json
//! ├── raw/        original fetched HTML, for re-parsing without re-fetching
//! ├── images/
//! └── exports/    generated EPUB/TXT/Markdown
//! ```

use crate::error::{Error, Result};
use crate::store::model::{Chapter, FORMAT_VERSION, Manifest};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

/// File name for a chapter index, e.g. `0001.json`. Zero-padded so directory
/// listings and diffs stay in reading order.
pub fn chapter_file_name(index: u32) -> String {
    format!("{index:04}.json")
}

/// SHA-256 over the canonical JSON of a value, prefixed with the algorithm.
///
/// Used as a chapter's change-detection hash in the manifest.
pub fn hash_json<T: serde::Serialize>(value: &T) -> Result<String> {
    let bytes = serde_json::to_vec(value)?;
    let digest = Sha256::digest(&bytes);
    Ok(format!("sha256:{}", to_hex(&digest)))
}

fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

/// What a write actually produced, so the caller can record it without reading
/// the file back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    /// `sha256:…` over the canonical JSON of what was written.
    pub hash: String,
    /// Size of the file on disk, in bytes.
    pub bytes: u64,
}

/// A novel directory on disk.
#[derive(Debug, Clone)]
pub struct NovelDir {
    root: PathBuf,
}

impl NovelDir {
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn manifest_path(&self) -> PathBuf {
        self.root.join("manifest.json")
    }

    pub fn chapters_dir(&self) -> PathBuf {
        self.root.join("chapters")
    }

    pub fn raw_dir(&self) -> PathBuf {
        self.root.join("raw")
    }

    pub fn images_dir(&self) -> PathBuf {
        self.root.join("images")
    }

    pub fn exports_dir(&self) -> PathBuf {
        self.root.join("exports")
    }

    pub fn chapter_path(&self, index: u32) -> PathBuf {
        self.chapters_dir().join(chapter_file_name(index))
    }

    /// Create the directory skeleton. Safe to call repeatedly.
    pub fn ensure_layout(&self) -> Result<()> {
        fs::create_dir_all(self.chapters_dir())?;
        fs::create_dir_all(self.images_dir())?;
        Ok(())
    }

    pub fn has_chapter(&self, index: u32) -> bool {
        self.chapter_path(index).is_file()
    }

    pub fn read_manifest(&self) -> Result<Manifest> {
        let path = self.manifest_path();
        if !path.is_file() {
            return Err(Error::ManifestNotFound(self.root.display().to_string()));
        }
        let manifest: Manifest = serde_json::from_str(&fs::read_to_string(&path)?)?;
        // A *newer* file was written by a build that knows something this one
        // does not. An older one is readable by definition: the version only
        // moves for an incompatible change, so every older shape is one this
        // build still understands — and refusing it would strand directories
        // that predate an unrelated addition to the format.
        if manifest.format_version > FORMAT_VERSION {
            return Err(Error::FormatVersion {
                found: manifest.format_version,
                expected: FORMAT_VERSION,
            });
        }
        Ok(manifest)
    }

    pub fn write_manifest(&self, manifest: &Manifest) -> Result<()> {
        fs::create_dir_all(&self.root)?;
        write_json(&self.manifest_path(), manifest).map(|_| ())
    }

    pub fn read_chapter(&self, index: u32) -> Result<Chapter> {
        let path = self.chapter_path(index);
        if !path.is_file() {
            return Err(Error::ChapterNotFound(index));
        }
        let chapter: Chapter = serde_json::from_str(&fs::read_to_string(&path)?)?;
        // Newer only: see `read_manifest`.
        if chapter.format_version > FORMAT_VERSION {
            return Err(Error::FormatVersion {
                found: chapter.format_version,
                expected: FORMAT_VERSION,
            });
        }
        Ok(chapter)
    }

    /// Write a chapter, replacing any previous copy.
    ///
    /// The returned [`Written`] carries the hash to record in the manifest and
    /// the size, so the caller never has to read the file back.
    pub fn write_chapter(&self, chapter: &Chapter) -> Result<Written> {
        fs::create_dir_all(self.chapters_dir())?;
        let bytes = write_json(&self.chapter_path(chapter.index), chapter)?;
        Ok(Written {
            hash: hash_json(chapter)?,
            bytes,
        })
    }
}

/// Write JSON to `path` atomically: a sibling temporary file, then a rename.
///
/// An interrupted write leaves either the previous file or the new one, never a
/// truncated one. `fs::rename` replaces an existing file on Windows as well as
/// Unix, so one implementation covers both.
///
/// Returns the number of bytes written.
fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<u64> {
    // Pretty-printed: these files are meant to be diffed and read by humans,
    // and they are the input to a translation stage.
    let mut json = serde_json::to_string_pretty(value)?;
    json.push('\n');

    let temporary = temporary_path(path);
    if let Err(error) = fs::write(&temporary, json.as_bytes()) {
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    Ok(json.len() as u64)
}

/// `<path>.tmp`, a sibling of the target so the rename stays on one filesystem.
fn temporary_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".tmp");
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::model::{Block, ContentRating, Inline, SourceInfo};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_novel(tag: &str) -> NovelDir {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        NovelDir::at(
            std::env::temp_dir().join(format!("wnd-test-{tag}-{}-{nanos}", std::process::id())),
        )
    }

    #[test]
    fn chapter_file_names_are_zero_padded_and_ordered() {
        assert_eq!(chapter_file_name(1), "0001.json");
        assert_eq!(chapter_file_name(42), "0042.json");
        assert_eq!(chapter_file_name(12345), "12345.json");
    }

    #[test]
    fn manifest_round_trips_and_rejects_other_versions() {
        let dir = temp_novel("manifest");
        let mut manifest = Manifest::new(
            SourceInfo {
                id: "narou".into(),
                novel_url: "https://ncode.syosetu.com/n1234ab/".into(),
                site_title: None,
            },
            "テスト小説",
            "ja",
        );
        manifest.author = Some("作者".into());
        manifest.content_rating = ContentRating::Adult;
        manifest.tags.push("ファンタジー".into());

        dir.ensure_layout().unwrap();
        dir.write_manifest(&manifest).unwrap();

        assert_eq!(dir.read_manifest().unwrap(), manifest);

        // A file from a future format version must not be silently accepted.
        let mut future = manifest.clone();
        future.format_version = FORMAT_VERSION + 1;
        dir.write_manifest(&future).unwrap();
        assert!(matches!(
            dir.read_manifest(),
            Err(Error::FormatVersion { .. })
        ));

        fs::remove_dir_all(dir.root()).ok();
    }

    #[test]
    fn an_older_version_is_read_and_a_newer_one_is_refused() {
        // The version is only ever bumped for an incompatible change, so an
        // older file is readable by definition — refusing it would strand every
        // directory already on disk, which is the opposite of what a reader is
        // for. (`FORMAT_VERSION - 1` is not a real historical version while the
        // format is still at its first; the test is written this way so that it
        // becomes a real pair the moment the number moves.)
        let dir = temp_novel("version");
        dir.ensure_layout().unwrap();

        let mut manifest = Manifest::new(
            SourceInfo {
                id: "narou".into(),
                novel_url: "https://ncode.syosetu.com/n1234ab/".into(),
                site_title: None,
            },
            "older",
            "ja",
        );
        manifest.format_version = FORMAT_VERSION - 1;
        dir.write_manifest(&manifest).unwrap();
        assert_eq!(dir.read_manifest().unwrap().title, "older");

        let mut chapter = Chapter::new(1, "第一章");
        chapter.format_version = FORMAT_VERSION - 1;
        dir.write_chapter(&chapter).unwrap();
        assert_eq!(dir.read_chapter(1).unwrap().title, "第一章");

        // Newer is another matter: that file was written by a build that knows
        // something this one does not.
        let mut newer = manifest.clone();
        newer.format_version = FORMAT_VERSION + 1;
        dir.write_manifest(&newer).unwrap();
        let error = dir.read_manifest().unwrap_err();
        assert!(matches!(error, Error::FormatVersion { .. }));
        assert!(
            error.to_string().contains("newer"),
            "the message must say which way the mismatch goes: {error}"
        );

        let mut newer_chapter = chapter.clone();
        newer_chapter.format_version = FORMAT_VERSION + 1;
        dir.write_chapter(&newer_chapter).unwrap();
        assert!(matches!(
            dir.read_chapter(1),
            Err(Error::FormatVersion { .. })
        ));

        fs::remove_dir_all(dir.root()).ok();
    }

    #[test]
    fn write_chapter_reports_the_hash_and_the_size() {
        let dir = temp_novel("hash");
        dir.ensure_layout().unwrap();

        let mut chapter = Chapter::new(1, "第一章");
        chapter.nodes.push(Block::Paragraph {
            children: vec![Inline::Text {
                value: "こんにちは".into(),
            }],
        });

        let first = dir.write_chapter(&chapter).unwrap();
        assert!(first.hash.starts_with("sha256:"));
        assert_eq!(
            first.bytes,
            fs::metadata(dir.chapter_path(1)).unwrap().len(),
            "the reported size must be the size of the file on disk"
        );
        assert!(dir.has_chapter(1));

        // Same content -> same hash, so a re-fetch can be skipped.
        let second = dir.write_chapter(&chapter).unwrap();
        assert_eq!(first.hash, second.hash);

        // Changed content -> changed hash, so update tracking notices.
        chapter.nodes.push(Block::SceneBreak);
        let third = dir.write_chapter(&chapter).unwrap();
        assert_ne!(first.hash, third.hash);

        assert_eq!(dir.read_chapter(1).unwrap(), chapter);

        fs::remove_dir_all(dir.root()).ok();
    }

    #[test]
    fn writes_replace_in_place_and_leave_no_temporary_files() {
        let dir = temp_novel("atomic");
        dir.ensure_layout().unwrap();

        let mut chapter = Chapter::new(1, "first");
        dir.write_chapter(&chapter).unwrap();
        chapter.title = "second".into();
        dir.write_chapter(&chapter).unwrap();
        assert_eq!(dir.read_chapter(1).unwrap().title, "second");

        let mut manifest = Manifest::new(
            SourceInfo {
                id: "fake".into(),
                novel_url: "https://example.test/1/".into(),
                site_title: None,
            },
            "テスト",
            "ja",
        );
        dir.write_manifest(&manifest).unwrap();
        manifest.title = "二度目".into();
        dir.write_manifest(&manifest).unwrap();
        assert_eq!(dir.read_manifest().unwrap().title, "二度目");

        for checked in [dir.chapters_dir(), dir.root().to_path_buf()] {
            let names: Vec<String> = fs::read_dir(&checked)
                .unwrap()
                .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            assert!(
                names.iter().all(|name| !name.ends_with(".tmp")),
                "no temporary file may survive a completed write: {names:?}"
            );
        }

        fs::remove_dir_all(dir.root()).ok();
    }

    #[test]
    fn missing_files_produce_clear_errors() {
        let dir = temp_novel("missing");
        assert!(matches!(
            dir.read_manifest(),
            Err(Error::ManifestNotFound(_))
        ));
        assert!(matches!(
            dir.read_chapter(7),
            Err(Error::ChapterNotFound(7))
        ));
    }
}
