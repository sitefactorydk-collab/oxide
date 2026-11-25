//! The one edit engine (Phase 26): every place Oxide changes an existing text file with
//! edit blocks — chats' `edit_file`, the app builder's `app_edit_file`, the `artifact` tool's
//! `update` and the Utility Apps' `ai.edit` — goes through [`apply_edits`].
//!
//! An edit names an exact passage of the current text (`old`) and what replaces it (`new`).
//! Edits apply in order, each to the result of the one before, and all or nothing: when one
//! fails, the caller writes nothing. Matching is forgiving in a fixed order, because models
//! copy text imperfectly:
//!
//! 1. exact;
//! 2. with line endings normalized (CRLF/LF; the file keeps its own);
//! 3. with the whitespace around the passage trimmed;
//! 4. line by line, ignoring trailing whitespace;
//! 5. line by line, ignoring indentation (the replacement is re-indented to the match's).
//!
//! The first level that finds the passage decides: it must find it exactly once unless the
//! edit says `replace_all`. Errors say which edit failed and why, and for text that wasn't
//! found they quote the closest lines in the file, so a model can correct itself.
//!
//! Pure: no I/O.

use std::fmt;

use serde::{Deserialize, Serialize};

/// One edit block: replace `old` with `new`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Edit {
    pub old: String,
    pub new: String,
    /// Replace every place `old` appears instead of requiring exactly one.
    #[serde(default)]
    pub replace_all: bool,
}

impl Edit {
    pub fn new(old: impl Into<String>, new: impl Into<String>) -> Self {
        Self {
            old: old.into(),
            new: new.into(),
            replace_all: false,
        }
    }

    pub fn all(old: impl Into<String>, new: impl Into<String>) -> Self {
        Self {
            replace_all: true,
            ..Self::new(old, new)
        }
    }
}

/// How an edit's `old` was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MatchKind {
    Exact,
    LineEndings,
    Trimmed,
    TrailingWhitespace,
    Indentation,
    /// `old` was empty and the file was empty: the edit is the whole content.
    EmptyFile,
}

impl MatchKind {
    /// Words for a model or a person, for the matches that weren't exact.
    pub fn note(self) -> Option<&'static str> {
        match self {
            MatchKind::Exact | MatchKind::EmptyFile => None,
            MatchKind::LineEndings => Some("with line endings normalized"),
            MatchKind::Trimmed => Some("without the whitespace around it"),
            MatchKind::TrailingWhitespace => Some("ignoring trailing whitespace"),
            MatchKind::Indentation => Some("ignoring indentation (the new text was re-indented)"),
        }
    }
}

/// What one edit did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedEdit {
    /// Position in the list (0-based).
    pub index: usize,
    pub matched: MatchKind,
    /// How many places were replaced (1 unless `replace_all`).
    pub replacements: usize,
}

/// Lines of the new text that an edit wrote: `count` lines from `start` (1-based). A count of
/// 0 means lines were removed just before line `start`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineRange {
    pub start: usize,
    pub count: usize,
}

impl LineRange {
    pub fn label(&self) -> String {
        match self.count {
            0 => format!("removed at line {}", self.start),
            1 => format!("line {}", self.start),
            n => format!("lines {}–{}", self.start, self.start + n - 1),
        }
    }
}

/// The result of [`apply_edits`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditResult {
    pub text: String,
    pub applied: Vec<AppliedEdit>,
    /// The changed lines in `text`, in order, merged where they touch.
    pub ranges: Vec<LineRange>,
}

impl EditResult {
    /// “2 edits applied: lines 4–6, line 20.” plus notes on matches that weren't exact.
    pub fn summary(&self) -> String {
        let n = self.applied.len();
        let replaced: usize = self.applied.iter().map(|a| a.replacements).sum();
        let mut out = if n == 1 {
            if replaced == 1 {
                "1 edit applied".to_owned()
            } else {
                format!("1 edit applied ({replaced} places)")
            }
        } else if replaced == n {
            format!("{n} edits applied")
        } else {
            format!("{n} edits applied ({replaced} places)")
        };
        if self.ranges.is_empty() {
            out.push_str("; the text didn't change.");
        } else {
            let labels: Vec<String> = self.ranges.iter().map(LineRange::label).collect();
            out.push_str(&format!(": {}.", labels.join(", ")));
        }
        for a in &self.applied {
            if let Some(note) = a.matched.note() {
                out.push_str(&format!(" Edit {} matched {note}.", a.index + 1));
            }
        }
        out
    }

