//! Recognising the pages that are *not* the site.
//!
//! Two things answer where a novel page was expected, and neither is a parse
//! failure:
//!
//! - a **content filter** — the DNS answer was hijacked, or a transparent proxy
//!   rewrote the request — which replies with a block notice and, insultingly,
//!   `HTTP 200`;
//! - **Cloudflare's browser challenge**, which answers a non-browser client with
//!   `HTTP 403` and a `Just a moment...` page that only JavaScript can pass.
//!
//! Both are recognised here, before an adapter can mistake them for markup, and
//! reported with the text that gave them away. The markers are pinned to real
//! captures in `crates/core/tests/fixtures/blocked/`; the reasoning is in
//! `docs/engineering/fetch.md`.
//!
//! This module only *recognises* these pages. It does not try to get past them:
//! no user-agent spoofing, no borrowing of browser cookies, no retrying a
//! challenge. Reporting honestly is the whole job — see `Ways past, and the ones
//! we refuse` in `docs/engineering/fetch.md`.

/// What answered instead of the site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    /// An ISP content filter: Trustpositif/Komdigi, First Media's "Internet
    /// Sehat", and their kin.
    ContentFilter,
    /// Cloudflare's browser challenge.
    BotChallenge,
}

impl std::fmt::Display for BlockKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::ContentFilter => {
                "the network served an ISP content-filter page instead of the site"
            }
            Self::BotChallenge => "Cloudflare served its browser challenge instead of the site",
        })
    }
}

/// A page that is not the site, and the evidence that gave it away.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockPage {
    pub kind: BlockKind,
    /// A short phrase naming the evidence, ready to print after a colon.
    pub evidence: String,
}

/// The Komdigi/Trustpositif notice quotes, seen on the page the user's own
/// network serves for `http://syosetu.org/`.
///
/// They are long on purpose: "BLOCKED" or "DIBLOKIR" alone would match a novel,
/// while these whole phrases are a filter announcing itself.
const KOMDIGI_MARKERS: [&str; 2] = ["SITUS DIBLOKIR", "DIBLOKIR OLEH PEMERINTAH INDONESIA"];

/// Markers from the other template on the same network: First Media's
/// "Internet Sehat" page, captured in `tests/fixtures/blocked/internet-sehat.html`.
const FIRST_MEDIA_MARKERS: [&str; 2] = ["internetsehat.firstmedia.com", "internet-sehat"];

/// How much of a body is scanned.
///
/// A filter page is a couple of kilobytes including its scripts, and Cloudflare's
/// challenge is smaller. Scanning a prefix keeps the cost flat for a 200 KB
/// chapter and keeps the search away from the novel text in the middle of a
/// document, which is where a false positive would have to hide.
const SCAN_LIMIT: usize = 16 * 1024;

/// Classify `body` — and Cloudflare's `Cf-Mitigated` header, when there is one —
/// as a page that is not the site.
///
/// `None` means "no reason to think this is anything but the site", which is the
/// answer for every ordinary page.
pub fn classify(cf_mitigated: Option<&str>, body: &str) -> Option<BlockPage> {
    let head = scan_prefix(body).to_ascii_lowercase();

    if let Some(marker) = KOMDIGI_MARKERS
        .iter()
        .chain(FIRST_MEDIA_MARKERS.iter())
        .find(|marker| head.contains(&marker.to_ascii_lowercase()))
    {
        return Some(BlockPage {
            kind: BlockKind::ContentFilter,
            evidence: format!("the page says \"{marker}\""),
        });
    }

    // A challenge page names itself twice: `Just a moment...` in the title and
    // Cloudflare's challenge script below it. The title alone is not enough — a
    // chapter may well contain those words — so it takes both to count, and the
    // quote from the page is the better evidence when there is one.
    let challenge_page = head.contains("just a moment")
        && (head.contains("challenge-platform") || head.contains("__cf_chl"));
    if challenge_page {
        return Some(BlockPage {
            kind: BlockKind::BotChallenge,
            evidence: "the page says \"Just a moment...\"".to_string(),
        });
    }

    // Nothing readable in the body, but Cloudflare named itself in a header —
    // and nothing else sends that header — so that is enough on its own.
    if let Some(value) = cf_mitigated {
        return Some(BlockPage {
            kind: BlockKind::BotChallenge,
            evidence: format!("the response header Cf-Mitigated: {value}"),
        });
    }

    None
}

