//! A small Markdown → HTML renderer for exported chats (Phase 18).
//!
//! Covers what models write: headings, paragraphs, fenced code, block quotes, bulleted and
//! numbered lists (nested by indentation), GFM tables, rules, and inline code, bold, italic,
//! strikethrough, links and autolinks. Everything else is shown as text. All text is
//! HTML-escaped; links keep only `http`, `https` and `mailto` targets, and images become
//! links (an export loads nothing from the network).

/// Escapes text for HTML content and attribute values.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

fn safe_url(url: &str) -> Option<String> {
    let u = url.trim();
    let lower = u.to_ascii_lowercase();
    (lower.starts_with("https://") || lower.starts_with("http://") || lower.starts_with("mailto:"))
        .then(|| escape(u))
}

/// Renders inline Markdown (one paragraph's text).
pub fn inline(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len() + 16);
    render_inline(&chars, &mut out, 0);
    out
}

fn find_seq(chars: &[char], from: usize, seq: &[char]) -> Option<usize> {
    if seq.is_empty() || chars.len() < seq.len() {
        return None;
    }
    (from..=chars.len() - seq.len()).find(|&i| chars[i..i + seq.len()] == *seq)
}

fn render_inline(chars: &[char], out: &mut String, depth: u32) {
    let s: String = chars.iter().collect();
    if depth > 8 {
        out.push_str(&escape(&s));
        return;
    }
    let mut i = 0;
    let mut text = String::new();
    let flush = |text: &mut String, out: &mut String| {
        if !text.is_empty() {
            out.push_str(&escape(text));
            text.clear();
        }
    };
    while i < chars.len() {
        let c = chars[i];
        // Backslash escapes.
        if c == '\\' && i + 1 < chars.len() && chars[i + 1].is_ascii_punctuation() {
            text.push(chars[i + 1]);
            i += 2;
            continue;
        }
        // Inline code.
        if c == '`' {
            let mut ticks = 1;
            while i + ticks < chars.len() && chars[i + ticks] == '`' {
                ticks += 1;
            }
            let fence: Vec<char> = vec!['`'; ticks];
            if let Some(end) = find_seq(chars, i + ticks, &fence) {
                flush(&mut text, out);
                let code: String = chars[i + ticks..end].iter().collect();
                out.push_str("<code>");
                out.push_str(&escape(code.trim()));
                out.push_str("</code>");
                i = end + ticks;
                continue;
            }
        }
        // Autolinks <https://…>.
        if c == '<'
            && let Some(end) = chars[i + 1..].iter().position(|&x| x == '>' || x == ' ')
            && chars[i + 1 + end] == '>'
        {
            let url: String = chars[i + 1..i + 1 + end].iter().collect();
            if let Some(href) = safe_url(&url) {
                flush(&mut text, out);
                out.push_str(&format!(
                    "<a href=\"{href}\" rel=\"noopener noreferrer\">{}</a>",
                    escape(&url)
                ));
                i += end + 2;
                continue;
            }
        }
        // Links [text](url) and images ![alt](url).
        let image = c == '!' && chars.get(i + 1) == Some(&'[');
        if c == '[' || image {
            let open = if image { i + 1 } else { i };
            if let Some(close) = find_bracket(chars, open)
                && chars.get(close + 1) == Some(&'(')
                && let Some(paren) = matching_paren(&chars[close + 2..])
            {
                let label = &chars[open + 1..close];
                let target: String = chars[close + 2..close + 2 + paren].iter().collect();
                let url = target.split_whitespace().next().unwrap_or("");
                flush(&mut text, out);
                match safe_url(url) {
                    Some(href) => {
                        out.push_str(&format!("<a href=\"{href}\" rel=\"noopener noreferrer\">"));
                        if image {
                            out.push_str("🖼 ");
                        }
                        render_inline(label, out, depth + 1);
                        out.push_str("</a>");
                    }
                    None => render_inline(label, out, depth + 1),
                }
                i = close + 3 + paren;
                continue;
            }
        }
        // Bold, italic, strikethrough.
        let mut consumed = false;
        for (mark, tag) in [
            ("**", "strong"),
            ("__", "strong"),
            ("~~", "del"),
            ("*", "em"),
            ("_", "em"),
        ] {
            let m: Vec<char> = mark.chars().collect();
            if !chars[i..].starts_with(&m) {
                continue;
            }
            let start = i + m.len();
            // `_` inside words (snake_case) isn't emphasis.
            let in_word = mark.starts_with('_') && i > 0 && chars[i - 1].is_alphanumeric();
            if !in_word
                && start < chars.len()
                && !chars[start].is_whitespace()
                && let Some(end) = find_seq(chars, start + 1, &m)
                && !chars[end - 1].is_whitespace()
                && !(mark.starts_with('_')
                    && chars
                        .get(end + m.len())
                        .is_some_and(|x| x.is_alphanumeric()))
            {
                flush(&mut text, out);
                out.push_str(&format!("<{tag}>"));
                render_inline(&chars[start..end], out, depth + 1);
                out.push_str(&format!("</{tag}>"));
                i = end + m.len();
                consumed = true;
            }
            break;
        }
        if !consumed {
            // Plain text, including markers that didn't open anything.
            text.push(c);
            i += 1;
        }
    }
    flush(&mut text, out);
}

