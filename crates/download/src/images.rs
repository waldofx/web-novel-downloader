//! Storing the pictures a chapter and a work point at.
//!
//! An adapter can *find* an image — it is a URL in a block, or the cover URL on
//! the manifest — but only a fetcher can put one on disk, and only once it is on
//! disk can a chapter say `images/0001-1.png` instead of a URL. This module is
//! that second half.
//!
//! Nothing here is fatal. An illustration that cannot be fetched — a `404`, a
//! filter, a dead host — is reported and the chapter keeps the URL the source
//! stated, because losing a picture is not worth losing the text around it, and
//! because that URL is the only record of what the site offered.
//!
//! # Four ways an image is dealt with
//!
//! - [`store_chapter_images`] runs for every chapter that is fetched at all: it
//!   names, fetches, stores and rewrites.
//! - [`store_synopsis_images`] does the same for the pictures a work page puts
//!   among the synopsis' lines. They belong to the work, not to a chapter, and
//!   the synopsis is the one block list stored on the manifest itself.
//! - [`store_cover`] does the same for the work's cover, which belongs to no
//!   chapter and is the one image an otherwise up-to-date directory can lack.
//! - [`repair`] re-fetches a recorded image that is no longer what the manifest
//!   says. A chapter that is skipped is never looked into, so without this a
//!   picture deleted from `images/` by hand would stay broken forever.

use std::collections::HashMap;

use wnd_core::store::storage::{
    cover_file_name, hash_bytes, image_extension, image_file_name, is_page_media_type,
    synopsis_file_name,
};
use wnd_core::{Block, Chapter, HttpClient, Manifest, NovelDir, StoredImage};

use crate::Progress;

/// What storing images produced, for the caller's summary.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Stored {
    /// Images fetched and written now. A copy that was already on disk and
    /// still correct is not counted: nothing was fetched for it.
    pub(crate) fetched: usize,
    /// Images nothing could be done about, each one already reported.
    pub(crate) missing: usize,
    /// Bytes of image content written.
    pub(crate) bytes: u64,
}

impl Stored {
    pub(crate) fn merge(&mut self, other: Self) {
        self.fetched += other.fetched;
        self.missing += other.missing;
        self.bytes += other.bytes;
    }
}

/// Fetch and store a chapter's images, then point the chapter at them.
///
/// Every distinct image URL the chapter mentions, in the order it mentions them,
/// is stored as `images/<chapter>-<n>.<ext>`; the block is then rewritten to that
/// path. An image that cannot be fetched keeps its URL.
pub(crate) fn store_chapter_images(
    dir: &NovelDir,
    fetch: &HttpClient,
    referer: Option<&str>,
    manifest: &mut Manifest,
    chapter: &mut Chapter,
    tally: &mut Stored,
    on_progress: &mut impl FnMut(Progress),
) {
    let index = chapter.index;
    store_blocks(
        dir,
        fetch,
        referer,
        manifest,
        Pictures::Chapter {
            index,
            lists: [
                &mut chapter.nodes,
                &mut chapter.pre_note,
                &mut chapter.post_note,
            ],
        },
        tally,
        on_progress,
    );
}

/// Fetch and store the pictures the work's synopsis carries, then point the
/// synopsis at them.
///
/// A work page can put the author's illustrations among the synopsis' lines, so
/// they are stored as `images/synopsis-<n>.<ext>` and recorded like any other —
/// which is what puts them inside `repair`'s reach and keeps an update from
/// fetching them twice. The synopsis itself lives on the manifest, so this is
/// the one pass that rewrites the manifest rather than a chapter; the caller
/// writes it back.
pub(crate) fn store_synopsis_images(
    dir: &NovelDir,
    fetch: &HttpClient,
    referer: Option<&str>,
    manifest: &mut Manifest,
    tally: &mut Stored,
    on_progress: &mut impl FnMut(Progress),
) {
    // Taken out for the length of the pass so the manifest can record what each
    // picture becomes; it goes back whether or not every fetch worked, and one
    // that failed keeps its URL — as a chapter's does.
    let mut blocks = std::mem::take(&mut manifest.synopsis);
    store_blocks(
        dir,
        fetch,
        referer,
        manifest,
        Pictures::Synopsis {
            lists: [&mut blocks],
        },
        tally,
        on_progress,
    );
    manifest.synopsis = blocks;
}

