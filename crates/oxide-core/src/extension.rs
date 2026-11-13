//! Desktop extensions (`.mcpb` bundles) and skills, as shown in Settings.
//!
//! A desktop extension is a zip with a `manifest.json` (the public MCPB manifest format) and
//! an MCP server inside. Oxide unpacks it into its data folder, asks for the settings the
//! manifest declares (`user_config`) and runs the server like any other connector.
//!
//! A skill is a folder with a `SKILL.md` (name and description in YAML front matter, then
//! instructions). Models see each skill's name and description and load the rest with the
//! built-in `load_skill` tool when a task calls for it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::Timestamp;

/// How an extension's server runs (`server.type` in the manifest).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ExtensionServerType {
    /// JavaScript with its `node_modules` bundled; runs with the system's Node.js.
    Node,
    /// Python with its packages bundled; runs through uv.
    Python,
    /// A self-contained program.
    Binary,
    /// Python with a `pyproject.toml`; uv installs the dependencies.
    Uv,
}

/// The kind of value a setting holds (`user_config.<key>.type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ExtensionFieldType {
    String,
    Number,
    Boolean,
    Directory,
    File,
}

/// A setting value. Folders and files with `multiple` take a list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(untagged)]
pub enum ExtensionValue {
    Bool(bool),
    Number(f64),
    List(Vec<String>),
    Text(String),
}

impl ExtensionValue {
    /// Whether the value counts as "not filled in".
    pub fn is_empty(&self) -> bool {
        match self {
            ExtensionValue::Text(s) => s.trim().is_empty(),
            ExtensionValue::List(l) => l.iter().all(|s| s.trim().is_empty()),
            ExtensionValue::Bool(_) | ExtensionValue::Number(_) => false,
        }
    }

    /// The values it puts on a command line (a list gives one per entry).
    pub fn parts(&self) -> Vec<String> {
        match self {
            ExtensionValue::Bool(b) => vec![b.to_string()],
            ExtensionValue::Number(n) => vec![format_number(*n)],
            ExtensionValue::List(l) => l.iter().filter(|s| !s.trim().is_empty()).cloned().collect(),
            ExtensionValue::Text(s) => vec![s.clone()],
        }
    }
}

/// `3` rather than `3.0` for whole numbers.
pub fn format_number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        n.to_string()
    }
}

/// One setting an extension asks for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionField {
    pub key: String,
    #[serde(rename = "type")]
    pub kind: ExtensionFieldType,
    pub title: String,
    pub description: Option<String>,
    pub required: bool,
    /// The manifest's default, with `${HOME}` and friends already filled in.
    pub default: Option<ExtensionValue>,
    /// Folders and files: several may be picked.
    pub multiple: bool,
    /// Strings: masked, and kept in the OS keychain.
    pub sensitive: bool,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

/// A tool or prompt the manifest lists (servers may offer more at run time).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionItem {
    pub name: String,
    pub description: Option<String>,
}

/// What an extension's manifest says about it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionInfo {
    /// Machine name (`name`), also the connector's name in Oxide.
    pub name: String,
    pub display_name: String,
    pub version: String,
    pub description: String,
    pub long_description: Option<String>,
    pub author: String,
    pub author_url: Option<String>,
    pub homepage: Option<String>,
    pub repository: Option<String>,
    pub license: Option<String>,
    pub manifest_version: String,
    pub server_type: ExtensionServerType,
    pub tools: Vec<ExtensionItem>,
    pub prompts: Vec<ExtensionItem>,
    /// The server makes more tools than it lists.
    pub tools_generated: bool,
    pub keywords: Vec<String>,
    /// `darwin`, `win32`, `linux`; empty: every platform.
    pub platforms: Vec<String>,
    /// Version needs, e.g. `node: ">=18"`.
    pub runtimes: BTreeMap<String, String>,
    pub fields: Vec<ExtensionField>,
    /// The icon as a `data:` URL (PNG, small), if it has one.
    pub icon: Option<String>,
}

/// Whether an installed extension can run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ExtensionStatus {
    /// Configured; its server runs while it's enabled.
    Ready,
    /// A required setting is missing.
    NeedsSetup,
    /// It's for another operating system.
    Unsupported,
    /// Its files or manifest can't be read.
    Broken,
}

/// An installed extension.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Extension {
    /// `None` when the manifest can't be read (status `broken`).
    pub info: Option<ExtensionInfo>,
    pub name: String,
    pub version: String,
    pub enabled: bool,
    pub status: ExtensionStatus,
    /// Why it can't run, written for users.
    pub problem: Option<String>,
    /// Where it's unpacked.
    pub dir: String,
    /// Settings that aren't sensitive.
    pub values: BTreeMap<String, ExtensionValue>,
    /// Sensitive settings that have a value in the keychain (the values never leave it).
    pub secrets_set: Vec<String>,
    pub installed_at: Timestamp,
}

