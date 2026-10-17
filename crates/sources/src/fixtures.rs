//! Captured HTML, compiled into the test binary.
//!
//! Fixtures live as files under `tests/fixtures/` so they can be read, diffed,
//! and replaced with a fresh capture without editing Rust. They are pulled in
//! with `include_str!`, which resolves at compile time — a test therefore never
//! depends on the working directory it is run from.

/// A table-of-contents page: an arc heading, chapter rows, published dates (one
/// with a revision note), a pager, and an ideographic space in the title that
/// trimming must not eat.
pub(crate) const NAROU_TOC: &str = include_str!("../tests/fixtures/narou/toc.html");

/// A chapter page: preface, body with blank runs, HTML ruby, both forms of
/// plain-text ruby notation, emphasis, and an afterword.
pub(crate) const NAROU_CHAPTER: &str = include_str!("../tests/fixtures/narou/chapter.html");

/// A short story: no chapter rows at all, the whole body rendered inline on the
/// table-of-contents page.
pub(crate) const NAROU_SHORT_STORY: &str = include_str!("../tests/fixtures/narou/short-story.html");
