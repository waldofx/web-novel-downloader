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
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
