"use strict";

// The window is a thin shell over the same library the command line uses. It
// never decides anything: it sends a request, and it renders the events that
// come back on one channel.
//
// Everything from the site — a novel's title, a chapter's title — is written
// with `textContent`, never as markup: the text comes from a stranger.

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

/** The channel the job reports on. */
const EVENT = "nvd://progress";

const el = (id) => document.getElementById(id);

/** Buttons that start work, and so are disabled while work is going on. */
const BUTTONS = ["download", "update", "export", "describe"];

let running = false;
/** Chapter index -> its row in the live list. */
let rows = new Map();
let counts = { fetched: 0, skipped: 0, checked: 0, images: 0, missing: 0, bytes: 0 };
/** The saved sessions, as the picker and the manager need them. */
let sessionList = [];
/** The works the library folder holds, and the folder they came from. */
let libraryItems = [];
let libraryRoot = "";

// ---------------------------------------------------------------- formatting

function size(bytes) {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KiB", "MiB", "GiB", "TiB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(1)} ${units[unit]}`;
}

function node(tag, className, text) {
  const element = document.createElement(tag);
  if (className) element.className = className;
  if (text !== undefined && text !== null) element.textContent = String(text);
  return element;
}

function paragraph(className, text) {
  return node("p", className, text);
}

// -------------------------------------------------------------------- output

function begin(label, title) {
  running = true;
  for (const id of BUTTONS) el(id).disabled = true;
  lockLibrary(true);

  rows = new Map();
  counts = { fetched: 0, skipped: 0, checked: 0, images: 0, missing: 0, bytes: 0 };
  el("chapters").replaceChildren();
  el("summary").replaceChildren();
  el("details").hidden = true;
  el("output-title").textContent = title || label;
  el("counts").textContent = `${label}…`;
}

function finish() {
  running = false;
  for (const id of BUTTONS) el(id).disabled = false;
  lockLibrary(false);
  // A run can add a work or change its counts, so the shelf should not lie.
  // Only the folder that was actually read is re-read: a typed path the user
  // has not refreshed yet is not a reason to replace what is on screen.
  if (libraryRoot && el("library-dir").value.trim() === libraryRoot) refreshLibrary(libraryRoot);
}

/** Library buttons belong to the same one-job-at-a-time rule. */
function lockLibrary(locked) {
  for (const button of el("library-list").querySelectorAll("button")) {
    if (locked) {
      // Remember what the row itself decided: a broken manifest's Update and
      // Export were already disabled, and must be again when the run ends.
      button.dataset.wasDisabled = button.disabled ? "1" : "";
      button.disabled = true;
    } else {
      button.disabled = button.dataset.wasDisabled === "1";
      delete button.dataset.wasDisabled;
    }
  }
}

function liveCounts() {
  const parts = [];
  if (counts.fetched) parts.push(`${counts.fetched} fetched`);
  if (counts.skipped) parts.push(`${counts.skipped} skipped`);
  if (counts.checked) parts.push(`${counts.checked} compared`);
  if (counts.images) parts.push(`${counts.images} images`);
  if (counts.missing) parts.push(`${counts.missing} images not fetched`);
  if (counts.bytes) parts.push(size(counts.bytes));
  return parts.join(" · ");
}

function showCounts() {
  el("counts").textContent = liveCounts();
}

function addChapter(index, title, state) {
  const existing = rows.get(index);
  if (existing) {
    existing.querySelector(".state").textContent = state;
    existing.querySelector(".state").className = `state ${state}`;
    if (title) existing.querySelector(".title").textContent = title;
    return;
  }

  const row = node("li");
  row.append(
    node("span", "index", String(index).padStart(4, "0")),
    node("span", "title", title || ""),
    node("span", `state ${state}`, state),
  );
  rows.set(index, row);
  el("chapters").append(row);
}

/** The report at the end of a run, as the numbers a person would repeat. */
function showReport(report) {
  const box = el("summary");
  box.replaceChildren();
  box.append(paragraph("title", report.title));

  if (report.upToDate) {
    box.append(
      paragraph(
        "ok",
        `Nothing new or changed — ${report.chaptersTotal} chapters.`,
      ),
    );
  } else {
    box.append(
      paragraph(
        null,
        `${report.chaptersFetched} fetched, ${report.chaptersSkipped} skipped` +
          (report.chaptersChecked ? `, ${report.chaptersChecked} compared` : "") +
          `, ${report.chaptersTotal} total.`,
      ),
    );
  }

  if (report.imagesFetched || report.imagesMissing) {
    box.append(
      paragraph(
        report.imagesMissing ? "warn" : null,
        `${report.imagesFetched} image(s) fetched` +
          (report.imagesMissing
            ? `, ${report.imagesMissing} could not be (the chapter keeps the address)`
            : "") +
          ".",
      ),
    );
  }

  if (report.author) box.append(paragraph(null, `By ${report.author}.`));
  if (report.rating === "adult") {
    box.append(paragraph(null, "Rated adult by the source. The site's own age gate applies."));
  }

  box.append(paragraph(null, `Written: ${size(report.bytesWritten)}.`));
  box.append(paragraph(null, report.outputDir));

  for (const gone of report.removed) {
    box.append(
      paragraph(
        "warn",
        `Chapter ${gone.index} — ${gone.title} — is no longer in the index. Nothing was deleted.`,
      ),
    );
  }

  for (const file of report.exported) {
    box.append(
      paragraph(
        "ok",
        `${file.format.toUpperCase()} → ${file.path} (${size(file.bytes)}, ${file.chapters} chapters)`,
      ),
    );
  }
}

function showError(message) {
  el("summary").replaceChildren(paragraph("error", message));
}

// -------------------------------------------------------------------- events

function onEvent(event) {
  switch (event.kind) {
    case "planned":
      el("output-title").textContent = event.title;
      el("counts").textContent = `${event.chapters} chapters, ${event.toFetch} to fetch…`;
      break;

    case "fetched":
      counts.fetched += 1;
      counts.bytes += event.bytes;
      addChapter(event.index, event.title, "fetched");
      showCounts();
      break;

    case "skipped":
      counts.skipped += 1;
      addChapter(event.index, event.title, "skipped");
      showCounts();
      break;

    case "checked":
      counts.checked += 1;
      addChapter(event.index, event.title, "checked");
      showCounts();
      break;

    case "imageFetched":
      counts.images += 1;
      counts.bytes += event.bytes;
      showCounts();
      break;

    case "imageMissing":
      counts.missing += 1;
      showCounts();
      break;

    case "removed":
      addChapter(event.index, event.title, "gone");
      break;

    case "exported":
      break;

    case "done":
      showReport(event.report);
      finish();
      break;

    case "failed":
      showError(event.message);
      el("counts").textContent = "stopped";
      finish();
      break;

    default:
      // A kind this build does not know about is not a reason to lose the run.
      console.warn("unhandled event", event);
  }
}

// ------------------------------------------------------------------ requests

function settings() {
  const formats = [];
  if (el("epub").checked) formats.push("epub");
  if (el("txt").checked) formats.push("txt");
  if (el("md").checked) formats.push("md");

  return {
    session: el("session").value || null,
    export: formats,
    ruby: el("ruby").value,
    notes: el("notes").checked,
  };
}

function downloadRequest() {
  const max = el("max").value.trim();
  return {
    url: el("url").value,
    output: el("dir").value,
    maxChapters: max === "" ? null : Number(max),
    force: el("force").checked,
    ...settings(),
  };
}

/** Start a job. The command answers as soon as the work has started. */
async function start(command, label, request, title) {
  if (running) return;
  begin(label, title);
  try {
    await invoke(command, { request });
  } catch (error) {
    // Refused before anything was fetched: a URL nothing handles, a folder that
    // is not a novel, a session that is not there.
    showError(String(error));
    el("counts").textContent = "";
    finish();
  }
}

// ------------------------------------------------------------------- buttons

el("browse").addEventListener("click", async () => {
  const folder = await invoke("pick_directory");
  if (folder) el("dir").value = folder;
});

el("form").addEventListener("submit", (event) => {
  event.preventDefault();
  start("download", "downloading", downloadRequest(), null);
});

el("update").addEventListener("click", () => {
  start(
    "update",
    "updating",
    { dir: el("dir").value, check: el("check").checked, ...settings() },
    null,
  );
});

/** Export one folder, wherever the ask came from. */
async function exportFolder(dir) {
  if (running) return;
  begin("exporting", "exporting");
  try {
    const written = await invoke("export", { request: { dir, ...settings() } });
    const box = el("summary");
    box.replaceChildren();
    if (!written.length) box.append(paragraph(null, "Nothing to write."));
    for (const file of written) {
      box.append(
        paragraph(
          "ok",
          `${file.format.toUpperCase()} → ${file.path} (${size(file.bytes)}, ${file.chapters} chapters)`,
        ),
      );
    }
    el("counts").textContent = "";
  } catch (error) {
    showError(String(error));
  } finally {
    finish();
  }
}

el("export").addEventListener("click", () => exportFolder(el("dir").value));

el("describe").addEventListener("click", async () => {
  if (running) return;
  begin("reading", "reading the folder");
  try {
    const info = await invoke("info", { dir: el("dir").value });
    el("output-title").textContent = info.title;

    const facts = el("facts");
    facts.replaceChildren();
    const add = (label, value) => {
      facts.append(node("dt", null, label), node("dd", null, value));
    };
    if (info.author) add("author", info.author);
    add("source", `${info.source} — ${info.novelUrl}`);
    add("language", info.language);
    add("rating", info.rating === "adult" ? "adult (the source's own)" : "general");
    if (info.tags.length) add("tags", info.tags.join(", "));
    add("chapters", info.chapters);
    add("images", info.images);
    if (info.chaptersMissing) add("missing", `${info.chaptersMissing} chapter file(s) not saved yet`);
    if (info.imagesMissing) {
      add("missing", `${info.imagesMissing} image file(s) not on disk; an update fetches them back`);
    }
    add("folder", info.outputDir);

    el("details").hidden = false;
    el("details").open = true;
    el("counts").textContent = "";
  } catch (error) {
    showError(String(error));
  } finally {
    finish();
  }
});

// ------------------------------------------------------------------ sessions

/** How long ago a session was saved, in words. */
function age(seconds) {
  const minutes = Math.floor((Date.now() - Number(seconds) * 1000) / 60000);
  if (!Number.isFinite(minutes) || minutes < 0) return "unknown";
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} h ago`;
  return `${Math.floor(hours / 24)} d ago`;
}

