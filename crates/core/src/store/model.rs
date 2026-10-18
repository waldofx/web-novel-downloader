//! The data model of a downloaded novel.
//!
//! This is the interchange format between the downloader, the exporters, and the
//! future translation stage. See `docs/engineering/format.md` for the rationale;
//! the tests here assert that the serialized shape still matches what that
//! document describes.

use serde::{Deserialize, Serialize};

/// Version of the on-disk format. Bump when the shape changes incompatibly.
pub const FORMAT_VERSION: u32 = 1;

/// Whether a work is safe for general audiences or is adult content.
///
/// This is a content rating, not an access control: the tool never bypasses
/// age gates, logins, or paywalls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentRating {
    #[default]
    General,
    Adult,
}

/// Where a novel came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceInfo {
    /// Adapter id, e.g. `narou`.
    pub id: String,
    /// Canonical URL of the novel's table of contents.
    pub novel_url: String,
    /// The site's own title, kept for reference when it differs from ours.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site_title: Option<String>,
}

/// One entry in the manifest's chapter index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChapterMeta {
    pub index: u32,
    pub title: String,
    pub url: String,
    /// Arc (章) the chapter belongs to, when the site groups chapters into arcs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arc: Option<String>,
    /// RFC 3339, when the site exposes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
    /// RFC 3339, when the site says the chapter was last **revised**.
    ///
    /// A different fact from `published_at`, which stays the publication time: a
    /// revision does not rewrite when the chapter appeared. Narou publishes this
    /// for the chapters it has revised and for no others; a site that publishes
    /// no revisions (Kakuyomu, checked against the live site) leaves it `None`.
    ///
    /// `nvd update` fetches a chapter when this differs from the index's — in
    /// either direction, because a stamp that disappears means "cannot tell",
    /// which is not "unchanged".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revised_at: Option<String>,
    /// Hash of the chapter's canonical JSON, used to detect changes cheaply.
    ///
    /// `None` until the chapter has been fetched and written at least once;
    /// an adapter that only lists a chapter cannot know it yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
    /// Path to the chapter file, relative to the novel directory.
    pub file: String,
}

/// One image the downloader stored in `images/`.
///
/// A record of what was fetched — not of what the chapters point at, which is
/// the same thing only until a chapter is rewritten. Its purpose is the one
/// `ChapterMeta::hash` has: a later run can tell a stored image from a missing
/// or corrupted one without trusting a file name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredImage {
    /// The URL it was fetched from. Also its identity: the same URL next run
    /// means the same bytes, so nothing is fetched again.
    pub url: String,
    /// Path inside the novel directory, e.g. `images/0001-1.png`.
    pub file: String,
    /// `sha256:…` over the stored bytes. As with a chapter, the hash is what a
    /// resume check compares — never mere existence.
    pub hash: String,
}

/// Top-level metadata for one downloaded novel (`manifest.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub format_version: u32,
    pub source: SourceInfo,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// BCP 47 language tag of the *source* text, e.g. `ja`.
    pub language: String,
    pub content_rating: ContentRating,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub synopsis: Option<String>,
    /// Path to the cover image, relative to the novel directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<String>,
    /// URL of the cover image, as the source states it.
    ///
    /// Kept separate from `cover`, which is a path to an image that has been
    /// *fetched* into `images/`. A source can know the URL without anything having
    /// been fetched yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover_url: Option<String>,
    /// The site's one-line catchphrase, kept separate from the synopsis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catchphrase: Option<String>,
    /// Content warnings the source publishes for this work (e.g. "sexual",
    /// "violent", "cruel"). A warning is not a rating: it describes what the work
    /// contains, and is shown on all-ages sites too.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    /// RFC 3339 timestamp of the most recent fetch. Filled in by the fetcher.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fetched_at: Option<String>,
    #[serde(default)]
    pub chapters: Vec<ChapterMeta>,
    /// Images stored in `images/`, cover included.
    ///
    /// Empty for a novel with no illustrations and a cover nothing has fetched
    /// yet — which is every novel an adapter has only just described.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<StoredImage>,
}

