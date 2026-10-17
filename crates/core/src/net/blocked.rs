//! Recognising the answers that are not the site.
//!
//! A novel page is expected, and something else arrives: a DNS answer hijacked to
//! a filter box, a captive portal in a hotel, a transparent proxy, Cloudflare's
//! browser challenge, or the site itself after it changed its markup.
//!
//! The first version of this module knew the two filter pages we had actually
//! seen, by phrase (`SITUS DIBLOKIR`, `internetsehat.firstmedia.com`). A list like
//! that only ever covers the past: it says nothing about a block page in another
//! country, in another language, from a provider we have never met. What is here
//! now is only what can be said without knowing a vendor, a language, or a
//! country:
//!
//! - **Cloudflare's challenge**, from its `Cf-Mitigated` response header. A header
//!   is protocol-level, and nothing else sends it.
//! - **[`Unexpected`]**, the facts about an answer that is not the site's shape:
//!   the status, the size, the media type, the host that answered, and the
//!   readable text at the head of the body. That text is the evidence, and it is
//!   why this works anywhere: a block page announces itself, in its own words.
//!
//! Nothing here matches page text. The one thing a body is used for is being
//! *shown*.
//!
//! This module only *reports*. It does not try to get past a challenge or a
//! filter: no user-agent spoofing, no borrowing of browser cookies, no retrying a
//! challenge. Reporting honestly is the whole job — see `Ways past, and the ones
//! we refuse` in `docs/engineering/fetch.md`.

use crate::error::Error;
use crate::net::fetch::Page;
use reqwest::Url;
use std::fmt;

/// What answered instead of the site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    /// Cloudflare's browser challenge, named by its `Cf-Mitigated` header.
    BotChallenge,
    /// A page arrived that cannot be read as the novel page it should be: a
    /// filter, a captive portal, a proxy — or markup the site has changed. Which
    /// of those it is, the evidence says.
    NotANovelPage,
}

impl fmt::Display for BlockKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::BotChallenge => "Cloudflare served its browser challenge instead of the site",
            Self::NotANovelPage => "the page that arrived is not a novel page",
        })
    }
}

/// A page that is not the site, and the evidence that gave it away.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockPage {
    pub kind: BlockKind,
    /// Ready to print under the kind sentence, one fact per line.
    pub evidence: String,
}

/// Cloudflare's browser challenge, when its header names one.
///
/// The header is the whole rule, and that is deliberate: a challenge page that
/// arrives without it is reported by [`Unexpected`] with the page's own words
/// instead, which is better evidence than any phrase list we could keep — and it
/// covers the challenges that are not Cloudflare's.
pub fn challenge(cf_mitigated: Option<&str>) -> Option<BlockPage> {
    let value = cf_mitigated?;
    Some(BlockPage {
        kind: BlockKind::BotChallenge,
        evidence: format!("the response header Cf-Mitigated: {value}"),
    })
}

/// How much of a body's readable text is quoted.
const EXCERPT_CHARS: usize = 240;

/// What arrived instead of the site.
///
/// Every field is a fact about the answer, and together they are what a reader
/// needs to recognise their own network: a filter page is small, comes back
/// `200`, and says what it is in the language of whoever runs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unexpected {
    pub status: u16,
    pub bytes: usize,
    /// The media type alone: `text/html`, not `text/html; charset=utf-8`.
    pub content_type: Option<String>,
    /// Readable text from the head of the body, whitespace collapsed. Empty when
    /// there is no readable text at all — which happens: the captured First Media
    /// page is nothing but scripts, styles and a spacer image.
    pub excerpt: String,
    /// The host that was asked for.
    pub host: String,
    /// The host that finally answered, when it is not the one asked for.
    pub reached: Option<String>,
}

impl Unexpected {
    /// The facts of the answer `page`.
    pub fn of(page: &Page) -> Self {
        Self {
            status: page.status,
            bytes: page.body.len(),
            content_type: page.content_type.as_deref().map(media_type),
            excerpt: excerpt(&page.body),
            host: host_of(&page.url).unwrap_or_else(|| page.url.clone()),
            reached: reached_host(page),
        }
    }

    /// The evidence lines, ready to print under the kind sentence.
    ///
    /// `expected` is what was being looked for, article included — `a table of
    /// contents`. `intercepted` is the finding from
    /// [`HttpClient::answers_every_url_the_same`](crate::net::fetch::HttpClient::answers_every_url_the_same),
    /// which the caller makes because the caller has the client.
    pub fn evidence(&self, expected: &str, intercepted: bool) -> String {
        let mut lines = vec![self.answer_line(expected)];

        if !self.excerpt.is_empty() {
            lines.push(format!("it begins: \"{}\"", self.excerpt));
        }
        if let Some(reached) = &self.reached {
            lines.push(format!(
                "the answer came from {reached}, not from {}",
                self.host
            ));
        }
        if intercepted {
            lines.push(format!(
                "{} answered a URL that cannot exist with this same page, byte for byte: \
                 a DNS hijack, a captive portal, or a content filter is answering for it",
                self.host
            ));
        }

        lines.join("\n  ")
    }

