//! The exporters, driven through their public API on a real novel directory.
//!
//! The exact expected documents are spelled out line by line on purpose: the
//! output *is* the feature, so a test that only checked "it is not empty" would
//! not be worth writing. Anything the format promises — reading order, the
//! furigana fallback, relative image paths, a stored `mimetype`, byte-for-byte
//! reproducibility — is asserted here.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::{TempDir, entry, entry_bytes, entry_names};
use wnd_core::store::storage::chapter_file_name;
use wnd_core::{
    Block, Chapter, ChapterMeta, ContentRating, Inline, Manifest, NovelDir, SourceInfo,
};
use wnd_export::{ExportError, ExportOptions, Format, RubyStyle, export};

const NOVEL_URL: &str = "https://fake.test/novel/";
const TITLE: &str = "見本の勇者";

fn meta(index: u32, title: &str, arc: Option<&str>) -> ChapterMeta {
    ChapterMeta {
        index,
        title: title.to_string(),
        url: format!("{NOVEL_URL}{index}/"),
        arc: arc.map(str::to_string),
        published_at: None,
        revised_at: None,
        hash: None,
        file: format!("chapters/{}", chapter_file_name(index)),
    }
}

/// The fixture novel: two chapters in one arc, with furigana, emphasis, a scene
/// break, an illustration, and notes on both sides of the first chapter.
fn fixture_manifest() -> Manifest {
    let mut manifest = Manifest::new(
        SourceInfo {
            id: "fake".to_string(),
            novel_url: NOVEL_URL.to_string(),
            site_title: None,
        },
        TITLE,
        "ja",
    );
    manifest.author = Some("見本作者".to_string());
    manifest.synopsis = vec![
        Block::Paragraph {
            children: vec![text("――これはあらすじの見本です。")],
        },
        Block::Image {
            src: "images/synopsis-1.png".to_string(),
            alt: Some("あらすじの挿絵".to_string()),
        },
        Block::Paragraph {
            children: vec![text("本文ではありません。")],
        },
    ];
    manifest.cover = Some("images/cover.png".to_string());
    manifest.content_rating = ContentRating::General;
    manifest.chapters = vec![
        meta(1, "第1話　見本の一話", Some("第一章　見本の章")),
        meta(2, "第2話　見本の二話", Some("第一章　見本の章")),
    ];
    manifest
}

fn text(value: &str) -> Inline {
    Inline::Text {
        value: value.to_string(),
    }
}

fn fixture_chapter_one() -> Chapter {
    let mut chapter = Chapter::new(1, "第1話　見本の一話");
    chapter.pre_note.push(Block::Paragraph {
        children: vec![text("いよいよ新連載始めます！")],
    });
    chapter.nodes.push(Block::Paragraph {
        children: vec![text("　最初の行。")],
    });
    chapter.nodes.push(Block::SceneBreak);
    chapter.nodes.push(Block::Paragraph {
        children: vec![
            text("　"),
            Inline::Ruby {
                base: "漢字".to_string(),
                reading: "かんじ".to_string(),
            },
            text("と"),
            Inline::Strong {
                children: vec![text("強調")],
            },
            text("です。"),
        ],
    });
    chapter.post_note.push(Block::Paragraph {
        children: vec![text("次回もよろしく。")],
    });
    chapter
}

fn fixture_chapter_two() -> Chapter {
    let mut chapter = Chapter::new(2, "第2話　見本の二話");
    chapter.nodes.push(Block::Paragraph {
        children: vec![text("　二話目。")],
    });
    chapter.nodes.push(Block::Image {
        src: "images/0001-1.png".to_string(),
        alt: Some("挿絵".to_string()),
    });
    chapter
}