    /// The changed lines with their numbers (at most `max_lines`), to show what was written.
    pub fn excerpt(&self, max_lines: usize) -> String {
        let lines: Vec<&str> = self.text.split('\n').collect();
        let mut out = Vec::new();
        let mut shown = 0;
        for (i, r) in self.ranges.iter().enumerate() {
            if i > 0 {
                out.push("…".to_owned());
            }
            if r.count == 0 {
                out.push(format!("(lines removed before line {})", r.start));
                continue;
            }
            for n in r.start..r.start + r.count {
                if shown == max_lines {
                    out.push(format!(
                        "[… {} more changed lines]",
                        self.changed_lines() - shown
                    ));
                    return out.join("\n");
                }
                let line = lines.get(n - 1).copied().unwrap_or("");
                out.push(format!(
                    "{n:>5}  {}",
                    clip(line.trim_end_matches('\r'), 200)
                ));
                shown += 1;
            }
        }
        out.join("\n")
    }

    fn changed_lines(&self) -> usize {
        self.ranges.iter().map(|r| r.count).sum()
    }
}

/// Lines of the file near text that wasn't found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Closest {
    /// 1-based line of `lines[0]`.
    pub start: usize,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditErrorKind {
    /// No edits were given.
    NoEdits,
    /// `old` is empty and the file isn't.
    EmptyOld,
    /// `old` and `new` are the same.
    Unchanged,
    NotFound {
        closest: Option<Closest>,
    },
    /// Found more than once without `replace_all`; `lines` are where (1-based, at most 10).
    Ambiguous {
        count: usize,
        lines: Vec<usize>,
    },
}

/// Why the edits weren't applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditError {
    /// The edit that failed (0-based).
    pub index: usize,
    pub total: usize,
    pub kind: EditErrorKind,
}

impl EditError {
    /// The message, naming the field that holds the text to replace (`old_str`, `old_text`,
    /// `old`) the way the tool calls it.
    pub fn message(&self, field: &str) -> String {
        let which = if self.total > 1 {
            format!("Edit {} of {}: ", self.index + 1, self.total)
        } else {
            String::new()
        };
        let nothing = if self.total > 1 {
            " Nothing was changed (edits apply all or nothing)."
        } else {
            ""
        };
        if self.kind == EditErrorKind::NoEdits {
            return "No edits were given.".to_owned();
        }
        format!("{which}{}{nothing}", self.detail(field))
    }

    /// Why this edit failed, without saying which edit it was.
    pub fn detail(&self, field: &str) -> String {
        match &self.kind {
            EditErrorKind::NoEdits => "No edits were given.".to_owned(),
            EditErrorKind::EmptyOld => format!(
                "{field} is empty. An empty {field} only works on an empty file; to replace a \
                 whole file, write it instead."
            ),
            EditErrorKind::Unchanged => {
                format!("{field} and the new text are the same, so there is nothing to change.")
            }
            EditErrorKind::Ambiguous { count, lines } => {
                let at: Vec<String> = lines.iter().map(usize::to_string).collect();
                format!(
                    "{field} was found {count} times (lines {}). Include more surrounding lines \
                     so it is found once, or replace every place with replace_all.",
                    at.join(", ")
                )
            }
            EditErrorKind::NotFound { closest: None } => format!(
                "{field} wasn't found, and nothing in the file looks like it. Read the file again \
                 and copy the text exactly."
            ),
            EditErrorKind::NotFound { closest: Some(c) } => {
                let end = c.start + c.lines.len().saturating_sub(1);
                let span = if end > c.start {
                    format!("lines {}–{end}", c.start)
                } else {
                    format!("line {}", c.start)
                };
                let quoted: Vec<String> = c
                    .lines
                    .iter()
                    .enumerate()
                    .map(|(i, l)| format!("{:>5}  {l}", c.start + i))
                    .collect();
                format!(
                    "{field} wasn't found. The closest text in the file is at {span}:\n{}\n\
                     Copy {field} exactly from the file (without the line numbers) and try again.",
                    quoted.join("\n")
                )
            }
        }
    }
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message("old"))
    }
}

impl std::error::Error for EditError {}

