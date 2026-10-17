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

/// A Kakuyomu work page grouped into arcs (章): two chapters of two episodes
/// each, the author behind a reference, a catchphrase and content flags the
/// adapter deliberately ignores, and a cover the page does not have.
pub(crate) const KAKUYOMU_WORK_ARCS: &str =
    include_str!("../tests/fixtures/kakuyomu/work-arcs.html");

/// A Kakuyomu work page with no arcs: one unnamed group whose snapshot key has
/// an empty id (`TableOfContentsChapter:`).
pub(crate) const KAKUYOMU_WORK_FLAT: &str =
    include_str!("../tests/fixtures/kakuyomu/work-flat.html");

/// A Kakuyomu episode page: the body container, one `<br>` per blank line, a
/// blank line holding only an ideographic space, `<ruby>` with `<rb>`/`<rp>`,
/// and — around the container — the header, share buttons and ad slot that must
/// not be read as story text.
pub(crate) const KAKUYOMU_CHAPTER: &str = include_str!("../tests/fixtures/kakuyomu/chapter.html");

/// A Hameln work page: title, author, tags, a synopsis, arc headings, and
/// chapter rows with published dates and a revision stamp.
pub(crate) const HAMELN_WORK: &str = include_str!("../tests/fixtures/hameln/work.html");

/// A Hameln chapter page: a title and a body in `#honbun`, with `<ruby>`.
pub(crate) const HAMELN_CHAPTER: &str = include_str!("../tests/fixtures/hameln/chapter.html");