/// A `.mcpb` that was opened and checked, waiting for the user to install it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionPreview {
    /// Pass to `install_extension`.
    pub staging_id: String,
    pub info: ExtensionInfo,
    /// Set when this replaces an installed version.
    pub installed_version: Option<String>,
    /// Things the user should know (other OS, Node.js missing, …).
    pub warnings: Vec<String>,
    /// Starting values for the form: the installed extension's, else the defaults.
    pub values: BTreeMap<String, ExtensionValue>,
    /// Sensitive settings the installed version already has.
    pub secrets_set: Vec<String>,
    /// Unpacked size in bytes and number of files.
    pub size: u64,
    pub files: u32,
}

/// A skill in Oxide's skills folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    /// From the front matter (falls back to the folder name).
    pub name: String,
    pub description: String,
    /// The folder's name inside the skills folder.
    pub folder: String,
    pub path: String,
    pub enabled: bool,
    pub license: Option<String>,
    /// Files besides `SKILL.md`.
    pub files: u32,
    /// Why it can't be used (no `SKILL.md`, no description, …).
    pub problem: Option<String>,
    pub updated_at: Timestamp,
}

/// A skill with its instructions, for viewing and editing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SkillDetail {
    pub skill: Skill,
    /// `SKILL.md` after the front matter.
    pub body: String,
    /// Other files, as paths relative to the skill's folder.
    pub file_list: Vec<String>,
}

/// A skill written in Settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SkillInput {
    pub name: String,
    pub description: String,
    pub body: String,
}

/// The built-in tool that gives a model a skill's instructions and files.
pub const LOAD_SKILL: &str = "load_skill";
/// The built-in tool that lets a model create or change a skill (always asks first).
pub const SAVE_SKILL: &str = "save_skill";

/// What a `save_skill` call would write, shown on its approval card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SkillSavePreview {
    /// The skill's name as it will be saved.
    pub name: String,
    /// Its folder inside the skills folder.
    pub folder: String,
    /// A new skill (else a change to an existing one).
    pub is_new: bool,
    /// The whole `SKILL.md` as it will be written.
    pub skill_md: String,
    /// For a change: what changes in `SKILL.md`, as a unified diff (`None` when unchanged).
    pub diff: Option<String>,
    /// Other files the save writes.
    pub files: Vec<SkillFileChange>,
    /// Something the user should know first (the skill was added from a folder or zip, it's
    /// switched off, …).
    pub notes: Vec<String>,
}

/// One extra file a `save_skill` call writes inside the skill's folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SkillFileChange {
    /// Relative to the skill's folder, `/`-separated.
    pub path: String,
    pub bytes: u32,
    pub is_new: bool,
    /// The new content (cut for very long files).
    pub content: String,
    /// For a changed file: a unified diff against the current one.
    pub diff: Option<String>,
}
/// Longest skill name.
pub const MAX_SKILL_NAME: usize = 64;
/// Longest skill description.
pub const MAX_SKILL_DESCRIPTION: usize = 1024;

/// A skill name as Oxide stores it: lowercase letters, digits and hyphens.
pub fn skill_slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out: String = out.trim_matches('-').chars().take(MAX_SKILL_NAME).collect();
    out.trim_end_matches('-').to_owned()
}

/// `sys.platform`-style name of an OS (`std::env::consts::OS`).
pub fn platform_name(os: &str) -> &str {
    match os {
        "windows" => "win32",
        "macos" => "darwin",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_and_parts() {
        assert!(ExtensionValue::Text("  ".into()).is_empty());
        assert!(!ExtensionValue::Bool(false).is_empty());
        assert!(ExtensionValue::List(vec![" ".into()]).is_empty());
        assert_eq!(ExtensionValue::Number(3.0).parts(), vec!["3"]);
        assert_eq!(ExtensionValue::Number(2.5).parts(), vec!["2.5"]);
        assert_eq!(
            ExtensionValue::List(vec!["a".into(), "".into(), "b".into()]).parts(),
            vec!["a", "b"]
        );
    }

    #[test]
    fn values_travel_untagged() {
        let v: BTreeMap<String, ExtensionValue> =
            serde_json::from_str(r#"{"a": true, "b": 2, "c": ["x"], "d": "y"}"#).expect("json");
        assert_eq!(v["a"], ExtensionValue::Bool(true));
        assert_eq!(v["b"], ExtensionValue::Number(2.0));
        assert_eq!(v["c"], ExtensionValue::List(vec!["x".into()]));
        assert_eq!(v["d"], ExtensionValue::Text("y".into()));
    }

    #[test]
    fn slugs() {
        assert_eq!(skill_slug("Brand Guidelines!"), "brand-guidelines");
        assert_eq!(skill_slug("  --pdf__tools-- "), "pdf-tools");
        assert_eq!(skill_slug("Æøå"), "");
        assert_eq!(skill_slug(&"a".repeat(100)).len(), MAX_SKILL_NAME);
        assert_eq!(platform_name("windows"), "win32");
    }
}
