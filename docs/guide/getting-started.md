# Getting started

## Status (October 2026)

**There is no working download command yet.** The engine is ready — the fetch
layer, the Narou adapter, and the download orchestrator are implemented and
tested — but the CLI that drives it and the exporters are still ahead. Progress
is tracked in the [roadmap](../engineering/architecture.md#roadmap).

What works today:

| Command | State |
| --- | --- |
| `nvd info <dir>` | works — reads a downloaded directory |
| `nvd download <url> --out <dir>` | the engine works and is tested; the command is not wired up yet |
| `nvd update <dir>` | not implemented yet |
| `nvd export <dir> --format epub` | not implemented yet |

## Build it

You need a [Rust toolchain](https://rustup.rs) — stable, and on Windows the
MSVC target.

```bash
git clone https://github.com/waldofx/web-novel-downloader
cd web-novel-downloader
cargo build --release
```

The binary lands at `target/release/nvd` (`nvd.exe` on Windows).

## What the commands will look like

```bash
# fetch a whole novel into ./my-novel
nvd download https://ncode.syosetu.com/n0608gd/ --out ./my-novel

# fetch an adult work (this is the opt-in; it is never implicit)
nvd download https://novel18.syosetu.com/n8309id/ --out ./my-novel --allow-adult

# see what a downloaded folder contains
nvd info ./my-novel

# fetch only new or changed chapters
nvd update ./my-novel

# produce a file for an e-reader
nvd export ./my-novel --format epub
```

## What lands on disk

```
my-novel/
├── manifest.json   # the novel's details and its chapter list
├── chapters/       # one file per chapter
├── images/
└── exports/        # generated files
```

Copy it, move it, back it up — it is self-contained. `exports/` is disposable
and always regenerable from `chapters/`.

The [format page](../engineering/format.md) describes all of it in detail.
