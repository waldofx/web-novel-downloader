# Getting started

## Status (October 2026)

**Working end to end for Narou.** `nvd download` fetches a novel and its chapters
into a structured directory, `nvd update` fetches only what is new or changed,
`nvd export` turns one into an EPUB, a text file or Markdown, and `nvd info`
describes a directory you already have. Other sites come next — see the
[roadmap](../engineering/architecture.md#roadmap).

| Command | State |
| --- | --- |
| `nvd download <url> --out <dir>` | works |
| `nvd update <dir>` | works |
| `nvd export <dir> --format epub` | works |
| `nvd info <dir>` | works |
| `nvd session save/list/delete` | works (user-provided cookies for challenged sites) |

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

# Hameln too
nvd download https://syosetu.org/novel/316401/ --out ./my-novel

# Pixiv too. This is one chapter of a series, and on Pixiv the work is the
# series — so this is what you get. Add --max-chapters 1 for just that chapter.
nvd download https://www.pixiv.net/novel/show.php?id=29392114 --out ./my-novel

# fetch it and produce a file for an e-reader in the same step
nvd download https://ncode.syosetu.com/n0608gd/ --out ./my-novel --format epub,txt

# fetch an adult work (the site's own age gate still applies)
nvd download https://novel18.syosetu.com/n8309id/ --out ./my-novel

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
| `--max-chapters` | stop after this many chapters, counting from the first |
| `--force` | re-fetch every chapter, ignoring what is already on disk |
| `--no-robots` | do not consult `robots.txt`. You almost certainly do not want this |
| `--session` | use a saved session (by name) for the hosts it names — for sites that challenge ordinary clients |

There is no `--resume` flag because resuming is what happens by default: run the
same command again and it fetches only what is missing. `--force` is the opposite.
`nvd export` takes `--format` and defaults to `epub`.

Flags for `update`:

| Flag | What it does |
| --- | --- |
| `--check` | fetch every chapter and compare it, rewriting only the ones that really differ. One request per chapter: the only way to find an edit on a site that does not announce revisions |
| `--session` | use a saved session (by name) for the hosts it names |

`nvd update` reads the novel's address out of the directory's own `manifest.json`,
so it takes no URL: it is something you do *to a folder*. A chapter is fetched when
it is new, or when the site says it was revised. A chapter that has disappeared
from the site's index is reported and left on disk — never deleted. Everything
else is left alone, without a request.

Two conventions make it scriptable:

- **Progress goes to stderr, the summary to stdout.** `nvd download … >
  summary.txt` captures the result rather than the progress meter.
- **Exit codes mean something**: `0` on success, `1` on failure, `2` for a bad
  command line, and — for `nvd update` only — `3` when there was nothing new or
  changed, so a scheduled run can tell that apart from a failure without reading
  the output.

## What lands on disk

```
my-novel/
├── manifest.json   # the novel's details and its chapter list
├── chapters/       # one file per chapter
├── images/         # the cover, and any illustrations the chapters point at
└── exports/        # generated files
```

Copy it, move it, back it up — it is self-contained. `exports/` is disposable
and always regenerable from `chapters/`. An illustration that could not be
downloaded keeps its address instead of the chapter losing the picture
altogether; `nvd update` tries again on the next run.

The [format page](../engineering/format.md) describes all of it in detail.
