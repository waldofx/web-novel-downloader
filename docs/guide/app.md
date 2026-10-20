# The window

A desktop app for the same job the command line does, for people who would
rather not open a terminal. Paste a novel's address, choose a folder, press
**Download**, and watch the chapters arrive. The **Library** at the top of the
window lists the novels already in a folder you keep them in.

Everything the window *writes*, [`nvd`](getting-started.md) writes too. Nothing is
hidden behind it: the same folder, the same `manifest.json`, the same chapters,
the same illustrations and the same exports. Use whichever you prefer, on the
same folder, at different times. The library is only a way of looking at those
folders; the command line has no shelf, and does not need one.

## Starting it

The app is built from the same source as the command line:

```bash
cargo run -p wnd-gui
```

or, once built, `target/debug/nvd-app.exe` (`nvd-app` elsewhere). On Windows it
needs the **WebView2 runtime**, which is already present on current Windows 10
and 11; if it is not, Microsoft offers it as a free download.

There is no installer yet. Build it and run the binary.

## What it looks like

One page: a library at the top, a panel of settings, and a list that fills as
the work arrives.

- **Novel URL** — any address into a work: its table of contents, a chapter, or
  a series. The whole work is downloaded either way. A missing `https://` is
  filled in for you, because that is half of what people paste.
- **Save into** — the folder the novel is written into, which is the folder the
  command line's `--out` names: `manifest.json`, the chapters and the images
  land directly in it. Choosing the same folder again resumes it and fetches
  only what is missing.
- **Session** — for a site that asks for a cookie. The list is whatever you have
  saved, and **Manage…** beside it adds one from a `cookies.txt` or a pasted
  `Cookie:` header, shows what each holds, and forgets one. The command line can
  do the same (`nvd session save`, see [sessions](sessions.md)). A session is
  only ever sent to the hosts it names.
- **At most how many chapters** — blank means all of them, from the first.
- **Fetch every chapter again** — ignore what is already saved. Rarely what you
  want: running the same download twice already fetches only what is missing.
- **On update, fetch each chapter to compare it** — one request per chapter, and
  the only way to notice a quiet edit on a site that does not announce revisions.
- **Write an export** — EPUB, plain text, Markdown, or several. Written into
  `exports/` inside the novel's folder, and regenerable at any time.
- **Furigana** — how ruby is written where the format has none of its own
  (`base(reading)`, or the reading dropped).
- **Include the author's notes** — the ones the author puts after a chapter.

The four buttons:

| Button | What it does |
| --- | --- |
| **Download** | fetches the novel, and writes the exports you ticked |
| **Update** | fetches only what is new or has changed since last time |
| **Export** | writes the ticked exports again from what is already saved |
| **Describe folder** | says what a folder holds, and what it has lost, without fetching anything |

## The library

The panel at the top is a shelf. Point it at a folder that holds your novels —
one folder per work, each with a `manifest.json`, which is exactly what a
download writes — and it lists them: the author, the chapter and picture counts,
the rating, and when each was last fetched. The folder itself is listed too when
it is a novel, which is what **Use the save folder** shows you.

A download writes straight into the folder named in **Save into**, not into a
subfolder of it. To build a shelf, keep each work in its own folder under one
parent, then point the library at the parent.

Each work has four actions:

| Button | What it does |
| --- | --- |
| **Open** | opens the work's folder in your file manager |
| **Update** | fetches what the site has added since, into that folder |
| **Export** | writes the formats ticked in the form below, from what is already saved |
| **Remove** | moves the folder to the system trash |

**Remove** asks twice, and the second button names the work. It goes to the
trash, so it can be brought back. The window offers it only for a folder directly
inside the library folder that holds a `manifest.json`, and never for the library
folder itself. A folder whose manifest cannot be read still appears, with the
problem on its row, so it can be opened or removed instead of silently vanishing.

The library reads the folder and its direct children, not the whole disk. A plain
folder with no manifest is not a novel and is not listed.

## While it runs

The list fills with one line per chapter as it is saved, whether the chapter was
fetched or already there, and a running count of the bytes and the pictures. A
picture that could not be downloaded is named, not hidden: the chapter keeps its
address, and the next **Update** tries again.

When the run ends, a short report appears under the counts: what the work is,
what came back, what was skipped, what was missing, and where it all went. If
something went wrong, the same place says so in plain words and the folder is
left exactly as resumable as stopping the command line would leave it.

**One at a time.** A second job is refused while one is running, with the name
of the job in the way — two runs writing one folder would fight over the same
files. There is no cancel button yet; closing the window is safe, and running
the download again picks up where it stopped.

## What the window does not do

- **It never asks a site to be treated differently.** The command line has a
  `--no-robots` flag for the rare case you know what you are doing; the app has
  no such option and always consults `robots.txt`.
- **It never refuses a work because of its rating.** A rating, when the site
  publishes one, is shown in the report — an adult work downloads like any
  other, and each site's own age gate still applies.
- **It does not edit a chapter or a work's details.** The folder and its exports
  are the record, and the hashes in `manifest.json` are what an **Update**
  trusts; changing the words in the window would break them. Edit the page on the
  site, then update.
- **It does not show you a picture before it is downloaded.** Open the folder if
  you want to look inside it now.

## The folder it writes

Exactly the one the command line writes, and described in
[getting started](getting-started.md#what-lands-on-disk): a `manifest.json`, a
file per chapter, the cover and illustrations, and an `exports/` you can delete
whenever you like. Nothing about it is the app's private business — the command
line, this window and any future tool all read and write the same thing.
