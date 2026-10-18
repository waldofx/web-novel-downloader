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

/// One novel as Pixiv's `/ajax/novel/{id}` answers it: the work's metadata, its
/// tags as objects, the series it belongs to (`seriesId` as a *number*), an
/// uploaded image, and a body holding two reader pages, furigana, a scene break
/// and an image reference that cannot be resolved.
pub(crate) const PIXIV_NOVEL_IN_SERIES: &str =
    include_str!("../tests/fixtures/pixiv/novel-in-series.json");

/// A novel in no series: `seriesNavData` is null, and the body carries the
/// author's own `[chapter:…]` dividers, a link, and two pages of one illust.
pub(crate) const PIXIV_NOVEL_STANDALONE: &str =
    include_str!("../tests/fixtures/pixiv/novel-standalone.json");

/// A series as `/ajax/novel/series/{id}` answers it: plain tag names,
/// `firstNovelId` as a number, and a cover in every size the site generates.
pub(crate) const PIXIV_SERIES: &str = include_str!("../tests/fixtures/pixiv/series.json");

/// One page of a series listing as `/ajax/novel/series_content/{id}` answers
/// it: newest-first, three novels whose `seriesContentOrder` is *not* their
/// reading order.
pub(crate) const PIXIV_SERIES_CONTENT: &str =
    include_str!("../tests/fixtures/pixiv/series-content.json");

/// One illustration as `/ajax/illust/{id}` answers it, with two pages.
pub(crate) const PIXIV_ILLUST: &str = include_str!("../tests/fixtures/pixiv/illust.json");

/// Pixiv's failure envelope: HTTP 200, `error: true`, and the site's own words.
pub(crate) const PIXIV_ERROR: &str = include_str!("../tests/fixtures/pixiv/error.json");
