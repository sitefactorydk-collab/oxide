//! The tools a model may call: MCP servers' tools and Oxide's built-in ones.
//!
//! [`Tools`] builds a [`ToolSet`] for one request (which tools are on in the chat, under
//! which names the model sees them) and runs calls. Names the model sees must match
//! `^[a-zA-Z0-9_-]{1,64}$` for both Anthropic and OpenAI, so MCP tools become
//! `server__tool` with other characters replaced.

pub mod browser;
pub mod chrome;
pub mod computer;
pub mod files;
pub mod html;
pub mod python;
pub mod sandbox;
pub mod shell;
pub mod web;

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use futures_util::future::BoxFuture;
use oxide_core::artifact::ARTIFACT_TOOL;
use oxide_core::tools::{PROJECT_SEARCH, PROVIDER_SERVER, WEB_FETCH, WEB_SEARCH};
use oxide_core::{
    AppSettings, AttachmentSource, ChatTools, ContentBlock, McpContent, McpStatus, McpTool,
    McpToolResult, MediaSource, ProviderKind, ToolUsageKind,
};
use oxide_core::{LOAD_SKILL, SAVE_SKILL};
use oxide_llm::ToolSpec;
use oxide_store::SecretStore;
use serde_json::{Map, Value, json};

use crate::attachments::AttachmentStore;
use crate::skills::Skills;
pub use web::{Fetched, SearchHit, WebTools};

/// Most characters of text one tool result gives the model.
pub const MAX_RESULT_CHARS: usize = 100_000;

/// Where the app's MCP tools come from. Implemented for [`oxide_mcp::McpManager`].
pub trait McpTools: Send + Sync + 'static {
    /// Tools of the servers that are running.
    fn tools(&self) -> Vec<McpTool>;
    /// Calls a tool. `progress` is the token progress notices carry (the tool call's id).
    fn call<'a>(
        &'a self,
        server: &'a str,
        tool: &'a str,
        arguments: Value,
        progress: Option<String>,
    ) -> BoxFuture<'a, Result<McpToolResult, String>>;
}

impl McpTools for oxide_mcp::McpManager {
    fn tools(&self) -> Vec<McpTool> {
        self.states()
            .into_iter()
            .filter(|s| s.status == McpStatus::Running)
            .filter_map(|s| self.lists(&s.name))
            .flat_map(|l| l.tools)
            .collect()
    }

    fn call<'a>(
        &'a self,
        server: &'a str,
        tool: &'a str,
        arguments: Value,
        progress: Option<String>,
    ) -> BoxFuture<'a, Result<McpToolResult, String>> {
        Box::pin(async move {
            self.call_tool(server, tool, arguments, progress)
                .await
                .map_err(|e| e.to_string())
        })
    }
}

/// Oxide's own tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Builtin {
    WebSearch,
    WebFetch,
    /// Search the chat's project knowledge (run by the chat service, which knows the project).
    ProjectSearch,
    /// Create and change artifacts (run by the chat service, which knows the chat).
    Artifact,
    /// Read a skill's instructions or files.
    LoadSkill,
    /// Create or change a skill (always asks; run by the chat service, which shows the user
    /// what would be written first).
    SaveSkill,
    /// A file tool in the chat's granted folders (run by the chat service).
    File(files::FileOp),
    /// The chat's task list (run by the chat service).
    TaskList,
    /// A shell command in a granted folder (run by the chat service).
    RunCommand,
    /// Python code in the managed environment (run by the chat service).
    RunPython,
    /// A command in the chat's Docker sandbox (run by the chat service).
    SandboxExec,
    /// A copy from a shared folder into the sandbox (run by the chat service).
    SandboxCopyIn,
    /// A copy from the sandbox into a shared folder (run by the chat service).
    SandboxCopyOut,
    /// Image, video or audio generation (Phase 19; run by the chat service, which shows
    /// progress and finds reference images).
    Generate(oxide_core::GenerationKind),
}

impl Builtin {
    pub fn name(self) -> &'static str {
        match self {
            Builtin::WebSearch => WEB_SEARCH,
            Builtin::WebFetch => WEB_FETCH,
            Builtin::ProjectSearch => PROJECT_SEARCH,
            Builtin::Artifact => ARTIFACT_TOOL,
            Builtin::LoadSkill => LOAD_SKILL,
            Builtin::SaveSkill => SAVE_SKILL,
            Builtin::File(op) => op.name(),
            Builtin::TaskList => oxide_core::workspace::TASK_LIST,
            Builtin::RunCommand => oxide_core::workspace::RUN_COMMAND,
            Builtin::RunPython => oxide_core::RUN_PYTHON,
            Builtin::SandboxExec => oxide_core::sandbox::SANDBOX_EXEC,
            Builtin::SandboxCopyIn => oxide_core::sandbox::SANDBOX_COPY_IN,
            Builtin::SandboxCopyOut => oxide_core::sandbox::SANDBOX_COPY_OUT,
            Builtin::Generate(kind) => kind.tool().unwrap_or(oxide_core::GENERATE_AUDIO),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolKind {
    Builtin(Builtin),
    Mcp {
        server: String,
        tool: String,
    },
    /// A tool of the chat's sandbox file server (run by the chat service). Its own kind, so a
    /// connector that happens to be called "sandbox" is never mistaken for it.
    SandboxFile {
        tool: String,
    },
    /// A tool of the chat's browser agent (run by the chat service, which starts the
    /// chat's agent). Its own kind, like the sandbox's file tools.
    Browser {
        tool: String,
    },
    /// A tool of the user's Chrome through Oxide's extension (run by the chat service,
    /// which knows the chat's session).
    Chrome {
        tool: String,
    },
    /// The user's screen, mouse and keyboard (Phase 16; run by the chat service, which asks
    /// for the chat and for each app).
    Computer,
}

/// One tool as offered to the model.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolEntry {
    /// The name the model calls it by.
    pub api_name: String,
    pub kind: ToolKind,
    pub title: Option<String>,
    pub description: String,
    pub schema: Value,
    /// Says it only reads (MCP `readOnlyHint`; built-in web tools).
    pub read_only: bool,
    pub destructive: bool,
}

impl ToolEntry {
    pub fn server(&self) -> Option<&str> {
        match &self.kind {
            ToolKind::Builtin(_) => None,
            ToolKind::Mcp { server, .. } => Some(server),
            ToolKind::SandboxFile { .. } => Some(oxide_core::sandbox::SANDBOX_SERVER),
            ToolKind::Browser { .. } => Some(oxide_core::browser::BROWSER_SERVER),
            ToolKind::Chrome { .. } => Some(oxide_core::chrome::CHROME_SERVER),
            ToolKind::Computer => Some(oxide_core::computer::COMPUTER_TOOL),
        }
    }

