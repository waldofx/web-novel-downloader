# Architecture

## Vision

`web-novel-downloader` is **stage one** of a two-stage pipeline:

```
fetch  ──►  [translate]  ──►  read / export
(stage 1)      (stage 2)
```

- **Stage 1 (this repo):** fetch a novel from a web source and normalize it into
  a structured, style-preserving format.
- **Stage 2 (future, separate project):** translate that format with a local AI
  model while preserving markup, then export.

The design rule from the [index](README.md) is the contract: everything
downstream consumes the file format, and never re-scrapes a site.

## Goals

- Fetch whole novels from multiple sources, Japanese web novels first.
- **Preserve markup and styling**: ruby/furigana, emphasis, headings, scene
  breaks, illustrations, author's notes.
- Be polite and resilient: rate limiting, retries, resume after interruption,
  a raw-HTML cache.
- **Performance and a small footprint**: Rust, one binary, no runtime
  dependency, no daemon.
- One data format a translator can walk node by node without losing structure.
- Extensible per-site adapters.

## Non-goals (for now)

- Translation itself — a separate stage that shares the format.
- A GUI — later: Tauri, reusing the same core.
- Bypassing paywalls, logins, or access controls. Cookies for accounts *you own*
  are supported, for legitimate access only.
- Redistribution of downloaded content.

## Crates

A Cargo workspace, split so that each concern can be replaced independently.

| Crate | Path | Responsibility |
| --- | --- | --- |
| `wnd-core` | `crates/core` | everything site-independent: the model (`store::model`), on-disk storage (`store::storage`), and the network layer (`net::fetch`, `net::policy`, `net::rate_limit`, `net::robots`, `net::blocked`) |
| `wnd-sources` | `crates/sources` | one adapter per site or site family, plus the registry that picks one |
| `wnd-download` | `crates/download` | orchestration: a novel URL in, a novel directory out |
| `wnd-export` | `crates/export` | EPUB, TXT and Markdown, rendered from the chapters — see [export.md](export.md) |
| `nvd` | `crates/cli` | the command-line front end |

Planned later: `apps/gui` (Tauri) and `crates/translate`.

**Why the orchestrator gets its own crate.** Resolving a URL to an adapter needs
`wnd-sources`; `wnd-sources` produces types from `wnd-core`. Putting the
orchestrator inside `wnd-core` would close the cycle `core → sources → core`.
A third crate, depending on both, has no cycle.

**Why feature folders inside the crates.** A flat `src/` becomes a dumping
ground once a crate has more than a handful of modules. Modules are grouped by
feature (`core/src/net`, `core/src/store`), and a site adapter that outgrows one
file becomes a folder of one job per file (`sources/src/narou/{toc,chapter,inline}.rs`,
`sources/src/kakuyomu/{work,chapter,apollo}.rs`). The bar is a job, not a line
count: `narou/dom.rs` was small because it was one job — and when the second
adapter arrived it turned out to be a job *both* adapters have, so it moved up to
`sources/src/{dom,blocks,inline,selectors}.rs` and the adapters were left with
only what is peculiar to their site.

**Why the commonly used types are re-exported at the crate root.** Consumers
write `wnd_core::HttpClient`, not `wnd_core::net::fetch::HttpClient`. The
feature modules stay visible for the less common paths
(`wnd_core::store::storage::chapter_file_name`).

## The adapter interface

```rust
pub trait Source: Send + Sync {
    fn id(&self) -> &'static str;
    fn matches(&self, url: &Url) -> bool;
    fn content_rating(&self) -> ContentRating;
    fn image_referer(&self) -> Option<&'static str>;
    fn novel(&self, fetch: &HttpClient, url: &Url) -> Result<Manifest, SourceError>;
    fn chapter(&self, fetch: &HttpClient, meta: &ChapterMeta) -> Result<Chapter, SourceError>;
}
```

An earlier sketch had four calls (`novel`, `chapter_list`, `chapter`, and a
separate metadata call). The real one collapses metadata and the chapter list
into `novel`, because on every site we support a single table-of-contents fetch
carries both — and splitting them would mean fetching the same page twice.

`image_referer` is the one place a site is allowed to say that its pictures need
a header to come down, and only pixiv does. An adapter's job ends at *finding* a
picture — a cover URL, a `Block::Image` holding an absolute URL; the fetching and
the storing belong to `wnd-download`, which is why a chapter on disk holds a path
and an adapter's output holds a URL.

