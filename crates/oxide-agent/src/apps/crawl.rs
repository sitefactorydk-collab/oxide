//! Site crawling for Utility Apps (`oxide.web.crawl`): pages of one site, breadth first,
//! respecting the site's `robots.txt`.

use std::collections::{HashSet, VecDeque};

use serde::Serialize;
use url::Url;

use crate::tools::WebTools;
use crate::tools::html::html_to_markdown;

/// Most pages one crawl reads.
pub const MAX_CRAWL_PAGES: usize = 50;
/// The user-agent token Oxide's crawler answers to in `robots.txt`.
pub const ROBOTS_AGENT: &str = "oxide";

/// One `Allow` or `Disallow` line.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Rule {
    allow: bool,
    pattern: String,
}

/// The rules of `robots.txt` that apply to Oxide.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Robots {
    rules: Vec<Rule>,
}

impl Robots {
    /// Parses `robots.txt`. The group naming Oxide wins over `*`; without either, all is
    /// allowed.
    pub fn parse(text: &str) -> Self {
        let mut groups: Vec<(Vec<String>, Vec<Rule>)> = Vec::new();
        let mut in_agents = false;
        for raw in text.lines() {
            let line = raw.split('#').next().unwrap_or("").trim();
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let key = key.trim().to_ascii_lowercase();
            let value = value.trim();
            match key.as_str() {
                "user-agent" => {
                    if !in_agents || groups.is_empty() {
                        groups.push((Vec::new(), Vec::new()));
                    }
                    if let Some(g) = groups.last_mut() {
                        g.0.push(value.to_ascii_lowercase());
                    }
                    in_agents = true;
                }
                "allow" | "disallow" => {
                    in_agents = false;
                    if let Some(g) = groups.last_mut() {
                        // An empty `Disallow:` allows everything; it adds no rule.
                        if !value.is_empty() {
                            g.1.push(Rule {
                                allow: key == "allow",
                                pattern: value.to_owned(),
                            });
                        }
                    }
                }
                _ => in_agents = false,
            }
        }
        let pick = |want: &dyn Fn(&str) -> bool| {
            groups
                .iter()
                .filter(|(agents, _)| agents.iter().any(|a| want(a)))
                .flat_map(|(_, rules)| rules.clone())
                .collect::<Vec<_>>()
        };
        let own = pick(&|a: &str| a.contains(ROBOTS_AGENT));
        let has_own = groups
            .iter()
            .any(|(agents, _)| agents.iter().any(|a| a.contains(ROBOTS_AGENT)));
        let rules = if has_own {
            own
        } else {
            pick(&|a: &str| a == "*")
        };
        Self { rules }
    }

    /// Whether a path (with its query) may be read. The longest matching rule wins; `Allow`
    /// wins a tie.
    pub fn allows(&self, path: &str) -> bool {
        let mut best: Option<(usize, bool)> = None;
        for r in &self.rules {
            if matches(&r.pattern, path) {
                let len = r.pattern.len();
                match best {
                    Some((l, a)) if l > len || (l == len && a) => {}
                    _ => best = Some((len, r.allow)),
                }
            }
        }
        best.is_none_or(|(_, allow)| allow)
    }
}

/// `robots.txt` pattern matching: a prefix match with `*` for any run and `$` for the end.
fn matches(pattern: &str, path: &str) -> bool {
    let (pattern, anchored) = match pattern.strip_suffix('$') {
        Some(p) => (p, true),
        None => (pattern, false),
    };
    let parts: Vec<&str> = pattern.split('*').collect();
    let mut at = 0;
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            if !path.starts_with(part) {
                return false;
            }
            at = part.len();
            continue;
        }
        if part.is_empty() {
            continue;
        }
        match path[at..].find(part) {
            Some(pos) => at += pos + part.len(),
            None => return false,
        }
    }
    if anchored {
        // The last literal part must end the path (or the pattern ends with `*`).
        let last = parts.last().copied().unwrap_or("");
        return last.is_empty() || path.ends_with(last) && at == path.len();
    }
    true
}

