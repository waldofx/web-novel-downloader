# AGENTS.md

Orientation for AI agents — and humans — working in this repository. Read this
first; it is short, and everything else hangs off it.

## How work is done here

The full conventions live in **[CONTRIBUTING.md](CONTRIBUTING.md)**. The short form:

1. **One GitHub issue per scope of work** — per deliverable, not per commit.
2. **Branch `issue-<nnn>` from `development`**, and merge back into `development`
   by pull request. `main` is released code; never push to it.
3. **Commit subject**: `#<nnn> [type](scope): summary`, with scopes from the
   closed list in CONTRIBUTING.md.
4. **Test-first.** Write the failing test, watch it fail *for the intended
   reason*, then implement. A change with no test is unfinished.
5. **Docs last, in the same change** — once the tests pass and the shape has
   settled. Undocumented behaviour is unfinished behaviour.
6. **The gate**, before calling anything done:

   ```bash
   cargo test --workspace
   cargo clippy --workspace --all-targets -- -D warnings
   cargo fmt --all --check
   powershell -NoProfile -File scripts/clean-builds.ps1
   ```

   The last line keeps `target/` bounded: it cleans the debug profile only when
   the cache passes 3 GB. `target/` is generated, never committed; a release exe
   for manual testing is copied outside it first. See
   [testing.md](docs/engineering/testing.md#the-build-cache).

Global OpenCode skills carry the portable version of these conventions:
`git-workflow`, `test-first`, and `docs`. Load them when a task matches.

## Layout

| Path | What lives there |
| --- | --- |
| `crates/core` | site-independent code: `store/` (the model and on-disk storage) and `net/` (fetch, policy, rate limiting, robots) |
| `crates/sources` | one adapter per site or site family, as a folder of one job per file (e.g. `narou/{toc,chapter,inline,dom}.rs`) |
| `crates/download` | orchestration: a novel URL in, a novel directory out |
| `crates/export` | EPUB, TXT and Markdown, rendered from the chapters |
| `crates/cli` | the `nvd` binary |
| `apps/gui` | the `nvd-app` window: a Tauri front end with no logic of its own below it ([gui.md](docs/engineering/gui.md)) |
| `crates/*/tests` | per-feature integration tests; captured HTML in `tests/fixtures/<site>/` |
| `docs/guide` | plain-language docs for users |
| `docs/engineering` | the deep dive: architecture, format, fetch, sources, testing, **gotchas** |

## Before you change something

- **Nothing from a real novel is committed.** Not a line of prose, not a title,
  not an author's name, not a work's address. A fixture, a doc example or a test
  carries **made-up** words in the shape of the real thing: the site's own markup
  and vocabulary stay exactly as captured, and the work's own words and ids are
  replaced with obvious placeholders
  ([testing.md](docs/engineering/testing.md#fixtures)). The real capture stays in
  the gitignored `captures/`, and the works a live test runs against stay in the
  gitignored `fixtures.local.json`.
- Read [docs/engineering/gotchas.md](docs/engineering/gotchas.md) first. It is the
  cheapest page in the repository, and it is append-only: if something surprises
  you, add a row.
- The on-disk format is a **contract**
  ([format.md](docs/engineering/format.md)). Changing it means bumping
  `FORMAT_VERSION` and saying so.
- An adapter must not let a site's quirks leak outward: decode them into the node
  model at fetch time.
- Do not weaken, skip, or delete a test to get green. If a test is wrong, fix the
  test — and say plainly that it was wrong.
