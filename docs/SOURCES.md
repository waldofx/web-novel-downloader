# Sources

Roadmap of supported sites, in priority order. One adapter may cover several sites when they share a platform.

## Priority

### P0 — Japanese web novels (primary target)

| Site | Platform notes | Adapter difficulty |
| --- | --- | --- |
| `ncode.syosetu.com` | **Narou** (小説家になろう). Largest JP web-novel site. Ruby encoded in plain text as `｜漢字《かんじ》`; uses `《》`/`（）` as markup. | Medium |
| `novel18.syosetu.com` | Narou's R18 sibling (**Nocturne**, ノクターンノベルズ). Believed to share Narou's backend and HTML — one adapter should cover both. **Adult — gated behind `content_rating`.** | Low (if shared) |
| `kakuyomu.jp` | **Kakuyomu** (カクヨム), Kadokawa. Different platform; has a mobile/app API worth investigating. | Medium |

### P1 — Japanese, second wave

| Site | Platform notes | Adapter difficulty |
| --- | --- | --- |
| `syosetu.org` | **Hameln** (ハーメルン). Fan-fiction oriented, different platform from Narou. | Medium |
| `h.syosetu.org` | Appears to be Hameln's adult sibling. **To verify** whether it shares Hameln's markup. **Adult — gated.** | Low (if shared) |
| `pixiv.net/novel` | Pixiv novels. Requires **login cookies** and/or its API for many works. **Adult content possible — gated.** | High |

### P2 — TODO

| Site | Notes |
| --- | --- |
| Royal Road | Clean, well-structured HTML; straightforward adapter. |
| Scribble Hub | Straightforward; similar to Royal Road. |
| AO3 (`archiveofourown.org`) | Scraper-friendly, has a download interface. |
| FanFiction.net | Actively fights scrapers; expect difficulty. |

### P3 — TODO (legal/ToS sensitive)

| Site | Notes |
| --- | --- |
| Wuxiaworld | Check ToS before implementing. |
| Webnovel | Check ToS before implementing. |
| Other CNY/MTL aggregators | Case-by-case; many are low quality or hostile. |

## Shared-platform insight

Two pairs of sites likely share codebases, so one adapter each should suffice:

- `ncode.syosetu.com` + `novel18.syosetu.com` (Narou family)
- `syosetu.org` + `h.syosetu.org` (Hameln family — to verify)

If true, that's **five of the six P0/P1 sites covered by three adapters** after Pixiv.

## Adult content policy

Adult sources are supported explicitly, but gated:

- Every novel carries `content_rating: "general" | "adult"` in `manifest.json`.
- The CLI requires an explicit opt-in flag to fetch adult sources.
- Adult-only sites are never fetched implicitly (e.g. by "download everything").

This is purely a content-rating field in the data model. It has nothing to do with access control — the tool never bypasses paywalls, logins, or age gates.

## Cookies and authentication

Some sources (notably Pixiv) need a logged-in session. Rules:

- Cookies live in a **per-domain store that is gitignored**, never in the repo.
- The tool only ever uses credentials for accounts the user owns.
- Cookie handling is per-domain so one site's session never leaks to another.

## Test fixtures

Concrete example works used to develop and verify adapters (including adult titles) are kept in a **gitignored** local file, `fixtures.local.json`, so the site roadmap here stays public-safe and personal reading lists are not published. Copy `fixtures.local.example.json` and fill it in.

## Adapter checklist

For each new source:

- [ ] Detect the site from a novel URL (`Source::matches`).
- [ ] Parse novel metadata (title, author, tags, synopsis, cover, rating).
- [ ] Enumerate the chapter list, in reading order, with stable IDs/URLs.
- [ ] Parse one chapter into the node model, **including ruby, emphasis, scene breaks, notes, and images**.
- [ ] Handle pagination if chapter text is split across pages.
- [ ] Handle the site's text encoding correctly.
- [ ] Set `content_rating`.
- [ ] Add rate-limit guidance (min delay) for the host.
- [ ] Add a fixture-based test.
- [ ] Update the priority tables above.
