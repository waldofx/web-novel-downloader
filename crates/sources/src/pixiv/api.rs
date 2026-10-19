//! Pixiv's JSON API, as far as this adapter needs it.
//!
//! Pixiv renders its novels with JavaScript, and the pages behind them are the
//! site's real interface: reading the HTML shell would mean re-implementing the
//! app. The API is also what the roadmap asked us to look at first, and it is
//! the only source of some facts — a novel's tags and its embedded images never
//! reach the rendered page.
//!
//! Two properties of the API shape everything below:
//!
//! - every answer is the same envelope, `{"error": …, "message": …, "body": …}`,
//!   and a failed request is **HTTP 200 with `error: true`**. The envelope is
//!   read before the payload, never the status;
//! - ids are strings in some answers and numbers in others — a novel's own `id`
//!   is a string, its `seriesNavData.seriesId` is a number, a series'
//!   `firstNovelId` is a number. A field that carries an id therefore decodes
//!   through [`flex`] rather than through a guess about which it is.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};
use std::collections::HashMap;

/// Read a field the site may send as `null` as its empty value.
///
/// `#[serde(default)]` covers a field that is *missing*; it does not cover an
/// explicit `null`, and the site sends `null` for what it has none of — a novel
/// with no embedded images says `"textEmbeddedImages": null`, and without this
/// that novel does not parse at all.
fn null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

/// Every `/ajax/` answer wraps its payload the same way.
#[derive(Debug, Deserialize)]
struct Envelope {
    #[serde(default)]
    error: bool,
    #[serde(default)]
    message: String,
    #[serde(default)]
    body: serde_json::Value,
}

/// Why an answer's payload could not be read.
#[derive(Debug)]
pub struct BodyError {
    /// The site's own message, or serde's complaint about a body that is JSON
    /// but not the shape we asked for. Never our prose about it.
    pub message: String,
    /// The text was not JSON at all — so whatever answered is not this API. This
    /// is the one failure that is not a parse error but a page that is not the
    /// site, and the caller reports it as such.
    pub not_json: bool,
}

/// Read the payload out of an `/ajax/` answer.
pub fn body_of<T: DeserializeOwned>(text: &str) -> Result<T, BodyError> {
    let envelope: Envelope = serde_json::from_str(text).map_err(|error| BodyError {
        message: error.to_string(),
        not_json: true,
    })?;

    if envelope.error {
        let message = if envelope.message.trim().is_empty() {
            "the API reported an error and no message".to_string()
        } else {
            envelope.message
        };
        return Err(BodyError {
            message,
            not_json: false,
        });
    }

    serde_json::from_value(envelope.body).map_err(|error| BodyError {
        message: error.to_string(),
        not_json: false,
    })
}

/// The image URL to prefer out of the sizes the site offers.
///
/// The site names each size in a map (`original`, `1200x1200`, `480mw`, …) and
/// which keys are present varies by image. `original` is what a downloaded
/// copy wants — it is the file the author uploaded, and the smaller ones are
/// resized derivatives of it.
const WIDEST_FIRST: [&str; 5] = ["original", "1200x1200", "480mw", "240mw", "128x128"];

pub fn best(urls: &HashMap<String, String>) -> Option<&str> {
    WIDEST_FIRST
        .iter()
        .find_map(|key| urls.get(*key))
        .map(String::as_str)
}

/// Ids that the site states as a number in one answer and a string in another.
mod flex {
    use serde::de::Error as _;
    use serde::{Deserialize, Deserializer};

    /// A field that must be there, as `id` is.
    pub fn string<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
        Ok(match serde_json::Value::deserialize(deserializer)? {
            serde_json::Value::String(value) => value,
            serde_json::Value::Number(value) => value.to_string(),
            other => {
                return Err(D::Error::custom(format!(
                    "expected a string or a number, got {other}"
                )));
            }
        })
    }
}