/// Links (`href` of `<a>` and `<link rel=…>` are not followed; only anchors) in a page,
/// made absolute, without fragments.
pub fn links(html: &str, base: &Url) -> Vec<Url> {
    let lower = html.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(rel) = lower[at..].find("<a") {
        let start = at + rel;
        let Some(end_rel) = lower[start..].find('>') else {
            break;
        };
        let end = start + end_rel;
        let tag = &html[start..end];
        let tag_lower = &lower[start..end];
        at = end;
        // `<abbr>`, `<area>`, … aren't anchors.
        if !tag_lower[2..].starts_with(|c: char| c.is_whitespace()) {
            continue;
        }
        let Some(h) = tag_lower.find("href") else {
            continue;
        };
        let rest = tag[h + 4..].trim_start();
        let Some(rest) = rest.strip_prefix('=') else {
            continue;
        };
        let rest = rest.trim_start();
        let value = match rest.chars().next() {
            Some(q @ ('"' | '\'')) => rest[1..].split(q).next().unwrap_or(""),
            _ => rest
                .split(|c: char| c.is_whitespace() || c == '>')
                .next()
                .unwrap_or(""),
        };
        let value = crate::tools::html::decode_entities(value.trim());
        if value.is_empty()
            || value.starts_with('#')
            || value.starts_with("mailto:")
            || value.starts_with("javascript:")
            || value.starts_with("tel:")
        {
            continue;
        }
        if let Ok(mut u) = base.join(&value) {
            u.set_fragment(None);
            if matches!(u.scheme(), "http" | "https") {
                out.push(u);
            }
        }
    }
    out
}

