//! Readable Markdown from an HTML page, for `web_fetch`.
//!
//! A small, forgiving HTML parser builds a tree; the main content (the largest `<article>`,
//! else `<main>`, else `<body>`) is rendered as Markdown without navigation, scripts, forms
//! and other page furniture. It isn't a full readability algorithm, but it keeps headings,
//! paragraphs, lists, links, code, quotes and tables, which is what a model needs.

use std::collections::HashMap;

use url::Url;

/// A page as Markdown, with its title.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Readable {
    pub title: Option<String>,
    pub markdown: String,
}

#[derive(Debug)]
enum Node {
    Element(Element),
    Text(String),
}

#[derive(Debug, Default)]
struct Element {
    name: String,
    attrs: HashMap<String, String>,
    children: Vec<Node>,
}

impl Element {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.get(name).map(String::as_str)
    }
}

const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];
/// Elements whose content is raw text up to the matching end tag.
const RAW: &[&str] = &["script", "style", "textarea", "title", "noscript", "xmp"];
/// Subtrees that are never content.
const SKIP: &[&str] = &[
    "script", "style", "noscript", "template", "svg", "math", "iframe", "object", "embed",
    "canvas", "nav", "header", "footer", "aside", "form", "button", "select", "input", "dialog",
    "head", "link", "meta", "video", "audio", "picture",
];
/// class / id words that mark page furniture.
const NOISE: &[&str] = &[
    "cookie",
    "consent",
    "sidebar",
    "advert",
    "advertisement",
    "newsletter",
    "share",
    "social",
    "breadcrumb",
    "skip-link",
];

/// Converts `html` (fetched from `base`) to Markdown.
pub fn html_to_markdown(html: &str, base: Option<&Url>) -> Readable {
    let root = parse(html);
    let title = find_first(&root, "title")
        .map(|t| collapse(&text_of(t)))
        .filter(|t| !t.is_empty());
    let content = largest(&root, "article")
        .or_else(|| find_first(&root, "main"))
        .or_else(|| find_first(&root, "body"))
        .unwrap_or(&root);
    let mut r = Renderer::new(base);
    r.children(content);
    Readable {
        title,
        markdown: tidy(&r.out),
    }
}

// ---------------------------------------------------------------------------------------
// Parsing

fn parse(html: &str) -> Element {
    let mut stack: Vec<Element> = vec![Element {
        name: "#root".into(),
        ..Element::default()
    }];
    let bytes = html.as_bytes();
    let mut i = 0;
    let mut text_start = 0;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        // Text before this '<'.
        push_text(&mut stack, &html[text_start..i]);
        let rest = &html[i..];
        if rest.starts_with("<!--") {
            i += rest.find("-->").map_or(rest.len(), |e| e + 3);
            text_start = i;
            continue;
        }
        if rest.starts_with("<!") || rest.starts_with("<?") {
            i += rest.find('>').map_or(rest.len(), |e| e + 1);
            text_start = i;
            continue;
        }
        let Some(end) = tag_end(rest) else {
            // A lone '<': keep it as text.
            text_start = i;
            i += 1;
            continue;
        };
        let tag = &rest[1..end];
        i += end + 1;
        text_start = i;
        if let Some(name) = tag.strip_prefix('/') {
            let name = name.trim().to_ascii_lowercase();
            close(&mut stack, &name);
            continue;
        }
        let (name, attrs, self_closing) = parse_tag(tag);
        if name.is_empty() || !name.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
            push_text(&mut stack, &html[i - end - 1..i]);
            continue;
        }
        auto_close(&mut stack, &name);
        let el = Element {
            name: name.clone(),
            attrs,
            children: Vec::new(),
        };
        if RAW.contains(&name.as_str()) {
            let close_tag = format!("</{name}");
            let lower = html[i..].to_ascii_lowercase();
            let stop = lower.find(&close_tag).map_or(html.len(), |p| i + p);
            let mut el = el;
            let raw = &html[i..stop];
            // title and textarea are "escapable" raw text; scripts and styles aren't.
            let text = if matches!(name.as_str(), "title" | "textarea") {
                decode_entities(raw)
            } else {
                raw.to_owned()
            };
            el.children.push(Node::Text(text));
            append(&mut stack, Node::Element(el));
            i = html[stop..].find('>').map_or(html.len(), |p| stop + p + 1);
            text_start = i;
            continue;
        }
        if self_closing || VOID.contains(&name.as_str()) {
            append(&mut stack, Node::Element(el));
        } else {
            stack.push(el);
        }
    }
    push_text(&mut stack, &html[text_start.min(html.len())..]);
    while stack.len() > 1 {
        if let Some(el) = stack.pop() {
            append(&mut stack, Node::Element(el));
        }
    }
    stack.pop().unwrap_or_default()
}