/// Write the fixture novel to a fresh directory and hand back both the guard and
/// an opened [`NovelDir`].
fn fixture(tag: &str) -> (TempDir, NovelDir) {
    let dir = TempDir::new(tag);
    let novel = NovelDir::at(dir.path());
    novel.ensure_layout().unwrap();
    novel.write_manifest(&fixture_manifest()).unwrap();
    novel.write_chapter(&fixture_chapter_one()).unwrap();
    novel.write_chapter(&fixture_chapter_two()).unwrap();
    write_bytes(novel.root(), "images/cover.png", b"\x89PNG\r\n\x1a\ncover");
    write_bytes(
        novel.root(),
        "images/synopsis-1.png",
        b"\x89PNG\r\n\x1a\nsynopsis",
    );
    write_bytes(
        novel.root(),
        "images/0001-1.png",
        b"\x89PNG\r\n\x1a\nillustration",
    );
    (dir, novel)
}

/// A one-chapter novel with a caller-chosen title, for name handling.
fn minimal(tag: &str, title: &str) -> (TempDir, NovelDir) {
    let dir = TempDir::new(tag);
    let novel = NovelDir::at(dir.path());
    novel.ensure_layout().unwrap();
    let mut manifest = Manifest::new(
        SourceInfo {
            id: "fake".to_string(),
            novel_url: NOVEL_URL.to_string(),
            site_title: None,
        },
        title,
        "ja",
    );
    manifest.chapters = vec![meta(1, "第1話", None)];
    novel.write_manifest(&manifest).unwrap();
    novel.write_chapter(&Chapter::new(1, "第1話")).unwrap();
    (dir, novel)
}

fn write_bytes(root: &Path, relative: &str, bytes: &[u8]) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap()
}

// ---------------------------------------------------------------------------
// TXT
// ---------------------------------------------------------------------------

#[test]
fn the_txt_export_is_plain_text_in_reading_order() {
    let (_guard, novel) = fixture("txt");
    let summary = export(&novel, Format::Txt, &ExportOptions::default()).unwrap();

    assert_eq!(summary.format, Format::Txt);
    assert_eq!(summary.chapters, 2);
    assert_eq!(
        summary.path.file_name().unwrap().to_str().unwrap(),
        "見本の勇者.txt"
    );
    assert_eq!(summary.path, novel.exports_dir().join("見本の勇者.txt"));

    let expected = [
        "見本の勇者",
        "見本作者",
        "",
        "――これはあらすじの見本です。",
        "[画像: あらすじの挿絵]",
        "本文ではありません。",
        "",
        "",
        "【第一章　見本の章】",
        "",
        "第1話　見本の一話",
        "",
        "［前書き］",
        "いよいよ新連載始めます！",
        "",
        "　最初の行。",
        "",
        "＊　＊　＊",
        "",
        "　漢字(かんじ)と強調です。",
        "",
        "［後書き］",
        "次回もよろしく。",
        "",
        "",
        "第2話　見本の二話",
        "",
        "　二話目。",
        "[画像: 挿絵]",
    ]
    .join("\n")
        + "\n";

    assert_eq!(read(&summary.path), expected);
}

#[test]
fn furigana_can_be_reduced_to_the_base_alone() {
    let (_guard, novel) = fixture("txt-base-only");
    let options = ExportOptions {
        ruby: RubyStyle::BaseOnly,
        ..ExportOptions::default()
    };

    let summary = export(&novel, Format::Txt, &options).unwrap();
    let text = read(&summary.path);

    assert!(text.contains("　漢字と強調です。"), "got:\n{text}");
    assert!(!text.contains("かんじ"), "the reading must be gone");
}

#[test]
fn author_notes_can_be_excluded() {
    let (_guard, novel) = fixture("txt-no-notes");
    let options = ExportOptions {
        include_notes: false,
        ..ExportOptions::default()
    };

    let summary = export(&novel, Format::Txt, &options).unwrap();
    let text = read(&summary.path);

    assert!(!text.contains("前書き"));
    assert!(!text.contains("後書き"));
    // The notes are the option, so their text goes with them.
    assert!(!text.contains("いよいよ新連載始めます！"), "got:\n{text}");
    assert!(!text.contains("次回もよろしく。"), "got:\n{text}");
    assert!(text.contains("　最初の行。"), "the body is untouched");
}