/// What a crawl found.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CrawlResult {
    pub pages: Vec<CrawledPage>,
    pub skipped: Vec<SkippedPage>,
    /// `robots.txt` was found and followed.
    pub robots: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CrawledPage {
    pub url: String,
    pub title: Option<String>,
    /// Markdown, or the page's HTML when asked for.
    pub content: String,
    pub depth: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SkippedPage {
    pub url: String,
    pub reason: String,
}

/// What to crawl.
#[derive(Debug, Clone)]
pub struct CrawlRequest {
    pub start: String,
    pub max_pages: usize,
    pub max_depth: u32,
    /// Keep the pages' HTML instead of Markdown.
    pub html: bool,
}

/// Files the crawler doesn't open as pages.
fn looks_like_file(u: &Url) -> bool {
    let path = u.path().to_ascii_lowercase();
    [
        ".png", ".jpg", ".jpeg", ".gif", ".webp", ".svg", ".pdf", ".zip", ".mp4", ".mp3", ".css",
        ".js", ".ico", ".woff", ".woff2", ".xml", ".json",
    ]
    .iter()
    .any(|ext| path.ends_with(ext))
}

/// Crawls pages of the start URL's site (same scheme, host and port), breadth first.
/// `cancelled` is checked between pages.
pub async fn crawl(
    web: &WebTools,
    req: &CrawlRequest,
    cancelled: &(dyn Fn() -> bool + Send + Sync),
) -> Result<CrawlResult, String> {
    let start =
        Url::parse(req.start.trim()).map_err(|_| format!("“{}” isn't a valid URL", req.start))?;
    if !matches!(start.scheme(), "http" | "https") {
        return Err("only http and https sites can be crawled".into());
    }
    let max_pages = req.max_pages.clamp(1, MAX_CRAWL_PAGES);
    let origin = start.origin();
    let robots_url = start
        .join("/robots.txt")
        .map_err(|_| "the site's robots.txt address couldn't be made".to_owned())?;
    let (robots, found) = match web.fetch_raw(robots_url.as_str()).await {
        Ok(p) if !p.content_type.contains("html") => (Robots::parse(&p.text), true),
        _ => (Robots::default(), false),
    };
    let mut result = CrawlResult {
        pages: Vec::new(),
        skipped: Vec::new(),
        robots: found,
    };
    let mut seen: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<(Url, u32)> = VecDeque::new();
    seen.insert(start.to_string());
    queue.push_back((start, 0));
    while let Some((url, depth)) = queue.pop_front() {
        if result.pages.len() >= max_pages || cancelled() {
            break;
        }
        let path_q = match url.query() {
            Some(q) => format!("{}?{q}", url.path()),
            None => url.path().to_owned(),
        };
        if !robots.allows(&path_q) {
            result.skipped.push(SkippedPage {
                url: url.to_string(),
                reason: "robots.txt disallows it".into(),
            });
            continue;
        }
        let page = match web.fetch_raw(url.as_str()).await {
            Ok(p) => p,
            Err(e) => {
                result.skipped.push(SkippedPage {
                    url: url.to_string(),
                    reason: e,
                });
                continue;
            }
        };
        let final_url = Url::parse(&page.url).unwrap_or(url.clone());
        if final_url.origin() != origin {
            result.skipped.push(SkippedPage {
                url: url.to_string(),
                reason: format!("it redirects to another site ({final_url})"),
            });
            continue;
        }
        let is_html = page.content_type.contains("html");
        let readable = is_html.then(|| html_to_markdown(&page.text, Some(&final_url)));
        if is_html && depth < req.max_depth {
            for link in links(&page.text, &final_url) {
                if link.origin() != origin || looks_like_file(&link) {
                    continue;
                }
                if seen.insert(link.to_string()) {
                    queue.push_back((link, depth + 1));
                }
            }
        }
        let (title, content) = match readable {
            Some(r) if !req.html => (r.title, r.markdown),
            Some(r) => (r.title, page.text),
            None => (None, page.text),
        };
        result.pages.push(CrawledPage {
            url: final_url.to_string(),
            title,
            content,
            depth,
        });
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn robots_rules_follow_the_longest_match() {
        let r = Robots::parse(
            "User-agent: *\nDisallow: /private/\nAllow: /private/open\nDisallow: /*.pdf$\n\
             \nUser-agent: Googlebot\nDisallow: /\n",
        );
        assert!(r.allows("/"));
        assert!(r.allows("/blog/post"));
        assert!(!r.allows("/private/x"));
        assert!(r.allows("/private/open/page"));
        assert!(!r.allows("/files/a.pdf"));
        assert!(r.allows("/files/a.pdf?x=1"));
    }

    #[test]
    fn a_group_for_oxide_wins_over_the_star_group() {
        let r = Robots::parse("User-agent: *\nDisallow: /\n\nUser-agent: Oxide\nAllow: /\n");
        assert!(r.allows("/anything"));
        let r = Robots::parse("User-agent: *\nDisallow: /\n");
        assert!(!r.allows("/anything"));
        let r = Robots::parse("User-agent: a\nUser-agent: oxide\nDisallow: /x\n");
        assert!(!r.allows("/x/1"));
        assert!(r.allows("/y"));
        assert!(Robots::parse("").allows("/z"));
        assert!(Robots::parse("User-agent: *\nDisallow:\n").allows("/z"));
    }

    #[test]
    fn links_are_absolute_and_skip_non_pages() {
        let base = Url::parse("https://example.com/docs/").expect("url");
        let html = r##"<a href="a.html">A</a> <A HREF='/b'>B</A> <a class=x href=c#top>C</a>
            <a href="#here">h</a> <a href="mailto:x@y">m</a> <abbr href="nope">
            <a href="https://other.org/z">Z</a> <a href="/q?x=1&amp;y=2">Q</a>"##;
        let got: Vec<String> = links(html, &base)
            .into_iter()
            .map(|u| u.to_string())
            .collect();
        assert_eq!(
            got,
            vec![
                "https://example.com/docs/a.html",
                "https://example.com/b",
                "https://example.com/docs/c",
                "https://other.org/z",
                "https://example.com/q?x=1&y=2",
            ]
        );
    }
}
