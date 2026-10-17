//! Everything about talking to the network politely.
//!
//! - [`policy`] — the knobs: delays, retries, user agent, the robots switch.
//! - [`rate_limit`] — per-host spacing derived from the policy.
//! - [`robots`] — `robots.txt` parsing and the allow/deny decision.
//! - [`fetch`] — the client that combines policy, rate limiting, robots,
//!   retries, and the raw cache.
//! - [`blocked`] — recognising a filter page or a bot challenge, the two
//!   answers that are not the site.

pub mod blocked;
pub mod fetch;
pub mod policy;
pub mod rate_limit;
pub mod robots;