/// Applies `edits` to `text` in order, all or nothing.
pub fn apply_edits(text: &str, edits: &[Edit]) -> Result<EditResult, EditError> {
    let total = edits.len();
    if total == 0 {
        return Err(EditError {
            index: 0,
            total,
            kind: EditErrorKind::NoEdits,
        });
    }
    let mut current = text.to_owned();
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut applied = Vec::with_capacity(total);
    for (index, edit) in edits.iter().enumerate() {
        let fail = |kind| EditError { index, total, kind };
        if edit.old.is_empty() {
            if !current.is_empty() {
                return Err(fail(EditErrorKind::EmptyOld));
            }
            let new = edit.new.clone();
            record(&mut ranges, 0, 0, new.len());
            current = new;
            applied.push(AppliedEdit {
                index,
                matched: MatchKind::EmptyFile,
                replacements: 1,
            });
            continue;
        }
        if edit.old == edit.new {
            return Err(fail(EditErrorKind::Unchanged));
        }
        let Some((matched, spans)) = find(&current, edit) else {
            return Err(fail(EditErrorKind::NotFound {
                closest: closest(&current, &edit.old),
            }));
        };
        if spans.len() > 1 && !edit.replace_all {
            let lines = spans
                .iter()
                .take(10)
                .map(|s| line_of(&current, s.start))
                .collect();
            return Err(fail(EditErrorKind::Ambiguous {
                count: spans.len(),
                lines,
            }));
        }
        let replacements = spans.len();
        // From the last to the first, so earlier offsets stay valid.
        for s in spans.iter().rev() {
            current.replace_range(s.start..s.end, &s.new);
            record(&mut ranges, s.start, s.end, s.new.len());
        }
        applied.push(AppliedEdit {
            index,
            matched,
            replacements,
        });
    }
    let ranges = line_ranges(&current, ranges);
    Ok(EditResult {
        text: current,
        applied,
        ranges,
    })
}

/// Applies the edits that match and leaves out the ones that don't (each tried on the result
/// of the ones before it). Returns the result of the matching edits (`None` when none
/// matched) and why each other edit failed, numbered by its place in `edits`.
pub fn apply_what_matches(text: &str, edits: &[Edit]) -> (Option<EditResult>, Vec<EditError>) {
    let total = edits.len();
    let mut current = text.to_owned();
    let mut ok = Vec::new();
    let mut failed = Vec::new();
    for (index, edit) in edits.iter().enumerate() {
        match apply_edits(&current, std::slice::from_ref(edit)) {
            Ok(done) => {
                current = done.text;
                ok.push(edit.clone());
            }
            Err(e) => failed.push(EditError {
                index,
                total,
                kind: e.kind,
            }),
        }
    }
    if ok.is_empty() {
        return (None, failed);
    }
    // The same edits again in one pass: the same text, with ranges and summary for all of them.
    match apply_edits(text, &ok) {
        Ok(done) => (Some(done), failed),
        Err(_) => (None, failed),
    }
}

/// A place to replace: bytes `start..end` of the current text, and what goes there.
#[derive(Debug)]
struct Span {
    start: usize,
    end: usize,
    new: String,
}

/// The first matching level that finds `old`, with every place it found.
fn find(text: &str, edit: &Edit) -> Option<(MatchKind, Vec<Span>)> {
    let exact = |old: &str, new: &str| -> Vec<Span> {
        text.match_indices(old)
            .map(|(i, m)| Span {
                start: i,
                end: i + m.len(),
                new: new.to_owned(),
            })
            .collect()
    };
    let spans = exact(&edit.old, &edit.new);
    if !spans.is_empty() {
        return Some((MatchKind::Exact, spans));
    }

    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let old = to_eol(&edit.old, eol);
    let new = to_eol(&edit.new, eol);
    if old != edit.old {
        let spans = exact(&old, &new);
        if !spans.is_empty() {
            return Some((MatchKind::LineEndings, spans));
        }
    }

    let trimmed = old.trim();
    if !trimmed.is_empty() && trimmed != old {
        let lead = &old[..old.len() - old.trim_start().len()];
        let trail = &old[old.trim_end().len()..];
        let new_trimmed = new.strip_prefix(lead).unwrap_or(&new);
        let new_trimmed = new_trimmed.strip_suffix(trail).unwrap_or(new_trimmed);
        let spans = exact(trimmed, new_trimmed);
        if !spans.is_empty() {
            return Some((MatchKind::Trimmed, spans));
        }
    }

    let old_lf = edit.old.replace("\r\n", "\n");
    let ends_nl = old_lf.ends_with('\n');
    let body = old_lf.strip_suffix('\n').unwrap_or(&old_lf);
    let old_lines: Vec<&str> = body.split('\n').collect();
    if old_lines.iter().all(|l| l.trim().is_empty()) {
        return None;
    }
    let file = file_lines(text);
    let new_lf = edit.new.replace("\r\n", "\n");
    for kind in [MatchKind::TrailingWhitespace, MatchKind::Indentation] {
        let same = |a: &str, b: &str| match kind {
            MatchKind::TrailingWhitespace => a.trim_end() == b.trim_end(),
            _ => a.trim() == b.trim(),
        };
        let m = old_lines.len();
        let mut spans = Vec::new();
        let mut i = 0;
        while i + m <= file.len() {
            let hit = (0..m).all(|j| {
                same(
                    &text[file[i + j].start..file[i + j].content_end],
                    old_lines[j],
                )
            });
            if !hit {
                i += 1;
                continue;
            }
            let last = &file[i + m - 1];
            let (end, keep_nl) = if ends_nl && last.end > last.content_end {
                (last.end, true)
            } else {
                (last.content_end, false)
            };
            let mut replacement = if kind == MatchKind::Indentation {
                let file_indent = (i..i + m)
                    .map(|k| &text[file[k].start..file[k].content_end])
                    .find(|l| !l.trim().is_empty())
                    .map(indent_of)
                    .unwrap_or("");
                let old_indent = old_lines
                    .iter()
                    .find(|l| !l.trim().is_empty())
                    .map(|l| indent_of(l))
                    .unwrap_or("");
                reindent(&new_lf, old_indent, file_indent)
            } else {
                new_lf.clone()
            };
            if ends_nl && !keep_nl {
                // The passage ends the file without a line break: so does its replacement.
                if let Some(r) = replacement.strip_suffix('\n') {
                    replacement = r.to_owned();
                }
            }
            spans.push(Span {
                start: file[i].start,
                end,
                new: to_eol(&replacement, eol),
            });
            i += m;
        }
        if !spans.is_empty() {
            return Some((kind, spans));
        }
    }
    None
}

