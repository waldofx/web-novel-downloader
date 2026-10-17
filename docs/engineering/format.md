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
  "fetched_at": "2026-10-17T00:00:00Z",
  "chapters": [
    {
      "index": 1,
      "title": "…",
      "url": "…",
      "arc": "第一章",
      "published_at": "2020-04-01T23:23:00+09:00",
      "hash": "sha256:…",
      "file": "chapters/0001.json"
    }
  ]
}
```

`content_rating` is `general` or `adult`.

**Optional fields are omitted, never null.** `arc`, `published_at`, `author`,
`synopsis`, `tags`, and `cover` are absent when the site does not expose them.
`hash` is absent until the chapter has been downloaded at least once — an
adapter cannot know a chapter's hash before fetching it, so the field is
`Option<String>` in the model and skipped when `None`.

`format_version` is `FORMAT_VERSION` in `crates/core/src/store/model.rs`, and it
is bumped when the meaning of anything above changes.

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

Japanese web novels use furigana constantly. EPUB renders it natively
(`<ruby>`/`<rt>`); TXT falls back to `漢字(かんじ)`.

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
`SourceError::Parse`, not an empty chapter. Silence there would hide adapter
rot, and an empty chapter would be cached and never re-fetched.

## Resume and update

> **Designed, not yet implemented.** This section is the contract the downloader
> (`crates/download`) is being written against; see [architecture.md](architecture.md#roadmap).

The manifest records a `sha256:` hash of each chapter's canonical JSON. That is
what makes resume and update cheap:

- Write `manifest.json` **immediately after the table of contents is parsed**,
  then rewrite it after each chapter. An interrupted run leaves a valid,
  resumable directory rather than a half-written lie.
- On the next run, **skip** a chapter only when all three hold: the stored
  manifest has a hash for that index, the file exists, and re-hashing the file
  gives the same hash. Anything else re-fetches.
- `--force` ignores stored hashes and re-fetches everything.
- A stored manifest whose `source.novel_url` differs from the requested novel is
  ignored, not trusted.

## Storage mechanics

`crates/core/src/store/storage.rs` owns the filesystem, and `NovelDir` is the
only thing that knows the directory layout — nothing else builds these paths by
hand.

- `write_chapter` returns the hash of what it wrote, so the caller can record it
  in the manifest without re-reading the file.
- `hash_json` is the one definition of a chapter hash: it hashes the serialized
  form, so the same content always yields the same hash.
- `write_json` is currently a plain write. It becomes **atomic** (write a
  sibling temporary file, then rename over the target) with the downloader:
  a crash mid-write must leave either the old file or the new one, never a
  truncated one. `fs::rename` replaces an existing file on Windows as well as
  Unix, so the same code works on both — see [gotchas](gotchas.md).
- JSON is pretty-printed on purpose: these files are meant to be diffed and read
  by humans, and they are the input to the translation stage.