    /// The tool's own name.
    pub fn tool(&self) -> &str {
        match &self.kind {
            ToolKind::Builtin(b) => b.name(),
            ToolKind::Mcp { tool, .. }
            | ToolKind::SandboxFile { tool }
            | ToolKind::Browser { tool }
            | ToolKind::Chrome { tool } => tool,
            ToolKind::Computer => oxide_core::computer::COMPUTER_TOOL,
        }
    }

    /// Key of "Allow always for this chat": `server/tool` or `builtin/<name>`.
    pub fn grant_key(&self) -> String {
        match &self.kind {
            ToolKind::SandboxFile { tool } => format!("builtin/sandbox__{tool}"),
            ToolKind::Browser { tool } => format!("builtin/browser__{tool}"),
            ToolKind::Chrome { tool } => format!("builtin/chrome__{tool}"),
            ToolKind::Computer => oxide_core::computer::COMPUTER_SESSION_GRANT.to_owned(),
            _ => format!("{}/{}", self.server().unwrap_or("builtin"), self.tool()),
        }
    }

    pub fn usage_kind(&self) -> ToolUsageKind {
        match self.kind {
            ToolKind::Builtin(Builtin::WebSearch) => ToolUsageKind::WebSearch,
            ToolKind::Builtin(Builtin::WebFetch) => ToolUsageKind::WebFetch,
            ToolKind::Builtin(Builtin::ProjectSearch) => ToolUsageKind::ProjectSearch,
            ToolKind::Builtin(Builtin::Artifact) => ToolUsageKind::Artifact,
            ToolKind::Builtin(Builtin::LoadSkill | Builtin::SaveSkill) => ToolUsageKind::Skill,
            ToolKind::Builtin(Builtin::File(_)) => ToolUsageKind::File,
            ToolKind::Builtin(Builtin::TaskList) => ToolUsageKind::Tasks,
            ToolKind::Builtin(Builtin::RunCommand) => ToolUsageKind::Shell,
            ToolKind::Builtin(Builtin::RunPython) => ToolUsageKind::Python,
            ToolKind::Builtin(Builtin::Generate(_)) => ToolUsageKind::Generation,
            ToolKind::Builtin(
                Builtin::SandboxExec | Builtin::SandboxCopyIn | Builtin::SandboxCopyOut,
            )
            | ToolKind::SandboxFile { .. } => ToolUsageKind::Sandbox,
            ToolKind::Browser { .. } => ToolUsageKind::Browser,
            ToolKind::Chrome { .. } => ToolUsageKind::Chrome,
            ToolKind::Computer => ToolUsageKind::Computer,
            ToolKind::Mcp { .. } => ToolUsageKind::Connector,
        }
    }

    pub fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: self.api_name.clone(),
            description: self.description.clone(),
            input_schema: self.schema.clone(),
        }
    }
}

/// The tools of one request.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ToolSet {
    pub entries: Vec<ToolEntry>,
    /// Provider-run tools (Anthropic web search), as JSON definitions.
    pub server_tools: Vec<Value>,
    /// Skills the model may load (`load_skill` is in the set): name and description.
    pub skills: Vec<(String, String)>,
}

impl ToolSet {
    pub fn find(&self, api_name: &str) -> Option<&ToolEntry> {
        self.entries.iter().find(|e| e.api_name == api_name)
    }

    pub fn specs(&self) -> Vec<ToolSpec> {
        self.entries.iter().map(ToolEntry::spec).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.server_tools.is_empty()
    }

    /// Whether web search is available in some form.
    pub fn can_search(&self) -> bool {
        !self.server_tools.is_empty()
            || self
                .entries
                .iter()
                .any(|e| e.kind == ToolKind::Builtin(Builtin::WebSearch))
    }

    pub fn can_make_artifacts(&self) -> bool {
        self.entries
            .iter()
            .any(|e| e.kind == ToolKind::Builtin(Builtin::Artifact))
    }

    pub fn can_fetch(&self) -> bool {
        self.entries
            .iter()
            .any(|e| e.kind == ToolKind::Builtin(Builtin::WebFetch))
    }