impl Manifest {
    /// A manifest for a newly discovered novel, with no chapters fetched yet.
    pub fn new(source: SourceInfo, title: impl Into<String>, language: impl Into<String>) -> Self {
        Self {
            format_version: FORMAT_VERSION,
            source,
            title: title.into(),
            author: None,
            language: language.into(),
            content_rating: ContentRating::General,
            tags: Vec::new(),
            synopsis: None,
            cover: None,
            cover_url: None,
            catchphrase: None,
            warnings: Vec::new(),
            fetched_at: None,
            chapters: Vec::new(),
            images: Vec::new(),
        }
    }

    pub fn chapter(&self, index: u32) -> Option<&ChapterMeta> {
        self.chapters.iter().find(|c| c.index == index)
    }

    /// The stored image that came from `url`, when there is one.
    pub fn image(&self, url: &str) -> Option<&StoredImage> {
        self.images.iter().find(|image| image.url == url)
    }
}

/// A block-level element of chapter content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Block {
    Paragraph {
        children: Vec<Inline>,
    },
    Heading {
        level: u8,
        children: Vec<Inline>,
    },
    SceneBreak,
    HorizontalRule,
    /// An illustration in the body.
    ///
    /// `src` is what a renderer opens: a path into the novel directory's
    /// `images/` once the bytes have been stored, and the URL the source states
    /// until then. An adapter can *find* an image but only the fetcher can store
    /// one, so this is one field with one meaning — where the picture is. See
    /// `docs/engineering/format.md`, "A URL is not a path".
    Image {
        src: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        alt: Option<String>,
    },
}

/// An inline element. Only [`Inline::Text`] carries prose to translate; every
/// other variant exists to preserve structure and styling around it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Inline {
    Text {
        value: String,
    },
    /// Furigana. First-class because Japanese web novels use it constantly, and
    /// because Narou-family sites encode it as plain text (`｜漢字《かんじ》`).
    Ruby {
        base: String,
        reading: String,
    },
    Emphasis {
        children: Vec<Inline>,
    },
    Strong {
        children: Vec<Inline>,
    },
    Link {
        href: String,
        children: Vec<Inline>,
    },
}

/// One chapter (`chapters/NNNN.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chapter {
    pub format_version: u32,
    pub index: u32,
    pub title: String,
    #[serde(default)]
    pub nodes: Vec<Block>,
    /// 前書き — the author's note before the chapter body.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pre_note: Vec<Block>,
    /// 後書き — the author's note after the chapter body.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub post_note: Vec<Block>,
}

impl Chapter {
    pub fn new(index: u32, title: impl Into<String>) -> Self {
        Self {
            format_version: FORMAT_VERSION,
            index,
            title: title.into(),
            nodes: Vec::new(),
            pre_note: Vec::new(),
            post_note: Vec::new(),
        }
    }

    /// Every translatable string in the chapter, in reading order.
    pub fn translatable_texts(&self) -> impl Iterator<Item = &str> {
        let blocks = self
            .nodes
            .iter()
            .chain(&self.pre_note)
            .chain(&self.post_note);
        blocks.flat_map(block_texts)
    }
}

fn block_texts(block: &Block) -> Box<dyn Iterator<Item = &str> + '_> {
    match block {
        Block::Paragraph { children } | Block::Heading { children, .. } => {
            Box::new(inline_texts(children))
        }
        Block::SceneBreak | Block::HorizontalRule | Block::Image { .. } => {
            Box::new(std::iter::empty())
        }
    }
}

