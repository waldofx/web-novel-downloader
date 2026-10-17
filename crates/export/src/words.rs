//! The few strings whose wording depends on the novel's language.
//!
//! They live in one table so the plain-text markers, the EPUB navigation title,
//! and the note labels cannot drift apart, and so adding a language means adding
//! one arm instead of hunting through three renderers.

/// Language-dependent wording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Words {
    /// Title of the generated table of contents.
    pub contents: &'static str,
    /// Label for 前書き, the note before the chapter.
    pub preface: &'static str,
    /// Label for 後書き, the note after the chapter.
    pub afterword: &'static str,
    /// How a scene break is written when the format cannot draw one. Formats
    /// that can draw a rule (EPUB) ignore it.
    pub scene_break: &'static str,
}

impl Words {
    /// The wording for a BCP 47 language tag.
    ///
    /// Japanese gets the markers a Japanese book uses; anything else gets the
    /// English ones. That is a deliberate choice rather than a translation
    /// system: this is a formatting decision, not content.
    pub fn for_language(language: &str) -> Self {
        if language.starts_with("ja") {
            Self {
                contents: "目次",
                preface: "前書き",
                afterword: "後書き",
                scene_break: "＊　＊　＊",
            }
        } else {
            Self {
                contents: "Contents",
                preface: "Author's note",
                afterword: "Afterword",
                scene_break: "* * *",
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn japanese_gets_the_markers_a_japanese_book_uses() {
        let words = Words::for_language("ja");
        assert_eq!(words.contents, "目次");
        assert_eq!(words.scene_break, "＊　＊　＊");
        // A regional tag is still Japanese.
        assert_eq!(Words::for_language("ja-JP"), words);
    }

    #[test]
    fn anything_else_falls_back_to_english() {
        let words = Words::for_language("en");
        assert_eq!(words.contents, "Contents");
        assert_eq!(words.scene_break, "* * *");
        assert_eq!(Words::for_language(""), words);
    }
}