/// The index of the `>` that ends the tag starting at `s[0] == '<'`, skipping quoted values.
fn tag_end(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    if b.len() < 2 || !(b[1].is_ascii_alphabetic() || b[1] == b'/') {
        return None;
    }
    let mut quote: Option<u8> = None;
    for (j, &c) in b.iter().enumerate().skip(1) {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == b'"' || c == b'\'' => quote = Some(c),
            None if c == b'>' => return Some(j),
            None => {}
        }
    }
    None
}

fn parse_tag(tag: &str) -> (String, HashMap<String, String>, bool) {
    let self_closing = tag.trim_end().ends_with('/');
    let tag = tag.trim_end().trim_end_matches('/');
    let name_end = tag.find(|c: char| c.is_whitespace()).unwrap_or(tag.len());
    let name = tag[..name_end].to_ascii_lowercase();
    let mut attrs = HashMap::new();
    let s = &tag[name_end..];
    let b = s.as_bytes();
    let mut j = 0;
    while j < b.len() {
        while j < b.len() && (b[j].is_ascii_whitespace() || b[j] == b'/') {
            j += 1;
        }
        let k_start = j;
        while j < b.len() && !b[j].is_ascii_whitespace() && b[j] != b'=' {
            j += 1;
        }
        let key = s[k_start..j].to_ascii_lowercase();
        while j < b.len() && b[j].is_ascii_whitespace() {
            j += 1;
        }
        let mut value = String::new();
        if j < b.len() && b[j] == b'=' {
            j += 1;
            while j < b.len() && b[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < b.len() && (b[j] == b'"' || b[j] == b'\'') {
                let q = b[j];
                j += 1;
                let v_start = j;
                while j < b.len() && b[j] != q {
                    j += 1;
                }
                value = decode_entities(&s[v_start..j]);
                j += 1;
            } else {
                let v_start = j;
                while j < b.len() && !b[j].is_ascii_whitespace() {
                    j += 1;
                }
                value = decode_entities(&s[v_start..j]);
            }
        }
        if !key.is_empty() {
            attrs.entry(key).or_insert(value);
        }
    }
    (name, attrs, self_closing)
}

fn push_text(stack: &mut [Element], text: &str) {
    if !text.is_empty() {
        append(stack, Node::Text(decode_entities(text)));
    }
}

fn append(stack: &mut [Element], node: Node) {
    if let Some(top) = stack.last_mut() {
        top.children.push(node);
    }
}

/// Pops up to and including the innermost open `name`; ignores stray end tags.
fn close(stack: &mut Vec<Element>, name: &str) {
    let Some(pos) = stack.iter().rposition(|e| e.name == name) else {
        return;
    };
    if pos == 0 {
        return;
    }
    while stack.len() > pos {
        if let Some(el) = stack.pop() {
            append(stack, Node::Element(el));
        }
    }
}

/// Implied end tags: a new `<p>` closes an open `<p>`, a new `<li>` the previous `<li>`, …
fn auto_close(stack: &mut Vec<Element>, name: &str) {
    let closes: &[&str] = match name {
        "p" | "div" | "ul" | "ol" | "table" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "pre"
        | "blockquote" | "section" | "article" => &["p"],
        "li" => &["li", "p"],
        "dt" | "dd" => &["dt", "dd", "p"],
        "tr" => &["tr", "td", "th"],
        "td" | "th" => &["td", "th"],
        "option" => &["option"],
        _ => return,
    };
    // Only close within the nearest list/table container.
    let boundary = |n: &str| matches!(n, "ul" | "ol" | "table" | "tbody" | "thead" | "dl");
    if let Some(top) = stack.last()
        && closes.contains(&top.name.as_str())
        && !(name == "li" && boundary(&top.name))
    {
        let n = top.name.clone();
        close(stack, &n);
    } else if name == "li" || name == "tr" || name == "td" || name == "th" {
        // e.g. <li><p>a<li>b : close the open li above the p.
        for idx in (1..stack.len()).rev() {
            let n = stack[idx].name.as_str();
            if boundary(n) {
                break;
            }
            if closes.contains(&n) {
                let n = n.to_owned();
                close(stack, &n);
                break;
            }
        }
    }
}

fn find_first<'a>(el: &'a Element, name: &str) -> Option<&'a Element> {
    for c in &el.children {
        if let Node::Element(e) = c {
            if e.name == name {
                return Some(e);
            }
            if let Some(f) = find_first(e, name) {
                return Some(f);
            }
        }
    }
    None
}

