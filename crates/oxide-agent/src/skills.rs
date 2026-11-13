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
use oxide_core::{
    LOAD_SKILL, SAVE_SKILL, Skill, SkillDetail, SkillFileChange, SkillInput, SkillSavePreview,
};
use oxide_mcp::archive::{self, ZipLimits};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

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
/// Most extra files one `save_skill` call may write.
pub const MAX_SAVE_FILES: usize = 20;
/// Largest extra file `save_skill` writes.
pub const MAX_SAVE_FILE_BYTES: usize = 256 * 1024;
/// Largest `SKILL.md` `save_skill` writes.
pub const MAX_SAVE_SKILL_MD: usize = 200_000;
/// Skills listed in `save_skill`'s description, at most.
const MAX_SAVE_LISTED: usize = 100;
/// Characters of a file's content shown on the approval card.
const MAX_PREVIEW_CHARS: usize = 20_000;

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
    /// Where each skill came from (folder name → `settings`, `chat`, `folder`, `zip`).
    /// Skills made before Phase 14c have no entry.
    origins: BTreeMap<String, String>,
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

    /// Notes where a skill came from (for the approval card of a later change).
    fn set_origin(&self, folder: &str, origin: &str) {
        let mut state = self.state();
        state.origins.insert(folder.to_owned(), origin.to_owned());
        if let Err(e) = self.save_state(&state) {
            tracing::warn!("couldn't note where a skill came from: {e}");
        }
    }

    /// Writes a new skill (from Settings).
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
        self.set_origin(&slug, "settings");
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
        if state.disabled.iter().any(|d| d == folder) || state.origins.contains_key(folder) {
            state.disabled.retain(|d| d != folder);
            state.origins.remove(folder);
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
        let skill = self.copy_in(src)?;
        self.set_origin(&skill.folder, "folder");
        Ok(skill)
    }

    fn copy_in(&self, src: &Path) -> SkillResult<Skill> {
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
        .and_then(|_| self.copy_in(&staging));
        let _ = std::fs::remove_dir_all(&staging);
        if let Ok(skill) = &result {
            self.set_origin(&skill.folder, "zip");
        }
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

    /// Any skill (on, off or broken) by name or folder (case doesn't matter).
    fn find_any(&self, name: &str) -> Option<Skill> {
        let want = name.trim().to_lowercase();
        let skills = self.list();
        skills
            .iter()
            .find(|s| s.folder.to_lowercase() == want)
            .or_else(|| skills.iter().find(|s| s.name.to_lowercase() == want))
            .or_else(|| {
                let slug = skill_slug(&want);
                skills
                    .iter()
                    .find(|s| !slug.is_empty() && skill_slug(&s.name) == slug)
            })
            .cloned()
    }

    /// Checks a `save_skill` call and works out exactly what it would write. Nothing is
    /// written; [`Skills::apply_save`] writes the plan once the user allows it.
    pub fn plan_save(&self, input: &Value) -> SkillResult<SavePlan> {
        let text = |k: &str| {
            input
                .get(k)
                .and_then(Value::as_str)
                .map(str::to_owned)
                .filter(|v| !v.trim().is_empty())
        };
        let target = text("skill");
        let existing = match &target {
            Some(t) => Some(self.find_any(t).ok_or_else(|| {
                let names: Vec<String> = self.list().into_iter().map(|s| s.folder).collect();
                if names.is_empty() {
                    format!(
                        "There's no skill called \"{t}\" (there are no skills yet). Leave out \
                         `skill` to create a new one."
                    )
                } else {
                    format!(
                        "There's no skill called \"{t}\". Existing skills: {}. Leave out \
                         `skill` to create a new one.",
                        names.join(", ")
                    )
                }
            })?),
            None => None,
        };
        let (old_text, old_front, old_body) = match &existing {
            Some(s) => {
                let t = std::fs::read_to_string(self.root.join(&s.folder).join(SKILL_FILE))
                    .unwrap_or_default();
                let (f, b) = parse_skill_md(&t);
                (Some(t), f, b)
            }
            None => (None, FrontMatter::default(), String::new()),
        };
        let name = text("name").or_else(|| existing.as_ref().map(|s| s.name.clone()));
        let description = text("description").or_else(|| old_front.description.clone());
        let body = text("instructions")
            .or_else(|| text("body"))
            .or_else(|| existing.as_ref().map(|_| old_body.clone()));
        let missing: Vec<&str> = [
            ("name", name.is_none()),
            ("description", description.is_none()),
            ("instructions", body.is_none()),
        ]
        .iter()
        .filter(|(_, m)| *m)
        .map(|(k, _)| *k)
        .collect();
        if !missing.is_empty() {
            return Err(format!(
                "save_skill needs {} for a new skill.",
                missing.join(", ")
            ));
        }
        let skill_input = SkillInput {
            name: name.unwrap_or_default(),
            description: description.unwrap_or_default(),
            body: body.unwrap_or_default(),
        };
        let (slug, description) = Self::check_input(&skill_input)?;
        // Names stay unique: no other skill may have this one.
        let others: Vec<Skill> = self
            .list()
            .into_iter()
            .filter(|s| existing.as_ref().is_none_or(|e| e.folder != s.folder))
            .collect();
        if let Some(other) = others
            .iter()
            .find(|s| s.folder == slug || skill_slug(&s.name) == slug)
        {
            return Err(if existing.is_some() {
                format!(
                    "Another skill is already called \"{}\". Pick another name.",
                    other.name
                )
            } else {
                format!(
                    "There's already a skill called \"{}\" (folder {}). To change it, call \
                     save_skill again with \"skill\": \"{}\" (and load_skill it first to see \
                     what it says); to make a separate skill, pick another name.",
                    other.name, other.folder, other.folder
                )
            });
        }
        let folder = existing
            .as_ref()
            .map(|s| s.folder.clone())
            .unwrap_or_else(|| slug.clone());
        let dir = self.root.join(&folder);
        if existing.is_none() && dir.exists() {
            return Err(format!(
                "The skills folder already has a folder called \"{folder}\". Pick another name."
            ));
        }
        let skill_md_text = skill_md(
            &slug,
            &description,
            old_front.license.as_deref(),
            &skill_input.body,
        );
        if skill_md_text.len() > MAX_SAVE_SKILL_MD {
            return Err(format!(
                "{SKILL_FILE} would be {} characters; keep it under {MAX_SAVE_SKILL_MD} and put \
                 long reference material in extra files.",
                skill_md_text.len()
            ));
        }
        let files = Self::plan_files(input.get("files"), &dir)?;
        let current: u64 = walk(&dir).iter().map(|(_, n)| n).sum();
        let added: u64 = files.iter().map(|f| f.content.len() as u64).sum();
        if current + added + skill_md_text.len() as u64 > MAX_BYTES {
            return Err("the skill's files would be larger than 50 MB".into());
        }
        if walk(&dir).len() + files.len() > MAX_FILES {
            return Err(format!("the skill would have more than {MAX_FILES} files"));
        }

        let mut notes = Vec::new();
        if let Some(s) = &existing {
            let state = self.state();
            match state.origins.get(&s.folder).map(String::as_str) {
                Some("folder") => notes.push(
                    "You added this skill from a folder. Saving changes Oxide's copy in the \
                     skills folder; the folder you added it from stays as it is."
                        .to_owned(),
                ),
                Some("zip") => notes.push(
                    "You added this skill from a .zip or .skill file. Saving changes Oxide's \
                     copy in the skills folder; the file you added it from stays as it is."
                        .to_owned(),
                ),
                Some(_) => {}
                None if s.license.is_some() || s.files > 0 => notes.push(format!(
                    "This skill may have been added from a folder or zip{}. Saving changes \
                     Oxide's copy in the skills folder.",
                    s.license
                        .as_deref()
                        .map(|l| format!(" (its license: {l})"))
                        .unwrap_or_default()
                )),
                None => {}
            }
            if !s.enabled {
                notes.push(
                    "This skill is switched off in Settings → Skills; it stays off.".to_owned(),
                );
            }
            if s.name != slug {
                notes.push(format!(
                    "The skill is renamed from \"{}\" to \"{slug}\".",
                    s.name
                ));
            }
        }
        let diff = old_text
            .as_deref()
            .and_then(|old| unified_diff(old, &skill_md_text, SKILL_FILE));
        let preview = SkillSavePreview {
            name: slug.clone(),
            folder: folder.clone(),
            is_new: existing.is_none(),
            skill_md: skill_md_text.clone(),
            diff,
            files: files
                .iter()
                .map(|f| SkillFileChange {
                    path: f.path.clone(),
                    bytes: u32::try_from(f.content.len()).unwrap_or(u32::MAX),
                    is_new: f.old.is_none(),
                    content: truncate(&f.content, MAX_PREVIEW_CHARS),
                    diff: f
                        .old
                        .as_deref()
                        .and_then(|old| unified_diff(old, &f.content, &f.path)),
                })
                .collect(),
            notes,
        };
        Ok(SavePlan {
            preview,
            folder,
            is_new: existing.is_none(),
            skill_md: skill_md_text,
            files: files.into_iter().map(|f| (f.path, f.content)).collect(),
        })
    }

    /// The extra files of a `save_skill` call, checked: inside the skill's folder, text, not
    /// too many or too large.
    fn plan_files(files: Option<&Value>, dir: &Path) -> SkillResult<Vec<PlannedFile>> {
        let list = match files {
            None | Some(Value::Null) => return Ok(Vec::new()),
            Some(Value::Array(a)) => a,
            Some(_) => {
                return Err(
                    "`files` must be a list of {\"path\": …, \"content\": …} objects.".into(),
                );
            }
        };
        if list.len() > MAX_SAVE_FILES {
            return Err(format!(
                "save_skill writes at most {MAX_SAVE_FILES} files at a time."
            ));
        }
        let mut out: Vec<PlannedFile> = Vec::new();
        for f in list {
            let path = f
                .get("path")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .ok_or("each file needs a \"path\" (relative to the skill's folder)")?;
            let content = f
                .get("content")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("the file \"{path}\" needs a \"content\" string"))?;
            let rel = check_skill_path(path)?;
            let shown = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            if shown.eq_ignore_ascii_case(SKILL_FILE) {
                return Err(format!(
                    "{SKILL_FILE} is written from `name`, `description` and `instructions`; \
                     don't pass it as a file."
                ));
            }
            if content.len() > MAX_SAVE_FILE_BYTES {
                return Err(format!(
                    "\"{shown}\" is {} KB; files a skill saves can be at most {} KB.",
                    content.len() / 1024,
                    MAX_SAVE_FILE_BYTES / 1024
                ));
            }
            if out.iter().any(|o| o.path.eq_ignore_ascii_case(&shown)) {
                return Err(format!("\"{shown}\" is listed twice."));
            }
            // Nothing on the way may be a link (it could lead out of the skill's folder) or
            // a file where a folder is needed.
            let mut at = dir.to_path_buf();
            let parts: Vec<_> = rel.components().collect();
            let mut old = None;
            for (i, c) in parts.iter().enumerate() {
                at.push(c);
                let last = i + 1 == parts.len();
                match std::fs::symlink_metadata(&at) {
                    Err(_) => break,
                    Ok(m) if m.file_type().is_symlink() => {
                        return Err(format!(
                            "\"{shown}\" goes through a link; it can't be written."
                        ));
                    }
                    Ok(m) if last && !m.is_file() => {
                        return Err(format!("\"{shown}\" is a folder in the skill."));
                    }
                    Ok(m) if !last && !m.is_dir() => {
                        return Err(format!("\"{shown}\" is inside a file, not a folder."));
                    }
                    Ok(_) if last => {
                        old = Some(std::fs::read_to_string(&at).map_err(|_| {
                            format!(
                                "\"{shown}\" exists and isn't a text file; it can't be replaced."
                            )
                        })?);
                    }
                    Ok(_) => {}
                }
            }
            out.push(PlannedFile {
                path: shown,
                content: content.to_owned(),
                old,
            });
        }
        Ok(out)
    }

    /// Writes what [`Skills::plan_save`] worked out (after the user allowed it).
    pub fn apply_save(&self, plan: &SavePlan) -> SkillResult<Skill> {
        let dir = self.root.join(&plan.folder);
        if plan.is_new {
            if dir.exists() {
                return Err(format!(
                    "a skill called \"{}\" appeared in the meantime; nothing was saved",
                    plan.folder
                ));
            }
        } else if !dir.is_dir() {
            return Err(format!(
                "the skill \"{}\" was removed in the meantime; nothing was saved",
                plan.folder
            ));
        }
        let write = || -> SkillResult<()> {
            std::fs::create_dir_all(&dir).map_err(|e| format!("couldn't create the skill: {e}"))?;
            for (path, content) in &plan.files {
                let rel = check_skill_path(path)?;
                let to = dir.join(&rel);
                if let Some(p) = to.parent() {
                    std::fs::create_dir_all(p)
                        .map_err(|e| format!("couldn't create the folder for {path}: {e}"))?;
                }
                // Checked again: a link could have appeared since the card was shown.
                let mut at = dir.clone();
                for c in rel.components() {
                    at.push(c);
                    if std::fs::symlink_metadata(&at).is_ok_and(|m| m.file_type().is_symlink()) {
                        return Err(format!(
                            "\"{path}\" goes through a link; it can't be written"
                        ));
                    }
                }
                std::fs::write(&to, content).map_err(|e| format!("couldn't write {path}: {e}"))?;
            }
            std::fs::write(dir.join(SKILL_FILE), &plan.skill_md)
                .map_err(|e| format!("couldn't write {SKILL_FILE}: {e}"))
        };
        if let Err(e) = write() {
            if plan.is_new {
                let _ = std::fs::remove_dir_all(&dir);
            }
            return Err(e);
        }
        if plan.is_new {
            self.set_origin(&plan.folder, "chat");
        }
        Ok(self.read_skill(&plan.folder, &self.state().disabled))
    }
}

