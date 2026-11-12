//! Writing replies: one generation per chat at a time, with the tool loop.
//!
//! A reply is one assistant message. When the model calls tools, the loop runs them (asking
//! the user first where needed), adds the results to the same message and asks the model
//! again, until it answers without tools, the user stops it, or the step limit is reached.
//! The message's blocks keep the order things happened in: thinking, text, tool calls,
//! their results, more text.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use futures_util::future::join_all;
use oxide_core::RUN_PYTHON;
use oxide_core::browser::{BROWSER_SWITCH, browser_tool_reads_only};
use oxide_core::conversation::title_from_text;
use oxide_core::message::plain_text;
use oxide_core::model::split_model_id;
use oxide_core::sandbox::SANDBOX_SWITCH;
use oxide_core::tools::PROVIDER_SERVER;
use oxide_core::workspace::{FILES_SWITCH, RUN_COMMAND, TASK_LIST, command_allowed};
use oxide_core::{
    AppSettings, ApprovalDecision, ChatStreamEvent, ContentBlock, Conversation, GenerateOptions,
    Message, ModelCapabilities, ProviderKind, Role, StopReason, StreamEvent, TaskList,
    ThinkingStyle, ToolApprovalRequest, ToolUsageKind, Usage,
};
use oxide_llm::capabilities::anthropic_curated;
use oxide_llm::{ChatEvent, ChatMessage, ChatRequest, Provider, Thinking, complete_text};
use oxide_store::{NewReply, NewToolUsage, Store, new_id, now_ms};
use serde_json::Value;
use tokio::sync::{oneshot, watch};

use crate::AgentError;
use crate::artifacts::run_artifact;
use crate::context::{
    PromptContext, SandboxPrompt, build_messages, plan_request, request_messages, system_prompt,
};
use crate::knowledge::{
    KnowledgePlan, chat_budget, plan_knowledge, project_search_entry, run_project_search,
};
use crate::memory::{MEMORY_SYSTEM, Scope, memory_prompt, parse_proposals};
use crate::registry::ModelRegistry;
use crate::title::{clean_title, pick_title_model, title_prompt};
use crate::tools::browser as browser_tools;
use crate::tools::files::{FileOp, Sandbox, file_entry, file_label};
use crate::tools::python::{python_entry, run_python};
use crate::tools::sandbox as sandbox_tools;
use crate::tools::shell::{Shell, command_of, run_command, shell_entry};
use crate::tools::{
    Builtin, ToolEntry, ToolKind, ToolOutput, ToolSet, Tools, provider_result_summary, usage_label,
};

/// How often buffered text is sent to the UI while a reply streams.
const FLUSH_EVERY: Duration = Duration::from_millis(40);
/// Longest wait for an automatic title.
const TITLE_TIMEOUT: Duration = Duration::from_secs(30);
/// Longest wait for memory items after a reply.
const MEMORY_TIMEOUT: Duration = Duration::from_secs(60);

const STOPPED_NOTE: &str = "The user stopped the reply before this tool ran.";
const FAILED_NOTE: &str = "The reply ended before this tool ran.";
const DENIED_NOTE: &str = "The user declined to run this tool call. Don't try it again unless the \
                           user asks; continue without it or ask how they'd like to proceed.";

/// Receives reply events. The app crate forwards them to the window.
pub trait EventSink: Send + Sync + 'static {
    fn stream_event(&self, event: ChatStreamEvent);
    /// A chat changed in the background (its automatic title).
    fn conversation_changed(&self, conversation: Conversation);
    /// `added` memory items were learned in the background.
    fn memory_changed(&self, added: u32) {
        let _ = added;
    }
    /// The model set a chat's task list.
    fn tasks_changed(&self, list: TaskList) {
        let _ = list;
    }
}

struct Running {
    cancel: watch::Sender<bool>,
}

struct PendingApproval {
    request: ToolApprovalRequest,
    answer: oneshot::Sender<ApprovalDecision>,
}

/// The window's clock (time zone), for the system prompt.
#[derive(Debug, Clone, Default)]
struct ClientClock {
    utc_offset_minutes: Option<i32>,
    time_zone: Option<String>,
}

pub struct ChatService {
    store: Store,
    registry: Arc<ModelRegistry>,
    sink: Arc<dyn EventSink>,
    tools: Tools,
    running: Mutex<HashMap<String, Running>>,
    /// Tool calls waiting for the user, by tool call id.
    approvals: Mutex<HashMap<String, PendingApproval>>,
    clock: Mutex<ClientClock>,
}

/// How a generation ended.
enum Outcome {
    Finished(StopReason),
    Stopped,
    Failed(String),
}

/// Everything one reply's requests share.
struct Setup {
    provider: Arc<dyn Provider>,
    model: String,
    kind: ProviderKind,
    caps: ModelCapabilities,
    max_output: Option<u32>,
    settings: AppSettings,
    tool_set: ToolSet,
    system: String,
    /// Project knowledge that goes ahead of the system prompt (cached on Anthropic).
    system_context: Option<String>,
    /// The chat's project (for `project_search`).
    project_id: Option<String>,
    /// "Allow always for this chat" keys.
    grants: HashSet<String>,
    /// The chat's granted folders, resolved (file tools and the shell are confined to them).
    sandbox: Sandbox,
    /// The shell `run_command` uses (`None`: the shell tool is off).
    shell: Option<Shell>,
    /// The model `browser_task` runs with (the browser is on).
    browser_llm: Option<oxide_pyenv::BrowserLlm>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl ChatService {
    pub fn new(
        store: Store,
        registry: Arc<ModelRegistry>,
        sink: Arc<dyn EventSink>,
        tools: Tools,
    ) -> Arc<Self> {
        Arc::new(Self {
            store,
            registry,
            sink,
            tools,
            running: Mutex::new(HashMap::new()),
            approvals: Mutex::new(HashMap::new()),
            clock: Mutex::new(ClientClock::default()),
        })
    }

    pub fn tools(&self) -> &Tools {
        &self.tools
    }

    fn running(&self) -> std::sync::MutexGuard<'_, HashMap<String, Running>> {
        lock(&self.running)
    }

    /// The window's time zone, used for the date and time in the system prompt.
    pub fn set_client_clock(&self, utc_offset_minutes: i32, time_zone: Option<String>) {
        *lock(&self.clock) = ClientClock {
            utc_offset_minutes: Some(utc_offset_minutes.clamp(-14 * 60, 14 * 60)),
            time_zone: time_zone
                .map(|t| t.trim().chars().take(64).collect())
                .filter(|t: &String| !t.is_empty()),
        };
    }

    /// Whether a reply is being written in `conversation_id`.
    pub fn is_running(&self, conversation_id: &str) -> bool {
        self.running().contains_key(conversation_id)
    }

    /// Chats with a reply in progress.
    pub fn running_chats(&self) -> Vec<String> {
        self.running().keys().cloned().collect()
    }

    /// Stops the reply in `conversation_id`. What was written so far is kept. Returns
    /// whether anything was running.
    pub fn stop(&self, conversation_id: &str) -> bool {
        match self.running().get(conversation_id) {
            Some(r) => {
                r.cancel.send_replace(true);
                true
            }
            None => false,
        }
    }

    /// The chat is being deleted: stops its reply and drops its waiting tool calls at once,
    /// so nothing keeps asking for a chat that's gone.
    pub fn forget(&self, conversation_id: &str) {
        self.stop(conversation_id);
        lock(&self.approvals).retain(|_, p| p.request.conversation_id != conversation_id);
        // Its Python working folder goes too.
        if let Some(python) = self.tools.python().cloned()
            && let Ok(rt) = tokio::runtime::Handle::try_current()
        {
            let id = conversation_id.to_owned();
            rt.spawn(async move { python.forget_chat(&id).await });
        }
        // Its browser.
        if let Some(b) = self.tools.browser().cloned()
            && let Ok(rt) = tokio::runtime::Handle::try_current()
        {
            let id = conversation_id.to_owned();
            rt.spawn(async move { b.forget(&id).await });
        }
        // And its sandbox, with the files in it.
        if let Some(sb) = self.tools.sandboxes().cloned()
            && let Ok(rt) = tokio::runtime::Handle::try_current()
        {
            let id = conversation_id.to_owned();
            rt.spawn(async move { sb.forget(&id).await });
        }
    }

    /// Tool calls waiting for the user (all chats), oldest first.
    pub fn pending_approvals(&self) -> Vec<ToolApprovalRequest> {
        let mut v: Vec<ToolApprovalRequest> = lock(&self.approvals)
            .values()
            .map(|p| p.request.clone())
            .collect();
        v.sort_by_key(|r| r.requested_at);
        v
    }

    /// Answers a waiting tool call. Returns whether it was still waiting.
    pub fn answer_approval(
        &self,
        conversation_id: &str,
        tool_use_id: &str,
        decision: ApprovalDecision,
    ) -> bool {
        let mut map = lock(&self.approvals);
        match map.get(tool_use_id) {
            Some(p) if p.request.conversation_id == conversation_id => {}
            _ => return false,
        }
        let Some(p) = map.remove(tool_use_id) else {
            return false;
        };
        if decision == ApprovalDecision::AllowChat {
            // "Allow always" also answers the same tool's other waiting cards in this chat.
            let same: Vec<String> = map
                .iter()
                .filter(|(_, o)| {
                    o.request.conversation_id == conversation_id
                        && o.request.server == p.request.server
                        && o.request.tool == p.request.tool
                        && (p.request.server.is_some()
                            || p.request.tool != RUN_COMMAND
                            || o.request.input_json == p.request.input_json)
                })
                .map(|(id, _)| id.clone())
                .collect();
            for id in same {
                if let Some(o) = map.remove(&id) {
                    let _ = o.answer.send(ApprovalDecision::AllowOnce);
                }
            }
        }
        p.answer.send(decision).is_ok()
    }

    /// Saves a user message under `parent_id` and starts the reply.
    pub async fn send(
        self: &Arc<Self>,
        conversation_id: &str,
        parent_id: Option<&str>,
        content: Vec<ContentBlock>,
        options: GenerateOptions,
    ) -> Result<Message, AgentError> {
        if self.is_running(conversation_id) {
            return Err(AgentError::Busy);
        }
        let message = self
            .store
            .add_message(conversation_id, parent_id, Role::User, content, None)
            .await?;
        self.reply(conversation_id, &message.id, options).await?;
        Ok(message)
    }

    /// Edits a user message (as a new branch) and answers the edited version.
    pub async fn edit_and_resend(
        self: &Arc<Self>,
        message_id: &str,
        text: &str,
        options: GenerateOptions,
    ) -> Result<Message, AgentError> {
        let original = self.store.get_message(message_id).await?;
        if self.is_running(&original.conversation_id) {
            return Err(AgentError::Busy);
        }
        let edited = self.store.edit_user_message(message_id, text).await?;
        self.reply(&edited.conversation_id, &edited.id, options)
            .await?;
        Ok(edited)
    }

    /// Writes a new reply next to `message_id`: a sibling of an assistant message, or a
    /// (first or further) answer to a user message.
    pub async fn regenerate(
        self: &Arc<Self>,
        message_id: &str,
        options: GenerateOptions,
    ) -> Result<String, AgentError> {
        let m = self.store.get_message(message_id).await?;
        let user_id = match m.role {
            Role::User => m.id,
            Role::Assistant => m
                .parent_id
                .ok_or_else(|| AgentError::Invalid("this reply has no question".into()))?,
        };
        self.reply(&m.conversation_id, &user_id, options).await
    }

    /// Starts answering the user message `user_message_id`. Returns the id the reply will be
    /// saved under; progress arrives through the sink.
    pub async fn reply(
        self: &Arc<Self>,
        conversation_id: &str,
        user_message_id: &str,
        options: GenerateOptions,
    ) -> Result<String, AgentError> {
        let conversation = self.store.get_conversation(conversation_id).await?;
        let messages = self.store.list_messages(conversation_id).await?;
        let thread = path_to(&messages, user_message_id).ok_or(AgentError::NotFound("message"))?;
        if thread.last().map(|m| m.role) != Some(Role::User) {
            return Err(AgentError::Invalid(
                "only your messages can be answered".into(),
            ));
        }

        let reply_id = new_id();
        let (cancel, cancelled) = watch::channel(false);
        {
            let mut running = self.running();
            if running.contains_key(conversation_id) {
                return Err(AgentError::Busy);
            }
            running.insert(conversation_id.to_owned(), Running { cancel });
        }
        self.emit(
            conversation_id,
            StreamEvent::Started {
                message_id: reply_id.clone(),
                parent_id: user_message_id.to_owned(),
                model: conversation.model.clone(),
                started_at: now_ms(),
            },
        );

        let this = Arc::clone(self);
        let id = reply_id.clone();
        tokio::spawn(async move {
            this.generate(conversation, thread, id, options, cancelled)
                .await;
        });
        Ok(reply_id)
    }

    /// Marks the chat idle, then sends the final event, so a reply can be started as soon
    /// as the UI hears this one is over.
    fn finish(&self, conversation_id: &str, event: StreamEvent) {
        self.running().remove(conversation_id);
        lock(&self.approvals).retain(|_, p| p.request.conversation_id != conversation_id);
        self.emit(conversation_id, event);
    }

    fn emit(&self, conversation_id: &str, event: StreamEvent) {
        self.sink.stream_event(ChatStreamEvent {
            conversation_id: conversation_id.to_owned(),
            event,
        });
    }

    async fn generate(
        self: &Arc<Self>,
        conversation: Conversation,
        thread: Vec<Message>,
        reply_id: String,
        options: GenerateOptions,
        mut cancelled: watch::Receiver<bool>,
    ) {
        let cid = conversation.id.clone();
        let parent_id = thread.last().map(|m| m.id.clone()).unwrap_or_default();
        let mut reply = ReplyBuilder::default();
        let mut provider_label = String::new();

        let outcome = match self
            .run(
                &conversation,
                &thread,
                &reply_id,
                options,
                &mut reply,
                &mut provider_label,
                &mut cancelled,
            )
            .await
        {
            Ok(o) => o,
            Err(e) => Outcome::Failed(e.to_string()),
        };

        let (stop_reason, error) = match outcome {
            Outcome::Finished(r) => (r, None),
            Outcome::Stopped => (StopReason::Stopped, None),
            Outcome::Failed(e) => {
                let e = if provider_label.is_empty() {
                    e
                } else {
                    format!("{provider_label}: {e}")
                };
                (StopReason::Error, Some(e))
            }
        };
        // Calls that never ran get a result, so the reply stays a valid history.
        let note = if stop_reason == StopReason::Stopped {
            STOPPED_NOTE
        } else {
            FAILED_NOTE
        };
        for (id, out) in reply.close_open_calls(note) {
            self.emit(
                &cid,
                StreamEvent::ToolResult {
                    id,
                    content: out.content,
                    is_error: true,
                    duration_ms: 0,
                },
            );
        }
        let usage = reply.total_usage();
        let content = reply.take_content();
        let keep = !content.is_empty()
            || matches!(
                stop_reason,
                StopReason::EndTurn | StopReason::Refusal | StopReason::MaxTokens
            );
        let saved = if keep {
            let saved = self
                .store
                .add_reply(NewReply {
                    id: reply_id,
                    conversation_id: cid.clone(),
                    parent_id,
                    content,
                    model: conversation.model.clone(),
                    stop_reason,
                    usage,
                })
                .await;
            match saved {
                Ok(m) => Some(m),
                Err(e) => {
                    tracing::error!("couldn't save the reply: {e}");
                    self.finish(
                        &cid,
                        StreamEvent::Failed {
                            error: "The reply couldn't be saved.".into(),
                            message: None,
                        },
                    );
                    return;
                }
            }
        } else {
            None
        };

        match error {
            Some(error) => {
                tracing::info!(conversation = %cid, "reply failed: {error}");
                self.finish(
                    &cid,
                    StreamEvent::Failed {
                        error,
                        message: saved,
                    },
                );
            }
            None => {
                let first_exchange = saved.is_some()
                    && stop_reason != StopReason::Stopped
                    && thread.iter().filter(|m| m.role == Role::User).count() == 1;
                self.finish(
                    &cid,
                    StreamEvent::Done {
                        message: saved.clone(),
                    },
                );
                if first_exchange
                    && let (Some(first), Some(reply)) = (thread.first(), saved.as_ref())
                {
                    self.auto_title(&cid, first, reply).await;
                }
                if stop_reason == StopReason::EndTurn
                    && let (Some(user), Some(reply)) = (thread.last(), saved.as_ref())
                {
                    self.learn_memory(&cid, user, reply).await;
                }
            }
        }
    }