/// The `name` element with the most text.
fn largest<'a>(el: &'a Element, name: &str) -> Option<&'a Element> {
    let mut all = Vec::new();
    collect(el, name, &mut all);
    all.into_iter()
        .map(|e| (text_of(e).len(), e))
        .filter(|(n, _)| *n > 200)
        .max_by_key(|(n, _)| *n)
        .map(|(_, e)| e)
}

fn collect<'a>(el: &'a Element, name: &str, out: &mut Vec<&'a Element>) {
    for c in &el.children {
        if let Node::Element(e) = c {
            if e.name == name {
                out.push(e);
            }
            collect(e, name, out);
        }
    }
}

fn text_of(el: &Element) -> String {
    let mut s = String::new();
    fn walk(el: &Element, s: &mut String) {
        for c in &el.children {
            match c {
                Node::Text(t) => s.push_str(t),
                Node::Element(e) if !matches!(e.name.as_str(), "script" | "style") => walk(e, s),
                Node::Element(_) => {}
            }
        }
    }
    walk(el, &mut s);
    s
}

// ---------------------------------------------------------------------------------------
// Entities and whitespace

/// Decodes the common named entities and every numeric one.
pub fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_owned();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        rest = &rest[pos..];
        let end = rest[1..]
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '#'))
            .map(|e| e + 1);
        let (name, consumed) = match end {
            Some(e) if rest[e..].starts_with(';') => (&rest[1..e], e + 1),
            Some(e) => (&rest[1..e], e),
            None => (&rest[1..], rest.len()),
        };
        match entity(name) {
            Some(c) if !name.is_empty() => {
                out.push_str(&c);
                rest = &rest[consumed..];
            }
            _ => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn entity(name: &str) -> Option<String> {
    if let Some(num) = name.strip_prefix('#') {
        let code = match num.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => num.parse().ok()?,
        };
        return char::from_u32(code).map(String::from);
    }
    let c = match name {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        "nbsp" => " ",
        "mdash" => "—",
        "ndash" => "–",
        "hellip" => "…",
        "lsquo" => "‘",
        "rsquo" => "’",
        "ldquo" => "“",
        "rdquo" => "”",
        "laquo" => "«",
        "raquo" => "»",
        "copy" => "©",
        "reg" => "®",
        "trade" => "™",
        "euro" => "€",
        "pound" => "£",
        "yen" => "¥",
        "cent" => "¢",
        "deg" => "°",
        "middot" => "·",
        "bull" => "•",
        "times" => "×",
        "divide" => "÷",
        "shy" => "",
        "zwj" | "zwnj" => "",
        "aelig" => "æ",
        "oslash" => "ø",
        "aring" => "å",
        "AElig" => "Æ",
        "Oslash" => "Ø",
        "Aring" => "Å",
        "auml" => "ä",
        "ouml" => "ö",
        "uuml" => "ü",
        "eacute" => "é",
        "szlig" => "ß",
        _ => return None,
    };
    Some(c.to_owned())
}

fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Trims lines, drops runs of blank lines.
fn tidy(s: &str) -> String {
    let mut out = String::new();
    let mut blank = 0;
    let mut in_code = false;
    for line in s.lines() {
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
        }
        let line = if in_code { line } else { line.trim_end() };
        if line.trim().is_empty() && !in_code {
            blank += 1;
            continue;
        }
        if !out.is_empty() {
            out.push_str(if blank > 0 { "\n\n" } else { "\n" });
        }
        blank = 0;
        out.push_str(line);
    }
    out
}

// ---------------------------------------------------------------------------------------
// Rendering

struct Renderer<'a> {
    out: String,
    base: Option<&'a Url>,
    /// Current list nesting: `None` for bullets, `Some(n)` for the next number.
    lists: Vec<Option<u32>>,
    /// Text since the last break ended with whitespace (or nothing was written yet).
    space: bool,
}