#[test]
fn a_script_that_has_no_word_break_marker_gets_a_plain_one() {
    let (_guard, novel) = fixture("txt-scene-break-en");
    let mut manifest = novel.read_manifest().unwrap();
    manifest.language = "en".to_string();
    novel.write_manifest(&manifest).unwrap();

    let summary = export(&novel, Format::Txt, &ExportOptions::default()).unwrap();
    let text = read(&summary.path);
    assert!(text.contains("* * *"), "got:\n{text}");
    assert!(!text.contains("＊　＊　＊"));

    // The default, Japanese, uses the marker a Japanese book uses.
    let (_ja_guard, ja) = fixture("txt-scene-break-ja");
    let ja_summary = export(&ja, Format::Txt, &ExportOptions::default()).unwrap();
    assert!(read(&ja_summary.path).contains("＊　＊　＊"));
}

// ---------------------------------------------------------------------------
// Markdown
// ---------------------------------------------------------------------------

#[test]
fn the_markdown_export_uses_markup_and_relative_image_paths() {
    let (_guard, novel) = fixture("md");
    let summary = export(&novel, Format::Markdown, &ExportOptions::default()).unwrap();

    assert_eq!(Format::Markdown.extension(), "md");
    assert_eq!(
        summary.path.file_name().unwrap().to_str().unwrap(),
        "見本の勇者.md"
    );

    let expected = [
        "# 見本の勇者",
        "",
        "*見本作者*",
        "",
        "――これはあらすじの見本です。",
        "",
        "![あらすじの挿絵](../images/synopsis-1.png)",
        "",
        "本文ではありません。",
        "",
        "## 第一章　見本の章",
        "",
        "### 第1話　見本の一話",
        "",
        "> **前書き**",
        ">",
        "> いよいよ新連載始めます！",
        "",
        "　最初の行。",
        "",
        "* * *",
        "",
        "　漢字(かんじ)と**強調**です。",
        "",
        "",
        "> **後書き**",
        ">",
        "> 次回もよろしく。",
        "",
        "",
        "### 第2話　見本の二話",
        "",
        "　二話目。",
        "",
        // The image lives at images/… relative to the novel directory, but this
        // file lives in exports/, so the path has to be rewritten.
        "![挿絵](../images/0001-1.png)",
    ]
    .join("\n")
        + "\n";

    assert_eq!(read(&summary.path), expected);
}

#[test]
fn markdown_escapes_characters_that_would_be_markup() {
    let (_guard, novel) = minimal("md-escape", "見出し");
    let mut chapter = Chapter::new(1, "第1話");
    chapter.nodes.push(Block::Paragraph {
        children: vec![text("*強調したい* [リンク] と _下線_")],
    });
    chapter.nodes.push(Block::Paragraph {
        children: vec![text("# これは見出しではない")],
    });
    novel.write_chapter(&chapter).unwrap();

    let summary = export(&novel, Format::Markdown, &ExportOptions::default()).unwrap();
    let text = read(&summary.path);

    assert!(
        text.contains(r"\*強調したい\* \[リンク\] と \_下線\_"),
        "got:\n{text}"
    );
    assert!(text.contains(r"\# これは見出しではない"), "got:\n{text}");
}

