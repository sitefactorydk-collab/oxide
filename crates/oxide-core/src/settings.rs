//! App settings, stored as one typed JSON document.
//!
//! Every struct uses `#[serde(default)]`, so settings saved by an older build still load
//! after fields are added.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::browser::BrowserSettings;
use crate::context::ContextSettings;
use crate::python::PythonSettings;
use crate::sandbox::SandboxSettings;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ThemePref {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ChatFont {
    #[default]
    Serif,
    Sans,
    System,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct ProfileSettings {
    /// What Oxide calls the user (greeting, avatar initials).
    pub name: String,
}

/// The language of Oxide's own interface (Phase 18). Model replies follow the user.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum UiLanguage {
    /// The system's language when Oxide has it, else English.
    #[default]
    System,
    En,
    Da,
}

/// How a reply's tool steps show in the transcript (Phase 24): folded into one summary line
/// that opens to compact rows, or open.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ToolSteps {
    #[default]
    Collapsed,
    Expanded,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct AppearanceSettings {
    pub theme: ThemePref,
    pub chat_font: ChatFont,
    pub language: UiLanguage,
    /// Settings → Appearance → Show tool steps (Phase 24).
    pub tool_steps: ToolSteps,
}

/// Default transcription model for dictation.
pub const DEFAULT_DICTATION_MODEL: &str = "whisper-1";

/// Settings → Voice: dictation in the composer through an OpenAI-compatible
/// `/audio/transcriptions` endpoint (Phase 18).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct DictationSettings {
    /// Show the microphone button in the composer.
    pub enabled: bool,
    /// The endpoint (a configured provider of an OpenAI-compatible kind). `None`: not set up.
    pub provider_id: Option<String>,
    /// The transcription model (`whisper-1`, `gpt-4o-mini-transcribe`,
    /// `whisper-large-v3-turbo`, …).
    pub model: String,
    /// ISO-639-1 language of the speech (`da`, `en`); empty lets the model detect it.
    pub language: String,
}

impl Default for DictationSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            provider_id: None,
            model: DEFAULT_DICTATION_MODEL.into(),
            language: String::new(),
        }
    }
}

impl DictationSettings {
    pub fn normalized(mut self) -> Self {
        self.provider_id = self
            .provider_id
            .map(|p| p.trim().to_owned())
            .filter(|p| !p.is_empty() && !p.contains('/'));
        self.model = self.model.trim().chars().take(200).collect();
        if self.model.is_empty() {
            self.model = DEFAULT_DICTATION_MODEL.into();
        }
        let lang: String = self
            .language
            .trim()
            .to_ascii_lowercase()
            .chars()
            .take(8)
            .collect();
        self.language = if lang.chars().all(|c| c.is_ascii_lowercase() || c == '-') {
            lang
        } else {
            String::new()
        };
        self
    }
}

/// Default extended-thinking budget, in tokens (for models that take a budget).
pub const DEFAULT_THINKING_BUDGET: u32 = 8_000;
/// Smallest budget the APIs accept.
pub const MIN_THINKING_BUDGET: u32 = 1_024;
/// Largest budget Oxide asks for.
pub const MAX_THINKING_BUDGET: u32 = 64_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct ModelSettings {
    /// List models without native tool calling (greyed out, not selectable).
    pub show_unsupported: bool,
    /// Name new chats after their first exchange.
    pub auto_title: bool,
    /// Model used for automatic titles (`<providerId>/<model>`). `None` picks a small model
    /// from the same endpoint as the chat.
    pub title_model: Option<String>,
    /// Extended-thinking budget in tokens, for models that take one.
    pub thinking_budget: u32,
    /// Whether a model sees images, set by hand where the server doesn't say (Strata,
    /// OpenAI-compatible servers). Overrides what Oxide worked out.
    pub images: Vec<ModelImagesOverride>,
}

/// "Can see images" for one model, set in Settings → Models.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ModelImagesOverride {
    /// `<providerId>/<model>`.
    pub model: String,
    pub images: bool,
}

impl ModelSettings {
    /// The user's answer for a model, if they gave one.
    pub fn images_for(&self, model_id: &str) -> Option<bool> {
        self.images
            .iter()
            .find(|o| o.model == model_id)
            .map(|o| o.images)
    }

    /// Applies the user's "Can see images" answers to listed models.
    pub fn apply_to(&self, models: &mut [crate::model::ModelInfo]) {
        for m in models {
            if let Some(images) = self.images_for(&m.id) {
                m.capabilities.images = images;
            }
        }
    }
}

