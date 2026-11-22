//! Utility Apps (Phase 20): small React + TypeScript web apps that run in a sandboxed frame
//! and reach Oxide's models and tools through the `oxide` SDK.
//!
//! An app is a folder with a manifest ([`APP_MANIFEST`]) and its source files. Built-in apps
//! ship inside Oxide and are read-only; the user's own apps (clones, imports) live under
//! `<app data>/apps/<id>/`. Per-app settings ([`AppPrefs`]) and storage are kept in the
//! database, not in the folder, so an exported app carries no grants, keys or data.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::generation::{GenerationEndpoint, GenerationKind};

/// The manifest's file name inside an app's folder.
pub const APP_MANIFEST: &str = "oxide-app.json";
/// The extension of an exported app (a zip of its folder).
pub const APP_FILE_EXT: &str = "oxideapp";
/// Most files an app may have.
pub const MAX_APP_FILES: usize = 200;
/// Most bytes one app file may have.
pub const MAX_APP_FILE_BYTES: u64 = 2 * 1024 * 1024;
/// Most bytes an app may have in all.
pub const MAX_APP_BYTES: u64 = 20 * 1024 * 1024;
/// Most bytes one app may keep in its storage.
pub const MAX_APP_STORAGE_BYTES: u64 = 10 * 1024 * 1024;

/// Something an app may do that asks the user first (once per app).
///
/// Picking and saving files and the app's own storage don't ask: the user picks every file
/// and every save location, and storage belongs to the app. [`AppCapability::Folder`] is
/// granted when the user picks the app's folder (asked again only after a revoke).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "camelCase")]
pub enum AppCapability {
    /// Text generation with the app's chat model (images and PDFs as input, JSON output).
    Text,
    /// Image generation.
    Image,
    /// Video generation.
    Video,
    /// Music generation.
    Music,
    /// Web search.
    WebSearch,
    /// Reading web pages (fetch and crawl).
    WebFetch,
    /// Tools of the connected MCP servers.
    Connectors,
    /// Python in the managed environment (not sandboxed).
    Python,
    /// Commands in the app's Docker sandbox.
    Sandbox,
    /// The browser agent.
    Browser,
    /// The paired Chrome.
    Chrome,
    /// Reading and writing files inside one folder the user picked for the app
    /// ([`AppPrefs::folder`]).
    Folder,
}

impl AppCapability {
    pub const ALL: [AppCapability; 12] = [
        AppCapability::Text,
        AppCapability::Image,
        AppCapability::Video,
        AppCapability::Music,
        AppCapability::WebSearch,
        AppCapability::WebFetch,
        AppCapability::Connectors,
        AppCapability::Python,
        AppCapability::Sandbox,
        AppCapability::Browser,
        AppCapability::Chrome,
        AppCapability::Folder,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            AppCapability::Text => "text",
            AppCapability::Image => "image",
            AppCapability::Video => "video",
            AppCapability::Music => "music",
            AppCapability::WebSearch => "webSearch",
            AppCapability::WebFetch => "webFetch",
            AppCapability::Connectors => "connectors",
            AppCapability::Python => "python",
            AppCapability::Sandbox => "sandbox",
            AppCapability::Browser => "browser",
            AppCapability::Chrome => "chrome",
            AppCapability::Folder => "folder",
        }
    }

    pub fn parse(s: &str) -> Option<AppCapability> {
        AppCapability::ALL.into_iter().find(|c| c.as_str() == s)
    }

    /// What the approval card says the app wants to do.
    pub fn describe(self) -> &'static str {
        match self {
            AppCapability::Text => "use your AI models to write text (this may cost money)",
            AppCapability::Image => "generate images (this may cost money)",
            AppCapability::Video => "generate videos (this may cost money)",
            AppCapability::Music => "generate music (this may cost money)",
            AppCapability::WebSearch => "search the web",
            AppCapability::WebFetch => "read web pages",
            AppCapability::Connectors => "use the tools of your connectors",
            AppCapability::Python => "run Python code on this computer (not sandboxed)",
            AppCapability::Sandbox => "run commands in its Docker sandbox",
            AppCapability::Browser => "control the browser agent",
            AppCapability::Chrome => "control your paired Chrome",
            AppCapability::Folder => "read and write files in the folder you chose for it",
        }
    }

    /// The generation kind this capability stands for.
    pub fn generation(self) -> Option<GenerationKind> {
        match self {
            AppCapability::Image => Some(GenerationKind::Image),
            AppCapability::Video => Some(GenerationKind::Video),
            AppCapability::Music => Some(GenerationKind::Music),
            _ => None,
        }
    }
}

