# Engineering deep dive

How this is built: for developers and AI agents changing the code. If you just
want to use the tool, read the [guide](../guide/) instead.

Read in this order the first time:

1. [architecture.md](architecture.md) — what the pieces are, and why they are
   split that way
2. [format.md](format.md) — the on-disk and interchange format, and the
   invariants that must not be broken
3. [fetch.md](fetch.md) — politeness, retries, caching, `robots.txt`
4. [sessions.md](sessions.md) — the user-owned cookie store for challenged sites
5. [sources.md](sources.md) — the site roadmap, and how to add an adapter
6. [export.md](export.md) — EPUB, TXT and Markdown, and why the output is
   reproducible
7. [testing.md](testing.md) — how tests are run, and which ones need the network
8. [gotchas.md](gotchas.md) — **read this before debugging anything**

## The one rule

The downloader's output is a **stable, documented interchange format**. Every
exporter, and the future translation stage, consumes that format. Nothing
downstream may need to re-scrape a site, and no site's quirks may leak past the
adapter that understands them.

When a choice threatens that rule, the rule wins.
