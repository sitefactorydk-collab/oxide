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
use oxide_core::conversation::title_from_text;
use oxide_core::message::plain_text;
use oxide_core::model::split_model_id;
use oxide_core::tools::PROVIDER_SERVER;
use oxide_core::{
    AppSettings, ApprovalDecision, ChatStreamEvent, ContentBlock, Conversation, GenerateOptions,
    Message, ModelCapabilities, ProviderKind, Role, StopReason, StreamEvent, ThinkingStyle,
    ToolApprovalRequest, ToolUsageKind, Usage,
};
use oxide_llm::capabilities::anthropic_curated;
use oxide_llm::{ChatEvent, ChatMessage, ChatRequest, Provider, Thinking, complete_text};
use oxide_store::{NewReply, NewToolUsage, Store, new_id, now_ms};
use serde_json::Value;
use tokio::sync::{oneshot, watch};

use crate::AgentError;
use crate::context::{
    PromptContext, build_messages, plan_request, request_messages, system_prompt,
};
use crate::registry::ModelRegistry;
use crate::title::{clean_title, pick_title_model, title_prompt};
use crate::tools::{ToolEntry, ToolOutput, ToolSet, Tools, provider_result_summary, usage_label};

/// How often buffered text is sent to the UI while a reply streams.
const FLUSH_EVERY: Duration = Duration::from_millis(40);
/// Longest wait for an automatic title.
const TITLE_TIMEOUT: Duration = Duration::from_secs(30);

const STOPPED_NOTE: &str = "The user stopped the reply before this tool ran.";
const FAILED_NOTE: &str = "The reply ended before this tool ran.";
const DENIED_NOTE: &str = "The user declined to run this tool call. Don't try it again unless the \
                           user asks; continue without it or ask how they'd like to proceed.";

/// Receives reply events. The app crate forwards them to the window.
pub trait EventSink: Send + Sync + 'static {
    fn stream_event(&self, event: ChatStreamEvent);
    /// A chat changed in the background (its automatic title).
    fn conversation_changed(&self, conversation: Conversation);
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
    /// "Allow always for this chat" keys.
    grants: HashSet<String>,
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
        match map.remove(tool_use_id) {
            Some(p) => p.answer.send(decision).is_ok(),
            None => false,
        }
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
                if first_exchange && let (Some(first), Some(reply)) = (thread.first(), saved) {
                    self.auto_title(&cid, first, &reply).await;
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
        let tool_set = if caps.tools {
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
        let project = match &conversation.project_id {
            Some(p) => self.store.get_project(p).await.ok(),
            None => None,
        };
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
            project_instructions: project.as_ref().map(|p| p.instructions.as_str()),
            memory: None,
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
            grants,
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

    fn needs_approval(entry: &ToolEntry, setup: &Setup) -> bool {
        if setup.grants.contains(&entry.grant_key()) {
            return false;
        }
        match entry.server() {
            None => setup.settings.tools.ask_before_web,
            Some(_) => !(entry.read_only && setup.settings.tools.auto_allow_read_only),
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
    async fn run_calls(
        &self,
        cid: &str,
        reply_id: &str,
        setup: &mut Setup,
        calls: Vec<OpenCall>,
        reply: &mut ReplyBuilder,
        cancelled: &mut watch::Receiver<bool>,
    ) -> bool {
        let mut ready: Vec<(OpenCall, ToolEntry, Value)> = Vec::new();
        for call in calls {
            let stopped = *cancelled.borrow();
            if stopped {
                return false;
            }
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
            if Self::needs_approval(&entry, setup) {
                match self
                    .ask(cid, reply_id, &call, &entry, &input, cancelled)
                    .await
                {
                    None => return false,
                    Some(ApprovalDecision::Deny) => {
                        let out = ToolOutput::error(DENIED_NOTE);
                        self.finish_call(cid, reply_id, &call, Some(&entry), &input, out, reply)
                            .await;
                        continue;
                    }
                    Some(ApprovalDecision::AllowChat) => {
                        let key = entry.grant_key();
                        if let Err(e) = self.store.grant_chat_tool(cid, &key).await {
                            tracing::warn!("couldn't save the approval: {e}");
                        }
                        setup.grants.insert(key);
                    }
                    Some(ApprovalDecision::AllowOnce) => {}
                }
            }
            ready.push((call, entry, input));
        }

        // Tools that only read run side by side; anything else runs one at a time.
        let parallel = ready.len() > 1 && ready.iter().all(|(_, e, _)| e.read_only);
        if parallel {
            for (call, _, _) in &ready {
                self.emit(
                    cid,
                    StreamEvent::ToolRunning {
                        id: call.id.clone(),
                    },
                );
            }
            let settings = &setup.settings;
            let runs = join_all(
                ready
                    .iter()
                    .map(|(c, e, i)| self.tools.run(e, i, &c.id, settings)),
            );
            let outs = tokio::select! {
                outs = runs => outs,
                _ = cancelled.wait_for(|c| *c) => return false,
            };
            for ((call, entry, input), out) in ready.iter().zip(outs) {
                self.finish_call(cid, reply_id, call, Some(entry), input, out, reply)
                    .await;
            }
        } else {
            for (call, entry, input) in &ready {
                self.emit(
                    cid,
                    StreamEvent::ToolRunning {
                        id: call.id.clone(),
                    },
                );
                let out = tokio::select! {
                    out = self.tools.run(entry, input, &call.id, &setup.settings) => out,
                    _ = cancelled.wait_for(|c| *c) => return false,
                };
                self.finish_call(cid, reply_id, call, Some(entry), input, out, reply)
                    .await;
            }
        }
        true
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
                usage_label(entry.tool(), input),
                out.is_error,
            )
            .await;
        }
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
        let mut request = ChatRequest::new(
            model,
            vec![ChatMessage::user(title_prompt(user_text, reply_text))],
        );
        request.system = Some("You name chats. You answer with a short title only.".into());
        request.max_tokens = Some(match config.kind {
            ProviderKind::Anthropic => 60,
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
        let text = complete_text(provider.as_ref(), request).await?;
        Ok(clean_title(&text))
    }
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
        let service = ChatService::new(store.clone(), registry, sink.clone(), tools);
        let (chat, first) = store
            .start_conversation(StartConversation {
                text: "What is Rust?".into(),
                model: "lmstudio/fake-model".into(),
                project_id: None,
                attachments: Vec::new(),
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
            vec!["web_fetch", "fs__list", "fs__read", "fs__write"]
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