/// Default models an app's manifest may name. Text is a chat model id
/// (`<provider>/<model>`); the others are model names on the global endpoints.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct AppModels {
    pub text: Option<String>,
    pub image: Option<String>,
    pub video: Option<String>,
    pub music: Option<String>,
}

impl AppModels {
    pub fn generation(&self, kind: GenerationKind) -> Option<&str> {
        match kind {
            GenerationKind::Image => self.image.as_deref(),
            GenerationKind::Video => self.video.as_deref(),
            GenerationKind::Music => self.music.as_deref(),
        }
        .map(str::trim)
        .filter(|s| !s.is_empty())
    }
}

fn default_entry() -> String {
    "App.tsx".into()
}

fn default_version() -> String {
    "1.0.0".into()
}

fn default_icon() -> String {
    "sparkles".into()
}

fn default_category() -> String {
    "Other".into()
}

/// An app's `oxide-app.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppManifest {
    /// Unique, lower-case letters, digits and dashes (the folder's name).
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// A lucide icon name (`"file-code"`) or a single emoji.
    #[serde(default = "default_icon")]
    pub icon: String,
    #[serde(default = "default_category")]
    pub category: String,
    #[serde(default = "default_version")]
    pub version: String,
    /// The file whose default export is the app's root component.
    #[serde(default = "default_entry")]
    pub entry: String,
    /// What the app uses that asks first. Calls for anything else are refused.
    #[serde(default)]
    pub capabilities: Vec<AppCapability>,
    #[serde(default)]
    pub models: AppModels,
    #[serde(default)]
    pub author: Option<String>,
}

/// Turns a name into an app id: lower-case ASCII letters and digits, other runs become one
/// dash, at most 48 characters. `"app"` when nothing is left.
pub fn app_slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
        if out.len() >= 48 {
            break;
        }
    }
    let out = out.trim_matches('-').to_owned();
    if out.is_empty() { "app".into() } else { out }
}

/// Whether `id` is a valid app id.
pub fn valid_app_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !id.starts_with('-')
        && !id.ends_with('-')
}

/// Whether `path` may be an app file: relative, `/`-separated, no `..`, `.` or empty
/// segments, no backslashes or drive letters, at most 200 characters.
pub fn valid_app_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 200
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains(':')
        && !path.contains('\0')
        && path
            .split('/')
            .all(|s| !s.is_empty() && s != "." && s != ".." && !s.starts_with(' '))
}

impl AppManifest {
    /// Checks what Oxide relies on. The message says what is wrong, for the import preview.
    pub fn validate(&self) -> Result<(), String> {
        if !valid_app_id(&self.id) {
            return Err(format!(
                "The app id “{}” isn't valid: use lower-case letters, digits and dashes.",
                self.id
            ));
        }
        let name = self.name.trim();
        if name.is_empty() {
            return Err("The app has no name.".into());
        }
        if name.chars().count() > 80 {
            return Err("The app's name is longer than 80 characters.".into());
        }
        if self.description.chars().count() > 1_000 {
            return Err("The app's description is longer than 1000 characters.".into());
        }
        if self.category.trim().is_empty() || self.category.chars().count() > 40 {
            return Err("The app's category must be 1–40 characters.".into());
        }
        if self.icon.trim().is_empty() || self.icon.chars().count() > 40 {
            return Err("The app's icon must be a lucide icon name or an emoji.".into());
        }
        if self.version.trim().is_empty() || self.version.chars().count() > 40 {
            return Err("The app's version must be 1–40 characters.".into());
        }
        if !valid_app_path(&self.entry) {
            return Err(format!(
                "The entry file “{}” isn't a valid path.",
                self.entry
            ));
        }
        let ext = self.entry.rsplit('.').next().unwrap_or("");
        if !matches!(ext, "tsx" | "ts" | "jsx" | "js") {
            return Err("The entry file must be a .tsx, .ts, .jsx or .js file.".into());
        }
        Ok(())
    }

