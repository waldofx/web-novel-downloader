//! Core of `web-novel-downloader`.
//!
//! This crate owns everything that is *not* specific to a single website, split
//! by feature into two modules:
//!
//! - [`store`] — the [`model`](store::model) of a downloaded novel, and its
//!   [`storage`](store::storage) on disk,
//! - [`net`] — fetching politely: the [`policy`](net::policy), the
//!   [`rate_limit`](net::rate_limit)r enforcing it, [`robots`](net::robots)
//!   rules, and the [`fetch`](net::fetch) layer that applies them.
//!
//! The types consumers reach for most are re-exported at the crate root, so
//! `wnd_core::Manifest` works as well as `wnd_core::store::model::Manifest`.
//! Site-specific code lives in `wnd-sources` and produces these types.

pub mod error;
pub mod net;
pub mod store;

pub use error::{Error, Result};
pub use net::fetch::{HttpClient, RawCache};
pub use net::policy::FetchPolicy;
pub use net::rate_limit::RateLimiter;
pub use net::robots::RobotsRules;
pub use store::model::{
    Block, Chapter, ChapterMeta, ContentRating, FORMAT_VERSION, Inline, Manifest, SourceInfo,
};
pub use store::storage::{NovelDir, Written};