#[test]
fn markdown_keeps_the_emphasis_it_can_express() {
    let (_guard, novel) = fixture("md-emphasis");
    let mut chapter = novel.read_chapter(2).unwrap();
    chapter.post_note.push(Block::Paragraph {
        children: vec![
            text("ここは"),
            Inline::Emphasis {
                children: vec![text("斜体")],
            },
            text("と"),
            Inline::Strong {
                children: vec![text("太字")],
            },
            text("と"),
            Inline::Link {
                href: "https://example.test/".to_string(),
                children: vec![text("リンク")],
            },
            text("。"),
        ],
    });
    novel.write_chapter(&chapter).unwrap();

    let summary = export(&novel, Format::Markdown, &ExportOptions::default()).unwrap();
    let text = read(&summary.path);

    assert!(
        text.contains("ここは*斜体*と**太字**と[リンク](https://example.test/)。"),
        "got:\n{text}"
    );
}

// ---------------------------------------------------------------------------
// File naming and failure
// ---------------------------------------------------------------------------

#[test]
fn a_title_that_is_not_a_file_name_is_sanitised() {
    let (_guard, novel) = minimal("naming", r#"フール/バー:「最強」*?"#);
    let summary = export(&novel, Format::Txt, &ExportOptions::default()).unwrap();

    assert_eq!(
        summary.path.file_name().unwrap().to_str().unwrap(),
        "フール_バー_「最強」__.txt"
    );
}

#[test]
fn a_title_that_sanitises_away_to_nothing_falls_back_to_the_source() {
    let (_guard, novel) = minimal("naming-empty", "   ");
    let summary = export(&novel, Format::Txt, &ExportOptions::default()).unwrap();

    assert_eq!(
        summary.path.file_name().unwrap().to_str().unwrap(),
        "fake.txt"
    );
}

#[test]
fn an_empty_novel_cannot_be_exported() {
    let (_guard, novel) = minimal("empty", TITLE);
    let mut manifest = novel.read_manifest().unwrap();
    manifest.chapters.clear();
    novel.write_manifest(&manifest).unwrap();

    let error = export(&novel, Format::Txt, &ExportOptions::default()).unwrap_err();

    assert!(matches!(error, ExportError::Empty(_)), "got {error:?}");
}

// ---------------------------------------------------------------------------
// EPUB
// ---------------------------------------------------------------------------

/// Parse the first local file header by hand.
///
/// The EPUB specification requires `mimetype` to be the first entry and to be
/// stored uncompressed, and that is a property of the bytes, not of anything an
/// archive reader will tell us — so this reads the header.
fn mimetype_entry(path: &Path) -> (u16, Vec<u8>) {
    let bytes = fs::read(path).unwrap();
    assert!(
        bytes.len() > 30,
        "the file is too short to hold a local file header"
    );
    assert_eq!(
        &bytes[0..4],
        b"PK\x03\x04",
        "the archive must start with the mimetype entry's local file header"
    );

    let method = u16::from_le_bytes([bytes[8], bytes[9]]);
    let size = u32::from_le_bytes(bytes[22..26].try_into().unwrap()) as usize;
    let name_len = u16::from_le_bytes([bytes[26], bytes[27]]) as usize;
    let extra_len = u16::from_le_bytes([bytes[28], bytes[29]]) as usize;

    assert_eq!(&bytes[30..30 + name_len], b"mimetype");
    let start = 30 + name_len + extra_len;
    (method, bytes[start..start + size].to_vec())
}

#[test]
fn the_epub_writes_its_mimetype_uncompressed_and_first() {
    let (_guard, novel) = fixture("epub-mimetype");
    let summary = export(&novel, Format::Epub, &ExportOptions::default()).unwrap();

    assert_eq!(Format::Epub.extension(), "epub");
    assert!(summary.bytes > 0);
    assert_eq!(
        entry_names(&summary.path).first().map(String::as_str),
        Some("mimetype")
    );

    let (method, content) = mimetype_entry(&summary.path);
    assert_eq!(method, 0, "0 is stored; deflate would be 8");
    assert_eq!(content, b"application/epub+zip");
}

#[test]
fn the_epub_has_a_container_a_package_and_one_document_per_chapter() {
    let (_guard, novel) = fixture("epub-structure");
    let summary = export(&novel, Format::Epub, &ExportOptions::default()).unwrap();

    let names = entry_names(&summary.path);
    for expected in [
        "META-INF/container.xml",
        "OEBPS/content.opf",
        "OEBPS/nav.xhtml",
        "OEBPS/style.css",
        "OEBPS/text/synopsis.xhtml",
        "OEBPS/images/synopsis-1.png",
    ] {
        assert!(names.contains(&expected.to_string()), "missing {expected}");
    }

    // container.xml points at the package document.
    assert!(
        entry(&summary.path, "META-INF/container.xml").contains("OEBPS/content.opf"),
        "container.xml must name the package document"
    );

    // The package document is a valid OPF skeleton.
    let opf = entry(&summary.path, "OEBPS/content.opf");
    assert!(opf.contains("<dc:title>見本の勇者</dc:title>"));
    assert!(opf.contains("<dc:creator>見本作者</dc:creator>"));
    assert!(opf.contains("<dc:language>ja</dc:language>"));
    assert!(opf.contains(&format!("<dc:identifier id=\"novel-url\">{NOVEL_URL}")));
    assert!(opf.contains("properties=\"nav\""));
}

#[test]
fn the_epub_opens_with_the_synopsis_and_its_picture() {
    let (_guard, novel) = fixture("epub-synopsis");
    let summary = export(&novel, Format::Epub, &ExportOptions::default()).unwrap();

    let page = entry(&summary.path, "OEBPS/text/synopsis.xhtml");
    assert!(page.contains("epub:type=\"frontmatter\""), "got:\n{page}");
    assert!(
        page.contains("――これはあらすじの見本です。"),
        "got:\n{page}"
    );
    assert!(
        page.contains("<img src=\"../images/synopsis-1.png\" alt=\"あらすじの挿絵\" />"),
        "the picture sits between its lines, where the author put it:\n{page}"
    );
    assert!(page.contains("本文ではありません。"), "got:\n{page}");

    // The spine opens with it, and the description keeps the words so a reader
    // that shows metadata has the synopsis too.
    let opf = entry(&summary.path, "OEBPS/content.opf");
    assert!(
        opf.contains("<spine>\n<itemref idref=\"synopsis\" />\n<itemref idref=\"chapter-1\" />"),
        "the synopsis page comes before the first chapter:\n{opf}"
    );
    assert!(
        opf.contains(
            "<dc:description>――これはあらすじの見本です。\n\n本文ではありません。</dc:description>"
        ),
        "got:\n{opf}"
    );
}

#[test]
fn every_resource_the_package_claims_is_in_the_archive() {
    let (_guard, novel) = fixture("epub-resources");
    let summary = export(&novel, Format::Epub, &ExportOptions::default()).unwrap();

    let opf = entry(&summary.path, "OEBPS/content.opf");
    let names = entry_names(&summary.path);

    let mut declared = 0;
    for attribute in opf.split("href=\"").skip(1) {
        let href = attribute.split('"').next().unwrap();
        let path = format!("OEBPS/{href}");
        assert!(
            names.contains(&path),
            "the package declares {href}, but {path} is not in the archive"
        );
        declared += 1;
    }
    assert!(declared >= 5, "the package should declare its resources");
}

#[test]
fn the_epub_preserves_furigana_as_native_ruby() {
    let (_guard, novel) = fixture("epub-ruby");
    let summary = export(&novel, Format::Epub, &ExportOptions::default()).unwrap();

    let chapter = entry(&summary.path, "OEBPS/text/0001.xhtml");
    assert!(
        chapter.contains("<ruby>漢字<rp>(</rp><rt>かんじ</rt><rp>)</rp></ruby>"),
        "got:\n{chapter}"
    );
    assert!(chapter.contains("<strong>強調</strong>"), "got:\n{chapter}");
    // The notes are in the same document, marked as such.
    assert!(chapter.contains("epub:type=\"preface\""));
    assert!(chapter.contains("epub:type=\"afterword\""));
    assert!(chapter.contains("　最初の行。"));
}

#[test]
fn the_epub_lists_every_chapter_in_its_navigation() {
    let (_guard, novel) = fixture("epub-nav");
    let summary = export(&novel, Format::Epub, &ExportOptions::default()).unwrap();

    let nav = entry(&summary.path, "OEBPS/nav.xhtml");
    assert!(nav.contains("epub:type=\"toc\""));
    assert!(nav.contains("<a href=\"text/0001.xhtml\">第1話　見本の一話</a>"));
    assert!(nav.contains("<a href=\"text/0002.xhtml\">第2話　見本の二話</a>"));
}

/// A four-chapter novel in two arcs, with one chapter that belongs to neither —
/// the shape an author gets by adding a chapter without a divider above it.
fn arc_fixture(tag: &str) -> (TempDir, NovelDir) {
    let dir = TempDir::new(tag);
    let novel = NovelDir::at(dir.path());
    novel.ensure_layout().unwrap();

    let mut manifest = Manifest::new(
        SourceInfo {
            id: "fake".to_string(),
            novel_url: NOVEL_URL.to_string(),
            site_title: None,
        },
        "四章構成",
        "ja",
    );
    manifest.chapters = vec![
        meta(1, "第1話", Some("第一章　出発")),
        meta(2, "第2話", Some("第一章　出発")),
        meta(3, "第3話", None),
        meta(4, "第4話", Some("第二章　帰還")),
    ];
    novel.write_manifest(&manifest).unwrap();

    for index in 1..=4 {
        let mut chapter = Chapter::new(index, format!("第{index}話"));
        chapter.nodes.push(Block::Paragraph {
            children: vec![text(&format!("　{index}話目の本文。"))],
        });
        novel.write_chapter(&chapter).unwrap();
    }
    (dir, novel)
}

#[test]
fn the_epub_navigation_nests_chapters_under_their_arc() {
    let (_guard, novel) = fixture("epub-nav-arcs");
    let summary = export(&novel, Format::Epub, &ExportOptions::default()).unwrap();

    let nav = entry(&summary.path, "OEBPS/nav.xhtml");
    let expected = "\
<ol>
<li><span>第一章　見本の章</span>
<ol>
<li><a href=\"text/0001.xhtml\">第1話　見本の一話</a></li>
<li><a href=\"text/0002.xhtml\">第2話　見本の二話</a></li>
</ol>
</li>
</ol>
";
    assert!(
        nav.contains(expected),
        "a reader's table of contents must show the arc, not a flat list of chapters:\n{nav}"
    );
}

#[test]
fn the_epub_navigation_shows_an_arc_beside_a_chapter_that_is_in_no_arc() {
    let (_guard, novel) = arc_fixture("epub-nav-mixed");
    let summary = export(&novel, Format::Epub, &ExportOptions::default()).unwrap();

    let nav = entry(&summary.path, "OEBPS/nav.xhtml");
    let expected = "\
<ol>
<li><span>第一章　出発</span>
<ol>
<li><a href=\"text/0001.xhtml\">第1話</a></li>
<li><a href=\"text/0002.xhtml\">第2話</a></li>
</ol>
</li>
<li><a href=\"text/0003.xhtml\">第3話</a></li>
<li><span>第二章　帰還</span>
<ol>
<li><a href=\"text/0004.xhtml\">第4話</a></li>
</ol>
</li>
</ol>
";
    assert!(
        nav.contains(expected),
        "the loose chapter belongs to no arc and must not be filed under one:\n{nav}"
    );
}

#[test]
fn a_novel_without_arcs_has_a_flat_navigation() {
    let (_guard, novel) = minimal("epub-nav-flat", "アーク無し");
    let summary = export(&novel, Format::Epub, &ExportOptions::default()).unwrap();

    let nav = entry(&summary.path, "OEBPS/nav.xhtml");
    assert!(nav.contains("<ol>\n<li><a href=\"text/0001.xhtml\">第1話</a></li>\n</ol>"));
    assert!(
        !nav.contains("<span>"),
        "there is no arc to group by, so nothing may be invented:\n{nav}"
    );
}

#[test]
fn a_chapter_document_marks_where_its_arc_begins_and_not_again() {
    let (_guard, novel) = arc_fixture("epub-arc-divider");
    let summary = export(&novel, Format::Epub, &ExportOptions::default()).unwrap();

    let chapter = |index: u32| entry(&summary.path, &format!("OEBPS/text/{index:04}.xhtml"));

    assert!(
        chapter(1).contains("第一章　出発"),
        "the first chapter of an arc carries the divider:\n{}",
        chapter(1)
    );
    assert!(
        !chapter(2).contains("第一章　出発"),
        "the divider belongs above the first chapter of the arc and only there, \
         the same rule TXT and Markdown follow:\n{}",
        chapter(2)
    );
    assert!(
        !chapter(3).contains("第一章"),
        "a chapter in no arc carries no divider:\n{}",
        chapter(3)
    );
    assert!(
        chapter(4).contains("第二章　帰還"),
        "the second arc gets its own divider:\n{}",
        chapter(4)
    );
}

#[test]
fn the_epub_carries_the_cover_and_the_illustrations() {
    let (_guard, novel) = fixture("epub-images");
    let summary = export(&novel, Format::Epub, &ExportOptions::default()).unwrap();

    let names = entry_names(&summary.path);
    assert!(names.contains(&"OEBPS/images/cover.png".to_string()));
    assert!(names.contains(&"OEBPS/images/0001-1.png".to_string()));

    assert_eq!(
        entry_bytes(&summary.path, "OEBPS/images/cover.png"),
        b"\x89PNG\r\n\x1a\ncover",
        "the image must be copied, not re-encoded"
    );

    let opf = entry(&summary.path, "OEBPS/content.opf");
    assert!(
        opf.contains("properties=\"cover-image\""),
        "the cover needs its property so readers show it"
    );

    // The chapter refers to the illustration relatively, from text/.
    let chapter = entry(&summary.path, "OEBPS/text/0002.xhtml");
    assert!(
        chapter.contains("src=\"../images/0001-1.png\""),
        "got:\n{chapter}"
    );
}

#[test]
fn exporting_twice_produces_identical_bytes() {
    let (_guard, novel) = fixture("determinism");

    let first = export(&novel, Format::Epub, &ExportOptions::default()).unwrap();
    assert_eq!(first.format, Format::Epub);
    let first_bytes = fs::read(&first.path).unwrap();
    let second = export(&novel, Format::Epub, &ExportOptions::default()).unwrap();
    let second_bytes = fs::read(&second.path).unwrap();

    assert_eq!(
        first_bytes, second_bytes,
        "an export must be reproducible: no clock, no unstable ordering"
    );

    // The same holds for the text formats, which is what makes re-exports diff
    // cleanly instead of churning.
    let txt_a = export(&novel, Format::Txt, &ExportOptions::default()).unwrap();
    let txt_b = export(&novel, Format::Txt, &ExportOptions::default()).unwrap();
    assert_eq!(fs::read(txt_a.path).unwrap(), fs::read(txt_b.path).unwrap());
}

#[test]
fn the_export_directory_is_created_and_reused() {
    let (_guard, novel) = fixture("exports-dir");
    let exports: PathBuf = novel.exports_dir();

    assert!(!exports.exists());
    let summary = export(&novel, Format::Txt, &ExportOptions::default()).unwrap();

    assert!(exports.is_dir(), "exports/ must be created");
    assert!(
        summary.path.starts_with(&exports),
        "and the file goes in it"
    );
}
