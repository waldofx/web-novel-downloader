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
| `novel18.syosetu.com` | ✅ | Narou's R18 sibling (**Nocturne**, ノクターンノベルズ). Shares the backend and HTML, so one adapter covers both; the host decides the rating. | Low |
| `kakuyomu.jp` | ✅ | **Kakuyomu** (カクヨム), Kadokawa. A Next.js app: metadata and the episode list are read from the page's `__NEXT_DATA__` snapshot, because the rendered HTML holds only the first chapter's episodes. Real arcs (章). See [Kakuyomu notes](#kakuyomu). | Medium |

## P1 — Japanese, second wave

| Site | Status | Platform notes | Difficulty |
| --- | --- | --- | --- |
| `syosetu.org` | ✅ | **Hameln** (ハーメルン). Fan-fiction oriented; a different platform from Narou. Cloudflare challenge: reachable with a user-provided session (#31). Captures still needed. | Medium |
| `h.syosetu.org` | ✅ (assumed shared) | Appears to be Hameln's adult sibling. **To verify** whether it shares Hameln's markup. **Adult.** Challenge-gated too. | Low (if shared) |
| `pixiv.net/novel` | ✅ | **Pixiv** novels, read from the site's **JSON API** rather than from its pages, which are a JavaScript app over that API. A novel's whole text arrives in one answer, with `[newpage]` where the reader slices it. Adult works are marked per work (`xRestrict`). See [Pixiv notes](#pixiv). | High |

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

If the second holds, every P0/P1 site is covered: three adapters for the two
syosetu families, one for Kakuyomu, and one for Pixiv — which shares a platform
with nobody.

## Kakuyomu

Kakuyomu is where the node model stops being Narou-shaped and starts being a
model. It differs from Narou in every way that matters, and none of it reaches
the manifest:

| | Narou | Kakuyomu |
| --- | --- | --- |
| furigana | plain-text notation, `｜漢字《かんじ》` | real `<ruby><rb>…</rb><rp>（</rp><rt>…</rt><rp>）</rp></ruby>` |
| emphasis | `《《強調》》` in the text | `<strong>` / `<em>` |
| arcs | inferred from `第N章` headings in the `arc` column | real chapter dividers, titled in the episode list |
| numbering | in the URL (`/n0608gd/3/`) | opaque ids, so chapters are numbered from reading order |
| chapter dates | JST, no zone (`…T23:23:00+09:00`) | a UTC instant (`…T22:08:26.000Z`) |

### The snapshot is the source, not a shortcut

A Kakuyomu work page is a Next.js app. It ships a `__NEXT_DATA__` snapshot
holding everything the client will render — and it renders almost none of it
server-side. The work used to write these tests shows **seven** episode links in
its HTML and **1069** episodes in its snapshot, grouped into 23 arcs, and the
page's own `TableOfContentsChapter` reference for an un-arc'd work has a
*deliberately empty id*.

So the episode list comes from the snapshot and the body comes from the episode
page's HTML (episode pages carry no snapshot at all). The live test asserts the
episode count with a number in the message: if the adapter ever goes back to
reading HTML, it fails as "you downloaded one chapter of a thousand".

A snapshot that lists a chapter or an episode it does not contain is a truncated
payload and is an error, not a short novel.

### Rating

Kakuyomu is an **all-ages** site: there is no adult sibling host, so
`content_rating` is always `general`. Per-work `isSexual`, `isViolent` and
`isCruel` flags are *content warnings* shown on an all-ages site, not an age
gate. Mapping them to `adult` would refuse to fetch most of the catalogue while
protecting nobody, so they are not mapped — see the open gaps below.

### Politeness

Kakuyomu's `robots.txt` sets `Crawl-delay: 1` for `ClaudeBot`, and disallows the
`/read` suffix of an episode URL, `/shared_drafts/` and the login paths. We fetch
work and episode pages only, and the shared fetcher already leaves 1s between two
requests to the same host — which is what that crawl delay asks for — so no
per-host override is needed.

### Known gaps

- **Illustrations inside a body are skipped** (issue #18 owns images: fetching
  them, storing them, resolving URLs).
- **Paywalled episodes** (Kakuyomu Pay) would surface as a parse error rather
  than a partial chapter. No work tested has one.

## Pixiv

Pixiv is the site that made this list stop being about HTML. Its novel pages are
a JavaScript app; the data behind them is a JSON API, and that API is what the
adapter reads. Nothing here parses markup, which is why the module is one file of
API types, one of text markup, and no DOM helpers at all.

| | What the site has | What the adapter makes of it |
| --- | --- | --- |
| the work | one novel, or the series it belongs to | a novel URL that names one novel of a series **expands to the series** |
| a chapter | one novel: its whole text is in one answer | one chapter per novel, numbered in reading order |
| reader pages | `[newpage]`, four of them in a five-page chapter | the pages are **joined**; a page is not a chapter |
| furigana | `[[rb:base > reading]]` | `Inline::Ruby` |
| links | `[[jumpuri:label > url]]` | `Inline::Link` |
| the author's dividers | `[chapter:Title]` | a level-2 heading — never a chapter split |
| the author's images | `[uploadedimage:ID]`, with the URLs in the same answer | a `Block::Image` |
| somebody else's art | `[pixivimage:ID]` / `[pixivimage:ID-N]` | one `/ajax/illust/{id}` request; unresolved, it stays literal |

### The API, and not the pages

Every `/ajax/` answer is the same envelope — `{"error": …, "message": …,
"body": …}` — and a failed request is **HTTP 200 with `error: true`**. The
envelope is therefore read before the payload and the status is never consulted:
a request for a novel that is gone is a success as far as HTTP is concerned.

Two things about the payloads cost an afternoon each if they are assumed:

- **`null` is not a missing field.** A novel with no embedded images says
  `"textEmbeddedImages": null`, and `#[serde(default)]` does not cover that — it
  covers a field that is absent. Without a `null` decoder, whole novels fail to
  parse.
- **ids are strings in some answers and numbers in others.** A novel's own `id`
  is a string, its `seriesNavData.seriesId` is a number, a series' `firstNovelId`
  is a number. Fields that carry an id decode through one helper rather than
  through a guess.

### Reading a series

A series listing (`/ajax/novel/series_content/{id}`) arrives **newest-first** and
pages by `last_order`, which means "the entries below this one" — so the series
is walked from its end back to chapter one and sorted at the end. The first page
must not carry `last_order` at all: an empty value is an HTTP 400, and `limit`
has a ceiling of 10, which is also the pager's stop condition (a page that is not
full is the last one).

`seriesContentOrder` is the site's own numbering, used to page and sort. Chapters
are numbered `1..n` in reading order instead — the same rule Kakuyomu's
position-numbered episodes get, because a number the *site* assigns is not a
count of anything this tool promises.

### Where the URLs point

The manifest holds two kinds of URL for pixiv, and they are deliberately not the
same one:

- `source.novel_url` is the **page a human opens** (`/novel/show.php?id=…`, or the
  series page). `nvd update` re-resolves it, so it has to be a URL the adapter
  can read again.
- a chapter's `url` is the **JSON API address its body comes from**
  (`/ajax/novel/{id}`). It is fetched exactly as it stands — which keeps the
  invariant every other adapter has, that the manifest's URL *is* what gets
  fetched, and is what lets `crates/sources/tests/blocked.rs` drive pixiv's
  chapter path over a socket like the HTML adapters.

### Rating, and sessions

`xRestrict` is `1` for adult and `0` otherwise, per work — so `content_rating` is
refined from what each work states, and nothing is gated on it. The site enforces
its own age gate.

Most works read without a session. A work the account may not read comes back
with `isLoginOnly: true` and an empty body, and the adapter says so and points at
`nvd session save` rather than writing an empty chapter.

### Politeness

`robots.txt` allows `/ajax/novel/*`. It disallows some `/ajax/illust/*` paths
(`…/recommend/init`), `/novels/unlisted/*`, `/en/search/` and the bookmark and
follower paths; none of them is fetched. No `Crawl-delay`, so the shared 1s
per-host delay applies.

Images live on `i.pximg.net`, which answers **403 to a request without
`Referer: https://www.pixiv.net/`** — and its own `robots.txt` disallows Google's
crawlers, not us. Fetching them needs a per-source referer (#18), and a cover is
not an exception.

### Known gaps

- **`[pixivimage:…]` is best effort.** Each reference costs one request, and a
  reference that cannot be resolved — the illustration deleted, the reference
  malformed — is left in the text exactly as the author wrote it rather than
  costing the reader the chapter.
- **Images are found, not fetched** (#18 owns that for every source). An
  in-text image and a cover leave this adapter as URLs; the fetcher that stores
  them in `images/` and rewrites the chapter is next. See
  [format.md](format.md#a-url-is-not-a-path).
- **A series with no published novels** is reported as a page that is not the
  site, because a manifest with no chapters would otherwise download nothing and
  call it success.

## Adult content policy

Adult sources are supported explicitly, and **nothing is gated**:

- Every novel carries `content_rating: "general" | "adult"` in `manifest.json`.
- Nothing is gated on it: adult works download like any other. Each site
  enforces its own age gate, and the tool does not bypass it.

It is a content-rating field in the data model, and nothing more. It is not
access control: the tool never bypasses paywalls, logins, or age gates.

## Test fixtures

Concrete example works used to develop and verify adapters — including adult
titles — are kept in a **gitignored** local file, `fixtures.local.json`, so this
roadmap stays public-safe and personal reading lists are not published. Copy
`fixtures.local.example.json` and fill it in.

## Adding an adapter

1. Create `crates/sources/src/<site>/`, implementing [`Source`](../engineering/architecture.md#the-adapter-interface).
   Use one file per job once it needs more than one (see `narou/`, `kakuyomu/`).
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
- [ ] Enumerate the chapter list, in reading order, with stable URLs — plus the
      site's own arc/volume dividers, if it has any. Never infer a divider from a
      chapter's title.
- [ ] Parse one chapter into the node model — **including ruby, emphasis, scene
      breaks, notes, and images**.
- [ ] Handle pagination if the text is split across pages — including *lazy*
      pagination, where the HTML holds less than the page's own data.
- [ ] Handle the site's text encoding correctly.
- [ ] Set `content_rating`.
- [ ] Add rate-limit guidance (minimum delay) for the host.
- [ ] Add fixture-based unit tests, and one `#[ignore]`d live test.
- [ ] Update the tables on this page.