    /// Whether the manifest declares a capability.
    pub fn declares(&self, cap: AppCapability) -> bool {
        self.capabilities.contains(&cap)
    }
}

/// An app as the gallery lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UtilityApp {
    pub manifest: AppManifest,
    /// Ships with Oxide; read-only (clone it to change it).
    pub builtin: bool,
    /// Of an own app: where its folder is.
    pub folder: Option<String>,
    /// Why the app can't run (a broken manifest, a missing entry file).
    pub problem: Option<String>,
}

/// One source file of an app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppFile {
    pub path: String,
    pub content: String,
}

/// Everything the frame needs to run an app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppBundle {
    pub app: UtilityApp,
    pub files: Vec<AppFile>,
}

/// A per-app override of a generation endpoint. Empty fields use the global default
/// (Settings → Media generation): only the model, only a custom URL, or both can be set.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct AppGenerationOverride {
    /// An endpoint from Settings → Providers.
    pub provider_id: Option<String>,
    /// A custom OpenAI-compatible base URL (`https://…/v1`).
    pub base_url: String,
    pub model: String,
}

impl AppGenerationOverride {
    pub fn is_empty(&self) -> bool {
        self.provider_id.is_none()
            && self.base_url.trim().is_empty()
            && self.model.trim().is_empty()
    }

    /// The endpoint this override makes of the global one (and the manifest's model).
    pub fn apply(
        &self,
        global: &GenerationEndpoint,
        manifest_model: Option<&str>,
    ) -> GenerationEndpoint {
        let mut ep = global.clone();
        if let Some(m) = manifest_model {
            ep.model = m.to_owned();
        }
        if !self.base_url.trim().is_empty() {
            ep.base_url = self.base_url.trim().to_owned();
            ep.provider_id = None;
        } else if let Some(p) = &self.provider_id {
            ep.provider_id = Some(p.clone());
            ep.base_url = String::new();
        }
        if !self.model.trim().is_empty() {
            ep.model = self.model.trim().to_owned();
        }
        ep
    }
}

/// Per-app settings, kept in the database (never in the app's folder).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct AppPrefs {
    /// The chat model the app's text calls use (`<provider>/<model>`). `None`: the manifest's,
    /// else the first available.
    pub text_model: Option<String>,
    pub image: AppGenerationOverride,
    pub video: AppGenerationOverride,
    pub music: AppGenerationOverride,
    /// Capabilities the user allowed for this app.
    pub granted: Vec<AppCapability>,
    /// The folder the user picked for the app (apps that declare
    /// [`AppCapability::Folder`]); set only through Oxide's folder picker.
    pub folder: Option<String>,
}

impl AppPrefs {
    pub fn generation(&self, kind: GenerationKind) -> &AppGenerationOverride {
        match kind {
            GenerationKind::Image => &self.image,
            GenerationKind::Video => &self.video,
            GenerationKind::Music => &self.music,
        }
    }

    pub fn allows(&self, cap: AppCapability) -> bool {
        self.granted.contains(&cap)
    }
}

/// Settings → the app's settings dialog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppPrefsView {
    pub prefs: AppPrefs,
    /// The text model calls use now (after defaults), if any model is available.
    pub text_model: Option<String>,
    /// Kinds of generation with a key of the app's own in the keychain.
    pub keys: Vec<GenerationKind>,
    /// Bytes in the app's storage.
    pub storage_bytes: u32,
}

/// What an `.oxideapp` file holds, shown before it is installed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppImportPreview {
    /// Pass it to the install command (the file was read once; nothing is installed yet).
    pub token: String,
    pub manifest: AppManifest,
    pub files: u32,
    pub bytes: u32,
    /// An own app with this id exists and would be replaced.
    pub replaces: bool,
    /// A built-in app has this id: the import gets a new one.
    pub renamed_from: Option<String>,
}

/// Why an app's SDK call failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum AppErrorCode {
    /// The user hasn't allowed the capability for this app yet: ask, then call again.
    NeedsPermission,
    /// The app's manifest doesn't declare the capability.
    Undeclared,
    /// The user said no.
    Denied,
    /// The call was stopped.
    Cancelled,
    /// No such SDK method.
    Unknown,
    /// It ran and failed (the message says why).
    Failed,
}