    /// The MCP servers that have tools in this set.
    pub fn servers(&self) -> Vec<String> {
        let mut seen = Vec::new();
        for e in &self.entries {
            if let ToolKind::Mcp { server: s, .. } = &e.kind
                && !seen.iter().any(|x: &String| x == s)
            {
                seen.push(s.to_owned());
            }
        }
        seen
    }
}

/// What a tool call returned.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolOutput {
    pub content: Vec<ContentBlock>,
    pub is_error: bool,
    pub duration_ms: u32,
}

impl ToolOutput {
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            content: vec![ContentBlock::text(message)],
            is_error: true,
            duration_ms: 0,
        }
    }
}

/// Runs tools.
#[derive(Clone)]
pub struct Tools {
    mcp: Option<Arc<dyn McpTools>>,
    pub web: WebTools,
    attachments: Option<AttachmentStore>,
    skills: Option<Skills>,
    python: Option<oxide_pyenv::PythonEnv>,
    sandboxes: Option<oxide_sandbox::Sandboxes>,
    browser: Option<oxide_pyenv::BrowserAgents>,
    chrome: Option<oxide_bridge::Bridge>,
    computer: Option<crate::computer::ComputerUse>,
    generation: Option<crate::generation::Generation>,
}

impl Tools {
    pub fn new(
        mcp: Option<Arc<dyn McpTools>>,
        secrets: Arc<dyn SecretStore>,
        attachments: Option<AttachmentStore>,
    ) -> Self {
        Self {
            mcp,
            web: WebTools::new(secrets),
            attachments,
            skills: None,
            python: None,
            sandboxes: None,
            browser: None,
            chrome: None,
            computer: None,
            generation: None,
        }
    }

    /// Gives models the generation tools (for the kinds set up in Settings → Generation).
    pub fn with_generation(mut self, generation: crate::generation::Generation) -> Self {
        self.generation = Some(generation);
        self
    }

    pub fn generation(&self) -> Option<&crate::generation::Generation> {
        self.generation.as_ref()
    }

    /// Gives models `run_python`, running in this managed environment.
    pub fn with_python(mut self, python: oxide_pyenv::PythonEnv) -> Self {
        self.python = Some(python);
        self
    }

    pub fn python(&self) -> Option<&oxide_pyenv::PythonEnv> {
        self.python.as_ref()
    }

    /// Gives models the Docker sandbox's tools (when it is on in settings).
    pub fn with_sandboxes(mut self, sandboxes: oxide_sandbox::Sandboxes) -> Self {
        self.sandboxes = Some(sandboxes);
        self
    }

    pub fn sandboxes(&self) -> Option<&oxide_sandbox::Sandboxes> {
        self.sandboxes.as_ref()
    }

    /// Gives models the browser agent's tools (when it is on in settings).
    pub fn with_browser(mut self, browser: oxide_pyenv::BrowserAgents) -> Self {
        self.browser = Some(browser);
        self
    }

    pub fn browser(&self) -> Option<&oxide_pyenv::BrowserAgents> {
        self.browser.as_ref()
    }

    /// Gives models the Chrome extension's tools (when it is on in settings and a paired
    /// Chrome is connected).
    pub fn with_chrome(mut self, chrome: oxide_bridge::Bridge) -> Self {
        self.chrome = Some(chrome);
        self
    }

    pub fn chrome(&self) -> Option<&oxide_bridge::Bridge> {
        self.chrome.as_ref()
    }

    /// Gives models the `computer` tool (when Settings → Computer use is on).
    pub fn with_computer(mut self, computer: crate::computer::ComputerUse) -> Self {
        self.computer = Some(computer);
        self
    }

    pub fn computer(&self) -> Option<&crate::computer::ComputerUse> {
        self.computer.as_ref()
    }

    /// Gives models the skills in this folder (`load_skill`).
    pub fn with_skills(mut self, skills: Skills) -> Self {
        self.skills = Some(skills);
        self
    }

    pub fn skills(&self) -> Option<&Skills> {
        self.skills.as_ref()
    }

    pub fn with_web(mut self, web: WebTools) -> Self {
        self.web = web;
        self
    }

