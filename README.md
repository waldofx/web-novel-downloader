# web-novel-downloader

Fetch web novels into a structured, style-preserving offline format.

This is **stage one** of a two-stage pipeline: the downloader normalizes a novel
into a documented intermediate format — preserving ruby/furigana, emphasis, and
scene breaks — and a future translation stage consumes that same format. Both
feed the EPUB / TXT / Markdown exporters.

## Status

Early, but **working end to end for Narou, Kakuyomu, Hameln and Pixiv**. `nvd
download` fetches a novel and its chapters into a structured directory, `nvd
update` fetches only what is new or changed, `nvd export` turns one into EPUB,
TXT or Markdown, and `nvd info` describes a directory you already have.
Illustrations a body references are recorded but not fetched yet (#18). Other
sources are next — see the
[roadmap](docs/engineering/architecture.md#roadmap).

| | |
| --- | --- |
| 📖 [docs/guide/](docs/guide/) | how it works, in plain language |
| 🧭 [docs/engineering/](docs/engineering/) | architecture, format, fetch policy, sources, testing |
| 🗺️ [docs/engineering/sources.md](docs/engineering/sources.md) | supported sites, in priority order |
| ⚠️ [docs/engineering/gotchas.md](docs/engineering/gotchas.md) | the surprises, so you don't rediscover them |
| 🤝 [CONTRIBUTING.md](CONTRIBUTING.md) · [AGENTS.md](AGENTS.md) | how work is done here |

## Usage

```bash
nvd download <novel-url> --out ./my-novel     # fetch a novel and its chapters
nvd download <url> --out ./d --format epub    # ...and produce an epub in one go
nvd info ./my-novel                           # what is in the directory
nvd export ./my-novel --format epub,txt,md    # epub, txt, or md
nvd update ./my-novel                         # only new or changed chapters
```

Run `nvd <command> --help` for the flags, or read
[getting started](docs/guide/getting-started.md).

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