impl<'a> Renderer<'a> {
    fn new(base: Option<&'a Url>) -> Self {
        Self {
            out: String::new(),
            base,
            lists: Vec::new(),
            space: true,
        }
    }

    fn sub(&self) -> Renderer<'a> {
        Renderer::new(self.base)
    }

    fn indent(&self) -> String {
        "  ".repeat(self.lists.len())
    }

    /// Ends the current block: a blank line, or a new line inside lists.
    fn block(&mut self) {
        let trimmed = self.out.trim_end_matches([' ', '\t']).len();
        self.out.truncate(trimmed);
        if self.out.is_empty() {
            return;
        }
        if self.lists.is_empty() {
            while !self.out.ends_with("\n\n") {
                self.out.push('\n');
            }
        } else {
            if !self.out.ends_with('\n') {
                self.out.push('\n');
            }
            self.out.push_str(&self.indent());
        }
        self.space = true;
    }

    fn text(&mut self, t: &str) {
        for (i, word) in t.split_whitespace().enumerate() {
            if (i > 0 || t.starts_with(char::is_whitespace)) && !self.space {
                self.out.push(' ');
            }
            self.out.push_str(word);
            self.space = false;
        }
        if t.ends_with(char::is_whitespace) && !self.space && !t.trim().is_empty() {
            self.out.push(' ');
            self.space = true;
        }
    }

    fn raw(&mut self, s: &str) {
        self.out.push_str(s);
        self.space = s.ends_with(char::is_whitespace);
    }

    fn children(&mut self, el: &Element) {
        for c in &el.children {
            match c {
                Node::Text(t) => self.text(t),
                Node::Element(e) => self.element(e),
            }
        }
    }

    fn is_noise(e: &Element) -> bool {
        if SKIP.contains(&e.name.as_str()) {
            return true;
        }
        if e.attr("hidden").is_some() || e.attr("aria-hidden") == Some("true") {
            return true;
        }
        if let Some(role) = e.attr("role")
            && matches!(
                role,
                "navigation" | "banner" | "contentinfo" | "complementary" | "search"
            )
        {
            return true;
        }
        let words = format!(
            "{} {}",
            e.attr("class").unwrap_or(""),
            e.attr("id").unwrap_or("")
        )
        .to_ascii_lowercase();
        words
            .split(|c: char| c.is_whitespace() || c == '_')
            .any(|w| NOISE.contains(&w))
    }

    fn url(&self, href: &str) -> Option<String> {
        let href = href.trim();
        if href.is_empty()
            || href.starts_with('#')
            || href.to_ascii_lowercase().starts_with("javascript:")
            || href.starts_with("data:")
        {
            return None;
        }
        match self.base {
            Some(b) => b.join(href).ok().map(|u| u.to_string()),
            None => Some(href.to_owned()),
        }
    }

    fn inline_of(&self, e: &Element) -> String {
        let mut r = self.sub();
        r.children(e);
        collapse(&r.out)
    }

    fn element(&mut self, e: &Element) {
        if Self::is_noise(e) {
            return;
        }
        match e.name.as_str() {
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                let level = usize::from(e.name.as_bytes()[1] - b'0');
                let text = self.inline_of(e);
                if text.is_empty() {
                    return;
                }
                self.block();
                self.raw(&format!("{} {text}", "#".repeat(level)));
                self.block();
            }
            "p" | "div" | "section" | "article" | "main" | "figure" | "figcaption" | "dl"
            | "address" | "details" | "summary" | "center" => {
                self.block();
                self.children(e);
                self.block();
            }
            "dt" | "dd" => {
                self.block();
                if e.name == "dd" {
                    self.raw(": ");
                }
                self.children(e);
            }
            "br" => {
                if self.lists.is_empty() {
                    self.raw("\n");
                } else {
                    let ind = self.indent();
                    self.raw(&format!("\n{ind}"));
                }
                self.space = true;
            }
            "hr" => {
                self.block();
                self.raw("---");
                self.block();
            }
            "ul" | "ol" => {
                let start = e.attr("start").and_then(|s| s.parse().ok()).unwrap_or(1);
                self.block();
                self.lists
                    .push(if e.name == "ol" { Some(start) } else { None });
                self.children(e);
                self.lists.pop();
                self.block();
            }
            "li" => {
                let trimmed = self.out.trim_end_matches(' ').len();
                self.out.truncate(trimmed);
                if !self.out.ends_with('\n') && !self.out.is_empty() {
                    self.out.push('\n');
                }
                let depth = self.lists.len().saturating_sub(1);
                let marker = match self.lists.last_mut() {
                    Some(Some(n)) => {
                        let m = format!("{n}. ");
                        *n += 1;
                        m
                    }
                    _ => "- ".into(),
                };
                self.raw(&format!("{}{marker}", "  ".repeat(depth)));
                self.space = true;
                if self.lists.is_empty() {
                    self.lists.push(None);
                    self.children(e);
                    self.lists.pop();
                } else {
                    self.children(e);
                }
            }
            "a" => {
                let text = self.inline_of(e);
                match e.attr("href").and_then(|h| self.url(h)) {
                    Some(url) if !text.is_empty() => {
                        self.text_gap();
                        self.raw(&format!("[{}]({url})", text.replace(']', "\\]")));
                    }
                    _ => self.text(&format!(" {text} ")),
                }
            }
            "img" => {
                let alt = collapse(e.attr("alt").unwrap_or(""));
                if alt.is_empty() {
                    return;
                }
                if let Some(src) = e.attr("src").and_then(|s| self.url(s)) {
                    self.text_gap();
                    self.raw(&format!("![{alt}]({src})"));
                }
            }
            "strong" | "b" => self.wrap(e, "**"),
            "em" | "i" => self.wrap(e, "*"),
            "del" | "s" | "strike" => self.wrap(e, "~~"),
            "code" | "kbd" | "samp" | "tt" => {
                let text = collapse(&text_of(e));
                if !text.is_empty() {
                    self.text_gap();
                    let tick = if text.contains('`') { "``" } else { "`" };
                    self.raw(&format!("{tick}{text}{tick}"));
                }
            }
            "pre" => {
                let lang = e
                    .attr("class")
                    .or_else(|| {
                        match e.children.iter().find_map(|c| match c {
                            Node::Element(c) if c.name == "code" => Some(c),
                            _ => None,
                        }) {
                            Some(code) => code.attr("class"),
                            None => None,
                        }
                    })
                    .and_then(|c| {
                        c.split_whitespace()
                            .find_map(|w| w.strip_prefix("language-").or(w.strip_prefix("lang-")))
                    })
                    .unwrap_or("")
                    .to_owned();
                let code = text_of(e);
                let code = code.trim_matches('\n');
                self.block();
                self.raw(&format!("```{lang}\n{code}\n```"));
                self.block();
            }
            "blockquote" => {
                let mut r = self.sub();
                r.children(e);
                let inner = tidy(&r.out);
                if inner.is_empty() {
                    return;
                }
                self.block();
                let quoted: Vec<String> = inner
                    .lines()
                    .map(|l| {
                        if l.is_empty() {
                            ">".into()
                        } else {
                            format!("> {l}")
                        }
                    })
                    .collect();
                self.raw(&quoted.join("\n"));
                self.block();
            }
            "table" => self.table(e),
            _ => self.children(e),
        }
    }

    /// A space before an inline construct when the text before needs one.
    fn text_gap(&mut self) {
        if !self.space && !self.out.ends_with(['(', '[', '"', '\'', '/']) && !self.out.is_empty() {
            self.out.push(' ');
        }
    }

    fn wrap(&mut self, e: &Element, mark: &str) {
        let text = self.inline_of(e);
        if text.is_empty() {
            return;
        }
        self.text_gap();
        self.raw(&format!("{mark}{text}{mark}"));
    }

    fn table(&mut self, e: &Element) {
        let mut rows: Vec<Vec<String>> = Vec::new();
        let mut header_rows = 0;
        fn walk<'b>(
            el: &'b Element,
            rows: &mut Vec<&'b Element>,
            in_head: bool,
            heads: &mut usize,
        ) {
            for c in &el.children {
                if let Node::Element(x) = c {
                    match x.name.as_str() {
                        "tr" => {
                            if in_head {
                                *heads += 1;
                            }
                            rows.push(x);
                        }
                        "thead" => walk(x, rows, true, heads),
                        "table" => {}
                        _ => walk(x, rows, in_head, heads),
                    }
                }
            }
        }
        let mut trs = Vec::new();
        walk(e, &mut trs, false, &mut header_rows);
        for tr in &trs {
            let cells: Vec<String> = tr
                .children
                .iter()
                .filter_map(|c| match c {
                    Node::Element(c) if c.name == "td" || c.name == "th" => {
                        Some(self.inline_of(c).replace('|', "\\|"))
                    }
                    _ => None,
                })
                .collect();
            if !cells.is_empty() {
                rows.push(cells);
            }
        }
        if rows.is_empty() {
            return;
        }
        let width = rows.iter().map(Vec::len).max().unwrap_or(0);
        self.block();
        let mut lines = Vec::new();
        for (i, row) in rows.iter().enumerate() {
            let mut cells = row.clone();
            cells.resize(width, String::new());
            lines.push(format!("| {} |", cells.join(" | ")));
            if i == 0 {
                lines.push(format!("|{}", " --- |".repeat(width)));
            }
        }
        self.raw(&lines.join("\n"));
        self.block();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn md(html: &str) -> String {
        html_to_markdown(
            html,
            Url::parse("https://example.com/docs/page").ok().as_ref(),
        )
        .markdown
    }

    #[test]
    fn keeps_structure_and_drops_furniture() {
        let page = r#"<!DOCTYPE html><html><head><title>Oxide &amp; friends</title>
            <style>body{}</style><script>var x = "<p>no</p>";</script></head>
            <body><nav><a href="/">Home</a></nav>
            <header>Site header</header>
            <main><h1>Hello <em>world</em></h1>
            <p>First   paragraph with a <a href="/next">link</a> and <code>x &lt; y</code>.</p>
            <p>Second<br>line</p>
            <ul><li>one<li>two <b>bold</b></ul>
            <ol start="3"><li>three</li><li>four<ul><li>nested</li></ul></li></ol>
            <pre><code class="language-rust">fn main() {
    println!("hi");
}</code></pre>
            <blockquote><p>Quoted</p><p>twice</p></blockquote>
            <div class="cookie-banner consent">Accept cookies</div>
            <img src="pic.png" alt="A picture"><img src="x.png">
            </main><footer>© 2026</footer></body></html>"#;
        let r = html_to_markdown(
            page,
            Url::parse("https://example.com/docs/page").ok().as_ref(),
        );
        assert_eq!(r.title.as_deref(), Some("Oxide & friends"));
        let m = r.markdown;
        assert!(m.starts_with("# Hello *world*"), "{m}");
        assert!(
            m.contains("First paragraph with a [link](https://example.com/next) and `x < y`."),
            "{m}"
        );
        assert!(m.contains("Second\nline"), "{m}");
        assert!(m.contains("- one\n- two **bold**"), "{m}");
        assert!(m.contains("3. three\n4. four\n  - nested"), "{m}");
        assert!(
            m.contains("```rust\nfn main() {\n    println!(\"hi\");\n}\n```"),
            "{m}"
        );
        assert!(m.contains("> Quoted\n>\n> twice"), "{m}");
        assert!(
            m.contains("![A picture](https://example.com/docs/pic.png)"),
            "{m}"
        );
        for gone in ["Home", "Site header", "cookies", "©", "var x", "body{}"] {
            assert!(!m.contains(gone), "{gone} in {m}");
        }
    }

    #[test]
    fn tables_and_articles() {
        let long = "word ".repeat(60);
        let page = format!(
            "<body><p>outside</p><article><p>short</p></article><article><h2>Main</h2><p>{long}</p>\
             <table><thead><tr><th>A</th><th>B|c</th></tr></thead>\
             <tbody><tr><td>1</td><td>2</td></tr><tr><td>3</td></tr></tbody></table></article></body>"
        );
        let m = md(&page);
        assert!(m.starts_with("## Main"), "{m}");
        assert!(!m.contains("outside"));
        assert!(
            m.contains("| A | B\\|c |\n| --- | --- |\n| 1 | 2 |\n| 3 |  |"),
            "{m}"
        );
    }

    #[test]
    fn entities_and_broken_markup() {
        assert_eq!(
            decode_entities("a &amp; b &#x41;&#66; &copy; &bogus; &"),
            "a & b AB © &bogus; &"
        );
        let m = md("<p>Unclosed <b>bold <i>and</p><p>next 5 < 6 & 7 > 3</p></div></span>");
        assert!(m.contains("Unclosed **bold *and***"), "{m}");
        assert!(m.contains("next 5 < 6 & 7 > 3"), "{m}");
        assert_eq!(md("plain text only"), "plain text only");
        assert_eq!(md(""), "");
        let m = md("<a href=\"javascript:void(0)\">click</a> <a href=\"#top\">top</a>");
        assert_eq!(m, "click top");
    }
}
