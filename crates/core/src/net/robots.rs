//! A small, dependency-free `robots.txt` parser.
//!
//! We only need the part that matters for politeness: which paths a given
//! crawler may fetch. The parser follows the widely implemented precedence
//! rules — the most specific matching group wins, the longest matching rule
//! inside it wins, and `Allow` breaks a tie — rather than the letter of the
//! original 1994 draft, which almost no crawler implements.

use std::time::Duration;

#[derive(Debug, Clone, Default)]
struct Group {
    /// Lower-cased `User-agent` tokens that select this group.
    agents: Vec<String>,
    rules: Vec<Rule>,
    crawl_delay: Option<Duration>,
}

#[derive(Debug, Clone)]
struct Rule {
    allow: bool,
    path: String,
}

/// The rules of one `robots.txt`, ready to be queried.
#[derive(Debug, Clone, Default)]
pub struct RobotsRules {
    groups: Vec<Group>,
}

impl RobotsRules {
    /// Rules that permit everything, used when a site publishes no `robots.txt`.
    pub fn allow_all() -> Self {
        Self::default()
    }

    /// Parse the text of a `robots.txt` file.
    ///
    /// Unparseable lines are skipped rather than fatal: a site's formatting
    /// quirk must not stop us from being able to read its intentions.
    pub fn parse(text: &str) -> Self {
        let mut groups: Vec<Group> = Vec::new();
        let mut current: Option<Group> = None;
        let mut previous_was_agent = false;

        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let Some((field, value)) = line.split_once(':') else {
                continue;
            };
            let field = field.trim().to_ascii_lowercase();
            let value = value.trim();

            match field.as_str() {
                "user-agent" => {
                    // Consecutive `User-agent` lines share the rules that follow.
                    if !previous_was_agent {
                        if let Some(group) = current.take() {
                            groups.push(group);
                        }
                        current = Some(Group::default());
                    }
                    if let Some(group) = current.as_mut() {
                        group.agents.push(value.to_ascii_lowercase());
                    }
                    previous_was_agent = true;
                }
                "allow" | "disallow" => {
                    previous_was_agent = false;
                    let Some(group) = current.as_mut() else {
                        // Rules before any `User-agent` line apply to nobody.
                        continue;
                    };
                    // `Disallow:` with an empty value means "allow everything".
                    if field == "disallow" && value.is_empty() {
                        continue;
                    }
                    group.rules.push(Rule {
                        allow: field == "allow",
                        path: value.to_string(),
                    });
                }
                "crawl-delay" => {
                    previous_was_agent = false;
                    if let Some(group) = current.as_mut() {
                        if let Ok(seconds) = value.parse::<f64>() {
                            if seconds > 0.0 {
                                // Capped so a typo cannot turn into an hour of silence.
                                group.crawl_delay =
                                    Some(Duration::from_secs_f64(seconds.min(3600.0)));
                            }
                        }
                    }
                }
                _ => previous_was_agent = false,
            }
        }

        if let Some(group) = current {
            groups.push(group);
        }
        Self { groups }
    }

    /// Whether `user_agent` may fetch `path`.
    ///
    /// A path no rule mentions is allowed, which is the convention.
    pub fn is_allowed(&self, user_agent: &str, path: &str) -> bool {
        let Some(group) = self.group_for(user_agent) else {
            return true;
        };

        let mut best: Option<(usize, bool)> = None;
        for rule in &group.rules {
            let Some(specificity) = pattern_match(&rule.path, path) else {
                continue;
            };
            best = Some(match best {
                None => (specificity, rule.allow),
                Some((length, _)) if specificity > length => (specificity, rule.allow),
                Some((length, allow)) if specificity == length => (length, allow || rule.allow),
                Some(previous) => previous,
            });
        }
        best.map(|(_, allow)| allow).unwrap_or(true)
    }

    /// The `Crawl-delay` the matching group asks for, if any.
    pub fn crawl_delay(&self, user_agent: &str) -> Option<Duration> {
        self.group_for(user_agent)
            .and_then(|group| group.crawl_delay)
    }

    /// The group that most specifically addresses `user_agent`.
    fn group_for(&self, user_agent: &str) -> Option<&Group> {
        let user_agent = user_agent.to_ascii_lowercase();
        let mut best: Option<(usize, &Group)> = None;

        for group in &self.groups {
            for agent in &group.agents {
                // `*` is the fallback group and scores zero, so any named token
                // in our UA string wins over it. Substring matching is
                // deliberately lenient: our UA leads with the product token.
                let score = if agent == "*" {
                    0
                } else if user_agent.contains(agent.as_str()) {
                    agent.len()
                } else {
                    continue;
                };
                match best {
                    Some((previous, _)) if previous >= score => {}
                    _ => best = Some((score, group)),
                }
            }
        }
        best.map(|(_, group)| group)
    }
}

