//! Skills: folders with a `SKILL.md` (YAML front matter with `name` and `description`, then
//! instructions) in Oxide's skills folder, plus any files the instructions refer to.
//!
//! Models see every enabled skill's name and description in the system prompt and call the
//! built-in `load_skill` tool to read a skill's instructions (and then its files) when a task
//! matches. Skills can be written in Settings, copied from a folder or unpacked from a zip
//! (`.zip` / `.skill`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use oxide_core::extension::{MAX_SKILL_DESCRIPTION, MAX_SKILL_NAME, skill_slug};
use oxide_core::{LOAD_SKILL, Skill, SkillDetail, SkillInput};
use oxide_mcp::archive::{self, ZipLimits};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::tools::{Builtin, ToolEntry, ToolKind};

/// The file that makes a folder a skill.
pub const SKILL_FILE: &str = "SKILL.md";
/// Which skills are off (folder names), kept beside them.
const STATE_FILE: &str = ".oxide-skills.json";
/// Most files and bytes one skill may bring along.
const MAX_FILES: usize = 2_000;
const MAX_BYTES: u64 = 50 * 1024 * 1024;
/// Most characters `load_skill` returns from one file.
pub const MAX_LOAD_CHARS: usize = 100_000;
/// Files listed for the model, at most.
const MAX_LISTED: usize = 200;

/// Front matter fields Oxide uses.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FrontMatter {
    pub name: Option<String>,
    pub description: Option<String>,
    pub license: Option<String>,
}

/// Unquotes a YAML scalar (`'…'`, `"…"` with escapes, or plain).
fn scalar(raw: &str) -> String {
    let mut t = raw.trim();
    // A quoted value may be followed by a comment: cut after the closing quote.
    if let Some(q) = t.chars().next().filter(|c| *c == '"' || *c == '\'') {
        let mut escaped = false;
        let mut end = None;
        for (i, c) in t.char_indices().skip(1) {
            if q == '"' && c == '\\' && !escaped {
                escaped = true;
                continue;
            }
            if c == q && !escaped {
                // `''` inside single quotes is an escaped quote.
                if q == '\'' && t[i + 1..].starts_with('\'') {
                    escaped = true;
                    continue;
                }
                end = Some(i);
                break;
            }
            escaped = false;
        }
        if let Some(e) = end {
            t = &t[..=e];
        }
    }
    if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        let inner = &t[1..t.len() - 1];
        let mut out = String::new();
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some(o) => out.push(o),
                    None => {}
                }
            } else {
                out.push(c);
            }
        }
        out
    } else if t.len() >= 2 && t.starts_with('\'') && t.ends_with('\'') {
        t[1..t.len() - 1].replace("''", "'")
    } else {
        // A trailing comment on a plain scalar.
        match t.find(" #") {
            Some(i) => t[..i].trim_end().to_owned(),
            None => t.to_owned(),
        }
    }
}

