# Contributing

## Issue-driven commits

Every non-trivial change belongs to a GitHub issue describing a **scope of
work** (a package that can be finished and closed — not a single commit).

| Thing | Format | Example |
| --- | --- | --- |
| Issue title | `[type](scope): summary` | `[feat](narou): Narou adapter - TOC, chapters, ruby` |
| Commit subject | `#nnn [type](scope): summary` | `#2 [feat](narou): parse table of contents with arcs` |

The final commit that finishes an issue adds `Closes #nnn`.

This project has adopted the author's issue-driven workflow, so it applies here
regardless of what the commit history looks like. The general, portable version
of the convention (with the Windows-specific `gh` gotchas) is kept in the
author's global OpenCode `git-workflow` skill; this document is the reference for
*this* project. Do not assume the same convention in other repositories.

### Types

`feat` `fix` `chore` `docs` `test` `refactor` `perf`

Labels mirror the type: `feat` → `enhancement`, `fix` → `bug`, `docs` →
`documentation`, and the rest use same-name labels.

### Scopes (closed list)

```
core  fetch  download  sources  export  cli  gui  ext
narou  kakuyomu  hameln  pixiv
docs  build  deps  skills  tests
```

Adding a source adapter adds its site name to this list.

### Branches

The repository default branch is `development`.

| Branch | Meaning |
| --- | --- |
| `main` | **Released.** Moves only by a `development` → `main` PR. Protected: PR required, no direct pushes, no force pushes. |
| `development` | **Integration.** All work lands here. |
| `issue-<nnn>` | One per issue, branched from `development` and merged back into `development` by PR. |

```bash
git switch development && git pull
git switch -c issue-12
# ... work, commit with "#12 [feat](scope): ..." ...
git push -u origin issue-12
gh pr create --base development --fill
```

Never branch from `main`, and never open an issue PR against `main`. Issues close
automatically when their PR merges into `development` (the default branch);
`main` only receives work that is already finished and tested.

## Test-first

Write the **failing test first**, watch it fail for the intended reason, then
implement. Docs come last, once the tests pass — but in the *same* change.

- Unit tests live in a `#[cfg(test)] mod tests` in the file under test.
- Integration tests live in `crates/<crate>/tests/<feature>_<kind>.rs`; Cargo
  compiles only top-level files there, so a subfolder is a module, never a test
  target. Shared helpers go in `tests/common/mod.rs`.
- Captured HTML goes in `tests/fixtures/<site>/`, loaded with `include_str!`.
- **Network tests are `#[ignore]`d.** `cargo test` must pass offline.

The portable version of this convention is the author's global OpenCode
`test-first` skill. See [testing.md](docs/engineering/testing.md) for the details
that apply here.

## Documentation

Two tiers, and they do not mix:

| Tier | Path | Reader |
| --- | --- | --- |
| guide | `docs/guide/` | someone who wants to **use** the tool — plain language, no jargon |
| deep dive | `docs/engineering/` | someone who will **change** the code — decisions, invariants, failure modes |

Rules:

- **One canonical place per fact.** Link, never copy.
- Docs describe the **current** truth. Planned work is marked as planned.
- Docs land in the same change as the code, as the **last** commit.
- Anything surprising goes in [gotchas.md](docs/engineering/gotchas.md), which is
  **append-only**. Never delete an entry.

The portable version of this convention is the author's global OpenCode `docs`
skill.

## Building and testing

```bash
cargo build
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

`cargo` lives at `%USERPROFILE%\.cargo\bin\cargo.exe` on Windows if it is not on
`PATH`. If the change touched fetching or a source adapter, also run the ignored
live tests and paste the output:

```bash
cargo test -p wnd-sources --test narou_live -- --ignored --nocapture
```

## Repo layout

- `crates/core` — the site-independent model, storage, and network layer
- `crates/sources` — per-site adapters, one folder each
- `crates/download` — orchestration: resolve a URL, fetch what is missing, record hashes
- `crates/export` — EPUB, TXT and Markdown, rendered from a downloaded directory
- `crates/cli` — the `nvd` binary
- `docs/guide` — user documentation
- `docs/engineering` — architecture, format, fetch, sources, testing, gotchas

## Ground rules

- Respect each site's `robots.txt` and terms; do not circumvent paywalls.
- Adult sources are gated behind `--allow-adult` and are never fetched implicitly.
- Only text nodes are ever translated; structure and ruby are preserved.
- The on-disk format is a contract — bump `FORMAT_VERSION` if you change it.
- Never commit `fixtures.local.json`, `cookies.json`, `.env`, `exports/`, or `raw/`.
