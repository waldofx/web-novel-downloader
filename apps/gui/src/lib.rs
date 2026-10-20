//! The window: Tauri commands, one thin layer over [`job`].
//!
//! Nothing here decides anything. It checks what the front end sent, starts a
//! thread, and reports what happens on a channel the webview listens to, because
//! everything worth testing lives in `job.rs`, where there is no window to need.

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use tauri::{AppHandle, Emitter, Manager};

pub mod job;

/// The channel progress travels on.
pub const EVENT: &str = "nvd://progress";

/// The one thing the window may be doing at a time.
///
/// Two jobs at once would fight over the same files, and a second fetch of the
/// same chapter is impolite besides. A job releases the flag when it ends,
/// however it ends.
#[derive(Default)]
pub struct AppState {
    busy: Mutex<Option<String>>,
}

impl AppState {
    fn lock(&self) -> MutexGuard<'_, Option<String>> {
        // A panic in a job thread must not poison the app into a state where no
        // job can ever start again.
        self.busy
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn begin(&self, label: &str) -> Result<(), String> {
        let mut busy = self.lock();
        if let Some(running) = busy.as_deref() {
            return Err(format!("a {running} is already running"));
        }
        *busy = Some(label.to_string());
        Ok(())
    }

    fn release(&self) {
        *self.lock() = None;
    }

    /// What is running, if anything.
    ///
    /// Public because the window asks: a window that has just loaded needs to
    /// know whether a job started before it is still its own.
    pub fn busy(&self) -> Option<String> {
        self.lock().clone()
    }
}

/// Run `work` on a thread of its own, reporting how it went.
///
/// The command returns as soon as the thread is started: a download takes
/// minutes, and a window that waits for it would be a frozen window. The answer
/// arrives as [`job::Event::Done`] or [`job::Event::Failed`], which is also why
/// the return type of the commands below is not the interesting part.
fn spawn(
    app: &AppHandle,
    label: &'static str,
    work: impl FnOnce(&dyn Fn(job::Event)) -> Result<job::Report, String> + Send + 'static,
) -> Result<(), String> {
    app.state::<AppState>().begin(label)?;
    let app = app.clone();
    std::thread::spawn(move || {
        let target = app.clone();
        let emit = move |event: job::Event| {
            // A window that has gone away is not worth failing a download over.
            let _ = target.emit(EVENT, event);
        };
        conclude(work(&emit), &emit);
        app.state::<AppState>().release();
    });
    Ok(())
}

/// How a job ended, said in the one place that decides it.
///
/// A run that worked and a run that did not are the same two lines everywhere,
/// and the window waits for exactly one of them: nothing else tells it that the
/// buttons are its own again.
fn conclude(outcome: Result<job::Report, String>, emit: &dyn Fn(job::Event)) {
    match outcome {
        Ok(report) => emit(job::Event::Done { report }),
        Err(message) => emit(job::Event::Failed { message }),
    }
}

/// A session by name, loaded before the job starts, so that a name that is not
/// there is an error the window can show rather than a run that dies on its
/// first request.
fn session_for(name: &Option<String>) -> Result<Option<wnd_core::session::Session>, String> {
    name.as_deref().map(job::load_session).transpose()
}

/// Fetch a novel, and export it if the window asked for that.
#[tauri::command]
fn download(app: AppHandle, request: job::DownloadRequest) -> Result<(), String> {
    let prepared = job::prepare_download(&request)?;
    let session = session_for(&prepared.session)?;
    spawn(&app, "download", move |emit: &dyn Fn(job::Event)| {
        let client = job::client(session)?;
        job::download(&wnd_sources::registry(), &client, &prepared, emit)
    })
}

/// Bring a directory up to date, and export it if the window asked for that.
#[tauri::command]
fn update(app: AppHandle, request: job::UpdateRequest) -> Result<(), String> {
    let prepared = job::prepare_update(&request)?;
    let session = session_for(&prepared.session)?;
    spawn(&app, "update", move |emit: &dyn Fn(job::Event)| {
        let client = job::client(session)?;
        job::update(&wnd_sources::registry(), &client, &prepared, emit)
    })
}

/// Export a directory that is already downloaded.
///
/// Writing an EPUB is worth doing off the main thread, and there is nothing to
/// report along the way: the answer is the list of files that were written.
#[tauri::command]
async fn export(request: job::ExportRequest) -> Result<Vec<job::ExportedFile>, String> {
    let prepared = job::prepare_export(&request)?;
    tauri::async_runtime::spawn_blocking(move || job::export(&prepared, &mut |_event| {}))
        .await
        .map_err(|error| error.to_string())?
}

/// Describe a directory a window has open, without touching the network.
#[tauri::command]
fn info(dir: String) -> Result<job::NovelInfo, String> {
    job::info(Path::new(&dir))
}

/// The sessions the user has saved, for the picker.
#[tauri::command]
fn sessions() -> Result<Vec<job::SessionInfo>, String> {
    job::sessions()
}

