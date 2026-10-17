# Design

## Vision

`web-novel-downloader` is **stage one** of a two-stage pipeline:

```
fetch  ──►  [translate]  ──►  read / export
(stage 1)     (stage 2)
```

- **Stage 1 (this repo):** fetch a novel from a web source and normalize it into a structured, style-preserving intermediate format.
- **Stage 2 (future, separate repo/app):** translate that format with a local AI model while preserving markup, then export.

**Core design rule:** the downloader's output is a stable, documented interchange format. The translation stage and every exporter consume that format. Nothing downstream should ever need to re-scrape a site.

## Goals

- Fetch whole novels from multiple sources (Japanese web novels first).
- **Preserve markup and styling**: ruby/furigana, emphasis, headings, scene breaks, illustrations, author notes.
- Be polite and resilient: rate limiting, retries, resume-after-interruption, raw HTML caching.
- **Performance and a lightweight footprint**: Rust core, single binary, no runtime dependency.
- One data format a translation stage can transform node-by-node without losing structure.
- Extensible per-site source adapters.

## Non-goals (for now)

- Translation itself (separate stage, shares the format).
- A GUI (later: Tauri, reusing the same core).
- Bypassing paywalls, logins, or access controls. (Login *cookies for accounts you own* are supported for legitimate access only.)
- Redistribution of downloaded content.

## Architecture

A Cargo workspace, split so each concern can be replaced independently:

```
crates/
  core/       # data model, fetch policy, rate limiting, storage, exporters
  sources/    # site adapters, one module per site or site-family, plus registry
  cli/        # the `nvd` binary
later:
  apps/gui/          # Tauri desktop app
  crates/translate/  # local-model translation stage
```

Layers: **sources** (site-specific: URL → structured chapter) sit behind **core** (fetch policy, orchestration, storage, export), which the **cli** drives.

Adapter interface (sketch):

```rust
pub trait Source {
    fn id(&self) -> &'static str;
    fn matches(&self, url: &Url) -> bool;
    fn novel(&self, url: &Url) -> Result<NovelMetadata>;
    fn chapter_list(&self, url: &Url) -> Result<Vec<ChapterRef>>;
    fn chapter(&self, r: &ChapterRef) -> Result<Chapter>;
    fn content_rating(&self) -> ContentRating;
}
```

Adapters are intentionally small and isolated, because site HTML changes and adapters rot. The raw-fetch cache lets a broken adapter be fixed and re-run **without re-downloading**.

## On-disk format (the important part)

A downloaded novel is a self-contained directory:

```
my-novel/
├── manifest.json     # novel metadata + chapter index + per-chapter hashes
├── chapters/
│   ├── 0001.json     # structured content — CANONICAL, what the translator consumes
│   └── 0001.html     # sanitized HTML rendering — style fidelity + debugging
├── raw/              # original fetched HTML (optional; enables offline re-parsing)
├── images/           # cover + illustrations
└── exports/          # generated EPUB/TXT/Markdown (regenerable, not committed)
```

### `manifest.json`

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
      "published_at": "…",
      "hash": "sha256:…",
      "file": "chapters/0001.json"
    }
  ]
}
```

`content_rating` is `general` or `adult`. Per-chapter `hash` is what makes **update tracking** cheap: only changed or new chapters are re-fetched.

### Chapter JSON

A deliberately small document model — not raw HTML — so the translation stage can walk the tree, replace each **text** node, and leave structure untouched:

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

Node types stay few on purpose: `paragraph`, `text`, `ruby`, `emphasis`, `strong`, `heading`, `scene_break`, `image`, `horizontal_rule`, `link`. Simple enough to translate reliably, rich enough to preserve style.

Key decisions:

- **Ruby is first-class**, because Japanese web novels use it constantly. EPUB renders it natively (`<ruby>`/`<rt>`); TXT can render `漢字(かんじ)`.
- **Source-specific notation is parsed at fetch time.** Narou-family sites encode ruby *in plain text* (`｜漢字《かんじ》`) and use `《》`/`（）` as markup. Left unparsed, this notation leaks into translation and gets mangled. The parsed structure is canonical, and the untouched original text can be retained alongside it.
- **Author notes** (`前書き`/`後書き`) are kept separate from the chapter body so they can be translated (or not) independently.
- **JSON is canonical, HTML is derived.** HTML is what EPUB and readers want; JSON is what the translator wants. Both come from one model, so they cannot drift.

## Fetch policy

- **Concurrency:** bounded (default 2–3 in flight, configurable).
- **Politeness:** minimum delay per host (default ≥1s), honor `Retry-After`.
- **Retries:** exponential backoff on 429/5xx/network errors.
- **Resume:** skip chapters already present whose hash matches.
- **Raw cache:** keep original HTML so adapter fixes don't require re-fetching.
- **Identification:** a descriptive User-Agent.
- **Cookies:** per-domain store, **gitignored**, for login-gated sources (e.g. Pixiv). Never committed.
- **Stance:** respect `robots.txt` and each site's ToS; no paywall circumvention; personal offline reading.

## Exporters

Derived from the canonical JSON, and always regenerable:

- **EPUB** — chapters → XHTML, ruby → `<ruby>/<rt>`, metadata and cover, proper nav/TOC.
- **TXT** — plain text, configurable ruby rendering.
- **Markdown** — headings/emphasis with a ruby fallback.

## Roadmap

| Milestone | Content |
| --- | --- |
| **M0** | Repo, design docs, source roadmap ← *this* |
| **M1** | `core`: data model, storage, fetch policy, rate limiter |
| **M2** | **Narou** adapter (`ncode.syosetu.com`) + `novel18` (same platform) + EPUB/TXT export |
| **M3** | **Kakuyomu** adapter |
| **M4** | **Hameln** (`syosetu.org`) + `h.syosetu.org` |
| **M5** | **Pixiv** (cookie-based auth) |
| **M6** | Update tracking (fetch only new/changed chapters) |
| **M7** | Other sources (Royal Road, Scribble Hub, AO3, FFN, …) |
| **M8** | CLI polish → Tauri GUI → translation stage |

## Open questions

- Translation stage: separate repo (planned) or same workspace?
- Where do translations live — rewritten in place, or sidecar (`0001.ja.json` + `0001.en.json`)? Leaning sidecar so the original is never destroyed.
- EPUB ruby fallback for readers that don't support `<ruby>`.
- Policy for *gaiji* (characters rendered as images instead of Unicode).
