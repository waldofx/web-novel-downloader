//! Reading the cookie file every browser export extension writes.
//!
//! The **Netscape `cookies.txt`** format, because it is the one thing they all
//! agree on: Chrome, Firefox and Safari extensions write it, `curl -b` reads it,
//! and a user who has ever scripted a login already has one. Nothing here needs
//! to be the only way in.
//!
//! A line is seven tab-separated fields (`\t` stands for a real tab here):
//!
//! ```text
//! .pixiv.net\tTRUE\t/\tTRUE\t1735689600\tPHPSESSID\tabc123
//! #HttpOnly_.pixiv.net\tTRUE\t/\tTRUE\t0\tPHPSESSID\tabc123
//! ```
//!
//! | Field | Meaning |
//! | --- | --- |
//! | domain | `.` in front means subdomains are included |
//! | subdomains | `TRUE`/`FALSE`, the same thing said again |
//! | path | always starts with `/` |
//! | secure | `TRUE`/`FALSE` |
//! | expiry | Unix seconds; `0` means a session cookie |
//! | name, value | as set |
//!
//! Lines starting with `#` are comments — except `#HttpOnly_`, which is a marker
//! on a real cookie and not a comment at all, the one piece of this format that
//! catches everyone once.
//!
//! Errors name the **line**, never the content: a malformed line is still a line
//! holding a credential, and an error message ends up in terminals, logs and
//! bug reports.

use crate::error::{Error, Result};
use crate::session::cookie::Cookie;

/// The `#HttpOnly_` marker a browser writes in front of a cookie's domain.
const HTTP_ONLY: &str = "#HttpOnly_";

/// Every cookie in `text`, in the order the file lists them.
///
/// Blank lines and comments are skipped; duplicate `(domain, path, name)` triples
/// keep the last one, which is what a browser does with two.
pub fn parse(text: &str) -> Result<Vec<Cookie>> {
    // An editor that saves as "UTF-8 with BOM" puts one at the head of the file,
    // where it becomes part of the first domain and breaks a line that looks fine.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut cookies: Vec<Cookie> = Vec::new();

    for (offset, raw) in text.lines().enumerate() {
        let number = offset + 1;
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if line.trim().is_empty() {
            continue;
        }

        // `#HttpOnly_` is a marker on a real cookie; every other `#` is a comment.
        let line = match line.strip_prefix(HTTP_ONLY) {
            Some(cookie) => cookie,
            None if line.starts_with('#') => continue,
            None => line,
        };

        let fields: Vec<&str> = line.splitn(7, '\t').collect();
        if fields.len() != 7 {
            return Err(bad_line(
                number,
                &format!("expected 7 tab-separated fields, found {}", fields.len()),
            ));
        }
        let [domain, subdomains, path, secure, expiry, name, value] = fields[..] else {
            return Err(bad_line(number, "expected 7 tab-separated fields"));
        };

        let domain = domain.trim_start_matches('.');
        if domain.is_empty() {
            return Err(bad_line(number, "the domain is empty"));
        }
        if !path.starts_with('/') {
            return Err(bad_line(number, "the path must start with /"));
        }
        if name.is_empty() {
            return Err(bad_line(number, "the cookie has no name"));
        }

        let cookie = Cookie {
            domain: domain.to_string(),
            host_only: !flag(subdomains, number)?,
            path: path.to_string(),
            secure: flag(secure, number)?,
            expires_at: match expiry.trim() {
                "" | "0" => None,
                seconds => Some(seconds.parse::<u64>().map_err(|_| {
                    bad_line(
                        number,
                        "the expiry must be Unix seconds, or 0 for a session cookie",
                    )
                })?),
            },
            name: name.to_string(),
            value: value.to_string(),
        };

        // A browser keeps one cookie per (domain, path, name); so does this.
        match cookies.iter_mut().find(|existing| {
            existing.domain.eq_ignore_ascii_case(&cookie.domain)
                && existing.path == cookie.path
                && existing.name == cookie.name
        }) {
            Some(existing) => *existing = cookie,
            None => cookies.push(cookie),
        }
    }

    Ok(cookies)
}

/// A `TRUE`/`FALSE` field.
fn flag(value: &str, number: usize) -> Result<bool> {
    if value.eq_ignore_ascii_case("TRUE") {
        Ok(true)
    } else if value.eq_ignore_ascii_case("FALSE") {
        Ok(false)
    } else {
        Err(bad_line(number, "a flag must be TRUE or FALSE"))
    }
}

