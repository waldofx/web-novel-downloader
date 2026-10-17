//! Core of `web-novel-downloader`.
//!
//! This crate owns everything that is *not* specific to a single website:
//!
//! - the [`model`] of a downloaded novel (manifest, chapters, node tree),
//! - [`storage`] of that model on disk,
//! - the [`policy`] for fetching politely, and the [`rate_limit`]r enforcing it.
//!
//! Site-specific code lives in `wnd-sources` and produces the types defined here.

pub mod error;
pub mod model;
pub mod policy;
pub mod rate_limit;
pub mod storage;

pub use error::{Error, Result};
pub use model::{
    Block, Chapter, ChapterMeta, ContentRating, FORMAT_VERSION, Inline, Manifest, SourceInfo,
};
pub use policy::FetchPolicy;
pub use rate_limit::RateLimiter;
pub use storage::NovelDir;
