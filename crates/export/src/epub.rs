//! EPUB 3.
//!
//! An EPUB is a ZIP with a very specific first entry, a container that points at
//! a package document, and then whatever the package declares. Everything here is
//! written to be reproducible: the entries carry a fixed timestamp and are added
//! in a fixed order, so exporting the same novel twice gives the same bytes.
//!
//! EPUB 3 rather than EPUB 2: the specification this targets is `version="3.0"`,
//! and the navigation document is the one EPUB 3 defines. A `toc.ncx` is not
//! emitted, because every reader that matters reads the navigation document.

use std::fmt::Write as _;
use std::fs;
use std::io::Write as _;
use std::path::Path;

use wnd_core::{Block, Chapter, Manifest, NovelDir};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

use crate::ExportError;
use crate::ExportOptions;
use crate::words::Words;
use crate::xhtml;

/// The EPUB media type. The specification requires it to be the first entry and
/// to be stored rather than compressed, so that a reader can identify the file
/// from the first bytes without inflating anything.
const MIMETYPE: &[u8] = b"application/epub+zip";

/// The CSS, embedded rather than linked so an export is self-contained.
const STYLE: &str = "\
body { line-height: 1.7; margin: 1em; }
h1, h2, h3 { line-height: 1.4; }
h2 { margin: 0 0 0.8em; }
h3 { margin: 1.2em 0 0.6em; }
p { margin: 0 0 0.4em; }
p.arc { margin: 2em 0 0.8em; font-weight: bold; }
aside.note { margin: 1.2em 0; padding: 0.6em 0.9em; border-left: 3px solid #bbb;\
 font-size: 0.9em; color: #444; }
aside.note h3 { margin-top: 0; }
hr { border: 0; border-top: 1px solid #999; }
hr.scene-break { margin: 1.6em 35%; }
img { max-width: 100%; }
";

/// Write `manifest` and `chapters` to `target` as an EPUB, and report its size.
///
/// Images are copied from `dir` when they are there. A missing one is skipped
/// rather than fatal: a directory can hold a chapter whose illustration could not
/// be fetched (`nvd update` tries again), and refusing to export the novel over
/// one absent picture would be the wrong trade. The block still names it, so a
/// reader sees the alt text rather than nothing.
pub fn write(
    dir: &NovelDir,
    manifest: &Manifest,
    chapters: &[Chapter],
    options: &ExportOptions,
    target: &Path,
) -> Result<u64, ExportError> {
    let words = Words::for_language(&manifest.language);
    let images: Vec<String> = referenced_images(manifest, chapters)
        .into_iter()
        .filter(|image| dir.root().join(image).is_file())
        .collect();

    let file = fs::File::create(target)?;
    let mut zip = ZipWriter::new(std::io::BufWriter::new(file));

    // A fixed timestamp on every entry. ZIP records modification times, so
    // reading the clock here would make two exports of one novel differ.
    let stored = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .last_modified_time(DateTime::DEFAULT);
    let deflated = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(DateTime::DEFAULT);

    // 1. The mimetype, first and uncompressed, as the specification demands.
    zip.start_file("mimetype", stored)?;
    zip.write_all(MIMETYPE)?;

    // 2. The container, which is how a reader finds the package document.
    zip.start_file("META-INF/container.xml", deflated)?;
    zip.write_all(CONTAINER_XML.as_bytes())?;

    // 3. The stylesheet.
    zip.start_file("OEBPS/style.css", deflated)?;
    zip.write_all(STYLE.as_bytes())?;

    // 4. The images, copied byte for byte — never re-encoded.
    for image in &images {
        zip.start_file(format!("OEBPS/{image}"), deflated)?;
        zip.write_all(&fs::read(dir.root().join(image))?)?;
    }

    // 5. One document per chapter, in reading order. The manifest is where the
    // adapter recorded the arcs, so this is also where we know a chapter is the
    // first of its arc — and therefore where the arc's divider belongs.
    let mut arc: Option<&str> = None;
    for chapter in chapters {
        let chapter_arc = manifest
            .chapter(chapter.index)
            .and_then(|meta| meta.arc.as_deref());
        let divider = (chapter_arc != arc).then_some(chapter_arc).flatten();
        arc = chapter_arc;

        zip.start_file(format!("OEBPS/{}", chapter_href(chapter.index)), deflated)?;
        zip.write_all(chapter_document(manifest, chapter, divider, options, &words).as_bytes())?;
    }

    // 6. The navigation document, and the package that ties it together.
    zip.start_file("OEBPS/nav.xhtml", deflated)?;
    zip.write_all(nav_document(manifest, chapters, words).as_bytes())?;

    zip.start_file("OEBPS/content.opf", deflated)?;
    zip.write_all(package_document(manifest, chapters, &images, words).as_bytes())?;

    let mut writer = zip.finish()?;
    writer.flush()?;

    Ok(fs::metadata(target)?.len())
}

const CONTAINER_XML: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
<container version=\"1.0\" xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\">\n\
<rootfiles>\n\
<rootfile full-path=\"OEBPS/content.opf\" media-type=\"application/oebps-package+xml\" />\n\
</rootfiles>\n\
</container>\n";

/// Where a chapter's document lives, relative to the package document.
fn chapter_href(index: u32) -> String {
    format!("text/{index:04}.xhtml")
}

/// Every image the index or a chapter refers to, deduplicated and sorted.
///
/// Sorted because the archive has to be reproducible: the order images appear in
/// is the chapter's order, which is stable today, but a sort makes that a
/// property of this function rather than of its callers.
fn referenced_images(manifest: &Manifest, chapters: &[Chapter]) -> Vec<String> {
    let mut images = Vec::new();
    if let Some(cover) = &manifest.cover {
        images.push(cover.clone());
    }
    for chapter in chapters {
        let blocks = chapter
            .nodes
            .iter()
            .chain(&chapter.pre_note)
            .chain(&chapter.post_note);
        for block in blocks {
            if let Block::Image { src, .. } = block {
                images.push(src.clone());
            }
        }
    }
    images.sort();
    images.dedup();
    images
}

fn media_type(path: &str) -> &'static str {
    match path
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

/// One XHTML document, with the boilerplate every one of them shares.
fn document(title: &str, language: &str, stylesheet: &str, body: &str) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    out.push_str("<!DOCTYPE html>\n");
    let _ = writeln!(
        out,
        "<html xmlns=\"http://www.w3.org/1999/xhtml\" \
         xmlns:epub=\"http://www.idpf.org/2007/ops\" \
         xml:lang=\"{language}\" lang=\"{language}\">"
    );
    out.push_str("<head>\n");
    out.push_str("<meta charset=\"utf-8\" />\n");
    let _ = writeln!(out, "<title>{}</title>", xhtml::escape(title));
    let _ = writeln!(
        out,
        "<link rel=\"stylesheet\" type=\"text/css\" href=\"{stylesheet}\" />"
    );
    out.push_str("</head>\n<body>\n");
    out.push_str(body);
    out.push_str("</body>\n</html>\n");
    out
}

/// The chapter's document. Images are one level up, in the archive's root.
///
/// `arc` is the divider to put above the chapter, already narrowed to the
/// chapters that open an arc: an arc heading repeated on every chapter of the
/// arc would be noise in a book, and a chapter document cannot know where it
/// sits in the reading order by itself.
fn chapter_document(
    manifest: &Manifest,
    chapter: &Chapter,
    arc: Option<&str>,
    options: &ExportOptions,
    words: &Words,
) -> String {
    let mut body = String::new();
    body.push_str("<section epub:type=\"chapter\">\n");
    if let Some(arc) = arc {
        // A divider rather than a heading level: the structure a reader
        // navigates by is the nested list in `nav.xhtml`, and TXT and Markdown
        // treat an arc the same way.
        let _ = writeln!(body, "<p class=\"arc\">{}</p>", xhtml::escape(arc));
    }
    let _ = writeln!(body, "<h2>{}</h2>", xhtml::escape(&chapter.title));

    if options.include_notes && !chapter.pre_note.is_empty() {
        let _ = writeln!(
            body,
            "<aside class=\"note\" epub:type=\"preface\">\n<h3>{}</h3>",
            xhtml::escape(words.preface)
        );
        body.push_str(&xhtml::blocks(&chapter.pre_note, "../"));
        body.push_str("</aside>\n");
    }

    body.push_str(&xhtml::blocks(&chapter.nodes, "../"));

    if options.include_notes && !chapter.post_note.is_empty() {
        let _ = writeln!(
            body,
            "<aside class=\"note\" epub:type=\"afterword\">\n<h3>{}</h3>",
            xhtml::escape(words.afterword)
        );
        body.push_str(&xhtml::blocks(&chapter.post_note, "../"));
        body.push_str("</aside>\n");
    }

    body.push_str("</section>\n");
    document(&chapter.title, &manifest.language, "../style.css", &body)
}

/// The navigation document, which is the table of contents a reader shows.
///
/// A chapter that belongs to an arc is nested inside it, which is how EPUB 3
/// expresses a part: the arc is a list item with a `<span>` heading — there is
/// no link to it, it is a divider, not a page — and its chapters are the nested
/// list. A chapter in no arc stays a direct child, so a novel without arcs gets
/// exactly the flat list it got before arcs existed.
fn nav_document(manifest: &Manifest, chapters: &[Chapter], words: Words) -> String {
    let mut body = String::new();
    body.push_str("<nav epub:type=\"toc\" id=\"toc\">\n");
    let _ = writeln!(body, "<h1>{}</h1>", xhtml::escape(words.contents));
    body.push_str("<ol>\n");

    let mut arc: Option<&str> = None;
    for chapter in chapters {
        let chapter_arc = manifest
            .chapter(chapter.index)
            .and_then(|meta| meta.arc.as_deref());
        if chapter_arc != arc {
            if arc.is_some() {
                body.push_str("</ol>\n</li>\n");
            }
            if let Some(name) = chapter_arc {
                let _ = writeln!(body, "<li><span>{}</span>\n<ol>", xhtml::escape(name));
            }
            arc = chapter_arc;
        }
        let _ = writeln!(
            body,
            "<li><a href=\"{}\">{}</a></li>",
            chapter_href(chapter.index),
            xhtml::escape(&chapter.title)
        );
    }
    if arc.is_some() {
        body.push_str("</ol>\n</li>\n");
    }

    body.push_str("</ol>\n</nav>\n");
    document(&manifest.title, &manifest.language, "style.css", &body)
}

/// The package document: what the book is, what is in it, and in what order.
fn package_document(
    manifest: &Manifest,
    chapters: &[Chapter],
    images: &[String],
    words: Words,
) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    let _ = writeln!(
        out,
        "<package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" \
         unique-identifier=\"novel-url\" xml:lang=\"{}\">",
        manifest.language
    );

    out.push_str("<metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\n");
    // The novel's own URL is the identifier: it is stable, it is unique, and it
    // is what a reader would need to find the work again.
    let _ = writeln!(
        out,
        "<dc:identifier id=\"novel-url\">{}</dc:identifier>",
        xhtml::escape(&manifest.source.novel_url)
    );
    let _ = writeln!(
        out,
        "<dc:title>{}</dc:title>",
        xhtml::escape(&manifest.title)
    );
    let _ = writeln!(out, "<dc:language>{}</dc:language>", manifest.language);
    if let Some(author) = &manifest.author {
        let _ = writeln!(out, "<dc:creator>{}</dc:creator>", xhtml::escape(author));
    }
    if let Some(synopsis) = &manifest.synopsis {
        let _ = writeln!(
            out,
            "<dc:description>{}</dc:description>",
            xhtml::escape(synopsis)
        );
    }
    for tag in &manifest.tags {
        let _ = writeln!(out, "<dc:subject>{}</dc:subject>", xhtml::escape(tag));
    }
    // EPUB 3 wants a modification date, and a clock reading would make the export
    // irreproducible, so it comes from the manifest when the fetcher recorded one.
    if let Some(fetched_at) = &manifest.fetched_at {
        let _ = writeln!(
            out,
            "<meta property=\"dcterms:modified\">{fetched_at}</meta>"
        );
    }
    out.push_str("</metadata>\n");

    out.push_str("<manifest>\n");
    let _ = writeln!(
        out,
        "<item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\" />"
    );
    let _ = writeln!(
        out,
        "<item id=\"css\" href=\"style.css\" media-type=\"text/css\" />"
    );
    // Only declare a cover the archive actually carries, or the package would
    // name a file a reader cannot find.
    let cover = manifest
        .cover
        .as_ref()
        .filter(|cover| images.iter().any(|image| image == *cover));
    if let Some(cover) = cover {
        let _ = writeln!(
            out,
            "<item id=\"cover-image\" href=\"{}\" media-type=\"{}\" properties=\"cover-image\" />",
            xhtml::escape(cover),
            media_type(cover)
        );
    }
    for (position, image) in images
        .iter()
        .filter(|image| Some(*image) != manifest.cover.as_ref())
        .enumerate()
    {
        let _ = writeln!(
            out,
            "<item id=\"image-{}\" href=\"{}\" media-type=\"{}\" />",
            position + 1,
            xhtml::escape(image),
            media_type(image)
        );
    }
    for chapter in chapters {
        let _ = writeln!(
            out,
            "<item id=\"chapter-{}\" href=\"{}\" media-type=\"application/xhtml+xml\" />",
            chapter.index,
            chapter_href(chapter.index)
        );
    }
    out.push_str("</manifest>\n");

    out.push_str("<spine>\n");
    for chapter in chapters {
        let _ = writeln!(out, "<itemref idref=\"chapter-{}\" />", chapter.index);
    }
    out.push_str("</spine>\n");

    // A reading system may show this in its UI; the title is the useful label.
    let _ = writeln!(
        out,
        "<guide><reference type=\"toc\" title=\"{}\" \
        href=\"nav.xhtml\" /></guide>",
        xhtml::escape(words.contents)
    );

    out.push_str("</package>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_types_follow_the_extension() {
        assert_eq!(media_type("images/cover.png"), "image/png");
        assert_eq!(media_type("a/b.JPEG"), "image/jpeg");
        assert_eq!(media_type("images/0001-1.webp"), "image/webp");
        assert_eq!(media_type("images/mystery"), "application/octet-stream");
    }

    #[test]
    fn chapter_documents_are_numbered_from_the_index() {
        assert_eq!(chapter_href(1), "text/0001.xhtml");
        assert_eq!(chapter_href(348), "text/0348.xhtml");
    }
}
