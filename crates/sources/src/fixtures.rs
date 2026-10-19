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

/// A chapter with an illustration in its body, trimmed from a real capture to
/// the lines around it: the picture is a paragraph of its own, wrapped in an
/// `<a>` that points at the same picture, and its `src` is **protocol-relative**
/// (`//img.example.test/…`) — which is why an adapter has to resolve it against
/// the page rather than pass it on.
pub(crate) const NAROU_CHAPTER_ILLUSTRATED: &str =
    include_str!("../tests/fixtures/narou/chapter-illustrated.html");

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

/// A Hameln work page: title, author, tags, a synopsis, six arcs, and chapter
/// rows with published dates and a revision stamp.
///
/// Trimmed to the containers the parser reads, with twelve rows: the site puts
/// its whole list on one page, and that is a fact about the site, not something
/// a dozen rows could show either way. The words are placeholders (#41).
pub(crate) const HAMELN_WORK: &str = include_str!("../tests/fixtures/hameln/work.html");

/// A Hameln chapter page: a title and a body in `#honbun`, with `<ruby>`.
///
/// Both `font-size:120%` spans are kept in the order the site writes them, so
/// the fixture answers the way a real page answers (#51); the prose is a
/// placeholder (#41).
pub(crate) const HAMELN_CHAPTER: &str = include_str!("../tests/fixtures/hameln/chapter.html");

/// A Hameln **body** with illustrations in it, trimmed from a real chapter capture
/// kept in the gitignored `captures/` to the lines around the pictures.
///
/// The shape is the site's; the words are not. The prose and the picture addresses
/// are placeholders, because a committed fixture should carry neither an author's
/// text nor a working link to somebody's illustration (#41).
///
/// Four things here are Hameln's and only Hameln's, which is why this is a capture
/// rather than a guess: the `alt` sits on the **`<a>`** rather than on the
/// `<img>`, that `<a>` carries `name="img"`, an illustration sits alone in its own
/// `<p id="N">` — a Hameln body is one `<p>` per line, not a synopsis' `<br>` runs
/// — and the site writes its separator as `<p id="N"></p><hr><p></p>`, empty
/// paragraphs around a rule.
///
/// The capture is a browser save, which had already rewritten every `src` to a
/// local `_files/` copy while leaving the `<a href>` alone. Both were due to be
/// replaced by placeholders anyway, so nothing was lost either way.
pub(crate) const HAMELN_CHAPTER_ILLUSTRATED: &str =
    include_str!("../tests/fixtures/hameln/chapter-illustrated.html");

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
