//! Reading and checking an MCPB `manifest.json`, and turning it plus the user's settings into
//! the command line that starts the extension's server.
//!
//! Follows the public MCPB manifest spec (manifest versions 0.1–0.4, and the older
//! `dxt_version` key): `server.type` node / python / binary / uv, `mcp_config` with
//! `platform_overrides`, `user_config` fields, and the `${…}` variables.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use oxide_core::{
    ExtensionField, ExtensionFieldType, ExtensionInfo, ExtensionItem, ExtensionServerType,
    ExtensionValue,
};
use serde_json::{Map, Value};

use crate::McpError;

/// Manifest versions Oxide knows. Newer ones load with a warning.
pub const KNOWN_VERSIONS: &[&str] = &["0.1", "0.2", "0.3", "0.4"];
/// Longest extension name Oxide accepts.
const MAX_NAME: usize = 64;

/// The command block of a manifest, before variables are filled in.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CommandSpec {
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub env: BTreeMap<String, String>,
}

/// A checked manifest.
#[derive(Debug, Clone, PartialEq)]
pub struct Manifest {
    pub info: ExtensionInfo,
    pub entry_point: String,
    pub mcp_config: Option<CommandSpec>,
    /// Keyed by platform (`win32`, `darwin`, `linux`).
    pub overrides: BTreeMap<String, CommandSpec>,
    /// Path of the icon inside the bundle.
    pub icon_path: Option<String>,
    /// Things worth telling the user that don't stop the install.
    pub warnings: Vec<String>,
}

fn bad(msg: impl Into<String>) -> McpError {
    McpError::Extension(msg.into())
}

fn text(v: Option<&Value>) -> Option<String> {
    v.and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| match x {
                    Value::String(s) => Some(s.clone()),
                    Value::Number(n) => Some(n.to_string()),
                    Value::Bool(b) => Some(b.to_string()),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

fn command_spec(v: &Value) -> CommandSpec {
    let env = v
        .get("env")
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .map(|(k, v)| {
                    let v = match v {
                        Value::String(s) => s.clone(),
                        Value::Null => String::new(),
                        other => other.to_string(),
                    };
                    (k.clone(), v)
                })
                .collect()
        })
        .unwrap_or_default();
    CommandSpec {
        command: text(v.get("command")),
        args: v
            .get("args")
            .filter(|a| a.is_array())
            .map(|a| strings(Some(a))),
        env,
    }
}

/// Whether a name can be a folder and a connector name as it is.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().count() <= MAX_NAME
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && !name.starts_with('.')
}

/// The folder and connector name for a manifest `name` (`@org/tool` → `org-tool`).
pub fn folder_name(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out: String = out
        .trim_matches(|c| c == '-' || c == '.')
        .chars()
        .take(MAX_NAME)
        .collect();
    out.trim_end_matches(['-', '.']).to_owned()
}

/// Folders `${HOME}` and friends stand for.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UserDirs {
    pub home: PathBuf,
    pub desktop: PathBuf,
    pub documents: PathBuf,
    pub downloads: PathBuf,
}

impl UserDirs {
    pub fn from_home(home: PathBuf) -> Self {
        Self {
            desktop: home.join("Desktop"),
            documents: home.join("Documents"),
            downloads: home.join("Downloads"),
            home,
        }
    }

    /// The current user's.
    pub fn current() -> Self {
        Self::from_home(crate::process::home_dir().unwrap_or_default())
    }
}

/// Fills in `${HOME}`, `${DESKTOP}`, `${DOCUMENTS}`, `${DOWNLOADS}` and the path separator.
fn fill_dirs(s: &str, dirs: &UserDirs) -> String {
    let sep = std::path::MAIN_SEPARATOR.to_string();
    s.replace("${HOME}", &dirs.home.to_string_lossy())
        .replace("${DESKTOP}", &dirs.desktop.to_string_lossy())
        .replace("${DOCUMENTS}", &dirs.documents.to_string_lossy())
        .replace("${DOWNLOADS}", &dirs.downloads.to_string_lossy())
        .replace("${pathSeparator}", &sep)
        .replace("${/}", &sep)
}

