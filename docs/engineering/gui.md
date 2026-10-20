# The desktop app

`apps/gui` — a window over the same library the command line uses, so that a
reader never has to open a terminal. Paste a novel's address, choose a folder,
watch the chapters arrive, and take an EPUB away.

It is deliberately the **thinnest possible layer**. Everything it does, `nvd`
already does; the app adds a window, a live list, a folder picker, and a session
manager. Nothing in `wnd-core`, `wnd-sources`, `wnd-download` or `wnd-export`
knows it exists.

## Three layers

| File | Holds | Tested by |
| --- | --- | --- |
| `src/job.rs` | what the window *does*: the request types, the event types, the report, validation, the session store, and calls into `wnd-download`/`wnd-export` | `src/job.rs` unit tests, `tests/job.rs` |
| `src/lib.rs` | the Tauri commands, the one-job rule, the thread, and the event channel | `src/lib.rs` unit tests |
| `ui/` | `index.html`, `style.css`, `app.js` — plain files, no npm, no framework | `tests/ui.rs` |

`job.rs` has **no Tauri types in it at all**. That is the point: the interesting
half — which is "ask the library for the right thing and describe what happened"
— can be tested without a window, and does not become untestable the day someone
wants a different front end. The Tauri half is small enough to read in one
sitting.

The types in `job.rs` are `Serialize`/`Deserialize` because JSON is the wire to
the webview, and for no other reason.

## The event stream

One channel, `nvd://progress`, carrying one shape per thing that happened. Every
event is tagged with `kind` and its fields are camelCase, so the front end
switches on `kind` and reads `event.toFetch` without a mapping table:

| `kind` | Fields | When |
| --- | --- | --- |
| `planned` | `title`, `chapters`, `toFetch` | the index has been read |
| `fetched` | `index`, `title`, `bytes` | a chapter was written |
| `skipped` | `index`, `title` | it was already there |
| `checked` | `index`, `title` | it was fetched to compare, under "check for changes" |
| `imageFetched` | `file`, `bytes` | a picture landed in `images/` |
| `imageMissing` | `url`, `reason` | a picture could not be fetched; the chapter keeps its URL |
| `removed` | `index`, `title` | the index no longer lists a chapter the folder has |
| `exported` | `format`, `path`, `bytes` | an export was written |
| `done` | `report` | the run finished |
| `failed` | `message` | it stopped |

`done` and `failed` are emitted by `lib.rs`, not by `job.rs`: `job::download`
*returns* the report, and the layer that owns the channel wraps it. A run that
failed therefore cannot also say it finished.

The titles travel with the indices for the same reason they do in `Progress`: a
window that says only "chapter 43" is not something a person can read, and a
front end should not have to open the directory it is writing to in order to
label its own list.

## One job at a time

`AppState` holds a single flag. A second download is refused with the label of
the one already running, because two runs writing one directory would fight over
the same files, and because a second fetch of the same chapter is a request to
somebody else's server for no reason.

There is **no cancel**, and that is a known gap rather than an oversight:
`wnd-download` has no cancellation point to hook. Closing the window mid-fetch
leaves the folder exactly as resumable as stopping `nvd` with Ctrl-C does.

## The rating is shown, never a gate

A work's `content_rating` is displayed when it is known, and nothing is ever
refused on it — the same rule as the command line, where the adult gate was
removed on purpose. Each site enforces its own age gate; the tool does not
bypass it, and a checkbox in a window would suggest otherwise.

## Sessions, from the window

The **Session** row carries a **Manage…** dialog. It lists what the store holds
as `SessionInfo` — name, hosts, cookie count, saved-at, earliest expiry — with
**Add** and **Forget**.

- **Add from a file**: `pick_cookies_file` is the `rfd` sibling of
  `pick_directory`, and `job::save_session_file_in` reads the Netscape export a
  browser extension wrote.
- **Add from a header**: name, site, and the `Cookie:` value. The site may be a
  host or a pasted address; `host_from_input` keeps the host, because a cookie
  domain has to be one. `job::save_session_header_in` then scopes the jar to
  that site alone.
