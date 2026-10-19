//! The repository's one rule about other people's words: nothing from a real
//! novel is committed.
//!
//! A fixture, a doc example and a test all carry **made-up** words in the shape
//! of the real thing (#41): the site's own markup, the site's own vocabulary,
//! and placeholders where a work's title, prose or address would be. The raw
//! capture and the real works a live test runs against stay in the gitignored
//! `captures/` and `fixtures.local.json`.
//!
//! Half of that rule is mechanical, so it is checked here rather than trusted: a
//! work address in this repository has to point at a placeholder. The prose half
//! is not greppable, and that is what review is for.
//!
//! The check is deliberately about *shapes that are real*: `kakuyomu.jp/works/1`
//! is nobody's work, and only an address long enough to be a real one is an
//! offence. A URL written as a template (`<ncode>`) is how the docs spell "your
//! work here", and is not one either.

use std::fs;
use std::path::{Path, PathBuf};

/// What may follow a work-address marker.
#[derive(Clone, Copy)]
enum Allowed {
    /// A run of digits that has to start with this — `000`, `1000000`.
    Prefix(&'static str),
}

/// A marker, how long an id has to be before it counts as a real one, and what
/// a placeholder looks like.
struct Rule {
    marker: &'static str,
    /// An id shorter than this is obviously made up, so it is not an offence:
    /// `kakuyomu.jp/works/1` is nobody's work.
    real_length: usize,
    /// A real id's digits must start with this instead.
    allowed: Allowed,
}

const RULES: &[Rule] = &[
    Rule {
        marker: "ncode.syosetu.com/n",
        real_length: 6,
        allowed: Allowed::Prefix("000"),
    },
    Rule {
        marker: "novel18.syosetu.com/n",
        real_length: 6,
        allowed: Allowed::Prefix("000"),
    },
    Rule {
        marker: "syosetu.org/novel/",
        real_length: 4,
        allowed: Allowed::Prefix("000"),
    },
    Rule {
        marker: "syosetu.org/img/user/",
        real_length: 3,
        allowed: Allowed::Prefix("000"),
    },
    Rule {
        marker: "kakuyomu.jp/works/",
        real_length: 10,
        allowed: Allowed::Prefix("10000000"),
    },
    Rule {
        marker: "pixiv.net/novel/show.php?id=",
        real_length: 6,
        allowed: Allowed::Prefix("1000000"),
    },
    Rule {
        marker: "pixiv.net/novel/series/",
        real_length: 6,
        allowed: Allowed::Prefix("1000000"),
    },
];

/// Directories that hold generated files, another machine's captures, or a
/// manual run's leftovers — never what is committed.
const SKIP: &[&str] = &[
    "target",
    ".git",
    "captures",
    "node_modules",
    "ui-manual",
    "exports",
    "raw",
];

/// The gitignored locals, which is exactly where a real work belongs: the
/// capture a fixture was trimmed from, and the works a live test runs against.
const LOCAL: &[&str] = &["fixtures.local.json", "cookies.json"];

const SUFFIXES: &[&str] = &[
    "rs", "md", "html", "json", "toml", "js", "css", "txt", "yml", "yaml",
];

fn workspace() -> PathBuf {
    // `crates/sources` → the repository root.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the crate lives two levels under the root")
        .to_path_buf()
}

/// Every committed-looking text file, as `(path, contents)`.
fn sources(root: &Path) -> Vec<(PathBuf, String)> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(directory) = stack.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                if !SKIP.contains(&name.as_str()) && !name.starts_with('.') {
                    stack.push(path);
                }
                continue;
            }
            if LOCAL.contains(&name.as_str()) {
                continue;
            }
            let suffix = path
                .extension()
                .map(|extension| extension.to_string_lossy().into_owned())
                .unwrap_or_default();
            if !SUFFIXES.contains(&suffix.as_str()) {
                continue;
            }
            if let Ok(text) = fs::read_to_string(&path) {
                files.push((path, text));
            }
        }
    }
    files
}

/// The word after `marker` at `from`, up to the first character that cannot be
/// part of a work id.
fn id_after(text: &str, from: usize) -> &str {
    text[from..]
        .split(|c: char| !c.is_ascii_alphanumeric())
        .next()
        .unwrap_or("")
}

fn is_offence(id: &str, rule: &Rule) -> bool {
    if id.len() < rule.real_length || id.starts_with('<') {
        return false;
    }
    let digits: String = id.chars().take_while(char::is_ascii_digit).collect();
    let Allowed::Prefix(prefix) = rule.allowed;
    !digits.starts_with(prefix)
}

#[test]
fn no_committed_address_points_at_a_real_work() {
    let root = workspace();
    let mut offences = Vec::new();

    for (path, text) in sources(&root) {
        for rule in RULES {
            let mut from = 0;
            while let Some(found) = text[from..].find(rule.marker) {
                let at = from + found + rule.marker.len();
                let id = id_after(&text, at);
                if is_offence(id, rule) {
                    let line = text[..at].matches('\n').count() + 1;
                    offences.push(format!(
                        "{}:{line}: {}{id}",
                        path.strip_prefix(&root).unwrap_or(&path).display(),
                        rule.marker
                    ));
                }
                from = at;
            }
        }
    }

    assert!(
        offences.is_empty(),
        "a work address must point at a placeholder (see docs/engineering/testing.md, \
         `Fixtures`):\n{}",
        offences.join("\n")
    );
}
