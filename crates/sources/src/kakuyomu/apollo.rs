//! The JSON snapshot a Next.js page ships next to its HTML.
//!
//! Kakuyomu is a Next.js app. Every page embeds a `<script id="__NEXT_DATA__">`
//! holding the Apollo cache — the data the client-side app would render from.
//! The *rendered* HTML is only part of that data, and on a work page it is a
//! small part (see [`super::work`]), so the snapshot is not a shortcut: it is the
//! source.
//!
//! Apollo stores objects in one flat map, keyed `<Type>:<id>`, and refers to
//! them with `{"__ref": "<key>"}`. So a work page's data is spread across
//! `Work:<id>`, `TableOfContentsChapter:<id>`, `Chapter:<id>`, `Episode:<id>` and
//! `UserAccount:<id>` entries that point at each other.
//!
//! Nothing here panics. A page with no snapshot, or one whose shape has moved,
//! yields `None`; the caller turns that into an error naming the URL, because a
//! silent empty result is how an adapter rots without anyone noticing.

use scraper::{Html, Selector};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use std::sync::LazyLock;

static SEL_NEXT_DATA: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("script#__NEXT_DATA__").expect("selector literal is valid"));

/// A `{"__ref": "Type:id"}` pointer to another entry.
#[derive(Debug, Deserialize)]
pub(super) struct Reference {
    #[serde(rename = "__ref")]
    pub(super) key: String,
}

/// The Apollo cache of a page, if it has one.
pub(super) struct Apollo(Map<String, Value>);

impl Apollo {
    /// Read the snapshot out of a page's HTML.
    pub(super) fn from_html(html: &str) -> Option<Self> {
        let document = Html::parse_document(html);
        let script = document.select(&SEL_NEXT_DATA).next()?;
        let payload: String = script.text().collect();
        let json: Value = serde_json::from_str(&payload).ok()?;
        let state = json
            .get("props")?
            .get("pageProps")?
            .get("__APOLLO_STATE__")?;
        Some(Self(state.as_object()?.clone()))
    }

    /// Whether an entry exists, so a caller can tell "the site changed shape"
    /// from "this page is not the one you asked for".
    pub(super) fn contains(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }

    /// The entry at `key`, deserialized.
    pub(super) fn entry<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        serde_json::from_value(self.0.get(key)?.clone()).ok()
    }

    /// The entry a reference points at, deserialized.
    pub(super) fn follow<T: DeserializeOwned>(&self, reference: &Reference) -> Option<T> {
        self.entry(&reference.key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A page with a snapshot carrying the given entries.
    fn page(entries: &str) -> String {
        format!(
            r#"<!DOCTYPE html><html><body>
<script id="__NEXT_DATA__" type="application/json">{{"props":{{"pageProps":{{"__APOLLO_STATE__":{{{entries}}}}}}},"page":"/x"}}</script>
</body></html>"#
        )
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct Thing {
        id: String,
        #[serde(default)]
        extra: Option<String>,
    }

    #[test]
    fn entries_are_read_by_key() {
        let apollo = Apollo::from_html(&page(
            r#""Thing:1":{"id":"1"},"Thing:2":{"id":"2","extra":"x"}"#,
        ))
        .expect("snapshot");

        assert!(apollo.contains("Thing:1"));
        assert!(!apollo.contains("Thing:3"));
        assert_eq!(
            apollo.entry::<Thing>("Thing:2"),
            Some(Thing {
                id: "2".into(),
                extra: Some("x".into())
            })
        );
        assert_eq!(apollo.entry::<Thing>("Thing:3"), None);
    }

    #[test]
    fn a_reference_is_followed() {
        let apollo = Apollo::from_html(&page(r#""A":{},"B":{"id":"b"}"#)).expect("snapshot");
        let reference = Reference {
            key: "B".to_string(),
        };

        assert_eq!(
            apollo.follow::<Thing>(&reference),
            Some(Thing {
                id: "b".into(),
                extra: None
            })
        );
    }

    #[test]
    fn an_entry_of_the_wrong_shape_is_absent_rather_than_a_panic() {
        let apollo = Apollo::from_html(&page(r#""Thing:1":{"id":42}"#)).expect("snapshot");
        assert_eq!(
            apollo.entry::<Thing>("Thing:1"),
            None,
            "a number where a string was expected must not be guessed at"
        );
    }

    #[test]
    fn a_page_without_a_snapshot_is_none() {
        assert!(Apollo::from_html("<html><body><p>nothing</p></body></html>").is_none());
    }

    #[test]
    fn a_snapshot_of_the_wrong_shape_is_none() {
        // The script is there, but the data has moved somewhere else.
        let moved = r#"<html><body><script id="__NEXT_DATA__" type="application/json">{"props":{"pageProps":{}}}</script></body></html>"#;
        assert!(Apollo::from_html(moved).is_none());

        let broken = r#"<html><body><script id="__NEXT_DATA__" type="application/json">{not json}</script></body></html>"#;
        assert!(Apollo::from_html(broken).is_none());
    }
}
