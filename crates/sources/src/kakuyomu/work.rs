//! A Kakuyomu work page: the metadata and the episode list.
//!
//! Both come from the page's snapshot, not from its HTML, and that is a
//! deliberate choice forced by the site. A work page renders only the episodes
//! of the first chapter server-side and lazy-loads the rest: the work used to
//! write these tests shows **seven** episode links in its HTML and **1069**
//! episodes in its snapshot. An HTML reader would download one chapter of a
//! thousand-episode novel and say nothing. Of all the ways to be wrong, silently
//! downloading a fraction is the worst.
//!
//! The episode list has two shapes, and both are ordinary:
//!
//! - grouped into arcs (章), each with a title, like `第１章　覚醒`;
//! - one unnamed group, whose key is literally `TableOfContentsChapter:` — the
//!   empty id is not a bug in the page, it is what an un-arc'd work looks like.
//!
//! Either way the episodes are read in order and numbered from one, because
//! Kakuyomu's episode URLs carry opaque ids and no number at all.

use super::apollo::{Apollo, Reference};
use crate::SourceError;
use serde::Deserialize;
use url::Url;
use wnd_core::ChapterMeta;
use wnd_core::store::storage::chapter_file_name;

/// Everything the work page contributes to a manifest.
#[derive(Debug)]
pub(super) struct WorkPage {
    pub(super) title: String,
    pub(super) author: Option<String>,
    pub(super) synopsis: Option<String>,
    pub(super) tags: Vec<String>,
    pub(super) chapters: Vec<ChapterMeta>,
}

#[derive(Debug, Deserialize)]
struct Work {
    title: String,
    #[serde(default)]
    introduction: Option<String>,
    #[serde(rename = "tagLabels", default)]
    tags: Vec<String>,
    #[serde(default)]
    author: Option<Reference>,
    #[serde(rename = "tableOfContentsV2", default)]
    contents: Vec<Reference>,
}

/// A chapter of the table of contents: an arc name, and the episodes under it.
#[derive(Debug, Deserialize)]
struct TocChapter {
    /// Absent for a work with no arcs — the single unnamed group.
    #[serde(default)]
    chapter: Option<Reference>,
    #[serde(rename = "episodeUnions", default)]
    episodes: Vec<Reference>,
}

#[derive(Debug, Deserialize)]
struct Arc {
    title: String,
}

