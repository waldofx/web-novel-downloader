# On-disk and interchange format

A downloaded novel is a self-contained directory. This file is **normative**:
code comments are local truth, but where they disagree with this page, this page
is right and the code is a bug.

```
my-novel/
├── manifest.json     # novel metadata + chapter index + per-chapter hashes
├── chapters/
│   ├── 0001.json     # structured content — CANONICAL, what the translator consumes
│   └── 0001.html     # sanitized HTML rendering — style fidelity + debugging
├── raw/              # original fetched HTML (optional; enables offline re-parsing)
├── images/           # cover + illustrations
└── exports/          # generated EPUB/TXT/Markdown (regenerable, never committed)
```

JSON is canonical and HTML is derived: both come from one model, so they cannot
drift. Anything in `exports/` can be deleted and rebuilt.

## `manifest.json`

```json
{
  "format_version": 1,
  "source": { "id": "narou", "novel_url": "https://ncode.syosetu.com/n1234ab/" },
  "title": "…",
  "author": "…",
  "language": "ja",
  "content_rating": "general",
  "tags": ["…"],
  "synopsis": "…",
  "cover": "images/cover.jpg",
  "cover_url": "https://…",
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
  ]
}
```

`content_rating` is `general` or `adult`.

**Optional fields are omitted, never null.** `arc`, `published_at`, `revised_at`,
`author`, `synopsis`, `tags`, `cover` and `cover_url` are absent when the site does
not expose them. `hash` is absent until the chapter has been downloaded at least
once — an adapter cannot know a chapter's hash before fetching it, so the field is
`Option<String>` in the model and skipped when `None`.

### A URL is not a path

Two fields name a picture, and they are deliberately not the same kind of thing:

- `cover_url`, and a body image's `src` **as an adapter produces it**, are *where
  the source says the picture is*. An adapter can find an image; it cannot store
  one.
- `cover`, and the `src` a *stored* chapter holds, are a **path inside the novel
  directory**. Only the fetcher can put bytes on disk.

`nvd download` is what turns one into the other: it fetches the cover into
`images/cover.<ext>` and sets `cover`, and it fetches each body image into
`images/<chapter>-<n>.<ext>` and rewrites that chapter's `src` before the chapter
is written. An image that could not be fetched keeps its URL rather than being
dropped — a renderer falls back to the alt text, nothing about it is lost, and a
later run can try again.

Until that exists, an adapter's output is the half-way state on purpose:
`cover_url` is set and `cover` is not, and a chapter written to disk still holds
URLs. That is issue #18; it is why a novel downloaded today can have a cover URL
in its manifest and an empty `images/`.

`published_at` is when the chapter appeared and `revised_at` is when the site says
it was last revised — two facts, and conflating them would rewrite publication
history on every edit. Narou prints the revision time on revised rows only; a site
that publishes none states nothing, and `nvd update` treats "the index says
nothing" as "not fetched" unless `--check` is asked for it.

`format_version` is `FORMAT_VERSION` in `crates/core/src/store/model.rs`. It is
bumped **only** when the meaning of something above changes incompatibly, so an
older file is readable by definition and is read as a matter of course. A *newer*
one is refused: it may mean things this build does not. That is what lets an
additive optional field arrive without stranding every directory already on disk —
whereas bumping for one would make `nvd update` unable to read the very
directories it exists to update.

## Chapter JSON

A deliberately small document model — not raw HTML — so the translation stage
can walk the tree, replace each **text** node, and leave structure untouched:

```json
{
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
  ],
  "pre_note": [],
  "post_note": []
}
```

Node types stay few on purpose: `paragraph`, `text`, `ruby`, `emphasis`,
`strong`, `heading`, `scene_break`, `image`, `horizontal_rule`, `link`. Simple
enough to translate reliably, rich enough to preserve style.

## Invariants

These are the things that must stay true. Each one has cost a decision, and
each one is cheaper to preserve than to rediscover.

### Ruby is first-class and parsed at fetch time