/// Save a session from a `cookies.txt` the user exported.
#[tauri::command]
fn save_session_file(name: String, file: String) -> Result<(), String> {
    job::save_session_file(&name, Path::new(&file))
}

/// Save a session from a raw `Cookie:` header, for the one site it names.
#[tauri::command]
fn save_session_header(name: String, host: String, header: String) -> Result<(), String> {
    job::save_session_header(&name, &host, &header)
}

/// Forget a saved session.
#[tauri::command]
fn forget_session(name: String) -> Result<(), String> {
    job::forget_session(&name)
}

/// What is running, if anything.
///
/// A window that has just loaded asks: a job started before a reload is still
/// its own, and offering a second one would only end in a refusal.
#[tauri::command]
fn status(state: tauri::State<'_, AppState>) -> Option<String> {
    state.busy()
}

/// The native "choose a folder" dialog.
///
/// Blocking, and on a thread of its own: the dialog belongs to the operating
/// system, and it is the one thing here that cannot be asynchronous. It is given
/// the window as its parent so that it opens over the app rather than behind it.
#[tauri::command]
async fn pick_directory(window: tauri::Window) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || {
        rfd::FileDialog::new()
            .set_title("Choose a folder")
            .set_parent(&window)
            .pick_folder()
            .map(|folder| folder.display().to_string())
    })
    .await
    .ok()
    .flatten()
}

/// The native "choose a cookies file" dialog, the sibling of the folder one.
#[tauri::command]
async fn pick_cookies_file(window: tauri::Window) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || {
        rfd::FileDialog::new()
            .set_title("Choose a cookies file")
            .add_filter("Cookies", &["txt"])
            .set_parent(&window)
            .pick_file()
            .map(|file| file.display().to_string())
    })
    .await
    .ok()
    .flatten()
}

/// Start the window.
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            pick_directory,
            pick_cookies_file,
            sessions,
            save_session_file,
            save_session_header,
            forget_session,
            status,
            download,
            update,
            export,
            info
        ])
        .run(tauri::generate_context!())
        .expect("the app could not start");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_job_is_refused_while_one_is_running() {
        let state = AppState::default();
        assert_eq!(state.busy(), None);
        state.begin("download").unwrap();
        assert_eq!(state.busy().as_deref(), Some("download"));

        let refused = state.begin("update").unwrap_err();
        assert!(
            refused.contains("already running") && refused.contains("download"),
            "the refusal names what is in the way: {refused}"
        );

        state.release();
        assert_eq!(state.busy(), None);
        state.begin("update").unwrap();
    }

    #[test]
    fn a_poisoned_flag_still_starts_a_job() {
        let state = std::sync::Arc::new(AppState::default());
        let other = state.clone();
        let _ = std::thread::spawn(move || {
            let _guard = other.lock();
            panic!("a job died while holding the flag");
        })
        .join();

        // The mutex is poisoned now. Reading it and starting a job must both
        // still work: a job that died is not a reason to refuse every job after
        // it.
        assert_eq!(
            state.busy(),
            None,
            "a poisoned flag reads as nothing running"
        );
        state.begin("download").unwrap();
        assert_eq!(state.busy().as_deref(), Some("download"));
    }

    /// A report with nothing in it, for the wrapping below.
    fn report() -> job::Report {
        job::Report {
            title: "本".to_string(),
            author: None,
            rating: None,
            chapters_total: 0,
            chapters_fetched: 0,
            chapters_skipped: 0,
            chapters_checked: 0,
            images_fetched: 0,
            images_missing: 0,
            bytes_written: 0,
            output_dir: "C:\\novels\\本".to_string(),
            exported: Vec::new(),
            removed: Vec::new(),
            up_to_date: true,
        }
    }

    /// The window learns that a job is over from exactly one event, and it is
    /// this function that decides which.
    #[test]
    fn a_job_ends_in_exactly_one_event() {
        // `conclude` hands the event to a `Fn`, which is what the job thread has
        // to give it, so the collecting is behind a lock.
        let collected = std::sync::Mutex::new(Vec::new());
        conclude(Ok(report()), &|event| {
            collected.lock().expect("the sink").push(event)
        });
        let events = collected.into_inner().expect("the sink");

        assert_eq!(events.len(), 1);
        match &events[0] {
            job::Event::Done { report } => {
                assert_eq!(report.title, "本");
                assert!(report.up_to_date);
            }
            other => panic!("a run that worked ends in `done`: {other:?}"),
        }

        let collected = std::sync::Mutex::new(Vec::new());
        conclude(Err("nothing to fetch".to_string()), &|event| {
            collected.lock().expect("the sink").push(event)
        });
        assert_eq!(
            collected.into_inner().expect("the sink"),
            [job::Event::Failed {
                message: "nothing to fetch".to_string()
            }]
        );
    }
}
