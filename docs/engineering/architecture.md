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
| `wnd-core` | `crates/core` | everything site-independent: the model (`store::model`), on-disk storage (`store::storage`), and the network layer (`net::fetch`, `net::policy`, `net::rate_limit`, `net::robots`) |
| `wnd-sources` | `crates/sources` | one adapter per site or site family, plus the registry that picks one |
| `wnd-download` | `crates/download` *(planned)* | orchestration: a novel URL in, a novel directory out |
| `nvd` | `crates/cli` | the command-line front end |

Planned later: `apps/gui` (Tauri) and `crates/translate`.

**Why the orchestrator gets its own crate.** Resolving a URL to an adapter needs
`wnd-sources`; `wnd-sources` produces types from `wnd-core`. Putting the
orchestrator inside `wnd-core` would close the cycle `core → sources → core`.
A third crate, depending on both, has no cycle.

**Why feature folders inside the crates.** A flat `src/` becomes a dumping
ground once a crate has more than a handful of modules. Modules are grouped by
feature (`core/src/net`, `core/src/store`), and a site adapter that outgrows one
file becomes a folder of one job per file (`sources/src/narou/{toc,chapter,inline,dom}.rs`).
The bar is a job, not a line count: `dom.rs` is small because it is one job.

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
    fn novel(&self, fetch: &HttpClient, url: &Url) -> Result<Manifest, SourceError>;
    fn chapter(&self, fetch: &HttpClient, meta: &ChapterMeta) -> Result<Chapter, SourceError>;
}
```

An earlier sketch had four calls (`novel`, `chapter_list`, `chapter`, and a
separate metadata call). The real one collapses metadata and the chapter list
into `novel`, because on every site we support a single table-of-contents fetch
carries both — and splitting them would mean fetching the same page twice.

Adapters receive the `HttpClient` rather than building their own, so the fetch
policy, rate limit, robots rules, and raw cache apply uniformly.

Adapters are deliberately small and isolated, because site HTML changes and
adapters rot. The [raw cache](fetch.md#the-raw-cache) is what makes a fix cheap:
the saved HTML can be re-parsed offline, without re-downloading anything.

## Data flow

```
                     ┌── Source::novel ──► Manifest (metadata + chapter index)
   URL ──find──► Source                                   │
                                                         ├─► manifest.json
                     ┌───────────────────────────────────┘
                     │
   for each missing chapter:
                     ├── Source::chapter ──► Chapter (node tree)
                     │                        ├─► chapters/NNNN.json
                     │                        └─► chapters/NNNN.html
                     └─► update manifest.json with the chapter's hash

   exporters:      chapters/*.json ──► EPUB / TXT / Markdown
                   (never touch the network)
```

The manifest is written **before** the chapters and updated after each one, so
an interrupted run leaves a directory that is valid and resumable rather than a
half-written lie. Resume compares recorded hashes instead of trusting the file
to exist; see [format.md](format.md#resume-and-update).

## Error handling

Two error types, each owning one layer:

- `wnd_core::Error` — network, robots, storage, and URL problems.
- `wnd_sources::SourceError` — adapter problems, with `Core(#[from] wnd_core::Error)`
  so a fetch failure inside an adapter keeps its original classification.

`SourceError::Parse { url, message }` means *the page loaded but its shape was
not what the adapter expects*: almost always the site changed its HTML. It is
deliberately distinct from a network failure, because only one of those is the
user's problem.

## Roadmap

| Milestone | Content | State |
| --- | --- | --- |
| **M0** | repo, design docs, source roadmap | done |
| **M1** | `core`: model, storage, fetch policy, rate limiter, robots | done |
| **M2** | **Narou** adapter (`ncode` + `novel18`), orchestrator, EPUB/TXT/MD export, CLI wiring | in progress |
| **M3** | **Kakuyomu** adapter | — |
| **M4** | **Hameln** (`syosetu.org`) + `h.syosetu.org` | — |
| **M5** | **Pixiv** (cookie-based auth) | — |
| **M6** | update tracking (fetch only new/changed chapters) | — |
| **M7** | other sources (Royal Road, Scribble Hub, AO3, FFN, …) | — |
| **M8** | CLI polish → Tauri GUI → translation stage | — |

## Open questions

- Translation stage: separate repo (planned) or the same workspace?
- Where do translations live — rewritten in place, or a sidecar
  (`0001.ja.json` + `0001.en.json`)? Leaning sidecar, so the original is never
  destroyed.
- EPUB ruby fallback for readers that do not support `<ruby>`.
- Policy for *gaiji*: characters the site renders as images rather than Unicode.