    /// Resolves the model, its endpoint and the chat's tools.
    async fn setup(
        &self,
        conversation: &Conversation,
        provider_label: &mut String,
    ) -> Result<Setup, AgentError> {
        let (provider_id, model) = split_model_id(&conversation.model).ok_or_else(|| {
            AgentError::Invalid(format!(
                "unknown model “{}”. Pick a model in the composer.",
                conversation.model
            ))
        })?;
        let (config, provider) =
            self.registry
                .provider(provider_id)
                .await
                .map_err(|e| {
                    match e {
                AgentError::Store(oxide_store::StoreError::NotFound { .. }) => AgentError::Invalid(
                    "this chat's endpoint no longer exists. Pick another model in the composer."
                        .into(),
                ),
                other => other,
            }
                })?;
        *provider_label = config.name.clone();
        let info = self.registry.find(&conversation.model).await;
        let caps = match (&info, config.kind) {
            (Some(i), _) => i.capabilities.clone(),
            (None, ProviderKind::Anthropic) => anthropic_curated(model),
            (None, _) => ModelCapabilities::default(),
        };
        let settings = self.store.app_settings().await?;
        let chat_tools = self.store.chat_tools(&conversation.id).await?;
        let mut tool_set = if caps.tools {
            self.tools
                .tool_set(&chat_tools, &settings, config.kind)
                .await
        } else {
            ToolSet::default()
        };
        let grants: HashSet<String> = self
            .store
            .chat_tool_grants(&conversation.id)
            .await?
            .into_iter()
            .collect();
        // Agentic work: the task list, and file tools (and the shell) in granted folders.
        let workspace = self.store.chat_workspace(&conversation.id).await?;
        let sandbox = tokio::task::spawn_blocking(move || Sandbox::new(&workspace))
            .await
            .unwrap_or_default();
        let mut shell = None;
        let mut sandbox_prompt = None;
        let mut browser_llm = None;
        let mut browser_on = false;
        if caps.tools {
            let at = tool_set
                .entries
                .iter()
                .position(|e| matches!(e.kind, ToolKind::Mcp { .. }))
                .unwrap_or(tool_set.entries.len());
            let mut own = Vec::new();
            if chat_tools.allows_builtin(TASK_LIST) {
                own.push(crate::tasks::task_list_entry());
            }
            if !sandbox.is_empty() && chat_tools.allows_builtin(FILES_SWITCH) {
                for op in FileOp::ALL {
                    if chat_tools.allows_builtin(op.name()) {
                        own.push(file_entry(op, sandbox.allows_delete()));
                    }
                }
            }
            if settings.shell.enabled
                && !sandbox.is_empty()
                && chat_tools.allows_builtin(RUN_COMMAND)
            {
                let sh = tokio::task::spawn_blocking(Shell::detect)
                    .await
                    .unwrap_or_else(|_| Shell::detect());
                own.push(shell_entry(&sh));
                shell = Some(sh);
            }
            if settings.python.enabled
                && self.tools.python().is_some()
                && chat_tools.allows_builtin(RUN_PYTHON)
            {
                own.push(python_entry());
            }
            // The Docker sandbox. `sandbox_exec` is offered even while Docker is down, so a
            // request to use it ends in Docker's own message instead of a missing tool.
            if settings.sandbox.enabled
                && chat_tools.allows_builtin(SANDBOX_SWITCH)
                && let Some(sb) = self.tools.sandboxes()
            {
                sb.configure(&settings.sandbox);
                own.push(sandbox_tools::exec_entry());
                let copies = !sandbox.is_empty();
                if copies {
                    own.push(sandbox_tools::copy_in_entry());
                    own.push(sandbox_tools::copy_out_entry());
                }
                let files = match sb.file_tools() {
                    Some(t) => Some(t),
                    None => tokio::time::timeout(Duration::from_secs(20), sb.discover_file_tools())
                        .await
                        .ok()
                        .and_then(Result::ok),
                };
                if let Some(t) = &files {
                    own.extend(sandbox_tools::file_entries(t));
                }
                sandbox_prompt = Some(SandboxPrompt {
                    image: settings.sandbox.image.clone(),
                    network: settings.sandbox.network,
                    files: files.is_some_and(|t| !t.is_empty()),
                    copies,
                });
            }
            // The browser agent: its tools, and the model its `browser_task` uses.
            if settings.browser.enabled
                && chat_tools.allows_builtin(BROWSER_SWITCH)
                && let Some(b) = self.tools.browser()
            {
                b.configure(&settings.browser);
                own.extend(browser_tools::browser_entries());
                browser_on = true;
                let model_id = settings
                    .browser
                    .agent_model
                    .clone()
                    .unwrap_or_else(|| conversation.model.clone());
                browser_llm = self.browser_llm(&model_id).await;
            }
            tool_set.entries.splice(at..at, own);
        }
        let project = match &conversation.project_id {
            Some(p) => self.store.get_project(p).await.ok(),
            None => None,
        };
        // Knowledge: in full when it fits the budget, else searched with `project_search`.
        let mut system_context = None;
        let mut knowledge_search = Vec::new();
        if let Some(p) = &project {
            let files = self.store.knowledge_texts(&p.id).await?;
            let budget = chat_budget(
                settings.projects.knowledge_budget,
                info.as_ref().and_then(|i| i.context_window),
            );
            match plan_knowledge(&files, budget) {
                KnowledgePlan::None => {}
                KnowledgePlan::Full(block) => system_context = Some(block),
                KnowledgePlan::Search(names) => {
                    if caps.tools {
                        tool_set.entries.insert(0, project_search_entry());
                    }
                    knowledge_search = names;
                }
            }
        }
        let memory = if settings.memory.enabled {
            let items = self
                .store
                .memory_for(conversation.project_id.as_deref())
                .await?;
            oxide_core::memory::memory_prompt(&items)
        } else {
            None
        };
        let skills = crate::skills::skills_prompt(&tool_set.skills);
        let clock = lock(&self.clock).clone();
        let system = system_prompt(&PromptContext {
            user_name: &settings.profile.name,
            now_ms: now_ms(),
            utc_offset_minutes: clock.utc_offset_minutes,
            time_zone: clock.time_zone.as_deref(),
            os: std::env::consts::OS,
            web_search: tool_set.can_search(),
            web_fetch: tool_set.can_fetch(),
            connectors: tool_set.servers(),
            artifacts: tool_set.can_make_artifacts(),
            project_name: project.as_ref().map(|p| p.name.as_str()),
            project_instructions: project.as_ref().map(|p| p.instructions.as_str()),
            knowledge_search,
            memory: memory.as_deref(),
            skills: skills.as_deref(),
            folders: if tool_set
                .entries
                .iter()
                .any(|e| matches!(e.kind, ToolKind::Builtin(Builtin::File(_))))
            {
                sandbox.root_paths()
            } else {
                Vec::new()
            },
            delete_for_good: sandbox.allows_delete(),
            shell: shell.as_ref().map(|s| s.label.as_str()),
            python: tool_set
                .entries
                .iter()
                .any(|e| e.kind == ToolKind::Builtin(Builtin::RunPython)),
            task_list: tool_set
                .entries
                .iter()
                .any(|e| e.kind == ToolKind::Builtin(Builtin::TaskList)),
            sandbox: sandbox_prompt,
            browser: browser_on,
        });
        Ok(Setup {
            provider,
            model: model.to_owned(),
            kind: config.kind,
            caps,
            max_output: info.as_ref().and_then(|i| i.max_output_tokens),
            settings,
            tool_set,
            system,
            system_context,
            project_id: conversation.project_id.clone(),
            grants,
            sandbox,
            shell,
            browser_llm,
        })
    }

    /// The endpoint, key and abilities of a model, for Browser-Use. `None` when the model's
    /// endpoint is gone (`browser_task` then says it has no model).
    async fn browser_llm(&self, model_id: &str) -> Option<oxide_pyenv::BrowserLlm> {
        let (provider_id, model) = split_model_id(model_id)?;
        let (config, api_key) = match self.registry.endpoint(provider_id).await {
            Ok(e) => e,
            Err(e) => {
                tracing::info!("no endpoint for the browser's model: {e}");
                return None;
            }
        };
        let vision = match (self.registry.find(model_id).await, config.kind) {
            (Some(i), _) => i.capabilities.images,
            (None, ProviderKind::Anthropic) => anthropic_curated(model).images,
            (None, _) => false,
        };
        Some(oxide_pyenv::BrowserLlm {
            kind: config.kind,
            base_url: config.base_url,
            model: model.to_owned(),
            api_key,
            vision,
        })
    }

    /// The tool loop: model, tools, model, … until the model is done.
    #[allow(clippy::too_many_arguments)]
    async fn run(
        &self,
        conversation: &Conversation,
        thread: &[Message],
        reply_id: &str,
        options: GenerateOptions,
        reply: &mut ReplyBuilder,
        provider_label: &mut String,
        cancelled: &mut watch::Receiver<bool>,
    ) -> Result<Outcome, AgentError> {
        let cid = conversation.id.as_str();
        let mut setup = self.setup(conversation, provider_label).await?;
        let history = build_messages(thread);
        let max_steps = setup.settings.tools.max_steps;
        let mut steps = 0u32;
        loop {
            let mut request = ChatRequest::new(
                setup.model.clone(),
                request_messages(&history, &reply.blocks),
            );
            request.system = Some(setup.system.clone());
            request.system_context = setup.system_context.clone();
            plan_request(
                &mut request,
                setup.kind,
                &setup.caps,
                setup.max_output,
                options,
                setup.settings.models.thinking_budget,
            );
            request.tools = setup.tool_set.specs();
            request.server_tools = setup.tool_set.server_tools.clone();

            reply.begin_step();
            let outcome = self
                .stream_step(cid, reply_id, &setup, request, reply, cancelled)
                .await?;
            let open = reply.open_calls();
            match outcome {
                // Some local models end with "stop" after calling tools.
                Outcome::Finished(StopReason::ToolUse | StopReason::EndTurn)
                    if !open.is_empty() =>
                {
                    steps += 1;
                    if steps > max_steps {
                        let note = format!(
                            "Oxide stopped here: this reply reached the limit of {max_steps} tool \
                             steps (Settings → Permissions)."
                        );
                        for (id, out) in reply.close_open_calls(&note) {
                            self.emit(
                                cid,
                                StreamEvent::ToolResult {
                                    id,
                                    content: out.content,
                                    is_error: true,
                                    duration_ms: 0,
                                },
                            );
                        }
                        return Ok(Outcome::Finished(StopReason::StepLimit));
                    }
                    if !self
                        .run_calls(cid, reply_id, &mut setup, open, reply, cancelled)
                        .await
                    {
                        return Ok(Outcome::Stopped);
                    }
                }
                // The provider ran its own tools and wants to go on.
                Outcome::Finished(StopReason::PauseTurn) => {
                    steps += 1;
                    if steps > max_steps {
                        return Ok(Outcome::Finished(StopReason::StepLimit));
                    }
                }
                Outcome::Finished(StopReason::ToolUse) => {
                    return Ok(Outcome::Finished(StopReason::EndTurn));
                }
                other => return Ok(other),
            }
        }
    }

    /// One model request, streamed into `reply`.
    async fn stream_step(
        &self,
        cid: &str,
        reply_id: &str,
        setup: &Setup,
        request: ChatRequest,
        reply: &mut ReplyBuilder,
        cancelled: &mut watch::Receiver<bool>,
    ) -> Result<Outcome, AgentError> {
        let mut stream = tokio::select! {
            s = setup.provider.stream(request) => s?,
            _ = cancelled.wait_for(|c| *c) => return Ok(Outcome::Stopped),
        };
        let mut ticker = tokio::time::interval(FLUSH_EVERY);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let outcome = loop {
            let event = tokio::select! {
                _ = cancelled.wait_for(|c| *c) => break Outcome::Stopped,
                _ = ticker.tick() => {
                    self.flush(cid, reply);
                    continue;
                }
                next = stream.next() => match next {
                    None => break Outcome::Failed("the reply ended unexpectedly".into()),
                    Some(Err(e)) => break Outcome::Failed(e.to_string()),
                    Some(Ok(ChatEvent::Stop(reason))) => break Outcome::Finished(reason),
                    Some(Ok(event)) => event,
                },
            };
            let events = reply.apply(event);
            if !events.is_empty() {
                self.flush(cid, reply);
            }
            for ev in events {
                if let StreamEvent::ToolResult { id, is_error, .. } = &ev {
                    self.log_provider_tool(cid, reply_id, reply, id, *is_error)
                        .await;
                }
                self.emit(cid, ev);
            }
        };
        self.flush(cid, reply);
        Ok(outcome)
    }

    async fn log_provider_tool(
        &self,
        cid: &str,
        reply_id: &str,
        reply: &ReplyBuilder,
        id: &str,
        is_error: bool,
    ) {
        let Some((name, input)) = reply.server_call(id) else {
            return;
        };
        let label = usage_label(
            if name == "web_search" {
                oxide_core::tools::WEB_SEARCH
            } else {
                &name
            },
            &input,
        );
        let kind = if name == "web_search" {
            ToolUsageKind::WebSearch
        } else {
            ToolUsageKind::ProviderTool
        };
        self.log(
            cid,
            reply_id,
            id,
            kind,
            Some(PROVIDER_SERVER.into()),
            name,
            label,
            is_error,
        )
        .await;
    }

    #[allow(clippy::too_many_arguments)]
    async fn log(
        &self,
        cid: &str,
        reply_id: &str,
        tool_use_id: &str,
        kind: ToolUsageKind,
        server: Option<String>,
        tool: String,
        label: Option<String>,
        is_error: bool,
    ) {
        let r = self
            .store
            .log_tool_use(NewToolUsage {
                conversation_id: cid.to_owned(),
                message_id: reply_id.to_owned(),
                tool_use_id: tool_use_id.to_owned(),
                kind,
                server,
                tool,
                label,
                is_error,
            })
            .await;
        if let Err(e) = r {
            tracing::warn!("couldn't log a tool call: {e}");
        }
    }

    /// Key of "Allow always for this chat" for one call. Shell commands are allowed one
    /// command at a time, not the whole tool.
    fn grant_key(entry: &ToolEntry, input: &Value) -> String {
        match entry.kind {
            ToolKind::Builtin(Builtin::RunCommand) => format!(
                "{}:{}",
                entry.grant_key(),
                command_of(input).unwrap_or_default()
            ),
            _ => entry.grant_key(),
        }
    }

    fn needs_approval(entry: &ToolEntry, input: &Value, setup: &Setup) -> bool {
        if setup.grants.contains(&Self::grant_key(entry, input)) {
            return false;
        }
        match &entry.kind {
            // Searching the project's own files never asks.
            ToolKind::Builtin(Builtin::ProjectSearch) => false,
            // Artifacts and the task list only write into Oxide's own database.
            ToolKind::Builtin(Builtin::Artifact | Builtin::TaskList) => false,
            // Skills are Oxide's own files, only read.
            ToolKind::Builtin(Builtin::LoadSkill) => false,
            // The user granted the folder: looking is fine, changing asks.
            ToolKind::Builtin(Builtin::File(op)) => !op.read_only(),
            // Python code runs as the user, unsandboxed: each run asks until "Allow always".
            ToolKind::Builtin(Builtin::RunPython) => true,
            ToolKind::Builtin(Builtin::RunCommand) => match command_of(input) {
                Ok(c) => !command_allowed(&c, &setup.settings.shell.allowlist),
                // Runs (and fails with a message) without bothering the user.
                Err(_) => false,
            },
            // Inside the sandbox nothing reaches this computer: commands and file changes ask
            // only when Settings → Sandbox says so. Copies cross into it, so they always ask.
            ToolKind::Builtin(Builtin::SandboxExec) => setup.settings.sandbox.ask,
            ToolKind::Builtin(Builtin::SandboxCopyIn | Builtin::SandboxCopyOut) => true,
            ToolKind::SandboxFile { .. } => !entry.read_only && setup.settings.sandbox.ask,
            // Browsing acts on websites as the user: opening, clicking, typing and tasks ask
            // unless Settings → Browser says not to. Reading and screenshots never ask.
            ToolKind::Browser { tool } => {
                !browser_tool_reads_only(tool) && setup.settings.browser.ask
            }
            ToolKind::Builtin(_) => setup.settings.tools.ask_before_web,
            ToolKind::Mcp { .. } => !(entry.read_only && setup.settings.tools.auto_allow_read_only),
        }
    }

    /// Asks the user about one call. `None`: the reply was stopped while waiting.
    async fn ask(
        &self,
        cid: &str,
        reply_id: &str,
        call: &OpenCall,
        entry: &ToolEntry,
        input: &Value,
        cancelled: &mut watch::Receiver<bool>,
    ) -> Option<ApprovalDecision> {
        let request = ToolApprovalRequest {
            conversation_id: cid.to_owned(),
            message_id: reply_id.to_owned(),
            tool_use_id: call.id.clone(),
            server: entry.server().map(str::to_owned),
            tool: entry.tool().to_owned(),
            title: entry.title.clone(),
            description: Some(entry.description.clone()).filter(|d| !d.is_empty()),
            input_json: serde_json::to_string_pretty(input).unwrap_or_else(|_| input.to_string()),
            read_only: entry.read_only,
            destructive: entry.destructive,
            requested_at: now_ms(),
        };
        let (tx, rx) = oneshot::channel();
        lock(&self.approvals).insert(
            call.id.clone(),
            PendingApproval {
                request: request.clone(),
                answer: tx,
            },
        );
        self.emit(cid, StreamEvent::ToolApproval { request });
        let answer = tokio::select! {
            a = rx => Some(a.unwrap_or(ApprovalDecision::Deny)),
            _ = cancelled.wait_for(|c| *c) => None,
        };
        lock(&self.approvals).remove(&call.id);
        answer
    }