/** When a session stops being sent, in words. */
function expiry(seconds) {
  if (seconds === null || seconds === undefined) return "no expiry";
  const at = Number(seconds) * 1000;
  if (at <= Date.now()) return "expired";
  return `expires ${new Date(at).toLocaleDateString()}`;
}

function sessionStatus(text, kind) {
  const line = el("sessions-status");
  line.textContent = text;
  line.className = kind || "hint";
}

function fillPicker() {
  const select = el("session");
  const chosen = select.value;
  select.replaceChildren(node("option", null, "none"));
  select.firstChild.value = "";
  for (const session of sessionList) {
    const option = node("option", null, `${session.name} (${session.hosts.join(", ")})`);
    option.value = session.name;
    select.append(option);
  }
  if (chosen && sessionList.some((session) => session.name === chosen)) select.value = chosen;
}

/** Forget is a two-step button: no native modal, and the name is in the question. */
function sessionActions(session) {
  const actions = node("div", "actions");
  const forget = node("button", "quiet", "Forget");
  forget.type = "button";
  forget.addEventListener("click", () => {
    actions.replaceChildren();
    const sure = node("button", null, `Forget “${session.name}”?`);
    sure.type = "button";
    sure.addEventListener("click", async () => {
      try {
        await invoke("forget_session", { name: session.name });
        await refreshSessions();
        sessionStatus(`Forgot the “${session.name}” session.`, "ok");
      } catch (error) {
        sessionStatus(String(error), "error");
      }
    });
    const keep = node("button", "quiet", "Keep");
    keep.type = "button";
    keep.addEventListener("click", () => actions.replaceChildren(forget));
    actions.append(sure, keep);
  });
  actions.append(forget);
  return actions;
}

