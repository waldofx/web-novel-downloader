# Testing

## The rule

Tests come **before** the code: write the failing test, watch it fail for the
intended reason, then implement. Docs come last, once the tests pass and the
shape has settled. The portable version of this convention lives in the author's
global OpenCode `test-first` skill; this page is how it applies here.

A change with no test is not finished, however small.

## Where tests live

Rust has two kinds of test and both are used deliberately:

| Kind | Location | Why there |
| --- | --- | --- |
| unit | `#[cfg(test)] mod tests` in the file under test | needs private access; keeps the test next to the code it describes |
| integration | `tests/<feature>_<kind>.rs` | exercises the public API as a separate binary |

Cargo compiles only **top-level files** in `tests/`. A subfolder there is a
module, not a test target — so feature grouping in `tests/` is a filename
prefix, not a directory:

```
crates/sources/tests/
├── common/mod.rs          # helpers shared by the test files (not a target)
├── fixtures/narou/*.html  # captured pages
└── narou_live.rs          # the Narou network tests
```

`tests/common/mod.rs` carries `#![allow(dead_code)]`: every test binary compiles
it, and no single binary uses everything in it.

## Fixtures

Captured HTML lives in `tests/fixtures/<site>/` as **files**, not inline strings,
once it is longer than a screen or has to mirror real markup. `src/fixtures.rs`
pulls them in with `include_str!`, which resolves at compile time: no test
depends on the working directory, and a fresh capture can be dropped in without
editing Rust.

When a fixture is updated from a live capture, re-read the assertions that depend
on it — a changed page is exactly when an adapter needs attention.

`crates/core/tests/fixtures/blocked/` holds two pages that are *not* the site
(First Media's filter notice, Cloudflare's challenge), each captured from the real
thing with `curl`. Nothing matches their text any more — they are here so the
excerpt has a real page to read, including one with no readable text at all.

## The suite must pass offline

Network tests are `#[ignore]`d. They must never make `cargo test` depend on a
third party's server being reachable — or on it tolerating us.

```bash
cargo test --workspace                                    # offline, always green
cargo test -p wnd-core   --test net_live   -- --ignored --nocapture
cargo test -p wnd-sources --test narou_live    -- --ignored --nocapture
cargo test -p wnd-sources --test kakuyomu_live -- --ignored --nocapture
cargo test -p wnd-sources --test hameln_live   -- --ignored --nocapture
cargo test -p wnd-sources --test pixiv_live    -- --ignored --nocapture
```

Live tests print what they found (`--nocapture`) so that a failure shows the real
shape of the page instead of only an assertion.

## The gate

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

`--all-targets` matters: without it, warnings in test code are missed.

If the change touched fetching or an adapter, run the ignored live tests and
paste the output. "Should work" is not evidence.

## What is covered where

| Area | Where | Notes |
| --- | --- | --- |
| policy defaults, retry classification, backoff, `Retry-After` | `core/src/net/*` unit tests | no network |
| `robots.txt` parsing and decisions | `core/src/net/robots.rs` | plus one live test |
| model serialization, storage round-trip, hash stability | `core/src/store/{model,storage}.rs` | temp directories; atomic writes leave no `.tmp` behind |
| the download sequence: resume, `--force`, `--max-chapters`, an interrupted run | `crates/download/tests/download.rs` | driven by an in-memory adapter — no network |
| the exports: exact TXT and Markdown documents, EPUB structure, reproducibility | `crates/export/tests/export.rs` | a novel written to a temp directory; the EPUB is read back with `zip` |
| the excerpt from a body that is not the site: tags, comments, `<script>`/`<style>`, entities, whitespace, and the cut on a character boundary | `core/src/net/blocked.rs` | fixture captures in `crates/core/tests/fixtures/blocked/`, plus the captured Komdigi notice's own words |
| what the fetch layer does with an answer that is not the site: the `Cf-Mitigated` header, no page-text matching, the facts that travel with a body, a redirect, and the interception probe | `crates/core/tests/blocked.rs` | a real one-shot socket on `127.0.0.1`, so it stays offline; the exact user-facing sentence is asserted |
| that an adapter reports a page which is not the site with the page's own words, and does not cry "filter" when nothing was proven | `crates/sources/tests/blocked.rs` | drives Narou's, Kakuyomu's and Pixiv's `chapter` over a one-shot socket and serves a block page in a language this repository has never seen |
| Narou DOM helpers, notation, blank lines, TOC, dates | `sources/src/narou/*` | fixture files |
| the shared DOM helpers, blank-line rule and inline walk | `sources/src/{dom,blocks,inline}.rs` | every HTML adapter goes through them, so they are covered by those adapters' tests as well as their own. Pixiv is the one adapter that reads no markup: it uses the blank-line rule and nothing else |
| Kakuyomu's snapshot reader, arc grouping, blank-line counts, body scoping | `sources/src/kakuyomu/*` | fixture files, including a work whose HTML would lie about its size |
| Pixiv's API envelope, its `null` fields, ids stated as strings or numbers, and the shape of a series listing | `sources/src/pixiv/api.rs` | fixture answers, including the failure envelope that arrives as HTTP 200 |
| Pixiv's text notation: `[newpage]` joining, furigana, links, the author's `[chapter:]` headings, uploaded and referenced images, and the markers that stay literal | `sources/src/pixiv/content.rs` | a captured body, block for block |
| Pixiv URL forms, work expansion, chapter numbering and the URLs a manifest holds | `sources/src/pixiv/mod.rs` | pure functions over the answers above; the work/listing paths themselves are covered by the live tests |
| the whole pipeline under the real binary: exit codes, which stream output goes to, flags, `--help` | `crates/cli/tests/cli.rs` | runs `nvd` itself, so no library test can cover it. Offline, except one `#[ignore]`d test that downloads two chapters for real |
| Narou against the real site | `sources/tests/narou_live.rs` | `#[ignore]`d |
| Kakuyomu against the real site, including the 1069-episode work | `sources/tests/kakuyomu_live.rs` | `#[ignore]`d |
| Pixiv against the real site: a series URL expanding to every chapter it states, and a standalone novel's reader pages arriving whole | `sources/tests/pixiv_live.rs` | `#[ignore]`d. Both tests re-read the API answer the manifest points at and compare *counts* — the two ways this adapter fails quietly are a series cut to one chapter and a chapter cut to one reader page |