    /// The tools a chat may use with a provider.
    pub async fn tool_set(
        &self,
        chat: &ChatTools,
        settings: &AppSettings,
        kind: ProviderKind,
    ) -> ToolSet {
        let mut set = ToolSet::default();
        let mut taken: HashSet<String> = HashSet::new();
        let ws = &settings.web_search;
        if chat.allows_builtin(WEB_SEARCH) {
            if kind == ProviderKind::Anthropic && ws.anthropic_native {
                set.server_tools.push(json!({
                    "type": "web_search_20250305",
                    "name": "web_search",
                    "max_uses": 5,
                }));
                taken.insert(WEB_SEARCH.into());
            } else if self.web.search_ready(ws).await {
                set.entries.push(builtin_entry(Builtin::WebSearch));
                taken.insert(WEB_SEARCH.into());
            }
        }
        if chat.allows_builtin(WEB_FETCH) {
            set.entries.push(builtin_entry(Builtin::WebFetch));
            taken.insert(WEB_FETCH.into());
        }
        if chat.allows_builtin(ARTIFACT_TOOL) {
            set.entries.push(crate::artifacts::artifact_entry());
            taken.insert(ARTIFACT_TOOL.into());
        }
        if let Some(skills) = &self.skills
            && (chat.allows_builtin(LOAD_SKILL) || chat.allows_builtin(SAVE_SKILL))
        {
            let dir = skills.clone();
            let all = tokio::task::spawn_blocking(move || dir.list())
                .await
                .unwrap_or_default();
            let usable: Vec<_> = all
                .iter()
                .filter(|s| s.enabled && s.problem.is_none())
                .cloned()
                .collect();
            if chat.allows_builtin(SAVE_SKILL) {
                set.entries.push(crate::skills::save_skill_entry(&all));
                taken.insert(SAVE_SKILL.into());
            }
            if chat.allows_builtin(LOAD_SKILL) && !usable.is_empty() {
                set.skills = usable
                    .into_iter()
                    .map(|s| (s.name, s.description))
                    .collect();
                set.entries.push(crate::skills::load_skill_entry());
                taken.insert(LOAD_SKILL.into());
            }
        }
        if let Some(mcp) = &self.mcp {
            let mut tools = mcp.tools();
            tools.sort_by(|a, b| (&a.server, &a.name).cmp(&(&b.server, &b.name)));
            for t in tools {
                if !chat.allows(&t.server, &t.name) {
                    continue;
                }
                let api_name = unique_name(&api_name(&t.server, &t.name), &taken);
                taken.insert(api_name.clone());
                let title = t
                    .title
                    .clone()
                    .or_else(|| t.annotations.title.clone())
                    .filter(|s| !s.trim().is_empty());
                let mut description = t.description.clone().unwrap_or_default();
                if description.trim().is_empty() {
                    description = title.clone().unwrap_or_else(|| t.name.clone());
                }
                set.entries.push(ToolEntry {
                    api_name,
                    title,
                    description: format!("[{}] {}", t.server, description.trim()),
                    schema: normalize_schema(&t.input_schema),
                    read_only: t.annotations.read_only_hint == Some(true),
                    destructive: t.annotations.destructive_hint == Some(true)
                        && t.annotations.read_only_hint != Some(true),
                    kind: ToolKind::Mcp {
                        server: t.server,
                        tool: t.name,
                    },
                });
            }
        }
        set
    }

