//! Error type shared across the workspace.

/// Errors produced by the core crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),

    /// The on-disk format is a different version than this build understands.
    ///
    /// The format is versioned so a future translation stage can rely on it.
    #[error("unsupported format version {found}; this build expects {expected}")]
    FormatVersion { found: u32, expected: u32 },

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