/// A line of the text: `start..content_end` is its text, `end` is after its line break.
struct FileLine {
    start: usize,
    content_end: usize,
    end: usize,
}

fn file_lines(text: &str) -> Vec<FileLine> {
    let mut out = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let (content_end, end) = match text[start..].find('\n') {
            Some(i) => {
                let nl = start + i;
                let content_end = if nl > start && text.as_bytes()[nl - 1] == b'\r' {
                    nl - 1
                } else {
                    nl
                };
                (content_end, nl + 1)
            }
            None => (text.len(), text.len()),
        };
        out.push(FileLine {
            start,
            content_end,
            end,
        });
        start = end;
    }
    out
}

fn to_eol(s: &str, eol: &str) -> String {
    let lf = s.replace("\r\n", "\n");
    if eol == "\n" {
        lf
    } else {
        lf.replace('\n', eol)
    }
}

fn indent_of(line: &str) -> &str {
    &line[..line.len() - line.trim_start_matches([' ', '\t']).len()]
}

/// Moves `new`'s lines from `old_indent` to `file_indent`.
fn reindent(new: &str, old_indent: &str, file_indent: &str) -> String {
    if old_indent == file_indent {
        return new.to_owned();
    }
    new.split('\n')
        .map(|line| {
            if line.trim().is_empty() {
                line.to_owned()
            } else if let Some(extra) = file_indent.strip_prefix(old_indent) {
                format!("{extra}{line}")
            } else if old_indent.starts_with(file_indent) {
                let cut = (old_indent.len() - file_indent.len()).min(indent_of(line).len());
                line[cut..].to_owned()
            } else if let Some(rest) = line.strip_prefix(old_indent) {
                format!("{file_indent}{rest}")
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Notes that bytes `start..end` became `len` bytes, shifting and merging earlier ranges.
fn record(ranges: &mut Vec<(usize, usize)>, start: usize, end: usize, len: usize) {
    let delta = len as isize - (end - start) as isize;
    let shift = |x: usize| (x as isize + delta).max(0) as usize;
    let mut lo = start;
    let mut hi = start + len;
    let mut kept = Vec::with_capacity(ranges.len() + 1);
    for &(a, b) in ranges.iter() {
        if b < start {
            kept.push((a, b));
        } else if a > end {
            kept.push((shift(a), shift(b)));
        } else {
            // Overlaps or touches the replaced passage: becomes part of it.
            lo = lo.min(a);
            hi = hi.max(if b > end { shift(b) } else { start + len });
        }
    }
    kept.push((lo, hi));
    kept.sort_unstable();
    *ranges = kept;
}

fn line_of(text: &str, byte: usize) -> usize {
    text[..byte.min(text.len())].matches('\n').count() + 1
}

fn line_ranges(text: &str, mut bytes: Vec<(usize, usize)>) -> Vec<LineRange> {
    bytes.sort_unstable();
    let mut out: Vec<LineRange> = Vec::new();
    for (a, b) in bytes {
        let start = line_of(text, a);
        let count = if b <= a {
            0
        } else {
            // A passage ending with its line break doesn't reach into the next line.
            let last = if text.as_bytes().get(b - 1) == Some(&b'\n') {
                b - 1
            } else {
                b
            };
            line_of(text, last) - start + 1
        };
        if let Some(prev) = out.last_mut() {
            let prev_end = prev.start + prev.count;
            if start <= prev_end {
                let end = (start + count).max(prev_end);
                prev.count = end - prev.start;
                continue;
            }
        }
        out.push(LineRange { start, count });
    }
    out
}

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        let cut: String = s.chars().take(max).collect();
        format!("{cut}…")
    }
}

/// The window of the file most like `old`, if anything is alike enough to help.
fn closest(text: &str, old: &str) -> Option<Closest> {
    let old_lf = old.replace("\r\n", "\n");
    let old_lines: Vec<&str> = old_lf.trim_matches('\n').split('\n').collect();
    let file: Vec<&str> = text.split('\n').map(|l| l.trim_end_matches('\r')).collect();
    if file.is_empty() || old_lines.iter().all(|l| l.trim().is_empty()) {
        return None;
    }
    // Keep the work bounded on large files.
    let budget = 400_000usize;
    let m = old_lines
        .len()
        .min(40)
        .min((budget / file.len().max(1)).max(1))
        .min(file.len());
    let old_grams: Vec<Vec<u64>> = old_lines[..m].iter().map(|l| bigrams(l)).collect();
    let file_grams: Vec<Vec<u64>> = file.iter().map(|l| bigrams(l)).collect();
    let weights: Vec<usize> = old_lines[..m]
        .iter()
        .map(|l| usize::from(!l.trim().is_empty()))
        .collect();
    let weight: usize = weights.iter().sum::<usize>().max(1);
    let mut best = (0.0f64, 0usize);
    for i in 0..=file.len() - m {
        let mut score = 0.0;
        for j in 0..m {
            if weights[j] == 1 {
                score += dice(&old_grams[j], &file_grams[i + j]);
            }
        }
        let score = score / weight as f64;
        if score > best.0 {
            best = (score, i);
        }
    }
    if best.0 < 0.35 {
        return None;
    }
    let shown = m.min(12);
    Some(Closest {
        start: best.1 + 1,
        lines: file[best.1..best.1 + shown]
            .iter()
            .map(|l| clip(l, 160))
            .collect(),
    })
}

/// The line's character pairs (whitespace collapsed), sorted, for a quick likeness score.
fn bigrams(line: &str) -> Vec<u64> {
    let chars: Vec<char> = line
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .collect();
    let mut out: Vec<u64> = chars
        .windows(2)
        .map(|w| (u64::from(u32::from(w[0])) << 32) | u64::from(u32::from(w[1])))
        .collect();
    out.sort_unstable();
    out
}

/// Sørensen–Dice likeness of two sorted bigram lists (0–1).
fn dice(a: &[u64], b: &[u64]) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let (mut i, mut j, mut common) = (0, 0, 0usize);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                common += 1;
                i += 1;
                j += 1;
            }
        }
    }
    2.0 * common as f64 / (a.len() + b.len()) as f64
}

