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
