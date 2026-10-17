# Sessions and cookies

Some sites answer an ordinary client with Cloudflare's browser challenge
(Hameln, Pixiv). The tool will not solve a challenge, borrow a browser, or spoof
its way past one — so the only honest way in is a credential *you* provide
deliberately: a **session**.

A session is a cookie jar you exported from your own browser and handed over by
name. The tool never borrows a browser, never clears a challenge, and never
does anything to a session you did not give it. Without one, a challenged site
keeps failing loudly.

## What a session is

- a **name** ("pixiv", "syosetu", "hameln"),
- the **hosts** it applies to,
- a **cookie jar**,
- and when it was saved, so staleness can be seen and said out loud.

## Managing sessions

```
nvd session save <name> <file>      # import a Netscape cookies.txt
nvd session list                    # name, hosts, saved time, earliest expiry
nvd session delete <name>           # forget one
```

- `--session-dir <path>` on any command overrides where sessions live
  (default: the user's own data directory — `%LOCALAPPDATA%\web-novel-downloader\sessions`
  on Windows, `~/Library/Application Support/...` on macOS,
  `$XDG_DATA_HOME`/`~/.local/share` elsewhere).
- One JSON file per name, `0600` on Unix, written atomically.
- `save` with an existing name **replaces** it — that is the refresh gesture.
- `list` shows name, hosts, saved time and the earliest expiry. **Never a value.**
- Delete the files by hand if you prefer.

## Use it

```
nvd download <url> --session pixiv
nvd update ./my-novel --session pixiv
```

The jar is sent **only to the hosts it names** — a `pixiv.net` session is never
offered to `i.pximg.net`.

## Getting the cookies — no extension needed

The browser's DevTools can hand you every cookie the site set, HttpOnly included,
without any third-party extension:

1. Open the site and log in, so the browser holds the clearance cookie.
2. Press **F12** → **Network** tab.
3. Reload the page (**F5**).
4. Click any request to the site's domain (e.g. `pixiv.net`).
5. In **Headers → Request Headers**, find `Cookie:` and copy the whole value.
   It looks like `name=value; name2=value2; …`.
6. Save it:

   ```
   nvd session save pixiv --cookie "name=value; name2=value2" --host pixiv.net
   ```

   `--host` scopes the session to that host. Re-export and re-run `save` to
   refresh when the session expires.

## Getting the cookies — with an extension

If you already use a cookie-export extension, export as **Netscape
`cookies.txt`** and import the file:

```
nvd session save pixiv cookies.txt
```

Every export extension writes this format, and `curl -b` reads it.

## If a download still gets the challenge

The session probably expired (they're short-lived). Re-export from the browser
and `save` again — same name replaces it.

## Notes

- The tool never borrows a session it was not given, and never does anything to
  a session the user did not hand it.
- A challenge with a session in use **blames the session** — the error names it
  and says it may have expired.
- See [engineering/sessions.md](../engineering/sessions.md) for the format and
  the rules the code keeps.
