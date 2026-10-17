# Sources

The site roadmap, in priority order, plus what it takes to add one.

| Status | Meaning |
| --- | --- |
| ✅ | adapter implemented and tested |
| 🚧 | in progress |
| · | planned, not started |

## P0 — Japanese, primary target

| Site | Status | Platform notes | Difficulty |
| --- | --- | --- | --- |
| `ncode.syosetu.com` | ✅ | **Narou** (小説家になろう), the largest JP web-novel site. Ruby is encoded in *plain text* as `｜漢字《かんじ》`, and `《》` doubles as emphasis markup. | Medium |
| `novel18.syosetu.com` | ✅ | Narou's R18 sibling (**Nocturne**, ノクターンノベルズ). Shares the backend and HTML, so one adapter covers both; the host decides the rating. **Adult — gated.** | Low |
| `kakuyomu.jp` | · | **Kakuyomu** (カクヨム), Kadokawa. A different platform; has a mobile/app API worth investigating. | Medium |

## P1 — Japanese, second wave

| Site | Status | Platform notes | Difficulty |
| --- | --- | --- | --- |
| `syosetu.org` | · | **Hameln** (ハーメルン). Fan-fiction oriented; a different platform from Narou. | Medium |
| `h.syosetu.org` | · | Appears to be Hameln's adult sibling. **To verify** whether it shares Hameln's markup. **Adult — gated.** | Low (if shared) |
| `pixiv.net/novel` | · | Pixiv novels. Needs **login cookies** and/or its API for many works. Adult content possible. | High |

## P2 — TODO

| Site | Notes |
| --- | --- |
| Royal Road | Clean, well-structured HTML; a straightforward adapter. |
| Scribble Hub | Straightforward; similar to Royal Road. |
| AO3 (`archiveofourown.org`) | Scraper-friendly, and it has its own download interface. |
| FanFiction.net | Actively fights scrapers; expect difficulty. |

## P3 — TODO (legal/ToS sensitive)

| Site | Notes |
| --- | --- |
| Wuxiaworld | Check the terms before implementing. |
| Webnovel | Check the terms before implementing. |
| Other CNY/MTL aggregators | Case by case; many are low quality or hostile. |

## Shared-platform insight

Two pairs of sites probably share codebases, so one adapter each should do:

- `ncode.syosetu.com` + `novel18.syosetu.com` (Narou family) — **confirmed**
- `syosetu.org` + `h.syosetu.org` (Hameln family) — unverified

If the second holds, five of the six P0/P1 sites are covered by three adapters
(Pixiv being the exception).

## Adult content policy

Adult sources are supported explicitly, but gated:

- Every novel carries `content_rating: "general" | "adult"` in `manifest.json`.
- The CLI requires an explicit `--allow-adult` to fetch an adult source or work.
- Adult sources are never fetched implicitly — not by "download everything",
  not by a batch run.

This is a content-rating field in the data model, and nothing more. It is not
access control: the tool never bypasses paywalls, logins, or age gates.

## Test fixtures

Concrete example works used to develop and verify adapters — including adult
titles — are kept in a **gitignored** local file, `fixtures.local.json`, so this
roadmap stays public-safe and personal reading lists are not published. Copy
`fixtures.local.example.json` and fill it in.

## Adding an adapter

1. Create `crates/sources/src/<site>/`, implementing [`Source`](../engineering/architecture.md#the-adapter-interface).
   Use one file per job once it needs more than one (see `narou/`).
2. Register it in `registry()` in `crates/sources/src/lib.rs`. **Order matters**:
   the first adapter whose `matches()` returns true wins.
3. Capture real pages into `crates/sources/tests/fixtures/<site>/` and write unit
   tests against them.
4. Add a `#[ignore]`d live test in `crates/sources/tests/<site>_live.rs`, and run
   it by hand at least once against the real site.
5. Add the site name to the scope list in `CONTRIBUTING.md`.
6. Update the tables on this page.

### Checklist

- [ ] Detect the site from a novel URL (`Source::matches`).
- [ ] Parse novel metadata (title, author, tags, synopsis, cover, rating).
- [ ] Enumerate the chapter list, in reading order, with stable URLs.
- [ ] Parse one chapter into the node model — **including ruby, emphasis, scene
      breaks, notes, and images**.
- [ ] Handle pagination if the text is split across pages.
- [ ] Handle the site's text encoding correctly.
- [ ] Set `content_rating`.
- [ ] Add rate-limit guidance (minimum delay) for the host.
- [ ] Add fixture-based unit tests, and one `#[ignore]`d live test.
- [ ] Update the tables on this page.