fn field_default(
    v: Option<&Value>,
    kind: ExtensionFieldType,
    dirs: &UserDirs,
) -> Option<ExtensionValue> {
    let v = v?;
    Some(match (kind, v) {
        (ExtensionFieldType::Boolean, Value::Bool(b)) => ExtensionValue::Bool(*b),
        (ExtensionFieldType::Boolean, Value::String(s)) => ExtensionValue::Bool(s == "true"),
        (ExtensionFieldType::Number, Value::Number(n)) => ExtensionValue::Number(n.as_f64()?),
        (ExtensionFieldType::Number, Value::String(s)) => {
            ExtensionValue::Number(s.trim().parse().ok()?)
        }
        (_, Value::Array(a)) => ExtensionValue::List(
            a.iter()
                .filter_map(Value::as_str)
                .map(|s| fill_dirs(s, dirs))
                .collect(),
        ),
        (_, Value::String(s)) => ExtensionValue::Text(fill_dirs(s, dirs)),
        (_, Value::Number(n)) => ExtensionValue::Text(n.to_string()),
        (_, Value::Bool(b)) => ExtensionValue::Text(b.to_string()),
        _ => return None,
    })
}

fn fields(v: Option<&Value>, dirs: &UserDirs) -> Result<Vec<ExtensionField>, McpError> {
    let Some(map) = v.and_then(Value::as_object) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for (key, f) in map {
        let kind = match f.get("type").and_then(Value::as_str).unwrap_or("string") {
            "string" => ExtensionFieldType::String,
            "number" | "integer" => ExtensionFieldType::Number,
            "boolean" => ExtensionFieldType::Boolean,
            "directory" => ExtensionFieldType::Directory,
            "file" => ExtensionFieldType::File,
            other => {
                return Err(bad(format!(
                    "the setting \"{key}\" has an unknown type \"{other}\""
                )));
            }
        };
        let multiple = f.get("multiple").and_then(Value::as_bool).unwrap_or(false)
            && matches!(
                kind,
                ExtensionFieldType::Directory | ExtensionFieldType::File
            );
        let mut default = field_default(f.get("default"), kind, dirs);
        // A single default for a multi-pick field is a list of one.
        if multiple && let Some(ExtensionValue::Text(s)) = &default {
            default = Some(ExtensionValue::List(vec![s.clone()]));
        }
        out.push(ExtensionField {
            key: key.clone(),
            kind,
            title: text(f.get("title")).unwrap_or_else(|| key.clone()),
            description: text(f.get("description")),
            required: f.get("required").and_then(Value::as_bool).unwrap_or(false),
            default,
            multiple,
            sensitive: f.get("sensitive").and_then(Value::as_bool).unwrap_or(false)
                && kind == ExtensionFieldType::String,
            min: f.get("min").and_then(Value::as_f64),
            max: f.get("max").and_then(Value::as_f64),
        });
    }
    Ok(out)
}