Japanese web novels use furigana constantly. The exporters render it natively
(`<ruby>`/`<rt>`, with `<rp>` for readers that ignore ruby) or fall back to
`漢字(かんじ)` where the format has no ruby — see [export.md](export.md#ruby-the-one-hard-case).

Narou-family sites encode ruby **in plain text** — `｜漢字《かんじ》` — and use
`《》` for emphasis. Left unparsed, that notation leaks into translation and gets
mangled. So the adapter decodes it at fetch time, and the canonical form is
structured.

A `｜` or `《` with **no usable base stays literal text**. Dropping a character
the author typed is worse than showing a stray mark; the model is not allowed to
lose data to tidy itself up.

### Author notes are separate from the body

`pre_note` (`前書き`) and `post_note` (`後書き`) are their own arrays so they can
be kept, hidden, or translated independently of the story.

### Blank lines are normalized, not preserved verbatim

Sites use empty lines for both spacing and scene changes, so the run length is
meaning:

- a single blank line is a paragraph separator, and disappears;
- a run of two or more becomes one `scene_break`;
- leading and trailing blanks are dropped.

**Only ASCII whitespace is trimmed.** Narou indents paragraphs with U+3000, and
that indentation is content. `str::trim()` would eat it; `str::trim_ascii()`
does not. This rule is enforced by tests in `sources/src/narou/{dom,chapter}.rs`.

### Timestamps are RFC 3339 in a fixed offset

Narou prints `2020/04/01 23:23` in Japan Standard Time, which has no daylight
saving, so the offset is always `+09:00`. A date with no time is midnight.
Anything malformed is **rejected**, not guessed: substituting midnight for
`25:00` would invent a fact rather than admit the data is bad. No calendar
crate is involved.

### A missing text block is an error

A chapter page with no recognizable text container returns
`SourceError::NotTheSite`, not an empty chapter — and the fetch layer turns that
into a report of what actually arrived. Silence there would hide adapter rot, and
an empty chapter would be cached and never re-fetched.

## Resume and update

The manifest records a `sha256:` hash of each chapter's canonical JSON. That is
what makes resume and update cheap:

- Write `manifest.json` **immediately after the table of contents is parsed**,
  then rewrite it after each chapter. An interrupted run leaves a valid,
  resumable directory rather than a half-written lie.
- On the next run, **skip** a chapter only when all three hold: the stored
  manifest has a hash for that index, the file reads back, and hashing the parsed
  content reproduces that hash. Anything else — missing, truncated, corrupt, or
  edited — re-fetches. The comparison is over content, not bytes, so accidental
  whitespace is not mistaken for a change.
- `--force` ignores stored hashes and re-fetches everything.
- A stored manifest whose `source.novel_url` differs from the requested novel is
  ignored, not trusted.

### What `nvd update` treats as changed

An update is `nvd download` with a memory. The novel comes from the directory's
own `manifest.json` — an update is a thing done *to a directory*, so no URL is
given — and the index as it stands now is diffed against what the last run
recorded:

| The index now… | What happens | Why |
| --- | --- | --- |
| lists a chapter the directory has never seen | fetched | it is new |
| says a chapter was **revised** — `revised_at` differs from the recorded value, in either direction | fetched | the file is known to be older, and a stamp that *appears* where there was none is not evidence that the file is current |
| still lists a chapter whose file hashes to the recorded hash | not requested at all | nothing says otherwise |
| no longer lists a chapter | reported, and **nothing is deleted** | the user's copy is theirs; a grep of the output is a better record than a silent `rm` |
| says nothing about revisions (Kakuyomu publishes none) | not requested — unless `--check` is given | one request per chapter is a real cost, so it is opt-in |

Chapter identity is the **url**, never the number. Kakuyomu numbers episodes by
position and gives each an opaque id, so an episode inserted in the middle of a
work moves every later number without any of them becoming a different chapter. A
comparison by number would report those as changed — and a hash-only comparison
would do something worse: skip them, and leave the *previous* chapter's text under
every shifted number.

`--check` is the honest cover for a site that announces nothing: every chapter is
fetched, compared with what is on disk, and only the ones that really differ are
written. That is `chapters_checked` in the summary — fetched, compared, unchanged,
not written.

An update follows the index, which means a directory downloaded with
`--max-chapters` grows to the whole novel on its first update: the cap was a
download-time choice, and nothing on disk records that it was ever made.

## Storage mechanics

`crates/core/src/store/storage.rs` owns the filesystem, and `NovelDir` is the
only thing that knows the directory layout — nothing else builds these paths by
hand.

- `write_chapter` returns `Written { hash, bytes }`, so the caller records the
  hash and reports progress without reading the file back.
- `hash_json` is the one definition of a chapter hash: it hashes the serialized
  value, so equal content always produces an equal hash — which is what makes
  "is this file still what we fetched?" a single comparison.
- `write_json` is **atomic**: it writes a sibling `<name>.tmp` and renames it over
  the target. An interrupted write therefore leaves either the old file or the
  new one, never a truncated one. `fs::rename` replaces an existing file on
  Windows as well as Unix, so one implementation covers both, and a failed write
  removes its temporary file on the way out.
- JSON is pretty-printed on purpose: these files are meant to be diffed and read
  by humans, and they are the input to the translation stage.