function renderSessions() {
  const list = el("session-list");
  list.replaceChildren();
  if (!sessionList.length) {
    list.append(node("li", "empty", "No sessions yet. Add one below."));
    return;
  }
  for (const session of sessionList) {
    const item = node("li");
    item.append(node("span", "name", session.name));
    item.append(
      node(
        "div",
        "meta",
        `${session.hosts.length ? session.hosts.join(", ") : "no site"} · ` +
          `${session.cookies} cookie${session.cookies === 1 ? "" : "s"} · ` +
          `saved ${age(session.savedAt)} · ${expiry(session.expiresAt)}`,
      ),
    );
    item.append(sessionActions(session));
    list.append(item);
  }
}

/** Read the store, and show it in both the picker and the manager. */
async function refreshSessions() {
  sessionList = await invoke("sessions");
  fillPicker();
  renderSessions();
}

/** The same, for startup, where a failure is worth no more than a warning. */
async function loadSessions() {
  try {
    await refreshSessions();
  } catch (error) {
    console.warn("could not list sessions", error);
  }
}

/** Choose the session just saved, if the store kept it. */
function chooseSession(name) {
  if (sessionList.some((session) => session.name === name)) el("session").value = name;
}

el("manage-sessions").addEventListener("click", async () => {
  sessionStatus("");
  try {
    await refreshSessions();
  } catch (error) {
    sessionStatus(String(error), "error");
  }
  const dialog = el("sessions-dialog");
  if (!dialog.open) dialog.showModal();
});