/// One novel, `/ajax/novel/{id}`.
///
/// This answer is both the work's metadata and its only chapter's body: a pixiv
/// novel carries its whole text in `content`, however many reader pages that is.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Novel {
    #[serde(deserialize_with = "flex::string")]
    pub id: String,
    /// Required: a novel always has one, so a body without it is not this
    /// answer. Every other text field is optional, because the site omits the
    /// empty ones.
    pub title: String,
    #[serde(default)]
    pub user_name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub create_date: String,
    #[serde(default)]
    pub upload_date: String,
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub cover_url: String,
    /// The whole text, reader page breaks and all.
    ///
    /// Optional so that an answer that omits it — which is how a work the
    /// account may not read comes back — reaches the caller as an empty body,
    /// which `chapter` explains in terms of the session rather than as a parse
    /// failure naming a field.
    #[serde(default)]
    pub content: String,
    /// `1` for adult, `0` otherwise. Per work, not per site.
    #[serde(default)]
    pub x_restrict: i64,
    /// The work renders as a teaser unless a signed-in account may read it. The
    /// tool uses the access the user already has and never more.
    #[serde(default)]
    pub is_login_only: bool,
    #[serde(default)]
    pub series_nav_data: Option<SeriesNav>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub tags: Tags,
    /// The images the author placed in the text, by the id the text refers to.
    #[serde(default, deserialize_with = "null_as_default")]
    pub text_embedded_images: HashMap<String, EmbeddedImage>,
}

impl Novel {
    /// The series this novel is one chapter of, if any.
    pub fn series_id(&self) -> Option<&str> {
        self.series_nav_data
            .as_ref()
            .map(|nav| nav.series_id.as_str())
    }

    pub fn tag_names(&self) -> Vec<String> {
        self.tags.tags.iter().map(|tag| tag.tag.clone()).collect()
    }
}

/// A novel's tags, which the site wraps in an object with the author's own id.
#[derive(Debug, Default, Deserialize)]
pub struct Tags {
    #[serde(default)]
    tags: Vec<TagEntry>,
}

#[derive(Debug, Deserialize)]
struct TagEntry {
    #[serde(default)]
    tag: String,
}

/// Where a novel sits in its series.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesNav {
    #[serde(deserialize_with = "flex::string")]
    pub series_id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub order: i64,
    #[serde(default)]
    pub next: Option<SeriesNeighbour>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesNeighbour {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub order: i64,
}

/// An image the author embedded in a novel's text.
#[derive(Debug, Deserialize)]
pub struct EmbeddedImage {
    #[serde(default, deserialize_with = "null_as_default")]
    pub urls: HashMap<String, String>,
}

/// A series, `/ajax/novel/series/{id}`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Series {
    #[serde(deserialize_with = "flex::string")]
    pub id: String,
    /// Required, as it is for a novel.
    pub title: String,
    #[serde(default)]
    pub user_name: String,
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub published_content_count: i64,
    #[serde(default)]
    pub x_restrict: i64,
    #[serde(default)]
    pub caption: String,
    /// Plain tag names here, where a novel's tags are objects.
    #[serde(default, deserialize_with = "null_as_default")]
    pub tags: Vec<String>,
    #[serde(default)]
    pub cover: Option<Cover>,
}

#[derive(Debug, Deserialize)]
pub struct Cover {
    #[serde(default, deserialize_with = "null_as_default")]
    pub urls: HashMap<String, String>,
}

/// One page of a series listing, `/ajax/novel/series_content/{id}`.
///
/// The page is answered newest-first and holds up to `limit` entries. Both
/// lists in it are read by the site's own client and hold the same novels; the
/// one under `thumbnails` is the one that carries everything needed here.
#[derive(Debug, Default, Deserialize)]
pub struct SeriesContent {
    #[serde(default, deserialize_with = "null_as_default")]
    pub thumbnails: Thumbnails,
}

#[derive(Debug, Default, Deserialize)]
pub struct Thumbnails {
    #[serde(default, deserialize_with = "null_as_default")]
    pub novel: Vec<SeriesEntry>,
}