fn items(v: Option<&Value>) -> Vec<ExtensionItem> {
    v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|t| {
                    Some(ExtensionItem {
                        name: text(t.get("name"))?,
                        description: text(t.get("description")),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A URL field that may be a string or `{ "url": … }`.
fn url_of(v: Option<&Value>) -> Option<String> {
    text(v).or_else(|| text(v.and_then(|o| o.get("url"))))
}

/// Parses and checks `manifest.json`.
pub fn parse(json: &str, dirs: &UserDirs) -> Result<Manifest, McpError> {
    let v: Value = serde_json::from_str(json)
        .map_err(|e| bad(format!("manifest.json isn't valid JSON: {e}")))?;
    let Value::Object(m) = v else {
        return Err(bad("manifest.json must contain a JSON object"));
    };
    parse_object(&m, dirs)
}

fn parse_object(m: &Map<String, Value>, dirs: &UserDirs) -> Result<Manifest, McpError> {
    let mut warnings = Vec::new();
    let manifest_version = text(m.get("manifest_version"))
        .or_else(|| text(m.get("dxt_version")))
        .ok_or_else(|| bad("manifest.json has no \"manifest_version\""))?;
    if !KNOWN_VERSIONS.contains(&manifest_version.as_str()) {
        warnings.push(format!(
            "This extension uses manifest version {manifest_version}, which is newer than \
             Oxide knows ({}). It may not work as intended.",
            KNOWN_VERSIONS.join(", ")
        ));
    }
    let raw_name = text(m.get("name")).ok_or_else(|| bad("manifest.json has no \"name\""))?;
    let name = folder_name(&raw_name);
    if !valid_name(&name) {
        return Err(bad(format!(
            "the extension's name \"{raw_name}\" can't be used"
        )));
    }
    let version = text(m.get("version")).ok_or_else(|| bad("manifest.json has no \"version\""))?;
    let description =
        text(m.get("description")).ok_or_else(|| bad("manifest.json has no \"description\""))?;
    let author = m
        .get("author")
        .and_then(|a| text(a.get("name")).or_else(|| text(Some(a))))
        .ok_or_else(|| bad("manifest.json has no \"author\" with a \"name\""))?;
    let server = m
        .get("server")
        .and_then(Value::as_object)
        .ok_or_else(|| bad("manifest.json has no \"server\""))?;
    let server_type = match server.get("type").and_then(Value::as_str) {
        Some("node") => ExtensionServerType::Node,
        Some("python") => ExtensionServerType::Python,
        Some("binary") => ExtensionServerType::Binary,
        Some("uv") => ExtensionServerType::Uv,
        Some(other) => {
            return Err(bad(format!(
                "the server type \"{other}\" isn't supported (node, python, binary or uv)"
            )));
        }
        None => return Err(bad("manifest.json's \"server\" has no \"type\"")),
    };
    let entry_point = text(server.get("entry_point"))
        .ok_or_else(|| bad("manifest.json's \"server\" has no \"entry_point\""))?;
    let mcp_config = server.get("mcp_config").filter(|c| c.is_object());
    if mcp_config.is_none() && server_type != ExtensionServerType::Uv {
        return Err(bad("manifest.json's \"server\" has no \"mcp_config\""));
    }
    let overrides = mcp_config
        .and_then(|c| c.get("platform_overrides"))
        .and_then(Value::as_object)
        .map(|o| {
            o.iter()
                .map(|(k, v)| (k.clone(), command_spec(v)))
                .collect()
        })
        .unwrap_or_default();
    let mcp_config = mcp_config.map(command_spec);
    if let Some(c) = &mcp_config
        && c.command.is_none()
        && server_type != ExtensionServerType::Uv
    {
        return Err(bad("manifest.json's \"mcp_config\" has no \"command\""));
    }

    let compat = m.get("compatibility");
    let platforms = strings(compat.and_then(|c| c.get("platforms")));
    let runtimes: BTreeMap<String, String> = compat
        .and_then(|c| c.get("runtimes"))
        .and_then(Value::as_object)
        .map(|r| {
            r.iter()
                .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_owned())))
                .collect()
        })
        .unwrap_or_default();

    let icon_path = text(m.get("icon"))
        .filter(|i| !i.starts_with("http"))
        .or_else(|| {
            // The `icons` list: the largest light (or theme-less) one.
            m.get("icons").and_then(Value::as_array).and_then(|a| {
                a.iter()
                    .filter(|i| i.get("theme").and_then(Value::as_str) != Some("dark"))
                    .max_by_key(|i| {
                        i.get("size")
                            .and_then(Value::as_str)
                            .and_then(|s| s.split('x').next())
                            .and_then(|n| n.parse::<u32>().ok())
                            .unwrap_or(0)
                    })
                    .and_then(|i| text(i.get("src")))
                    .filter(|s| !s.starts_with("http"))
            })
        });

    let info = ExtensionInfo {
        display_name: text(m.get("display_name")).unwrap_or_else(|| raw_name.clone()),
        name,
        version,
        description,
        long_description: text(m.get("long_description")),
        author,
        author_url: m.get("author").and_then(|a| text(a.get("url"))),
        homepage: text(m.get("homepage")),
        repository: url_of(m.get("repository")),
        license: text(m.get("license")),
        manifest_version,
        server_type,
        tools: items(m.get("tools")),
        prompts: items(m.get("prompts")),
        tools_generated: m
            .get("tools_generated")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        keywords: strings(m.get("keywords")),
        platforms,
        runtimes,
        fields: fields(m.get("user_config"), dirs)?,
        icon: None,
    };
    Ok(Manifest {
        info,
        entry_point,
        mcp_config,
        overrides,
        icon_path,
        warnings,
    })
}

impl Manifest {
    /// Whether the extension says it runs on `platform` (`win32`, `darwin`, `linux`).
    pub fn supports(&self, platform: &str) -> bool {
        self.info.platforms.is_empty() || self.info.platforms.iter().any(|p| p == platform)
    }

    /// Checks the bundle's files against the manifest.
    pub fn check_files(&self, dir: &Path) -> Result<(), McpError> {
        let entry = dir.join(self.entry_point.replace('\\', "/"));
        if self.info.server_type != ExtensionServerType::Binary && !entry.exists() {
            return Err(bad(format!(
                "the bundle doesn't contain its entry point ({})",
                self.entry_point
            )));
        }
        if self.info.server_type == ExtensionServerType::Uv && !dir.join("pyproject.toml").is_file()
        {
            return Err(bad(
                "a uv extension needs a pyproject.toml, and this bundle has none",
            ));
        }
        Ok(())
    }
}

/// A missing required setting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingSetting(pub String);

/// What starts the server, ready for the MCP manager.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    /// The command line with sensitive values hidden, for display.
    pub summary: String,
}