el("sessions-close").addEventListener("click", () => el("sessions-dialog").close());

el("session-browse").addEventListener("click", async () => {
  const file = await invoke("pick_cookies_file");
  if (file) el("session-file").value = file;
});

el("session-save-file").addEventListener("click", async () => {
  const name = el("session-name").value.trim();
  const file = el("session-file").value.trim();
  if (!file) {
    sessionStatus("Choose a cookies.txt first.", "error");
    return;
  }
  try {
    await invoke("save_session_file", { name, file });
    el("session-name").value = "";
    el("session-file").value = "";
    await refreshSessions();
    chooseSession(name);
    sessionStatus(`Saved the “${name}” session.`, "ok");
  } catch (error) {
    sessionStatus(String(error), "error");
  }
});

el("session-save-header").addEventListener("click", async () => {
  const name = el("session-name").value.trim();
  const host = el("session-host").value.trim();
  try {
    await invoke("save_session_header", { name, host, header: el("session-header").value });
    // The header is a credential; do not leave it on the screen.
    el("session-header").value = "";
    el("session-host").value = "";
    el("session-name").value = "";
    await refreshSessions();
    chooseSession(name);
    sessionStatus(`Saved the “${name}” session.`, "ok");
  } catch (error) {
    sessionStatus(String(error), "error");
  }
});

// ------------------------------------------------------------------- library

function libraryStatus(text, kind) {
  const line = el("library-status");
  line.textContent = text;
  line.className = kind || "hint";
}

/** The one line under a work's title. */
function libraryMeta(item) {
  const parts = [];
  if (item.author) parts.push(item.author);
  parts.push(`${item.chapters} chapter${item.chapters === 1 ? "" : "s"}`);
  if (item.images) parts.push(`${item.images} image${item.images === 1 ? "" : "s"}`);
  if (item.rating === "adult") parts.push("adult");
  if (item.fetchedAt) {
    const when = new Date(item.fetchedAt);
    if (!Number.isNaN(when.getTime())) parts.push(`fetched ${when.toLocaleDateString()}`);
  }
  return parts.join(" · ");
}