    /// What was expected, and what arrived instead.
    fn answer_line(&self, expected: &str) -> String {
        if self.bytes == 0 {
            return format!("{expected} was expected, and an empty answer arrived instead");
        }
        match &self.content_type {
            Some(content_type) => format!(
                "{expected} was expected; {} bytes of {content_type} arrived instead",
                self.bytes
            ),
            None => format!(
                "{expected} was expected; {} bytes arrived instead",
                self.bytes
            ),
        }
    }
}

/// The error for a page that is not the site's shape, with its evidence.
pub(crate) fn not_the_site(page: &Page, expected: &str, intercepted: bool) -> Error {
    Error::Blocked {
        status: page.status,
        url: page.url.clone(),
        kind: BlockKind::NotANovelPage,
        evidence: Unexpected::of(page).evidence(expected, intercepted),
        session: None,
        session_note: String::new(),
    }
}

/// `text/html; charset=utf-8` → `text/html`.
fn media_type(raw: &str) -> String {
    raw.split(';')
        .next()
        .unwrap_or(raw)
        .trim()
        .to_ascii_lowercase()
}

/// The host of a URL, when it has one.
fn host_of(url: &str) -> Option<String> {
    Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
}

/// The host that finally answered, when it is not the host asked for.
///
/// Ports are ignored: a redirect that only changes the port is not a different
/// server in any sense a reader cares about.
fn reached_host(page: &Page) -> Option<String> {
    let asked = host_of(&page.url);
    let answered = host_of(&page.final_url);
    match (asked, answered) {
        (Some(asked), Some(answered)) if !asked.eq_ignore_ascii_case(&answered) => Some(answered),
        _ => None,
    }
}

/// The readable text at the head of `body`.
///
/// This is the evidence, so it is worth being careful about: tags, comments and
/// the contents of `<script>`/`<style>` are dropped (a block page's first
/// kilobyte is usually markup, a tag manager and a stylesheet, none of which a
/// reader recognises), every tag leaves a word boundary behind, whitespace is
/// collapsed, the few entities that would otherwise print as noise are decoded,
/// and the result is cut to [`EXCERPT_CHARS`] on a character boundary.
fn excerpt(body: &str) -> String {
    cut(&tidy(&text_of(body)))
}

/// Everything `body` says in text.
///
/// A tag becomes a space rather than nothing, so `<h1>blocked</h1>by order`
/// reads as `blocked by order` instead of running two words together.
fn text_of(body: &str) -> String {
    // Indices into an ASCII-lowercased copy line up with the original, because
    // ASCII case folding never changes a byte's length.
    let lower = body.to_ascii_lowercase();
    let mut text = String::new();
    let mut at = 0usize;

    while at < body.len() {
        let Some(offset) = lower[at..].find('<') else {
            text.push_str(&body[at..]);
            break;
        };
        text.push_str(&body[at..at + offset]);
        at += offset;
        text.push(' ');

        if lower[at..].starts_with("<!--") {
            // A comment, to its end — or to the end of the body, if it never
            // closes.
            match lower[at..].find("-->") {
                Some(end) => at += end + "-->".len(),
                None => break,
            }
            continue;
        }

        // A script or a stylesheet: drop what is inside it too.
        if let Some(open) = ["<script", "<style"]
            .iter()
            .find(|open| lower[at..].starts_with(*open))
        {
            let close = format!("</{}>", &open[1..]);
            match lower[at..].find(&close) {
                Some(end) => at += end + close.len(),
                None => break,
            }
            continue;
        }

        // Any other tag: to its `>`, or to the end of the body if it never comes.
        match lower[at..].find('>') {
            Some(end) => at += end + 1,
            None => break,
        }
    }

    text
}