    /// Runs one call. `id` is the tool call's id (MCP progress token).
    pub async fn run(
        &self,
        entry: &ToolEntry,
        input: &Value,
        id: &str,
        settings: &AppSettings,
    ) -> ToolOutput {
        let started = Instant::now();
        let mut out = match &entry.kind {
            ToolKind::Builtin(Builtin::WebSearch) => self.web_search(input, settings).await,
            ToolKind::Builtin(Builtin::WebFetch) => self.web_fetch(input).await,
            ToolKind::Builtin(Builtin::ProjectSearch) => {
                ToolOutput::error("Project search runs in the chat, not here.")
            }
            ToolKind::Builtin(Builtin::Artifact) => {
                ToolOutput::error("Artifacts are made in the chat, not here.")
            }
            ToolKind::Builtin(Builtin::LoadSkill) => self.load_skill(input).await,
            ToolKind::Builtin(
                Builtin::File(_)
                | Builtin::SaveSkill
                | Builtin::TaskList
                | Builtin::RunCommand
                | Builtin::RunPython
                | Builtin::SandboxExec
                | Builtin::SandboxCopyIn
                | Builtin::SandboxCopyOut
                | Builtin::Generate(_),
            )
            | ToolKind::SandboxFile { .. }
            | ToolKind::Browser { .. }
            | ToolKind::Chrome { .. }
            | ToolKind::Computer => ToolOutput::error("This tool runs in the chat, not here."),
            ToolKind::Mcp { server, tool } => match &self.mcp {
                None => ToolOutput::error("Connectors aren't available."),
                Some(mcp) => match mcp
                    .call(server, tool, input.clone(), Some(id.to_owned()))
                    .await
                {
                    Ok(r) => ToolOutput {
                        content: self.mcp_blocks(r).await,
                        is_error: false,
                        duration_ms: 0,
                    }
                    .with_error_flag(),
                    Err(e) => ToolOutput::error(format!("The tool couldn't run: {e}")),
                },
            },
        };
        out.duration_ms = u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);
        cap_text(&mut out.content);
        out
    }

    /// An MCP tool result as a tool output (images stored, errors flagged).
    pub async fn mcp_output(&self, r: McpToolResult) -> ToolOutput {
        ToolOutput {
            content: self.mcp_blocks(r).await,
            is_error: false,
            duration_ms: 0,
        }
        .with_error_flag()
    }

    async fn load_skill(&self, input: &Value) -> ToolOutput {
        let Some(skills) = self.skills.clone() else {
            return ToolOutput::error("Skills aren't available.");
        };
        let Some(name) = input.get("name").and_then(Value::as_str).map(str::to_owned) else {
            return ToolOutput::error("load_skill needs a \"name\" string.");
        };
        let path = input.get("path").and_then(Value::as_str).map(str::to_owned);
        let res = tokio::task::spawn_blocking(move || skills.load(&name, path.as_deref()))
            .await
            .unwrap_or_else(|e| Err(e.to_string()));
        match res {
            Ok(text) => ToolOutput {
                content: vec![ContentBlock::text(text)],
                is_error: false,
                duration_ms: 0,
            },
            Err(e) => ToolOutput::error(e),
        }
    }

    async fn web_search(&self, input: &Value, settings: &AppSettings) -> ToolOutput {
        let Some(query) = input.get("query").and_then(Value::as_str) else {
            return ToolOutput::error("web_search needs a \"query\" string.");
        };
        let max = input
            .get("max_results")
            .and_then(Value::as_u64)
            .map(|n| u32::try_from(n).unwrap_or(10))
            .unwrap_or(settings.web_search.max_results);
        match self
            .web
            .search(&settings.web_search, query, max, None)
            .await
        {
            Ok(hits) => ToolOutput {
                content: vec![ContentBlock::text(web::format_hits(query, &hits))],
                is_error: false,
                duration_ms: 0,
            },
            Err(e) => ToolOutput::error(format!("The search failed: {e}")),
        }
    }

    async fn web_fetch(&self, input: &Value) -> ToolOutput {
        let Some(url) = input.get("url").and_then(Value::as_str) else {
            return ToolOutput::error("web_fetch needs a \"url\" string.");
        };
        let start = input
            .get("start")
            .and_then(Value::as_u64)
            .map(|n| usize::try_from(n).unwrap_or(0))
            .unwrap_or(0);
        let max = input
            .get("max_chars")
            .and_then(Value::as_u64)
            .map(|n| usize::try_from(n).unwrap_or(web::MAX_FETCH_CHARS))
            .unwrap_or(web::DEFAULT_FETCH_CHARS)
            .clamp(1_000, web::MAX_FETCH_CHARS);
        match self.web.fetch(url).await {
            Ok(Fetched::Text {
                url,
                title,
                text,
                content_type,
            }) => {
                let mut head = String::new();
                if let Some(t) = title {
                    head.push_str(&format!("# {t}\n"));
                }
                head.push_str(&format!("URL: {url}\nType: {content_type}\n\n"));
                let body = if text.trim().is_empty() {
                    "[The page has no readable text. It may need JavaScript.]".to_owned()
                } else {
                    web::page_slice(&text, start, max)
                };
                ToolOutput {
                    content: vec![ContentBlock::text(head + &body)],
                    is_error: false,
                    duration_ms: 0,
                }
            }
            Ok(Fetched::Image {
                url,
                media_type,
                data_base64,
            }) => ToolOutput {
                content: vec![
                    ContentBlock::text(format!("Image from {url}:")),
                    self.image_block(&media_type, data_base64, "fetched-image")
                        .await,
                ],
                is_error: false,
                duration_ms: 0,
            },
            Err(e) => ToolOutput::error(format!("The page couldn't be fetched: {e}")),
        }
    }

    /// An image block, stored as an attachment file when possible (keeps the database small).
    pub(crate) async fn image_block(
        &self,
        media_type: &str,
        data: String,
        name: &str,
    ) -> ContentBlock {
        if let Some(store) = &self.attachments {
            let ext = media_type.rsplit('/').next().unwrap_or("png");
            match store
                .prepare(AttachmentSource::Bytes {
                    name: format!("{name}.{ext}"),
                    data: data.clone(),
                })
                .await
            {
                Ok(att) => {
                    if let Ok(mut blocks) = store.blocks(&[att])
                        && let Some(b) = blocks.pop()
                    {
                        return b;
                    }
                }
                Err(e) => tracing::info!("tool image kept inline: {e}"),
            }
        }
        ContentBlock::Image {
            media_type: media_type.to_owned(),
            source: MediaSource::Base64 { data },
        }
    }

    async fn mcp_blocks(&self, r: McpToolResult) -> Vec<ContentBlock> {
        let mut out = Vec::new();
        for c in r.content {
            match c {
                McpContent::Text { text } => out.push(ContentBlock::text(text)),
                McpContent::Image { data, mime_type } => {
                    out.push(self.image_block(&mime_type, data, "tool-image").await);
                }
                McpContent::Audio { mime_type, .. } => {
                    out.push(ContentBlock::text(format!(
                        "[Audio ({mime_type}) not shown]"
                    )));
                }
                McpContent::ResourceLink {
                    uri,
                    name,
                    description,
                    ..
                } => out.push(ContentBlock::text(format!(
                    "[Resource: {}{} <{uri}>]",
                    name.unwrap_or_default(),
                    description.map(|d| format!(" — {d}")).unwrap_or_default()
                ))),
                McpContent::Resource {
                    uri,
                    mime_type,
                    text,
                    blob,
                } => match (text, blob) {
                    (Some(t), _) => out.push(ContentBlock::text(format!(
                        "<resource uri=\"{uri}\">\n{t}\n</resource>"
                    ))),
                    (None, Some(b))
                        if mime_type
                            .as_deref()
                            .is_some_and(|m| m.starts_with("image/")) =>
                    {
                        let m = mime_type.unwrap_or_default();
                        out.push(self.image_block(&m, b, "tool-resource").await);
                    }
                    _ => out.push(ContentBlock::text(format!(
                        "[Binary resource {uri} ({}) not shown]",
                        mime_type.unwrap_or_else(|| "unknown type".into())
                    ))),
                },
            }
        }
        if out.is_empty()
            && let Some(s) = r.structured
        {
            out.push(ContentBlock::text(s));
        }
        if out.is_empty() {
            out.push(ContentBlock::text("(no output)"));
        }
        if r.is_error {
            out.insert(0, ContentBlock::text("[tool error]"));
        }
        out
    }
}

