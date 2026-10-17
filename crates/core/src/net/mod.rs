//! Everything about talking to the network politely.
//!
//! - [`policy`] — the knobs: delays, retries, user agent, the robots switch.
//! - [`rate_limit`] — per-host spacing derived from the policy.
//! - [`robots`] — `robots.txt` parsing and the allow/deny decision.
//! - [`fetch`] — the client that combines policy, rate limiting, robots,
//!   retries, and the raw cache.

pub mod fetch;
pub mod policy;
pub mod rate_limit;
pub mod robots;