/// `text` with the handful of entities that would print as noise decoded, every
/// run of whitespace collapsed to one space, and both ends trimmed.
fn tidy(raw: &str) -> String {
    const ENTITIES: [(&str, char); 7] = [
        ("&amp;", '&'),
        ("&lt;", '<'),
        ("&gt;", '>'),
        ("&quot;", '"'),
        ("&apos;", '\''),
        ("&#39;", '\''),
        ("&nbsp;", ' '),
    ];

    let mut out = String::with_capacity(raw.len().min(EXCERPT_CHARS * 2));
    let mut pending_space = false;
    let mut at = 0usize;

    while at < raw.len() {
        let (ch, width) = match ENTITIES
            .iter()
            .find(|(entity, _)| raw[at..].starts_with(*entity))
        {
            Some((entity, ch)) => (*ch, entity.len()),
            None => {
                let ch = raw[at..].chars().next().expect("a character boundary");
                (ch, ch.len_utf8())
            }
        };
        at += width;

        if ch.is_whitespace() {
            // A space is only ever emitted *before* the next word, so neither
            // leading nor trailing whitespace survives.
            pending_space = !out.is_empty();
            continue;
        }
        if pending_space {
            out.push(' ');
            pending_space = false;
        }
        out.push(ch);
    }

    out
}

/// The head of `text`, cut to [`EXCERPT_CHARS`] characters, with an ellipsis when
/// there was more.
fn cut(text: &str) -> String {
    let mut chars = text.chars();
    let head: String = chars.by_ref().take(EXCERPT_CHARS).collect();
    if chars.next().is_some() {
        return format!("{head}…");
    }
    head
}

#[cfg(test)]
mod tests {
    use super::*;

    /// First Media's "Internet Sehat" page, captured from `http://syosetu.org/`
    /// on 2026-10-17 (`HTTP 200`) — the DNS answer had been hijacked. Note what
    /// this page is: scripts, styles, and a spacer image, with no readable text
    /// at all.
    const INTERNET_SEHAT: &str = include_str!("../../tests/fixtures/blocked/internet-sehat.html");

    /// The Komdigi/Trustpositif notice, as pasted from the page the user's
    /// network served. Only its own words are used here: the filter box handed a
    /// *different* template to a VPN exit, so this page could not be captured
    /// from this side.
    const KOMDIGI_NOTICE: &str = concat!(
        "<html><head><title>Trustpositif</title></head><body>",
        "<h1>SITUS DIBLOKIR / WEBSITE BLOCKED</h1>",
        "<p>MAAF, AKSES KE SITUS INI DIBLOKIR OLEH PEMERINTAH INDONESIA KARENA ",
        "MENGANDUNG KONTEN NEGATIF</p>",
        "<p>aduanpseprivat@komdigi.go.id</p></body></html>",
    );

    fn page(status: u16, url: &str, body: &str) -> Page {
        Page {
            status,
            url: url.to_string(),
            final_url: url.to_string(),
            content_type: Some("text/html; charset=utf-8".to_string()),
            body: body.to_string(),
        }
    }

    #[test]
    fn a_challenge_is_recognised_from_its_header() {
        let page = challenge(Some("challenge")).expect("the header names a challenge");

        assert_eq!(page.kind, BlockKind::BotChallenge);
        assert!(
            page.evidence.contains("Cf-Mitigated: challenge"),
            "evidence must name the header: {}",
            page.evidence
        );
    }

    #[test]
    fn no_header_is_no_challenge() {
        assert_eq!(challenge(None), None, "nothing else may claim a challenge");
    }

    #[test]
    fn the_excerpt_is_the_text_of_the_page() {
        assert_eq!(
            excerpt(KOMDIGI_NOTICE),
            "Trustpositif SITUS DIBLOKIR / WEBSITE BLOCKED MAAF, AKSES KE SITUS INI \
             DIBLOKIR OLEH PEMERINTAH INDONESIA KARENA MENGANDUNG KONTEN NEGATIF \
             aduanpseprivat@komdigi.go.id"
        );
    }

    #[test]
    fn the_excerpt_of_a_page_that_is_all_scripts_is_empty() {
        // The real capture: a tag manager, a stylesheet, an ad script and a
        // spacer image. Quoting any of it would be quoting nothing.
        assert_eq!(excerpt(INTERNET_SEHAT), "");
    }

    #[test]
    fn the_excerpt_drops_scripts_styles_and_comments() {
        let body = concat!(
            "<style>.all{display:block}</style>",
            "<!-- Google Tag Manager -->",
            "<script>var gtm = 'https://example.test/gtm.js';</script>",
            "<p>ACCESS DENIED</p>",
        );

        assert_eq!(excerpt(body), "ACCESS DENIED");
    }

    #[test]
    fn a_tag_leaves_a_word_boundary_behind() {
        assert_eq!(excerpt("<h1>blocked</h1>by order"), "blocked by order");
        assert_eq!(excerpt("<li>one</li><li>two</li>"), "one two");
    }

    #[test]
    fn whitespace_is_collapsed() {
        assert_eq!(excerpt("<p>  a\n\n  b\tto</p>\n c  "), "a b to c");
        assert_eq!(excerpt("<p>   </p>"), "", "whitespace alone is no evidence");
    }