/// If `pattern` matches `path`, its specificity: the number of literal
/// characters, which is how "longest match wins" is decided.
///
/// Supports the `*` wildcard and a trailing `$` end anchor. A pattern with
/// neither matches as a prefix, which is what the robots convention specifies.
fn pattern_match(pattern: &str, path: &str) -> Option<usize> {
    if pattern.is_empty() {
        return None;
    }
    let anchored = pattern.ends_with('$');
    let pattern = pattern.strip_suffix('$').unwrap_or(pattern);
    let specificity = pattern.chars().filter(|c| *c != '*').count();

    if !pattern.contains('*') {
        let matched = if anchored {
            path == pattern
        } else {
            path.starts_with(pattern)
        };
        return matched.then_some(specificity);
    }

    let parts: Vec<&str> = pattern.split('*').collect();
    let mut rest = path;
    for (position, part) in parts.iter().enumerate() {
        let is_first = position == 0;
        let is_last = position == parts.len() - 1;

        if part.is_empty() {
            if is_last && anchored && !rest.is_empty() {
                return None;
            }
            continue;
        }
        if is_first && !rest.starts_with(part) {
            return None;
        }
        if is_last && anchored {
            return rest.ends_with(part).then_some(specificity);
        }
        if is_first {
            rest = &rest[part.len()..];
            continue;
        }
        rest = &rest[rest.find(part)? + part.len()..];
    }

    if anchored && !rest.is_empty() {
        return None;
    }
    Some(specificity)
}

#[cfg(test)]
mod tests {
    use super::*;

    const UA: &str = "web-novel-downloader/0.1.0 (+https://example.com)";

    #[test]
    fn a_wildcard_group_with_a_disallow_blocks_that_path() {
        let rules = RobotsRules::parse("User-agent: *\nDisallow: /private/\nAllow: /\n");
        assert!(!rules.is_allowed(UA, "/private/x"));
        assert!(rules.is_allowed(UA, "/public/x"));
    }

    #[test]
    fn the_longest_matching_rule_wins() {
        let rules = RobotsRules::parse("User-agent: *\nDisallow: /a/\nAllow: /a/b/\n");
        assert!(!rules.is_allowed(UA, "/a/x"));
        assert!(rules.is_allowed(UA, "/a/b/c"));
    }

    #[test]
    fn allow_breaks_a_tie() {
        let rules = RobotsRules::parse("User-agent: *\nDisallow: /x\nAllow: /x\n");
        assert!(rules.is_allowed(UA, "/x"));
    }

    #[test]
    fn wildcards_and_the_end_anchor_are_supported() {
        let rules = RobotsRules::parse("User-agent: *\nDisallow: /*.json$\nDisallow: /tmp*\n");
        assert!(!rules.is_allowed(UA, "/data/a.json"));
        assert!(rules.is_allowed(UA, "/data/a.json.bak"));
        assert!(!rules.is_allowed(UA, "/tmpfile"));
        assert!(rules.is_allowed(UA, "/data/a.txt"));
    }

    #[test]
    fn a_named_group_beats_the_wildcard_group() {
        let rules = RobotsRules::parse(
            "User-agent: *\nDisallow: /\n\nUser-agent: web-novel-downloader\nDisallow: /nope/\n",
        );
        assert!(rules.is_allowed(UA, "/anything"));
        assert!(!rules.is_allowed(UA, "/nope/x"));
        assert!(
            !rules.is_allowed("some-other-bot/1.0", "/anything"),
            "an unnamed crawler falls back to the wildcard group"
        );
    }

    #[test]
    fn consecutive_user_agent_lines_share_one_group() {
        let rules = RobotsRules::parse("User-agent: alpha\nUser-agent: beta\nDisallow: /secret\n");
        assert!(!rules.is_allowed("alpha/1", "/secret/1"));
        assert!(!rules.is_allowed("beta/1", "/secret/1"));
        assert!(rules.is_allowed("gamma/1", "/secret/1"));
    }

    #[test]
    fn comments_blank_lines_and_unknown_fields_are_ignored() {
        let rules = RobotsRules::parse(
            "# a comment\n\nSitemap: https://example.com/sitemap.xml\nUser-agent: *\nDisallow: /only   # trailing note\nRequest-rate: 1/5\n",
        );
        assert!(!rules.is_allowed(UA, "/only"));
        assert!(rules.is_allowed(UA, "/other"));
    }

    #[test]
    fn an_empty_disallow_allows_everything() {
        let rules = RobotsRules::parse("User-agent: *\nDisallow:\n");
        assert!(rules.is_allowed(UA, "/anything"));
    }

    #[test]
    fn crawl_delay_is_parsed_for_the_matching_group() {
        let rules = RobotsRules::parse("User-agent: *\nCrawl-delay: 2.5\n");
        assert_eq!(rules.crawl_delay(UA), Some(Duration::from_millis(2500)));
        assert_eq!(
            rules.crawl_delay("nobody/1"),
            Some(Duration::from_millis(2500))
        );
    }

    #[test]
    fn missing_robots_allows_everything() {
        let rules = RobotsRules::allow_all();
        assert!(rules.is_allowed(UA, "/anything"));
        assert_eq!(rules.crawl_delay(UA), None);
    }

    #[test]
    fn rules_before_any_user_agent_are_ignored() {
        let rules = RobotsRules::parse("Disallow: /\nUser-agent: *\nAllow: /\n");
        assert!(rules.is_allowed(UA, "/anything"));
    }
}