impl Default for ModelSettings {
    fn default() -> Self {
        Self {
            show_unsupported: false,
            auto_title: true,
            title_model: None,
            thinking_budget: DEFAULT_THINKING_BUDGET,
            images: Vec::new(),
        }
    }
}

/// Default number of tool steps in one reply.
pub const DEFAULT_MAX_STEPS: u32 = 25;
/// Most tool steps Oxide allows in one reply.
pub const MAX_MAX_STEPS: u32 = 200;

/// Tool approvals and the tool loop (Settings → Permissions).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct ToolSettings {
    /// Run MCP tools that say they only read (`readOnlyHint`) without asking.
    pub auto_allow_read_only: bool,
    /// Ask before Oxide's own web search and web fetch run.
    pub ask_before_web: bool,
    /// Most model → tools → model rounds in one reply.
    pub max_steps: u32,
    /// The permission mode new chats start in (Phase 23).
    pub default_mode: crate::PermissionMode,
    /// The model Auto's safety check asks (`<providerId>/<model>`); `None`: the chat's own.
    pub safety_model: Option<String>,
    /// Tools that ask even in Auto: a tool's name, `server/tool` or a whole `server`.
    pub always_ask: Vec<String>,
}

impl Default for ToolSettings {
    fn default() -> Self {
        Self {
            auto_allow_read_only: false,
            ask_before_web: false,
            max_steps: DEFAULT_MAX_STEPS,
            default_mode: crate::PermissionMode::Manual,
            safety_model: None,
            always_ask: Vec::new(),
        }
    }
}

/// Which service Oxide's `web_search` tool uses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum WebSearchProvider {
    /// No search service: models get no `web_search` tool (except Anthropic's own, if on).
    #[default]
    None,
    Brave,
    Tavily,
    Searxng,
}

/// Web search (Settings → Web search). API keys live in the keychain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct WebSearchSettings {
    pub provider: WebSearchProvider,
    /// Base URL of a SearXNG instance (its JSON format must be enabled).
    pub searxng_url: String,
    /// On Anthropic models, use Anthropic's server-side web search instead.
    pub anthropic_native: bool,
    /// Results per search (1–10).
    pub max_results: u32,
}

impl Default for WebSearchSettings {
    fn default() -> Self {
        Self {
            provider: WebSearchProvider::None,
            searxng_url: String::new(),
            anthropic_native: true,
            max_results: 5,
        }
    }
}

/// Default knowledge budget: projects up to this size go into the prompt in full.
pub const DEFAULT_KNOWLEDGE_BUDGET: u32 = 40_000;
/// Smallest and largest knowledge budget Settings accepts.
pub const MIN_KNOWLEDGE_BUDGET: u32 = 1_000;
pub const MAX_KNOWLEDGE_BUDGET: u32 = 400_000;

/// Projects (Settings → Projects & memory).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct ProjectSettings {
    /// Estimated tokens of knowledge that go into the prompt in full. Larger projects are
    /// searched instead. A chat also keeps it under a quarter of the model's context window.
    pub knowledge_budget: u32,
}

impl Default for ProjectSettings {
    fn default() -> Self {
        Self {
            knowledge_budget: DEFAULT_KNOWLEDGE_BUDGET,
        }
    }
}

/// Memory across chats (opt-in).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct MemorySettings {
    /// Learn memory items from chats, and give saved items to models.
    pub enabled: bool,
    /// Model that writes memory items (`<providerId>/<model>`). `None` picks a small model
    /// from the chat's endpoint, like automatic titles.
    pub model: Option<String>,
}

/// Tray, quick entry, start at login and notifications (Settings → Desktop).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct DesktopSettings {
    /// The window's close button hides Oxide to the tray instead of quitting.
    pub close_to_tray: bool,
    /// The global quick entry shortcut is on.
    pub quick_entry: bool,
    /// The quick entry shortcut (`Alt+Space`, `Ctrl+Shift+Space`, …).
    pub quick_entry_shortcut: String,
    /// Start Oxide (hidden, in the tray) when the user logs in.
    pub launch_at_login: bool,
    /// Notify when a reply finishes or a tool needs approval while Oxide isn't in front.
    pub notifications: bool,
}

