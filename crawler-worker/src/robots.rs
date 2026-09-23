use url::Url;

#[derive(Debug, Clone)]
struct Rule {
    allow: bool,
    pattern: String,
}

#[derive(Debug, Default)]
pub struct RobotsRules {
    rules: Vec<Rule>,
    pub delay_ms: i64,
    fail_closed: bool,
}

#[derive(Debug, Default)]
struct Group {
    agents: Vec<String>,
    rules: Vec<Rule>,
    delay_ms: i64,
}

pub fn parse(input: &str, product_token: &str) -> RobotsRules {
    let Some(groups) = parse_groups(input) else {
        return default_rules(true);
    };
    let token = product_token.to_ascii_lowercase();
    let Some(rank) = matching_agent_rank(&groups, &token) else {
        return default_rules(false);
    };
    let selected = groups
        .into_iter()
        .filter(|group| group_matches_agent(group, &token, rank));
    let mut result = default_rules(false);
    for group in selected {
        result.delay_ms = result.delay_ms.max(group.delay_ms);
        result.rules.extend(group.rules);
    }
    result
}

fn default_rules(fail_closed: bool) -> RobotsRules {
    RobotsRules {
        rules: Vec::new(),
        delay_ms: 1_000,
        fail_closed,
    }
}

fn parse_groups(input: &str) -> Option<Vec<Group>> {
    if input.lines().count() > 10_000 {
        return None;
    }
    let mut groups = Vec::new();
    let mut group = Group::default();
    for raw_line in input.lines() {
        if let Some((name, value)) = parse_directive(raw_line) {
            add_directive(&mut group, &mut groups, &name, &value);
        }
    }
    if !group.agents.is_empty() {
        groups.push(group);
    }
    Some(groups)
}

fn parse_directive(raw_line: &str) -> Option<(String, String)> {
    let line = raw_line.split('#').next()?.trim();
    let (name, value) = line.split_once(':')?;
    Some((name.trim().to_ascii_lowercase(), value.trim().to_owned()))
}

fn add_directive(group: &mut Group, groups: &mut Vec<Group>, name: &str, value: &str) {
    match name {
        "user-agent" => {
            if !group.agents.is_empty() && (!group.rules.is_empty() || group.delay_ms > 0) {
                groups.push(std::mem::take(group));
            }
            if !value.is_empty() {
                group.agents.push(value.to_ascii_lowercase());
            }
        }
        "allow" | "disallow" if !group.agents.is_empty() && !value.is_empty() => {
            group.rules.push(Rule {
                allow: name == "allow",
                pattern: value.to_owned(),
            });
        }
        "crawl-delay" if !group.agents.is_empty() => {
            if let Some(delay_ms) = parse_crawl_delay(value) {
                group.delay_ms = delay_ms;
            }
        }
        _ => {}
    }
}

fn parse_crawl_delay(value: &str) -> Option<i64> {
    let seconds = value.parse::<f64>().ok()?;
    (seconds.is_finite() && seconds >= 0.0).then(|| (seconds * 1_000.0).clamp(0.0, 60_000.0) as i64)
}

fn matching_agent_rank(groups: &[Group], token: &str) -> Option<usize> {
    groups
        .iter()
        .flat_map(|group| group.agents.iter())
        .filter_map(|agent| matching_agent_length(agent, token))
        .max()
}

fn matching_agent_length(agent: &str, token: &str) -> Option<usize> {
    if agent == "*" {
        Some(0)
    } else if token.contains(agent) {
        Some(agent.len())
    } else {
        None
    }
}

fn group_matches_agent(group: &Group, token: &str, rank: usize) -> bool {
    group.agents.iter().any(|agent| {
        if agent == "*" {
            rank == 0
        } else {
            token.contains(agent) && agent.len() == rank
        }
    })
}

impl RobotsRules {
    pub fn allows(&self, url: &Url) -> bool {
        if self.fail_closed {
            return false;
        }
        let mut path = url.path().to_owned();
        if let Some(query) = url.query() {
            path.push('?');
            path.push_str(query);
        }
        let best = self
            .rules
            .iter()
            .filter(|rule| matches_pattern(&rule.pattern, &path))
            .max_by(|left, right| {
                left.pattern
                    .trim_end_matches('$')
                    .len()
                    .cmp(&right.pattern.trim_end_matches('$').len())
                    .then_with(|| left.allow.cmp(&right.allow))
            });
        best.is_none_or(|rule| rule.allow)
    }
}

fn matches_pattern(pattern: &str, path: &str) -> bool {
    let anchored = pattern.ends_with('$');
    let expression = pattern.strip_suffix('$').unwrap_or(pattern);
    if !expression.contains('*') {
        return if anchored {
            path == expression
        } else {
            path.starts_with(expression)
        };
    }
    let mut remaining = path;
    let mut segments = expression.split('*').peekable();
    if let Some(first) = segments.next()
        && !first.is_empty()
    {
        if !remaining.starts_with(first) {
            return false;
        }
        remaining = &remaining[first.len()..];
    }
    for segment in segments {
        if segment.is_empty() {
            continue;
        }
        let Some(index) = remaining.find(segment) else {
            return false;
        };
        remaining = &remaining[index + segment.len()..];
    }
    !anchored || remaining.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_most_specific_group_and_prefers_allow_on_tie() {
        let rules = parse(
            "User-agent: *\nDisallow: /\n\nUser-agent: WebCrawler\nDisallow: /private\nAllow: /private/public\n",
            "WebCrawler",
        );
        assert!(!rules.allows(&Url::parse("https://example.org/private/file").unwrap()));
        assert!(rules.allows(&Url::parse("https://example.org/private/public/file").unwrap()));
        assert!(rules.allows(&Url::parse("https://example.org/elsewhere").unwrap()));
        let fallback = parse("User-agent: *\nDisallow: /private", "OtherCrawler");
        assert!(!fallback.allows(&Url::parse("https://example.org/private").unwrap()));
    }

    #[test]
    fn supports_wildcards_end_anchors_queries_and_comments() {
        let rules = parse(
            "User-agent: * # everyone\nDisallow: /*.pdf$\nDisallow: /search?private=1\n",
            "WebCrawler",
        );
        assert!(!rules.allows(&Url::parse("https://example.org/a.pdf").unwrap()));
        assert!(rules.allows(&Url::parse("https://example.org/a.pdf?download=1").unwrap()));
        assert!(!rules.allows(&Url::parse("https://example.org/search?private=1").unwrap()));
    }

    #[test]
    fn parses_non_standard_crawl_delay_but_honors_one_second_floor() {
        let rules = parse("User-agent: *\nCrawl-delay: 2.5\n", "WebCrawler");
        assert_eq!(rules.delay_ms, 2_500);
    }

    #[test]
    fn fails_closed_when_policy_has_too_many_lines() {
        let input = "Disallow: /private\n".repeat(10_001);
        let rules = parse(&input, "WebCrawler");
        assert!(!rules.allows(&Url::parse("https://example.org/").unwrap()));
    }
}