/// Which pictures are being stored, and what the file names have to say about
/// them.
enum Pictures<'a> {
    /// A chapter: its body and its two notes, all named for the chapter.
    Chapter {
        index: u32,
        lists: [&'a mut Vec<Block>; 3],
    },
    /// The work's synopsis: one list, on the manifest.
    Synopsis { lists: [&'a mut Vec<Block>; 1] },
}

impl Pictures<'_> {
    /// The blocks these pictures are in, in reading order.
    fn each(&self) -> Vec<&Block> {
        match self {
            Pictures::Chapter { lists, .. } => lists.iter().flat_map(|list| list.iter()).collect(),
            Pictures::Synopsis { lists } => lists.iter().flat_map(|list| list.iter()).collect(),
        }
    }

    /// The same blocks, to be rewritten.
    fn each_mut(&mut self) -> Vec<&mut Block> {
        match self {
            Pictures::Chapter { lists, .. } => {
                lists.iter_mut().flat_map(|list| list.iter_mut()).collect()
            }
            Pictures::Synopsis { lists } => {
                lists.iter_mut().flat_map(|list| list.iter_mut()).collect()
            }
        }
    }

    /// What the picture at `position` (counted from one) is called.
    fn naming(&self, position: usize) -> Naming {
        match self {
            Pictures::Chapter { index, .. } => Naming::Chapter {
                index: *index,
                position,
            },
            Pictures::Synopsis { .. } => Naming::Synopsis { position },
        }
    }
}

/// Store every distinct picture in `pictures`, then rewrite the blocks to the paths.
///
/// A chapter's pictures are spread over three lists (`nodes` and the two notes),
/// and a synopsis' are in one; the numbering counts the *distinct* pictures
/// across all of them, in reading order, so a picture repeated in an afterword
/// is one file.
fn store_blocks(
    dir: &NovelDir,
    fetch: &HttpClient,
    referer: Option<&str>,
    manifest: &mut Manifest,
    mut pictures: Pictures<'_>,
    tally: &mut Stored,
    on_progress: &mut impl FnMut(Progress),
) {
    let mut stored: HashMap<String, String> = HashMap::new();

    for (position, url) in image_urls(pictures.each()).into_iter().enumerate() {
        match store_one(
            dir,
            fetch,
            referer,
            manifest,
            &url,
            pictures.naming(position + 1),
        ) {
            Outcome::Ready { file, bytes } => {
                if bytes > 0 {
                    tally.fetched += 1;
                    tally.bytes += bytes;
                    on_progress(Progress::ImageFetched {
                        file: file.clone(),
                        bytes,
                    });
                }
                stored.insert(url, file);
            }
            Outcome::Failed { reason } => {
                tally.missing += 1;
                on_progress(Progress::ImageMissing { url, reason });
            }
        }
    }

    rewrite(pictures.each_mut(), &stored);
}

/// Fetch and store the work's cover, and record where it landed.
///
/// `cover_url` is what an adapter states, and `cover` is a path inside the novel
/// directory, so this is the only thing that can set it. Nothing happens when the
/// source names no cover: an update carries the stored path over by itself.
pub(crate) fn store_cover(
    dir: &NovelDir,
    fetch: &HttpClient,
    referer: Option<&str>,
    manifest: &mut Manifest,
    tally: &mut Stored,
    on_progress: &mut impl FnMut(Progress),
) {
    let Some(url) = manifest.cover_url.clone() else {
        return;
    };

    match store_one(dir, fetch, referer, manifest, &url, Naming::Cover) {
        Outcome::Ready { file, bytes } => {
            if bytes > 0 {
                tally.fetched += 1;
                tally.bytes += bytes;
                on_progress(Progress::ImageFetched {
                    file: file.clone(),
                    bytes,
                });
            }
            // A cover that could not be fetched leaves the carried-over path
            // alone; one that was found settles it, whether it was fetched now
            // or was already there.
            manifest.cover = Some(file);
        }
        Outcome::Failed { reason } => {
            tally.missing += 1;
            on_progress(Progress::ImageMissing { url, reason });
        }
    }
}

/// Re-fetch every recorded image that is no longer what the manifest says.
///
/// A picture deleted or corrupted by hand is invisible to the plan: the chapter
/// it belongs to is skipped by construction, so nothing ever asks for it again.
/// This is the pass that notices, and it is why the rule for an image is the
/// rule for a chapter — present means *hashes to what was recorded*, not "a file
/// of that name exists".
///
/// The cost is one read and one hash per recorded image per run, and no request
/// at all when nothing is wrong. The name is left as it was, because the chapters
/// that point at the file are not being rewritten.
pub(crate) fn repair(
    dir: &NovelDir,
    fetch: &HttpClient,
    referer: Option<&str>,
    manifest: &mut Manifest,
    tally: &mut Stored,
    on_progress: &mut impl FnMut(Progress),
) {
    // Examined in the order they were recorded, so the report reads in the order
    // the pictures appear rather than in whatever order the disk returns.
    let recorded: Vec<StoredImage> = manifest.images.clone();

    for image in recorded {
        if intact(dir, &image.file, &image.hash) {
            continue;
        }

        match fetch_image(fetch, referer, &image.url) {
            Ok((bytes, _extension)) => match dir.write_image(&image.file, &bytes) {
                Ok(written) => {
                    tally.fetched += 1;
                    tally.bytes += written.bytes;
                    if let Some(entry) = manifest
                        .images
                        .iter_mut()
                        .find(|entry| entry.url == image.url)
                    {
                        entry.hash = written.hash;
                    }
                    on_progress(Progress::ImageFetched {
                        file: image.file,
                        bytes: written.bytes,
                    });
                }
                Err(error) => {
                    tally.missing += 1;
                    on_progress(Progress::ImageMissing {
                        url: image.url,
                        reason: error.to_string(),
                    });
                }
            },
            Err(reason) => {
                tally.missing += 1;
                on_progress(Progress::ImageMissing {
                    url: image.url,
                    reason,
                });
            }
        }
    }
}

/// What an image is, which is what decides the name it is stored under.
#[derive(Debug, Clone, Copy)]
enum Naming {
    /// A chapter's `position`th distinct image.
    Chapter { index: u32, position: usize },
    /// A synopsis' `position`th distinct image.
    Synopsis { position: usize },
    /// The work's cover.
    Cover,
}

/// What storing one image produced.
enum Outcome {
    /// A file the caller can point at, and the bytes written for it — zero when
    /// the copy already on disk was still exactly what the manifest recorded.
    Ready { file: String, bytes: u64 },
    /// Nothing was stored, and this is why.
    Failed { reason: String },
}

/// Store one image, or decide that the copy on disk will do.
fn store_one(
    dir: &NovelDir,
    fetch: &HttpClient,
    referer: Option<&str>,
    manifest: &mut Manifest,
    url: &str,
    naming: Naming,
) -> Outcome {
    // The URL is the identity: the same one means the same picture, so there is
    // nothing to fetch — provided the file is still what the manifest says. A
    // truncated or hand-edited one is not, exactly as for a chapter.
    if let Some(known) = manifest.image(url) {
        if intact(dir, &known.file, &known.hash) {
            return Outcome::Ready {
                file: known.file.clone(),
                bytes: 0,
            };
        }
    }

    let (bytes, extension) = match fetch_image(fetch, referer, url) {
        Ok(fetched) => fetched,
        Err(reason) => return Outcome::Failed { reason },
    };

    let file = match naming {
        Naming::Chapter { index, position } => image_file_name(index, position, &extension),
        Naming::Synopsis { position } => synopsis_file_name(position, &extension),
        Naming::Cover => cover_file_name(&extension),
    };

    let written = match dir.write_image(&file, &bytes) {
        Ok(written) => written,
        Err(error) => {
            return Outcome::Failed {
                reason: error.to_string(),
            };
        }
    };

    // One entry per URL and one per file: a chapter that changed can put a
    // different picture where the last one was, and a picture that changed type
    // is recorded under the name it now has rather than twice.
    manifest
        .images
        .retain(|entry| entry.url != url && entry.file != file);
    manifest.images.push(StoredImage {
        url: url.to_string(),
        file: file.clone(),
        hash: written.hash,
    });

    Outcome::Ready {
        file,
        bytes: written.bytes,
    }
}

/// Fetch an image, and decide what its bytes are.
///
/// Returns the bytes and the extension to store them under. Only a fetcher can
/// make that decision — it is the server's media type that says what the picture
/// is, and a URL that lies about it is common.
fn fetch_image(
    fetch: &HttpClient,
    referer: Option<&str>,
    url: &str,
) -> Result<(Vec<u8>, String), String> {
    let asset = fetch
        .get_asset(url, referer)
        .map_err(|error| error.to_string())?;

    // An image URL that answers with a page is a filter, a captive portal, or a
    // soft `404`: not a picture, and not something to put in someone's EPUB.
    if is_page_media_type(asset.content_type.as_deref()) {
        return Err(format!(
            "the server sent {}, which is a page and not a picture",
            asset.content_type.as_deref().unwrap_or("a page")
        ));
    }

    let extension = image_extension(asset.content_type.as_deref(), url);
    Ok((asset.bytes, extension))
}

/// Whether the stored file is still exactly what `hash` describes.
fn intact(dir: &NovelDir, file: &str, hash: &str) -> bool {
    dir.read_image(file)
        .map(|bytes| hash_bytes(&bytes) == hash)
        .unwrap_or(false)
}

/// The distinct image URLs of these blocks, in the order the page says them.
fn image_urls<'a>(blocks: impl IntoIterator<Item = &'a Block>) -> Vec<String> {
    let mut urls: Vec<String> = Vec::new();
    for block in blocks {
        if let Block::Image { src, .. } = block {
            if !urls.iter().any(|seen| seen == src) {
                urls.push(src.clone());
            }
        }
    }
    urls
}

/// Point every image block at the stored copy of what it referenced.
fn rewrite<'a>(blocks: impl IntoIterator<Item = &'a mut Block>, stored: &HashMap<String, String>) {
    for block in blocks {
        if let Block::Image { src, .. } = block {
            if let Some(file) = stored.get(src) {
                *src = file.clone();
            }
        }
    }
}