/// Splits `SKILL.md` into its front matter and body. Understands the YAML a skill needs:
/// `key: value` (plain or quoted), folded (`>`) and literal (`|`) blocks, plain values that
/// continue on indented lines, comments, and nested maps (skipped).
pub fn parse_skill_md(text: &str) -> (FrontMatter, String) {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = text.lines();
    let first = lines.next().unwrap_or("");
    if first.trim_end() != "---" {
        return (FrontMatter::default(), text.to_owned());
    }
    let mut yaml: Vec<&str> = Vec::new();
    let mut closed = false;
    for line in lines.by_ref() {
        if line.trim_end() == "---" || line.trim_end() == "..." {
            closed = true;
            break;
        }
        yaml.push(line);
    }
    if !closed {
        return (FrontMatter::default(), text.to_owned());
    }
    let body: Vec<&str> = lines.collect();
    let body = body.join("\n").trim_start_matches(['\n', '\r']).to_owned();

    let mut values: BTreeMap<String, String> = BTreeMap::new();
    let mut i = 0;
    while i < yaml.len() {
        let line = yaml[i];
        i += 1;
        if line.trim().is_empty() || line.trim_start().starts_with('#') || line.starts_with(' ') {
            continue;
        }
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().to_owned();
        let rest = rest.trim();
        // Indented lines that belong to this key.
        let mut block: Vec<&str> = Vec::new();
        while i < yaml.len()
            && (yaml[i].starts_with(' ') || yaml[i].starts_with('\t') || yaml[i].trim().is_empty())
        {
            block.push(yaml[i]);
            i += 1;
        }
        while block.last().is_some_and(|l| l.trim().is_empty()) {
            block.pop();
        }
        let value = if rest.starts_with('|') || rest.starts_with('>') {
            let indent = block
                .iter()
                .filter(|l| !l.trim().is_empty())
                .map(|l| l.len() - l.trim_start().len())
                .min()
                .unwrap_or(0);
            let lines: Vec<String> = block
                .iter()
                .map(|l| l.get(indent..).unwrap_or("").trim_end().to_owned())
                .collect();
            if rest.starts_with('|') {
                lines.join("\n")
            } else {
                // Folded: lines join with spaces, blank lines become line breaks.
                let mut out = String::new();
                for l in lines {
                    if l.is_empty() {
                        out.push('\n');
                    } else {
                        if !out.is_empty() && !out.ends_with('\n') {
                            out.push(' ');
                        }
                        out.push_str(&l);
                    }
                }
                out
            }
        } else if rest.is_empty() {
            // A nested map or list (e.g. `metadata:`): not a text value.
            if block
                .iter()
                .any(|l| l.trim_start().contains(':') || l.trim_start().starts_with('-'))
            {
                continue;
            }
            block.iter().map(|l| l.trim()).collect::<Vec<_>>().join(" ")
        } else {
            let mut v = rest.to_owned();
            for l in &block {
                v.push(' ');
                v.push_str(l.trim());
            }
            scalar(&v)
        };
        values.insert(key, value.trim().to_owned());
    }
    let get = |k: &str| values.get(k).filter(|v| !v.is_empty()).cloned();
    (
        FrontMatter {
            name: get("name"),
            description: get("description"),
            license: get("license"),
        },
        body,
    )
}

