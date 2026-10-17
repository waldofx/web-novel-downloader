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
of the convention (with Windows-specific `gh` gotchas) is kept in the author's
global OpenCode `git-workflow` skill; this document is the reference for *this*
project. Do not assume the same convention in other repositories.

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

Currently commits go directly to `main`. The planned shape is one branch per
issue (`issue/<nnn>-<short-slug>`) merged by PR; see #6.

## Building and testing

```bash
cargo build
cargo test
cargo clippy --all-targets
cargo fmt --all
```

`cargo` lives at `%USERPROFILE%\.cargo\bin\cargo.exe` on Windows if it is not
on `PATH`.

## Repo layout

- `crates/core` — data model, on-disk storage, fetch policy, download orchestration
- `crates/sources` — per-site adapters
- `crates/cli` — the `nvd` binary
- `DESIGN.md` — architecture and the on-disk format
- `docs/SOURCES.md` — source priority and adapter notes

## Ground rules

- Respect each site's `robots.txt` and terms; do not circumvent paywalls.
- Adult sources are gated behind `--allow-adult`.
- Only text nodes are ever translated; structure and ruby are preserved.
- Never commit `fixtures.local.json`, `cookies.json`, `.env`, `exports/`, or `raw/`.
