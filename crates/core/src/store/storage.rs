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
    Ok(hash_bytes(&bytes))
}

/// SHA-256 over raw bytes, prefixed with the algorithm.
///
/// The same hash as [`hash_json`] over the same bytes; used for the stored
/// images, whose content is not JSON.
pub fn hash_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!("sha256:{}", to_hex(&digest))
}

/// File name for a chapter's image, e.g. `images/0001-2.png`.
///
/// `position` counts the chapter's *distinct* images from one, not blocks: an
/// image keeps its name when the prose around it is edited, so a chapter that
/// gained a paragraph does not look like a chapter whose illustration moved.
/// Zero-padded so a directory listing stays in reading order.
pub fn image_file_name(chapter: u32, position: usize, extension: &str) -> String {
    format!("images/{chapter:04}-{position}.{extension}")
}

/// File name for the work's cover, e.g. `images/cover.png`.
pub fn cover_file_name(extension: &str) -> String {
    format!("images/cover.{extension}")
}

/// The extension an image is stored under.
///
/// The server's media type comes first, because the extension is what every
/// reader afterwards uses to decide whether it can open the file — the EPUB's
/// own manifest included, where a wrong one is a broken illustration. The URL's
/// extension is the fallback for a host that sends no `Content-Type`, and `bin`
/// is the last resort: nothing renders it, but nothing is lost either.
///
/// The result is a bare token, because a media type is a stranger's string and
/// this value becomes part of a file name.
pub fn image_extension(content_type: Option<&str>, url: &str) -> String {
    if let Some(subtype) = content_type.and_then(media_subtype) {
        if let Some(known) = known_extension(subtype) {
            return known.to_string();
        }
        // An unrecognized `image/…`: the token is still a better answer than
        // nothing, and harmless once stripped to `[a-z0-9]`.
        if let Some(token) = sanitized_extension(subtype) {
            return token;
        }
    }
    url_extension(url)
        .and_then(sanitized_extension)
        .unwrap_or_else(|| "bin".to_string())
}

/// Whether a response's media type says the body is a page rather than a picture.
///
/// A server that answers an image URL with `text/html` has served a *page* — a
/// filter, a captive portal, a soft 404 — and storing that under `images/` would
/// put it in someone's EPUB. `None` is not that: a host that omits the header is
/// common, and the URL's own extension is then the best evidence there is.
///
/// Deliberately only the unambiguous cases. `application/octet-stream` is
/// accepted, because hosts that send it for a perfectly good image are common
/// enough that refusing would lose pictures to a technicality.
pub fn is_page_media_type(content_type: Option<&str>) -> bool {
    let Some(media_type) = content_type.and_then(|value| value.split(';').next()) else {
        return false;
    };
    let media_type = media_type.trim().to_ascii_lowercase();
    media_type.starts_with("text/") || media_type == "application/xhtml+xml"
}

/// The subtype of an `image/…` media type, e.g. `png` from `image/png; q=1`.
fn media_subtype(content_type: &str) -> Option<&str> {
    let media_type = content_type.split(';').next()?.trim();
    let (kind, subtype) = media_type.split_once('/')?;
    if kind.eq_ignore_ascii_case("image") {
        Some(subtype.trim())
    } else {
        None
    }
}

/// The extension we store a media subtype under, for the ones readers know.
fn known_extension(subtype: &str) -> Option<&'static str> {
    match subtype.trim().to_ascii_lowercase().as_str() {
        "jpeg" | "jpg" => Some("jpg"),
        "png" => Some("png"),
        "gif" => Some("gif"),
        "webp" => Some("webp"),
        "avif" => Some("avif"),
        "svg+xml" => Some("svg"),
        "bmp" => Some("bmp"),
        _ => None,
    }
}

/// The extension in a URL's last path segment, query string ignored.
fn url_extension(url: &str) -> Option<&str> {
    // Not `url::Url`: this runs on strings a server chose, and the only part
    // that matters is the segment before `?` or `#`.
    let path = url.split(['?', '#']).next()?;
    let segment = path.rsplit('/').next()?;
    let (stem, extension) = segment.rsplit_once('.')?;
    (!stem.is_empty() && !extension.is_empty()).then_some(extension)
}