impl Default for DesktopSettings {
    fn default() -> Self {
        Self {
            close_to_tray: true,
            quick_entry: true,
            quick_entry_shortcut: crate::desktop::DEFAULT_QUICK_ENTRY_SHORTCUT.into(),
            launch_at_login: false,
            notifications: true,
        }
    }
}

/// Default time limit of one shell command, in seconds.
pub const DEFAULT_SHELL_TIMEOUT_SECS: u32 = 120;
/// Longest time limit Settings accepts for one shell command, in seconds.
pub const MAX_SHELL_TIMEOUT_SECS: u32 = 3_600;

/// The shell tool (Settings → Permissions). Off by default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct ShellSettings {
    /// Models may run commands in a chat's granted folders (`run_command`).
    pub enabled: bool,
    /// Commands that run without asking: a command whose start matches an entry word by
    /// word (`git status` allows `git status -s`, not `git push`). Everything else asks.
    pub allowlist: Vec<String>,
    /// Time limit of one command, in seconds.
    pub timeout_secs: u32,
}

impl Default for ShellSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            allowlist: Vec::new(),
            timeout_secs: DEFAULT_SHELL_TIMEOUT_SECS,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct AppSettings {
    pub profile: ProfileSettings,
    pub appearance: AppearanceSettings,
    pub models: ModelSettings,
    pub tools: ToolSettings,
    pub web_search: WebSearchSettings,
    pub projects: ProjectSettings,
    pub memory: MemorySettings,
    pub desktop: DesktopSettings,
    pub shell: ShellSettings,
    pub python: PythonSettings,
    pub sandbox: SandboxSettings,
    pub browser: BrowserSettings,
    /// Settings → Chrome (the extension and its bridge).
    pub chrome: crate::chrome::ChromeSettings,
    /// Settings → Computer use.
    pub computer: crate::computer::ComputerSettings,
    /// Context management (shown in Settings → Models).
    pub context: ContextSettings,
    /// Settings → About → Updates.
    pub updates: crate::data::UpdateSettings,
    /// Settings → Voice (dictation).
    pub dictation: DictationSettings,
    /// Settings → Media generation (Phase 19).
    pub generation: crate::generation::GenerationSettings,
}