fn inline_texts(inline: &[Inline]) -> impl Iterator<Item = &str> {
    inline
        .iter()
        .flat_map(|node| -> Box<dyn Iterator<Item = &str> + '_> {
            match node {
                Inline::Text { value } => Box::new(std::iter::once(value.as_str())),
                Inline::Emphasis { children } | Inline::Strong { children } => {
                    Box::new(inline_texts(children))
                }
                Inline::Link { children, .. } => Box::new(inline_texts(children)),
                // Ruby's base is the text that gets translated; its reading is
                // annotation and is handled deliberately by the translation stage.
                Inline::Ruby { base, .. } => Box::new(std::iter::once(base.as_str())),
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(value: &str) -> Inline {
        Inline::Text {
            value: value.to_string(),
        }
    }

    #[test]
    fn chapter_json_matches_the_documented_shape() {
        // The exact example from docs/engineering/format.md. If this stops
        // parsing, the docs and the code have drifted apart.
        let json = r#"{
          "format_version": 1,
          "index": 1,
          "title": "…",
          "nodes": [
            { "type": "paragraph", "children": [ { "type": "text", "value": "…" } ] },
            { "type": "paragraph", "children": [
                { "type": "ruby", "base": "漢字", "reading": "かんじ" },
                { "type": "text", "value": "の" },
                { "type": "emphasis", "children": [ { "type": "text", "value": "…" } ] }
            ] },
            { "type": "scene_break" },
            { "type": "image", "src": "images/0001-1.png", "alt": "…" },
            { "type": "horizontal_rule" }
          ]
        }"#;

        let chapter: Chapter = serde_json::from_str(json).expect("documented shape must parse");

        assert_eq!(chapter.format_version, FORMAT_VERSION);
        assert_eq!(chapter.nodes.len(), 5);
        assert!(chapter.pre_note.is_empty());

        let want = vec!["…", "漢字", "の", "…"];
        assert_eq!(chapter.translatable_texts().collect::<Vec<_>>(), want);
    }

    #[test]
    fn unknown_version_is_preserved_for_the_caller_to_check() {
        // serde alone must not silently coerce versions; storage validates.
        let json = r#"{ "format_version": 99, "index": 1, "title": "t", "nodes": [] }"#;
        let chapter: Chapter = serde_json::from_str(json).unwrap();
        assert_eq!(chapter.format_version, 99);
    }

    #[test]
    fn optional_fields_are_omitted_not_null() {
        let chapter = Chapter::new(3, "第三章");
        let json = serde_json::to_string(&chapter).unwrap();
        assert!(json.contains(r#""index":3"#));
        assert!(!json.contains("pre_note"), "empty notes must be omitted");
        assert!(
            !json.contains("null"),
            "optionals must be omitted, not null: {json}"
        );
    }

    #[test]
    fn ruby_and_emphasis_survive_a_round_trip() {
        let mut chapter = Chapter::new(1, "t");
        chapter.nodes.push(Block::Paragraph {
            children: vec![
                Inline::Ruby {
                    base: "漢字".into(),
                    reading: "かんじ".into(),
                },
                text("の"),
                Inline::Strong {
                    children: vec![text("強調")],
                },
            ],
        });

        let json = serde_json::to_string(&chapter).unwrap();
        let back: Chapter = serde_json::from_str(&json).unwrap();
        assert_eq!(chapter, back);
    }

    #[test]
    fn a_revision_stamp_is_kept_apart_from_the_publication_date() {
        // Narou lists a revised chapter's revision time in the index. It is a
        // different fact from when the chapter was published, and an update run
        // compares it to decide whether the author changed anything — so it is
        // stored on its own rather than overwriting `published_at`.
        let meta = ChapterMeta {
            index: 1,
            title: "第一章".into(),
            url: "https://ncode.syosetu.com/n1234ab/1/".into(),
            arc: None,
            published_at: Some("2020-04-01T23:23:00+09:00".into()),
            revised_at: Some("2023-09-01T14:36:00+09:00".into()),
            hash: None,
            file: "chapters/0001.json".into(),
        };

        let json = serde_json::to_string(&meta).unwrap();
        assert!(
            json.contains(r#""revised_at":"2023-09-01T14:36:00+09:00""#),
            "the revision time must be stored: {json}"
        );
        assert_eq!(serde_json::from_str::<ChapterMeta>(&json).unwrap(), meta);

        // Never revised, or a site that does not publish revisions: nothing is
        // said, and the publication date is left alone.
        let mut unrevised = meta.clone();
        unrevised.revised_at = None;
        let json = serde_json::to_string(&unrevised).unwrap();
        assert!(!json.contains("revised_at"), "absent means absent: {json}");
        assert_eq!(
            serde_json::from_str::<ChapterMeta>(&json)
                .unwrap()
                .published_at
                .as_deref(),
            Some("2020-04-01T23:23:00+09:00")
        );
    }

    #[test]
    fn chapter_meta_omits_absent_arc_and_hash() {
        let meta = ChapterMeta {
            index: 1,
            title: "第一章".into(),
            url: "https://ncode.syosetu.com/n1234ab/1/".into(),
            arc: None,
            published_at: None,
            revised_at: None,
            hash: None,
            file: "chapters/0001.json".into(),
        };
        let json = serde_json::to_string(&meta).unwrap();
        assert!(!json.contains("arc"), "absent arc must be omitted: {json}");
        assert!(
            !json.contains("revised_at"),
            "an unabridged manifest is not cluttered with absent facts: {json}"
        );
        assert!(!json.contains("hash"), "unset hash must be omitted: {json}");
        assert!(json.contains(r#""file":"chapters/0001.json""#));

        let back: ChapterMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(back, meta);

        // A pre-existing manifest without the newer fields still parses.
        let legacy = r#"{"index":2,"title":"二","url":"u","published_at":"2020-01-01T00:00:00+09:00","hash":"sha256:x","file":"chapters/0002.json"}"#;
        let parsed: ChapterMeta = serde_json::from_str(legacy).unwrap();
        assert_eq!(parsed.arc, None);
        assert_eq!(parsed.revised_at, None);
        assert_eq!(parsed.hash.as_deref(), Some("sha256:x"));
    }

    #[test]
    fn content_rating_serializes_as_snake_case() {
        assert_eq!(
            serde_json::to_string(&ContentRating::Adult).unwrap(),
            r#""adult""#
        );
        assert_eq!(ContentRating::default(), ContentRating::General);
    }

    #[test]
    fn manifest_json_matches_the_documented_shape() {
        // The example from docs/engineering/format.md, images included. If this
        // stops parsing, the docs and the code have drifted apart.
        let json = r#"{
          "format_version": 1,
          "source": { "id": "narou", "novel_url": "https://ncode.syosetu.com/n1234ab/" },
          "title": "…",
          "author": "…",
          "language": "ja",
          "content_rating": "general",
          "tags": ["…"],
          "synopsis": "…",
          "cover": "images/cover.jpg",
          "cover_url": "https://example.test/cover.jpg",
          "fetched_at": "2026-10-17T00:00:00Z",
          "chapters": [
            {
              "index": 1,
              "title": "…",
              "url": "…",
              "arc": "第一章",
              "published_at": "2020-04-01T23:23:00+09:00",
              "revised_at": "2023-09-01T14:36:00+09:00",
              "hash": "sha256:…",
              "file": "chapters/0001.json"
            }
          ],
          "images": [
            {
              "url": "https://example.test/cover.jpg",
              "file": "images/cover.jpg",
              "hash": "sha256:…"
            }
          ]
        }"#;

        let manifest: Manifest = serde_json::from_str(json).expect("documented shape must parse");
        assert_eq!(manifest.format_version, FORMAT_VERSION);
        assert_eq!(manifest.chapters.len(), 1);
        assert_eq!(
            manifest
                .image("https://example.test/cover.jpg")
                .map(|image| image.file.as_str()),
            Some("images/cover.jpg")
        );
    }

    #[test]
    fn an_empty_image_list_is_omitted_and_a_missing_one_still_parses() {
        let manifest = Manifest::new(
            SourceInfo {
                id: "narou".into(),
                novel_url: "https://ncode.syosetu.com/n1234ab/".into(),
                site_title: None,
            },
            "テスト小説",
            "ja",
        );
        let json = serde_json::to_string(&manifest).unwrap();
        assert!(
            !json.contains("images") && !json.contains("null"),
            "a novel with no pictures says nothing about pictures: {json}"
        );

        // A directory written before images were recorded is still readable:
        // the list is an addition, so the version does not move for it.
        let legacy = r#"{"format_version":1,
          "source":{"id":"narou","novel_url":"https://ncode.syosetu.com/n1234ab/"},
          "title":"t","language":"ja","content_rating":"general","chapters":[]}"#;
        assert!(
            serde_json::from_str::<Manifest>(legacy)
                .unwrap()
                .images
                .is_empty()
        );
    }
}