impl ToolOutput {
    /// MCP marks failures in the result; `mcp_blocks` puts a marker first.
    fn with_error_flag(mut self) -> Self {
        if let Some(ContentBlock::Text { text }) = self.content.first()
            && text == "[tool error]"
        {
            self.content.remove(0);
            self.is_error = true;
        }
        self
    }
}

fn builtin_entry(b: Builtin) -> ToolEntry {
    let (title, description, schema) = match b {
        Builtin::ProjectSearch => return crate::knowledge::project_search_entry(),
        Builtin::Artifact => return crate::artifacts::artifact_entry(),
        Builtin::LoadSkill => return crate::skills::load_skill_entry(),
        Builtin::SaveSkill => return crate::skills::save_skill_entry(&[]),
        Builtin::File(op) => return files::file_entry(op, false),
        Builtin::TaskList => return crate::tasks::task_list_entry(),
        Builtin::RunCommand => return shell::shell_entry(&shell::Shell::detect()),
        Builtin::RunPython => return python::python_entry(),
        Builtin::SandboxExec => return sandbox::exec_entry(),
        Builtin::SandboxCopyIn => return sandbox::copy_in_entry(),
        Builtin::SandboxCopyOut => return sandbox::copy_out_entry(),
        Builtin::Generate(kind) => {
            return crate::generation::tool_entry(kind, &oxide_core::GenerationSettings::default());
        }
        Builtin::WebSearch => (
            "Web search",
            "Search the web. Returns the titles, URLs and snippets of the top results. Use it \
             for current events, facts that may have changed since your training, and anything \
             you don't know. Read promising pages with web_fetch.",
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "What to search for." },
                    "max_results": { "type": "integer", "minimum": 1, "maximum": 10,
                                     "description": "How many results (default 5)." }
                },
                "required": ["query"]
            }),
        ),
        Builtin::WebFetch => (
            "Fetch web page",
            "Fetch a web page or file (HTML, text, JSON, PDF or image) by URL and return its \
             main content as Markdown. Long pages are cut; pass `start` to read further.",
            json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "An http or https URL." },
                    "start": { "type": "integer", "minimum": 0,
                               "description": "Character to start at, to read past a cut." },
                    "max_chars": { "type": "integer", "minimum": 1000, "maximum": 100000,
                                   "description": "Most characters to return (default 40000)." }
                },
                "required": ["url"]
            }),
        ),
    };
    ToolEntry {
        api_name: b.name().into(),
        kind: ToolKind::Builtin(b),
        title: Some(title.into()),
        description: description.into(),
        schema,
        read_only: true,
        destructive: false,
    }
}

/// `server__tool` with characters the APIs reject replaced by `_`, at most 64 long.
pub fn api_name(server: &str, tool: &str) -> String {
    let clean = |s: &str| -> String {
        s.chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    };
    let name = format!("{}__{}", clean(server), clean(tool));
    name.chars().take(64).collect()
}

fn unique_name(base: &str, taken: &HashSet<String>) -> String {
    if !taken.contains(base) {
        return base.to_owned();
    }
    for n in 2.. {
        let suffix = format!("_{n}");
        let keep = 64 - suffix.len();
        let candidate = format!("{}{suffix}", base.chars().take(keep).collect::<String>());
        if !taken.contains(&candidate) {
            return candidate;
        }
    }
    base.to_owned()
}

/// Makes an MCP tool's input schema acceptable to both Anthropic and OpenAI: a top-level
/// `object` with `properties`, no `$schema`/`$id`, and top-level `anyOf`/`oneOf`/`allOf`
/// (which both APIs reject) folded into `properties`.
pub fn normalize_schema(raw: &str) -> Value {
    let mut v: Value = serde_json::from_str(raw).unwrap_or_else(|_| json!({}));
    if !v.is_object() {
        v = json!({});
    }
    let Some(o) = v.as_object_mut() else {
        return json!({"type": "object", "properties": {}});
    };
    o.remove("$schema");
    o.remove("$id");
    let mut props: Map<String, Value> = match o.remove("properties") {
        Some(Value::Object(p)) => p,
        _ => Map::new(),
    };
    let mut required: Vec<String> = match o.remove("required") {
        Some(Value::Array(r)) => r
            .into_iter()
            .filter_map(|x| x.as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    };
    for key in ["allOf", "anyOf", "oneOf"] {
        if let Some(Value::Array(alts)) = o.remove(key) {
            for alt in alts {
                if let Some(p) = alt.get("properties").and_then(Value::as_object) {
                    for (k, val) in p {
                        props.entry(k.clone()).or_insert_with(|| val.clone());
                    }
                }
                // Only allOf's requirements hold for every input.
                if key == "allOf"
                    && let Some(r) = alt.get("required").and_then(Value::as_array)
                {
                    required.extend(r.iter().filter_map(|x| x.as_str().map(str::to_owned)));
                }
            }
        }
    }
    o.remove("not");
    o.insert("type".into(), json!("object"));
    required.retain(|r| props.contains_key(r));
    required.dedup();
    o.insert("properties".into(), Value::Object(props));
    if !required.is_empty() {
        o.insert("required".into(), json!(required));
    }
    v
}

/// Caps the text of a tool result.
fn cap_text(content: &mut [ContentBlock]) {
    let mut left = MAX_RESULT_CHARS;
    for b in content.iter_mut() {
        if let ContentBlock::Text { text } = b {
            let n = text.chars().count();
            if n > left {
                let kept: String = text.chars().take(left).collect();
                *text = format!(
                    "{kept}\n\n[Output cut: {} more characters weren't shown.]",
                    n - left
                );
                left = 0;
            } else {
                left -= n;
            }
        }
    }
}

/// A short label for the usage log: the query, URL or path the call was about.
pub fn usage_label(entry_tool: &str, input: &Value) -> Option<String> {
    let s = |k: &str| {
        input
            .get(k)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_owned)
    };
    match entry_tool {
        WEB_SEARCH => s("query"),
        WEB_FETCH => s("url"),
        PROJECT_SEARCH => s("query"),
        ARTIFACT_TOOL => s("title").or_else(|| s("id")),
        LOAD_SKILL => s("name").map(|n| match s("path") {
            Some(p) => format!("{n} · {p}"),
            None => n,
        }),
        SAVE_SKILL => match s("skill") {
            Some(k) => Some(format!("Updated {k}")),
            None => s("name").map(|n| format!("Created {}", oxide_core::extension::skill_slug(&n))),
        },
        _ => [
            "path", "file", "filename", "url", "uri", "query", "q", "name", "title", "command",
            "repoName",
        ]
        .iter()
        .find_map(|k| s(k))
        .or_else(|| {
            input.as_object().and_then(|o| {
                o.values().find_map(|v| {
                    v.as_str()
                        .filter(|x| !x.trim().is_empty())
                        .map(str::to_owned)
                })
            })
        }),
    }
    .map(|l| l.chars().take(200).collect())
}

