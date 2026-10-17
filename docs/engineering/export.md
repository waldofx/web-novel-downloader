# Exports

The downloader writes a novel directory; the exporters turn it into something you
can read. They live in `crates/export` (`wnd-export`), which depends only on
`wnd-core` — an exporter never touches the network, and never invents content.

## The rule that shapes everything here

**An export is a projection.** `chapters/*.json` is the source of truth; `exports/`
is disposable and can be deleted and rebuilt at any time. That has two
consequences worth stating:

- No exporter may be the only place a piece of information exists. If a fact
  matters, it is in the JSON.
- An exporter may *lose* something the format cannot express (Markdown has no
  furigana), but it must never *invent* something the JSON does not hold.

Where the directory itself is concerned, see [format.md](format.md).

## Format selection

| Format | What it is for | What it keeps |
| --- | --- | --- |
| EPUB 3 | e-readers | everything: native `<ruby>`, emphasis, links, images, notes |
| TXT | grepping, pasting, diffing | the words, in reading order, plus image placeholders |
| Markdown | GitHub, an editor, a wiki | headings, emphasis, links, images; furigana as text |

## Determinism

Exporting the same directory twice must produce **byte-identical** files. This is
a requirement, not a nicety: it means a re-export diffs cleanly, a generated EPUB
can be committed or hashed, and a bug report can be reproduced.

Three things would break it, and all three are deliberately avoided:

- **The clock.** Nothing reads it. The EPUB's `dcterms:modified` comes from
  `manifest.fetched_at` (what the fetcher recorded) and is omitted if the
  manifest has none.
- **ZIP entry timestamps.** The ZIP format stores a modification time per entry,
  so every entry is written with a fixed `DateTime::DEFAULT` (1980-01-01) rather
  than "now". `zip` only reads the clock when its `time` feature is on, and this
  crate does not enable it — but the code sets the timestamp explicitly anyway, so
  turning that feature on later cannot silently break this.
- **Unordered iteration.** The image list is sorted and de-duplicated before it
  is written, so the archive does not depend on the order a `HashMap` happened to
  give.

## File naming

The file is named after the novel's title, because that is what a human
recognises in a file manager:

```
exports/二重勇者はすごいです！.epub
```

A title is free text and a file name is not, so the characters Windows forbids —
`\ / : * ? " < > |` and control characters — become `_`, and a trailing dot or
space is trimmed (Windows strips those silently, which would make the written file
differ from the reported path). An 80-character cap keeps the result usable. The
ideographic space `　` is **not** touched: it is content, not formatting.

If nothing usable survives — a title of pure punctuation, say — the name falls
back to the adapter id. The name is a pure function of the manifest, so the same
novel never scatters renamed copies around the directory.

## Ruby, the one hard case

Furigana is the reason the format is structured rather than a string of text, so
each exporter has to decide what to do with it.

| Format | Rendering |
| --- | --- |
| EPUB | `<ruby>漢字<rp>(</rp><rt>かんじ</rt><rp>)</rp></ruby>` |
| TXT, Markdown | `漢字(かんじ)`, or `漢字` with `RubyStyle::BaseOnly` |

The `<rp>` element is the EPUB answer to readers that do not render ruby at all:
without it, such a reader shows the reading run straight into the base text
(`漢字かんじ`). With it, it shows `漢字(かんじ)`.

The plain-text fallback uses parentheses rather than the source's
`｜漢字《かんじ》`: these formats are read as prose, and the notation a site
happened to use is an implementation detail of that site.

A ruby node with an **empty reading** has no `<ruby>` at all — it is just the
base word. Emitting an empty `<rt>` would leave a hole in the text.

## Language-dependent wording

Four strings depend on the novel's language, and they live in one table
(`src/words.rs`) so they cannot drift apart:

| | `ja` | anything else |
| --- | --- | --- |
| table of contents | 目次 | Contents |
| note before the chapter | 前書き | Author's note |
| note after the chapter | 後書き | Afterword |
| scene break, in text | `＊　＊　＊` | `* * *` |

This is a formatting decision, not a translation system. The EPUB does not use the
scene-break string at all: it draws a rule (`<hr class="scene-break" />`), which is
better than any text marker when the format can do it.

## TXT

Structure is carried by whitespace, because there is nothing else to carry it:

```
<title>
<author>

<synopsis>

<blank>
<blank>

【arc】

<chapter title>

［前書き］
<note>

<paragraph>
<blank>
＊　＊　＊
<blank>
<paragraph>

［後書き］
<note>
```

- Two blank lines **between chapters**, one blank line between a chapter title and
  its body. The chapter break is the bigger break on purpose.
- An arc heading (`【…】`) is printed once, above the first chapter of the arc.
  Repeating it on every chapter is how a site's navigation looks, not how a book
  does.
