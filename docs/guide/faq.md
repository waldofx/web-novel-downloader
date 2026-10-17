# FAQ

### Is this legal?

It downloads pages you can already read, for personal offline reading, slowly
and politely. It does not bypass paywalls, logins, or age gates, and it does not
redistribute anything. Whether you may keep an offline copy is between you and
the site: most sites allow personal use and forbid redistribution.

### Why save JSON instead of just copying the HTML?

Because a browser-shaped copy is hard to *do* anything with. The structured form
is what lets the same story be re-rendered as EPUB, TXT, or Markdown, and what
will let translation replace the text without destroying furigana and layout.
See the [format](../engineering/format.md).

### Where is the furigana?

Kept as data, attached to its base word. The EPUB renders it as real `<ruby>`;
TXT and Markdown have to fall back to something like `漢字(かんじ)`, which you can
turn down to just `漢字` if you prefer.

### Can I download a novel from somewhere other than Narou?

Yes — Narou (`ncode.syosetu.com` and its R18 sibling) and Kakuyomu
(`kakuyomu.jp`). Hameln and Pixiv are next, and English sites (Royal Road,
Scribble Hub, AO3, FanFiction.net) are on the list. Paste any URL from a work,
including the page of the chapter you are reading.

### Can it fetch adult (R18) works?

Yes, but only when you ask: `--allow-adult`. Adult sources are never included by
accident.

### Do I need an account?

For most sites, no. Pixiv generally needs a logged-in session; cookies stay on
your machine, outside the repository, and are never published.

### Why is it so slow?

On purpose. A site that gets hammered will block the tool — or everyone. The
delay between requests is not a limitation to be tuned away.

### It stopped halfway. Do I start over?

No. Run the same command again: it re-reads the chapter list, checks each
chapter's recorded hash, and fetches only what is missing — or what changed on
disk since it was written.

### Where did the author's notes go?

They are kept, separately from the chapter body, so you can keep them, hide
them, or translate them independently.

### It says a filter page or a Cloudflare challenge answered. What now?

Nothing was downloaded, and the message names what answered instead, with its
HTTP status:

- **an ISP content-filter page** (`HTTP 200`) — your network redirected the
  request to a block notice, because a filter is in force for that domain. The
  tool will not pretend the notice was a novel, and getting around the filter
  (another network, a VPN) is your call rather than something it does for you.
- **a Cloudflare challenge** (`HTTP 403`) — the site checks that its visitors
  run JavaScript, and a downloader is not a browser. Sources behind such a check
  need a session the tool does not have yet.

Either way, re-running once the cause is gone is safe: chapters already on disk
are verified by hash and skipped.

### Why does a filtered site hang instead of saying so?

Because the filter usually hijacks the *name*, not the connection. An `https://`
request then lands on a box with no valid certificate and times out rather than
answering, so you get a network error naming the URL and the timeout. The
`http://` form of the same URL is where the block notice appears — and that is
the one the tool recognises and reports. See
[gotchas](../engineering/gotchas.md).