/// What a `save_skill` call writes, worked out before the user is asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavePlan {
    /// What the approval card shows.
    pub preview: SkillSavePreview,
    folder: String,
    is_new: bool,
    skill_md: String,
    /// Extra files: relative path and content.
    files: Vec<(String, String)>,
}

impl SavePlan {
    pub fn is_new(&self) -> bool {
        self.is_new
    }
}

struct PlannedFile {
    path: String,
    content: String,
    /// The current content, when the file exists.
    old: Option<String>,
}

/// A path inside a skill's folder: relative, no `..`, no hidden parts.
fn check_skill_path(path: &str) -> SkillResult<PathBuf> {
    let p = path.replace('\\', "/");
    if p.starts_with('/') || p.contains(':') {
        return Err(format!(
            "\"{path}\" must be a path inside the skill's folder, like reference.md or \
             scripts/tool.py"
        ));
    }
    let rel = archive::safe_path(&p)
        .ok()
        .flatten()
        .ok_or_else(|| format!("\"{path}\" isn't a path inside the skill's folder"))?;
    if rel
        .components()
        .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
    {
        return Err(format!(
            "\"{path}\": hidden files and folders can't be saved in a skill"
        ));
    }
    if rel.components().count() > 8 {
        return Err(format!("\"{path}\" is nested too deeply"));
    }
    Ok(rel)
}