/// An app's SDK call failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppCallError {
    pub code: AppErrorCode,
    pub message: String,
    pub capability: Option<AppCapability>,
}

impl AppCallError {
    pub fn failed(message: impl Into<String>) -> Self {
        Self {
            code: AppErrorCode::Failed,
            message: message.into(),
            capability: None,
        }
    }

    pub fn new(code: AppErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            capability: None,
        }
    }
}

impl std::fmt::Display for AppCallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// The keychain entry of an app's own key for a kind of generation.
pub fn app_secret_name(app_id: &str, kind: GenerationKind) -> String {
    format!("app:{app_id}:generation:{}", kind.as_str())
}

/// The prefix of every keychain entry of an app (`Delete all` and uninstall remove them).
pub fn app_secret_prefix(app_id: &str) -> String {
    format!("app:{app_id}:")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> AppManifest {
        serde_json::from_str(r#"{"id":"meta-tags","name":"Meta tags"}"#).expect("parses")
    }

    #[test]
    fn manifests_have_defaults_and_validate() {
        let m = manifest();
        assert_eq!(m.entry, "App.tsx");
        assert_eq!(m.version, "1.0.0");
        assert!(m.capabilities.is_empty());
        assert!(m.validate().is_ok());
        let caps: AppManifest = serde_json::from_str(
            r#"{"id":"a","name":"A","capabilities":["text","webFetch"],"models":{"image":"x"}}"#,
        )
        .expect("parses");
        assert!(caps.declares(AppCapability::WebFetch));
        assert_eq!(caps.models.generation(GenerationKind::Image), Some("x"));
        assert!(
            serde_json::from_str::<AppManifest>(r#"{"id":"a","name":"A","capabilities":["fly"]}"#)
                .is_err()
        );
    }

    #[test]
    fn invalid_manifests_say_why() {
        let mut m = manifest();
        m.id = "Bad Id".into();
        assert!(m.validate().unwrap_err().contains("id"));
        let mut m = manifest();
        m.name = "  ".into();
        assert!(m.validate().is_err());
        let mut m = manifest();
        m.entry = "../x.tsx".into();
        assert!(m.validate().is_err());
        let mut m = manifest();
        m.entry = "index.html".into();
        assert!(m.validate().is_err());
    }

    #[test]
    fn paths_and_ids() {
        assert!(valid_app_path("App.tsx"));
        assert!(valid_app_path("lib/seo.ts"));
        for bad in ["", "/etc/x", "a/../b", "a\\b", "C:/x", "./a", "a//b"] {
            assert!(!valid_app_path(bad), "{bad}");
        }
        assert_eq!(
            app_slug("AI Meta Tag Generator (copy)"),
            "ai-meta-tag-generator-copy"
        );
        assert_eq!(app_slug("!!!"), "app");
        assert!(valid_app_id("ai-svg-2"));
        assert!(!valid_app_id("-a"));
        assert!(!valid_app_id("a_b"));
    }

    #[test]
    fn overrides_apply_over_the_global_endpoint() {
        let global = GenerationEndpoint {
            provider_id: Some("openai".into()),
            base_url: String::new(),
            model: "gpt-image-2".into(),
        };
        let none = AppGenerationOverride::default();
        assert!(none.is_empty());
        assert_eq!(none.apply(&global, None), global);
        assert_eq!(none.apply(&global, Some("dall-e")).model, "dall-e");
        let url = AppGenerationOverride {
            provider_id: None,
            base_url: "http://local:8190/v1".into(),
            model: "flux".into(),
        };
        let ep = url.apply(&global, Some("dall-e"));
        assert_eq!(ep.provider_id, None);
        assert_eq!(ep.base_url, "http://local:8190/v1");
        assert_eq!(ep.model, "flux");
        assert_eq!(
            app_secret_name("a", GenerationKind::Video),
            "app:a:generation:video"
        );
        assert!(app_secret_name("a", GenerationKind::Video).starts_with(&app_secret_prefix("a")));
    }

    #[test]
    fn capabilities_round_trip() {
        for c in AppCapability::ALL {
            assert_eq!(AppCapability::parse(c.as_str()), Some(c));
            assert_eq!(
                serde_json::to_string(&c).expect("serializes"),
                format!("\"{}\"", c.as_str())
            );
        }
    }
}
