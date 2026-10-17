# The guide

Plain-language documentation for **people who want to read web novels
offline**. No Rust, no JSON, no jargon.

## What this is

A tool that saves a web novel — Japanese ones first — to your own computer,
keeping the formatting that makes it readable:

- **furigana** (the small reading printed above kanji) stays attached to the
  right characters instead of being flattened into the text;
- **emphasis, scene breaks, chapter titles and author's notes** stay where the
  author put them;
- **illustrations** come along too.

It saves into a folder you own, and from there produces an **EPUB** for an
e-reader, plain **TXT**, or **Markdown**.

## Why not just use a browser extension?

Because you read in more places than a browser: an e-reader on a train, a phone
with no signal, a translation tool. One tidy folder works everywhere — and a
tool that understands the format can later *translate* the novel without
destroying the furigana and the layout.

## How it behaves

- **Slowly and politely.** It waits between requests, says who it is, and
  respects a site's `robots.txt`. It is not a scraper that hammers a server.
- **Resumable.** If you stop it, or the connection drops, run the same command
  again — it re-reads the chapter list and fetches only what is missing.
- **Honest about content.** A work's rating is recorded; it is never refused
  on it. Each site enforces its own age gate, and the tool does not bypass it.

## What it will not do

- It will not bypass paywalls, logins, or age gates. It only ever uses access
  you already have.
- It will not publish or share what you download. It is for your own reading.
- It does not translate yet. Translation is a separate stage that will consume
  the same files.

## Where to go next

- [getting-started.md](getting-started.md) — build it, run it
- [sessions.md](sessions.md) — the cookie jars for challenged sites
- [faq.md](faq.md) — the questions that keep coming up
- The [engineering docs](../engineering/) — for the code itself