- **Forget** is two buttons: the second names the session. A native confirm is
  not something this code depends on inside a webview, and a test could not click
  one anyway.
- A name already in the store is replaced, which is the refresh gesture the CLI
  has.
- A blank name, an empty export, an empty header, and a missing file are errors
  the dialog shows in place — never a panic, never a silent no-op.

The core's rule holds here unchanged: no value is ever rendered. The list is
summaries, and the header box is cleared the moment it is saved. The window also
never sets a site's age cookie — the user's session is their own decision, and
the site's gate stays the site's.

## What the app does *not* offer

- **`--no-robots`.** The flag exists on the command line, said out loud and once.
  A checkbox that quietly asks a site to be treated worse is a worse default than
  a flag, so the app always respects `robots.txt`.
- **Packaging.** `cargo build -p wnd-gui` produces the binary; wiring
  `tauri build` into an installer is not done, and `bundle.active` is `false`.

## Tauri, specifically

- `tauri.conf.json` sets `frontendDist: "ui"`, which is resolved **relative to
  the config file's own directory** (not the crate root). `withGlobalTauri: true`
  is what lets `app.js` call `invoke` with no bundler and no npm: the UI is three
  static files that the app embeds at compile time.
- The CSP is tight — `default-src 'self'`, no inline script or style — and the
  front end writes everything from a site with `textContent`, never as markup.
  Titles come from a stranger.
- `capabilities/default.json` grants `core:default` to the `main` window: the
  event channel and the app's own commands. Nothing else is granted, because
  nothing else is used. An unknown permission name is a **build** error, so this
  file cannot rot quietly.
- `icons/icon.ico` is a hand-built placeholder — a solid 32×32 square — and exists
  only because the Windows resource compiler wants one.
- `rfd` provides the folder dialog, on a blocking thread rather than the main one:
  the modal belongs to the operating system, and it is the one thing here that
  cannot be asynchronous. It is the same crate Tauri's own dialog plugin wraps,
  without the plugin and its permission, and `set_parent` takes the window itself,
  since Tauri's `Window` implements both handle traits.

## Running it

```bash
cargo run -p wnd-gui      # or: cargo build -p wnd-gui && ./target/debug/nvd-app.exe
```

The window needs the **WebView2 runtime** on Windows (present on current Windows
10/11; otherwise it is a free download from Microsoft).

## Tests

The fetching is `wnd-download`'s and is tested there. What is tested here is that
the front end asks for the right thing and reports it in the shape the window
reads:

```bash
cargo test -p wnd-gui
```

- `tests/job.rs` — a URL without a scheme, validation before a thread starts, the
  exact shape of every event, a download over an in-memory adapter, resume, an
  update with and without "check for changes", `info` counting what a folder has
  and what it has lost, and the session picker and manager: saving from a file
  and from a header, a pasted address reduced to a host, replace-on-same-name,
  forget, every refusal — and that a cookie value never reaches the wire.
- `src/job.rs` — the request JSON as the window sends it: camelCase, the options
  spread next to the request's own fields, and what is assumed when a field is
  absent.
- `src/lib.rs` — the one-job rule, a poisoned flag not stopping every later job,
  and a run ending in exactly one `done` or `failed`.
- `tests/ui.rs` — the **cross-language contract**: every command `app.js` invokes
  is one `generate_handler!` registers (and the other way round), the channel
  name, and every element id the front end looks up being in the page. A window
  cannot be clicked by a test, and those are the failures that would otherwise
  arrive as a blank window with an error nobody can see.

Two things are knowingly *not* covered by a committed test. The folder dialog is
an operating-system modal no test can click, called by one line in `lib.rs`. The
click handlers are `addEventListener` plus code the tests above cover, but the
click itself is not something a Rust test can perform. What *was* checked by
hand, once, in the real window, is the whole loop — see
[testing.md](testing.md#the-desktop-app-one-manual-check).