    /// Runs the calls of one step. Returns `false` if the user stopped the reply.
    ///
    /// Every call goes its own way at once: calls that need approval show their cards
    /// together and start as soon as they're allowed; the others start right away. Tools
    /// that only read run side by side; the rest take turns, so two writes never overlap.
    /// The step ends when every call has a result.
    async fn run_calls(
        &self,
        cid: &str,
        reply_id: &str,
        setup: &mut Setup,
        calls: Vec<OpenCall>,
        reply: &mut ReplyBuilder,
        cancelled: &mut watch::Receiver<bool>,
    ) -> bool {
        struct Ready {
            call: OpenCall,
            entry: ToolEntry,
            input: Value,
            ask: bool,
        }
        let mut ready: Vec<Ready> = Vec::new();
        for call in calls {
            let Some(entry) = setup.tool_set.find(&call.name).cloned() else {
                let out = ToolOutput::error(format!(
                    "There's no tool called “{}”. Use one of the tools you were given.",
                    call.name
                ));
                self.finish_call(cid, reply_id, &call, None, &Value::Null, out, reply)
                    .await;
                continue;
            };
            reply.label_call(&call.id, entry.server(), entry.tool());
            let input: Value = if call.input_json.trim().is_empty() {
                Value::Object(Default::default())
            } else {
                match serde_json::from_str(&call.input_json) {
                    Ok(v) => v,
                    Err(e) => {
                        self.emit_call(cid, &call, &entry);
                        let out = ToolOutput::error(format!(
                            "The arguments weren't valid JSON ({e}). Call the tool again with a \
                             JSON object."
                        ));
                        self.finish_call(
                            cid,
                            reply_id,
                            &call,
                            Some(&entry),
                            &Value::Null,
                            out,
                            reply,
                        )
                        .await;
                        continue;
                    }
                }
            };
            self.emit_call(cid, &call, &entry);
            let ask = Self::needs_approval(&entry, &input, setup);
            ready.push(Ready {
                call,
                entry,
                input,
                ask,
            });
        }

        let grants = Mutex::new(std::mem::take(&mut setup.grants));
        // Calls that may change something run one at a time.
        let turns = tokio::sync::Mutex::new(());
        let done: Mutex<Vec<(usize, ToolOutput)>> = Mutex::new(Vec::new());
        let settings = &setup.settings;
        let project_id = setup.project_id.as_deref();
        let sandbox = &setup.sandbox;
        let shell = setup.shell.as_ref();
        let browser_llm = setup.browser_llm.as_ref();
        let work = join_all(ready.iter().enumerate().map(|(i, r)| {
            let mut cancel = cancelled.clone();
            let (grants, turns, done) = (&grants, &turns, &done);
            async move {
                let key = Self::grant_key(&r.entry, &r.input);
                let granted = lock(grants).contains(&key);
                if r.ask && !granted {
                    match self
                        .ask(cid, reply_id, &r.call, &r.entry, &r.input, &mut cancel)
                        .await
                    {
                        None => return,
                        Some(ApprovalDecision::Deny) => {
                            let out = ToolOutput::error(DENIED_NOTE);
                            self.report(cid, reply_id, &r.call, Some(&r.entry), &r.input, &out)
                                .await;
                            lock(done).push((i, out));
                            return;
                        }
                        Some(ApprovalDecision::AllowChat) => {
                            if let Err(e) = self.store.grant_chat_tool(cid, &key).await {
                                tracing::warn!("couldn't save the approval: {e}");
                            }
                            lock(grants).insert(key);
                        }
                        Some(ApprovalDecision::AllowOnce) => {}
                    }
                }
                let _turn = if r.entry.read_only {
                    None
                } else {
                    Some(turns.lock().await)
                };
                self.emit(
                    cid,
                    StreamEvent::ToolRunning {
                        id: r.call.id.clone(),
                    },
                );
                let out = if r.entry.kind == ToolKind::Builtin(Builtin::ProjectSearch) {
                    let started = Instant::now();
                    let mut out = run_project_search(&self.store, project_id, &r.input).await;
                    out.duration_ms =
                        u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);
                    out
                } else if let ToolKind::Builtin(
                    b @ (Builtin::File(_)
                    | Builtin::TaskList
                    | Builtin::RunCommand
                    | Builtin::RunPython),
                ) = r.entry.kind
                {
                    let started = Instant::now();
                    let mut out = self
                        .run_workspace_tool(cid, reply_id, b, &r.input, sandbox, shell, settings)
                        .await;
                    out.duration_ms =
                        u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);
                    out
                } else if let ToolKind::Builtin(
                    b @ (Builtin::SandboxExec | Builtin::SandboxCopyIn | Builtin::SandboxCopyOut),
                ) = r.entry.kind
                {
                    let started = Instant::now();
                    let mut out = self
                        .run_sandbox_tool(cid, b, &r.input, sandbox, settings)
                        .await;
                    out.duration_ms =
                        u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);
                    out
                } else if let ToolKind::Browser { tool } = &r.entry.kind {
                    let started = Instant::now();
                    let mut out = self
                        .run_browser_tool(cid, tool, &r.input, &r.call.id, browser_llm)
                        .await;
                    out.duration_ms =
                        u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);
                    out
                } else if let ToolKind::SandboxFile { tool } = &r.entry.kind {
                    let started = Instant::now();
                    let mut out = self.run_sandbox_file(cid, tool, &r.input).await;
                    out.duration_ms =
                        u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);
                    out
                } else if r.entry.kind == ToolKind::Builtin(Builtin::Artifact) {
                    let started = Instant::now();
                    let mut out = run_artifact(&self.store, cid, reply_id, &r.input).await;
                    out.duration_ms =
                        u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);
                    out
                } else {
                    self.tools
                        .run(&r.entry, &r.input, &r.call.id, settings)
                        .await
                };
                self.report(cid, reply_id, &r.call, Some(&r.entry), &r.input, &out)
                    .await;
                lock(done).push((i, out));
            }
        }));
        let finished = tokio::select! {
            _ = work => true,
            _ = cancelled.wait_for(|c| *c) => false,
        };
        // A stop also ends waiting calls at once, so `work` can finish in the same poll;
        // `select!` picks a ready branch at random, so check the stop itself.
        let finished = finished && !*cancelled.borrow();
        setup.grants = grants.into_inner().unwrap_or_else(|e| e.into_inner());
        // Results that arrived (also before a stop) go into the reply in call order.
        let mut outs = done.into_inner().unwrap_or_else(|e| e.into_inner());
        outs.sort_by_key(|(i, _)| *i);
        for (i, out) in outs {
            if let Some(r) = ready.get(i) {
                reply.push_result(&r.call.id, out);
            }
        }
        finished
    }

    /// Runs a file tool, the task list or a shell command for a chat.
    #[allow(clippy::too_many_arguments)]
    async fn run_workspace_tool(
        &self,
        cid: &str,
        reply_id: &str,
        tool: Builtin,
        input: &Value,
        sandbox: &Sandbox,
        shell: Option<&Shell>,
        settings: &AppSettings,
    ) -> ToolOutput {
        let text = |t: String| ToolOutput {
            content: vec![ContentBlock::text(t)],
            is_error: false,
            duration_ms: 0,
        };
        match tool {
            Builtin::File(op) => {
                let (sb, input) = (sandbox.clone(), input.clone());
                match tokio::task::spawn_blocking(move || sb.run(op, &input)).await {
                    Ok(Ok(t)) => text(t),
                    Ok(Err(e)) => ToolOutput::error(e),
                    Err(e) => ToolOutput::error(format!("The file tool failed: {e}")),
                }
            }
            Builtin::TaskList => {
                let items = match crate::tasks::parse_tasks(input) {
                    Ok(i) => i,
                    Err(e) => return ToolOutput::error(e),
                };
                match self.store.set_task_list(cid, &items).await {
                    Ok(list) => {
                        let summary = crate::tasks::summary(&list);
                        self.sink.tasks_changed(list);
                        text(summary)
                    }
                    Err(e) => ToolOutput::error(format!("The task list couldn't be saved: {e}")),
                }
            }
            Builtin::RunCommand => {
                let Some(shell) = shell else {
                    return ToolOutput::error("The shell tool is off (Settings → Permissions).");
                };
                let limit = Duration::from_secs(u64::from(settings.shell.timeout_secs.max(1)));
                match run_command(sandbox, shell, input, limit).await {
                    Ok((t, failed)) => ToolOutput {
                        content: vec![ContentBlock::text(t)],
                        is_error: failed,
                        duration_ms: 0,
                    },
                    Err(e) => ToolOutput::error(e),
                }
            }
            Builtin::RunPython => {
                let limit = Duration::from_secs(u64::from(settings.python.timeout_secs.max(1)));
                run_python(
                    &self.tools,
                    &self.store,
                    cid,
                    reply_id,
                    sandbox,
                    input,
                    limit,
                )
                .await
            }
            _ => ToolOutput::error("This tool isn't a workspace tool."),
        }
    }

    /// Runs a command or a copy in the chat's Docker sandbox.
    async fn run_sandbox_tool(
        &self,
        cid: &str,
        tool: Builtin,
        input: &Value,
        shared: &Sandbox,
        settings: &AppSettings,
    ) -> ToolOutput {
        let Some(sb) = self.tools.sandboxes() else {
            return ToolOutput::error("The sandbox isn't available in this build.");
        };
        let text = |t: String, is_error: bool| ToolOutput {
            content: vec![ContentBlock::text(t)],
            is_error,
            duration_ms: 0,
        };
        let r = match tool {
            Builtin::SandboxExec => {
                let limit = Duration::from_secs(u64::from(settings.sandbox.timeout_secs.max(1)));
                sandbox_tools::exec(sb, cid, input, limit).await
            }
            Builtin::SandboxCopyIn => sandbox_tools::copy_in(sb, shared, cid, input)
                .await
                .map(|t| (t, false)),
            Builtin::SandboxCopyOut => sandbox_tools::copy_out(sb, shared, cid, input)
                .await
                .map(|t| (t, false)),
            _ => Err("This isn't a sandbox tool.".into()),
        };
        match r {
            Ok((t, failed)) => text(t, failed),
            Err(e) => ToolOutput::error(e),
        }
    }

    /// Calls one of the sandbox file server's tools for the chat.
    async fn run_sandbox_file(&self, cid: &str, tool: &str, input: &Value) -> ToolOutput {
        let Some(sb) = self.tools.sandboxes() else {
            return ToolOutput::error("The sandbox isn't available in this build.");
        };
        match sb.call_file_tool(cid, tool, input.clone()).await {
            Ok(r) => self.tools.mcp_output(r).await,
            Err(e) => ToolOutput::error(e.to_string()),
        }
    }

    /// Calls one of the browser agent's tools for the chat (setting the browser up and
    /// starting the chat's agent when needed).
    async fn run_browser_tool(
        &self,
        cid: &str,
        tool: &str,
        input: &Value,
        call_id: &str,
        llm: Option<&oxide_pyenv::BrowserLlm>,
    ) -> ToolOutput {
        let Some(b) = self.tools.browser() else {
            return ToolOutput::error("The browser isn't available in this build.");
        };
        match b
            .call(cid, tool, input.clone(), call_id, llm.cloned())
            .await
        {
            Ok(r) => self.tools.mcp_output(r).await,
            Err(e) => ToolOutput::error(e.to_string()),
        }
    }

    fn emit_call(&self, cid: &str, call: &OpenCall, entry: &ToolEntry) {
        self.emit(
            cid,
            StreamEvent::ToolCall {
                id: call.id.clone(),
                name: call.name.clone(),
                server: entry.server().map(str::to_owned),
                tool: entry.tool().to_owned(),
                title: entry.title.clone(),
                input_json: call.input_json.clone(),
            },
        );
    }

    /// Sends a call's result to the UI and logs it.
    async fn report(
        &self,
        cid: &str,
        reply_id: &str,
        call: &OpenCall,
        entry: Option<&ToolEntry>,
        input: &Value,
        out: &ToolOutput,
    ) {
        self.emit(
            cid,
            StreamEvent::ToolResult {
                id: call.id.clone(),
                content: out.content.clone(),
                is_error: out.is_error,
                duration_ms: out.duration_ms,
            },
        );
        if let Some(entry) = entry {
            self.log(
                cid,
                reply_id,
                &call.id,
                entry.usage_kind(),
                entry.server().map(str::to_owned),
                entry.tool().to_owned(),
                match entry.kind {
                    ToolKind::Builtin(Builtin::File(op)) => file_label(op, input),
                    ToolKind::Builtin(Builtin::RunPython) => input
                        .get("code")
                        .and_then(Value::as_str)
                        .map(oxide_core::python::code_label)
                        .filter(|l| !l.is_empty()),
                    ToolKind::Builtin(Builtin::TaskList) => crate::tasks::parse_tasks(input)
                        .ok()
                        .and_then(|t| crate::tasks::label(&t)),
                    ToolKind::Builtin(
                        ref b @ (Builtin::SandboxExec
                        | Builtin::SandboxCopyIn
                        | Builtin::SandboxCopyOut),
                    ) => sandbox_tools::label(b, input),
                    ToolKind::Browser { ref tool } => browser_tools::browser_label(tool, input),
                    _ => usage_label(entry.tool(), input),
                },
                out.is_error,
            )
            .await;
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn finish_call(
        &self,
        cid: &str,
        reply_id: &str,
        call: &OpenCall,
        entry: Option<&ToolEntry>,
        input: &Value,
        out: ToolOutput,
        reply: &mut ReplyBuilder,
    ) {
        self.report(cid, reply_id, call, entry, input, &out).await;
        reply.push_result(&call.id, out);
    }

    /// Sends buffered text to the UI.
    fn flush(&self, cid: &str, reply: &mut ReplyBuilder) {
        if !reply.pending_thinking.is_empty() {
            let text = std::mem::take(&mut reply.pending_thinking);
            self.emit(cid, StreamEvent::ThinkingDelta { text });
        }
        if !reply.pending_text.is_empty() {
            let text = std::mem::take(&mut reply.pending_text);
            self.emit(cid, StreamEvent::TextDelta { text });
        }
        for (id, partial_json) in std::mem::take(&mut reply.pending_input) {
            self.emit(cid, StreamEvent::ToolInputDelta { id, partial_json });
        }
    }

    /// Names a new chat after its first exchange, unless the user renamed it already.
    async fn auto_title(&self, cid: &str, first: &Message, reply: &Message) {
        let Ok(settings) = self.store.app_settings().await else {
            return;
        };
        if !settings.models.auto_title {
            return;
        }
        let user_text = plain_text(&first.content);
        let reply_text = plain_text(&reply.content);
        if user_text.trim().is_empty() || reply_text.trim().is_empty() {
            return;
        }
        let Ok(convo) = self.store.get_conversation(cid).await else {
            return;
        };
        if convo.title != title_from_text(&user_text) {
            return; // renamed by the user
        }
        let title = tokio::time::timeout(
            TITLE_TIMEOUT,
            self.suggest_title(
                &convo,
                &settings.models.title_model,
                &user_text,
                &reply_text,
            ),
        )
        .await;
        let title = match title {
            Ok(Ok(Some(t))) => t,
            Ok(Ok(None)) => return,
            Ok(Err(e)) => {
                tracing::info!(conversation = %cid, "no automatic title: {e}");
                return;
            }
            Err(_) => {
                tracing::info!(conversation = %cid, "no automatic title: timed out");
                return;
            }
        };
        // Check again: the user may have renamed it while the title was written.
        match self.store.get_conversation(cid).await {
            Ok(c) if c.title == convo.title => {}
            _ => return,
        }
        match self.store.rename_conversation(cid, &title).await {
            Ok(c) => self.sink.conversation_changed(c),
            Err(e) => tracing::warn!("couldn't save the automatic title: {e}"),
        }
    }

    async fn suggest_title(
        &self,
        convo: &Conversation,
        configured: &Option<String>,
        user_text: &str,
        reply_text: &str,
    ) -> Result<Option<String>, AgentError> {
        let text = self
            .quick_text(
                convo,
                configured,
                "You name chats. You answer with a short title only.",
                title_prompt(user_text, reply_text),
                60,
            )
            .await?;
        Ok(clean_title(&text))
    }

    /// Learns memory items from the latest exchange, when memory is on.
    async fn learn_memory(&self, cid: &str, user: &Message, reply: &Message) {
        let Ok(settings) = self.store.app_settings().await else {
            return;
        };
        if !settings.memory.enabled {
            return;
        }
        let user_text = text_blocks(&user.content);
        let reply_text = text_blocks(&reply.content);
        if user_text.trim().is_empty() {
            return;
        }
        let Ok(convo) = self.store.get_conversation(cid).await else {
            return;
        };
        let project = match &convo.project_id {
            Some(p) => self.store.get_project(p).await.ok(),
            None => None,
        };
        let existing = self
            .store
            .memory_for(convo.project_id.as_deref())
            .await
            .unwrap_or_default();
        let prompt = memory_prompt(
            &existing,
            project.as_ref().map(|p| p.name.as_str()),
            &user_text,
            &reply_text,
        );
        let answer = tokio::time::timeout(
            MEMORY_TIMEOUT,
            self.quick_text(&convo, &settings.memory.model, MEMORY_SYSTEM, prompt, 400),
        )
        .await;
        let raw = match answer {
            Ok(Ok(t)) => t,
            Ok(Err(e)) => {
                tracing::info!(conversation = %cid, "no memory update: {e}");
                return;
            }
            Err(_) => {
                tracing::info!(conversation = %cid, "no memory update: timed out");
                return;
            }
        };
        let mut added = 0u32;
        for p in parse_proposals(&raw, &existing, project.is_some()) {
            let project_id = match p.scope {
                Scope::Project => convo.project_id.clone(),
                Scope::Global => None,
            };
            match self
                .store
                .add_memory(
                    oxide_core::NewMemory {
                        project_id,
                        text: p.text,
                    },
                    Some(cid),
                )
                .await
            {
                Ok(_) => added += 1,
                Err(e) => {
                    tracing::info!("memory item not saved: {e}");
                    break;
                }
            }
        }
        if added > 0 {
            tracing::info!(conversation = %cid, "memory: {added} new item(s)");
            self.sink.memory_changed(added);
        }
    }

    /// A short non-streaming job (titles, memory) on a small model: the configured one, or a
    /// cheap one from the chat's endpoint. `anthropic_max` caps output on Anthropic;
    /// others get room for reasoning models' thinking.
    async fn quick_text(
        &self,
        convo: &Conversation,
        configured: &Option<String>,
        system: &str,
        prompt: String,
        anthropic_max: u32,
    ) -> Result<String, AgentError> {
        let model_id = match configured {
            Some(m) => m.clone(),
            None => {
                let (provider_id, _) = split_model_id(&convo.model)
                    .ok_or_else(|| AgentError::Invalid("unknown model".into()))?;
                let config = self.store.get_provider(provider_id).await?;
                let models = self.registry.models_of(provider_id).await;
                pick_title_model(config.kind, &models).unwrap_or_else(|| convo.model.clone())
            }
        };
        let (provider_id, model) = split_model_id(&model_id)
            .ok_or_else(|| AgentError::Invalid("unknown title model".into()))?;
        let (config, provider) = self.registry.provider(provider_id).await?;
        let caps = self
            .registry
            .find(&model_id)
            .await
            .map(|m| m.capabilities)
            .unwrap_or_default();
        let mut request = ChatRequest::new(model, vec![ChatMessage::user(prompt)]);
        request.system = Some(system.to_owned());
        request.max_tokens = Some(match config.kind {
            ProviderKind::Anthropic => anthropic_max,
            // Reasoning models spend output tokens before the answer.
            _ => 1_024,
        });
        if (config.kind == ProviderKind::Anthropic
            && caps.thinking == ThinkingStyle::Adaptive
            && caps.thinking_off)
            || caps.thinking == ThinkingStyle::Switch
        {
            request.thinking = Thinking::Off;
        }
        Ok(complete_text(provider.as_ref(), request).await?)
    }
}

/// The text blocks of a message (no tool results or files), joined by blank lines.
fn text_blocks(content: &[ContentBlock]) -> String {
    content
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Text { text } if !text.trim().is_empty() => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// The path from the root to `id`, or `None` if `id` isn't among `messages`.
pub fn path_to(messages: &[Message], id: &str) -> Option<Vec<Message>> {
    let by_id: HashMap<&str, &Message> = messages.iter().map(|m| (m.id.as_str(), m)).collect();
    let mut path = Vec::new();
    let mut cur = by_id.get(id).copied();
    while let Some(m) = cur {
        if path.len() > messages.len() {
            return None; // a cycle; can't happen with foreign keys
        }
        path.push(m.clone());
        cur = m.parent_id.as_deref().and_then(|p| by_id.get(p).copied());
    }
    if path.is_empty() {
        return None;
    }
    path.reverse();
    Some(path)
}

/// A tool call the model made that has no result yet.
#[derive(Debug, Clone, PartialEq, Eq)]
struct OpenCall {
    id: String,
    name: String,
    input_json: String,
}

/// Accumulates a reply into content blocks, in the order things happen.
#[derive(Default)]
struct ReplyBuilder {
    blocks: Vec<ContentBlock>,
    /// When the open thinking block (the last block) started.
    thinking_started: Option<Instant>,
    /// Usage of finished steps, summed.
    usage_done: Option<Usage>,
    /// Usage of the step being streamed (providers report it cumulatively).
    usage_step: Option<Usage>,
    pending_text: String,
    pending_thinking: String,
    /// Tool-call arguments not yet sent to the UI: (call id, JSON text).
    pending_input: Vec<(String, String)>,
}

fn add_usage(a: Option<Usage>, b: Option<Usage>) -> Option<Usage> {
    match (a, b) {
        (None, x) | (x, None) => x,
        (Some(a), Some(b)) => Some(Usage {
            input_tokens: a.input_tokens.saturating_add(b.input_tokens),
            output_tokens: a.output_tokens.saturating_add(b.output_tokens),
            cache_read_tokens: a.cache_read_tokens.saturating_add(b.cache_read_tokens),
            cache_write_tokens: a.cache_write_tokens.saturating_add(b.cache_write_tokens),
        }),
    }
}

impl ReplyBuilder {
    fn begin_step(&mut self) {
        self.end_thinking();
        self.usage_done = add_usage(self.usage_done, self.usage_step.take());
    }

    fn total_usage(&self) -> Option<Usage> {
        add_usage(self.usage_done, self.usage_step)
    }

    /// Applies an event. Text is buffered (see `flush`); other events the UI shows are
    /// returned to be sent straight away.
    fn apply(&mut self, event: ChatEvent) -> Vec<StreamEvent> {
        match event {
            ChatEvent::ThinkingDelta(t) => {
                match self.blocks.last_mut() {
                    Some(ContentBlock::Thinking { text, .. })
                        if self.thinking_started.is_some() =>
                    {
                        text.push_str(&t);
                    }
                    _ => {
                        self.thinking_started = Some(Instant::now());
                        self.blocks.push(ContentBlock::Thinking {
                            text: t.clone(),
                            signature: None,
                            duration_ms: None,
                        });
                    }
                }
                self.pending_thinking.push_str(&t);
                vec![]
            }
            ChatEvent::ThinkingSignature(s) => {
                if let Some(ContentBlock::Thinking { signature, .. }) = self
                    .blocks
                    .iter_mut()
                    .rev()
                    .find(|b| matches!(b, ContentBlock::Thinking { .. }))
                {
                    *signature = Some(s);
                }
                vec![]
            }
            ChatEvent::TextDelta(t) => {
                self.end_thinking();
                match self.blocks.last_mut() {
                    Some(ContentBlock::Text { text }) => text.push_str(&t),
                    _ => self.blocks.push(ContentBlock::text(t.clone())),
                }
                self.pending_text.push_str(&t);
                vec![]
            }
            ChatEvent::ToolUseStart { id, name } => {
                self.end_thinking();
                self.blocks.push(ContentBlock::ToolUse {
                    id: id.clone(),
                    name: name.clone(),
                    input_json: String::new(),
                    server: None,
                    tool: None,
                });
                vec![StreamEvent::ToolUse { id, name }]
            }
            ChatEvent::ServerToolStart { id, name } => {
                self.end_thinking();
                self.blocks.push(ContentBlock::ServerToolUse {
                    id: id.clone(),
                    name: name.clone(),
                    input_json: String::new(),
                });
                vec![StreamEvent::ToolUse { id, name }]
            }
            ChatEvent::ToolUseDelta { id, partial_json } => {
                if let Some(json) = self.input_of(&id) {
                    json.push_str(&partial_json);
                    match self.pending_input.last_mut() {
                        Some((last, text)) if *last == id => text.push_str(&partial_json),
                        _ => self.pending_input.push((id, partial_json)),
                    }
                }
                vec![]
            }
            ChatEvent::ToolUseEnd { id } => {
                if let Some(json) = self.input_of(&id)
                    && json.trim().is_empty()
                {
                    *json = "{}".into();
                }
                // Provider-run tools are announced here; Oxide's own when they run.
                self.blocks
                    .iter()
                    .find_map(|b| match b {
                        ContentBlock::ServerToolUse {
                            id: bid,
                            name,
                            input_json,
                        } if *bid == id => Some(StreamEvent::ToolCall {
                            id: id.clone(),
                            name: name.clone(),
                            server: Some(PROVIDER_SERVER.into()),
                            tool: name.clone(),
                            title: Some(if name == "web_search" {
                                "Web search".into()
                            } else {
                                name.clone()
                            }),
                            input_json: input_json.clone(),
                        }),
                        _ => None,
                    })
                    .into_iter()
                    .chain(std::iter::once(StreamEvent::ToolRunning { id: id.clone() }))
                    .filter(|_| self.server_call(&id).is_some())
                    .collect()
            }
            ChatEvent::ServerToolResult {
                tool_use_id,
                raw_json,
            } => {
                let (content, is_error) = provider_result_summary(&raw_json);
                self.blocks.push(ContentBlock::ServerToolResult {
                    tool_use_id: tool_use_id.clone(),
                    raw_json,
                });
                vec![StreamEvent::ToolResult {
                    id: tool_use_id,
                    content,
                    is_error,
                    duration_ms: 0,
                }]
            }
            ChatEvent::Usage(u) => {
                let mut step = self.usage_step.unwrap_or_default();
                step.merge(u);
                self.usage_step = Some(step);
                match self.total_usage() {
                    Some(usage) => vec![StreamEvent::Usage { usage }],
                    None => vec![],
                }
            }
            ChatEvent::Stop(_) => vec![],
        }
    }

    fn input_of(&mut self, id: &str) -> Option<&mut String> {
        self.blocks.iter_mut().rev().find_map(|b| match b {
            ContentBlock::ToolUse {
                id: bid,
                input_json,
                ..
            }
            | ContentBlock::ServerToolUse {
                id: bid,
                input_json,
                ..
            } if bid == id => Some(input_json),
            _ => None,
        })
    }

    /// Name and input of a provider-run call.
    fn server_call(&self, id: &str) -> Option<(String, Value)> {
        self.blocks.iter().find_map(|b| match b {
            ContentBlock::ServerToolUse {
                id: bid,
                name,
                input_json,
            } if bid == id => Some((
                name.clone(),
                serde_json::from_str(input_json).unwrap_or(Value::Null),
            )),
            _ => None,
        })
    }

    fn end_thinking(&mut self) {
        if let Some(start) = self.thinking_started.take()
            && let Some(ContentBlock::Thinking { duration_ms, .. }) = self
                .blocks
                .iter_mut()
                .rev()
                .find(|b| matches!(b, ContentBlock::Thinking { .. }))
            && duration_ms.is_none()
        {
            *duration_ms = Some(u32::try_from(start.elapsed().as_millis()).unwrap_or(u32::MAX));
        }
    }

    /// Oxide-run calls without a result, in order.
    fn open_calls(&self) -> Vec<OpenCall> {
        let answered: HashSet<&str> = self
            .blocks
            .iter()
            .filter_map(|b| match b {
                ContentBlock::ToolResult { tool_use_id, .. } => Some(tool_use_id.as_str()),
                _ => None,
            })
            .collect();
        self.blocks
            .iter()
            .filter_map(|b| match b {
                ContentBlock::ToolUse {
                    id,
                    name,
                    input_json,
                    ..
                } if !answered.contains(id.as_str()) => Some(OpenCall {
                    id: id.clone(),
                    name: name.clone(),
                    input_json: input_json.clone(),
                }),
                _ => None,
            })
            .collect()
    }

    /// Fills in which server and tool a call went to.
    fn label_call(&mut self, call_id: &str, server_name: Option<&str>, tool_name: &str) {
        for b in &mut self.blocks {
            if let ContentBlock::ToolUse {
                id, server, tool, ..
            } = b
                && id == call_id
            {
                *server = server_name.map(str::to_owned);
                *tool = Some(tool_name.to_owned());
            }
        }
    }

    fn push_result(&mut self, id: &str, out: ToolOutput) {
        self.end_thinking();
        self.blocks.push(ContentBlock::ToolResult {
            tool_use_id: id.to_owned(),
            content: out.content,
            is_error: out.is_error,
        });
    }

    /// Gives every open call an error result. Returns them for the UI.
    fn close_open_calls(&mut self, note: &str) -> Vec<(String, ToolOutput)> {
        let open = self.open_calls();
        let mut closed = Vec::new();
        for c in open {
            let out = ToolOutput::error(note);
            self.push_result(&c.id, out.clone());
            closed.push((c.id, out));
        }
        closed
    }

    fn take_content(&mut self) -> Vec<ContentBlock> {
        self.end_thinking();
        std::mem::take(&mut self.blocks)
            .into_iter()
            .filter(|b| match b {
                ContentBlock::Text { text } => !text.is_empty(),
                ContentBlock::Thinking {
                    text, signature, ..
                } => !text.trim().is_empty() || signature.is_some(),
                _ => true,
            })
            .map(|b| match b {
                ContentBlock::ToolUse {
                    id,
                    name,
                    input_json,
                    server,
                    tool,
                } if input_json.trim().is_empty() => ContentBlock::ToolUse {
                    id,
                    name,
                    input_json: "{}".into(),
                    server,
                    tool,
                },
                b => b,
            })
            .collect()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::registry::Connector;
    use crate::tools::tests::{FakeMcp, tool};
    use futures_util::FutureExt;
    use futures_util::future::BoxFuture;
    use oxide_core::{ModelInfo, StartConversation};
    use oxide_llm::{ChatStream, Endpoint, LlmError, ModelListing, Provider};
    use oxide_store::{MemorySecrets, SecretStore};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::mpsc;

    /// One scripted step of a fake reply.
    #[derive(Clone)]
    pub enum Step {
        Event(ChatEvent),
        Fail(&'static str),
        /// Wait until the test sends on the gate channel.
        Gate,
    }

    /// A provider that lists one model ("fake-model", tools on) and replays scripts.
    #[derive(Clone)]
    pub struct FakeProvider {
        inner: Arc<FakeInner>,
    }

    pub struct FakeInner {
        endpoint: Mutex<Option<Endpoint>>,
        scripts: Mutex<Vec<Vec<Step>>>,
        gate: Mutex<Option<mpsc::UnboundedReceiver<()>>>,
        gate_tx: mpsc::UnboundedSender<()>,
        unverified: bool,
        probe_answer: bool,
        pub requests: Mutex<Vec<ChatRequest>>,
        pub list_calls: AtomicUsize,
        pub probe_calls: AtomicUsize,
    }

    impl std::ops::Deref for FakeProvider {
        type Target = FakeInner;
        fn deref(&self) -> &FakeInner {
            &self.inner
        }
    }

    impl FakeProvider {
        pub fn new(scripts: Vec<Vec<Step>>) -> Self {
            let (gate_tx, gate) = mpsc::unbounded_channel();
            Self {
                inner: Arc::new(FakeInner {
                    endpoint: Mutex::new(None),
                    scripts: Mutex::new(scripts),
                    gate: Mutex::new(Some(gate)),
                    gate_tx,
                    unverified: false,
                    probe_answer: false,
                    requests: Mutex::new(Vec::new()),
                    list_calls: AtomicUsize::new(0),
                    probe_calls: AtomicUsize::new(0),
                }),
            }
        }

        pub fn unverified(self, on: bool) -> Self {
            self.rebuild(|i| i.unverified = on)
        }

        pub fn probe_answer(self, tools: bool) -> Self {
            self.rebuild(|i| i.probe_answer = tools)
        }

        fn rebuild(self, f: impl FnOnce(&mut FakeInner)) -> Self {
            let mut inner = Arc::try_unwrap(self.inner)
                .ok()
                .expect("unshared while building");
            f(&mut inner);
            Self {
                inner: Arc::new(inner),
            }
        }

        pub fn open_gate(&self) {
            self.gate_tx.send(()).expect("gate open");
        }

        pub fn requests(&self) -> Vec<ChatRequest> {
            self.requests.lock().expect("lock").clone()
        }
    }

    struct Bound {
        fake: FakeProvider,
        endpoint: Endpoint,
    }

    impl Provider for Bound {
        fn endpoint(&self) -> &Endpoint {
            &self.endpoint
        }

        fn list_models(&self) -> BoxFuture<'_, Result<ModelListing, LlmError>> {
            self.fake.list_calls.fetch_add(1, Ordering::SeqCst);
            let info = ModelInfo {
                id: format!("{}/fake-model", self.endpoint.id),
                provider_id: self.endpoint.id.clone(),
                model: "fake-model".into(),
                label: "Fake".into(),
                capabilities: ModelCapabilities {
                    tools: !self.fake.unverified,
                    ..ModelCapabilities::default()
                },
                context_window: None,
                max_output_tokens: None,
            };
            let unverified = if self.fake.unverified {
                vec!["fake-model".to_owned()]
            } else {
                vec![]
            };
            async move {
                Ok(ModelListing {
                    models: vec![info],
                    unverified,
                })
            }
            .boxed()
        }

        fn stream(&self, request: ChatRequest) -> BoxFuture<'_, Result<ChatStream, LlmError>> {
            self.fake.requests.lock().expect("lock").push(request);
            let script = {
                let mut s = self.fake.scripts.lock().expect("lock");
                if s.is_empty() {
                    Vec::new()
                } else {
                    s.remove(0)
                }
            };
            let gate = self.fake.gate.lock().expect("lock").take();
            async move {
                let state = (script.into_iter(), gate);
                let s = futures_util::stream::unfold(state, |(mut steps, mut gate)| async move {
                    loop {
                        match steps.next()? {
                            Step::Event(e) => return Some((Ok(e), (steps, gate))),
                            Step::Fail(m) => {
                                return Some((Err(LlmError::Stream(m.into())), (steps, gate)));
                            }
                            Step::Gate => {
                                if let Some(g) = gate.as_mut() {
                                    g.recv().await;
                                }
                            }
                        }
                    }
                });
                Ok(s.boxed() as ChatStream)
            }
            .boxed()
        }

        fn probe_tools<'a>(&'a self, _model: &'a str) -> BoxFuture<'a, Result<bool, LlmError>> {
            self.fake.probe_calls.fetch_add(1, Ordering::SeqCst);
            let answer = self.fake.probe_answer;
            async move { Ok(answer) }.boxed()
        }
    }

    pub fn fake_connector(fake: FakeProvider) -> Connector {
        Arc::new(move |endpoint: Endpoint| {
            *fake.endpoint.lock().expect("lock") = Some(endpoint.clone());
            Ok(Arc::new(Bound {
                fake: fake.clone(),
                endpoint,
            }) as Arc<dyn Provider>)
        })
    }

    #[derive(Default)]
    pub struct Recorder {
        pub events: Mutex<Vec<ChatStreamEvent>>,
        pub changed: Mutex<Vec<Conversation>>,
        pub memory_changes: AtomicUsize,
        pub tasks: Mutex<Vec<TaskList>>,
        notify: tokio::sync::Notify,
    }

    impl EventSink for Recorder {
        fn stream_event(&self, event: ChatStreamEvent) {
            self.events.lock().expect("lock").push(event);
            self.notify.notify_waiters();
        }
        fn conversation_changed(&self, conversation: Conversation) {
            self.changed.lock().expect("lock").push(conversation);
            self.notify.notify_waiters();
        }
        fn memory_changed(&self, _added: u32) {
            self.memory_changes.fetch_add(1, Ordering::SeqCst);
            self.notify.notify_waiters();
        }
        fn tasks_changed(&self, list: TaskList) {
            self.tasks.lock().expect("lock").push(list);
            self.notify.notify_waiters();
        }
    }

    impl Recorder {
        pub fn events(&self) -> Vec<StreamEvent> {
            self.events
                .lock()
                .expect("lock")
                .iter()
                .map(|e| e.event.clone())
                .collect()
        }

        /// Waits until an event matching `pred` was recorded.
        async fn wait_for(&self, pred: impl Fn(&Recorder) -> bool) {
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let notified = self.notify.notified();
                    if pred(self) {
                        return;
                    }
                    notified.await;
                }
            })
            .await
            .expect("event arrives in time");
        }

        async fn wait_finished(&self) {
            self.wait_for(|r| {
                r.events()
                    .iter()
                    .any(|e| matches!(e, StreamEvent::Done { .. } | StreamEvent::Failed { .. }))
            })
            .await;
        }
    }

    struct Fixture {
        store: Store,
        service: Arc<ChatService>,
        fake: FakeProvider,
        sink: Arc<Recorder>,
        chat: Conversation,
        first: Message,
    }

    async fn fixture(scripts: Vec<Vec<Step>>) -> Fixture {
        fixture_with(scripts, None).await
    }

    async fn fixture_with(scripts: Vec<Vec<Step>>, mcp: Option<Arc<FakeMcp>>) -> Fixture {
        fixture_full(scripts, mcp, None).await
    }

    async fn fixture_full(
        scripts: Vec<Vec<Step>>,
        mcp: Option<Arc<FakeMcp>>,
        skills: Option<crate::skills::Skills>,
    ) -> Fixture {
        let store = Store::open_in_memory().await.expect("store");
        let secrets: Arc<dyn SecretStore> = Arc::new(MemorySecrets::default());
        let fake = FakeProvider::new(scripts);
        let registry = Arc::new(ModelRegistry::with_connector(
            store.clone(),
            secrets.clone(),
            fake_connector(fake.clone()),
        ));
        let sink = Arc::new(Recorder::default());
        let tools = Tools::new(
            mcp.map(|m| m as Arc<dyn crate::tools::McpTools>),
            secrets,
            None,
        );
        let tools = match skills {
            Some(sk) => tools.with_skills(sk),
            None => tools,
        };
        let service = ChatService::new(store.clone(), registry, sink.clone(), tools);
        let (chat, first) = store
            .start_conversation(StartConversation {
                text: "What is Rust?".into(),
                model: "lmstudio/fake-model".into(),
                project_id: None,
                attachments: Vec::new(),
                folders: vec![],
            })
            .await
            .expect("chat");
        Fixture {
            store,
            service,
            fake,
            sink,
            chat,
            first,
        }
    }

    fn ev(e: ChatEvent) -> Step {
        Step::Event(e)
    }

    fn text(t: &str) -> Step {
        ev(ChatEvent::TextDelta(t.into()))
    }

    #[tokio::test]
    async fn a_reply_streams_is_saved_and_titles_the_chat() {
        let f = fixture(vec![
            vec![
                ev(ChatEvent::ThinkingDelta("Consider".into())),
                ev(ChatEvent::ThinkingSignature("sig".into())),
                text("Rust is "),
                text("a language."),
                ev(ChatEvent::Usage(Usage {
                    input_tokens: 9,
                    output_tokens: 4,
                    ..Usage::default()
                })),
                ev(ChatEvent::Stop(StopReason::EndTurn)),
            ],
            vec![
                text("\"Rust overview.\""),
                ev(ChatEvent::Stop(StopReason::EndTurn)),
            ],
        ])
        .await;
        let id = f
            .service
            .reply(
                &f.chat.id,
                &f.first.id,
                GenerateOptions {
                    thinking: true,
                    effort: None,
                },
            )
            .await
            .expect("starts");
        f.sink
            .wait_for(|r| !r.changed.lock().expect("lock").is_empty())
            .await;

        let events = f.sink.events();
        assert!(
            matches!(&events[0], StreamEvent::Started { message_id, parent_id, .. }
            if *message_id == id && *parent_id == f.first.id)
        );
        let streamed: String = events
            .iter()
            .filter_map(|e| match e {
                StreamEvent::TextDelta { text } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(streamed, "Rust is a language.");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, StreamEvent::ThinkingDelta { text } if text == "Consider"))
        );
        let Some(StreamEvent::Done {
            message: Some(saved),
        }) = events.last()
        else {
            panic!("ends with done: {events:?}");
        };
        assert_eq!(saved.id, id);
        assert_eq!(saved.stop_reason, Some(StopReason::EndTurn));
        assert_eq!(saved.usage.map(|u| u.output_tokens), Some(4));
        assert!(
            matches!(&saved.content[0], ContentBlock::Thinking { text, signature: Some(s), .. }
            if text == "Consider" && s == "sig")
        );
        assert_eq!(saved.content[1], ContentBlock::text("Rust is a language."));
        assert_eq!(f.store.get_message(&id).await.expect("saved"), *saved);

        // The request: system prompt, the question, no thinking for a model without it.
        let req = &f.fake.requests()[0];
        assert_eq!(req.model, "fake-model");
        assert!(req.system.as_deref().is_some_and(|s| s.contains("Oxide")));
        assert_eq!(req.messages, vec![ChatMessage::user("What is Rust?")]);
        assert_eq!(req.thinking, Thinking::Default);

        let changed = f.sink.changed.lock().expect("lock").clone();
        assert_eq!(changed[0].title, "Rust overview");
        assert_eq!(
            f.store
                .get_conversation(&f.chat.id)
                .await
                .expect("ok")
                .title,
            "Rust overview"
        );
        assert!(!f.service.is_running(&f.chat.id));
    }

    #[tokio::test]
    async fn stop_keeps_what_was_written() {
        let f = fixture(vec![vec![text("Partial"), Step::Gate, text(" never")]]).await;
        f.service
            .reply(&f.chat.id, &f.first.id, GenerateOptions::default())
            .await
            .expect("starts");
        f.sink
            .wait_for(|r| {
                r.events()
                    .iter()
                    .any(|e| matches!(e, StreamEvent::TextDelta { .. }))
            })
            .await;
        assert!(f.service.is_running(&f.chat.id));
        assert!(f.service.stop(&f.chat.id));
        f.sink.wait_finished().await;
        let Some(StreamEvent::Done { message: Some(m) }) = f.sink.events().last().cloned() else {
            panic!("done with the partial reply");
        };
        assert_eq!(m.content, vec![ContentBlock::text("Partial")]);
        assert_eq!(m.stop_reason, Some(StopReason::Stopped));
        // Stopped replies don't trigger a title.
        assert!(f.sink.changed.lock().expect("lock").is_empty());
        assert!(!f.service.is_running(&f.chat.id));
        assert!(!f.service.stop(&f.chat.id));
    }

    #[tokio::test]
    async fn stop_before_any_output_saves_nothing() {
        let f = fixture(vec![vec![Step::Gate, text("late")]]).await;
        f.service
            .reply(&f.chat.id, &f.first.id, GenerateOptions::default())
            .await
            .expect("starts");
        f.service.stop(&f.chat.id);
        f.sink.wait_finished().await;
        assert!(matches!(
            f.sink.events().last(),
            Some(StreamEvent::Done { message: None })
        ));
        assert_eq!(
            f.store.list_messages(&f.chat.id).await.expect("ok").len(),
            1
        );
    }

    #[tokio::test]
    async fn failures_keep_partial_text_and_report_the_endpoint() {
        let f = fixture(vec![
            vec![text("Half"), Step::Fail("the connection broke off")],
            vec![Step::Fail("boom")],
        ])
        .await;
        f.service
            .reply(&f.chat.id, &f.first.id, GenerateOptions::default())
            .await
            .expect("starts");
        f.sink.wait_finished().await;
        let Some(StreamEvent::Failed {
            error,
            message: Some(m),
        }) = f.sink.events().last().cloned()
        else {
            panic!("failed with the partial reply");
        };
        assert_eq!(error, "LM Studio: the connection broke off");
        assert_eq!(m.stop_reason, Some(StopReason::Error));
        assert_eq!(m.content, vec![ContentBlock::text("Half")]);

        // Retrying answers the same question again, as a sibling reply.
        f.sink.events.lock().expect("lock").clear();
        f.service
            .regenerate(&m.id, GenerateOptions::default())
            .await
            .expect("starts");
        f.sink.wait_finished().await;
        assert!(matches!(
            f.sink.events().last(),
            Some(StreamEvent::Failed { message: None, .. })
        ));
    }

    #[tokio::test]
    async fn one_reply_at_a_time_per_chat() {
        let f = fixture(vec![vec![
            Step::Gate,
            text("x"),
            ev(ChatEvent::Stop(StopReason::EndTurn)),
        ]])
        .await;
        f.service
            .reply(&f.chat.id, &f.first.id, GenerateOptions::default())
            .await
            .expect("starts");
        let again = f
            .service
            .reply(&f.chat.id, &f.first.id, GenerateOptions::default())
            .await;
        assert!(matches!(again, Err(AgentError::Busy)));
        let send = f
            .service
            .send(
                &f.chat.id,
                Some(&f.first.id),
                vec![ContentBlock::text("more")],
                GenerateOptions::default(),
            )
            .await;
        assert!(matches!(send, Err(AgentError::Busy)));
        assert_eq!(
            f.store.list_messages(&f.chat.id).await.expect("ok").len(),
            1,
            "nothing saved"
        );
        f.fake.open_gate();
        f.sink.wait_finished().await;
    }

    #[tokio::test]
    async fn send_and_edit_branch_the_thread() {
        let f = fixture(vec![
            vec![text("A1"), ev(ChatEvent::Stop(StopReason::EndTurn))],
            vec![text("A2"), ev(ChatEvent::Stop(StopReason::EndTurn))],
            vec![text("A3"), ev(ChatEvent::Stop(StopReason::EndTurn))],
        ])
        .await;
        f.store
            .set_conversation_model(&f.chat.id, "lmstudio/fake-model")
            .await
            .expect("ok");
        // Rename first so no title request consumes a script.
        f.store
            .rename_conversation(&f.chat.id, "Mine")
            .await
            .expect("ok");
        f.service
            .reply(&f.chat.id, &f.first.id, GenerateOptions::default())
            .await
            .expect("ok");
        f.sink.wait_finished().await;
        let a1 = f
            .store
            .list_messages(&f.chat.id)
            .await
            .expect("ok")
            .pop()
            .expect("reply");

        f.sink.events.lock().expect("lock").clear();
        let q2 = f
            .service
            .send(
                &f.chat.id,
                Some(&a1.id),
                vec![ContentBlock::text("And Go?")],
                GenerateOptions::default(),
            )
            .await
            .expect("sends");
        f.sink.wait_finished().await;
        let req = &f.fake.requests()[1];
        assert_eq!(req.messages.len(), 3);
        assert_eq!(req.messages[2], ChatMessage::user("And Go?"));

        f.sink.events.lock().expect("lock").clear();
        let edited = f
            .service
            .edit_and_resend(&q2.id, "And Zig?", GenerateOptions::default())
            .await
            .expect("edits");
        f.sink.wait_finished().await;
        assert_eq!(edited.parent_id.as_deref(), Some(a1.id.as_str()));
        let req = &f.fake.requests()[2];
        assert_eq!(req.messages[2], ChatMessage::user("And Zig?"));
        assert_eq!(
            f.store.list_messages(&f.chat.id).await.expect("ok").len(),
            6
        );
        assert_eq!(
            f.store
                .get_conversation(&f.chat.id)
                .await
                .expect("ok")
                .title,
            "Mine"
        );
    }

    #[tokio::test]
    async fn unknown_endpoints_fail_with_a_hint() {
        let f = fixture(vec![]).await;
        f.store
            .set_conversation_model(&f.chat.id, "gone/model")
            .await
            .expect("ok");
        f.service
            .reply(&f.chat.id, &f.first.id, GenerateOptions::default())
            .await
            .expect("starts");
        f.sink.wait_finished().await;
        let Some(StreamEvent::Failed { error, .. }) = f.sink.events().last().cloned() else {
            panic!("fails");
        };
        assert!(error.contains("no longer exists"), "{error}");
    }

    #[tokio::test]
    async fn only_user_messages_can_be_answered() {
        let f = fixture(vec![]).await;
        let r = f
            .service
            .reply(&f.chat.id, "missing", GenerateOptions::default())
            .await;
        assert!(matches!(r, Err(AgentError::NotFound(_))));
    }

    /// Tauri runs async commands on a multi-threaded runtime: every future they await must
    /// be `Send`. Checked at compile time; never run.
    #[allow(dead_code, clippy::let_underscore_future)]
    fn command_futures_are_send(service: Arc<ChatService>, registry: Arc<ModelRegistry>) {
        fn send<T: Send>(_: T) {}
        let o = GenerateOptions::default();
        send(service.send("c", None, vec![], o));
        send(service.reply("c", "m", o));
        send(service.edit_and_resend("m", "t", o));
        send(service.regenerate("m", o));
        send(registry.list(false));
        send(registry.find("a/b"));
    }

    // ---- The tool loop ----------------------------------------------------------------

    fn call(id: &str, name: &str, json: &str) -> Vec<Step> {
        vec![
            ev(ChatEvent::ToolUseStart {
                id: id.into(),
                name: name.into(),
            }),
            ev(ChatEvent::ToolUseDelta {
                id: id.into(),
                partial_json: json.into(),
            }),
            ev(ChatEvent::ToolUseEnd { id: id.into() }),
        ]
    }

    fn turn(mut steps: Vec<Step>, stop: StopReason) -> Vec<Step> {
        steps.push(ev(ChatEvent::Usage(Usage {
            input_tokens: 10,
            output_tokens: 5,
            ..Usage::default()
        })));
        steps.push(ev(ChatEvent::Stop(stop)));
        steps
    }

    fn fs_mcp() -> Arc<FakeMcp> {
        Arc::new(FakeMcp {
            tools: vec![
                tool("fs", "read", true),
                tool("fs", "list", true),
                tool("fs", "write", false),
            ],
            ..FakeMcp::default()
        })
    }

    async fn wait_approval(f: &Fixture, id: &str) -> ToolApprovalRequest {
        let id = id.to_owned();
        f.sink
            .wait_for(|r| {
                r.events()
                    .iter()
                    .any(|e| matches!(e, StreamEvent::ToolApproval { request } if request.tool_use_id == id))
            })
            .await;
        f.service
            .pending_approvals()
            .into_iter()
            .find(|r| r.tool_use_id == id)
            .expect("pending")
    }

    fn saved(f: &Fixture) -> Message {
        match f.sink.events().last().cloned() {
            Some(StreamEvent::Done { message: Some(m) }) => m,
            other => panic!("ends with a saved reply: {other:?}"),
        }
    }

    #[tokio::test]
    async fn tools_run_after_approval_and_the_model_continues() {
        let mcp = fs_mcp();
        let mut first = vec![text("Let me look.")];
        first.extend(call("c1", "fs__write", "{\"path\":\"a.txt\"}"));
        let f = fixture_with(
            vec![
                turn(first, StopReason::ToolUse),
                turn(vec![text("Done: wrote it.")], StopReason::EndTurn),
                vec![text("Title"), ev(ChatEvent::Stop(StopReason::EndTurn))],
            ],
            Some(mcp.clone()),
        )
        .await;
        let reply_id = f
            .service
            .reply(&f.chat.id, &f.first.id, GenerateOptions::default())
            .await
            .expect("starts");
        let ask = wait_approval(&f, "c1").await;
        assert_eq!(ask.server.as_deref(), Some("fs"));
        assert_eq!(ask.tool, "write");
        assert_eq!(ask.message_id, reply_id);
        assert!(
            ask.input_json.contains("\"path\": \"a.txt\""),
            "{}",
            ask.input_json
        );
        assert!(
            !f.service
                .answer_approval("other-chat", "c1", ApprovalDecision::AllowOnce)
        );
        assert!(
            f.service
                .answer_approval(&f.chat.id, "c1", ApprovalDecision::AllowOnce)
        );
        f.sink.wait_finished().await;

        let m = saved(&f);
        assert_eq!(m.stop_reason, Some(StopReason::EndTurn));
        assert_eq!(m.content.len(), 4, "{:?}", m.content);
        assert_eq!(m.content[0], ContentBlock::text("Let me look."));
        assert!(
            matches!(&m.content[1], ContentBlock::ToolUse { id, server: Some(s), tool: Some(t), .. }
            if id == "c1" && s == "fs" && t == "write")
        );
        assert!(
            matches!(&m.content[2], ContentBlock::ToolResult { tool_use_id, is_error: false, .. }
            if tool_use_id == "c1")
        );
        assert_eq!(m.content[3], ContentBlock::text("Done: wrote it."));
        // Two steps' usage, added up.
        assert_eq!(m.usage.map(|u| u.input_tokens), Some(20));
        assert_eq!(
            mcp.calls.lock().expect("lock")[0].2,
            serde_json::json!({"path": "a.txt"})
        );

        let reqs = f.fake.requests();
        let names: Vec<&str> = reqs[0].tools.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "web_fetch",
                "artifact",
                "task_list",
                "fs__list",
                "fs__read",
                "fs__write"
            ]
        );
        assert!(
            reqs[0]
                .system
                .as_deref()
                .is_some_and(|s| s.contains("connectors: fs"))
        );
        // The second request carries the call and its result.
        let second = &reqs[1].messages;
        assert_eq!(second.len(), 3);
        assert_eq!(second[1].role, Role::Assistant);
        assert!(matches!(second[1].content[1], ContentBlock::ToolUse { .. }));
        assert!(matches!(
            second[2].content[0],
            ContentBlock::ToolResult { .. }
        ));

        let events = f.sink.events();
        let kinds: Vec<&str> = events
            .iter()
            .filter_map(|e| match e {
                StreamEvent::ToolUse { .. } => Some("use"),
                StreamEvent::ToolCall { .. } => Some("call"),
                StreamEvent::ToolApproval { .. } => Some("ask"),
                StreamEvent::ToolRunning { .. } => Some("run"),
                StreamEvent::ToolResult { .. } => Some("result"),
                _ => None,
            })
            .collect();
        assert_eq!(kinds, vec!["use", "call", "ask", "run", "result"]);

        let used = f.store.tool_usage(&f.chat.id).await.expect("usage");
        assert_eq!(used.len(), 1);
        assert_eq!(used[0].label.as_deref(), Some("a.txt"));
        assert_eq!(used[0].message_id, reply_id);
        assert!(f.service.pending_approvals().is_empty());

        // The next reply sees the tool turns in its history.
        let next = f
            .service
            .send(
                &f.chat.id,
                Some(&reply_id),
                vec![ContentBlock::text("thanks")],
                GenerateOptions::default(),
            )
            .await;
        assert!(next.is_ok());
    }

    #[tokio::test]
    async fn denied_calls_tell_the_model_and_allow_always_sticks() {
        let mut first = call("c1", "fs__write", "{}");
        first.extend(call("c2", "fs__write", "{\"path\":\"b\"}"));
        let f = fixture_with(
            vec![
                turn(first, StopReason::ToolUse),
                turn(
                    call("c3", "fs__write", "{\"path\":\"c\"}"),
                    StopReason::ToolUse,
                ),
                turn(vec![text("ok")], StopReason::EndTurn),
                vec![text("T"), ev(ChatEvent::Stop(StopReason::EndTurn))],
            ],
            Some(fs_mcp()),
        )
        .await;
        f.service
            .reply(&f.chat.id, &f.first.id, GenerateOptions::default())
            .await
            .expect("starts");
        wait_approval(&f, "c1").await;
        f.service
            .answer_approval(&f.chat.id, "c1", ApprovalDecision::Deny);
        wait_approval(&f, "c2").await;
        f.service
            .answer_approval(&f.chat.id, "c2", ApprovalDecision::AllowChat);
        // c3 runs without asking.
        f.sink.wait_finished().await;
        let asks = f
            .sink
            .events()
            .iter()
            .filter(|e| matches!(e, StreamEvent::ToolApproval { .. }))
            .count();
        assert_eq!(asks, 2);
        let m = saved(&f);
        let results: Vec<(String, bool)> = m
            .content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::ToolResult {
                    tool_use_id,
                    is_error,
                    ..
                } => Some((tool_use_id.clone(), *is_error)),
                _ => None,
            })
            .collect();
        assert_eq!(
            results,
            vec![
                ("c1".into(), true),
                ("c2".into(), false),
                ("c3".into(), false)
            ]
        );
        assert!(plain_text(&m.content).contains("declined"));
        assert_eq!(
            f.store.chat_tool_grants(&f.chat.id).await.expect("grants"),
            vec!["fs/write".to_owned()]
        );
    }

    #[tokio::test]
    async fn read_only_tools_can_run_unasked_and_in_parallel() {
        let mcp = Arc::new(FakeMcp {
            tools: vec![tool("fs", "read", true), tool("fs", "list", true)],
            delay_ms: 300,
            ..FakeMcp::default()
        });
        let mut first = call("r1", "fs__read", "{\"path\":\"a\"}");
        first.extend(call("r2", "fs__list", "{\"path\":\"b\"}"));
        let f = fixture_with(
            vec![
                turn(first, StopReason::ToolUse),
                turn(vec![text("both read")], StopReason::EndTurn),
                vec![text("T"), ev(ChatEvent::Stop(StopReason::EndTurn))],
            ],
            Some(mcp.clone()),
        )
        .await;
        let mut settings = f.store.app_settings().await.expect("settings");
        settings.tools.auto_allow_read_only = true;
        f.store.save_app_settings(settings).await.expect("saves");
        let started = Instant::now();
        f.service
            .reply(&f.chat.id, &f.first.id, GenerateOptions::default())
            .await
            .expect("starts");
        f.sink.wait_finished().await;
        assert!(
            started.elapsed() < Duration::from_millis(550),
            "ran side by side"
        );
        assert_eq!(mcp.calls.lock().expect("lock").len(), 2);
        assert!(
            !f.sink
                .events()
                .iter()
                .any(|e| matches!(e, StreamEvent::ToolApproval { .. }))
        );
        assert_eq!(saved(&f).stop_reason, Some(StopReason::EndTurn));
    }

    #[tokio::test]
    async fn stop_while_waiting_keeps_a_valid_reply() {
        let f = fixture_with(
            vec![turn(call("c1", "fs__write", "{}"), StopReason::ToolUse)],
            Some(fs_mcp()),
        )
        .await;
        f.service
            .reply(&f.chat.id, &f.first.id, GenerateOptions::default())
            .await
            .expect("starts");
        wait_approval(&f, "c1").await;
        assert!(f.service.stop(&f.chat.id));
        f.sink.wait_finished().await;
        let m = saved(&f);
        assert_eq!(m.stop_reason, Some(StopReason::Stopped));
        assert!(matches!(
            &m.content[1],
            ContentBlock::ToolResult { is_error: true, .. }
        ));
        assert!(plain_text(&m.content).contains("stopped the reply"));
        assert!(f.service.pending_approvals().is_empty());
        assert!(!f.service.is_running(&f.chat.id));
    }

    #[tokio::test]
    async fn forgetting_a_chat_drops_its_waiting_approvals() {
        let f = fixture_with(
            vec![turn(call("c1", "fs__write", "{}"), StopReason::ToolUse)],
            Some(fs_mcp()),
        )
        .await;
        f.service
            .reply(&f.chat.id, &f.first.id, GenerateOptions::default())
            .await
            .expect("starts");
        wait_approval(&f, "c1").await;
        f.service.forget(&f.chat.id);
        // Gone at once, before the reply has wound down.
        assert!(f.service.pending_approvals().is_empty());
        f.sink.wait_finished().await;
        assert!(!f.service.is_running(&f.chat.id));
    }

    #[tokio::test]
    async fn step_limit_bad_json_and_unknown_tools() {
        let mut first = call("u1", "nope__tool", "{}");
        first.extend(call("j1", "fs__read", "{not json"));
        let f = fixture_with(
            vec![
                turn(first, StopReason::ToolUse),
                turn(call("r1", "fs__read", "{}"), StopReason::ToolUse),
                turn(vec![text("never")], StopReason::EndTurn),
            ],
            Some(fs_mcp()),
        )
        .await;
        let mut settings = f.store.app_settings().await.expect("settings");
        settings.tools.max_steps = 1;
        f.store.save_app_settings(settings).await.expect("saves");
        f.service
            .reply(&f.chat.id, &f.first.id, GenerateOptions::default())
            .await
            .expect("starts");
        f.sink.wait_finished().await;
        let m = saved(&f);
        assert_eq!(m.stop_reason, Some(StopReason::StepLimit));
        let text = plain_text(&m.content);
        assert!(
            text.contains("There's no tool called “nope__tool”"),
            "{text}"
        );
        assert!(text.contains("weren't valid JSON"), "{text}");
        assert!(text.contains("limit of 1 tool steps"), "{text}");
        // Two model steps (a title request may follow; it has no tools).
        let steps = f
            .fake
            .requests()
            .iter()
            .filter(|r| !r.tools.is_empty())
            .count();
        assert_eq!(steps, 2);
    }

    #[tokio::test]
    async fn local_models_that_stop_after_calls_still_get_results() {
        let f = fixture_with(
            vec![
                turn(call("r1", "fs__read", "{}"), StopReason::EndTurn),
                turn(vec![text("read")], StopReason::EndTurn),
                vec![text("T"), ev(ChatEvent::Stop(StopReason::EndTurn))],
            ],
            Some(fs_mcp()),
        )
        .await;
        f.store
            .grant_chat_tool(&f.chat.id, "fs/read")
            .await
            .expect("grant");
        f.service
            .reply(&f.chat.id, &f.first.id, GenerateOptions::default())
            .await
            .expect("starts");
        f.sink.wait_finished().await;
        assert_eq!(saved(&f).content.last(), Some(&ContentBlock::text("read")));
    }

    #[tokio::test]
    async fn provider_tools_are_shown_and_logged() {
        let f = fixture(vec![
            turn(
                vec![
                    ev(ChatEvent::ServerToolStart {
                        id: "s1".into(),
                        name: "web_search".into(),
                    }),
                    ev(ChatEvent::ToolUseDelta {
                        id: "s1".into(),
                        partial_json: "{\"query\":\"oxide\"}".into(),
                    }),
                    ev(ChatEvent::ToolUseEnd { id: "s1".into() }),
                    ev(ChatEvent::ServerToolResult {
                        tool_use_id: "s1".into(),
                        raw_json: r#"{"type":"web_search_tool_result","tool_use_id":"s1","content":[{"type":"web_search_result","title":"Oxide","url":"https://o.example"}]}"#.into(),
                    }),
                ],
                StopReason::PauseTurn,
            ),
            turn(vec![text("Found it.")], StopReason::EndTurn),
            vec![text("T"), ev(ChatEvent::Stop(StopReason::EndTurn))],
        ])
        .await;
        f.service
            .reply(&f.chat.id, &f.first.id, GenerateOptions::default())
            .await
            .expect("starts");
        f.sink.wait_finished().await;
        let m = saved(&f);
        assert!(matches!(m.content[0], ContentBlock::ServerToolUse { .. }));
        assert!(matches!(
            m.content[1],
            ContentBlock::ServerToolResult { .. }
        ));
        // The paused turn went back with the server blocks.
        let second = &f.fake.requests()[1].messages;
        assert!(matches!(
            second.last().map(|m| &m.content[0]),
            Some(ContentBlock::ServerToolUse { .. })
        ));
        assert!(f.sink.events().iter().any(|e| matches!(e,
            StreamEvent::ToolCall { server: Some(s), .. } if s == "anthropic")));
        let used = f.store.tool_usage(&f.chat.id).await.expect("usage");
        assert_eq!(used[0].kind, ToolUsageKind::WebSearch);
        assert_eq!(used[0].label.as_deref(), Some("oxide"));
    }

    #[tokio::test]
    async fn calls_of_a_step_wait_for_approval_side_by_side() {
        let mut first = call("w1", "fs__write", "{\"path\":\"a\"}");
        first.extend(call("w2", "fs__write", "{\"path\":\"b\"}"));
        first.extend(call("r1", "fs__read", "{\"path\":\"c\"}"));
        let f = fixture_with(
            vec![
                turn(first, StopReason::ToolUse),
                turn(vec![text("done")], StopReason::EndTurn),
                vec![text("T"), ev(ChatEvent::Stop(StopReason::EndTurn))],
            ],
            Some(fs_mcp()),
        )
        .await;
        f.store
            .grant_chat_tool(&f.chat.id, "fs/read")
            .await
            .expect("grant");
        f.service
            .reply(&f.chat.id, &f.first.id, GenerateOptions::default())
            .await
            .expect("starts");
        // Both cards wait at once, and the read ran without waiting for them.
        wait_approval(&f, "w1").await;
        wait_approval(&f, "w2").await;
        f.sink
            .wait_for(|r| {
                r.events()
                    .iter()
                    .any(|e| matches!(e, StreamEvent::ToolResult { id, .. } if id == "r1"))
            })
            .await;
        assert_eq!(f.service.pending_approvals().len(), 2);
        // "Allow always" on the second card answers the first too.
        assert!(
            f.service
                .answer_approval(&f.chat.id, "w2", ApprovalDecision::AllowChat)
        );
        f.sink.wait_finished().await;
        let m = saved(&f);
        let results: Vec<(String, bool)> = m
            .content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::ToolResult {
                    tool_use_id,
                    is_error,
                    ..
                } => Some((tool_use_id.clone(), *is_error)),
                _ => None,
            })
            .collect();
        assert_eq!(
            results,
            vec![
                ("w1".into(), false),
                ("w2".into(), false),
                ("r1".into(), false)
            ]
        );
        assert_eq!(m.content.last(), Some(&ContentBlock::text("done")));
    }

    // ---- Agentic work: folders, file tools, task list, shell -------------------------------

    fn result_of(m: &Message, id: &str) -> (String, bool) {
        m.content
            .iter()
            .find_map(|b| match b {
                ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    is_error,
                } if tool_use_id == id => Some((text_blocks(content), *is_error)),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no result for {id}"))
    }

    #[tokio::test]
    async fn workspace_tools_stay_in_granted_folders_and_ask_before_changes() {
        let tmp = tempfile::tempdir().expect("tmp");
        let work = tmp.path().join("work");
        std::fs::create_dir_all(&work).expect("mkdir");
        std::fs::create_dir_all(tmp.path().join("outside")).expect("mkdir");
        std::fs::write(work.join("a.txt"), "alpha").expect("write");
        std::fs::write(tmp.path().join("outside/secret.txt"), "no").expect("write");

        let mut first = call(
            "t1",
            "task_list",
            r#"{"tasks":[{"title":"Look","status":"in_progress"},{"title":"Sort","status":"pending"}]}"#,
        );
        first.extend(call("l1", "list_dir", "{}"));
        first.extend(call(
            "e1",
            "read_file",
            r#"{"path":"../outside/secret.txt"}"#,
        ));
        first.extend(call(
            "m1",
            "move_file",
            r#"{"from":"a.txt","to":"Text/a.txt"}"#,
        ));
        first.extend(call("d1", "delete_file", r#"{"path":"Text"}"#));
        let f = fixture(vec![
            turn(first, StopReason::ToolUse),
            turn(vec![text("Sorted.")], StopReason::EndTurn),
            title_step(),
        ])
        .await;
        let root = crate::tools::files::resolve_grant(&work.to_string_lossy()).expect("grant");
        f.store
            .add_chat_folder(&f.chat.id, &root)
            .await
            .expect("adds");
        f.service
            .reply(&f.chat.id, &f.first.id, GenerateOptions::default())
            .await
            .expect("starts");
        // Only the changes ask: the move and the delete.
        let mv = wait_approval(&f, "m1").await;
        assert_eq!(mv.tool, "move_file");
        assert!(mv.server.is_none());
        let del = wait_approval(&f, "d1").await;
        assert!(del.destructive);
        assert_eq!(f.service.pending_approvals().len(), 2);
        assert!(
            f.service
                .answer_approval(&f.chat.id, "m1", ApprovalDecision::AllowOnce)
        );
        assert!(
            f.service
                .answer_approval(&f.chat.id, "d1", ApprovalDecision::Deny)
        );
        f.sink.wait_finished().await;

        let reqs = f.fake.requests();
        let names: Vec<&str> = reqs[0].tools.iter().map(|t| t.name.as_str()).collect();
        for t in [
            "task_list",
            "list_dir",
            "read_file",
            "move_file",
            "delete_file",
        ] {
            assert!(names.contains(&t), "{t} in {names:?}");
        }
        assert!(
            !names.contains(&"run_command"),
            "the shell is off by default"
        );
        let system = reqs[0].system.as_deref().expect("system");
        assert!(
            system.contains(&format!("<shared_folders>\n- {root}\n")),
            "{system}"
        );
        assert!(system.contains("task_list"), "{system}");

        let m = saved(&f);
        assert!(result_of(&m, "t1").0.contains("0 of 2 done"));
        assert!(result_of(&m, "l1").0.contains("a.txt"));
        let (escape, err) = result_of(&m, "e1");
        assert!(err && escape.contains("outside"), "{escape}");
        assert!(!result_of(&m, "m1").1);
        assert!(work.join("Text/a.txt").exists(), "moved");
        assert!(result_of(&m, "d1").1, "denied");
        assert!(work.join("Text").exists(), "not deleted");

        let tasks = f.sink.tasks.lock().expect("lock").clone();
        assert_eq!(tasks.len(), 1);
        assert_eq!(
            f.store
                .task_list(&f.chat.id)
                .await
                .expect("list")
                .items
                .len(),
            2
        );
        let used = f.store.tool_usage(&f.chat.id).await.expect("usage");
        let kinds: Vec<(ToolUsageKind, Option<String>)> =
            used.iter().map(|u| (u.kind, u.label.clone())).collect();
        assert!(
            kinds.contains(&(ToolUsageKind::Tasks, Some("Look".into()))),
            "{kinds:?}"
        );
        assert!(kinds.contains(&(ToolUsageKind::File, Some("a.txt → Text/a.txt".into()))));
        assert!(kinds.contains(&(ToolUsageKind::File, Some(".".into()))));
    }

    #[tokio::test]
    async fn no_folders_no_file_tools_and_switches_apply() {
        let f = fixture(vec![
            turn(vec![text("Hi.")], StopReason::EndTurn),
            title_step(),
            turn(vec![text("Again.")], StopReason::EndTurn),
        ])
        .await;
        let mut settings = f.store.app_settings().await.expect("settings");
        settings.shell.enabled = true;
        f.store.save_app_settings(settings).await.expect("saves");
        f.service
            .reply(&f.chat.id, &f.first.id, GenerateOptions::default())
            .await
            .expect("starts");
        f.sink.wait_finished().await;
        let names: Vec<String> = f.fake.requests()[0]
            .tools
            .iter()
            .map(|t| t.name.clone())
            .collect();
        assert!(names.contains(&"task_list".to_owned()));
        assert!(
            !names.iter().any(|n| n == "list_dir" || n == "run_command"),
            "{names:?}"
        );

        // With a folder, but files and the task list switched off in the chat.
        let tmp = tempfile::tempdir().expect("tmp");
        let root =
            crate::tools::files::resolve_grant(&tmp.path().to_string_lossy()).expect("grant");
        f.store
            .add_chat_folder(&f.chat.id, &root)
            .await
            .expect("adds");
        f.store
            .set_chat_tools(
                &f.chat.id,
                &oxide_core::ChatTools {
                    disabled_builtins: vec!["files".into(), "task_list".into()],
                    ..Default::default()
                },
            )
            .await
            .expect("switches");
        f.sink.events.lock().expect("lock").clear();
        f.service
            .regenerate(&f.first.id, GenerateOptions::default())
            .await
            .expect("starts");
        f.sink.wait_finished().await;
        let reqs = f.fake.requests();
        let names: Vec<&str> = reqs
            .last()
            .expect("req")
            .tools
            .iter()
            .map(|t| t.name.as_str())
            .collect();
        assert!(names.contains(&"run_command"), "{names:?}");
        assert!(
            !names.contains(&"list_dir") && !names.contains(&"task_list"),
            "{names:?}"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn shell_commands_ask_unless_allowlisted_one_command_at_a_time() {
        let tmp = tempfile::tempdir().expect("tmp");
        let mut first = call("s1", "run_command", r#"{"command":"echo hi"}"#);
        first.extend(call(
            "s2",
            "run_command",
            r#"{"command":"echo hi > out.txt"}"#,
        ));
        first.extend(call("s3", "run_command", r#"{"command":"pwd"}"#));
        let mut second = call("s4", "run_command", r#"{"command":"pwd"}"#);
        second.extend(call("s5", "run_command", r#"{"command":"ls"}"#));
        let f = fixture(vec![
            turn(first, StopReason::ToolUse),
            turn(second, StopReason::ToolUse),
            turn(vec![text("Done.")], StopReason::EndTurn),
            title_step(),
        ])
        .await;
        let mut settings = f.store.app_settings().await.expect("settings");
        settings.shell.enabled = true;
        settings.shell.allowlist = vec!["echo".into()];
        f.store.save_app_settings(settings).await.expect("saves");
        let root =
            crate::tools::files::resolve_grant(&tmp.path().to_string_lossy()).expect("grant");
        f.store
            .add_chat_folder(&f.chat.id, &root)
            .await
            .expect("adds");
        f.service
            .reply(&f.chat.id, &f.first.id, GenerateOptions::default())
            .await
            .expect("starts");
        // `echo hi` is allowlisted; a redirect isn't, nor is `pwd`.
        wait_approval(&f, "s2").await;
        wait_approval(&f, "s3").await;
        assert_eq!(f.service.pending_approvals().len(), 2);
        // "Allow always" for `pwd` doesn't answer the other command's card.
        assert!(
            f.service
                .answer_approval(&f.chat.id, "s3", ApprovalDecision::AllowChat)
        );
        assert_eq!(f.service.pending_approvals().len(), 1);
        assert!(
            f.service
                .answer_approval(&f.chat.id, "s2", ApprovalDecision::Deny)
        );
        // Next step: `pwd` runs without asking now, `ls` asks.
        wait_approval(&f, "s5").await;
        assert_eq!(f.service.pending_approvals().len(), 1);
        assert!(
            f.service
                .answer_approval(&f.chat.id, "s5", ApprovalDecision::AllowOnce)
        );
        f.sink.wait_finished().await;
        let m = saved(&f);
        assert!(result_of(&m, "s1").0.contains("hi"));
        assert!(result_of(&m, "s2").1);
        assert!(!tmp.path().join("out.txt").exists());
        assert!(result_of(&m, "s4").0.contains("Exit code 0"));
        let used = f.store.tool_usage(&f.chat.id).await.expect("usage");
        assert!(
            used.iter()
                .any(|u| u.kind == ToolUsageKind::Shell && u.label.as_deref() == Some("echo hi"))
        );
        let system = f.fake.requests()[0].system.clone().unwrap_or_default();
        assert!(system.contains("run_command (sh"), "{system}");
    }

    // ---- Docker sandbox ---------------------------------------------------------------------

    /// Like `fixture`, with the sandbox's tools (Docker as `docker`; `None` searches PATH).
    async fn fixture_sandbox(
        scripts: Vec<Vec<Step>>,
        root: &std::path::Path,
        docker: Option<std::path::PathBuf>,
    ) -> (Fixture, oxide_sandbox::Sandboxes) {
        let store = Store::open_in_memory().await.expect("store");
        let secrets: Arc<dyn SecretStore> = Arc::new(MemorySecrets::default());
        let fake = FakeProvider::new(scripts);
        let registry = Arc::new(ModelRegistry::with_connector(
            store.clone(),
            secrets.clone(),
            fake_connector(fake.clone()),
        ));
        let sink = Arc::new(Recorder::default());
        let sandboxes = oxide_sandbox::Sandboxes::with_docker(
            root.join("sandboxes"),
            None,
            Arc::new(oxide_sandbox::NoSink),
            docker,
        );
        let tools = Tools::new(None, secrets, None).with_sandboxes(sandboxes.clone());
        let service = ChatService::new(store.clone(), registry, sink.clone(), tools);
        let (chat, first) = store
            .start_conversation(StartConversation {
                text: "Build it".into(),
                model: "lmstudio/fake-model".into(),
                project_id: None,
                attachments: Vec::new(),
                folders: vec![],
            })
            .await
            .expect("chat");
        let f = Fixture {
            store,
            service,
            fake,
            sink,
            chat,
            first,
        };
        (f, sandboxes)
    }

    #[tokio::test]
    async fn sandbox_tools_follow_the_setting_and_explain_a_missing_docker() {
        let tmp = tempfile::tempdir().expect("tmp");
        let (f, _sb) = fixture_sandbox(
            vec![
                turn(
                    call("x1", "sandbox_exec", r#"{"command":"make test"}"#),
                    StopReason::ToolUse,
                ),
                turn(vec![text("Docker is needed.")], StopReason::EndTurn),
                title_step(),
            ],
            tmp.path(),
            Some(tmp.path().join("no-docker")),
        )
        .await;
        let mut settings = f.store.app_settings().await.expect("settings");
        settings.sandbox.enabled = true;
        f.store.save_app_settings(settings).await.expect("saves");
        f.service
            .reply(&f.chat.id, &f.first.id, GenerateOptions::default())
            .await
            .expect("starts");
        // Commands ask by default (Settings → Sandbox).
        let req = wait_approval(&f, "x1").await;
        assert_eq!(req.tool, "sandbox_exec");
        assert!(
            f.service
                .answer_approval(&f.chat.id, "x1", ApprovalDecision::AllowOnce)
        );
        f.sink.wait_finished().await;
        let m = saved(&f);
        let (r, bad) = result_of(&m, "x1");
        assert!(bad);
        assert!(
            r.contains("Docker isn't installed") && r.contains("tell the user"),
            "{r}"
        );
        let reqs = f.fake.requests();
        let names: Vec<&str> = reqs[0].tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"sandbox_exec"), "{names:?}");
        // Copies need a shared folder; file tools need Docker.
        assert!(
            !names.iter().any(|n| n.starts_with("sandbox_copy")),
            "{names:?}"
        );
        assert!(
            !names.iter().any(|n| n.starts_with("sandbox__")),
            "{names:?}"
        );
        let system = reqs[0].system.clone().unwrap_or_default();
        assert!(system.contains("This chat has a sandbox"), "{system}");
        let used = f.store.tool_usage(&f.chat.id).await.expect("usage");
        assert!(used.iter().any(|u| u.kind == ToolUsageKind::Sandbox
            && u.label.as_deref() == Some("make test")
            && u.is_error));
    }

    // ---- Browser agent -----------------------------------------------------------------------

    /// Like `fixture`, with the browser agent's tools in a Python environment `env`.
    #[cfg(unix)]
    async fn fixture_browser(
        scripts: Vec<Vec<Step>>,
        env: oxide_pyenv::PythonEnv,
        installed: Vec<oxide_core::InstalledBrowser>,
    ) -> (Fixture, oxide_pyenv::BrowserAgents) {
        let store = Store::open_in_memory().await.expect("store");
        let secrets: Arc<dyn SecretStore> = Arc::new(MemorySecrets::default());
        let fake = FakeProvider::new(scripts);
        let registry = Arc::new(ModelRegistry::with_connector(
            store.clone(),
            secrets.clone(),
            fake_connector(fake.clone()),
        ));
        let sink = Arc::new(Recorder::default());
        let browser = oxide_pyenv::BrowserAgents::with_installed(
            env,
            installed,
            Arc::new(oxide_pyenv::NoBrowserSink),
        );
        let tools = Tools::new(None, secrets, None).with_browser(browser.clone());
        let service = ChatService::new(store.clone(), registry, sink.clone(), tools);
        let (chat, first) = store
            .start_conversation(StartConversation {
                text: "Find the opening hours".into(),
                model: "lmstudio/fake-model".into(),
                project_id: None,
                attachments: Vec::new(),
                folders: vec![],
            })
            .await
            .expect("chat");
        (
            Fixture {
                store,
                service,
                fake,
                sink,
                chat,
                first,
            },
            browser,
        )
    }

    /// A Python environment whose `uv` always fails (nothing can be installed).
    #[cfg(unix)]
    fn broken_python(root: &std::path::Path) -> oxide_pyenv::PythonEnv {
        use std::os::unix::fs::PermissionsExt;
        let uv = root.join("uv");
        std::fs::write(&uv, "#!/bin/sh\necho 'error: no network' >&2\nexit 2\n").expect("write");
        std::fs::set_permissions(&uv, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        let mut cfg = oxide_pyenv::PythonConfig::new(root.join("python"));
        cfg.uv = Some(uv);
        oxide_pyenv::PythonEnv::new(cfg, Arc::new(oxide_pyenv::NoSink))
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn browser_tools_follow_the_setting_ask_for_actions_and_explain_a_failed_setup() {
        let tmp = tempfile::tempdir().expect("tmp");
        let mut both = call(
            "b1",
            "browser__navigate",
            r#"{"url":"https://example.com"}"#,
        );
        both.extend(call("b2", "browser__read_page", "{}"));
        let (f, _b) = fixture_browser(
            vec![
                turn(both, StopReason::ToolUse),
                turn(
                    vec![text("The browser couldn't start.")],
                    StopReason::EndTurn,
                ),
                title_step(),
            ],
            broken_python(tmp.path()),
            Vec::new(),
        )
        .await;
        let mut settings = f.store.app_settings().await.expect("settings");
        settings.browser.enabled = true;
        f.store.save_app_settings(settings).await.expect("saves");
        f.service
            .reply(&f.chat.id, &f.first.id, GenerateOptions::default())
            .await
            .expect("starts");
        // Opening a page asks; reading it doesn't.
        let req = wait_approval(&f, "b1").await;
        assert_eq!(req.tool, "navigate");
        assert_eq!(req.server.as_deref(), Some("browser"));
        assert_eq!(f.service.pending_approvals().len(), 1);
        assert!(
            f.service
                .answer_approval(&f.chat.id, "b1", ApprovalDecision::AllowOnce)
        );
        f.sink.wait_finished().await;
        let m = saved(&f);
        for id in ["b1", "b2"] {
            let (r, bad) = result_of(&m, id);
            assert!(bad, "{r}");
            assert!(
                r.contains("Setting up the browser failed") || r.contains("uv"),
                "{r}"
            );
        }
        let reqs = f.fake.requests();
        let names: Vec<&str> = reqs[0].tools.iter().map(|t| t.name.as_str()).collect();
        for want in [
            "browser__browser_task",
            "browser__navigate",
            "browser__read_page",
            "browser__click",
            "browser__type",
            "browser__screenshot",
            "browser__close",
        ] {
            assert!(names.contains(&want), "{names:?}");
        }
        let system = reqs[0].system.clone().unwrap_or_default();
        assert!(system.contains("browser__navigate opens"), "{system}");
        let used = f.store.tool_usage(&f.chat.id).await.expect("usage");
        assert!(used.iter().any(|u| u.kind == ToolUsageKind::Browser
            && u.label.as_deref() == Some("https://example.com")
            && u.server.as_deref() == Some("browser")
            && u.is_error));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn browser_tools_are_off_by_default_and_per_chat() {
        let tmp = tempfile::tempdir().expect("tmp");
        let (f, _b) = fixture_browser(
            vec![
                turn(vec![text("ok")], StopReason::EndTurn),
                title_step(),
                turn(vec![text("ok")], StopReason::EndTurn),
            ],
            broken_python(tmp.path()),
            Vec::new(),
        )
        .await;
        f.service
            .reply(&f.chat.id, &f.first.id, GenerateOptions::default())
            .await
            .expect("starts");
        f.sink.wait_finished().await;
        let reqs = f.fake.requests();
        assert!(
            reqs[0]
                .tools
                .iter()
                .all(|t| !t.name.starts_with("browser__"))
        );
        assert!(
            !reqs[0]
                .system
                .clone()
                .unwrap_or_default()
                .contains("browser__")
        );
        // On in Settings, off in this chat ("+" menu): still not offered.
        let mut settings = f.store.app_settings().await.expect("settings");
        settings.browser.enabled = true;
        f.store.save_app_settings(settings).await.expect("saves");
        let mut tools = f.store.chat_tools(&f.chat.id).await.expect("tools");
        tools.disabled_builtins.push(BROWSER_SWITCH.into());
        f.store
            .set_chat_tools(&f.chat.id, &tools)
            .await
            .expect("saves");
        // Wait for the title too, so the next reply gets the next script.
        f.sink
            .wait_for(|r| {
                r.events()
                    .iter()
                    .any(|e| matches!(e, StreamEvent::Done { .. }))
            })
            .await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        f.service
            .reply(&f.chat.id, &f.first.id, GenerateOptions::default())
            .await
            .expect("starts");
        f.sink
            .wait_for(|r| {
                r.events()
                    .iter()
                    .filter(|e| matches!(e, StreamEvent::Done { .. }))
                    .count()
                    >= 2
            })
            .await;
        let reqs = f.fake.requests();
        let last = reqs.last().expect("a request");
        assert!(last.tools.iter().all(|t| !t.name.starts_with("browser__")));
    }

    /// A stand-in for Browser-Use that reports the model it was given.
    #[cfg(unix)]
    const ECHO_BROWSER_USE: &str = r#"
class ChatOpenAI:
    def __init__(self, model, api_key=None, base_url=None):
        self.model, self.base_url, self.api_key = model, base_url, api_key
class ChatAnthropic(ChatOpenAI):
    pass
class Browser:
    def __init__(self, cdp_url=None, keep_alive=None):
        self.cdp_url = cdp_url
class History:
    def __init__(self, t): self.t = t
    def final_result(self): return self.t
    def is_done(self): return True
    def is_successful(self): return True
class Agent:
    def __init__(self, task, llm=None, browser=None, use_vision=True):
        self.llm, self.browser, self.vision = llm, browser, use_vision
    async def run(self, max_steps=100):
        return History(f"model={self.llm.model} base={self.llm.base_url} key={self.llm.api_key} vision={self.vision} steps={max_steps}")
"#;

    /// `browser_task` through a chat: the chat's model reaches Browser-Use. Needs a Python with
    /// `mcp` 2.x and Playwright (`OXIDE_TEST_SYSTEM_PYTHON`) and a Chrome (`OXIDE_TEST_CHROME`).
    #[cfg(unix)]
    #[tokio::test]
    #[ignore = "needs Python with mcp and Playwright, and Chrome"]
    async fn browser_task_through_a_chat_uses_the_chats_model() {
        let py = std::env::var_os("OXIDE_TEST_SYSTEM_PYTHON").expect("OXIDE_TEST_SYSTEM_PYTHON");
        let chrome = std::env::var("OXIDE_TEST_CHROME").expect("OXIDE_TEST_CHROME");
        let tmp = tempfile::tempdir().expect("tmp");
        let env = oxide_pyenv::PythonEnv::for_tests(
            &tmp.path().join("python"),
            std::path::Path::new(&py),
        );
        env.mark_group_for_tests("browser");
        let agents = tmp.path().join("python/agents/browser_use");
        std::fs::create_dir_all(&agents).expect("mkdir");
        std::fs::write(agents.join("__init__.py"), ECHO_BROWSER_USE).expect("write");
        let (f, b) = fixture_browser(
            vec![
                turn(
                    call(
                        "t1",
                        "browser__browser_task",
                        r#"{"goal":"Find the hours","max_steps":4}"#,
                    ),
                    StopReason::ToolUse,
                ),
                turn(vec![text("Found them.")], StopReason::EndTurn),
                title_step(),
            ],
            env,
            vec![oxide_core::InstalledBrowser {
                name: "Chrome".into(),
                path: chrome,
            }],
        )
        .await;
        let mut settings = f.store.app_settings().await.expect("settings");
        settings.browser.enabled = true;
        settings.browser.ask = false;
        f.store.save_app_settings(settings).await.expect("saves");
        f.service
            .reply(&f.chat.id, &f.first.id, GenerateOptions::default())
            .await
            .expect("starts");
        // Starting Python and the browser takes longer than the usual wait.
        for _ in 0..600 {
            if f.sink
                .events()
                .iter()
                .any(|e| matches!(e, StreamEvent::Done { .. } | StreamEvent::Failed { .. }))
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let m = saved(&f);
        let (r, bad) = result_of(&m, "t1");
        assert!(!bad, "{r}");
        assert!(
            r.contains(
                "model=fake-model base=http://127.0.0.1:1234/v1 key=not-needed vision=False steps=4"
            ),
            "{r}"
        );
        assert!(b.is_open(&f.chat.id));
        // Deleting the chat closes its browser.
        f.service.forget(&f.chat.id);
        for _ in 0..50 {
            if !b.is_open(&f.chat.id) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(!b.is_open(&f.chat.id));
    }

    #[tokio::test]
    async fn sandbox_tools_are_off_by_default() {
        let tmp = tempfile::tempdir().expect("tmp");
        let (f, _sb) = fixture_sandbox(
            vec![turn(vec![text("ok")], StopReason::EndTurn), title_step()],
            tmp.path(),
            Some(tmp.path().join("no-docker")),
        )
        .await;
        f.service
            .reply(&f.chat.id, &f.first.id, GenerateOptions::default())
            .await
            .expect("starts");
        f.sink.wait_finished().await;
        let reqs = f.fake.requests();
        assert!(reqs[0].tools.iter().all(|t| !t.name.starts_with("sandbox")));
        let system = reqs[0].system.clone().unwrap_or_default();
        assert!(!system.contains("sandbox"), "{system}");
    }

    /// The exit check through the chat service: a project in the sandbox, its test failing,
    /// fixed through the file server, passing, and copied out to the shared folder with
    /// approvals. Needs Docker and `OXIDE_SANDBOX_IMAGE` / `OXIDE_SANDBOX_FILES_IMAGE`
    /// (downloaded already).
    #[tokio::test]
    #[ignore = "needs Docker and the sandbox images"]
    async fn sandbox_build_fix_and_copy_out_through_a_chat() {
        let tmp = tempfile::tempdir().expect("tmp");
        let shared = tmp.path().join("shared");
        std::fs::create_dir(&shared).expect("mkdir");
        let make = r#"{"command":"mkdir -p app && printf 'def double(x):\\n    return x + 2\\n' > app/m.py && printf 'import unittest\\nfrom m import double\\nclass T(unittest.TestCase):\\n    def test(self):\\n        self.assertEqual(double(5), 10)\\nunittest.main()\\n' > app/test_m.py && cd app && python3 test_m.py"}"#;
        let fix = r#"{"path":"/workspace/app/m.py","content":"def double(x):\n    return x * 2  # fixed\n"}"#;
        let (f, sb) = fixture_sandbox(
            vec![
                turn(call("b1", "sandbox_exec", make), StopReason::ToolUse),
                turn(call("b2", "sandbox__write_file", fix), StopReason::ToolUse),
                turn(
                    call(
                        "b3",
                        "sandbox_exec",
                        r#"{"command":"python3 test_m.py","cwd":"app"}"#,
                    ),
                    StopReason::ToolUse,
                ),
                turn(
                    call("b4", "sandbox_copy_out", r#"{"from":"app"}"#),
                    StopReason::ToolUse,
                ),
                turn(vec![text("Done.")], StopReason::EndTurn),
                title_step(),
            ],
            tmp.path(),
            None,
        )
        .await;
        let mut settings = f.store.app_settings().await.expect("settings");
        settings.sandbox.enabled = true;
        if let Ok(i) = std::env::var("OXIDE_SANDBOX_IMAGE") {
            settings.sandbox.image = i;
        }
        if let Ok(i) = std::env::var("OXIDE_SANDBOX_FILES_IMAGE") {
            settings.sandbox.files_image = i;
        }
        f.store
            .save_app_settings(settings.clone())
            .await
            .expect("saves");
        sb.configure(&settings.sandbox);
        sb.prepare().await.expect("images");
        let root = crate::tools::files::resolve_grant(&shared.to_string_lossy()).expect("grant");
        f.store
            .add_chat_folder(&f.chat.id, &root)
            .await
            .expect("adds");
        f.service
            .reply(&f.chat.id, &f.first.id, GenerateOptions::default())
            .await
            .expect("starts");
        for id in ["b1", "b2", "b3", "b4"] {
            wait_approval(&f, id).await;
            assert!(
                f.service
                    .answer_approval(&f.chat.id, id, ApprovalDecision::AllowOnce)
            );
        }
        f.sink.wait_finished().await;
        let m = saved(&f);
        let (r1, bad1) = result_of(&m, "b1");
        assert!(bad1 && r1.contains("FAILED"), "{r1}");
        let (r2, bad2) = result_of(&m, "b2");
        assert!(!bad2, "{r2}");
        let (r3, bad3) = result_of(&m, "b3");
        assert!(!bad3 && r3.contains("Exit code 0"), "{r3}");
        let (r4, bad4) = result_of(&m, "b4");
        assert!(!bad4, "{r4}");
        let code = std::fs::read_to_string(shared.join("app").join("m.py")).expect("copied out");
        assert!(code.contains("x * 2"), "{code}");
        // The file server's tools were offered from the start (learned before the reply).
        let names: Vec<String> = f.fake.requests()[0]
            .tools
            .iter()
            .map(|t| t.name.clone())
            .collect();
        assert!(
            names.iter().any(|n| n == "sandbox__write_file"),
            "{names:?}"
        );
        assert!(names.iter().any(|n| n == "sandbox_copy_out"), "{names:?}");
        sb.reset(&f.chat.id).await.expect("resets");
    }

    // ---- Python ---------------------------------------------------------------------------

    #[cfg(unix)]
    fn system_python() -> Option<std::path::PathBuf> {
        let out = std::process::Command::new("sh")
            .args(["-c", "command -v python3"])
            .output()
            .ok()?;
        let p = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        (!p.is_empty()).then(|| std::path::PathBuf::from(p))
    }

    /// Like `fixture`, with `run_python` available.
    #[cfg(unix)]
    async fn fixture_python(scripts: Vec<Vec<Step>>, env: oxide_pyenv::PythonEnv) -> Fixture {
        let store = Store::open_in_memory().await.expect("store");
        let secrets: Arc<dyn SecretStore> = Arc::new(MemorySecrets::default());
        let fake = FakeProvider::new(scripts);
        let registry = Arc::new(ModelRegistry::with_connector(
            store.clone(),
            secrets.clone(),
            fake_connector(fake.clone()),
        ));
        let sink = Arc::new(Recorder::default());
        let tools = Tools::new(None, secrets, None).with_python(env);
        let service = ChatService::new(store.clone(), registry, sink.clone(), tools);
        let (chat, first) = store
            .start_conversation(StartConversation {
                text: "Plot it".into(),
                model: "lmstudio/fake-model".into(),
                project_id: None,
                attachments: Vec::new(),
                folders: vec![],
            })
            .await
            .expect("chat");
        Fixture {
            store,
            service,
            fake,
            sink,
            chat,
            first,
        }
    }

    #[cfg(unix)]
    const PNG_1X1: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

    #[cfg(unix)]
    #[tokio::test]
    async fn run_python_asks_until_allowed_and_returns_images_and_files() {
        let Some(py) = system_python() else { return };
        let tmp = tempfile::tempdir().expect("tmp");
        let env = oxide_pyenv::PythonEnv::for_tests(&tmp.path().join("python"), &py);
        let code = format!(
            "import base64\\nprint('hello')\\nopen('data.csv','w').write('a,b\\\\n1,2\\\\n')\\n\
             open('chart.png','wb').write(base64.b64decode('{PNG_1X1}'))\\nopen('big.bin','wb').write(b'x')\\n"
        );
        let first = call("p1", "run_python", &format!("{{\"code\":\"{code}\"}}"));
        let second = call("p2", "run_python", r#"{"code":"print(40 + 2)"}"#);
        let f = fixture_python(
            vec![
                turn(first, StopReason::ToolUse),
                turn(second, StopReason::ToolUse),
                turn(vec![text("Done.")], StopReason::EndTurn),
                title_step(),
            ],
            env,
        )
        .await;
        f.service
            .reply(&f.chat.id, &f.first.id, GenerateOptions::default())
            .await
            .expect("starts");
        // Code runs as the user: it asks, and shows the code.
        let req = wait_approval(&f, "p1").await;
        assert_eq!(req.tool, "run_python");
        assert!(req.input_json.contains("hello"));
        assert!(
            f.service
                .answer_approval(&f.chat.id, "p1", ApprovalDecision::AllowChat)
        );
        // "Allow always" covers the next run in this chat.
        f.sink.wait_finished().await;
        assert!(f.service.pending_approvals().is_empty());
        let m = saved(&f);
        let (r1, bad1) = result_of(&m, "p1");
        assert!(!bad1, "{r1}");
        assert!(r1.contains("Exit code 0") && r1.contains("hello"), "{r1}");
        assert!(
            r1.contains("chart.png") && r1.contains("shown to the user"),
            "{r1}"
        );
        assert!(r1.contains("data.csv") && r1.contains("artifact"), "{r1}");
        assert!(
            r1.contains("big.bin") && r1.contains("saved in the folder"),
            "{r1}"
        );
        let blocks: Vec<&ContentBlock> = m
            .content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    ..
                } if tool_use_id == "p1" => Some(content.iter()),
                _ => None,
            })
            .flatten()
            .collect();
        assert!(blocks.iter().any(
            |b| matches!(b, ContentBlock::Image { media_type, .. } if media_type == "image/png")
        ));
        assert!(
            blocks.iter().any(
                |b| matches!(b, ContentBlock::ArtifactRef { title, .. } if title == "data.csv")
            )
        );
        let (r2, bad2) = result_of(&m, "p2");
        assert!(!bad2 && r2.contains("42"), "{r2}");
        // The csv became an artifact of the chat.
        let arts = f
            .store
            .conversation_artifacts(&f.chat.id)
            .await
            .expect("artifacts");
        assert_eq!(arts.len(), 1);
        assert_eq!(arts[0].title, "data.csv");
        let used = f.store.tool_usage(&f.chat.id).await.expect("usage");
        assert!(used.iter().any(
            |u| u.kind == ToolUsageKind::Python && u.label.as_deref() == Some("import base64")
        ));
        let system = f.fake.requests()[0].system.clone().unwrap_or_default();
        assert!(system.contains("run_python"), "{system}");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn run_python_runs_in_a_shared_folder_and_stops_at_the_time_limit() {
        let Some(py) = system_python() else { return };
        let tmp = tempfile::tempdir().expect("tmp");
        let shared = tmp.path().join("shared");
        std::fs::create_dir_all(&shared).expect("mkdir");
        std::fs::write(shared.join("in.txt"), "from the folder").expect("write");
        let env = oxide_pyenv::PythonEnv::for_tests(&tmp.path().join("python"), &py);
        let mut first = call(
            "p1",
            "run_python",
            r#"{"code":"print(open('in.txt').read())"}"#,
        );
        first.extend(call(
            "p2",
            "run_python",
            r#"{"code":"import time\ntime.sleep(30)"}"#,
        ));
        first.extend(call(
            "p3",
            "run_python",
            r#"{"code":"print(1)","cwd":"../elsewhere"}"#,
        ));
        let f = fixture_python(
            vec![
                turn(first, StopReason::ToolUse),
                turn(vec![text("Done.")], StopReason::EndTurn),
                title_step(),
            ],
            env,
        )
        .await;
        let mut settings = f.store.app_settings().await.expect("settings");
        settings.python.timeout_secs = 1;
        f.store.save_app_settings(settings).await.expect("saves");
        let root = crate::tools::files::resolve_grant(&shared.to_string_lossy()).expect("grant");
        f.store
            .add_chat_folder(&f.chat.id, &root)
            .await
            .expect("adds");
        f.service
            .reply(&f.chat.id, &f.first.id, GenerateOptions::default())
            .await
            .expect("starts");
        for id in ["p1", "p2", "p3"] {
            wait_approval(&f, id).await;
        }
        assert!(
            f.service
                .answer_approval(&f.chat.id, "p1", ApprovalDecision::AllowChat)
        );
        f.sink.wait_finished().await;
        let m = saved(&f);
        let (r1, bad1) = result_of(&m, "p1");
        assert!(!bad1 && r1.contains("from the folder"), "{r1}");
        let (r2, bad2) = result_of(&m, "p2");
        assert!(bad2 && r2.contains("time limit"), "{r2}");
        let (r3, bad3) = result_of(&m, "p3");
        assert!(bad3 && r3.contains("outside"), "{r3}");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn run_python_follows_the_setting_and_its_folder_goes_with_the_chat() {
        let Some(py) = system_python() else { return };
        let tmp = tempfile::tempdir().expect("tmp");
        let env = oxide_pyenv::PythonEnv::for_tests(&tmp.path().join("python"), &py);
        let f = fixture_python(
            vec![
                turn(
                    call(
                        "p1",
                        "run_python",
                        r#"{"code":"open('note.txt','w').write('x')"}"#,
                    ),
                    StopReason::ToolUse,
                ),
                turn(vec![text("Done.")], StopReason::EndTurn),
                title_step(),
            ],
            env.clone(),
        )
        .await;
        f.service
            .reply(&f.chat.id, &f.first.id, GenerateOptions::default())
            .await
            .expect("starts");
        wait_approval(&f, "p1").await;
        assert!(
            f.service
                .answer_approval(&f.chat.id, "p1", ApprovalDecision::AllowOnce)
        );
        f.sink.wait_finished().await;
        // Without shared folders the code ran in the chat's own folder.
        let dir = env.work_dir(&f.chat.id);
        assert!(dir.join("note.txt").is_file());
        f.service.forget(&f.chat.id);
        for _ in 0..50 {
            if !dir.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(!dir.exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn run_python_is_not_offered_when_switched_off_in_settings() {
        let Some(py) = system_python() else { return };
        let tmp = tempfile::tempdir().expect("tmp");
        let env = oxide_pyenv::PythonEnv::for_tests(&tmp.path().join("python"), &py);
        let f = fixture_python(
            vec![turn(vec![text("ok")], StopReason::EndTurn), title_step()],
            env,
        )
        .await;
        let mut settings = f.store.app_settings().await.expect("settings");
        settings.python.enabled = false;
        f.store.save_app_settings(settings).await.expect("saves");
        f.service
            .reply(&f.chat.id, &f.first.id, GenerateOptions::default())
            .await
            .expect("starts");
        f.sink.wait_finished().await;
        let reqs = f.fake.requests();
        assert!(reqs[0].tools.iter().all(|t| t.name != "run_python"));
    }

    #[tokio::test]
    async fn run_python_is_not_offered_without_a_managed_environment() {
        let f = fixture(vec![
            turn(vec![text("Hi")], StopReason::EndTurn),
            title_step(),
        ])
        .await;
        f.service
            .reply(&f.chat.id, &f.first.id, GenerateOptions::default())
            .await
            .expect("starts");
        f.sink.wait_finished().await;
        let reqs = f.fake.requests();
        assert!(
            reqs[0].tools.iter().all(|t| t.name != "run_python"),
            "{:?}",
            reqs[0].tools.iter().map(|t| &t.name).collect::<Vec<_>>()
        );
    }

    // ---- Projects & memory ---------------------------------------------------------------

    fn knowledge(name: &str, text: &str) -> oxide_store::NewKnowledgeFile {
        oxide_store::NewKnowledgeFile {
            name: name.into(),
            kind: oxide_core::KnowledgeKind::Text,
            media_type: "text/markdown".into(),
            pages: None,
            note: None,
            text: text.into(),
        }
    }

    /// Puts the fixture's chat in a new project with instructions and `files`.
    async fn in_project(f: &Fixture, files: Vec<oxide_store::NewKnowledgeFile>) -> String {
        let p = f
            .store
            .create_project(oxide_core::NewProject {
                name: "Launch".into(),
                description: String::new(),
            })
            .await
            .expect("project");
        f.store
            .update_project(
                &p.id,
                oxide_core::ProjectPatch {
                    instructions: Some("Answer like a pirate.".into()),
                    ..Default::default()
                },
            )
            .await
            .expect("instructions");
        for file in files {
            f.store.add_knowledge_file(&p.id, file).await.expect("adds");
        }
        f.store
            .set_conversation_project(&f.chat.id, Some(&p.id))
            .await
            .expect("moves");
        p.id
    }

    fn title_step() -> Vec<Step> {
        vec![text("T"), ev(ChatEvent::Stop(StopReason::EndTurn))]
    }

    #[tokio::test]
    async fn small_project_knowledge_goes_into_the_prompt_in_full() {
        let f = fixture(vec![
            turn(vec![text("Arr, tangerine.")], StopReason::EndTurn),
            title_step(),
        ])
        .await;
        in_project(
            &f,
            vec![knowledge("codes.md", "The launch code is tangerine.")],
        )
        .await;
        f.service
            .reply(&f.chat.id, &f.first.id, GenerateOptions::default())
            .await
            .expect("starts");
        f.sink.wait_finished().await;
        let req = &f.fake.requests()[0];
        let context = req.system_context.as_deref().expect("knowledge");
        assert!(context.contains("<document index=\"1\" name=\"codes.md\">"));
        assert!(context.contains("The launch code is tangerine."));
        let system = req.system.as_deref().expect("system");
        assert!(system.contains("project “Launch”"));
        assert!(system.contains("Answer like a pirate."));
        assert!(!system.contains("project_search"));
        assert!(req.tools.iter().all(|t| t.name != "project_search"));
    }

    #[tokio::test]
    async fn artifacts_are_made_without_asking_and_stream_their_input() {
        let mut first = vec![text("Here's the page.")];
        first.extend(call(
            "a1",
            "artifact",
            r#"{"command":"create","id":"hello","type":"html","title":"Hello page","content":"<h1>Hi</h1>"}"#,
        ));
        let f = fixture(vec![
            turn(first, StopReason::ToolUse),
            turn(vec![text("Made it.")], StopReason::EndTurn),
            title_step(),
        ])
        .await;
        let mut settings = f.store.app_settings().await.expect("settings");
        settings.tools.ask_before_web = true; // doesn't apply to artifacts
        f.store.save_app_settings(settings).await.expect("saves");
        f.service
            .reply(&f.chat.id, &f.first.id, GenerateOptions::default())
            .await
            .expect("starts");
        f.sink.wait_finished().await;

        let reqs = f.fake.requests();
        assert!(reqs[0].tools.iter().any(|t| t.name == "artifact"));
        let system = reqs[0].system.as_deref().expect("system");
        assert!(system.contains("make artifacts"), "{system}");
        let events = f.sink.events();
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, StreamEvent::ToolApproval { .. })),
            "artifacts never ask"
        );
        // The arguments reach the UI while they stream, before the call runs.
        let streamed: String = events
            .iter()
            .filter_map(|e| match e {
                StreamEvent::ToolInputDelta { id, partial_json } if id == "a1" => {
                    Some(partial_json.as_str())
                }
                _ => None,
            })
            .collect();
        assert!(streamed.contains("<h1>Hi</h1>"), "{streamed}");
        let delta_at = events
            .iter()
            .position(|e| matches!(e, StreamEvent::ToolInputDelta { .. }));
        let call_at = events
            .iter()
            .position(|e| matches!(e, StreamEvent::ToolCall { .. }));
        assert!(delta_at < call_at);

        let artifacts = f
            .store
            .conversation_artifacts(&f.chat.id)
            .await
            .expect("lists");
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].title, "Hello page");
        let reply = saved(&f);
        let version = f
            .store
            .artifact_version(&artifacts[0].id, None)
            .await
            .expect("version");
        assert_eq!(version.message_id.as_deref(), Some(reply.id.as_str()));
        assert!(reply.content.iter().any(|b| matches!(
            b,
            ContentBlock::ToolResult { content, is_error: false, .. }
                if content.iter().any(|c| matches!(c, ContentBlock::ArtifactRef { version: 1, .. }))
        )));
        let used = f.store.tool_usage(&f.chat.id).await.expect("log");
        assert_eq!(used[0].kind, ToolUsageKind::Artifact);
        assert_eq!(used[0].label.as_deref(), Some("Hello page"));
    }

    #[tokio::test]
    async fn skills_are_listed_and_loaded_without_asking() {
        let dir = tempfile::tempdir().expect("tmp");
        let skills = crate::skills::Skills::new(dir.path().to_path_buf());
        skills
            .create(&oxide_core::SkillInput {
                name: "haiku".into(),
                description: "Use when the user asks for a haiku".into(),
                body: "Always write three lines: 5, 7 and 5 syllables. End with a season word."
                    .into(),
            })
            .expect("creates");
        let mut first = vec![text("Loading the skill.")];
        first.extend(call("s1", "load_skill", "{\"name\":\"haiku\"}"));
        let f = fixture_full(
            vec![
                turn(first, StopReason::ToolUse),
                turn(vec![text("Autumn wind...")], StopReason::EndTurn),
                title_step(),
            ],
            None,
            Some(skills.clone()),
        )
        .await;
        let mut settings = f.store.app_settings().await.expect("settings");
        settings.tools.ask_before_web = true; // doesn't apply to skills
        f.store.save_app_settings(settings).await.expect("saves");
        f.service
            .reply(&f.chat.id, &f.first.id, GenerateOptions::default())
            .await
            .expect("starts");
        f.sink.wait_finished().await;

        let reqs = f.fake.requests();
        assert!(reqs[0].tools.iter().any(|t| t.name == "load_skill"));
        let system = reqs[0].system.as_deref().expect("system");
        assert!(
            system.contains("- haiku: Use when the user asks for a haiku"),
            "{system}"
        );
        assert!(
            !f.sink
                .events()
                .iter()
                .any(|e| matches!(e, StreamEvent::ToolApproval { .. })),
            "skills never ask"
        );
        // The second step sees the instructions as the tool's result.
        let second = format!("{:?}", reqs[1].messages);
        assert!(second.contains("season word"), "{second}");
        let used = f.store.tool_usage(&f.chat.id).await.expect("log");
        assert_eq!(used[0].kind, ToolUsageKind::Skill);
        assert_eq!(used[0].label.as_deref(), Some("haiku"));

        // Switched off: no tool and no list.
        skills.set_enabled("haiku", false).expect("off");
        let set = f
            .service
            .tools()
            .tool_set(
                &oxide_core::ChatTools::default(),
                &oxide_core::AppSettings::default(),
                ProviderKind::LmStudio,
            )
            .await;
        assert!(set.find("load_skill").is_none());
        assert!(set.skills.is_empty());
    }

    #[tokio::test]
    async fn large_project_knowledge_is_searched_without_asking() {
        let mut first = vec![text("Let me check the files.")];
        first.extend(call("k1", "project_search", "{\"query\":\"launch code\"}"));
        let f = fixture(vec![
            turn(first, StopReason::ToolUse),
            turn(
                vec![text("It's tangerine (codes.md).")],
                StopReason::EndTurn,
            ),
            title_step(),
        ])
        .await;
        let mut settings = f.store.app_settings().await.expect("settings");
        settings.projects.knowledge_budget = 1_000;
        settings.tools.ask_before_web = true; // doesn't apply to project search
        f.store.save_app_settings(settings).await.expect("saves");
        let mut big = String::new();
        for i in 0..300 {
            big.push_str(&format!("Section {i} describes routine matters.\n\n"));
        }
        big.push_str("The launch code is tangerine.\n\n");
        in_project(
            &f,
            vec![knowledge("codes.md", &big), knowledge("b.md", "beta")],
        )
        .await;

        f.service
            .reply(&f.chat.id, &f.first.id, GenerateOptions::default())
            .await
            .expect("starts");
        f.sink.wait_finished().await;
        let reqs = f.fake.requests();
        assert_eq!(reqs[0].system_context, None);
        assert_eq!(reqs[0].tools[0].name, "project_search");
        let system = reqs[0].system.as_deref().expect("system");
        assert!(
            system.contains("too large to include here: codes.md, b.md."),
            "{system}"
        );
        assert!(
            !f.sink
                .events()
                .iter()
                .any(|e| matches!(e, StreamEvent::ToolApproval { .. })),
            "searching the project never asks"
        );
        let reply = saved(&f);
        let result = reply
            .content
            .iter()
            .find_map(|b| match b {
                ContentBlock::ToolResult {
                    content, is_error, ..
                } => Some((plain_text(content), *is_error)),
                _ => None,
            })
            .expect("a result");
        assert!(!result.1);
        assert!(result.0.contains("file=\"codes.md\""), "{}", result.0);
        assert!(result.0.contains("tangerine"));
        assert_eq!(
            reply.content.last(),
            Some(&ContentBlock::text("It's tangerine (codes.md)."))
        );
        let used = f.store.tool_usage(&f.chat.id).await.expect("log");
        assert_eq!(used[0].kind, ToolUsageKind::ProjectSearch);
        assert_eq!(used[0].label.as_deref(), Some("launch code"));
    }

    #[tokio::test]
    async fn memory_is_learned_after_replies_and_given_to_later_ones() {
        let f = fixture(vec![
            turn(
                vec![text("Rust is a language, Mikkel.")],
                StopReason::EndTurn,
            ),
            title_step(),
            vec![
                text("{\"memories\": [{\"text\": \"Is called Mikkel\", \"scope\": \"global\"},"),
                text(" {\"text\": \"Ships Launch on Windows\", \"scope\": \"project\"}]}"),
                ev(ChatEvent::Stop(StopReason::EndTurn)),
            ],
            turn(vec![text("Hi again.")], StopReason::EndTurn),
            vec![
                text("{\"memories\": []}"),
                ev(ChatEvent::Stop(StopReason::EndTurn)),
            ],
        ])
        .await;
        let project = in_project(&f, Vec::new()).await;
        let mut settings = f.store.app_settings().await.expect("settings");
        settings.memory.enabled = true;
        f.store.save_app_settings(settings).await.expect("saves");

        f.service
            .reply(&f.chat.id, &f.first.id, GenerateOptions::default())
            .await
            .expect("starts");
        f.sink
            .wait_for(|r| r.memory_changes.load(Ordering::SeqCst) == 1)
            .await;
        let items = f.store.list_memory().await.expect("memory");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].text, "Is called Mikkel");
        assert_eq!(items[0].project_id, None);
        assert_eq!(
            items[0].conversation_id.as_deref(),
            Some(f.chat.id.as_str())
        );
        assert_eq!(items[1].project_id.as_deref(), Some(project.as_str()));
        let reqs = f.fake.requests();
        assert!(
            reqs[0]
                .system
                .as_deref()
                .is_some_and(|s| !s.contains("<memory>"))
        );
        let memory_req = &reqs[2];
        assert_eq!(memory_req.system.as_deref(), Some(MEMORY_SYSTEM));
        assert!(
            plain_text(&memory_req.messages[0].content).contains("Rust is a language, Mikkel.")
        );

        // The next reply sees both items.
        let reply_id = saved(&f).id;
        f.service
            .send(
                &f.chat.id,
                Some(&reply_id),
                vec![ContentBlock::text("Hello")],
                GenerateOptions::default(),
            )
            .await
            .expect("sends");
        f.sink
            .wait_for(|r| {
                r.events()
                    .iter()
                    .filter(|e| matches!(e, StreamEvent::Done { .. }))
                    .count()
                    == 2
            })
            .await;
        let reqs = f.fake.requests();
        let system = reqs[3].system.as_deref().expect("system");
        assert!(
            system.contains("<memory>\n- Is called Mikkel\n- Ships Launch on Windows\n</memory>"),
            "{system}"
        );
    }

    #[tokio::test]
    async fn memory_off_asks_nothing_and_shares_nothing() {
        let f = fixture(vec![
            turn(vec![text("Rust is a language.")], StopReason::EndTurn),
            title_step(),
        ])
        .await;
        f.store
            .add_memory(
                oxide_core::NewMemory {
                    project_id: None,
                    text: "Secret preference".into(),
                },
                None,
            )
            .await
            .expect("adds");
        f.service
            .reply(&f.chat.id, &f.first.id, GenerateOptions::default())
            .await
            .expect("starts");
        f.sink
            .wait_for(|r| !r.changed.lock().expect("lock").is_empty())
            .await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        let reqs = f.fake.requests();
        assert_eq!(reqs.len(), 2, "reply and title only");
        assert!(
            reqs[0]
                .system
                .as_deref()
                .is_some_and(|s| !s.contains("Secret preference"))
        );
        assert_eq!(f.sink.memory_changes.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn paths_follow_parents() {
        let m = |id: &str, parent: Option<&str>| Message {
            id: id.into(),
            conversation_id: "c".into(),
            parent_id: parent.map(Into::into),
            role: Role::User,
            content: vec![],
            model: None,
            created_at: 0,
            stop_reason: None,
            usage: None,
            feedback: None,
        };
        let all = vec![
            m("a", None),
            m("b", Some("a")),
            m("c", Some("a")),
            m("d", Some("c")),
        ];
        let ids: Vec<String> = path_to(&all, "d")
            .expect("path")
            .into_iter()
            .map(|m| m.id)
            .collect();
        assert_eq!(ids, vec!["a", "c", "d"]);
        assert!(path_to(&all, "x").is_none());
    }
}