impl AppSettings {
    /// Trims free-text fields and caps their length.
    pub fn normalized(mut self) -> Self {
        self.profile.name = self.profile.name.trim().chars().take(80).collect();
        self.models.thinking_budget = self
            .models
            .thinking_budget
            .clamp(MIN_THINKING_BUDGET, MAX_THINKING_BUDGET);
        self.models.title_model = self
            .models
            .title_model
            .map(|m| m.trim().to_owned())
            .filter(|m| crate::model::split_model_id(m).is_some());
        // Valid ids only; the last answer for a model wins.
        let mut images: Vec<ModelImagesOverride> = Vec::new();
        for o in std::mem::take(&mut self.models.images).into_iter().rev() {
            let model = o.model.trim().to_owned();
            if crate::model::split_model_id(&model).is_some()
                && !images.iter().any(|i| i.model == model)
            {
                images.push(ModelImagesOverride {
                    model,
                    images: o.images,
                });
            }
        }
        images.reverse();
        images.truncate(500);
        self.models.images = images;
        self.tools.max_steps = self.tools.max_steps.clamp(1, MAX_MAX_STEPS);
        self.tools.safety_model = self
            .tools
            .safety_model
            .map(|m| m.trim().to_owned())
            .filter(|m| crate::model::split_model_id(m).is_some());
        let mut always: Vec<String> = Vec::new();
        for e in &self.tools.always_ask {
            let e: String = e.trim().chars().take(200).collect();
            if !e.is_empty() && !always.iter().any(|a| a.eq_ignore_ascii_case(&e)) {
                always.push(e);
            }
        }
        always.truncate(crate::MAX_ALWAYS_ASK);
        self.tools.always_ask = always;
        self.web_search.max_results = self.web_search.max_results.clamp(1, 10);
        self.projects.knowledge_budget = self
            .projects
            .knowledge_budget
            .clamp(MIN_KNOWLEDGE_BUDGET, MAX_KNOWLEDGE_BUDGET);
        self.memory.model = self
            .memory
            .model
            .map(|m| m.trim().to_owned())
            .filter(|m| crate::model::split_model_id(m).is_some());
        self.web_search.searxng_url = self
            .web_search
            .searxng_url
            .trim()
            .trim_end_matches('/')
            .to_owned();
        self.shell.timeout_secs = self.shell.timeout_secs.clamp(1, MAX_SHELL_TIMEOUT_SECS);
        let mut allow: Vec<String> = Vec::new();
        for entry in &self.shell.allowlist {
            let e = entry.split_whitespace().collect::<Vec<_>>().join(" ");
            let e: String = e.chars().take(200).collect();
            if !e.is_empty() && !allow.contains(&e) {
                allow.push(e);
            }
        }
        allow.truncate(100);
        self.shell.allowlist = allow;
        self.python.timeout_secs = self
            .python
            .timeout_secs
            .clamp(1, crate::python::MAX_PYTHON_TIMEOUT_SECS);
        self.sandbox = self.sandbox.normalized();
        self.browser = self.browser.normalized();
        self.chrome = self.chrome.normalized();
        self.computer = self.computer.normalized();
        self.context = self.context.normalized();
        self.dictation = self.dictation.normalized();
        self.generation = self.generation.normalized();
        self.desktop.quick_entry_shortcut =
            crate::desktop::clean_shortcut(&self.desktop.quick_entry_shortcut)
                .unwrap_or_else(|| crate::desktop::DEFAULT_QUICK_ENTRY_SHORTCUT.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let s: AppSettings = serde_json::from_str(r#"{"profile":{"name":"Ada"}}"#).expect("parses");
        assert_eq!(s.profile.name, "Ada");
        assert_eq!(s.appearance, AppearanceSettings::default());
    }

    #[test]
    fn python_settings_default_on_and_are_clamped() {
        let mut s = AppSettings::default();
        assert!(s.python.enabled);
        assert_eq!(s.python.timeout_secs, 120);
        s.python.timeout_secs = 0;
        assert_eq!(s.clone().normalized().python.timeout_secs, 1);
        s.python.timeout_secs = 999_999;
        assert_eq!(s.normalized().python.timeout_secs, 3_600);
    }

    #[test]
    fn sandbox_settings_default_off_and_are_normalized() {
        let s: AppSettings = serde_json::from_str("{}").expect("parses");
        assert!(!s.sandbox.enabled);
        let mut s = AppSettings::default();
        s.sandbox.cpus = 0;
        assert_eq!(s.normalized().sandbox.cpus, 1);
    }

    #[test]
    fn shell_settings_are_cleaned() {
        let mut s = AppSettings::default();
        assert!(!s.shell.enabled);
        s.shell.timeout_secs = 0;
        s.shell.allowlist = vec!["  git   status ".into(), "".into(), "git status".into()];
        let s = s.normalized();
        assert_eq!(s.shell.timeout_secs, 1);
        assert_eq!(s.shell.allowlist, vec!["git status".to_owned()]);
    }

    #[test]
    fn language_and_dictation_default_and_are_cleaned() {
        let s: AppSettings =
            serde_json::from_str(r#"{"appearance":{"language":"da"}}"#).expect("parses");
        assert_eq!(s.appearance.language, UiLanguage::Da);
        assert!(s.dictation.enabled);
        assert_eq!(s.dictation.model, DEFAULT_DICTATION_MODEL);
        let mut s = AppSettings::default();
        s.dictation.provider_id = Some(" openai ".into());
        s.dictation.model = "  ".into();
        s.dictation.language = " DA ".into();
        let d = s.normalized().dictation;
        assert_eq!(d.provider_id.as_deref(), Some("openai"));
        assert_eq!(d.model, DEFAULT_DICTATION_MODEL);
        assert_eq!(d.language, "da");
        let mut s = AppSettings::default();
        s.dictation.provider_id = Some("a/b".into());
        s.dictation.language = "d a".into();
        let d = s.normalized().dictation;
        assert_eq!(d.provider_id, None);
        assert_eq!(d.language, "");
    }

    #[test]
    fn tool_steps_default_to_collapsed() {
        let s: AppSettings =
            serde_json::from_str(r#"{"appearance":{"theme":"dark"}}"#).expect("parses");
        assert_eq!(s.appearance.tool_steps, ToolSteps::Collapsed);
        let s: AppSettings =
            serde_json::from_str(r#"{"appearance":{"toolSteps":"expanded"}}"#).expect("parses");
        assert_eq!(s.appearance.tool_steps, ToolSteps::Expanded);
        let json = serde_json::to_value(&s).expect("serializes");
        assert_eq!(json["appearance"]["toolSteps"], "expanded");
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let s: AppSettings =
            serde_json::from_str(r#"{"appearance":{"theme":"dark","future":1}}"#).expect("parses");
        assert_eq!(s.appearance.theme, ThemePref::Dark);
    }

    #[test]
    fn normalized_trims_name() {
        let mut s = AppSettings::default();
        s.profile.name = "  Mikkel ".into();
        assert_eq!(s.normalized().profile.name, "Mikkel");
    }

    #[test]
    fn model_settings_default_and_clamp() {
        let s: AppSettings = serde_json::from_str("{}").expect("parses");
        assert!(s.models.auto_title);
        assert_eq!(s.models.thinking_budget, DEFAULT_THINKING_BUDGET);
        let mut s = AppSettings::default();
        s.models.thinking_budget = 10;
        s.models.title_model = Some("  ".into());
        let s = s.normalized();
        assert_eq!(s.models.thinking_budget, MIN_THINKING_BUDGET);
        assert_eq!(s.models.title_model, None);
    }

    #[test]
    fn models_can_be_marked_as_seeing_images() {
        let s: AppSettings = serde_json::from_str("{}").expect("parses");
        assert!(s.models.images.is_empty());
        let mut s = AppSettings::default();
        s.models.images = vec![
            ModelImagesOverride {
                model: "strata/swift".into(),
                images: false,
            },
            ModelImagesOverride {
                model: " no-slash ".into(),
                images: true,
            },
            ModelImagesOverride {
                model: " strata/swift ".into(),
                images: true,
            },
        ];
        let s = s.normalized();
        assert_eq!(
            s.models.images,
            vec![ModelImagesOverride {
                model: "strata/swift".into(),
                images: true
            }]
        );
        assert_eq!(s.models.images_for("strata/swift"), Some(true));
        assert_eq!(s.models.images_for("strata/other"), None);
        let mut listed = vec![crate::model::ModelInfo {
            id: "strata/swift".into(),
            provider_id: "strata".into(),
            model: "swift".into(),
            label: "swift".into(),
            capabilities: crate::model::ModelCapabilities::default(),
            context_window: None,
            max_output_tokens: None,
        }];
        s.models.apply_to(&mut listed);
        assert!(listed[0].capabilities.images);
    }

    #[test]
    fn tool_settings_default_and_clamp() {
        let s: AppSettings = serde_json::from_str("{}").expect("parses");
        assert_eq!(s.tools.max_steps, DEFAULT_MAX_STEPS);
        assert!(!s.tools.auto_allow_read_only);
        assert!(s.web_search.anthropic_native);
        assert_eq!(s.web_search.provider, WebSearchProvider::None);
        let mut s = AppSettings::default();
        s.tools.max_steps = 0;
        s.web_search.max_results = 50;
        s.web_search.searxng_url = " http://localhost:8888/ ".into();
        let s = s.normalized();
        assert_eq!(s.tools.max_steps, 1);
        assert_eq!(s.web_search.max_results, 10);
        assert_eq!(s.web_search.searxng_url, "http://localhost:8888");
    }

    #[test]
    fn project_and_memory_settings() {
        let s: AppSettings = serde_json::from_str("{}").expect("parses");
        assert_eq!(s.projects.knowledge_budget, DEFAULT_KNOWLEDGE_BUDGET);
        assert!(!s.memory.enabled, "memory is opt-in");
        let mut s = AppSettings::default();
        s.projects.knowledge_budget = 5;
        s.memory.model = Some("nope".into());
        let s = s.normalized();
        assert_eq!(s.projects.knowledge_budget, MIN_KNOWLEDGE_BUDGET);
        assert_eq!(s.memory.model, None);
    }

    #[test]
    fn desktop_settings() {
        let s: AppSettings = serde_json::from_str("{}").expect("parses");
        assert!(s.desktop.close_to_tray);
        assert!(s.desktop.quick_entry);
        assert!(!s.desktop.launch_at_login, "start at login is opt-in");
        assert_eq!(s.desktop.quick_entry_shortcut, "Alt+Space");
        let mut s = AppSettings::default();
        s.desktop.quick_entry_shortcut = "shift+ctrl+space".into();
        assert_eq!(
            s.normalized().desktop.quick_entry_shortcut,
            "Ctrl+Shift+Space"
        );
        let mut s = AppSettings::default();
        s.desktop.quick_entry_shortcut = "K".into();
        assert_eq!(s.normalized().desktop.quick_entry_shortcut, "Alt+Space");
    }
}
