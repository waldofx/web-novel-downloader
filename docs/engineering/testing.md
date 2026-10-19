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

Each crate keeps its own copy of those helpers: an integration test is a separate
crate, so it can share a module *within* a crate but not across crates. The one
that is deliberately identical everywhere is `targets(site)` — where a live test
gets its work from (below).

## Fixtures

Captured HTML lives in `tests/fixtures/<site>/` as **files**, not inline strings,
once it is longer than a screen or has to mirror real markup. `src/fixtures.rs`
pulls them in with `include_str!`, which resolves at compile time: no test
depends on the working directory, and a fresh capture can be dropped in without
editing Rust.

When a fixture is updated from a live capture, re-read the assertions that depend
on it — a changed page is exactly when an adapter needs attention.

A capture is usually longer than the test needs, and a **trimmed** one is still a
real one: keep the markup around the thing under test, cut the rest, and say in
the constant's doc comment in `src/fixtures.rs` what was trimmed and where the
markup came from. That matters most when a fixture is composed from two captures,
or when the site could not be reached for a fresh one —
`hameln/chapter-illustrated.html` was once both, and the comment beside the
constant said so.

The same goes for length: a list's *shape* is in its rows, not in how many of them
a particular work has. `hameln/work.html` carries twelve episodes where the site
puts a work's whole list on one page, and the assertion that the parser reads
*every* episode is a fact about the site's markup, not about that row count.

The **shape** is what a fixture is for, not the words in it, and that is a
two-part rule:

- **The site's half stays exactly as captured**: elements, classes, attributes,
  spacing, its own UI text (`感想を書く`, `次の話 >>`), its dates, its host names,
  and the vocabulary the site defines — a genre label, a rating, a tag. Those are
  what an adapter matches on, and a fixture that "tidies" them tests the adapter
  against a page that does not exist.
- **The work's half does not**: a work's title, its author's name, its synopsis
  and chapter titles, a line of its prose, its ids and its addresses are all
  replaced. Nothing from a real novel is committed — that is the rule in
  [AGENTS.md](../../AGENTS.md) and [CONTRIBUTING.md](../../CONTRIBUTING.md) — and a
  **made-up** example is what a fixture, a doc or a test carries instead.

The placeholder fleet, so that a replaced address still reads as one:

| what | what stands in for it |
| --- | --- |
| a work title | `見本の作品　～サンプル～` |
| an author | `見本作者`, `見本の作者` |
| a chapter title | `第一話　見本の一話` |
| prose | `本文の一行目の見本です。` |
| a Narou code | `n0000aa` … `n0000dd` |
| a Hameln or `h.syosetu` id | `/novel/000000/`, `/novel/000001/` |
| a Kakuyomu id | `10000000000000000001`, … |
| a Pixiv id | `show.php?id=10000001`, `series/10000006` |
| an image | `https://img.example.test/…`, `i.pximg.net/…/00000000_p0.png` |
| a host that is not a site | `example.test` |

`crates/sources/tests/placeholders.rs` checks the mechanical half of that — every
work address in the repository points at a placeholder — and `cargo test` runs it
with everything else. The prose half is not greppable, and is what review is for.
The raw capture keeps the real words in the gitignored `captures/`, and the works
a live test runs against live in the gitignored `fixtures.local.json`.