/// The `)` closing a link target (parentheses inside it may nest).
fn matching_paren(chars: &[char]) -> Option<usize> {
    let mut depth = 0;
    for (j, &c) in chars.iter().enumerate() {
        match c {
            '(' => depth += 1,
            ')' if depth == 0 => return Some(j),
            ')' => depth -= 1,
            ' ' | '\n' if depth > 0 => return None,
            _ => {}
        }
    }
    None
}

fn find_bracket(chars: &[char], open: usize) -> Option<usize> {
    let mut depth = 0;
    for (j, &c) in chars.iter().enumerate().skip(open) {
        match c {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(j);
                }
            }
            _ => {}
        }
    }
    None
}

fn is_rule(line: &str) -> bool {
    let t: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    t.len() >= 3
        && (t.chars().all(|c| c == '-')
            || t.chars().all(|c| c == '*')
            || t.chars().all(|c| c == '_'))
}

/// A list item's marker: (indent, ordered, content start).
fn list_marker(line: &str) -> Option<(usize, bool, usize)> {
    let indent = line.len() - line.trim_start().len();
    let rest = &line[indent..];
    for m in ["- ", "* ", "+ "] {
        if rest.starts_with(m) && !is_rule(line) {
            return Some((indent, false, indent + 2));
        }
    }
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    if (1..=9).contains(&digits) {
        let after = &rest[digits..];
        if after.starts_with(". ") || after.starts_with(") ") {
            return Some((indent, true, indent + digits + 2));
        }
    }
    None
}

fn table_cells(line: &str) -> Vec<String> {
    let t = line.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    t.split('|').map(|c| c.trim().to_owned()).collect()
}

fn is_table_separator(line: &str) -> bool {
    let cells = table_cells(line);
    line.contains('-')
        && !cells.is_empty()
        && cells.iter().all(|c| {
            let c = c.trim_matches(':');
            !c.is_empty() && c.chars().all(|x| x == '-')
        })
}

/// Renders a Markdown document to HTML blocks.
pub fn render(markdown: &str) -> String {
    let lines: Vec<&str> = markdown.lines().collect();
    let mut out = String::new();
    render_blocks(&lines, &mut out, 0);
    out
}

