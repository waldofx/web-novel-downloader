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

Kept as data, attached to its base word. EPUB renders it as real `<ruby>`; TXT
has to fall back to something like `漢字(かんじ)`.

### Can I download a novel that isn't Japanese?

Not yet. Narou comes first; Kakuyomu, Hameln and Pixiv are next, and English
sites (Royal Road, Scribble Hub, AO3, FanFiction.net) are on the list.

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

No. Run the same command again and chapters already fetched are skipped. (This
lands with the downloader; see [getting started](getting-started.md).)

### Where did the author's notes go?

They are kept, separately from the chapter body, so you can keep them, hide
them, or translate them independently.
