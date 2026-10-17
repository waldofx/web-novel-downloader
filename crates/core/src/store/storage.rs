//! Reading and writing a novel directory.
//!
//! Layout (see `DESIGN.md`):
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
        if manifest.format_version != FORMAT_VERSION {
            return Err(Error::FormatVersion {
                found: manifest.format_version,
                expected: FORMAT_VERSION,
            });
        }
        Ok(manifest)
    }

    pub fn write_manifest(&self, manifest: &Manifest) -> Result<()> {
        fs::create_dir_all(&self.root)?;
        write_json(&self.manifest_path(), manifest)
    }

    pub fn read_chapter(&self, index: u32) -> Result<Chapter> {
        let path = self.chapter_path(index);
        if !path.is_file() {
            return Err(Error::ChapterNotFound(index));
        }
        let chapter: Chapter = serde_json::from_str(&fs::read_to_string(&path)?)?;
        if chapter.format_version != FORMAT_VERSION {
            return Err(Error::FormatVersion {
                found: chapter.format_version,
                expected: FORMAT_VERSION,
            });
        }
        Ok(chapter)
    }

    /// Write a chapter and return the hash to store in the manifest.
    pub fn write_chapter(&self, chapter: &Chapter) -> Result<String> {
        fs::create_dir_all(self.chapters_dir())?;
        write_json(&self.chapter_path(chapter.index), chapter)?;
        hash_json(chapter)
    }
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
    // Pretty-printed: these files are meant to be diffed and read by humans,
    // and they are the input to a translation stage.
    let mut json = serde_json::to_string_pretty(value)?;
    json.push('\n');
    fs::write(path, json)?;
    Ok(())
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
    fn chapter_hash_is_stable_and_content_sensitive() {
        let dir = temp_novel("hash");
        dir.ensure_layout().unwrap();

        let mut chapter = Chapter::new(1, "第一章");
        chapter.nodes.push(Block::Paragraph {
            children: vec![Inline::Text {
                value: "こんにちは".into(),
            }],
        });

        let first = dir.write_chapter(&chapter).unwrap();
        assert!(first.starts_with("sha256:"));
        assert!(dir.has_chapter(1));

        // Same content -> same hash, so a re-fetch can be skipped.
        let second = dir.write_chapter(&chapter).unwrap();
        assert_eq!(first, second);

        // Changed content -> changed hash, so update tracking notices.
        chapter.nodes.push(Block::SceneBreak);
        let third = dir.write_chapter(&chapter).unwrap();
        assert_ne!(first, third);

        assert_eq!(dir.read_chapter(1).unwrap(), chapter);

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