/// The first [`SCAN_LIMIT`] bytes of `body`, cut on a character boundary.
fn scan_prefix(body: &str) -> &str {
    if body.len() <= SCAN_LIMIT {
        return body;
    }
    let mut end = SCAN_LIMIT;
    while !body.is_char_boundary(end) {
        end -= 1;
    }
    &body[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cloudflare's challenge, captured from `https://syosetu.org/novel/288118/`
    /// on 2026-10-17 (`HTTP 403`, `Cf-Mitigated: challenge`).
    const CLOUDFLARE_CHALLENGE: &str =
        include_str!("../../tests/fixtures/blocked/cloudflare-challenge.html");

    /// First Media's "Internet Sehat" page, captured from `http://syosetu.org/`
    /// on 2026-10-17 (`HTTP 200`) — the DNS answer had been hijacked.
    const INTERNET_SEHAT: &str = include_str!("../../tests/fixtures/blocked/internet-sehat.html");

    /// The Komdigi/Trustpositif notice, as pasted from the page the user's
    /// network served. Only its own strings are pinned here: the filter box
    /// handed a *different* template to a VPN exit, so this page could not be
    /// captured from this side.
    const KOMDIGI_NOTICE: &str = concat!(
        "<html><head><title>Trustpositif</title></head><body>",
        "<h1>SITUS DIBLOKIR / WEBSITE BLOCKED</h1>",
        "<p>MAAF, AKSES KE SITUS INI DIBLOKIR OLEH PEMERINTAH INDONESIA KARENA ",
        "MENGANDUNG KONTEN NEGATIF</p>",
        "<p>aduanpseprivat@komdigi.go.id</p></body></html>",
    );

    #[test]
    fn a_cloudflare_challenge_is_recognised_from_the_page() {
        let page = classify(None, CLOUDFLARE_CHALLENGE).expect("the challenge page must be caught");

        assert_eq!(page.kind, BlockKind::BotChallenge);
        assert!(
            page.evidence.contains("Just a moment"),
            "evidence must quote the page: {}",
            page.evidence
        );
    }

    #[test]
    fn the_cf_mitigated_header_alone_is_enough() {
        let page =
            classify(Some("challenge"), "<html></html>").expect("the header is Cloudflare's");

        assert_eq!(page.kind, BlockKind::BotChallenge);
        assert!(
            page.evidence.contains("Cf-Mitigated: challenge"),
            "evidence must name the header: {}",
            page.evidence
        );
    }

    #[test]
    fn a_first_media_filter_page_is_recognised() {
        let page = classify(None, INTERNET_SEHAT).expect("the filter page must be caught");

        assert_eq!(page.kind, BlockKind::ContentFilter);
        assert!(
            page.evidence.contains("internetsehat.firstmedia.com"),
            "evidence must quote the page: {}",
            page.evidence
        );
    }

    #[test]
    fn a_komdigi_filter_page_is_recognised() {
        let page = classify(None, KOMDIGI_NOTICE).expect("the block notice must be caught");

        assert_eq!(page.kind, BlockKind::ContentFilter);
        assert!(
            page.evidence.contains("SITUS DIBLOKIR"),
            "evidence must quote the page: {}",
            page.evidence
        );
    }

    #[test]
    fn a_chapter_that_merely_says_the_words_is_left_alone() {
        let chapter = concat!(
            "<div class=\"widget-episodeBody\">",
            "<p>Just a moment, she said, and the door closed behind her.</p>",
            "<p>The page was blocked, but only in the sense that she could not turn it.</p>",
            "</div>",
        );

        assert_eq!(classify(None, chapter), None);
    }

    #[test]
    fn a_marker_beyond_the_scanned_prefix_is_ignored() {
        let mut body = "x".repeat(SCAN_LIMIT + 64);
        body.push_str("SITUS DIBLOKIR");

        assert_eq!(
            classify(None, &body),
            None,
            "only the head of a body is searched, so a chapter full of filler cannot turn into a block notice"
        );
    }

    #[test]
    fn the_prefix_cut_lands_on_a_character_boundary() {
        // Multi-byte characters across the limit: slicing at a byte index would
        // panic, so the cut walks back to a boundary.
        let body = "あ".repeat(SCAN_LIMIT);
        let head = scan_prefix(&body);

        assert!(head.len() <= SCAN_LIMIT);
        assert!(body.starts_with(head));
    }

    #[test]
    fn an_ordinary_page_is_not_a_block_page() {
        assert_eq!(
            classify(None, "<html><body><p>novel</p></body></html>"),
            None
        );
        assert_eq!(classify(None, ""), None);
    }

    #[test]
    fn the_kind_renders_the_sentence_the_user_reads() {
        assert_eq!(
            BlockKind::ContentFilter.to_string(),
            "the network served an ISP content-filter page instead of the site"
        );
        assert_eq!(
            BlockKind::BotChallenge.to_string(),
            "Cloudflare served its browser challenge instead of the site"
        );
    }
}