Two traps in a browser-saved capture. It rewrites every asset it loaded to
`./<page title>_files/…` and leaves the `<a href>` beside it alone, so the `src`
in the save is not the address the site served. And the page can be a different
one than it looks: `hameln/chapter-illustrated.html`'s markup first came from a
work page's **synopsis**, because `syosetu.org` was unreachable from the machine
that wrote it, and now comes from a body.

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
cargo test -p wnd-download --test pixiv_live   -- --ignored --nocapture
```

Live tests print what they found (`--nocapture`) so that a failure shows the real
shape of the page instead of only an assertion.

They also take the works they hit from the **gitignored** `fixtures.local.json` at
the repository root — copied from `fixtures.local.example.json`, whose URLs are
all placeholders. A test asks that file for a site, takes the first entry that
fits the shape it needs (a multi-chapter work, a one-chapter work, a long work),
and prints what it was looking for before skipping when there is nothing to use.
No committed test hard-codes a work, so a red live test means the adapter rather
than a stale id — and a fresh clone runs the whole suite without reaching anyone.

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
| fetching a picture: the file and its name, the manifest entry and its hash, rewriting the chapter, resume by hash, a `404` that costs no chapter, a page served where a picture belongs, the extension coming from the media type | `crates/download/tests/images.rs` | a real one-shot HTTP server on `127.0.0.1`, so it stays offline; it records each request and its `Referer`, which is the only way to check the header |
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
| what the window asks the library for — a URL without a scheme, validation before a thread, every response's shape, resume, update, `info`, the session picker — and the report it renders | `apps/gui/tests/job.rs` | an in-memory adapter, a temp directory, no Tauri and no window |
| the request JSON exactly as the front end sends it: camelCase keys, the options spread beside the request's own fields, and what is assumed when a field is absent | `apps/gui/src/job.rs` | unit tests; the two files on either side of the IPC have to agree on this shape |
| one job at a time, a poisoned flag not blocking every later job, and a run ending in exactly one `done` or `failed` | `apps/gui/src/lib.rs` | unit tests. The Tauri half of that file is a handful of lines and is not separately tested — see the note below |
| the front end against the command layer: every command `app.js` invokes is one `generate_handler!` registers, the channel name, and every element id it looks up being in the page | `apps/gui/tests/ui.rs` | reads both sides. A window cannot be clicked by a test, and these are the disagreements that would otherwise arrive as a blank window with an error nobody can see |
| Narou against the real site | `sources/tests/narou_live.rs` | `#[ignore]`d |
| Kakuyomu against the real site, including a long work whose rendered HTML would lie about its size | `sources/tests/kakuyomu_live.rs` | `#[ignore]`d. Which works these are is this machine's own business — `fixtures.local.json` |
| Pixiv against the real site: a series URL expanding to every chapter it states, and a standalone novel's reader pages arriving whole | `sources/tests/pixiv_live.rs` | `#[ignore]`d. Both tests re-read the API answer the manifest points at and compare *counts* — the two ways this adapter fails quietly are a series cut to one chapter and a chapter cut to one reader page |
| An illustrated pixiv series, downloaded whole: the cover and the in-text pictures, every hash verified against bytes a real server sent, a second run fetching nothing, and `--check` finding nothing changed | `download/tests/pixiv_live.rs` | `#[ignore]`d. This is the one test that proves the `Referer`, because `i.pximg.net` answers `403` without it and would fail every assertion at once |

## The desktop app's one manual check

`apps/gui/ui/` is three static files, and a window is the one thing here a blast
of `cargo test` cannot click. What the committed tests cover is that the front
end asks for the right things and that it agrees with the command layer. What
they cannot cover is that the page *loads*, that its event handler renders, and
that a round trip closes.

Tauri's own mock runtime would test that, and is not usable here: linking the
runtime into a test binary pulls in `comctl32` imports that need a manifest only
the *app* gets (see [gotchas.md](gotchas.md)). So it was checked once, by hand,
and the harness is kept at `apps/gui/tests/ui_manual_check.js`, because writing
it again costs an afternoon:

1. Copy `apps/gui/ui/` to `apps/gui/ui-manual/`, and add
   `<script src="ui_manual_check.js" defer></script>` to the copy's
   `index.html`, after `app.js`. Copy the file there too — it is not loaded from
   `tests/`.
2. Set `build.frontendDist` in `apps/gui/tauri.conf.json` to `ui-manual`, and
   edit the output folder and the novel URL at the top of the check.
3. `cargo build -p wnd-gui` and run `target/debug/nvd-app.exe`. The check runs
   itself: it drives the shipped `onEvent` with the payloads a job emits and
   asserts what the page then shows, then downloads two chapters of a real novel
   through the real commands, and — from the `done` payload it received — asks
   for an export. **A `.txt` in the novel's `exports/` means every check passed;
   a `.md` means one of them failed**, and the export is the only channel out of
   a window that is not being watched.
4. Put `frontendDist` back to `ui` and delete `ui-manual/`. `tests/ui.rs` asserts
   the configuration points at `ui`, so forgetting this fails the gate rather
   than shipping a UI that is not the one the tests read.

That is what proved, for this change, that the page loads, that `onEvent` renders
the plan, the chapter rows, the counts, the missing picture and the report, that
`invoke` reaches the command layer and returns something the front end can use,
and that the event payload the job emits is the payload the front end reads. The
folder dialog and a real button click are still only as tested as a manual run
makes them.