/// One novel as the series listing states it.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesEntry {
    #[serde(deserialize_with = "flex::string")]
    pub id: String,
    pub title: String,
    /// The site's own position for the novel, and required: it is what the
    /// listing is *paged and sorted* by, so an entry without one would leave
    /// the series in the order the API happened to use — newest first, which
    /// is backwards.
    pub series_content_order: i64,
    #[serde(default)]
    pub create_date: String,
    #[serde(default)]
    pub update_date: String,
}

/// One illustration, `/ajax/illust/{id}`, which is a different work by whoever
/// drew it and is only read to resolve a `[pixivimage:…]` reference.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Illust {
    #[serde(default, deserialize_with = "null_as_default")]
    pub urls: HashMap<String, String>,
    /// One entry per page of a multi-page illustration.
    #[serde(default, deserialize_with = "null_as_default")]
    pub meta_pages: Vec<IllustPage>,
}

#[derive(Debug, Deserialize)]
pub struct IllustPage {
    #[serde(default, deserialize_with = "null_as_default")]
    pub urls: HashMap<String, String>,
}

impl Illust {
    /// The original image of page `at` (zero-based), falling back to the
    /// illustration's own first image when it does not have that many pages.
    pub fn original_at(&self, at: usize) -> Option<&str> {
        self.meta_pages
            .get(at)
            .and_then(|page| page.urls.get("original"))
            .or_else(|| self.urls.get("original"))
            .map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;

    #[test]
    fn a_failed_answer_is_read_from_the_envelope_and_not_from_the_status() {
        // The site answers a request for something that is not there with
        // HTTP 200 and `error: true`. Nothing may look at the status.
        let error = body_of::<Novel>(fixtures::PIXIV_ERROR).expect_err("the envelope says error");

        assert_eq!(error.message, "Couldn't find requested page");
        assert!(
            !error.not_json,
            "this is the API answering, in its own words: {}",
            error.message
        );
    }

    #[test]
    fn a_body_that_is_not_json_is_not_this_api_answering() {
        // A filter page, a captive portal, a proxy: none of them speak JSON, and
        // the adapter must report that as "not the site" rather than as a parse
        // failure — the two read very differently to whoever is staring at it.
        let error =
            body_of::<Novel>("<html><body>SİTE ENGELLENDİ</body></html>").expect_err("not JSON");

        assert!(error.not_json, "got: {error:?}");
    }

    #[test]
    fn the_novel_answer_carries_the_series_the_tags_and_the_embedded_images() {
        let novel: Novel =
            body_of(fixtures::PIXIV_NOVEL_IN_SERIES).expect("the captured novel answer");

        assert_eq!(novel.id, "10000004");
        assert_eq!(novel.title, "第一話　出会い");
        assert_eq!(novel.user_name, "サンプル作者");
        assert_eq!(novel.tag_names(), vec!["SF", "学園"]);
        assert!(
            novel.content.contains("[newpage]") && novel.content.contains("[[rb:"),
            "the answer holds the whole text, markup and reader page breaks included"
        );
        assert!(
            !novel.is_login_only,
            "a work anyone may read says so with a flag"
        );

        assert_eq!(
            novel.series_id(),
            Some("10000006"),
            "seriesId is a number in the answer and must read as an id"
        );
        let nav = novel.series_nav_data.as_ref().expect("series navigation");
        assert_eq!(nav.title, "サンプルシリーズ");
        assert_eq!(nav.order, 1);
        assert_eq!(nav.next.as_ref().map(|next| next.order), Some(2));

        let embedded = novel
            .text_embedded_images
            .get("21674723")
            .expect("the id the text refers to");
        assert_eq!(
            best(&embedded.urls),
            Some(
                "https://i.pximg.net/novel-cover-original/img/2021/03/26/15/00/36/\
                 tei10000006_deadbeef.png"
            ),
            "the original is the file worth downloading"
        );
    }

    #[test]
    fn a_novel_that_is_not_in_a_series_says_so_by_having_no_series() {
        let novel: Novel =
            body_of(fixtures::PIXIV_NOVEL_STANDALONE).expect("the captured standalone answer");

        assert_eq!(novel.series_id(), None);
        assert!(novel.series_nav_data.is_none());
        assert!(novel.text_embedded_images.is_empty());
        assert!(!novel.cover_url.is_empty());
    }

    #[test]
    fn the_series_answer_reads_with_plain_tags_and_a_numbered_first_novel() {
        let series: Series = body_of(fixtures::PIXIV_SERIES).expect("the captured series answer");

        assert_eq!(series.id, "10000006");
        assert_eq!(series.title, "サンプルシリーズ");
        assert_eq!(series.tags, vec!["R-18", "学園", "SF"]);
        assert_eq!(series.user_name, "サンプル作者");
        assert_eq!(series.language, "ja");

        let cover = series.cover.expect("a cover");
        assert!(
            best(&cover.urls).is_some_and(|url| url.ends_with("sci10000006_deadbeef.png")),
            "got: {:?}",
            best(&cover.urls)
        );
    }

    #[test]
    fn a_series_listing_keeps_the_sites_own_order_for_the_caller_to_sort() {
        // The listing arrives newest-first and its `seriesContentOrder` is the
        // site's numbering, not a chapter number. This is the shape the pager
        // walks; putting it in reading order is the adapter's job.
        let content: SeriesContent =
            body_of(fixtures::PIXIV_SERIES_CONTENT).expect("the captured listing");

        let orders: Vec<i64> = content
            .thumbnails
            .novel
            .iter()
            .map(|entry| entry.series_content_order)
            .collect();

        assert_eq!(orders.len(), 3);
        assert!(
            !orders.windows(2).all(|pair| pair[0] > pair[1]),
            "the fixture must not be sorted, or it proves nothing: {orders:?}"
        );
        assert!(
            content
                .thumbnails
                .novel
                .iter()
                .all(|entry| !entry.id.is_empty())
        );
        assert!(
            content.thumbnails.novel[0].create_date.ends_with("+09:00"),
            "a chapter's dates are stated as instants and are kept as stated"
        );
    }

    #[test]
    fn an_illust_resolves_the_page_a_reference_names() {
        let illust: Illust = body_of(fixtures::PIXIV_ILLUST).expect("the captured illust answer");

        assert!(
            illust
                .original_at(0)
                .is_some_and(|url| url.ends_with("_p0.png"))
        );
        assert!(
            illust
                .original_at(1)
                .is_some_and(|url| url.ends_with("_p1.png"))
        );
        assert_eq!(
            illust.original_at(9),
            illust.urls.get("original").map(String::as_str),
            "a page the illustration does not have falls back to its own image"
        );
    }

    #[test]
    fn a_field_the_site_sends_as_null_reads_as_the_empty_value() {
        // The site says `null`, not nothing, for what it has none of: a novel
        // with no embedded images is an ordinary novel and must still parse.
        let novel: Novel = body_of(
            r#"{"error":false,"body":{"id":"1","title":"t",
                "tags":null,"textEmbeddedImages":null,"seriesNavData":null}}"#,
        )
        .expect("a null field is not a missing field");

        assert!(novel.tag_names().is_empty());
        assert!(novel.text_embedded_images.is_empty());
        assert!(novel.series_id().is_none());
    }

    #[test]
    fn an_answer_that_is_json_but_not_this_shape_is_a_parse_failure() {
        let error = body_of::<Novel>(r#"{"error":false,"message":"","body":{"id":"1"}}"#)
            .expect_err("a novel with no title is not the shape we asked for");

        assert!(
            !error.not_json,
            "the API answered; it just did not answer with a novel: {error:?}"
        );
    }

    #[test]
    fn an_id_stated_as_a_number_reads_as_the_same_id_stated_as_a_string() {
        let body = |id: &str| format!(r#"{{"error":false,"body":{{"id":{id},"title":"t"}}}}"#);

        let as_number = body_of::<Series>(&body("42")).unwrap();
        let as_string = body_of::<Series>(&body(r#""42""#)).unwrap();

        assert_eq!(as_number.id, as_string.id);
    }
}
