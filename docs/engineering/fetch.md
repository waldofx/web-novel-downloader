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

A novel page is expected, and something else arrives: a DNS answer hijacked to a
filter box, a captive portal in a hotel, a transparent proxy, Cloudflare's browser
challenge, or the site itself after it changed its markup. `net::blocked` exists
so the failure names what actually answered instead of blaming a parser for markup
it never saw.

The first version of this matched **phrases** from the two filter pages we had
actually seen (`SITUS DIBLOKIR`, `internetsehat.firstmedia.com`). A list only ever
covers the past: it says nothing about a block page in another country, in another
language, from a provider we have never met. What is left is only what can be said
without knowing a vendor, a language, or a country.

### The signals

| Signal | Where it comes from | Vocabulary |
| --- | --- | --- |
| Cloudflare's browser challenge | its `Cf-Mitigated` response header | none — a header is protocol-level, and nothing else sends it |
| The answer came from another host | the final URL, after redirects | none |
| The host answers **every** URL — a path that cannot exist included — with the same page, byte for byte | one extra request, once a page has already failed to look like the site | none |
| What actually arrived | the status, size, media type, and the readable text of the body | none |

Nothing matches page text. A body is used for exactly one thing: being **shown**.

### The evidence

An adapter knows the site's shape, so an adapter is what says a page is not the
site. It names what it was looking for — `a table of contents`, `an episode body`
— raises `SourceError::NotTheSite`, and the caller, which holds the `Page`, turns
that into the user-facing message with `SourceError::with_evidence`:

```
error: could not download http://syosetu.org/novel/288118/: the page that arrived is not a novel page (HTTP 200)
  a table of contents was expected; 2741 bytes of text/html arrived instead
  it begins: "Trustpositif SITUS DIBLOKIR / WEBSITE BLOCKED MAAF, AKSES KE SITUS INI DIBLOKIR OLEH PEMERINTAH INDONESIA …"
  syosetu.org answered a URL that cannot exist with this same page, byte for byte: a DNS hijack, a captive portal, or a content filter is answering for it
```

(`error: could not download <url>:` is the CLI's prefix; the library's message
begins at "the page that arrived".) The status is in the text deliberately: a
filter answers `200`, and a reader deserves to see that rather than a success that
turns into "could not parse".

The excerpt is the load-bearing part: the page names itself, in whatever language
its operator speaks. Getting it out is more than stripping tags. The first
kilobyte of a block page is usually a tag manager and a stylesheet, so comments and
the contents of `<script>`/`<style>` go too; every tag leaves a word boundary
behind; whitespace is collapsed; and the few entities that would otherwise print
as noise are decoded. The captured First Media page in
`crates/core/tests/fixtures/blocked/` is what keeps this honest — it is scripts and
a spacer image, its excerpt is empty, and the message says so instead of quoting
CSS.

The two host findings are optional, because each has to be earned:

- a **redirect** is only worth naming when the host that answered is not the host
  that was asked;
- the **probe** is the one signal that tells "your network intercepted this" from
  "the site changed its markup", and it costs a request — so it is asked once, and
  only after a page has already failed to look like the site. It compares **byte
  for byte** on purpose: a near-match would also match the soft-404 page many sites
  serve with `200`, which echoes the URL it was asked for and nothing else.

A challenge page that arrives *without* the header is not a special case. It is a
body that is not the site's shape, reported with its own text — better evidence
than any list of strings, and it covers the challenges that are not Cloudflare's.

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