Adapters receive the `HttpClient` rather than building their own, so the fetch
policy, rate limit, robots rules, and raw cache apply uniformly. What comes back
is a [`Page`](fetch.md#the-evidence) — the body plus the status, media type and
final URL — because a body on its own cannot say who answered. When an adapter
says the markup it needs is not there, the page it holds becomes the evidence in
the error, so a filter page or a bot challenge is reported as what it is instead
of as a parse failure.

Adapters are deliberately small and isolated, because site HTML changes and
adapters rot. The [raw cache](fetch.md#the-raw-cache) is what makes a fix cheap:
the saved HTML can be re-parsed offline, without re-downloading anything.

## Data flow

```
                     ┌── Source::novel ──► Manifest (metadata + chapter index)
   URL ──find──► Source                                   │
                                                          ├─► manifest.json
                     ┌────────────────────────────────────┘
                     │
   for each chapter still missing:
                     ├── Source::chapter ──► Chapter (node tree)
                     │                        └─► chapters/NNNN.json
                     └─► rewrite manifest.json with the chapter's hash

   exporters:      chapters/*.json ──► EPUB / TXT / Markdown
                   (never touch the network)
```

`crates/download` owns that sequence, and four properties of it are deliberate:

1. **Nothing is written until the table of contents succeeds**, so a bad URL or a
   dead site leaves no directory behind at all.
2. **Nothing is gated on rating**: the work's rating is recorded but never
   refused — each site enforces its own age gate.
3. **The plan is computed before the first request**, so "2 of 89" is known up
   front rather than discovered as it goes.
4. **The manifest is durable before the first chapter and rewritten after each
   one.** A failure mid-novel therefore leaves a valid, resumable directory: the
   chapters already fetched keep their hashes and are never fetched again.

`nvd update` is that same sequence with a memory. It reads the novel's URL out of
the directory's own manifest, re-reads the index, and diffs:

```
   manifest.json ──► stored chapter urls, hashes, revision stamps
        │
   Source::novel ──► the index as it stands now
        │
        ├─ new, or revised, or (--check) differing ──► fetch, write, re-hash
        ├─ still listed and still hashing the same  ──► not requested
        └─ gone from the index                      ──► reported, never deleted
```

Identity is compared by chapter **url**, not by number: Kakuyomu numbers episodes
by position, so a chapter inserted in the middle moves every later number without
any of them becoming a different chapter. The two properties worth remembering are
that an update **never deletes** what the user already has, and that it never
refuses anything on rating — each site enforces its own age gate, and the tool
does not bypass it.

Resume compares recorded hashes rather than trusting a file to exist — existence
alone would accept a truncated or hand-edited chapter. The rule is in
[format.md](format.md#resume-and-update).

## Error handling

Two error types, each owning one layer:

- `wnd_core::Error` — network, robots, storage, and URL problems.
- `wnd_sources::SourceError` — adapter problems, with `Core(#[from] wnd_core::Error)`
  so a fetch failure inside an adapter keeps its original classification.

`SourceError::Parse { url, message }` means *the page loaded, and it is this
site's page, but something in it was not what the adapter expects*: almost always
the site changed its HTML. It is deliberately distinct from a network failure,
because only one of those is the user's problem.

`SourceError::NotTheSite { url, expected }` means *the shape the adapter needs is
simply not there*. The adapter names what it was looking for, and
`SourceError::with_evidence` — called where the page and the client are both in
hand — turns it into `wnd_core::Error::Blocked`: the status, the size, the media
type, the readable head of the body, and whatever the fetch layer could prove
about the host. A filter page and a markup change look the same from here, so the
message shows the evidence instead of picking one. See
[fetch.md](fetch.md#the-evidence).

## Roadmap

| Milestone | Content | State |
| --- | --- | --- |
| **M0** | repo, design docs, source roadmap | done |
| **M1** | `core`: model, storage, fetch policy, rate limiter, robots | done |
| **M2** | **Narou** adapter (`ncode` + `novel18`), orchestrator, EPUB/TXT/MD export, CLI wiring | done — `nvd download` and `nvd export` work end to end |
| **M3** | **Kakuyomu** adapter | done — snapshot-based TOC (HTML alone lists one chapter of a thousand), real arcs, real `<ruby>` |
| **M3b** | **Sessions** — a user-owned cookie store for challenged sites (#31) | done — `nvd session save/list/delete`, `--session` on `download` and `update`; Netscape `cookies.txt` import; per-host, never logged |
| **M4** | **Hameln** (`syosetu.org`) + `h.syosetu.org` | done — adapter merged; `h.` markup sharing assumed, not verified |
| **M5** | **Pixiv** (`pixiv.net/novel`), read from the site's JSON API (#20) | done — any URL into a work downloads the work (a series URL and a novel URL inside it both give the series); a novel's reader pages are joined rather than cut short; `[[rb:]]`, `[[jumpuri:]]`, `[chapter:]` and `[uploadedimage:]` are decoded; `[pixivimage:]` is best-effort |
| **M5b** | **Illustrations** — fetch and store the images a body references (#18) | done — an adapter leaves a picture as a URL and the downloader fetches it into `images/` as `cover.<ext>` or `<chapter>-<n>.<ext>`, records the URL and a hash in `manifest.images`, and rewrites the chapter to the path; a picture that cannot be fetched is reported and keeps its URL; pixiv's `i.pximg.net` is reached with the referer it demands |
| **M6** | update tracking (fetch only new/changed chapters) | done — `nvd update` diffs the index against the recorded manifest; `--check` compares content where the site announces nothing |
| **M7** | other sources (Royal Road, Scribble Hub, AO3, FFN, …) | — |
| **M8** | CLI polish → Tauri GUI → translation stage | — |

## Open questions

- Translation stage: separate repo (planned) or the same workspace?
- Where do translations live — rewritten in place, or a sidecar
  (`0001.ja.json` + `0001.en.json`)? Leaning sidecar, so the original is never
  destroyed.
- EPUB ruby fallback for readers that do not support `<ruby>`: `<rp>` is emitted,
  but whether any target reader ignores it is untested against real devices.
- Policy for *gaiji*: characters the site renders as images rather than Unicode.
