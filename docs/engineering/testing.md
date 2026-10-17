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

## The suite must pass offline

Network tests are `#[ignore]`d. They must never make `cargo test` depend on a
third party's server being reachable — or on it tolerating us.

```bash
cargo test --workspace                                    # offline, always green
cargo test -p wnd-core   --test net_live   -- --ignored --nocapture
cargo test -p wnd-sources --test narou_live -- --ignored --nocapture
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
| the download sequence: resume, `--force`, `--max-chapters`, adult refusal, an interrupted run | `crates/download/tests/download.rs` | driven by an in-memory adapter — no network |
| the exports: exact TXT and Markdown documents, EPUB structure, reproducibility | `crates/export/tests/export.rs` | a novel written to a temp directory; the EPUB is read back with `zip` |
| Narou DOM helpers, notation, blank lines, TOC, dates | `sources/src/narou/*` | fixture files |
| the whole pipeline under the real binary: exit codes, which stream output goes to, flags, `--help` | `crates/cli/tests/cli.rs` | runs `nvd` itself, so no library test can cover it. Offline, except one `#[ignore]`d test that downloads two chapters for real |
| Narou against the real site | `sources/tests/narou_live.rs` | `#[ignore]`d |
