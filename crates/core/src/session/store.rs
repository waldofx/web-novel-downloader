//! Where a session lives, and the three things the user does with it.
//!
//! One directory, one file per session, under the user's own data directory —
//! never in the repository, never inside a novel directory, never anywhere it
//! could be committed or uploaded. The file is `0600` on Unix, and it is written
//! atomically, so an interrupted save leaves the previous session rather than
//! half of a new one.
//!
//! `save`, `list` and `delete` are the whole API, deliberately: they are the
//! three buttons the desktop app (#21) will show, and the CLI is only the first
//! of its two clients.
//!
//! The file format is ours and internal — not the interchange format of
//! [store::model](crate::store::model), which is a contract. It holds the same
//! seven fields as the export it came from, plus the name and the time it was
//! saved; nothing else reads it, so it may change while sessions are young.

use crate::error::{Error, Result};
use crate::session::cookie::{Cookie, host_is_covered};
use crate::store::storage::write_json_private;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// The directory under the user's data directory.
const APP_DIR: &str = "web-novel-downloader";
/// The one under that, holding the sessions.
const SESSIONS_DIR: &str = "sessions";
/// The most a session name may be, to stay a file name.
const MAX_NAME: usize = 64;

/// A cookie jar the user handed over, under a name.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    name: String,
    /// The hosts it may be sent to: a session for `pixiv.net` is not offered to
    /// `i.pximg.net`, which is a different site that happens to serve images.
    hosts: Vec<String>,
    /// Unix seconds, so staleness can be seen and said out loud.
    saved_at: u64,
    cookies: Vec<Cookie>,
}

/// What a listing shows. Never a cookie value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSummary {
    pub name: String,
    pub hosts: Vec<String>,
    pub saved_at: u64,
    /// The first moment any of its cookies expires, or `None` when none does.
    pub expires_at: Option<u64>,
    /// How many cookies it holds, so "0" is visible rather than silent.
    pub cookies: usize,
}

impl Session {
    /// A session under `name`, restricted to `hosts`.
    pub fn new(name: impl Into<String>, hosts: Vec<String>, cookies: Vec<Cookie>) -> Self {
        Self {
            name: name.into(),
            hosts,
            cookies,
            saved_at: now(),
        }
    }

    /// A session restricted to the domains its own cookies were set for.
    pub fn from_cookies(name: impl Into<String>, cookies: Vec<Cookie>) -> Self {
        Self::new(name, hosts_of(&cookies), cookies)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn hosts(&self) -> &[String] {
        &self.hosts
    }

    pub fn cookies(&self) -> &[Cookie] {
        &self.cookies
    }

    pub fn saved_at(&self) -> u64 {
        self.saved_at
    }

    /// Whether this session may be sent to `host` at all.
    pub fn covers(&self, host: &str) -> bool {
        self.hosts.iter().any(|named| host_is_covered(named, host))
    }

    /// The first moment any of its cookies expires, or `None` when none does.
    pub fn expires_at(&self) -> Option<u64> {
        self.cookies
            .iter()
            .filter_map(|cookie| cookie.expires_at)
            .min()
    }

    /// The summary `list` shows.
    pub fn summary(&self) -> SessionSummary {
        SessionSummary {
            name: self.name.clone(),
            hosts: self.hosts.clone(),
            saved_at: self.saved_at,
            expires_at: self.expires_at(),
            cookies: self.cookies.len(),
        }
    }
}

/// The hosts a jar names: every cookie's domain, once, in order.
pub fn hosts_of(cookies: &[Cookie]) -> Vec<String> {
    let mut hosts: Vec<String> = cookies
        .iter()
        .map(|cookie| cookie.domain.to_lowercase())
        .collect();
    hosts.sort();
    hosts.dedup();
    hosts
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Session")
            .field("name", &self.name)
            .field("hosts", &self.hosts)
            .field("cookies", &format_args!("{} cookies", self.cookies.len()))
            .field("saved_at", &self.saved_at)
            .finish()
    }
}

/// One directory holding one file per session.
#[derive(Debug, Clone)]
pub struct SessionStore {
    dir: PathBuf,
}