#[derive(Debug, Deserialize)]
struct Episode {
    id: String,
    title: String,
    #[serde(rename = "publishedAt", default)]
    published_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct User {
    #[serde(rename = "activityName", default)]
    activity_name: Option<String>,
}

/// Parse the snapshot of the work page at `base`.
pub(super) fn parse(apollo: &Apollo, work_id: &str, base: &Url) -> Result<WorkPage, SourceError> {
    let key = format!("Work:{work_id}");
    let work: Work = apollo.entry(&key).ok_or_else(|| SourceError::Parse {
        url: base.to_string(),
        message: if apollo.contains(&key) {
            "the work snapshot is not the shape this adapter expects; the site probably changed"
                .to_string()
        } else {
            format!("no work `{work_id}` in the page snapshot")
        },
    })?;

    Ok(WorkPage {
        author: author_of(apollo, &work),
        synopsis: synopsis_of(&work),
        tags: work.tags.clone(),
        chapters: chapters_of(apollo, &work, base)?,
        title: work.title.clone(),
    })
}

fn author_of(apollo: &Apollo, work: &Work) -> Option<String> {
    let user: User = apollo.follow(work.author.as_ref()?)?;
    user.activity_name
}

/// The introduction, with the same treatment a page's text block gets: each line
/// trimmed of ASCII padding and blank lines dropped from the ends.
///
/// The work also carries a one-line `catchphrase`, which the manifest has no
/// field for; see `docs/engineering/sources.md`.
fn synopsis_of(work: &Work) -> Option<String> {
    let text = lines_trimmed(work.introduction.as_deref()?);
    (!text.is_empty()).then_some(text)
}

fn lines_trimmed(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').map(str::trim_ascii).collect();
    // A line of nothing but U+3000 is indentation with no content, which is how
    // the site's own text blocks end.
    let blank = |line: &str| line.trim().is_empty();
    let start = lines.iter().position(|line| !blank(line)).unwrap_or(0);
    let end = lines
        .iter()
        .rposition(|line| !blank(line))
        .map_or(start, |last| last + 1);
    lines[start..end].join("\n")
}

/// The episode list, in reading order, numbered from one and grouped into arcs.
fn chapters_of(apollo: &Apollo, work: &Work, base: &Url) -> Result<Vec<ChapterMeta>, SourceError> {
    let mut chapters: Vec<ChapterMeta> = Vec::new();

    for reference in &work.contents {
        let toc: TocChapter = apollo.follow(reference).ok_or_else(|| SourceError::Parse {
            url: base.to_string(),
            message: "the snapshot lists a chapter it does not contain; the page was truncated"
                .to_string(),
        })?;
        let arc = toc
            .chapter
            .as_ref()
            .and_then(|reference| apollo.follow::<Arc>(reference))
            .map(|arc| arc.title);

        for reference in &toc.episodes {
            let episode: Episode = apollo.follow(reference).ok_or_else(|| SourceError::Parse {
                url: base.to_string(),
                message:
                    "the snapshot lists an episode it does not contain; the page was truncated"
                        .to_string(),
            })?;
            let index = chapters.len() as u32 + 1;
            chapters.push(ChapterMeta {
                index,
                title: episode.title,
                url: format!("{base}/episodes/{}", episode.id),
                arc: arc.clone(),
                published_at: episode.published_at,
                // Kakuyomu publishes no revision time, per chapter or at all:
                // a live work page (2026-10-17) carries `{id, title,
                // publishedAt}` for every episode and only a
                // `lastEpisodePublishedAt` on the work. `nvd update --check`
                // is what covers an edit there.
                revised_at: None,
                hash: None,
                file: format!("chapters/{}", chapter_file_name(index)),
            });
        }
    }

    if chapters.is_empty() {
        return Err(SourceError::Parse {
            url: base.to_string(),
            message: "the work lists no episodes; is it published?".to_string(),
        });
    }
    Ok(chapters)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{KAKUYOMU_WORK_ARCS, KAKUYOMU_WORK_FLAT};

    const ARCS: &str = "16816452221336015212";
    const FLAT: &str = "11054822663954717597";

    fn base(work_id: &str) -> Url {
        Url::parse(&format!("https://kakuyomu.jp/works/{work_id}")).expect("url")
    }

    fn parsed(html: &str, work_id: &str) -> WorkPage {
        let apollo = Apollo::from_html(html).expect("the fixture has a snapshot");
        parse(&apollo, work_id, &base(work_id)).expect("the fixture parses")
    }

    #[test]
    fn metadata_comes_from_the_snapshot() {
        let page = parsed(KAKUYOMU_WORK_ARCS, ARCS);

        assert_eq!(
            page.title,
            "ギャルゲーのヘイトを溜めるクズでゲスな親友役として転生してしまいました。そして主人公が無能すぎて役にたたない……。"
        );
        assert_eq!(page.author.as_deref(), Some("桜祭"));
        assert_eq!(
            page.tags,
            vec![
                "恋愛",
                "ラブコメ",
                "ゲーム",
                "主人公最強",
                "学園",
                "現代ファンタジー",
                "ハーレム",
                "死亡フラグ"
            ]
        );
        let synopsis = page.synopsis.expect("this work has an introduction");
        assert!(
            synopsis.starts_with("前世の記憶を思い出してしまい"),
            "the introduction is the synopsis, without leading blank lines: {synopsis:?}"
        );
        assert!(
            synopsis.contains("\n\n"),
            "paragraph breaks inside the introduction are kept: {synopsis:?}"
        );
        assert!(
            !synopsis.ends_with('\n'),
            "trailing blank lines are dropped: {synopsis:?}"
        );
    }

    #[test]
    fn episodes_are_numbered_in_reading_order_across_arcs() {
        let page = parsed(KAKUYOMU_WORK_ARCS, ARCS);

        assert_eq!(page.chapters.len(), 4, "two arcs of two episodes");
        let indices: Vec<u32> = page.chapters.iter().map(|c| c.index).collect();
        assert_eq!(indices, vec![1, 2, 3, 4], "numbering does not restart");

        assert_eq!(page.chapters[0].title, "１、『人に命令を下せる』能力の覚醒");
        assert_eq!(
            page.chapters[0].url,
            format!("https://kakuyomu.jp/works/{ARCS}/episodes/16816452221336047787")
        );
        assert_eq!(page.chapters[0].arc.as_deref(), Some("第１章　覚醒"));
        assert_eq!(
            page.chapters[0].published_at.as_deref(),
            Some("2021-07-03T06:46:00.000Z")
        );
        assert_eq!(page.chapters[0].file, "chapters/0001.json");
        assert_eq!(
            page.chapters[0].hash, None,
            "no hash before the first fetch"
        );

        assert_eq!(page.chapters[2].index, 3);
        assert_eq!(
            page.chapters[2].arc.as_deref(),
            Some("第２章　禁断の恋愛"),
            "the arc changes with the chapter it belongs to"
        );
    }

    #[test]
    fn a_work_without_arcs_gets_one_unnamed_group() {
        let page = parsed(KAKUYOMU_WORK_FLAT, FLAT);

        assert_eq!(page.chapters.len(), 2);
        assert_eq!(page.chapters[0].title, "第1話　勇者、魔王の軍門に下る");
        assert_eq!(
            page.chapters[0].arc, None,
            "the single group has no name, and an empty id is not a name"
        );
        assert_eq!(page.author.as_deref(), Some("ナガワ　ヒイロ"));
        assert_eq!(page.tags[0], "主人公最強");
    }

    #[test]
    fn the_introduction_is_tidied_but_not_rewritten() {
        // Leading and trailing blank lines go; the author's own indentation and
        // paragraph breaks stay.
        assert_eq!(
            lines_trimmed("\n　一行目\n\n　二行目\n\n　\n"),
            "　一行目\n\n　二行目"
        );
        assert_eq!(lines_trimmed(""), "");
        assert_eq!(lines_trimmed("\n\n"), "");
        assert_eq!(lines_trimmed("  一行  "), "一行");
    }

    #[test]
    fn a_page_that_is_not_this_work_is_reported() {
        let apollo = Apollo::from_html(KAKUYOMU_WORK_ARCS).expect("snapshot");
        let error = parse(&apollo, "999", &base("999")).unwrap_err();
        let message = error.to_string();

        assert!(
            message.contains("no work `999`") && message.contains("https://kakuyomu.jp/works/999"),
            "the message must name both the work and the page: {message}"
        );
    }

    #[test]
    fn a_truncated_snapshot_is_an_error_not_a_short_novel() {
        // A chapter that the snapshot lists but does not contain is the
        // signature of a half-delivered payload. Downloading the part that did
        // arrive would look like success.
        let html = r#"<!DOCTYPE html><html><body>
<script id="__NEXT_DATA__" type="application/json">{"props":{"pageProps":{"__APOLLO_STATE__":{
"Work:1":{"title":"t","tableOfContentsV2":[{"__ref":"TableOfContentsChapter:missing"}]}
}}},"page":"/x"}</script>
</body></html>"#;
        let apollo = Apollo::from_html(html).expect("snapshot");
        let error = parse(&apollo, "1", &base("1")).unwrap_err();

        assert!(error.to_string().contains("truncated"), "got: {error}");
    }

    #[test]
    fn an_episode_with_no_title_is_reported_rather_than_invented() {
        let html = r#"<!DOCTYPE html><html><body>
<script id="__NEXT_DATA__" type="application/json">{"props":{"pageProps":{"__APOLLO_STATE__":{
"Work:1":{"title":"t","tableOfContentsV2":[{"__ref":"TableOfContentsChapter:a"}]},
"TableOfContentsChapter:a":{"episodeUnions":[{"__ref":"Episode:9"}]},
"Episode:9":{"id":"9"}
}}},"page":"/x"}</script>
</body></html>"#;
        let apollo = Apollo::from_html(html).expect("snapshot");
        let error = parse(&apollo, "1", &base("1")).unwrap_err();

        assert!(error.to_string().contains("truncated"), "got: {error}");
    }

    #[test]
    fn a_work_with_no_episodes_is_reported() {
        let html = r#"<!DOCTYPE html><html><body>
<script id="__NEXT_DATA__" type="application/json">{"props":{"pageProps":{"__APOLLO_STATE__":{
"Work:1":{"title":"t","tableOfContentsV2":[]}
}}},"page":"/x"}</script>
</body></html>"#;
        let apollo = Apollo::from_html(html).expect("snapshot");
        let error = parse(&apollo, "1", &base("1")).unwrap_err();

        assert!(error.to_string().contains("no episodes"), "got: {error}");
    }

    #[test]
    fn a_missing_author_is_not_an_error() {
        let html = r#"<!DOCTYPE html><html><body>
<script id="__NEXT_DATA__" type="application/json">{"props":{"pageProps":{"__APOLLO_STATE__":{
"Work:1":{"title":"t","tableOfContentsV2":[{"__ref":"TableOfContentsChapter:a"}]},
"TableOfContentsChapter:a":{"episodeUnions":[{"__ref":"Episode:9"}]},
"Episode:9":{"id":"9","title":"一"}
}}},"page":"/x"}</script>
</body></html>"#;
        let apollo = Apollo::from_html(html).expect("snapshot");
        let page = parse(&apollo, "1", &base("1")).expect("a work with no author still parses");

        assert_eq!(page.author, None);
        assert_eq!(page.synopsis, None);
        assert_eq!(page.chapters.len(), 1);
        assert_eq!(
            page.chapters[0].url,
            "https://kakuyomu.jp/works/1/episodes/9"
        );
        assert_eq!(page.chapters[0].published_at, None);
    }
}