/// How to fill in `${user_config.KEY}` placeholders.
struct Filler<'a> {
    dir: &'a Path,
    dirs: &'a UserDirs,
    values: &'a BTreeMap<String, ExtensionValue>,
}

impl Filler<'_> {
    fn fill(&self, s: &str) -> String {
        let mut out = fill_dirs(s, self.dirs).replace("${__dirname}", &self.dir.to_string_lossy());
        while let Some(start) = out.find("${user_config.") {
            let Some(len) = out[start..].find('}') else {
                break;
            };
            let key = &out[start + "${user_config.".len()..start + len];
            let value = self
                .values
                .get(key)
                .map(|v| v.parts().join(","))
                .unwrap_or_default();
            out.replace_range(start..start + len + 1, &value);
        }
        out
    }

    /// One argument; a lone placeholder of a list becomes one argument per entry, and a lone
    /// placeholder of an unset setting disappears.
    fn fill_arg(&self, arg: &str) -> Vec<String> {
        let t = arg.trim();
        if let Some(key) = t
            .strip_prefix("${user_config.")
            .and_then(|k| k.strip_suffix('}'))
            && !key.contains('}')
        {
            return match self.values.get(key) {
                Some(v) => v.parts(),
                None => Vec::new(),
            };
        }
        vec![self.fill(arg)]
    }
}

