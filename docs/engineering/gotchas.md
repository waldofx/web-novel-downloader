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
| A fixture written by a PowerShell helper has mojibake Japanese, or a BOM that `include_str!` then carries | the `.ps1` is read as ANSI, so non-ASCII literals in it are already mangled; and `Set-Content -Encoding UTF8` writes a BOM | Build non-ASCII from code points (`[char]0x3000`) or copy it out of the capture file; write with `[IO.File]::WriteAllText($path, $text, (New-Object Text.UTF8Encoding($false)))`; check the first bytes afterwards |

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

## Kakuyomu markup

| Symptom | Cause | Fix / workaround |
| --- | --- | --- |
| A 1069-episode novel downloads **seven** chapters, and nothing complains | the work page renders only the first chapter's episodes and lazy-loads the rest; the full list never reaches the HTML | Read the episode list from `__NEXT_DATA__` → `props.pageProps.__APOLLO_STATE__`. The live test asserts the count with a number in the message, so a regression fails loudly |
| Counting episode links on a work page gives a number you cannot explain | the "1話目から読む" button carries the same `…/episodes/<id>` href as the first TOC row, and the TOC is lazily paginated | Do not count links. (Moot now that the list comes from the snapshot — kept because it is the symptom you will see if you try) |
| Every scene break in every chapter silently disappeared | a blank line is `<p class="blank">` holding **one `<br>` per blank line**; reading the class as "one blank line" makes a scene break look like a paragraph separator | Count the `<br>`s inside the paragraph (`blank_lines` in `kakuyomu/chapter.rs`). One real chapter has 46 scene breaks that a naive reading finds none of |
| A paragraph of a single U+3000 reaches the reader | a blank line whose content is an ideographic space, marked only by the class | Test blankness as *class `blank`* **or** empty text. `trim()` would eat U+3000, and the U+3000 is that paragraph's only content |
| No tags, no author, or a cover that is obviously not the cover | none of the three is in the rendered HTML: tags and the author's name live only in the snapshot, and `og:image` is a *generated* share card | Read `tagLabels`, `author` → `UserAccount.activityName`, and `adminCoverImageUrl` from the snapshot. Do not fall back to `og:image`, and do not set `Manifest.cover` — that field is a path to a *fetched* image |
| The chapter body contains the header, the share buttons, and an ad | the body container sits in the middle of a page full of chrome | Scope to `div.widget-episodeBody`; a page with no such container is `SourceError::Parse`, since an empty chapter would hide adapter rot |
| An episode page has no `__NEXT_DATA__` at all, while the work page has one | only work pages carry a snapshot | The two pages need two idioms: the list from JSON, the body from HTML. An adapter that assumes one shape for the whole site gets one of them wrong |
| A work with no arcs and a work with arcs need different code | they do not: an un-arc'd work has one unnamed group whose key is literally `TableOfContentsChapter:` — the empty id is the page's own shape | Resolve refs by exact key string, and treat a missing `chapter` field as "no arc". Requiring a non-empty id breaks every un-arc'd work |
| A well-meaning change makes `published_at` read `…+09:00` like Narou's | Narou states a wall-clock time with no zone, Kakuyomu states a UTC instant; both are RFC 3339, which is all the model promises | Leave the instant as the site stated it. Converting means date arithmetic (leap years, month lengths) for a field nothing displays; a consumer that wants local time formats it |

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
| `if let Some(x) = a && b` compiles locally but not for someone on the stated MSRV | let-chains are Rust **1.88**; this workspace claims `rust-version = "1.85"` | Chain with combinators instead — `a.as_ref().filter(\|x\| b)` — or nest the `if`. Do not "fix" it by raising `rust-version` without a decision |
| `writeln!(s, …)` on a `String` will not compile, or adding `use std::io::Write` breaks `write_all` | both traits are named `Write`, and `writeln!` needs the `std::fmt` one while `write_all` needs the `std::io` one | Import both anonymously: `use std::fmt::Write as _;` and `use std::io::Write as _;` |
| A dependency build pulls in a C toolchain, or is far slower to compile than expected | `zip`'s **default features** enable zopfli (optimal but slow), plus lzma, bzip2, zstd and AES | Depend on the slice you need: `zip = { version = "7", default-features = false, features = ["deflate-flate2"] }` — pure Rust, sane speed |
| Testing the CLI looks like it needs `assert_cmd`/`escargot` | Cargo already tells an integration test where its own binary is | `env!("CARGO_BIN_EXE_nvd")` in `crates/cli/tests/cli.rs`. `Command::output()` also captures raw UTF-8 bytes, which sidesteps the console-encoding trouble described above |
| A CLI test asserts "non-zero" and gets `2` | `clap` exits `2` for a usage error before your code runs | Assert `2` for a rejected flag or value and `1` for an application failure. Collapsing them into "non-zero" would hide a broken argument definition |

## Format and storage

| Symptom | Cause | Fix / workaround |
| --- | --- | --- |
| A crash mid-write left a truncated JSON file that fails to parse | `write_json` writes a sibling `.tmp` and renames it over the target | Never write a data file in place: the rename replaces an existing file on Windows as well as Unix, so one implementation is atomic on both. See [format.md](format.md#storage-mechanics) |
| The manifest records a hash but the chapter file is empty, stale, or hand-edited | the file write and the manifest update are separate steps, so a kill between them leaves the two disagreeing | Resume verifies the **content hash**, never existence. Do not "simplify" that check to `exists()`: it would accept exactly the cases the hash exists to catch |
| A chapter was re-fetched although it was already on disk | resume trusts file existence instead of content | Resume only when the stored hash exists, the file exists, **and** re-hashing the file reproduces the stored hash |
| An adapter was blamed for an empty chapter that was never cached | a page with no text block returned an empty chapter instead of an error | A chapter page with no text container is `SourceError::Parse` — silence there hides adapter rot |

## Exports

| Symptom | Cause | Fix / workaround |
| --- | --- | --- |
| A reader says the file "is not a valid EPUB", or refuses it outright | `mimetype` was not the first entry in the ZIP, or it was deflated | It must be first **and** `CompressionMethod::Stored`, containing exactly `application/epub+zip`. This is a property of the bytes, so the test parses the local file header by hand — nothing an archive reader reports will tell you |
| Two exports of one novel differ, so a re-export churns in git | something read the clock, or iterated a `HashMap` | ZIP stores a modification time per entry, so write every entry with a fixed `DateTime::DEFAULT`; sort and de-duplicate the image list; take the EPUB's `dcterms:modified` from `manifest.fetched_at`, never from "now". See [export.md](export.md#determinism) |
| The exported file's name is not the path the code reported | Windows silently strips a trailing dot or space from a file name | Trim trailing `.` and spaces when deriving a name from the title, so the file that gets written is the path that was returned |
| Furigana reads as `漢字かんじ` on an e-reader | a reader that ignores `<ruby>` shows the `<rt>` content inline | Emit `<rp>(</rp>` and `<rp>)</rp>` around the reading: a reader that ignores ruby renders `漢字(かんじ)` instead |
| Every illustration in the EPUB is a broken link | the model's `src` is relative to the **novel directory**, but the document is not (EPUB: `text/`, Markdown: `exports/`) | Prefix the path so it is relative to the document doing the referencing — `../images/…` in both cases. A test that asserts the attribute is cheaper than opening a reader |
| A synopsis loses its line breaks in the EPUB | a newline is only whitespace in XHTML | Turn newlines into `<br />` when rendering text that came from a `<br>` in the source |