- A paragraph is one line. A run of two or more blank lines in the source became a
  `SceneBreak` at fetch time, and gets blank lines around its marker.
- An image becomes `[画像: alt]` (falling back to the file name when the source
  gave no alt text). The image cannot be shown, but its existence should not be
  silently dropped.
- The document ends with exactly one newline.

## Markdown

Markdown is written as Markdown where the format allows it, and as prose where it
does not:

```markdown
# <title>

*<author>*

<synopsis>

## <arc>

### <chapter title>

> **前書き**
>
> <note>

<paragraph>

* * *

<paragraph>
```

- The notes become blockquotes, which is how a renderer sets an aside apart.
- Emphasis, strong text, links and images use real Markdown.
- **Image paths are rewritten** to `../images/…`. The model's `src` is relative to
  the novel directory, but the Markdown file lives in `exports/`, so the same
  relative path would point at nothing.
- Escaping is deliberately short — `\ * _ ` ` [ ]` inside text, plus a leading `#`
  or `>` — because this is prose. Backslash-escaping Japanese punctuation would
  make the file worse to read, not better. It is not a complete CommonMark
  escaper, and it is not trying to be.

## EPUB

An EPUB is a ZIP with rules. In the order they are written:

| Entry | Why |
| --- | --- |
| `mimetype` | first, and **stored** (not deflated), containing exactly `application/epub+zip` — the specification requires both so a reader can identify the file from its first bytes |
| `META-INF/container.xml` | says where the package document is |
| `OEBPS/style.css` | one small stylesheet, embedded rather than linked |
| `OEBPS/images/…` | the images, **copied byte for byte** — never re-encoded |
| `OEBPS/text/NNNN.xhtml` | one document per chapter, numbered from the chapter index |
| `OEBPS/nav.xhtml` | the navigation document (the table of contents), with chapters nested under their arcs |
| `OEBPS/content.opf` | the package: metadata, manifest, spine |

Decisions inside the package document:

- **EPUB 3** (`version="3.0"`) with a navigation document. No `toc.ncx` is emitted:
  that is the EPUB 2 mechanism, and every reader worth targeting reads the EPUB 3
  navigation document. Adding one back is a deliberate decision, not an oversight.
- The **identifier is the novel's own URL** (`id="novel-url"`). It is stable,
  unique, and it is what a reader would need to find the work again.
- The **cover image carries `properties="cover-image"`**, which is how a reading
  system knows to show it. Only declared when the archive actually contains the
  file — naming a file a reader cannot find is worse than omitting the cover.
- A chapter's **notes are in the same document** as its body, as
  `<aside epub:type="preface">` / `<aside epub:type="afterword">`. A reader can
  style them apart, and they stay attached to the chapter they belong to.
- Paragraph line breaks (`<br />`): a synopsis keeps its line structure as
  newlines, and a newline is whitespace in XHTML. Without this, lines would run
  together.
- **Arcs group chapters**, in the navigation and in the text. A chapter that
  belongs to an arc is nested inside it in `nav.xhtml`: the arc is a list item
  whose heading is a `<span>` — there is no page to link to, it is a divider —
  and its chapters are the nested `<ol>`. A chapter in no arc stays a direct
  child, so a novel without arcs exports exactly as it did before arcs existed.
  In the chapter document the arc is a `<p class="arc">` divider, printed on the
  first chapter of the arc and **only** there: the same rule TXT and Markdown
  follow, for the same reason. A chapter document cannot work that out alone, so
  `epub::write` narrows the divider while it walks the chapters in reading order.

### Images

Images referenced by the index or by any chapter are collected, sorted,
de-duplicated, and copied into `OEBPS/images/`. The relative `src` in the XHTML is
prefixed with `../` because the chapter document lives one level down in `text/`.

A referenced image that is **not on disk is skipped**, and so is its manifest
entry. This is deliberate for now: the downloader does not fetch images yet (see
the [roadmap](architecture.md#roadmap)), so treating a missing file as fatal would
make the exporter refuse to run on ordinary novels. When image fetching lands, a
missing image becomes a fetch failure, and this can be revisited.

## Notes, and why they are on by default

`ExportOptions::include_notes` defaults to **true**. The 前書き and 後書き are the
author's own words about the chapter — part of the work, not an annotation about
it. Turning them off is available, and it removes the note's text as well as its
label.

## Deliberately not done

- **No cover page.** The cover image is declared with `properties="cover-image"`;
  a separate XHTML page with the image in it is a workaround for readers that
  ignore that property, and nothing needs it yet.
- **No footnote linking.** A link is kept as a link; turning 前書き references into
  EPUB footnotes would require understanding the prose.
- **No re-encoding of images, and no CSS beyond one stylesheet.** An exporter that
  rewrites assets is an exporter that can corrupt them.
- **No textual footer or generation timestamp.** It would be content that is not
  in the JSON, and it would break determinism.
