# web-novel-downloader

Fetch web novels into a structured, style-preserving offline format.

This is **stage one** of a two-stage pipeline: the downloader normalizes a novel into a documented intermediate format, and a future translation stage consumes that format — preserving markup such as ruby/furigana, emphasis, and scene breaks — before exporting to EPUB/TXT/Markdown.

## Status

Early. The design and source roadmap are settled; implementation starts with the `core` crate and the Narou adapter.

- 📐 [DESIGN.md](DESIGN.md) — architecture, data format, fetch policy, roadmap
- 🗺️ [docs/SOURCES.md](docs/SOURCES.md) — supported sites, priority order, adapter checklist

## Planned usage

```bash
# fetch a novel into a directory
nvd download <novel-url> --out ./my-novel

# export from the downloaded directory
nvd export ./my-novel --format epub

# update: fetch only new/changed chapters
nvd update ./my-novel
```

## Format at a glance

```
my-novel/
├── manifest.json     # metadata, chapter index, per-chapter hashes
├── chapters/
│   ├── 0001.json     # canonical structured content (translation-ready)
│   └── 0001.html     # sanitized HTML rendition
├── images/
└── exports/          # generated EPUB/TXT/Markdown
```

## Building

Requires a Rust toolchain (stable, MSVC target on Windows).

```bash
cargo build
cargo test
```

## License

Not decided yet.