/// Builds the command line. `values` has every setting (sensitive ones included); `secret`
/// lists the sensitive keys whose values must not show in the summary. `uv` is the uv
/// program to run Python extensions with, if one was found.
pub fn launch(
    manifest: &Manifest,
    dir: &Path,
    values: &BTreeMap<String, ExtensionValue>,
    platform: &str,
    dirs: &UserDirs,
    uv: Option<&str>,
) -> Result<Launch, MissingSetting> {
    // Settings: what the user gave, else the default.
    let mut effective = BTreeMap::new();
    for f in &manifest.info.fields {
        let v = values
            .get(&f.key)
            .filter(|v| !v.is_empty())
            .cloned()
            .or_else(|| f.default.clone().filter(|v| !v.is_empty()));
        match v {
            Some(v) => {
                effective.insert(f.key.clone(), v);
            }
            None if f.required => return Err(MissingSetting(f.title.clone())),
            None => {}
        }
    }
    let filler = Filler {
        dir,
        dirs,
        values: &effective,
    };

    let base = manifest.mcp_config.clone().unwrap_or_default();
    let over = manifest
        .overrides
        .get(platform)
        .cloned()
        .unwrap_or_default();
    let command = over.command.or(base.command);
    let args = over.args.or(base.args).unwrap_or_default();
    let mut env_spec = base.env;
    env_spec.extend(over.env);

    let (mut command, mut args): (String, Vec<String>) = match command {
        Some(c) => (
            filler.fill(&c),
            args.iter().flat_map(|a| filler.fill_arg(a)).collect(),
        ),
        // uv without an mcp_config: run the entry point in its project.
        None => (
            uv.unwrap_or("uv").to_owned(),
            vec![
                "run".into(),
                "--directory".into(),
                dir.to_string_lossy().into_owned(),
                "--quiet".into(),
                manifest.entry_point.clone(),
            ],
        ),
    };
    let mut env: BTreeMap<String, String> = BTreeMap::new();
    for (k, v) in &env_spec {
        let unset = v
            .trim()
            .strip_prefix("${user_config.")
            .and_then(|k| k.strip_suffix('}'));
        if let Some(key) = unset
            && !effective.contains_key(key)
        {
            continue;
        }
        env.insert(k.clone(), filler.fill(v));
    }

    match manifest.info.server_type {
        // Bundled Python runs through uv when it's there (Oxide's own uv comes later), so
        // the version the manifest asks for can be fetched.
        ExtensionServerType::Python => {
            let base = Path::new(&command)
                .file_stem()
                .map(|s| s.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            if let Some(uv) = uv
                && matches!(base.as_str(), "python" | "python3" | "py")
            {
                let mut wrapped = vec!["run".into(), "--no-project".into(), "--quiet".into()];
                if let Some(spec) = manifest.info.runtimes.get("python") {
                    wrapped.push("--python".into());
                    wrapped.push(spec.clone());
                }
                wrapped.push("python".into());
                wrapped.append(&mut args);
                args = wrapped;
                command = uv.to_owned();
            }
        }
        // A binary named relative to the bundle.
        ExtensionServerType::Binary => {
            let p = Path::new(&command);
            let in_bundle = command.contains('/') || command.contains('\\') || dir.join(p).exists();
            if p.is_relative() && in_bundle {
                command = dir.join(p).to_string_lossy().into_owned();
            }
        }
        ExtensionServerType::Node | ExtensionServerType::Uv => {}
    }

    // The summary hides sensitive values.
    let secrets: Vec<String> = manifest
        .info
        .fields
        .iter()
        .filter(|f| f.sensitive)
        .filter_map(|f| effective.get(&f.key))
        .flat_map(ExtensionValue::parts)
        .filter(|s| s.len() >= 3)
        .collect();
    let mut summary = std::iter::once(command.as_str())
        .chain(args.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" ");
    for s in &secrets {
        summary = summary.replace(s.as_str(), "••••");
    }
    Ok(Launch {
        command,
        args,
        env,
        summary,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirs() -> UserDirs {
        UserDirs::from_home(PathBuf::from("/home/me"))
    }

    const FS: &str = r#"{
      "manifest_version": "0.3",
      "name": "files",
      "display_name": "Files",
      "version": "1.2.0",
      "description": "Reads files",
      "author": {"name": "Ann", "url": "https://ann.dev"},
      "repository": {"type": "git", "url": "https://github.com/ann/files"},
      "server": {
        "type": "node",
        "entry_point": "server/index.js",
        "mcp_config": {
          "command": "node",
          "args": ["${__dirname}/server/index.js", "--root", "${user_config.dirs}", "--depth=${user_config.depth}"],
          "env": {"API_KEY": "${user_config.key}", "MODE": "${user_config.mode}", "FIXED": "1"},
          "platform_overrides": {"win32": {"env": {"FIXED": "win"}}}
        }
      },
      "tools": [{"name": "read", "description": "Read a file"}, {"nope": 1}],
      "compatibility": {"platforms": ["win32", "darwin"], "runtimes": {"node": ">=18"}},
      "user_config": {
        "dirs": {"type": "directory", "title": "Folders", "multiple": true, "required": true, "default": "${HOME}/docs"},
        "depth": {"type": "number", "title": "Depth", "default": 3, "min": 1, "max": 10},
        "key": {"type": "string", "title": "API key", "sensitive": true, "required": true},
        "mode": {"type": "string", "title": "Mode"}
      }
    }"#;

    #[test]
    fn reads_a_manifest() {
        let m = parse(FS, &dirs()).expect("parses");
        assert_eq!(m.info.name, "files");
        assert_eq!(m.info.display_name, "Files");
        assert_eq!(m.info.author, "Ann");
        assert_eq!(
            m.info.repository.as_deref(),
            Some("https://github.com/ann/files")
        );
        assert_eq!(m.info.server_type, ExtensionServerType::Node);
        assert_eq!(m.info.tools.len(), 1);
        assert!(m.supports("win32"));
        assert!(!m.supports("linux"));
        assert_eq!(
            m.info.runtimes.get("node").map(String::as_str),
            Some(">=18")
        );
        let dirs_field = &m.info.fields[1];
        assert_eq!(dirs_field.key, "dirs");
        assert!(dirs_field.multiple);
        assert_eq!(
            dirs_field.default,
            Some(ExtensionValue::List(vec!["/home/me/docs".into()]))
        );
        let key = m.info.fields.iter().find(|f| f.key == "key").expect("key");
        assert!(key.sensitive && key.required);
        assert!(m.warnings.is_empty());
    }

    #[test]
    fn builds_the_command_line() {
        let m = parse(FS, &dirs()).expect("parses");
        let dir = Path::new("/ext/files");
        let mut values = BTreeMap::new();
        // Required key missing → needs setup.
        let err = launch(&m, dir, &values, "linux", &dirs(), None).expect_err("missing");
        assert_eq!(err, MissingSetting("API key".into()));

        values.insert("key".into(), ExtensionValue::Text("sk-secret".into()));
        values.insert(
            "dirs".into(),
            ExtensionValue::List(vec!["/a".into(), "/b c".into()]),
        );
        let l = launch(&m, dir, &values, "win32", &dirs(), None).expect("launch");
        assert_eq!(l.command, "node");
        assert_eq!(
            l.args,
            vec![
                "/ext/files/server/index.js",
                "--root",
                "/a",
                "/b c",
                "--depth=3"
            ]
        );
        assert_eq!(l.env.get("API_KEY").map(String::as_str), Some("sk-secret"));
        assert_eq!(l.env.get("FIXED").map(String::as_str), Some("win"));
        assert!(
            !l.env.contains_key("MODE"),
            "unset optional settings are left out"
        );
        assert!(!l.summary.contains("sk-secret"));

        let linux = launch(&m, dir, &values, "linux", &dirs(), None).expect("launch");
        assert_eq!(linux.env.get("FIXED").map(String::as_str), Some("1"));
    }

    #[test]
    fn python_runs_through_uv_and_uv_needs_no_config() {
        let py = r#"{"manifest_version": "0.2", "name": "py", "version": "1", "description": "d",
            "author": {"name": "a"},
            "server": {"type": "python", "entry_point": "server/main.py",
              "mcp_config": {"command": "python", "args": ["${__dirname}/server/main.py"],
                "env": {"PYTHONPATH": "${__dirname}/server/lib"}}},
            "compatibility": {"runtimes": {"python": ">=3.10"}}}"#;
        let m = parse(py, &dirs()).expect("parses");
        let dir = Path::new("/ext/py");
        let l = launch(&m, dir, &BTreeMap::new(), "linux", &dirs(), Some("/bin/uv")).expect("ok");
        assert_eq!(l.command, "/bin/uv");
        assert_eq!(
            l.args,
            vec![
                "run",
                "--no-project",
                "--quiet",
                "--python",
                ">=3.10",
                "python",
                "/ext/py/server/main.py"
            ]
        );
        let plain = launch(&m, dir, &BTreeMap::new(), "linux", &dirs(), None).expect("ok");
        assert_eq!(plain.command, "python");

        let uv = r#"{"manifest_version": "0.4", "name": "u", "version": "1", "description": "d",
            "author": {"name": "a"}, "server": {"type": "uv", "entry_point": "src/server.py"}}"#;
        let m = parse(uv, &dirs()).expect("parses");
        let l = launch(
            &m,
            Path::new("/ext/u"),
            &BTreeMap::new(),
            "linux",
            &dirs(),
            None,
        )
        .expect("ok");
        assert_eq!(l.command, "uv");
        assert_eq!(
            l.args,
            vec!["run", "--directory", "/ext/u", "--quiet", "src/server.py"]
        );
    }

    #[test]
    fn binaries_resolve_inside_the_bundle() {
        let b = r#"{"dxt_version": "0.1", "name": "@acme/Tool X", "version": "1", "description": "d",
            "author": "Acme",
            "server": {"type": "binary", "entry_point": "bin/tool",
              "mcp_config": {"command": "bin/tool", "args": []}}}"#;
        let m = parse(b, &dirs()).expect("parses");
        assert_eq!(m.info.name, "acme-Tool-X");
        assert_eq!(m.info.display_name, "@acme/Tool X");
        let l = launch(
            &m,
            Path::new("/ext/t"),
            &BTreeMap::new(),
            "linux",
            &dirs(),
            None,
        )
        .expect("ok");
        assert_eq!(Path::new(&l.command), Path::new("/ext/t").join("bin/tool"));
    }

    #[test]
    fn explains_what_is_wrong() {
        let cases = [
            ("[]", "JSON object"),
            ("{", "valid JSON"),
            (r#"{"name": "x"}"#, "manifest_version"),
            (
                r#"{"manifest_version": "0.3", "name": "x", "version": "1", "description": "d", "author": {"name": "a"}}"#,
                "server",
            ),
            (
                r#"{"manifest_version": "0.3", "name": "x", "version": "1", "description": "d", "author": {"name": "a"}, "server": {"type": "java", "entry_point": "a"}}"#,
                "isn't supported",
            ),
            (
                r#"{"manifest_version": "0.3", "name": "x", "version": "1", "description": "d", "author": {"name": "a"}, "server": {"type": "node", "entry_point": "a"}}"#,
                "mcp_config",
            ),
            (
                r#"{"manifest_version": "0.3", "name": "///", "version": "1", "description": "d", "author": {"name": "a"}, "server": {"type": "uv", "entry_point": "a"}}"#,
                "can't be used",
            ),
        ];
        for (json, want) in cases {
            let err = parse(json, &dirs()).expect_err(json);
            assert!(err.to_string().contains(want), "{json}: {err}");
        }
        let newer = r#"{"manifest_version": "0.9", "name": "x", "version": "1", "description": "d",
            "author": {"name": "a"}, "server": {"type": "uv", "entry_point": "a"}}"#;
        assert_eq!(parse(newer, &dirs()).expect("loads").warnings.len(), 1);
    }

    #[test]
    fn checks_bundle_files() {
        let dir = tempfile::tempdir().expect("tmp");
        let uv = r#"{"manifest_version": "0.4", "name": "u", "version": "1", "description": "d",
            "author": {"name": "a"}, "server": {"type": "uv", "entry_point": "main.py"}}"#;
        let m = parse(uv, &dirs()).expect("parses");
        assert!(m.check_files(dir.path()).is_err());
        std::fs::write(dir.path().join("main.py"), "").expect("write");
        assert!(m.check_files(dir.path()).is_err(), "no pyproject");
        std::fs::write(dir.path().join("pyproject.toml"), "").expect("write");
        m.check_files(dir.path()).expect("ok");
    }
}
