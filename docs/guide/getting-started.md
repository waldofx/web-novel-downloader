# Getting started

## Status (October 2026)

**Working end to end for Narou.** `nvd download` fetches a novel and its chapters
into a structured directory, `nvd export` turns one into an EPUB, a text file or
Markdown, and `nvd info` describes a directory you already have. `nvd update` is
not implemented yet, and other sites come next — see the
[roadmap](../engineering/architecture.md#roadmap).

| Command | State |
| --- | --- |
| `nvd download <url> --out <dir>` | works |
| `nvd export <dir> --format epub` | works |
| `nvd info <dir>` | works |
| `nvd update <dir>` | not implemented yet |

## Build it

You need a [Rust toolchain](https://rustup.rs) — stable, and on Windows the
MSVC target.

```bash
git clone https://github.com/waldofx/web-novel-downloader
cd web-novel-downloader
cargo build --release
```

The binary lands at `target/release/nvd` (`nvd.exe` on Windows).

## Running it

```bash
# fetch a whole novel into ./my-novel
nvd download https://ncode.syosetu.com/n0608gd/ --out ./my-novel

# Kakuyomu works too — any of its URLs, including the chapter you are reading
nvd download https://kakuyomu.jp/works/16816452221336015212 --out ./my-novel

# fetch it and produce a file for an e-reader in the same step
nvd download https://ncode.syosetu.com/n0608gd/ --out ./my-novel --format epub,txt

# fetch an adult work (this is the opt-in; it is never implicit)
nvd download https://novel18.syosetu.com/n8309id/ --out ./my-novel --allow-adult

# see what a downloaded folder contains
nvd info ./my-novel

# produce a file for an e-reader from a folder you already have
nvd export ./my-novel --format epub,md

# fetch only new or changed chapters
nvd update ./my-novel
```

Flags for `download` (`nvd download --help` lists them all):

| Flag | What it does |
| --- | --- |
| `--out`, `-o` | where to write. Required |
| `--format` | also export: `epub`, `txt` or `md`, repeatable or comma-separated |
| `--allow-adult` | allow adult works, which are otherwise refused before anything is written |
| `--max-chapters` | stop after this many chapters, counting from the first |
| `--force` | re-fetch every chapter, ignoring what is already on disk |
| `--no-robots` | do not consult `robots.txt`. You almost certainly do not want this |

There is no `--resume` flag because resuming is what happens by default: run the
same command again and it fetches only what is missing. `--force` is the opposite.
`nvd export` takes `--format` and defaults to `epub`.

Two conventions make it scriptable:

- **Progress goes to stderr, the summary to stdout.** `nvd download … >
  summary.txt` captures the result rather than the progress meter.
- **Exit codes mean something**: `0` on success, `1` on failure, `2` for a bad
  command line.

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
