# web-novel-downloader

Fetch web novels into a structured, style-preserving offline format.

This is **stage one** of a two-stage pipeline: the downloader normalizes a novel
into a documented intermediate format — preserving ruby/furigana, emphasis, and
scene breaks — and a future translation stage consumes that same format. Both
feed the EPUB / TXT / Markdown exporters.

## Status

Early, and **not usable yet**. The fetch layer, the Narou adapter, the download
orchestrator and the exporters are implemented and tested; only the CLI wiring is
left. `nvd info` is the one command that does anything today.

| | |
| --- | --- |
| 📖 [docs/guide/](docs/guide/) | how it works, in plain language |
| 🧭 [docs/engineering/](docs/engineering/) | architecture, format, fetch policy, sources, testing |
| 🗺️ [docs/engineering/sources.md](docs/engineering/sources.md) | supported sites, in priority order |
| ⚠️ [docs/engineering/gotchas.md](docs/engineering/gotchas.md) | the surprises, so you don't rediscover them |
| 🤝 [CONTRIBUTING.md](CONTRIBUTING.md) · [AGENTS.md](AGENTS.md) | how work is done here |

## Usage

```bash
nvd download <novel-url> --out ./my-novel   # fetch a whole novel
nvd info ./my-novel                         # what is in the directory
nvd update ./my-novel                       # only new or changed chapters
nvd export ./my-novel --format epub         # epub, txt, or md
```

Only `info` works so far — see [getting started](docs/guide/getting-started.md).

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

The JSON is canonical; everything else is derived and regenerable. The
[format spec](docs/engineering/format.md) is normative.

## Building

Requires a Rust toolchain (stable; the MSVC target on Windows).

```bash
cargo build --release
cargo test
```

## License

Not decided yet.