fn render_blocks(lines: &[&str], out: &mut String, depth: u32) {
    let mut i = 0;
    let mut para: Vec<&str> = Vec::new();
    let flush_para = |para: &mut Vec<&str>, out: &mut String| {
        if !para.is_empty() {
            let text = para.iter().map(|l| l.trim()).collect::<Vec<_>>().join("\n");
            out.push_str("<p>");
            out.push_str(&inline(&text));
            out.push_str("</p>\n");
            para.clear();
        }
    };
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            flush_para(&mut para, out);
            i += 1;
            continue;
        }
        // Fenced code.
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            flush_para(&mut para, out);
            let fence = &trimmed[..3];
            let lang: String = trimmed[3..]
                .trim()
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || "+-#._".contains(*c))
                .collect();
            let mut code: Vec<&str> = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim_start().starts_with(fence) {
                code.push(lines[i]);
                i += 1;
            }
            i += 1;
            if lang.is_empty() {
                out.push_str("<pre><code>");
            } else {
                out.push_str(&format!("<pre><code class=\"language-{}\">", escape(&lang)));
            }
            out.push_str(&escape(&code.join("\n")));
            out.push_str("</code></pre>\n");
            continue;
        }
        // Headings.
        let hashes = trimmed.chars().take_while(|&c| c == '#').count();
        if (1..=6).contains(&hashes) && trimmed[hashes..].starts_with(' ') {
            flush_para(&mut para, out);
            let text = trimmed[hashes..].trim().trim_end_matches('#').trim();
            out.push_str(&format!("<h{hashes}>{}</h{hashes}>\n", inline(text)));
            i += 1;
            continue;
        }
        if is_rule(trimmed) && para.is_empty() {
            out.push_str("<hr>\n");
            i += 1;
            continue;
        }
        // Block quotes.
        if trimmed.starts_with('>') {
            flush_para(&mut para, out);
            let mut inner: Vec<&str> = Vec::new();
            while i < lines.len() && lines[i].trim_start().starts_with('>') {
                let l = lines[i].trim_start()[1..]
                    .strip_prefix(' ')
                    .unwrap_or(&lines[i].trim_start()[1..]);
                inner.push(l);
                i += 1;
            }
            out.push_str("<blockquote>\n");
            if depth < 8 {
                render_blocks(&inner, out, depth + 1);
            }
            out.push_str("</blockquote>\n");
            continue;
        }
        // Tables.
        if line.contains('|') && i + 1 < lines.len() && is_table_separator(lines[i + 1]) {
            flush_para(&mut para, out);
            let head = table_cells(line);
            out.push_str("<table>\n<thead><tr>");
            for c in &head {
                out.push_str(&format!("<th>{}</th>", inline(c)));
            }
            out.push_str("</tr></thead>\n<tbody>\n");
            i += 2;
            while i < lines.len() && lines[i].contains('|') && !lines[i].trim().is_empty() {
                out.push_str("<tr>");
                for c in table_cells(lines[i]).iter().take(head.len().max(1)) {
                    out.push_str(&format!("<td>{}</td>", inline(c)));
                }
                out.push_str("</tr>\n");
                i += 1;
            }
            out.push_str("</tbody>\n</table>\n");
            continue;
        }
        // Lists.
        if let Some((indent, ordered, _)) = list_marker(line) {
            flush_para(&mut para, out);
            let tag = if ordered { "ol" } else { "ul" };
            out.push_str(&format!("<{tag}>\n"));
            while i < lines.len() {
                let Some((ind, ord, start)) = list_marker(lines[i]) else {
                    break;
                };
                if ind != indent || ord != ordered {
                    break;
                }
                let mut item: Vec<&str> = vec![&lines[i][start..]];
                i += 1;
                // The item's continuation: deeper-indented lines (nested lists, code) and
                // lazy text lines, until a blank line followed by a non-indented line.
                while i < lines.len() {
                    let l = lines[i];
                    let lind = l.len() - l.trim_start().len();
                    if l.trim().is_empty() {
                        let next_deeper = lines.get(i + 1).is_some_and(|n| {
                            n.len() - n.trim_start().len() > indent && !n.trim().is_empty()
                        });
                        if next_deeper {
                            item.push("");
                            i += 1;
                            continue;
                        }
                        break;
                    }
                    if lind > indent {
                        item.push(
                            l.get(indent.min(lind) + 2.min(lind - indent)..)
                                .unwrap_or(l.trim_start()),
                        );
                        i += 1;
                    } else if list_marker(l).is_none() && !l.trim_start().starts_with('#') {
                        item.push(l.trim_start());
                        i += 1;
                    } else {
                        break;
                    }
                }
                out.push_str("<li>");
                let simple = item.len() == 1;
                if simple {
                    out.push_str(&inline(item[0].trim()));
                } else if depth < 8 {
                    let mut inner = String::new();
                    render_blocks(&item, &mut inner, depth + 1);
                    // A single paragraph renders without <p>, like a tight list.
                    let tight = inner.matches("<p>").count() == 1
                        && inner.starts_with("<p>")
                        && !inner.contains("\n\n");
                    out.push_str(&if tight {
                        inner.replacen("<p>", "", 1).replacen("</p>\n", "", 1)
                    } else {
                        inner
                    });
                }
                out.push_str("</li>\n");
                // Skip blank lines between items of the same list.
                while i < lines.len()
                    && lines[i].trim().is_empty()
                    && lines
                        .get(i + 1)
                        .and_then(|n| list_marker(n))
                        .is_some_and(|(ind, ord, _)| ind == indent && ord == ordered)
                {
                    i += 1;
                }
            }
            out.push_str(&format!("</{tag}>\n"));
            continue;
        }
        para.push(line);
        i += 1;
    }
    flush_para(&mut para, out);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_marks_and_escaping() {
        assert_eq!(inline("a <b> & \"c\""), "a &lt;b&gt; &amp; &quot;c&quot;");
        assert_eq!(
            inline("**bold** and *it* and `x<y`"),
            "<strong>bold</strong> and <em>it</em> and <code>x&lt;y</code>"
        );
        assert_eq!(inline("snake_case_name stays"), "snake_case_name stays");
        assert_eq!(inline("~~gone~~"), "<del>gone</del>");
        assert_eq!(inline("2 * 3 * 4"), "2 * 3 * 4");
        assert_eq!(
            inline("[site](https://example.com/a?b=1&c=2) <https://x.dk>"),
            "<a href=\"https://example.com/a?b=1&amp;c=2\" rel=\"noopener noreferrer\">site</a> <a href=\"https://x.dk\" rel=\"noopener noreferrer\">https://x.dk</a>"
        );
        // Unsafe targets keep only their text.
        assert_eq!(inline("[click](javascript:alert(1))"), "click");
        assert!(!inline("![x](https://e.com/i.png)").contains("<img"));
        assert_eq!(inline("\\*not em\\*"), "*not em*");
    }

    #[test]
    fn blocks_render() {
        let md = "# Title\n\nSome *text*\nover lines.\n\n```rust\nfn main() { println!(\"<hi>\"); }\n```\n\n> quoted\n> more\n\n- one\n- two\n  - nested\n- three\n\n1. first\n2. second\n\n| A | B |\n|---|:-:|\n| 1 | **2** |\n\n---\nend";
        let html = render(md);
        assert!(html.contains("<h1>Title</h1>"), "{html}");
        assert!(
            html.contains("<p>Some <em>text</em>\nover lines.</p>"),
            "{html}"
        );
        assert!(html.contains("<pre><code class=\"language-rust\">fn main() { println!(&quot;&lt;hi&gt;&quot;); }</code></pre>"), "{html}");
        assert!(
            html.contains("<blockquote>\n<p>quoted\nmore</p>\n</blockquote>"),
            "{html}"
        );
        assert!(html.contains("<ul>\n<li>one</li>\n<li>two<ul>\n<li>nested</li>\n</ul>\n</li>\n<li>three</li>\n</ul>"), "{html}");
        assert!(
            html.contains("<ol>\n<li>first</li>\n<li>second</li>\n</ol>"),
            "{html}"
        );
        assert!(
            html.contains("<thead><tr><th>A</th><th>B</th></tr></thead>"),
            "{html}"
        );
        assert!(html.contains("<td><strong>2</strong></td>"), "{html}");
        assert!(html.contains("<hr>"), "{html}");
        assert!(html.ends_with("<p>end</p>\n"), "{html}");
    }

    #[test]
    fn hostile_input_stays_text() {
        let html = render(
            "<script>alert(1)</script>\n\n<img src=x onerror=alert(1)>\n\n```\n</code></pre><script>\n```",
        );
        assert!(!html.contains("<script>"), "{html}");
        assert!(!html.contains("<img"), "{html}");
        // Deep nesting and odd input don't panic.
        let _ = render(&"> ".repeat(200));
        let _ = render(&"- ".repeat(200));
        let _ = inline(&"[".repeat(500));
        let _ = inline(&"**".repeat(500));
        let _ = render("|\n|-|\n|");
        let _ = render("  1. x\n     - y\n\n\n  2. z");
    }
}
