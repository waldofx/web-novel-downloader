//! Error type shared across the workspace.

/// Errors produced by the core crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),

    /// The on-disk format is **newer** than this build understands.
    ///
    /// The format is versioned so a future translation stage can rely on it. It
    /// is only bumped for an incompatible change, so an *older* file is read as
    /// a matter of course — this error is one direction only: a file written by
    /// a newer build, which may mean things this one does not.
    #[error("format version {found} is newer than this build reads ({expected}); update the tool")]
    FormatVersion { found: u32, expected: u32 },

    /// A line of a cookie file that is not the Netscape `cookies.txt` format.
    ///
    /// Only the line number and the reason: a malformed line still holds a
    /// credential, and an error message ends up in terminals, logs and issues.
    #[error("not a Netscape cookies.txt: line {line}: {reason}")]
    CookieFile { line: usize, reason: String },

    /// A session name that cannot be a file name.
    #[error("invalid session name `{name}`: {why}")]
    InvalidSessionName { name: String, why: &'static str },

    /// No session under that name. Never carries a cookie: only the name.
    #[error("no session named `{0}`")]
    SessionNotFound(String),

    /// A file in the sessions directory that is not a session.
    #[error("not a session file: {path}: {source}")]
    SessionFile {
        path: String,
        #[source]
        source: serde_json::Error,
    },

    /// Neither a home nor a data directory could be found for the user.
    #[error("no user data directory found: set {0}")]
    NoDataDir(&'static str),

    #[error("chapter {0} not found")]
    ChapterNotFound(u32),

    #[error("manifest not found in {0}")]
    ManifestNotFound(String),

    /// Reached only for work that a later milestone will implement.
    #[error("not implemented yet: {0}")]
    NotImplemented(String),

    /// A URL that could not be parsed, caught before any request is made.
    #[error("invalid URL: {0}")]
    InvalidUrl(String),

    /// The HTTP client itself could not be constructed.
    #[error("could not build the HTTP client: {0}")]
    ClientBuild(#[source] reqwest::Error),

    /// The server answered with a status we will not retry.
    #[error("http {status} from {url}")]
    Http { status: u16, url: String },

    /// The page that arrived was not the site.
    ///
    /// Cloudflare's browser challenge (`HTTP 403`, a `Cf-Mitigated` header), or a
    /// page that cannot be read as the novel page it should be — a filter, a
    /// captive portal, a proxy, or markup the site has changed. See
    /// [`net::blocked`](crate::net::blocked).
    ///
    /// Neither is a parse failure, which is what the adapter would have reported
    /// instead. The evidence is a line per fact — the status, what arrived, and
    /// the text that gives it away — because no list of vendor phrases can name
    /// every filter in every country: the page names itself.
    ///
    /// The URL is a field rather than part of the message on purpose: callers
    /// (the CLI) already name it, and the message reads worse for repeating it.
    #[error("{kind} (HTTP {status})\n  {evidence}")]
    Blocked {
        status: u16,
        url: String,
        kind: crate::net::blocked::BlockKind,
        evidence: String,
    },

    /// A transport-level failure (timeout, DNS, reset) after retries.
    #[error("network error for {url}: {source}")]
    Network {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    /// `robots.txt` asked us not to fetch this path.
    #[error("robots.txt disallows {0}")]
    RobotsDisallowed(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
