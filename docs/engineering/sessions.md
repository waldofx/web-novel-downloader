# Sessions

Sessions are the answer to a site that answers an ordinary client with
Cloudflare's browser challenge (#17 Hameln, #20 Pixiv). A user runs the site in
their own browser, exports the cookies, and hands them to the tool by name. The
tool never borrows a browser, never solves a challenge, and never does anything
to a session it was not given.

## What a session is

- a **name** ("pixiv", "syosetu"),
- the **hosts** it applies to,
- a **cookie jar**,
- and when it was saved, so staleness can be seen and said out loud.

## Where they live

Under the user's own data directory, never the repository, never a novel
directory, never anywhere that could be committed or uploaded:

- Windows: `%LOCALAPPDATA%\web-novel-downloader\sessions`
- macOS: `~/Library/Application Support/web-novel-downloader/sessions`
- Linux / other Unix: `$XDG_DATA_HOME/web-novel-downloader/sessions`, or
  `~/.local/share/web-novel-downloader/sessions`

One file per session, named `<name>.json`, user-readable only (`0600` on Unix),
written atomically so a failed save leaves the previous session rather than half
of a new one. The directory is created on the first save; if it does not exist,
there are no sessions.

To remove them by hand: delete the files, or the whole directory. Nothing is
stored anywhere else.

## Import format

The import format is **Netscape `cookies.txt`**: every browser export extension
writes it and `curl -b` reads it, so the user's existing tooling already
produces it and nothing here has to be the only way in. Internal storage is our
own JSON; the import format is not.

## The three operations

| Operation | CLI | Desktop app (#21) |
| --- | --- | --- |
| save | `nvd session save <name> <file>` (optional `--host`) | "Add session": paste, or pick a browser export |
| list | `nvd session list` | a list with the same columns |
| delete | `nvd session delete <name>` | "Forget" |

`save` with an existing name replaces it — that is the refresh gesture.
`--session <name>` on `download` and `update` sends the jar only to the hosts it
names.

## Rules the code keeps

- **Sent only to the hosts it names.** A session for `pixiv.net` is never
  offered to `i.pximg.net`, which is a different site that happens to serve the
  images. A session with no hosts goes nowhere whatever it holds.
- **A challenge with a session in use blames the session.** A `403` that arrives
  while a jar was sent says the session may have expired and can be refreshed by
  saving it again.
- **Named, never valued.** `list` shows name, hosts, saved time and the earliest
  expiry. No value reaches a log, an error message, or `Debug`.
- **Per-user, per-session.** One file per name; concurrent saves of different
  names do not interfere.

## Where the code lives

- `crates/core/src/session/cookie.rs` — what a cookie is, and where it may be
  sent (host, path, secure, expiry).
- `crates/core/src/session/netscape.rs` — reading the `cookies.txt` export.
- `crates/core/src/session/store.rs` — `Session`, `SessionStore`, `default_dir`.
- `HttpClient::with_session` — sending the jar.
- `crates/cli/src/main.rs` — the `session` subcommand and `--session`.

## Tests

- Unit tests in `session/cookie.rs` and `session/netscape.rs` (pure logic).
- Unit tests in `session/store.rs` (the platform rules for `default_dir`).
- Integration tests in `crates/core/tests/session.rs` (round trip, permissions,
  name safety, redaction) and `crates/core/tests/session_fetch.rs` (sending
  only to named hosts, blaming the session on a challenge).
- CLI tests in `crates/cli/tests/cli.rs`.

The ignored network tests are not affected: a session is a credential, not a
network dependency.