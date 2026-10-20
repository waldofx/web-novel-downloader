// The desktop app's one manual check — the real front end, in the real window.
//
// Not part of `cargo test`: it needs a webview, a screen, and a live novel. It
// is kept because writing it again costs an afternoon, and because a window is
// the one thing here a test cannot click. `docs/engineering/testing.md` says how
// to run it; the short form:
//
//   1. copy `apps/gui/ui/` to `apps/gui/ui-manual/`, put this file in it, and
//      load it from that copy's `index.html` — after `app.js`:
//
//          <script src="ui_manual_check.js" defer></script>
//
//   2. point `build.frontendDist` in `apps/gui/tauri.conf.json` at `ui-manual`,
//      and set `dir` and `url` below;
//   3. `cargo build -p wnd-gui` and run `target/debug/nvd-app.exe`;
//   4. the verdict is the export: a `.txt` in the novel's `exports/` means every
//      check passed, a `.md` means one of them failed. Then put `frontendDist`
//      back to `ui` and delete `ui-manual/`.
//
// It runs the *shipped* `app.js` — not a copy of its logic — so what it checks
// is the page that would be shipped.

(async () => {
  // Edit these two.
  const dir = "C:\\Users\\user\\AppData\\Local\\Temp\\opencode\\gui-e2e";
  const url = "https://www.pixiv.net/novel/show.php?id=10000001";

  const channel = "nvd://progress";

  const { invoke } = window.__TAURI__.core;
  const { listen } = window.__TAURI__.event;

  const checks = [];
  const check = (name, ok) => checks.push(`${ok ? "ok" : "FAIL"} ${name}`);

  // `app.js` is a classic script, so its top-level declarations are reachable
  // from here under their own names.
  const handler = typeof onEvent === "function" ? onEvent : window.onEvent;
  check("app.js is loaded and reachable", typeof handler === "function");

  // 1. The page and the front end agree on what is on it.
  const wanted = [
    "url", "dir", "browse", "session", "max", "force", "check", "epub", "txt",
    "md", "ruby", "notes", "form", "download", "update", "export", "describe",
    "output-title", "counts", "details", "facts", "chapters", "summary",
    "manage-sessions", "sessions-dialog", "session-list", "sessions-status",
    "session-name", "session-file", "session-browse", "session-save-file",
    "session-host", "session-header", "session-save-header", "sessions-close",
    "library-dir", "library-browse", "library-refresh", "library-use-save",
    "library-status", "library-list",
  ];
  const missing = wanted.filter((id) => el(id) === null);
  check(`no element is missing from the page (${missing.join(",") || "none"})`, missing.length === 0);

  // 2. The session manager: summaries in the list, and nothing else.
  sessionList = [
    {
      name: "demo",
      hosts: ["example.com"],
      cookies: 2,
      savedAt: Math.floor(Date.now() / 1000),
      expiresAt: null,
    },
  ];
  renderSessions();
  const listed = el("session-list").children;
  check("the manager lists a session", listed.length === 1);
  check(
    "the manager names its site and its cookies",
    listed[0].textContent.includes("example.com") && listed[0].textContent.includes("2 cookies"),
  );

  // 3. The library: a row is a work, with what it holds and what it can do.
  libraryRoot = "C:\\Novels";
  libraryItems = [
    {
      title: "A made-up work",
      author: "Someone",
      chapters: 12,
      images: 2,
      rating: "general",
      fetchedAt: "2026-01-02T03:04:05Z",
      outputDir: "C:\\Novels\\A made-up work",
      removable: true,
      problem: null,
    },
  ];
  renderLibrary();
  const works = el("library-list").children;
  check("the library lists a work", works.length === 1);
  check(
    "the library names its work and offers its actions",
    works[0].textContent.includes("A made-up work") &&
      works[0].querySelectorAll("button").length === 4,
  );
  // Leave no fake root behind: the real run below would try to read it.
  libraryItems = [];
  libraryRoot = "";
  renderLibrary();

  // 4. The render path, driven with the payloads a job emits.
  handler({ kind: "planned", title: "自", chapters: 11, toFetch: 11 });
  check("planned names the novel", el("output-title").textContent === "自");

  handler({ kind: "fetched", index: 1, title: "第一話", bytes: 2048 });
  check("a fetched chapter becomes a row", el("chapters").children.length === 1);
  check("the row is titled", el("chapters").children[0].textContent.includes("第一話"));
  check("the counts move", el("counts").textContent.includes("1 fetched"));

  handler({ kind: "skipped", index: 2, title: "第二話" });
  check("a skipped chapter is a row too", el("chapters").children.length === 2);

  handler({ kind: "imageMissing", url: "https://i.example/1.png", reason: "http 404" });
  check(
    "a missing image is said out loud",
    el("counts").textContent.includes("1 images not fetched"),
  );

  handler({
    kind: "done",
    report: {
      title: "自",
      author: "誰か",
      rating: "adult",
      chaptersTotal: 11,
      chaptersFetched: 1,
      chaptersSkipped: 1,
      chaptersChecked: 0,
      imagesFetched: 0,
      imagesMissing: 1,
      bytesWritten: 2048,
      outputDir: dir,
      exported: [],
      removed: [],
      upToDate: false,
    },
  });
  const summary = el("summary").textContent;
  check("the report names the work", summary.includes("自"));
  check("the report counts the run", summary.includes("1 fetched, 1 skipped"));
  check("an adult work is said to be one", summary.includes("adult"));
  check("the buttons come back", el("download").disabled === false);

  // 5. The real thing: a real novel, through the real commands, on the real
  //    channel — and then an export decided by the payload the job sent.
  const finished = new Promise((resolve, reject) => {
    listen(channel, (event) => {
      if (event.payload.kind === "done") resolve(event.payload.report);
      if (event.payload.kind === "failed") reject(new Error(event.payload.message));
    });
  });

  await invoke("download", {
    request: {
      url,
      output: dir,
      maxChapters: 2,
      force: false,
      session: null,
      export: [],
      ruby: "parenthesised",
      notes: true,
    },
  });

  const report = await finished;
  check("the run named its own folder", typeof report.outputDir === "string" && report.outputDir.length > 0);
  check("the run fetched what it was asked for", report.chaptersFetched === 2);
  check("the run wrote chapters", report.bytesWritten > 0);

  const passed = checks.every((line) => line.startsWith("ok"));
  const written = await invoke("export", {
    request: {
      dir: report.outputDir,
      export: passed ? ["txt"] : ["md"],
      ruby: "parenthesised",
      notes: true,
    },
  });
  check("the export wrote a file", written.length === 1);

  document.title = `${passed && written.length === 1 ? "SELFTEST PASS" : "SELFTEST FAIL"} — ${checks.join(" | ")}`;
})().catch((error) => {
  // Nothing to swallow: the absence of an export is the machine-readable half
  // of the answer, and the title is where a human looks.
  document.title = `SELFTEST FAIL — ${String(error)}`;
  const box = document.getElementById("summary");
  if (box) box.textContent = `selftest failed: ${String(error)}`;
});
