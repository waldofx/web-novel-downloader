# Gotchas

**Append-only.** Every time something surprises you, costs time, or would bite
the next person, add a row. Never delete one: a fixed gotcha is usually one
version away from coming back, and the next person will search for the *symptom*
— so write the symptom as you actually observed it, then the cause, then the fix.

## Tooling on Windows

| Symptom | Cause | Fix / workaround |
| --- | --- | --- |
| Non-ASCII output looks mangled (`本斁E`, stray `\u{3000}`) while tests pass or fail consistently | PowerShell's console decoding, not the data | Judge the **assertion message and the file bytes**, never the console rendering. Set `[Console]::OutputEncoding = [Text.Encoding]::UTF8` before reading output, or read the file back with a tool |
| `cargo`, `git` or `gh` print red errors and the shell reports exit 1, although they succeeded | PowerShell turns native stderr into `NativeCommandError` and propagates a non-zero code | Check the real output/artifact rather than the exit code. A success line ("Switched to branch…", "Finished") means it worked |
| A commit message or issue body containing Japanese arrives garbled or truncated | native argv encoding on Windows | Write the text to a UTF-8 file first, then `git commit -F <file>` / `gh ... --body-file <file>` |
| `something > out.txt` produces a UTF-16 file full of NUL bytes | PowerShell `>` redirects as UTF-16 | Use `Set-Content -Encoding utf8` or a write tool, then verify by reading the file |
| `gh issue list --jq '.title'` errors, or the expression is split into separate arguments | `--jq` requires `--json`, and PowerShell splits arguments containing spaces | Put `--json <fields>` before `--jq`, or avoid spaces in the expression |
| A branch was created literally named `issue-` | the command interpolated an empty variable (`$num` never matched) | Echo the extracted value before using it; derive numbers from real tool output |

## Git and GitHub workflow

| Symptom | Cause | Fix / workaround |
| --- | --- | --- |
| The "next" issue number is one off | issues and pull requests share a single numbering sequence | Derive the number from `gh` output; never count |
| A push to `main` is rejected | `main` is protected (PR required, `enforce_admins`) | Work goes `issue-<nnn>` → `development`; only a human merges `development` → `main` |
| A rename shows up as a delete plus an add, and the history is lost | plain add/delete instead of `git mv` | Use `git mv` for renames |

## Narou markup

| Symptom | Cause | Fix / workaround |
| --- | --- | --- |
| Trimming ate the paragraph indentation | Narou indents with U+3000, which is **not** ASCII whitespace, so `trim()` removes it | Use `trim_ascii()` throughout the Narou adapter. Enforced by tests in `sources/src/narou/dom.rs` |
| A blank line vanished, or two blank lines produced nothing at all | Narou has no empty `<p>`: a blank line is `<p id="L1"><br /></p>` | Test blankness with `element_text(..).trim_ascii().is_empty()`; let the line-run logic turn a run of ≥2 into a `SceneBreak` |
| The chapter date is wrong, or a revision date shows up as the published date | the revision date is in a `title` **attribute** on a `<span>` inside the update cell, and naive text extraction picks up both | Read only the cell's own text; ignore attributes (see `sources/src/narou/toc.rs`) |
| A short novel lists zero chapters, or its chapters 404 | a single-chapter novel renders its whole body inline on the table-of-contents page, and `/1/` does not exist | `chapter_metas()` synthesizes one chapter whose `url` is the TOC URL itself |
| An adapter claims a lookalike domain such as `ncode.syosetu.com.evil.example` | substring matching instead of exact host matching | Compare the full lowercased `host_str()` against the known host list |
| An `novel18` work is tagged as general audience | the rating is not in the page; it is a property of the host | `rating_for()` derives it from the host |

## Rust layout and tests

| Symptom | Cause | Fix / workaround |
| --- | --- | --- |
| "no method `children`/`descendants`" on `ElementRef` when working with `scraper` | `ElementRef` exposes no child iteration; the tree belongs to `ego-tree` | Walk with `first_child()`/`next_sibling()`/`parent()`/`value()` and wrap nodes with `ElementRef::wrap(node)`. Advance the cursor to `next_sibling()` **before** using the node: the nodes are reference counted |
| `include_str!("tests/fixtures/…")` cannot find a file that clearly exists | `include_str!` resolves relative to the **source file**, not the crate root | Count the `../` from the including file. One `src/fixtures.rs` centralizes all of them |
| An item marked `pub(super)` is unexpectedly reachable, or one that is not marked is unreachable, from a sibling module | `pub(super)` means "visible in the parent module", whose other children are inside it; and *private* items of a module are visible to its descendants | In `narou/`, cross-module items are `pub(super)` and the selectors are private in `mod.rs` — reachable from `toc`/`chapter` because those are descendants |
| A `#[test]` in a `tests/` subfolder never runs | Cargo compiles only **top-level** files in `tests/`; subfolders are modules | Name the file `tests/<feature>_<kind>.rs`; keep subfolders for shared helpers and fixtures only |
| A test fails and the implementation looks wrong | sometimes the **expectation** is wrong (a bad index, a trailing U+3000 that is genuinely content) | Decide which side is wrong before touching either. Fixing code to satisfy a wrong test is worse than red — see the `test-first` skill |
| `cargo clippy` is clean but the test code has warnings | `--all-targets` was omitted | `cargo clippy --workspace --all-targets -- -D warnings` |
| Clippy fails a loop that walks a `Vec` by index ("the loop variable is used to index …") | `needless_range_loop` — and it fires even when the collection being indexed is a *different* one from the collection being iterated | Iterate the one you only read (`for (position, skip) in skips.iter().enumerate()`) and index the one the body must mutate |

## Format and storage

| Symptom | Cause | Fix / workaround |
| --- | --- | --- |
| A crash mid-write left a truncated JSON file that fails to parse | `write_json` writes a sibling `.tmp` and renames it over the target | Never write a data file in place: the rename replaces an existing file on Windows as well as Unix, so one implementation is atomic on both. See [format.md](format.md#storage-mechanics) |
| The manifest records a hash but the chapter file is empty, stale, or hand-edited | the file write and the manifest update are separate steps, so a kill between them leaves the two disagreeing | Resume verifies the **content hash**, never existence. Do not "simplify" that check to `exists()`: it would accept exactly the cases the hash exists to catch |
| A chapter was re-fetched although it was already on disk | resume trusts file existence instead of content | Resume only when the stored hash exists, the file exists, **and** re-hashing the file reproduces the stored hash |
| An adapter was blamed for an empty chapter that was never cached | a page with no text block returned an empty chapter instead of an error | A chapter page with no text container is `SourceError::Parse` — silence there hides adapter rot |
