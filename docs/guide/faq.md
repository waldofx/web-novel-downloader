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

Yes. Adult sources (Narou's R18 sibling, Hameln's `h.` host) are handled like
any other: the rating is recorded in the manifest, but nothing is refused. Each
site enforces its own age gate, and the tool does not bypass it.

### A site says "Cloudflare served its browser challenge". Can I get past it?

See [sessions.md](sessions.md) for the full recipe. The short version: open the
site in your browser, log in, press F12 → **Network** → reload → click a request
to the site → copy the `Cookie:` header → save it with

```
nvd session save <name> --cookie "name=value; name2=value2" --host <host>
```

No extension needed. `--session <name>` on `download`/`update` then uses it.

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

### The novel has new chapters now. How do I get them?

`nvd update ./my-novel` — no URL, because the address is already in the folder's
own `manifest.json`. It fetches chapters that are new, and chapters the site says
were revised; it leaves everything else alone without a request. It exits `3` when
there was nothing to do, so a nightly job can tell "nothing new" from "it broke".

If it exits `0` but you expected new chapters, check the `gone:` lines: the
author may have removed a chapter, which an update reports and never deletes.

Some sites do not announce revisions at all (Kakuyomu does not), so an edit there
goes unnoticed by default. `nvd update ./my-novel --check` fetches every chapter
and compares it, rewriting only the ones that really differ. It costs one request
per chapter, which is why it is not what happens by default.

### Where did the author's notes go?

They are kept, separately from the chapter body, so you can keep them, hide
them, or translate them independently.

### It says the page that arrived is not a novel page. What now?

Nothing was downloaded, and the message shows what answered instead — its HTTP
status, its size, and the beginning of its own text:

```
error: could not download http://syosetu.org/novel/288118/: the page that arrived is not a novel page (HTTP 200)
  a table of contents was expected; 2741 bytes of text/html arrived instead
  it begins: "SITUS DIBLOKIR / WEBSITE BLOCKED MAAF, AKSES KE SITUS INI DIBLOKIR OLEH PEMERINTAH …"
  syosetu.org answered a URL that cannot exist with this same page, byte for byte: a DNS hijack, a captive portal, or a content filter is answering for it
```

Read the quoted text: it is your own network's, in your own language, so it
usually says plainly what is going on.

- **A block notice** (`HTTP 200`) — a filter is in force for that domain. The
  tool will not pretend the notice was a novel, and getting around the filter
  (another network, a VPN) is your call rather than something it does for you.
- **A Cloudflare challenge** (`HTTP 403`) — the site checks that its visitors run
  JavaScript, and a downloader is not a browser. It is named in so many words:
  "Cloudflare served its browser challenge instead of the site".
- **The site's own page**, and the tool still cannot read it — the site changed
  its markup and the adapter needs a fix. Re-running will not help, but that
  message is a bug report you can paste whole.

Nothing is guessed from a list of known block pages: the answer is *shown*, not
recognised, so it works the same on a network whose block page nobody here has
ever seen. The last line is the one piece of proof, and the tool goes and gets
it: it asks the host for a path that cannot exist, and if the same page comes
back, something is answering for the site.

Either way, re-running once the cause is gone is safe: chapters already on disk
are verified by hash and skipped.

### A site says "Cloudflare served its browser challenge". Can I get past it?

Some sites answer a downloader with a JavaScript challenge. The tool will not
solve it or pretend to be a browser — but if you run the site yourself in a
browser, you can hand over what the browser obtained.

1. Open the site in your browser and let it load, so the browser holds the
   clearance cookie.
2. Export the cookies for that site in **Netscape `cookies.txt`** format — every
   cookie-export extension writes this, and `curl -b` reads it.
3. Save them: `nvd session save pixiv cookies.txt`
4. Use them: `nvd download <url> --session pixiv`

The session is stored under your own data directory, one file per name, `0600`
on Unix, and is never logged or committed. `nvd session list` shows name, hosts,
saved time and the earliest expiry — never a cookie value. `nvd session delete
pixiv` forgets it, or delete the files by hand. If a download still gets the
challenge, the session probably expired: re-export and save again.

### Why does a filtered site hang instead of saying so?

Because the filter usually hijacks the *name*, not the connection. An `https://`
request then lands on a box with no valid certificate and times out rather than
answering, so you get a network error naming the URL and the timeout. The
`http://` form of the same URL is where the block notice appears — and that is
the one the tool can show you. See [gotchas](../engineering/gotchas.md).