function libraryActions(item) {
  const actions = node("div", "actions");
  const buttons = [];

  const open = node("button", "quiet", "Open");
  open.type = "button";
  open.addEventListener("click", async () => {
    try {
      await invoke("open_path", { path: item.outputDir });
    } catch (error) {
      libraryStatus(String(error), "error");
    }
  });
  buttons.push(open);

  const update = node("button", null, "Update");
  update.type = "button";
  update.disabled = Boolean(item.problem);
  update.addEventListener("click", () => {
    start(
      "update",
      "updating",
      { dir: item.outputDir, check: el("check").checked, ...settings() },
      item.title,
    );
  });
  buttons.push(update);

  const write = node("button", null, "Export");
  write.type = "button";
  write.disabled = Boolean(item.problem);
  write.addEventListener("click", () => exportFolder(item.outputDir));
  buttons.push(write);

  if (item.removable) {
    const remove = node("button", "quiet", "Remove");
    remove.type = "button";
    remove.addEventListener("click", () => {
      if (running) return;
      // Two steps, and the second names the work: no native dialog is needed,
      // and nothing is removed without saying what.
      actions.replaceChildren();
      const sure = node("button", "danger", `Remove “${item.title}”?`);
      sure.type = "button";
      sure.title = "The folder goes to the system trash.";
      sure.addEventListener("click", async () => {
        if (running) return;
        try {
          await invoke("remove_novel", { root: libraryRoot, novel: item.outputDir });
          await refreshLibrary(libraryRoot);
          libraryStatus(`Moved “${item.title}” to the trash.`, "ok");
        } catch (error) {
          libraryStatus(String(error), "error");
        }
      });
      const keep = node("button", "quiet", "Keep");
      keep.type = "button";
      keep.addEventListener("click", () => actions.replaceChildren(...buttons));
      actions.append(sure, keep);
    });
    buttons.push(remove);
  }

  actions.append(...buttons);
  return actions;
}

function renderLibrary() {
  const list = el("library-list");
  list.replaceChildren();
  if (!libraryRoot) {
    list.append(node("li", "empty", "Choose a folder to see the novels it holds."));
    return;
  }
  if (!libraryItems.length) {
    list.append(
      node(
        "li",
        "empty",
        "No novels here yet. A download into a folder in this one puts the first work here.",
      ),
    );
    return;
  }
  for (const item of libraryItems) {
    const row = node("li");
    const work = node("div", "work");
    work.append(node("span", "title", item.title));
    work.append(node("div", "meta", libraryMeta(item)));
    if (item.problem) work.append(node("div", "problem", `manifest: ${item.problem}`));
    work.append(node("div", "path", item.outputDir));
    row.append(work, libraryActions(item));
    list.append(row);
  }
}

/** Read a library folder; the argument is the folder the caller means. */
let libraryReads = 0;

async function refreshLibrary(root = el("library-dir").value.trim()) {
  const read = ++libraryReads;
  libraryRoot = root;
  if (!root) {
    libraryItems = [];
    libraryStatus("");
    renderLibrary();
    return;
  }
  try {
    const items = await invoke("library", { root });
    // A slower read must not put an older shelf back over a newer one.
    if (read !== libraryReads) return;
    libraryItems = items;
    renderLibrary();
    libraryStatus(items.length === 1 ? "1 novel" : `${items.length} novels`, "");
  } catch (error) {
    if (read !== libraryReads) return;
    libraryItems = [];
    el("library-list").replaceChildren();
    libraryStatus(String(error), "error");
  }
}

el("library-browse").addEventListener("click", async () => {
  const folder = await invoke("pick_directory");
  if (!folder) return;
  el("library-dir").value = folder;
  await refreshLibrary();
});

el("library-refresh").addEventListener("click", () => refreshLibrary());

el("library-use-save").addEventListener("click", async () => {
  const save = el("dir").value.trim();
  if (!save) {
    libraryStatus("Choose the save folder first.", "error");
    return;
  }
  el("library-dir").value = save;
  await refreshLibrary();
});

el("library-dir").addEventListener("keydown", (event) => {
  if (event.key !== "Enter") return;
  event.preventDefault();
  refreshLibrary();
});

async function main() {
  await listen(EVENT, (event) => onEvent(event.payload));
  await loadSessions();
  await refreshLibrary();

  // A job started before this window loaded is still its own.
  const busy = await invoke("status");
  if (busy) {
    running = true;
    for (const id of BUTTONS) el(id).disabled = true;
    el("counts").textContent = `a ${busy} is already running`;
  }
}

main();