/// A unified diff of two texts (3 lines of context), or `None` when they're the same.
pub fn unified_diff(old: &str, new: &str, name: &str) -> Option<String> {
    if old == new {
        return None;
    }
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    // (tag, old index, new index): ' ' same, '-' removed, '+' added.
    let mut ops: Vec<(char, usize, usize)> = Vec::new();
    if a.len().saturating_mul(b.len()) > 4_000_000 {
        ops.extend((0..a.len()).map(|i| ('-', i, 0)));
        ops.extend((0..b.len()).map(|j| ('+', a.len(), j)));
    } else {
        // Longest common subsequence, from the end.
        let (n, m) = (a.len(), b.len());
        let mut l = vec![0u32; (n + 1) * (m + 1)];
        let at = |i: usize, j: usize| i * (m + 1) + j;
        for i in (0..n).rev() {
            for j in (0..m).rev() {
                l[at(i, j)] = if a[i] == b[j] {
                    l[at(i + 1, j + 1)] + 1
                } else {
                    l[at(i + 1, j)].max(l[at(i, j + 1)])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < n || j < m {
            if i < n && j < m && a[i] == b[j] {
                ops.push((' ', i, j));
                i += 1;
                j += 1;
            } else if i < n && (j == m || l[at(i + 1, j)] >= l[at(i, j + 1)]) {
                ops.push(('-', i, j));
                i += 1;
            } else {
                ops.push(('+', i, j));
                j += 1;
            }
        }
    }
    const CONTEXT: usize = 3;
    let changed: Vec<usize> = ops
        .iter()
        .enumerate()
        .filter(|(_, o)| o.0 != ' ')
        .map(|(k, _)| k)
        .collect();
    if changed.is_empty() {
        // Only line endings differ.
        return Some(format!(
            "--- {name}\n+++ {name}\n(only line endings change)\n"
        ));
    }
    let mut out = format!("--- {name}\n+++ {name}\n");
    let mut k = 0;
    while k < changed.len() {
        let start = changed[k].saturating_sub(CONTEXT);
        let mut end = (changed[k] + CONTEXT + 1).min(ops.len());
        while k + 1 < changed.len() && changed[k + 1] <= end + CONTEXT {
            k += 1;
            end = (changed[k] + CONTEXT + 1).min(ops.len());
        }
        k += 1;
        let hunk = &ops[start..end];
        let old_start = hunk
            .iter()
            .find(|o| o.0 != '+')
            .map(|o| o.1)
            .unwrap_or(hunk[0].1);
        let new_start = hunk
            .iter()
            .find(|o| o.0 != '-')
            .map(|o| o.2)
            .unwrap_or(hunk[0].2);
        let old_len = hunk.iter().filter(|o| o.0 != '+').count();
        let new_len = hunk.iter().filter(|o| o.0 != '-').count();
        out.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            if old_len == 0 {
                old_start
            } else {
                old_start + 1
            },
            old_len,
            if new_len == 0 {
                new_start
            } else {
                new_start + 1
            },
            new_len
        ));
        for o in hunk {
            let line = if o.0 == '+' { b[o.2] } else { a[o.1] };
            out.push(o.0);
            out.push_str(line);
            out.push('\n');
        }
    }
    Some(out)
}

/// The `save_skill` tool as offered to models, listing the skills there are.
pub fn save_skill_entry(existing: &[Skill]) -> ToolEntry {
    let mut description = String::from(
        "Create a new skill for the user, or change one of their skills. A skill is reusable \
         instructions for a kind of task (a SKILL.md with a name, a description that says when \
         to use it, and instructions), plus optional text files in its folder. The user always \
         sees the whole skill (and, for a change, what changes) and decides whether it's saved. \
         Only call this when the user asked for a skill or agreed to one. To change a skill, \
         pass its folder as `skill` (load_skill it first) and only the fields that change; \
         never make a second skill for the same thing.",
    );
    if existing.is_empty() {
        description.push_str("\n\nThe user has no skills yet.");
    } else {
        description.push_str("\n\nThe user's skills (folder: description):");
        for s in existing.iter().take(MAX_SAVE_LISTED) {
            let d: String = s.description.chars().take(160).collect();
            description.push_str(&format!(
                "\n- {}{}: {}",
                s.folder,
                if s.enabled { "" } else { " (switched off)" },
                d
            ));
        }
        if existing.len() > MAX_SAVE_LISTED {
            description.push_str(&format!(
                "\n- … and {} more",
                existing.len() - MAX_SAVE_LISTED
            ));
        }
    }
    ToolEntry {
        api_name: SAVE_SKILL.into(),
        kind: ToolKind::Builtin(Builtin::SaveSkill),
        title: Some("Save skill".into()),
        description,
        schema: json!({
            "type": "object",
            "properties": {
                "skill": { "type": "string",
                           "description": "To change an existing skill: its folder (as listed). Leave out to create a new skill." },
                "name": { "type": "string",
                          "description": "Short name: lowercase words joined by hyphens, e.g. weekly-report. Required for a new skill." },
                "description": { "type": "string",
                                 "description": "What the skill does and when to use it, in one or two sentences (models decide from this). Required for a new skill." },
                "instructions": { "type": "string",
                                  "description": "The instructions (Markdown, the body of SKILL.md without front matter): the steps, what to watch out for, examples. Required for a new skill; for a change, the whole new text." },
                "files": {
                    "type": "array",
                    "description": "Optional extra text files in the skill's folder, e.g. reference.md or scripts/tool.py. A file with the same path is replaced.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "path": { "type": "string", "description": "Relative to the skill's folder." },
                            "content": { "type": "string" }
                        },
                        "required": ["path", "content"]
                    }
                }
            }
        }),
        read_only: false,
        destructive: false,
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

    fn plan(skills: &Skills, v: Value) -> SkillResult<SavePlan> {
        skills.plan_save(&v)
    }

    #[test]
    fn save_creates_with_files_inside_the_folder_only() {
        let dir = tempfile::tempdir().expect("tmp");
        let skills = Skills::new(dir.path().join("skills"));
        let p = plan(
            &skills,
            json!({
                "name": "PDF Tools", "description": "Use for PDFs", "instructions": "Do it.",
                "files": [{ "path": "scripts\\fill.py", "content": "print(1)" }]
            }),
        )
        .expect("plans");
        assert!(p.is_new());
        assert_eq!(p.preview.folder, "pdf-tools");
        assert_eq!(p.preview.files[0].path, "scripts/fill.py");
        assert!(
            !dir.path().join("skills").exists(),
            "planning writes nothing"
        );
        let s = skills.apply_save(&p).expect("saves");
        assert_eq!(s.name, "pdf-tools");
        assert_eq!(s.files, 1);
        assert!(
            dir.path()
                .join("skills/pdf-tools/scripts/fill.py")
                .is_file()
        );
        assert_eq!(
            skills.state().origins.get("pdf-tools").map(String::as_str),
            Some("chat")
        );

        for (path, why) in [
            ("../escape.md", "inside"),
            ("/etc/passwd", "inside"),
            ("C:\\Windows\\x.txt", "inside"),
            ("a/../../x.md", "inside"),
            (".env", "hidden"),
            ("notes/.secret", "hidden"),
            ("SKILL.md", "SKILL.md"),
            ("skill.md", "SKILL.md"),
        ] {
            let err = plan(
                &skills,
                json!({ "skill": "pdf-tools", "files": [{ "path": path, "content": "x" }] }),
            )
            .expect_err(path);
            assert!(err.contains(why), "{path}: {err}");
        }
        let big = "x".repeat(MAX_SAVE_FILE_BYTES + 1);
        let err = plan(
            &skills,
            json!({ "skill": "pdf-tools", "files": [{ "path": "big.md", "content": big }] }),
        )
        .expect_err("too big");
        assert!(err.contains("at most"), "{err}");
        let many: Vec<Value> = (0..=MAX_SAVE_FILES)
            .map(|i| json!({ "path": format!("f{i}.md"), "content": "x" }))
            .collect();
        assert!(plan(&skills, json!({ "skill": "pdf-tools", "files": many })).is_err());
        let twice = json!({ "skill": "pdf-tools", "files": [
            { "path": "a.md", "content": "1" }, { "path": "A.md", "content": "2" }] });
        assert!(plan(&skills, twice).expect_err("twice").contains("twice"));
    }

    #[cfg(unix)]
    #[test]
    fn save_refuses_links_out_of_the_folder() {
        let dir = tempfile::tempdir().expect("tmp");
        let outside = tempfile::tempdir().expect("outside");
        let skills = Skills::new(dir.path().to_path_buf());
        skills
            .create(&SkillInput {
                name: "s".into(),
                description: "d".into(),
                body: "b".into(),
            })
            .expect("creates");
        std::os::unix::fs::symlink(outside.path(), dir.path().join("s/out")).expect("link");
        let err = plan(
            &skills,
            json!({ "skill": "s", "files": [{ "path": "out/x.md", "content": "x" }] }),
        )
        .expect_err("link");
        assert!(err.contains("link"), "{err}");
        assert!(!outside.path().join("x.md").exists());
    }

    #[test]
    fn save_edits_keep_what_is_not_passed_and_names_stay_unique() {
        let dir = tempfile::tempdir().expect("tmp");
        let skills = Skills::new(dir.path().to_path_buf());
        for n in ["alpha", "beta"] {
            skills
                .create(&SkillInput {
                    name: n.into(),
                    description: format!("Use for {n}"),
                    body: format!("Line one of {n}.\nLine two."),
                })
                .expect("creates");
        }
        // A new skill with a taken name points to the existing one.
        let err = plan(
            &skills,
            json!({ "name": "Alpha", "description": "x", "instructions": "y" }),
        )
        .expect_err("taken");
        assert!(err.contains("\"skill\": \"alpha\""), "{err}");
        // Renaming onto another skill's name is refused.
        let err = plan(&skills, json!({ "skill": "alpha", "name": "beta" })).expect_err("rename");
        assert!(err.contains("Another skill"), "{err}");
        // Unknown skill: the existing ones are named.
        let err = plan(&skills, json!({ "skill": "gamma", "name": "x" })).expect_err("unknown");
        assert!(err.contains("alpha, beta"), "{err}");
        // A new skill needs everything.
        let err = plan(&skills, json!({ "name": "gamma" })).expect_err("missing");
        assert!(err.contains("description, instructions"), "{err}");

        // Only the description changes; the instructions stay.
        let p = plan(
            &skills,
            json!({ "skill": "Alpha", "description": "Use for A things" }),
        )
        .expect("plans");
        assert!(!p.is_new());
        let diff = p.preview.diff.clone().expect("diff");
        assert!(diff.contains("-description: Use for alpha"), "{diff}");
        assert!(diff.contains("+description: Use for A things"), "{diff}");
        assert!(
            diff.starts_with("--- SKILL.md\n+++ SKILL.md\n@@ -"),
            "{diff}"
        );
        skills.apply_save(&p).expect("saves");
        let d = skills.detail("alpha").expect("detail");
        assert_eq!(d.skill.description, "Use for A things");
        assert!(d.body.contains("Line one of alpha."));
        assert_eq!(skills.list().len(), 2);

        // A changed extra file shows its own diff.
        let p = plan(
            &skills,
            json!({ "skill": "alpha", "files": [{ "path": "ref.md", "content": "one\n" }] }),
        )
        .expect("plans");
        assert!(p.preview.diff.is_none(), "SKILL.md unchanged");
        skills.apply_save(&p).expect("saves");
        let p = plan(
            &skills,
            json!({ "skill": "alpha", "files": [{ "path": "ref.md", "content": "two\n" }] }),
        )
        .expect("plans");
        let f = &p.preview.files[0];
        assert!(!f.is_new);
        assert_eq!(
            f.diff.as_deref(),
            Some("--- ref.md\n+++ ref.md\n@@ -1,1 +1,1 @@\n-one\n+two\n")
        );
    }

    #[test]
    fn the_card_says_when_a_skill_came_from_a_folder_or_is_off() {
        let dir = tempfile::tempdir().expect("tmp");
        let src = tempfile::tempdir().expect("src");
        std::fs::create_dir_all(src.path().join("brand")).expect("dir");
        std::fs::write(
            src.path().join("brand/SKILL.md"),
            "---\nname: brand\ndescription: Our voice\nlicense: MIT\n---\nBe warm.\n",
        )
        .expect("writes");
        let skills = Skills::new(dir.path().to_path_buf());
        skills.add_folder(&src.path().join("brand")).expect("adds");
        skills.set_enabled("brand", false).expect("off");
        let p = plan(
            &skills,
            json!({ "skill": "brand", "instructions": "Be warm and short." }),
        )
        .expect("plans");
        let notes = p.preview.notes.join(" ");
        assert!(notes.contains("added this skill from a folder"), "{notes}");
        assert!(notes.contains("switched off"), "{notes}");
        assert!(
            p.preview.skill_md.contains("license: MIT"),
            "the license stays"
        );
        // Removing forgets where it came from.
        skills.remove("brand").expect("removes");
        assert!(skills.state().origins.is_empty());
    }

    #[test]
    fn diffs() {
        assert_eq!(unified_diff("a\nb\n", "a\nb\n", "x"), None);
        let d = unified_diff(
            "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n",
            "1\n2\n3\n4\nfive\n6\n7\n8\n9\n10\n",
            "f",
        )
        .expect("diff");
        assert_eq!(
            d,
            "--- f\n+++ f\n@@ -2,7 +2,7 @@\n 2\n 3\n 4\n-5\n+five\n 6\n 7\n 8\n"
        );
        let d = unified_diff("", "new\n", "f").expect("diff");
        assert_eq!(d, "--- f\n+++ f\n@@ -0,0 +1,1 @@\n+new\n");
        // Two changes far apart make two hunks.
        let old: String = (1..=30).map(|i| format!("line {i}\n")).collect();
        let new = old
            .replace("line 2\n", "line two\n")
            .replace("line 29\n", "line twenty-nine\n");
        let d = unified_diff(&old, &new, "f").expect("diff");
        assert_eq!(d.matches("@@ -").count(), 2, "{d}");
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