/// A failure on one line of the file.
fn bad_line(line: usize, reason: &str) -> Error {
    Error::CookieFile {
        line,
        reason: reason.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Four real fields, then a session cookie that never expires.
    const EXPORT: &str = "\
# Netscape HTTP Cookie File
# https://curl.se/docs/http-cookies.html
# This file was generated by libcurl! Edit at your own risk.

.pixiv.net\tTRUE\t/\tTRUE\t1735689600\tPHPSESSID\tsecret-value
www.syosetu.org\tFALSE\t/novel\tFALSE\t0\tsession_id\tabc-123
";

    fn cookies(text: &str) -> Vec<Cookie> {
        parse(text).expect("a parse")
    }

    #[test]
    fn a_browser_export_reads_back_field_for_field() {
        let cookies = cookies(EXPORT);
        assert_eq!(cookies.len(), 2, "comments and blank lines are not cookies");

        let pixiv = &cookies[0];
        assert_eq!(
            pixiv.domain, "pixiv.net",
            "the leading dot is not part of the name"
        );
        assert_eq!(pixiv.path, "/");
        assert!(pixiv.secure);
        assert_eq!(pixiv.expires_at, Some(1_735_689_600));
        assert_eq!(pixiv.name, "PHPSESSID");
        assert_eq!(pixiv.value, "secret-value");
        assert!(!pixiv.host_only);

        let syosetu = &cookies[1];
        assert_eq!(syosetu.domain, "www.syosetu.org");
        assert_eq!(syosetu.path, "/novel");
        assert!(!syosetu.secure);
        assert_eq!(syosetu.expires_at, None, "0 means a session cookie");
        assert!(syosetu.host_only);
    }

    #[test]
    fn an_http_only_line_is_a_cookie_not_a_comment() {
        let cookies = cookies("#HttpOnly_.pixiv.net\tTRUE\t/\tTRUE\t0\tPHPSESSID\tv\n");
        assert_eq!(cookies.len(), 1);
        assert_eq!(cookies[0].domain, "pixiv.net");
        assert_eq!(cookies[0].name, "PHPSESSID");
    }

    #[test]
    fn the_last_of_two_cookies_with_the_same_identity_wins() {
        let cookies = cookies(
            "pixiv.net\tFALSE\t/\tFALSE\t0\ta\told\npixiv.net\tFALSE\t/\tFALSE\t0\ta\tnew\n",
        );
        assert_eq!(cookies.len(), 1);
        assert_eq!(cookies[0].value, "new");
    }

    #[test]
    fn a_windows_line_ending_is_not_part_of_the_value() {
        let cookies = cookies("pixiv.net\tFALSE\t/\tFALSE\t0\ta\tvalue\r\n");
        assert_eq!(cookies[0].value, "value");
    }

    #[test]
    fn a_byte_order_mark_does_not_break_the_first_line() {
        let cookies = cookies("\u{feff}pixiv.net\tFALSE\t/\tFALSE\t0\ta\tvalue\n");
        assert_eq!(
            cookies.len(),
            1,
            "an editor that adds a BOM must not spoil the file"
        );
        assert_eq!(cookies[0].value, "value");
    }

    #[test]
    fn spaces_and_tabs_survive_in_a_value() {
        let cookies = cookies("pixiv.net\tFALSE\t/\tFALSE\t0\ta\tone two\tthree\n");
        assert_eq!(cookies[0].value, "one two\tthree");
    }

    #[test]
    fn a_line_with_too_few_fields_names_the_line_and_not_the_secret() {
        let error = parse("pixiv.net\tFALSE\tSECRET\n").expect_err("three fields is not a cookie");
        let message = error.to_string();
        assert!(message.contains("line 1"), "{message}");
        assert!(
            !message.contains("SECRET"),
            "an error must not quote the file: {message}"
        );
    }

    #[test]
    fn a_field_that_is_not_true_or_false_is_refused() {
        let error =
            parse("pixiv.net\tMAYBE\t/\tFALSE\t0\ta\tv\n").expect_err("MAYBE is not a flag");
        assert!(error.to_string().contains("line 1"), "{error}");
    }

    #[test]
    fn an_expiry_that_is_not_a_number_is_refused() {
        let error = parse("pixiv.net\tFALSE\t/\tFALSE\tyesterday\ta\tv\n")
            .expect_err("an expiry must be a number");
        assert!(error.to_string().contains("line 1"), "{error}");
    }

    #[test]
    fn a_path_that_is_not_absolute_is_refused() {
        let error =
            parse("pixiv.net\tFALSE\tnovel\tFALSE\t0\ta\tv\n").expect_err("paths start with /");
        assert!(error.to_string().contains("line 1"), "{error}");
    }

    #[test]
    fn a_nameless_cookie_is_refused() {
        let error =
            parse("pixiv.net\tFALSE\t/\tFALSE\t0\t\tv\n").expect_err("a cookie needs a name");
        assert!(error.to_string().contains("line 1"), "{error}");
    }

    #[test]
    fn an_empty_file_is_no_cookies_rather_than_an_error() {
        assert!(cookies("").is_empty());
        assert!(cookies("# just a header\n\n").is_empty());
    }
}