/// How many lines were added and removed from `old` to `new` (a line diff; line endings and
/// trailing whitespace count as changes only when the text differs). Large differences are
/// counted roughly, by which lines are missing on each side.
pub fn line_changes(old: &str, new: &str) -> (usize, usize) {
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    let head = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let tail = a[head..]
        .iter()
        .rev()
        .zip(b[head..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let a = &a[head..a.len() - tail];
    let b = &b[head..b.len() - tail];
    if a.is_empty() || b.is_empty() {
        return (b.len(), a.len());
    }
    if a.len().saturating_mul(b.len()) > 4_000_000 {
        use std::collections::HashMap;
        let mut count: HashMap<&str, isize> = HashMap::new();
        for l in a {
            *count.entry(l).or_default() += 1;
        }
        let mut added = 0;
        for l in b {
            let c = count.entry(l).or_default();
            if *c > 0 {
                *c -= 1;
            } else {
                added += 1;
            }
        }
        let removed = count.values().filter(|c| **c > 0).sum::<isize>() as usize;
        return (added, removed);
    }
    // Longest common subsequence, one row at a time.
    let mut prev = vec![0usize; b.len() + 1];
    let mut cur = vec![0usize; b.len() + 1];
    for x in a {
        for (j, y) in b.iter().enumerate() {
            cur[j + 1] = if x == y {
                prev[j] + 1
            } else {
                cur[j].max(prev[j + 1])
            };
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    let common = prev[b.len()];
    (b.len() - common, a.len() - common)
}

#[cfg(test)]
#[path = "edit_tests.rs"]
mod tests;