impl SessionStore {
    /// A store rooted at `dir`.
    pub fn at(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Write `session`, replacing any session of the same name.
    ///
    /// Returns the file it was written to. Saving over an existing name is the
    /// refresh gesture: export cookies again, save again, and a stale session
    /// becomes a live one.
    pub fn save(&self, session: &Session) -> Result<PathBuf> {
        let path = self.path_for(&session.name)?;
        std::fs::create_dir_all(&self.dir)?;
        write_json_private(&path, session)?;
        Ok(path)
    }

    /// The session under `name`.
    pub fn load(&self, name: &str) -> Result<Session> {
        let path = self.path_for(name)?;
        if !path.is_file() {
            return Err(Error::SessionNotFound(name.to_string()));
        }
        let text = std::fs::read_to_string(&path)?;
        serde_json::from_str(&text).map_err(|source| Error::SessionFile {
            path: path.display().to_string(),
            source,
        })
    }

    /// Every session, by name. An empty or missing directory is no sessions.
    pub fn list(&self) -> Result<Vec<SessionSummary>> {
        if !self.dir.is_dir() {
            return Ok(Vec::new());
        }
        let mut summaries = Vec::new();
        for entry in std::fs::read_dir(&self.dir)? {
            let path = entry?.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let text = std::fs::read_to_string(&path)?;
            let session: Session =
                serde_json::from_str(&text).map_err(|source| Error::SessionFile {
                    path: path.display().to_string(),
                    source,
                })?;
            summaries.push(session.summary());
        }
        summaries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(summaries)
    }

    /// Forget the session under `name`.
    pub fn delete(&self, name: &str) -> Result<()> {
        let path = self.path_for(name)?;
        if !path.is_file() {
            return Err(Error::SessionNotFound(name.to_string()));
        }
        std::fs::remove_file(&path)?;
        Ok(())
    }

    /// The file a name maps to, or nothing at all if the name is not one.
    fn path_for(&self, name: &str) -> Result<PathBuf> {
        if !valid_name(name) {
            return Err(Error::InvalidSessionName {
                name: name.to_string(),
                why: "1 to 64 characters of letters, digits, `-`, `_` and `.`, not starting with a dot",
            });
        }
        Ok(self.dir.join(format!("{name}.json")))
    }
}

/// Where sessions live when the user does not say.
///
/// The user's own data directory: `%LOCALAPPDATA%` on Windows,
/// `~/Library/Application Support` on macOS, `$XDG_DATA_HOME` or
/// `~/.local/share` elsewhere. Never the working directory — where a session is
/// *not* written is as much a promise as where it is.
pub fn default_dir() -> Result<PathBuf> {
    let platform = if cfg!(windows) {
        Platform::Windows
    } else if cfg!(target_os = "macos") {
        Platform::MacOs
    } else {
        Platform::Unix
    };
    data_dir_for(platform, |name| std::env::var(name).ok())
}

/// The platforms whose data-directory rule differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Platform {
    Windows,
    MacOs,
    Unix,
}

/// `default_dir`, with the platform and the environment passed in, so all three
/// rules can be tested on any machine.
fn data_dir_for(platform: Platform, env: impl Fn(&str) -> Option<String>) -> Result<PathBuf> {
    let root = match platform {
        Platform::Windows => env("LOCALAPPDATA").or_else(|| env("APPDATA")),
        Platform::MacOs => env("HOME").map(|home| format!("{home}/Library/Application Support")),
        Platform::Unix => {
            env("XDG_DATA_HOME").or_else(|| env("HOME").map(|home| format!("{home}/.local/share")))
        }
    };
    match root {
        Some(root) if !root.trim().is_empty() => {
            Ok(Path::new(&root).join(APP_DIR).join(SESSIONS_DIR))
        }
        _ => Err(Error::NoDataDir(match platform {
            Platform::Windows => "LOCALAPPDATA",
            _ => "HOME",
        })),
    }
}

fn valid_name(name: &str) -> bool {
    if name.is_empty() || name.len() > MAX_NAME {
        return false;
    }
    if name.starts_with('.') {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The directory the rule produces for an environment, as a string.
    fn dir_for(platform: Platform, env: &[(&str, &str)]) -> Result<String> {
        data_dir_for(platform, |name| {
            env.iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_string())
        })
        .map(|path| path.to_string_lossy().replace('\\', "/"))
    }

    #[test]
    fn the_default_directory_is_under_the_users_own_data() {
        let cases = [
            (
                Platform::Windows,
                vec![("LOCALAPPDATA", "C:/Users/x/AppData/Local")],
                "C:/Users/x/AppData/Local/web-novel-downloader/sessions",
            ),
            (
                Platform::MacOs,
                vec![("HOME", "/Users/x")],
                "/Users/x/Library/Application Support/web-novel-downloader/sessions",
            ),
            (
                Platform::Unix,
                vec![("HOME", "/home/x")],
                "/home/x/.local/share/web-novel-downloader/sessions",
            ),
        ];

        for (platform, env, expected) in cases {
            assert_eq!(dir_for(platform, &env).unwrap(), expected, "{platform:?}");
        }
    }

    #[test]
    fn the_variables_that_come_first_win() {
        // Windows falls back to roaming only when the local one is missing, and
        // Unix prefers XDG; both are pinned so neither silently stops working.
        assert_eq!(
            dir_for(
                Platform::Windows,
                &[("LOCALAPPDATA", "C:/local"), ("APPDATA", "C:/roaming"),],
            )
            .unwrap(),
            "C:/local/web-novel-downloader/sessions"
        );
        assert_eq!(
            dir_for(Platform::Windows, &[("APPDATA", "C:/roaming")],).unwrap(),
            "C:/roaming/web-novel-downloader/sessions"
        );
        assert_eq!(
            dir_for(
                Platform::Unix,
                &[("XDG_DATA_HOME", "/data"), ("HOME", "/home/x")],
            )
            .unwrap(),
            "/data/web-novel-downloader/sessions"
        );
    }

    #[test]
    fn without_a_data_directory_the_error_names_the_variable_to_set() {
        let error = data_dir_for(Platform::Unix, |_| None).unwrap_err();
        assert!(error.to_string().contains("HOME"), "{error}");

        let error = data_dir_for(Platform::Windows, |_| None).unwrap_err();
        assert!(error.to_string().contains("LOCALAPPDATA"), "{error}");

        let error = data_dir_for(Platform::Windows, |_| Some(String::new())).unwrap_err();
        assert!(
            error.to_string().contains("LOCALAPPDATA"),
            "an empty variable is not a directory: {error}"
        );
    }
}