/// Quotes a value for front matter when it needs it.
fn yaml_value(s: &str) -> String {
    let s = s.replace(['\r', '\n'], " ");
    let plain = !s.is_empty()
        && !s.starts_with([
            '"', '\'', '>', '|', '-', '?', ':', '[', ']', '{', '}', '#', '&', '*', '!', '%', '@',
            '`', ' ',
        ])
        && !s.contains(": ")
        && !s.contains(" #")
        && !s.ends_with(':')
        && !s.ends_with(' ');
    if plain {
        s
    } else {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

/// Writes a `SKILL.md`.
pub fn skill_md(name: &str, description: &str, license: Option<&str>, body: &str) -> String {
    let mut s = format!(
        "---\nname: {}\ndescription: {}\n",
        yaml_value(name),
        yaml_value(description)
    );
    if let Some(l) = license.filter(|l| !l.trim().is_empty()) {
        s.push_str(&format!("license: {}\n", yaml_value(l)));
    }
    s.push_str("---\n\n");
    s.push_str(body.trim());
    s.push('\n');
    s
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct State {
    disabled: Vec<String>,
}

/// Errors are written for users.
pub type SkillResult<T> = Result<T, String>;

fn ms(t: std::io::Result<std::time::SystemTime>) -> i64 {
    t.ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// Files under `dir` (relative, `/`-separated), skipping hidden ones and symlinks.
fn walk(dir: &Path) -> Vec<(String, u64)> {
    let mut out = Vec::new();
    let mut stack = vec![(dir.to_path_buf(), String::new())];
    while let Some((d, prefix)) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        let mut entries: Vec<_> = rd.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let Ok(ft) = e.file_type() else { continue };
            let rel = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            if ft.is_dir() {
                stack.push((e.path(), rel));
            } else if ft.is_file() {
                out.push((rel, e.metadata().map(|m| m.len()).unwrap_or(0)));
            }
        }
        if out.len() > MAX_FILES * 2 {
            break;
        }
    }
    out.sort();
    out
}

/// The skills folder.
#[derive(Debug, Clone)]
pub struct Skills {
    root: PathBuf,
}

impl Skills {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn state(&self) -> State {
        std::fs::read_to_string(self.root.join(STATE_FILE))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    fn save_state(&self, state: &State) -> SkillResult<()> {
        std::fs::create_dir_all(&self.root)
            .map_err(|e| format!("couldn't create the skills folder: {e}"))?;
        let text = serde_json::to_string_pretty(state).unwrap_or_else(|_| "{}".into());
        std::fs::write(self.root.join(STATE_FILE), text)
            .map_err(|e| format!("couldn't save the skill settings: {e}"))
    }

    fn read_skill(&self, folder: &str, disabled: &[String]) -> Skill {
        let dir = self.root.join(folder);
        let file = dir.join(SKILL_FILE);
        let meta = std::fs::metadata(&file);
        let files = walk(&dir)
            .into_iter()
            .filter(|(p, _)| p != SKILL_FILE)
            .count();
        let mut skill = Skill {
            name: folder.to_owned(),
            description: String::new(),
            folder: folder.to_owned(),
            path: dir.display().to_string(),
            enabled: !disabled.iter().any(|d| d == folder),
            license: None,
            files: u32::try_from(files).unwrap_or(u32::MAX),
            problem: None,
            updated_at: meta.as_ref().map(|m| ms(m.modified())).unwrap_or_default(),
        };
        let text = match std::fs::read_to_string(&file) {
            Ok(t) => t,
            Err(_) => {
                skill.problem = Some(format!("The folder has no {SKILL_FILE}."));
                return skill;
            }
        };
        let (front, _) = parse_skill_md(&text);
        if let Some(n) = front.name {
            skill.name = n.chars().take(MAX_SKILL_NAME).collect();
        }
        skill.license = front.license;
        match front.description {
            Some(d) => skill.description = d.chars().take(MAX_SKILL_DESCRIPTION).collect(),
            None => {
                skill.problem = Some(format!(
                    "{SKILL_FILE} has no description in its front matter, so models wouldn't \
                     know when to use it."
                ));
            }
        }
        skill
    }

    /// Every folder in the skills folder, by name.
    pub fn list(&self) -> Vec<Skill> {
        let disabled = self.state().disabled;
        let Ok(rd) = std::fs::read_dir(&self.root) else {
            return Vec::new();
        };
        let mut out: Vec<Skill> = rd
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| !n.starts_with('.'))
            .map(|n| self.read_skill(&n, &disabled))
            .collect();
        out.sort_by_key(|s| s.name.to_lowercase());
        out
    }

    /// Skills a model may use: enabled and readable.
    pub fn usable(&self) -> Vec<Skill> {
        self.list()
            .into_iter()
            .filter(|s| s.enabled && s.problem.is_none())
            .collect()
    }

    fn folder_of(&self, folder: &str) -> SkillResult<PathBuf> {
        let rel = archive::safe_path(folder)
            .ok()
            .flatten()
            .filter(|p| p.components().count() == 1)
            .ok_or_else(|| format!("there's no skill called \"{folder}\""))?;
        let dir = self.root.join(rel);
        if dir.is_dir() {
            Ok(dir)
        } else {
            Err(format!("there's no skill called \"{folder}\""))
        }
    }

    pub fn detail(&self, folder: &str) -> SkillResult<SkillDetail> {
        let dir = self.folder_of(folder)?;
        let skill = self.read_skill(folder, &self.state().disabled);
        let text = std::fs::read_to_string(dir.join(SKILL_FILE)).unwrap_or_default();
        let (_, body) = parse_skill_md(&text);
        Ok(SkillDetail {
            skill,
            body,
            file_list: walk(&dir)
                .into_iter()
                .map(|(p, _)| p)
                .filter(|p| p != SKILL_FILE)
                .collect(),
        })
    }

    fn check_input(input: &SkillInput) -> SkillResult<(String, String)> {
        let name = input.name.trim();
        let slug = skill_slug(name);
        if slug.is_empty() {
            return Err("give the skill a name (letters or digits)".into());
        }
        let description = input.description.trim().replace(['\r', '\n'], " ");
        if description.is_empty() {
            return Err(
                "describe what the skill does and when to use it (models decide from this)".into(),
            );
        }
        if description.chars().count() > MAX_SKILL_DESCRIPTION {
            return Err(format!(
                "the description can be at most {MAX_SKILL_DESCRIPTION} characters"
            ));
        }
        if input.body.trim().is_empty() {
            return Err("write the skill's instructions".into());
        }
        Ok((slug, description))
    }

    /// Writes a new skill.
    pub fn create(&self, input: &SkillInput) -> SkillResult<Skill> {
        let (slug, description) = Self::check_input(input)?;
        let dir = self.root.join(&slug);
        if dir.exists() {
            return Err(format!("there's already a skill called \"{slug}\""));
        }
        std::fs::create_dir_all(&dir).map_err(|e| format!("couldn't create the skill: {e}"))?;
        std::fs::write(
            dir.join(SKILL_FILE),
            skill_md(&slug, &description, None, &input.body),
        )
        .map_err(|e| format!("couldn't write {SKILL_FILE}: {e}"))?;
        Ok(self.read_skill(&slug, &self.state().disabled))
    }

    /// Rewrites a skill's `SKILL.md` (its other files stay). The folder keeps its name.
    pub fn update(&self, folder: &str, input: &SkillInput) -> SkillResult<Skill> {
        let dir = self.folder_of(folder)?;
        let (slug, description) = Self::check_input(input)?;
        let old = std::fs::read_to_string(dir.join(SKILL_FILE)).unwrap_or_default();
        let (front, _) = parse_skill_md(&old);
        std::fs::write(
            dir.join(SKILL_FILE),
            skill_md(&slug, &description, front.license.as_deref(), &input.body),
        )
        .map_err(|e| format!("couldn't write {SKILL_FILE}: {e}"))?;
        Ok(self.read_skill(folder, &self.state().disabled))
    }

    pub fn set_enabled(&self, folder: &str, enabled: bool) -> SkillResult<()> {
        self.folder_of(folder)?;
        let mut state = self.state();
        state.disabled.retain(|d| d != folder);
        if !enabled {
            state.disabled.push(folder.to_owned());
        }
        self.save_state(&state)
    }

    pub fn remove(&self, folder: &str) -> SkillResult<()> {
        let dir = self.folder_of(folder)?;
        std::fs::remove_dir_all(&dir).map_err(|e| format!("couldn't remove the skill: {e}"))?;
        let mut state = self.state();
        if state.disabled.iter().any(|d| d == folder) {
            state.disabled.retain(|d| d != folder);
            self.save_state(&state)?;
        }
        Ok(())
    }

    /// The folder in `src` that has the `SKILL.md`: `src` itself, or its only subfolder
    /// (zips often wrap the skill in a folder).
    fn skill_dir_in(src: &Path) -> SkillResult<PathBuf> {
        if src.join(SKILL_FILE).is_file() {
            return Ok(src.to_path_buf());
        }
        let subs: Vec<PathBuf> = std::fs::read_dir(src)
            .map_err(|e| format!("couldn't read the folder: {e}"))?
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .filter(|e| !e.file_name().to_string_lossy().starts_with(['.', '_']))
            .map(|e| e.path())
            .collect();
        match subs.as_slice() {
            [one] if one.join(SKILL_FILE).is_file() => Ok(one.clone()),
            _ => Err(format!(
                "there's no {SKILL_FILE} in it (a skill is a folder with a {SKILL_FILE} at its top)"
            )),
        }
    }

    /// Copies a skill folder (or a folder holding one) into the skills folder.
    pub fn add_folder(&self, src: &Path) -> SkillResult<Skill> {
        let dir = Self::skill_dir_in(src)?;
        let files = walk(&dir);
        if files.len() > MAX_FILES {
            return Err(format!("the skill has more than {MAX_FILES} files"));
        }
        if files.iter().map(|(_, n)| n).sum::<u64>() > MAX_BYTES {
            return Err("the skill's files are larger than 50 MB".into());
        }
        let text = std::fs::read_to_string(dir.join(SKILL_FILE))
            .map_err(|e| format!("couldn't read {SKILL_FILE}: {e}"))?;
        let (front, _) = parse_skill_md(&text);
        if front.description.is_none() {
            return Err(format!(
                "{SKILL_FILE} needs a description in its front matter (---, name: …, \
                 description: …, ---)"
            ));
        }
        let folder_name = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let slug = front
            .name
            .as_deref()
            .map(skill_slug)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| skill_slug(&folder_name));
        if slug.is_empty() {
            return Err("the skill needs a name".into());
        }
        let target = self.root.join(&slug);
        if target.exists() {
            return Err(format!(
                "there's already a skill called \"{slug}\"; remove it first to replace it"
            ));
        }
        for (rel, _) in &files {
            let from = dir.join(rel);
            let to = target.join(rel);
            if let Some(p) = to.parent() {
                std::fs::create_dir_all(p).map_err(|e| format!("couldn't copy the skill: {e}"))?;
            }
            std::fs::copy(&from, &to).map_err(|e| format!("couldn't copy {rel}: {e}"))?;
        }
        Ok(self.read_skill(&slug, &self.state().disabled))
    }

    /// Unpacks a zipped skill (`.zip` or `.skill`) into the skills folder.
    pub fn add_archive(&self, file: &Path) -> SkillResult<Skill> {
        let staging = self.root.join(".staging").join(format!(
            "{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let result = archive::extract_zip(
            file,
            &staging,
            ZipLimits {
                max_bytes: MAX_BYTES,
                max_entries: MAX_FILES,
            },
        )
        .map_err(|e| e.to_string())
        .and_then(|_| self.add_folder(&staging));
        let _ = std::fs::remove_dir_all(&staging);
        result
    }

    /// Finds a usable skill by name or folder (case doesn't matter).
    fn find(&self, name: &str) -> Option<Skill> {
        let want = name.trim().to_lowercase();
        let skills = self.usable();
        skills
            .iter()
            .find(|s| s.name.to_lowercase() == want)
            .or_else(|| skills.iter().find(|s| s.folder.to_lowercase() == want))
            .or_else(|| {
                skills
                    .iter()
                    .find(|s| skill_slug(&s.name) == skill_slug(&want))
            })
            .cloned()
    }

    /// What `load_skill` returns: the instructions with a list of the skill's files, or one
    /// of its files.
    pub fn load(&self, name: &str, path: Option<&str>) -> SkillResult<String> {
        let Some(skill) = self.find(name) else {
            let names: Vec<String> = self.usable().into_iter().map(|s| s.name).collect();
            return Err(if names.is_empty() {
                "No skills are installed.".into()
            } else {
                format!(
                    "There's no skill called \"{name}\". Available: {}.",
                    names.join(", ")
                )
            });
        };
        let dir = self.root.join(&skill.folder);
        match path
            .map(str::trim)
            .filter(|p| !p.is_empty() && *p != SKILL_FILE)
        {
            None => {
                let text = std::fs::read_to_string(dir.join(SKILL_FILE))
                    .map_err(|e| format!("couldn't read the skill: {e}"))?;
                let (_, body) = parse_skill_md(&text);
                let mut out = format!(
                    "<skill name=\"{}\">\n{}\n</skill>",
                    skill.name,
                    truncate(body.trim(), MAX_LOAD_CHARS)
                );
                let files: Vec<(String, u64)> = walk(&dir)
                    .into_iter()
                    .filter(|(p, _)| p != SKILL_FILE)
                    .collect();
                if !files.is_empty() {
                    out.push_str(
                        "\n\nFiles in this skill (read one with load_skill and its path; Oxide \
                         can't run scripts yet, so use them as reference):\n",
                    );
                    for (p, n) in files.iter().take(MAX_LISTED) {
                        out.push_str(&format!("- {p} ({})\n", size_text(*n)));
                    }
                    if files.len() > MAX_LISTED {
                        out.push_str(&format!("- … and {} more\n", files.len() - MAX_LISTED));
                    }
                }
                Ok(out)
            }
            Some(p) => {
                let rel = archive::safe_path(p)
                    .ok()
                    .flatten()
                    .ok_or_else(|| format!("\"{p}\" isn't a path inside the skill"))?;
                let file = dir.join(&rel);
                // Symlinks could point outside the skill.
                let meta = std::fs::symlink_metadata(&file)
                    .map_err(|_| format!("The skill has no file \"{p}\"."))?;
                if !meta.is_file() {
                    return Err(format!("\"{p}\" isn't a file in the skill."));
                }
                let bytes = std::fs::read(&file).map_err(|e| format!("couldn't read {p}: {e}"))?;
                let text = String::from_utf8(bytes)
                    .map_err(|_| format!("\"{p}\" isn't a text file, so it can't be shown."))?;
                Ok(format!(
                    "<file skill=\"{}\" path=\"{}\">\n{}\n</file>",
                    skill.name,
                    p,
                    truncate(&text, MAX_LOAD_CHARS)
                ))
            }
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let cut: String = s.chars().take(max).collect();
    format!("{cut}\n[… cut at {max} characters]")
}

fn size_text(n: u64) -> String {
    if n < 1024 {
        format!("{n} B")
    } else if n < 1024 * 1024 {
        format!("{:.0} KB", n as f64 / 1024.0)
    } else {
        format!("{:.1} MB", n as f64 / (1024.0 * 1024.0))
    }
}

/// The `load_skill` tool as offered to models.
pub fn load_skill_entry() -> ToolEntry {
    ToolEntry {
        api_name: LOAD_SKILL.into(),
        kind: ToolKind::Builtin(Builtin::LoadSkill),
        title: Some("Load skill".into()),
        description: "Load one of the user's skills: instructions for a particular kind of task. \
                      Call it with the skill's name before starting a task that matches the \
                      skill's description, then follow what it says. Pass `path` to read one of \
                      the files the skill lists."
            .into(),
        schema: json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "The skill's name, as listed." },
                "path": { "type": "string",
                          "description": "Optional: a file inside the skill to read, e.g. reference.md." }
            },
            "required": ["name"]
        }),
        read_only: true,
        destructive: false,
    }
}

