//! Sessions the user owns.
//!
//! A *session* is a cookie jar the user exported from their own browser, given a
//! name, and handed to the tool. It exists because some sites answer an ordinary
//! client with Cloudflare's browser challenge (#17, #20): the tool will not solve
//! a challenge, borrow a browser, or spoof its way past one, so the only honest
//! way in is a credential the user provides deliberately.
//!
//! Three things live here:
//!
//! - [`cookie`] — what a cookie is, and the rules for where it may be sent,
//! - [`netscape`] — reading the `cookies.txt` every browser export extension and
//!   `curl` already write,
//! - and the [store](SessionStore) — one file per session under the user's data
//!   directory, with `save`, `list` and `delete` (the three buttons the desktop
//!   app will show, #21).
//!
//! A session is a credential, and the code treats it like one: no value is ever
//! written to a log, an error, `list` output, or `Debug`.

pub mod cookie;
pub mod netscape;
