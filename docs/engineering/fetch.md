# Fetching: politeness, retries, robots, cache

Everything on this page lives in `crates/core/src/net/`. This is the only code
in the project that talks to other people's servers, and it is written to be
boring on purpose.

## Policy

`FetchPolicy` is the single place the knobs live. The defaults are deliberately
conservative: a site can be large, and getting blocked helps nobody.

| Field | Default | Meaning |
| --- | --- | --- |
| `min_delay` | 1s | minimum gap between two requests to the **same host** |
| `max_concurrency` | 3 | maximum in-flight requests across all hosts |
| `max_retries` | 3 | attempts per request before giving up |
| `user_agent` | `web-novel-downloader/<version> (+repo URL)` | sent so a site can identify and rate-limit us properly |
| `respect_robots` | `true` | consult `robots.txt` first |

Timeouts are constants, not policy: `CONNECT_TIMEOUT` is 10s and
`REQUEST_TIMEOUT` is 30s.

### Why `max_concurrency` is not used yet

The fetcher is a **sequential loop**, because the per-host delay dominates: one
request a second, forever, is the ceiling no matter how many sockets are open.
`max_concurrency` is reserved for the multi-host/CDN case — a source that fans
out over several hosts, or a fetch that is not to the host we are already
delaying. It is intentionally unused.

Do not wire it up without a host-aware scheduler: two concurrent requests to the
same host would overlap and undo the politeness the policy promises.

## Retries

A response is retried only when retrying can plausibly help:

| Retried | Not retried |
| --- | --- |
| `408` request timeout, `425` too early, `429` too many requests, `500`, `502`, `503`, `504` | every other status, including `404` and `403` |

`is_retryable_status()` is the whole list, in one place.

Backoff is exponential: `BACKOFF_BASE` (1s), doubling each attempt, capped at
`BACKOFF_CAP` (60s). A `Retry-After` header is honoured in its **delta-seconds**
form only; the HTTP-date form is ignored, because parsing it would pull a
calendar dependency in for a header that is rarely sent.

`404` is final on purpose. The live test asserts both that it is a `404` **and**
that it returned quickly: a "retry" that turns a missing page into a long hang
is indistinguishable from a hang, and users blame the tool.

## robots.txt

`RobotsRules` parses `robots.txt` and answers `is_allowed(user_agent, path)`,
with `crawl_delay(user_agent)` available for a site-imposed delay. It is
consulted before a request, and a disallowed path fails with
`Error::RobotsDisallowed` rather than quietly returning something else.

Two details that matter:

- The group that applies is the one for **our own** user-agent, falling back to
  `*` — not the rules for some other crawler.
- The escape hatch is `FetchPolicy::respect_robots = false`, surfaced to users as
  `nvd download --no-robots`, which says out loud that it is ignoring the file. It
  exists for unusual cases, it is not the default, and it is not a licence to
  ignore a site's wishes. See [Stance](#stance).

## Pages that are not the site

Two answers arrive where a novel page was expected, and neither should reach an
adapter. `net::blocked` recognises them, so the failure names the network or the
site's bot policy instead of blaming a parser for markup it never saw.

| What answered | Status | How it is recognised |
| --- | --- | --- |
| an ISP content filter — Trustpositif/Komdigi's `SITUS DIBLOKIR` notice, First Media's "Internet Sehat" | `200`, which is the insult | a whole phrase from the page: `SITUS DIBLOKIR`, `DIBLOKIR OLEH PEMERINTAH INDONESIA`, `aduanpseprivat@komdigi.go.id`, `internetsehat.firstmedia.com` |
| Cloudflare's browser challenge | `403` with `Cf-Mitigated: challenge` | the `Just a moment...` title **and** Cloudflare's challenge script (`challenge-platform` or `__cf_chl`) — or that header alone, when there is no readable body |

The markers are whole phrases, matched case-insensitively, and only within the
first 16 KB of a body. `BLOCKED` or `DIBLOKIR` on its own would match a novel;
the phrases do not. The captures behind them live in
`crates/core/tests/fixtures/blocked/`.

Either case fails as one error, carrying the status and a quote from the page:

```
error: could not download https://h.syosetu.org/novel/288118/: Cloudflare served its browser challenge instead of the site (HTTP 403): the page says "Just a moment..."
```

(`error: could not download <url>:` is the CLI's own prefix; the library's
message begins at "Cloudflare".) The status is in the text deliberately: a filter
answers `200`, and a reader deserves to see that rather than a success that turns
into "could not parse".

Reading the body of a non-retryable status is the only cost this adds, and it is
not a retry — a challenge is never asked twice. `is_retryable_status()` still
keeps `403` final.

### Ways past, and the ones we refuse

The challenge *can* be passed: a real browser runs the script and is let in.
Using what it leaves behind — a `cf_clearance` cookie sent with the same
user agent — is a decision about the tool's identity rather than a bug fix, and
it is the same decision [Pixiv #20][pixiv] is waiting on. Until that is settled
this layer reports and stops. It does not spoof a browser user agent, hammer the
challenge, or pretend a block page was a novel: a Chrome user agent was tried
against the real site and got the same `403`, which is exactly why that is not
the answer.

[pixiv]: https://github.com/waldofx/web-novel-downloader/issues/20

## The raw cache

`RawCache` stores fetched bodies under a novel's `raw/` directory. It exists for
one reason: **adapters rot**. When a site changes its HTML, the saved pages can
be re-parsed offline while the adapter is being fixed — with no re-download and
no second visit to a site that has already been polite to us.

## Cookies

A per-domain store, **gitignored**, for login-gated sources such as Pixiv. Rules:

- Only accounts the user owns. The tool never borrows a session it was not given.
- Per-domain, so one site's session can never leak into another's.
- Never committed, never logged.

## Stance

Respect `robots.txt` and each site's terms of service. No paywall
circumvention. Personal offline reading only.