    #[test]
    fn entities_that_would_print_as_noise_are_decoded() {
        assert_eq!(
            excerpt("a&amp;b&lt;c&gt;d&quot;e&#39;f&apos;g&nbsp;h"),
            "a&b<c>d\"e'f'g h"
        );
    }

    #[test]
    fn an_unclosed_tag_or_comment_does_not_lose_the_text_before_it() {
        assert_eq!(excerpt("<p>kept</p><p>gone"), "kept gone");
        assert_eq!(excerpt("<p>kept</p><!-- unclosed"), "kept");
        assert_eq!(excerpt("<p>kept</p><script>var x"), "kept");
    }

    #[test]
    fn the_excerpt_is_cut_on_a_character_boundary() {
        // Multi-byte characters across the limit: cutting by bytes would panic.
        let body = format!("<p>{}</p>", "あ".repeat(EXCERPT_CHARS * 4));
        let excerpt = excerpt(&body);

        assert_eq!(excerpt.chars().count(), EXCERPT_CHARS + 1, "the ellipsis");
        assert!(excerpt.ends_with('…'));
        assert_eq!(
            excerpt.chars().filter(|ch| *ch == 'あ').count(),
            EXCERPT_CHARS
        );
    }

    #[test]
    fn a_short_excerpt_gets_no_ellipsis() {
        assert!(!excerpt("<p>short</p>").ends_with('…'));
    }

    #[test]
    fn the_facts_describe_the_answer() {
        let page = page(200, "https://example.test/novel/1/", KOMDIGI_NOTICE);
        let unexpected = Unexpected::of(&page);

        assert_eq!(unexpected.status, 200);
        assert_eq!(unexpected.bytes, KOMDIGI_NOTICE.len());
        assert_eq!(
            unexpected.content_type.as_deref(),
            Some("text/html"),
            "parameters are not part of the media type"
        );
        assert_eq!(unexpected.host, "example.test");
        assert_eq!(unexpected.reached, None, "nothing redirected");
        assert!(
            unexpected
                .excerpt
                .starts_with("Trustpositif SITUS DIBLOKIR")
        );
    }

    #[test]
    fn the_answer_from_another_host_is_noted() {
        let mut page = page(200, "https://syosetu.org/novel/1/", KOMDIGI_NOTICE);
        page.final_url = "https://filter.example/blocked".to_string();

        assert_eq!(
            Unexpected::of(&page).reached.as_deref(),
            Some("filter.example")
        );
    }

    #[test]
    fn the_same_host_in_another_case_is_not_another_host() {
        let mut page = page(200, "https://syosetu.org/novel/1/", KOMDIGI_NOTICE);
        page.final_url = "https://SYOSETU.org/novel/1/".to_string();

        assert_eq!(Unexpected::of(&page).reached, None);
    }

    #[test]
    fn the_evidence_names_the_expectation_and_what_arrived() {
        let unexpected = Unexpected::of(&page(200, "https://example.test/n/", "<p>BLOCKED</p>"));

        assert_eq!(
            unexpected.evidence("a table of contents", false),
            format!(
                "a table of contents was expected; {} bytes of text/html arrived instead\n  \
                 it begins: \"BLOCKED\"",
                "<p>BLOCKED</p>".len()
            )
        );
    }

    #[test]
    fn the_evidence_says_when_the_answer_was_empty() {
        let unexpected = Unexpected::of(&page(200, "https://example.test/n/", ""));

        assert_eq!(
            unexpected.evidence("a chapter body", false),
            "a chapter body was expected, and an empty answer arrived instead"
        );
    }

    #[test]
    fn the_evidence_says_when_another_host_answered() {
        let mut page = page(200, "https://syosetu.org/novel/1/", "<p>BLOCKED</p>");
        page.final_url = "https://filter.example/blocked".to_string();

        let evidence = Unexpected::of(&page).evidence("a table of contents", false);

        assert!(
            evidence.ends_with("the answer came from filter.example, not from syosetu.org"),
            "got: {evidence}"
        );
    }

    #[test]
    fn the_evidence_names_the_interception_when_the_host_answered_everything_the_same() {
        let unexpected = Unexpected::of(&page(200, "http://syosetu.org/novel/1/", "<p>x</p>"));

        let evidence = unexpected.evidence("a table of contents", true);

        assert!(
            evidence.contains(
                "syosetu.org answered a URL that cannot exist with this same page, byte for byte"
            ),
            "got: {evidence}"
        );
    }

    #[test]
    fn the_kind_renders_the_sentence_the_user_reads() {
        assert_eq!(
            BlockKind::BotChallenge.to_string(),
            "Cloudflare served its browser challenge instead of the site"
        );
        assert_eq!(
            BlockKind::NotANovelPage.to_string(),
            "the page that arrived is not a novel page"
        );
    }
}
