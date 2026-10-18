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

el("export").addEventListener("click", async () => {
  if (running) return;
  begin("exporting", "exporting");
  try {
    const written = await invoke("export", {
      request: { dir: el("dir").value, ...settings() },
    });
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
});

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

/** The saved sessions, for the picker. Failure is not worth a dialog. */
async function loadSessions() {
  try {
    const list = await invoke("sessions");
    const select = el("session");
    const chosen = select.value;
    select.replaceChildren(node("option", null, "none"));
    select.firstChild.value = "";
    for (const session of list) {
      const option = node("option", null, `${session.name} (${session.hosts.join(", ")})`);
      option.value = session.name;
      select.append(option);
    }
    if (chosen && list.some((session) => session.name === chosen)) select.value = chosen;
  } catch (error) {
    console.warn("could not list sessions", error);
  }
}

async function main() {
  await listen(EVENT, (event) => onEvent(event.payload));
  await loadSessions();

  // A job started before this window loaded is still its own.
  const busy = await invoke("status");
  if (busy) {
    running = true;
    for (const id of BUTTONS) el(id).disabled = true;
    el("counts").textContent = `a ${busy} is already running`;
  }
}

main();