/// `value` stripped to something safe to put in a file name, or `None`.
fn sanitized_extension(value: &str) -> Option<String> {
    let token: String = value
        .chars()
        .take(12)
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    (!token.is_empty()).then_some(token)
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

    /// Where a stored image lives. `file` is the manifest's path, relative to
    /// the novel directory (`images/0001-1.png`), so a directory that is moved
    /// or copied keeps working.
    pub fn image_path(&self, file: &str) -> PathBuf {
        self.root.join(file)
    }

    /// Write image bytes, replacing any previous copy of the same name.
    ///
    /// Atomic, like every other write here: an interrupted fetch leaves the
    /// previous picture rather than half of a new one.
    pub fn write_image(&self, file: &str, bytes: &[u8]) -> Result<Written> {
        let path = self.image_path(file);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        write_bytes(&path, bytes)?;
        Ok(Written {
            hash: hash_bytes(bytes),
            bytes: bytes.len() as u64,
        })
    }

    /// Read a stored image back, for the resume check and for `nvd info`.
    pub fn read_image(&self, file: &str) -> Result<Vec<u8>> {
        Ok(fs::read(self.image_path(file))?)
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
    write_json_with(path, value, None)
}

/// The same, for a file that holds a credential.
///
/// The permissions are set on the temporary file **before** it is renamed into
/// place, so the secret is never briefly readable by anyone else.
pub(crate) fn write_json_private<T: serde::Serialize>(path: &Path, value: &T) -> Result<u64> {
    write_json_with(path, value, Some(PRIVATE_FILE_MODE))
}

/// What a file only its owner may read is set to, where permissions exist.
#[cfg(unix)]
pub(crate) const PRIVATE_FILE_MODE: u32 = 0o600;
#[cfg(not(unix))]
pub(crate) const PRIVATE_FILE_MODE: u32 = 0;

fn write_json_with<T: serde::Serialize>(path: &Path, value: &T, mode: Option<u32>) -> Result<u64> {
    // Pretty-printed: these files are meant to be diffed and read by humans,
    // and they are the input to a translation stage.
    let mut json = serde_json::to_string_pretty(value)?;
    json.push('\n');

    let temporary = temporary_path(path);
    if let Err(error) = fs::write(&temporary, json.as_bytes()) {
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    if let Err(error) = set_permissions(&temporary, mode) {
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    Ok(json.len() as u64)
}

/// Write bytes to `path` atomically, the way [`write_json`] writes JSON.
fn write_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    let temporary = temporary_path(path);
    if let Err(error) = fs::write(&temporary, bytes) {
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    Ok(())
}

/// `0600` on `path`, where the platform has such a thing.
#[cfg(unix)]
fn set_permissions(path: &Path, mode: Option<u32>) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    match mode {
        Some(mode) => fs::set_permissions(path, fs::Permissions::from_mode(mode)),
        None => Ok(()),
    }
}

/// Windows has no read permission to set; the file is already the user's.
#[cfg(not(unix))]
fn set_permissions(_path: &Path, _mode: Option<u32>) -> std::io::Result<()> {
    Ok(())
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
                novel_url: "https://ncode.syosetu.com/n0000aa/".into(),
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
                novel_url: "https://ncode.syosetu.com/n0000aa/".into(),
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

    #[test]
    fn image_names_say_what_the_file_is() {
        // The media type wins: it is what the EPUB's own manifest will declare,
        // and a reader that is told `image/png` opens a png.
        assert_eq!(image_extension(Some("image/png"), "https://x/y.jpg"), "png");
        assert_eq!(image_extension(Some("image/jpeg"), "https://x/y"), "jpg");
        assert_eq!(
            image_extension(Some("image/webp; q=1"), "https://x/y"),
            "webp"
        );

        // A host that says nothing, or says something that carries no type: the
        // URL is the next best evidence.
        assert_eq!(image_extension(None, "https://x/a/cover.PNG"), "png");
        assert_eq!(
            image_extension(Some("application/octet-stream"), "https://x/a/1.jpeg"),
            "jpeg"
        );
        // A query string is not an extension.
        assert_eq!(
            image_extension(None, "https://x/cover.jpg?token=abc"),
            "jpg"
        );
        assert_eq!(image_extension(None, "https://x/cover"), "bin");

        // A media type is a stranger's string and it becomes part of a file
        // name, so nothing of it survives but letters and digits — and not many
        // of those.
        assert_eq!(
            image_extension(Some("image/../../etc/passwd"), "https://x/y"),
            "etcpa"
        );
    }

    #[test]
    fn a_page_is_not_an_image() {
        // A filter that answers an image URL has served a page, and a page must
        // not end up inside someone's EPUB.
        assert!(is_page_media_type(Some("text/html; charset=utf-8")));
        assert!(is_page_media_type(Some("TEXT/HTML")));
        assert!(is_page_media_type(Some("application/xhtml+xml")));

        assert!(!is_page_media_type(None), "silence is not an accusation");
        assert!(!is_page_media_type(Some("image/png")));
        // Hosts serve perfectly good images under this, so it is not a page.
        assert!(!is_page_media_type(Some("application/octet-stream")));
    }

    #[test]
    fn image_names_are_zero_padded_and_ordered() {
        assert_eq!(image_file_name(1, 1, "png"), "images/0001-1.png");
        assert_eq!(image_file_name(42, 12, "jpg"), "images/0042-12.jpg");
        assert_eq!(cover_file_name("webp"), "images/cover.webp");
    }

    #[test]
    fn write_image_reports_the_hash_and_leaves_no_temporary_file() {
        let dir = temp_novel("image");
        dir.ensure_layout().unwrap();

        let file = image_file_name(1, 1, "png");
        let bytes = b"\x89PNG\r\n\x1a\nnot really a png";
        let written = dir.write_image(&file, bytes).unwrap();
        assert_eq!(written.bytes, bytes.len() as u64);
        assert_eq!(written.hash, hash_bytes(bytes));
        assert_eq!(dir.read_image(&file).unwrap(), bytes);

        // Same bytes, same hash: a resume check compares the file with the
        // recorded hash rather than trusting the name.
        assert_eq!(dir.write_image(&file, bytes).unwrap().hash, written.hash);
        assert_eq!(dir.read_image(&file).unwrap(), bytes);

        let names: Vec<String> = fs::read_dir(dir.images_dir())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["0001-1.png".to_string()]);

        assert!(dir.read_image("images/absent.png").is_err());
        fs::remove_dir_all(dir.root()).ok();
    }
}