/// The system prompt's list of skills: `- name: description` lines.
pub fn skills_prompt(skills: &[(String, String)]) -> Option<String> {
    if skills.is_empty() {
        return None;
    }
    Some(
        skills
            .iter()
            .map(|(n, d)| format!("- {n}: {d}"))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_front_matter() {
        let (f, body) = parse_skill_md(
            "---\nname: brand-voice\ndescription: \"Writes in our voice: warm, short\" # c\n\
             license: MIT\nmetadata:\n  version: 1\n  tags: [a]\n---\n\n# Voice\nBe kind.\n",
        );
        assert_eq!(f.name.as_deref(), Some("brand-voice"));
        assert_eq!(
            f.description.as_deref(),
            Some("Writes in our voice: warm, short")
        );
        assert_eq!(f.license.as_deref(), Some("MIT"));
        assert_eq!(body, "# Voice\nBe kind.");

        let (f, _) = parse_skill_md(
            "---\r\nname: 'it''s'\r\ndescription: >\r\n  Folded text\r\n  over lines.\r\n\r\n  New para.\r\n---\r\nBody",
        );
        assert_eq!(f.name.as_deref(), Some("it's"));
        assert_eq!(
            f.description.as_deref(),
            Some("Folded text over lines.\nNew para.")
        );

        let (f, _) = parse_skill_md("---\ndescription: |\n  line one\n  line two\nname: x\n---\n");
        assert_eq!(f.description.as_deref(), Some("line one\nline two"));
        assert_eq!(f.name.as_deref(), Some("x"));

        let (f, _) = parse_skill_md("---\ndescription: Plain that\n  continues here\n---\n");
        assert_eq!(f.description.as_deref(), Some("Plain that continues here"));

        // No front matter, or an unclosed one: all body.
        let (f, body) = parse_skill_md("# Just text");
        assert_eq!(f, FrontMatter::default());
        assert_eq!(body, "# Just text");
        assert_eq!(parse_skill_md("---\nname: x\n").0, FrontMatter::default());
    }

    #[test]
    fn writes_front_matter_that_reads_back() {
        for d in [
            "Plain",
            "Has: a colon",
            "\"quoted\" and #hash",
            "- dash first",
            "ends with:",
        ] {
            let md = skill_md("n", d, Some("MIT"), "Do it.");
            let (f, body) = parse_skill_md(&md);
            assert_eq!(f.description.as_deref(), Some(d), "{md}");
            assert_eq!(f.license.as_deref(), Some("MIT"));
            assert_eq!(body.trim(), "Do it.");
        }
    }

    fn input(name: &str) -> SkillInput {
        SkillInput {
            name: name.into(),
            description: "Use for haiku requests".into(),
            body: "Write 5-7-5.\nSee forms.md.".into(),
        }
    }

    #[test]
    fn create_list_toggle_load_remove() {
        let dir = tempfile::tempdir().expect("tmp");
        let skills = Skills::new(dir.path().join("skills"));
        assert!(skills.list().is_empty());
        let s = skills.create(&input("Haiku Writer")).expect("creates");
        assert_eq!(s.folder, "haiku-writer");
        assert_eq!(s.name, "haiku-writer");
        assert!(s.enabled && s.problem.is_none());
        assert!(skills.create(&input("haiku writer")).is_err(), "taken");
        assert!(
            skills
                .create(&SkillInput {
                    description: " ".into(),
                    ..input("x")
                })
                .is_err()
        );

        std::fs::write(
            dir.path().join("skills/haiku-writer/forms.md"),
            "Tanka is 5-7-5-7-7.",
        )
        .expect("write");
        std::fs::create_dir_all(dir.path().join("skills/broken")).expect("mkdir");

        let list = skills.list();
        assert_eq!(list.len(), 2);
        let broken = list.iter().find(|s| s.folder == "broken").expect("broken");
        assert!(broken.problem.is_some());
        assert_eq!(skills.usable().len(), 1);

        let text = skills.load("Haiku-Writer", None).expect("loads");
        assert!(text.contains("<skill name=\"haiku-writer\">"), "{text}");
        assert!(text.contains("Write 5-7-5."));
        assert!(text.contains("- forms.md ("), "{text}");
        let file = skills.load("haiku-writer", Some("forms.md")).expect("file");
        assert!(file.contains("Tanka"));
        assert!(skills.load("haiku-writer", Some("../x")).is_err());
        assert!(skills.load("haiku-writer", Some("nope.md")).is_err());
        let err = skills.load("sonnet", None).expect_err("unknown");
        assert!(err.contains("Available: haiku-writer"), "{err}");

        let d = skills.detail("haiku-writer").expect("detail");
        assert_eq!(d.file_list, vec!["forms.md"]);
        assert!(d.body.starts_with("Write 5-7-5."));

        skills.set_enabled("haiku-writer", false).expect("off");
        assert!(
            !skills
                .list()
                .iter()
                .any(|s| s.enabled && s.folder == "haiku-writer")
        );
        assert!(skills.usable().is_empty());
        assert!(skills.load("haiku-writer", None).is_err());
        skills.set_enabled("haiku-writer", true).expect("on");

        let updated = skills
            .update(
                "haiku-writer",
                &SkillInput {
                    body: "New rules.".into(),
                    ..input("haiku-writer")
                },
            )
            .expect("updates");
        assert_eq!(updated.files, 1, "other files stay");
        assert!(
            skills
                .detail("haiku-writer")
                .expect("d")
                .body
                .contains("New rules.")
        );

        skills.remove("haiku-writer").expect("removes");
        assert!(skills.remove("haiku-writer").is_err());
        assert!(skills.folder_of("../skills").is_err());
    }

    #[test]
    fn adds_folders_and_zips() {
        let dir = tempfile::tempdir().expect("tmp");
        let skills = Skills::new(dir.path().join("skills"));
        // A folder that wraps the skill.
        let src = dir.path().join("download").join("PDF Tools");
        std::fs::create_dir_all(src.join("scripts")).expect("mkdir");
        std::fs::write(
            src.join(SKILL_FILE),
            "---\nname: pdf-tools\ndescription: Work with PDFs\n---\nUse scripts/fill.py.",
        )
        .expect("write");
        std::fs::write(src.join("scripts/fill.py"), "print('hi')").expect("write");
        let s = skills
            .add_folder(&dir.path().join("download"))
            .expect("adds");
        assert_eq!(s.folder, "pdf-tools");
        assert_eq!(s.files, 1);
        assert!(skills.add_folder(&src).is_err(), "already there");

        let zip = archive::write_zip(&[
            ("brand/", b""),
            (
                "brand/SKILL.md",
                b"---\nname: Brand Guide\ndescription: Our colours and tone\n---\nTeal and warm.",
            ),
            ("brand/colors.md", b"#0f766e"),
        ]);
        let file = dir.path().join("brand.skill");
        std::fs::write(&file, zip).expect("write");
        let s = skills.add_archive(&file).expect("unzips");
        assert_eq!(s.folder, "brand-guide");
        assert_eq!(s.name, "Brand Guide");
        assert!(
            skills
                .load("brand guide", Some("colors.md"))
                .expect("loads")
                .contains("0f766e")
        );
        assert!(
            !dir.path()
                .join("skills/.staging")
                .read_dir()
                .is_ok_and(|mut d| d.next().is_some())
        );

        let nodesc = archive::write_zip(&[("SKILL.md", b"---\nname: x\n---\nbody")]);
        std::fs::write(&file, nodesc).expect("write");
        let err = skills.add_archive(&file).expect_err("no description");
        assert!(err.contains("description"), "{err}");
        assert_eq!(
            skills.list().len(),
            2,
            "hidden staging folders aren't skills"
        );
    }

    /// The sample in `docs/samples` (what the Phase 10 exit check adds).
    #[test]
    fn the_sample_skill_loads() {
        let dir = tempfile::tempdir().expect("tmp");
        let skills = Skills::new(dir.path().to_path_buf());
        let sample =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/samples/release-notes.skill");
        let s = skills.add_archive(&sample).expect("adds the sample");
        assert_eq!(s.name, "release-notes");
        assert!(
            s.description.starts_with("Writes release notes"),
            "{}",
            s.description
        );
        let text = skills.load("release-notes", None).expect("loads");
        assert!(text.contains("Brewed in Oxide."), "{text}");
        assert!(text.contains("- template.md ("), "{text}");
        assert!(
            skills
                .load("release-notes", Some("template.md"))
                .expect("file")
                .contains("### ✨ New")
        );
    }

    #[test]
    fn prompt_lines() {
        assert_eq!(skills_prompt(&[]), None);
        assert_eq!(
            skills_prompt(&[("a".into(), "does A".into()), ("b".into(), "does B".into())])
                .as_deref(),
            Some("- a: does A\n- b: does B")
        );
    }
}
