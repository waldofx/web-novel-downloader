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
| The chapter date is wrong, or a revision date shows up as the published date | the revision date is in a `title` **attribute** on a `<span>` inside the update cell, and naive text extraction picks up both | The cell's own text is `published_at`; the attribute on the inner `<span>` is a different fact and is stored separately as `revised_at`. Read each from where it lives and never let one stand in for the other (`sources/src/narou/toc.rs`) |
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
| The chapter body contains the header, the share buttons, and an ad | the body container sits in the middle of a page full of chrome | Scope to `div.widget-episodeBody`; a page with no such container is `SourceError::NotTheSite`, not `Parse`: the fetch layer can then say what answered in its place — see [fetch.md](fetch.md#the-evidence) |
| An episode page has no `__NEXT_DATA__` at all, while the work page has one | only work pages carry a snapshot | The two pages need two idioms: the list from JSON, the body from HTML. An adapter that assumes one shape for the whole site gets one of them wrong |
| A work with no arcs and a work with arcs need different code | they do not: an un-arc'd work has one unnamed group whose key is literally `TableOfContentsChapter:` — the empty id is the page's own shape | Resolve refs by exact key string, and treat a missing `chapter` field as "no arc". Requiring a non-empty id breaks every un-arc'd work |
| A well-meaning change makes `published_at` read `…+09:00` like Narou's | Narou states a wall-clock time with no zone, Kakuyomu states a UTC instant; both are RFC 3339, which is all the model promises | Leave the instant as the site stated it. Converting means date arithmetic (leap years, month lengths) for a field nothing displays; a consumer that wants local time formats it |

## Networks that answer for the site

| Symptom | Cause | Fix / workaround |
| --- | --- | --- |
| One site opens fine in the browser, but every tool on the machine resolves it to a single address that serves a block notice | The ISP intercepts port 53 for every resolver: asking 8.8.8.8 and 1.1.1.1 **directly** both answered `202.137.1.74` for `syosetu.org`, while DoH returned Cloudflare's real addresses. The browser gets through because Secure DNS is HTTPS on 443 | It is a property of the network, not of an adapter: `Error::Blocked` now says so — see [fetch.md](fetch.md#pages-that-are-not-the-site). Windows 10 has no built-in DoH, and a local DoH resolver cannot bind port 53 while `SharedAccess` (ICS) holds it |
| Cloudflare answers `403` with `Just a moment...` to every client we have, including one pretending to be Chrome | A managed challenge: it wants JavaScript, so the user agent is not the lever | Recognised and reported. Getting past it needs a browser-obtained clearance cookie, which is a decision for #17/#20 and not something to slip in quietly. Do not spend another afternoon on UA spoofing |
| The filter page is never seen over `https`; the same host simply times out | The hijack box has no TLS listener, so `https://` dies at the connection while `http://` gets the notice | Expect `Error::Network` (a timeout) on a filtered `https://` URL and `Error::Blocked` on the `http://` form. There is no page to quote in the timeout case, so nothing reports it for you. Do not "fix" the timeout by retrying it |
| A fixture captured from one machine shows a page nobody else can reproduce | The filter box branches on the client: a VPN exit got First Media's "Internet Sehat" page, the customer's own line got the Komdigi `SITUS DIBLOKIR` notice | Do not pin a block page's *phrases* — pin the shape of the report. The message quotes whatever arrived, so no vendor list is needed, and `crates/core/tests/fixtures/blocked/` keeps one capture only to test the excerpt and its empty case |
| A block page in a language, or from a provider, this repository has never heard of | A phrase list only covers the filters already met — which is why #26's `SITUS DIBLOKIR`/`internetsehat` markers were deleted | The rule is structural: Cloudflare's `Cf-Mitigated` header, a redirect that answers from another host, and "the host answers a path that cannot exist with the same page, byte for byte". Everything else is *shown*, not recognised. Adding a phrase to the code is now the wrong instinct |
| A Cloudflare challenge was reported as `http 403 from <url>`, which reads as the site refusing us | The challenge is named by a non-retryable `403` | Read the header of a non-retryable status (`Cf-Mitigated`) and say challenge rather than status. Do not read the body looking for `Just a moment...`: a challenge without the header is reported as what arrived, with its text |
| In principle, a chapter that *quotes* a block notice would be reported as that notice | Any content heuristic can be wrong | Nothing matches content any more: a body is only ever quoted, and the tests keep it that way (`a_filter_page_served_with_200_is_only_a_page`) |
| An empty excerpt where the captured block page clearly has text | The captured First Media page is a tag manager, a stylesheet, an ad script and a spacer image; the excerpt drops all of them | Say so, and let the probe line carry the finding ("answered a URL that cannot exist with this same page"). Do not fall back to quoting markup: CSS is not evidence |
| The interception probe fires on a site that merely has its own not-found page | Many sites answer an unknown path with `200` and a template that echoes the URL it was asked for | The comparison is byte for byte **on purpose**, so a template that differs by the URL does not match. Do not "improve" it with a fuzzy comparison — that is the false positive |
| A cached body has no status, media type or final URL to report | `get_text_cached` stores the body only | It is for re-parsing, not for reporting. Anything that may have to say what answered uses `get_page`, and takes the facts with it |

## Sessions and credentials

| Symptom | Cause | Fix / workaround |
| --- | --- | --- |
| A `cookies.txt` line that looks fine is not read as a cookie | `#HttpOnly_` in front of the domain is a marker on a **real** cookie, not a comment; every other `#` is a comment. A parser that treats every `#`-prefixed line as a comment silently drops the session cookies | `netscape::parse` strips the marker and treats the rest as a cookie. Test: `an_http_only_line_is_a_cookie_not_a_comment` |
| An editor-saved `cookies.txt` breaks on its first line | "UTF-8 with BOM" puts `U+FEFF` where the first domain should be | Strip a leading BOM before parsing (`text.strip_prefix('\u{feff}')`) |
| A cookie file's error message quotes the secret | An error message ends up in terminals, logs and issues, and a cookie is a credential | Errors name the line and the reason only: `not a Netscape cookies.txt: line 1: ...`. Never the content |
| A session's `Debug` output reveals a cookie value | `#[derive(Debug)]` on a struct holding a `value` field prints it | Hand-write `Debug` for `Cookie` and `Session`, redacting the value; a test asserts it (`a_cookie_never_prints_its_value`) |
| A session is offered to a host it does not name | The filter is "send if the host matches", and a naive suffix check matches `notpixiv.net` | `host_is_covered` requires a dot boundary, rejects IPs' subdomains, and is case-insensitive. A session with no hosts goes nowhere |
| `list` of an empty store is an error | The directory does not exist yet on a fresh machine | `list` returns an empty vector when the directory is missing; `save` creates it |

## Rust layout and tests

| Symptom | Cause | Fix / workaround |
| --- | --- | --- |
| "no method `children`/`descendants`" on `ElementRef` when working with `scraper` | `ElementRef` exposes no child iteration; the tree belongs to `ego-tree` | Walk with `first_child()`/`next_sibling()`/`parent()`/`value()` and wrap nodes with `ElementRef::wrap(node)`. Advance the cursor to `next_sibling()` **before** using the node: the nodes are reference counted |
| `include_str!("tests/fixtures/…")` cannot find a file that clearly exists | `include_str!` resolves relative to the **source file**, not the crate root | Count the `../` from the including file. One `src/fixtures.rs` centralizes all of them |
| An item marked `pub(super)` is unexpectedly reachable, or one that is not marked is unreachable, from a sibling module | `pub(super)` means "visible in the parent module", whose other children are inside it; and *private* items of a module are visible to its descendants | In `narou/`, cross-module items are `pub(super)` and the selectors are private in `mod.rs` — reachable from `toc`/`chapter` because those are descendants |
| A `#[test]` in a `tests/` subfolder never runs | Cargo compiles only **top-level** files in `tests/`; subfolders are modules | Name the file `tests/<feature>_<kind>.rs`; keep subfolders for shared helpers and fixtures only |
| A test that drives an adapter's `novel` against a local socket cannot even be written | work URLs are matched on **host** (`ncode.syosetu.com`, `kakuyomu.jp`), and a local URL is rejected before any request is made | Drive `chapter` instead: a chapter URL comes from the manifest and is fetched as-is, so `crates/sources/tests/blocked.rs` exercises both adapters over a socket. The work/TOC path stays covered by parser tests plus captures |
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
| An adapter was blamed for an empty chapter that was never cached | a page with no text block returned an empty chapter instead of an error | A chapter page with no text container is `SourceError::NotTheSite` — silence there hides both adapter rot and a page that is not the site |

## Exports

| Symptom | Cause | Fix / workaround |
| --- | --- | --- |
| A reader says the file "is not a valid EPUB", or refuses it outright | `mimetype` was not the first entry in the ZIP, or it was deflated | It must be first **and** `CompressionMethod::Stored`, containing exactly `application/epub+zip`. This is a property of the bytes, so the test parses the local file header by hand — nothing an archive reader reports will tell you |
| Two exports of one novel differ, so a re-export churns in git | something read the clock, or iterated a `HashMap` | ZIP stores a modification time per entry, so write every entry with a fixed `DateTime::DEFAULT`; sort and de-duplicate the image list; take the EPUB's `dcterms:modified` from `manifest.fetched_at`, never from "now". See [export.md](export.md#determinism) |
| The exported file's name is not the path the code reported | Windows silently strips a trailing dot or space from a file name | Trim trailing `.` and spaces when deriving a name from the title, so the file that gets written is the path that was returned |
| Furigana reads as `漢字かんじ` on an e-reader | a reader that ignores `<ruby>` shows the `<rt>` content inline | Emit `<rp>(</rp>` and `<rp>)</rp>` around the reading: a reader that ignores ruby renders `漢字(かんじ)` instead |
| Every illustration in the EPUB is a broken link | the model's `src` is relative to the **novel directory**, but the document is not (EPUB: `text/`, Markdown: `exports/`) | Prefix the path so it is relative to the document doing the referencing — `../images/…` in both cases. A test that asserts the attribute is cheaper than opening a reader |
| A synopsis loses its line breaks in the EPUB | a newline is only whitespace in XHTML | Turn newlines into `<br />` when rendering text that came from a `<br>` in the source |
| An e-reader shows one undifferentiated list of 200 chapters for a novel the site groups into arcs | the exporter used `chapters[].arc` for TXT and Markdown but not for the EPUB | Nest a chapter under its arc in `nav.xhtml`, and print the arc as a divider in the chapter document. Pinned by `the_epub_navigation_nests_chapters_under_their_arc` |
| The arc heading is repeated before every chapter of the arc | each EPUB chapter is its **own document**, so it cannot see that the previous chapter carried the same divider | Narrow the divider where the chapters are walked in reading order (`epub::write`), the way TXT's and Markdown's running `arc` variable does. Fixing it inside `chapter_document` is impossible without the neighbours |
| A part heading with no page of its own does not fit the EPUB 3 navigation | `<li>` takes an `<a>` or a `<span>`, and the nested `<ol>` goes beside it — there is no "heading" element, and inventing an arc page changes the spine | `<li><span>第１章　覚醒</span><ol>…</ol></li>`. The structure a reader navigates by is the nav document, which is why the divider itself is a `<p class="arc">` rather than another heading level |

## Updating

| Symptom | Cause | Fix / workaround |
| --- | --- | --- |
| An update re-fetches every chapter after one is added in the middle of a work | Kakuyomu numbers episodes by **position**, so an episode inserted at 2 moves 2…n down one number each | Correct, not a bug: every shifted number now holds a different episode, and identity is the chapter's **url**. Comparing by number instead would leave the previous chapter's text under every shifted number. See [format.md](format.md#what-nvd-update-treats-as-changed) |
| Everything is re-fetched once, right after `revised_at` was introduced | directories written before the field existed have no stamp, and `None` vs `Some(…)` is "cannot tell", not "unchanged" | Fetch once, record the stamp, and every later run is quiet. Treating a missing stamp as "current" would silently keep stale text, which is worse than one extra fetch |
| A chapter was clearly edited but update never notices | Kakuyomu publishes no per-chapter revision time: a live work page (2026-10-17) gives every episode `{id, title, publishedAt}` and the work only `lastEpisodePublishedAt` | Nothing exists to store. `nvd update --check` fetches and compares instead — the only honest answer on such a site |
| `--check` takes a minute on a long novel | it is one request per chapter, under the per-host delay | Expected, and why it is opt-in rather than the default |
| `nvd download --max-chapters 2`, then `nvd update`, downloaded 87 more chapters | the cap was a download-time choice, and nothing on disk records that it was made | Update follows the index by design ([format.md](format.md#what-nvd-update-treats-as-changed)); re-download with the cap if the whole novel was not wanted |
| A directory became unreadable after a `FORMAT_VERSION` bump | the check compared versions for exact equality | Refuse a **newer** version only, and read an older one: the version moves only for an incompatible change, so older is readable by definition — and a bump that stranded existing directories would break exactly the thing `nvd update` is for |
| An update erased the recorded cover image | a cover is a path to a file **in the directory**, which no index can describe, so the fresh manifest has none | Preserve what the index cannot produce (`cover`) instead of overwriting it with silence |