/// A readable summary of a provider tool result (Anthropic `web_search_tool_result`).
pub fn provider_result_summary(raw_json: &str) -> (Vec<ContentBlock>, bool) {
    let v: Value = serde_json::from_str(raw_json).unwrap_or(Value::Null);
    match &v["content"] {
        Value::Array(items) => {
            let mut s = String::new();
            for (i, r) in items.iter().enumerate() {
                let title = r["title"].as_str().unwrap_or("");
                let url = r["url"].as_str().unwrap_or("");
                s.push_str(&format!("{}. {title}\n   {url}\n", i + 1));
            }
            if s.is_empty() {
                s = "No results.".into();
            }
            (vec![ContentBlock::text(s)], false)
        }
        Value::Object(o) => {
            let code = o
                .get("error_code")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            (
                vec![ContentBlock::text(format!("The search failed ({code})."))],
                true,
            )
        }
        _ => (vec![ContentBlock::text("(no result)")], false),
    }
}

/// `server` value for provider-run tools.
pub fn provider_server() -> String {
    PROVIDER_SERVER.to_owned()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use oxide_core::McpToolAnnotations;
    use oxide_store::MemorySecrets;
    use std::sync::Mutex;

    /// MCP servers that answer from a script.
    #[derive(Default)]
    pub struct FakeMcp {
        pub tools: Vec<McpTool>,
        pub calls: Mutex<Vec<(String, String, Value)>>,
        /// Delay for calls, to test stopping and parallel runs.
        pub delay_ms: u64,
        /// Pads every result to this many characters (0: no padding).
        pub result_chars: usize,
    }

    pub fn tool(server: &str, name: &str, read_only: bool) -> McpTool {
        McpTool {
            server: server.into(),
            name: name.into(),
            title: None,
            description: Some(format!("{name} does things")),
            input_schema: r#"{"type":"object","properties":{"path":{"type":"string"}}}"#.into(),
            annotations: McpToolAnnotations {
                read_only_hint: Some(read_only),
                ..McpToolAnnotations::default()
            },
        }
    }

    impl McpTools for FakeMcp {
        fn tools(&self) -> Vec<McpTool> {
            self.tools.clone()
        }
        fn call<'a>(
            &'a self,
            server: &'a str,
            tool: &'a str,
            arguments: Value,
            _progress: Option<String>,
        ) -> BoxFuture<'a, Result<McpToolResult, String>> {
            Box::pin(async move {
                if self.delay_ms > 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(self.delay_ms)).await;
                }
                self.calls.lock().expect("lock").push((
                    server.into(),
                    tool.into(),
                    arguments.clone(),
                ));
                if tool == "broken" {
                    return Err("the server crashed".into());
                }
                let mut text = format!("{tool} ran with {arguments}");
                while text.len() < self.result_chars {
                    text.push_str(" lorem");
                }
                Ok(McpToolResult {
                    content: vec![McpContent::Text { text }],
                    structured: None,
                    is_error: tool == "fails",
                    duration_ms: 1,
                })
            })
        }
    }

    #[test]
    fn names_are_api_safe_and_unique() {
        assert_eq!(
            api_name("my server.v2", "read file"),
            "my_server_v2__read_file"
        );
        let long = "x".repeat(80);
        assert_eq!(api_name(&long, "t").len(), 64);
        let mut taken = HashSet::new();
        taken.insert("a__b".to_owned());
        assert_eq!(unique_name("a__b", &taken), "a__b_2");
        taken.insert("a__b_2".to_owned());
        assert_eq!(unique_name("a__b", &taken), "a__b_3");
    }

    #[test]
    fn schemas_become_plain_objects() {
        let s = normalize_schema(
            r#"{"$schema":"http://json-schema.org/draft-07/schema#","type":"object",
                "properties":{"a":{"type":"string"}},"required":["a","ghost"],
                "anyOf":[{"properties":{"b":{"type":"number"}},"required":["b"]}],
                "allOf":[{"properties":{"c":{"type":"boolean"}},"required":["c"]}]}"#,
        );
        assert_eq!(s["type"], "object");
        assert!(s.get("$schema").is_none() && s.get("anyOf").is_none());
        assert_eq!(s["properties"]["b"]["type"], "number");
        assert_eq!(s["required"], json!(["a", "c"]));
        assert_eq!(
            normalize_schema("not json"),
            json!({"type":"object","properties":{}})
        );
        assert_eq!(normalize_schema("[1]")["type"], "object");
    }

    #[tokio::test]
    async fn tool_sets_follow_chat_switches_and_settings() {
        let mcp = Arc::new(FakeMcp {
            tools: vec![
                tool("fs", "read", true),
                tool("fs", "write", false),
                tool("web search", "web_fetch", true),
            ],
            ..FakeMcp::default()
        });
        let tools = Tools::new(Some(mcp), Arc::new(MemorySecrets::default()), None);
        let settings = AppSettings::default();
        let mut chat = ChatTools::default();

        let set = tools.tool_set(&chat, &settings, ProviderKind::Strata).await;
        let names: Vec<&str> = set.entries.iter().map(|e| e.api_name.as_str()).collect();
        // No search service set up: no web_search; fetch is always there.
        assert_eq!(
            names,
            vec![
                "web_fetch",
                "artifact",
                "fs__read",
                "fs__write",
                "web_search__web_fetch"
            ]
        );
        assert!(!set.can_search());
        assert!(set.can_make_artifacts());
        let read = set.find("fs__read").expect("read");
        assert!(read.read_only && read.description.starts_with("[fs] "));
        assert_eq!(read.grant_key(), "fs/read");
        assert_eq!(
            set.servers(),
            vec!["fs".to_owned(), "web search".to_owned()]
        );

        // Anthropic gets its own search.
        let set = tools
            .tool_set(&chat, &settings, ProviderKind::Anthropic)
            .await;
        assert!(set.can_search());
        assert_eq!(set.server_tools[0]["type"], "web_search_20250305");

        chat.disabled_servers = vec!["web search".into()];
        chat.disabled_tools = vec!["fs/write".into()];
        chat.disabled_builtins = vec![WEB_FETCH.into(), WEB_SEARCH.into(), ARTIFACT_TOOL.into()];
        let set = tools
            .tool_set(&chat, &settings, ProviderKind::Anthropic)
            .await;
        let names: Vec<&str> = set.entries.iter().map(|e| e.api_name.as_str()).collect();
        assert_eq!(names, vec!["fs__read"]);
        assert!(set.server_tools.is_empty());
    }

    #[tokio::test]
    async fn mcp_results_become_blocks() {
        let mcp = Arc::new(FakeMcp::default());
        let tools = Tools::new(Some(mcp.clone()), Arc::new(MemorySecrets::default()), None);
        let settings = AppSettings::default();
        let entry = ToolEntry {
            api_name: "fs__fails".into(),
            kind: ToolKind::Mcp {
                server: "fs".into(),
                tool: "fails".into(),
            },
            title: None,
            description: String::new(),
            schema: json!({}),
            read_only: false,
            destructive: false,
        };
        let out = tools
            .run(&entry, &json!({"path": "a"}), "t1", &settings)
            .await;
        assert!(out.is_error);
        assert_eq!(
            out.content,
            vec![ContentBlock::text("fails ran with {\"path\":\"a\"}")]
        );

        let mut broken = entry.clone();
        broken.kind = ToolKind::Mcp {
            server: "fs".into(),
            tool: "broken".into(),
        };
        let out = tools.run(&broken, &json!({}), "t2", &settings).await;
        assert!(out.is_error);
        assert!(plain(&out).contains("the server crashed"));

        let blocks = tools
            .mcp_blocks(McpToolResult {
                content: vec![
                    McpContent::Image {
                        data: "aGk=".into(),
                        mime_type: "image/png".into(),
                    },
                    McpContent::Resource {
                        uri: "file:///a".into(),
                        mime_type: None,
                        text: Some("body".into()),
                        blob: None,
                    },
                ],
                structured: None,
                is_error: false,
                duration_ms: 0,
            })
            .await;
        assert!(matches!(blocks[0], ContentBlock::Image { .. }));
        assert_eq!(
            blocks[1],
            ContentBlock::text("<resource uri=\"file:///a\">\nbody\n</resource>")
        );
    }

    fn plain(o: &ToolOutput) -> String {
        oxide_core::message::plain_text(&o.content)
    }

    #[test]
    fn labels_and_caps() {
        assert_eq!(
            usage_label(WEB_SEARCH, &json!({"query": " rust "})).as_deref(),
            Some("rust")
        );
        assert_eq!(
            usage_label(
                "read_file",
                &json!({"encoding": "utf8", "path": "C:/a.txt"})
            )
            .as_deref(),
            Some("C:/a.txt")
        );
        assert_eq!(usage_label("x", &json!({"n": 1})), None);
        let mut c = vec![ContentBlock::text("a".repeat(MAX_RESULT_CHARS + 5))];
        cap_text(&mut c);
        assert!(oxide_core::message::plain_text(&c).ends_with("5 more characters weren't shown.]"));
        let (blocks, err) = provider_result_summary(
            r#"{"type":"web_search_tool_result","content":{"type":"web_search_tool_result_error","error_code":"max_uses_exceeded"}}"#,
        );
        assert!(err && oxide_core::message::plain_text(&blocks).contains("max_uses_exceeded"));
    }
}
