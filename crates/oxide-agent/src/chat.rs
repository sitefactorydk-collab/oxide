//! Writing replies: one streamed generation per chat at a time.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use oxide_core::conversation::title_from_text;
use oxide_core::message::plain_text;
use oxide_core::model::split_model_id;
use oxide_core::{
    ChatStreamEvent, ContentBlock, Conversation, GenerateOptions, Message, ModelCapabilities,
    ProviderKind, Role, StopReason, StreamEvent, ThinkingStyle, Usage,
};
use oxide_llm::capabilities::anthropic_curated;
use oxide_llm::{ChatEvent, ChatMessage, ChatRequest, Thinking, complete_text};
use oxide_store::{NewReply, Store, new_id, now_ms};
use tokio::sync::watch;

use crate::AgentError;
use crate::context::{build_messages, plan_request, system_prompt};
use crate::registry::ModelRegistry;
use crate::title::{clean_title, pick_title_model, title_prompt};

/// How often buffered text is sent to the UI while a reply streams.
const FLUSH_EVERY: Duration = Duration::from_millis(40);
/// Longest wait for an automatic title.
const TITLE_TIMEOUT: Duration = Duration::from_secs(30);

/// Receives reply events. The app crate forwards them to the window.
pub trait EventSink: Send + Sync + 'static {
    fn stream_event(&self, event: ChatStreamEvent);
    /// A chat changed in the background (its automatic title).
    fn conversation_changed(&self, conversation: Conversation);
}

struct Running {
    cancel: watch::Sender<bool>,
}

pub struct ChatService {
    store: Store,
    registry: Arc<ModelRegistry>,
    sink: Arc<dyn EventSink>,
    running: Mutex<HashMap<String, Running>>,
}

/// How a generation ended.
enum Outcome {
    Finished(StopReason),
    Stopped,
    Failed(String),
}

impl ChatService {
    pub fn new(store: Store, registry: Arc<ModelRegistry>, sink: Arc<dyn EventSink>) -> Arc<Self> {
        Arc::new(Self {
            store,
            registry,
            sink,
            running: Mutex::new(HashMap::new()),
        })
    }

    fn running(&self) -> std::sync::MutexGuard<'_, HashMap<String, Running>> {
        self.running.lock().unwrap_or_else(|e| e.into_inner())
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
            .stream_reply(
                &conversation,
                &thread,
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
                    usage: reply.usage,
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

    /// Runs the request and accumulates the reply. Returns how it ended.
    async fn stream_reply(
        &self,
        conversation: &Conversation,
        thread: &[Message],
        options: GenerateOptions,
        reply: &mut ReplyBuilder,
        provider_label: &mut String,
        cancelled: &mut watch::Receiver<bool>,
    ) -> Result<Outcome, AgentError> {
        let cid = conversation.id.as_str();
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
                "this chat's endpoint no longer exists. Pick another model in the composer.".into(),
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

        let mut request = ChatRequest::new(model, build_messages(thread));
        request.system = Some(system_prompt(&settings.profile.name, now_ms()));
        plan_request(
            &mut request,
            config.kind,
            &caps,
            info.as_ref().and_then(|i| i.max_output_tokens),
            options.thinking,
            settings.models.thinking_budget,
        );

        let mut stream = tokio::select! {
            s = provider.stream(request) => s?,
            _ = cancelled.wait_for(|c| *c) => return Ok(Outcome::Stopped),
        };

        let mut ticker = tokio::time::interval(FLUSH_EVERY);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let outcome = loop {
            tokio::select! {
                _ = cancelled.wait_for(|c| *c) => break Outcome::Stopped,
                _ = ticker.tick() => self.flush(cid, reply),
                next = stream.next() => match next {
                    None => break Outcome::Failed("the reply ended unexpectedly".into()),
                    Some(Err(e)) => break Outcome::Failed(e.to_string()),
                    Some(Ok(ChatEvent::Stop(reason))) => break Outcome::Finished(reason),
                    Some(Ok(event)) => {
                        if let Some(ev) = reply.apply(event) {
                            self.flush(cid, reply);
                            self.emit(cid, ev);
                        }
                    }
                },
            }
        };
        self.flush(cid, reply);
        Ok(outcome)
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
        if config.kind == ProviderKind::Anthropic && caps.thinking == ThinkingStyle::Adaptive {
            request.thinking = Thinking::Off;
        }
        let text = complete_text(provider.as_ref(), request).await?;
        Ok(clean_title(&text))
    }
}

/// The path from the root to `id`, or `None` if `id` isn't among `messages`.
fn path_to(messages: &[Message], id: &str) -> Option<Vec<Message>> {
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

/// Accumulates a streamed reply into content blocks.
#[derive(Default)]
struct ReplyBuilder {
    thinking: String,
    signature: Option<String>,
    thinking_started: Option<Instant>,
    thinking_ms: Option<u32>,
    text: String,
    tools: Vec<(String, String, String)>,
    usage: Option<Usage>,
    pending_text: String,
    pending_thinking: String,
}

impl ReplyBuilder {
    /// Applies an event. Text is buffered (see `flush`); other events that the UI shows are
    /// returned to be sent straight away.
    fn apply(&mut self, event: ChatEvent) -> Option<StreamEvent> {
        match event {
            ChatEvent::ThinkingDelta(t) => {
                self.thinking_started.get_or_insert_with(Instant::now);
                self.thinking.push_str(&t);
                self.pending_thinking.push_str(&t);
                None
            }
            ChatEvent::ThinkingSignature(s) => {
                self.signature = Some(s);
                None
            }
            ChatEvent::TextDelta(t) => {
                self.end_thinking();
                self.text.push_str(&t);
                self.pending_text.push_str(&t);
                None
            }
            ChatEvent::ToolUseStart { id, name } => {
                self.end_thinking();
                self.tools.push((id.clone(), name.clone(), String::new()));
                Some(StreamEvent::ToolUse { id, name })
            }
            ChatEvent::ToolUseDelta { id, partial_json } => {
                if let Some(t) = self.tools.iter_mut().find(|t| t.0 == id) {
                    t.2.push_str(&partial_json);
                }
                None
            }
            ChatEvent::ToolUseEnd { .. } => None,
            ChatEvent::Usage(u) => {
                let mut total = self.usage.unwrap_or_default();
                total.merge(u);
                self.usage = Some(total);
                Some(StreamEvent::Usage { usage: total })
            }
            ChatEvent::Stop(_) => None,
        }
    }

    fn end_thinking(&mut self) {
        if self.thinking_ms.is_none()
            && let Some(start) = self.thinking_started
        {
            self.thinking_ms = Some(u32::try_from(start.elapsed().as_millis()).unwrap_or(u32::MAX));
        }
    }

    fn take_content(&mut self) -> Vec<ContentBlock> {
        self.end_thinking();
        let mut out = Vec::new();
        if !self.thinking.trim().is_empty() || self.signature.is_some() {
            out.push(ContentBlock::Thinking {
                text: std::mem::take(&mut self.thinking),
                signature: self.signature.take(),
                duration_ms: self.thinking_ms,
            });
        }
        if !self.text.is_empty() {
            out.push(ContentBlock::text(std::mem::take(&mut self.text)));
        }
        for (id, name, json) in self.tools.drain(..) {
            out.push(ContentBlock::ToolUse {
                id,
                name,
                input_json: if json.trim().is_empty() {
                    "{}".into()
                } else {
                    json
                },
            });
        }
        out
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::registry::Connector;
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
        let store = Store::open_in_memory().await.expect("store");
        let secrets: Arc<dyn SecretStore> = Arc::new(MemorySecrets::default());
        let fake = FakeProvider::new(scripts);
        let registry = Arc::new(ModelRegistry::with_connector(
            store.clone(),
            secrets,
            fake_connector(fake.clone()),
        ));
        let sink = Arc::new(Recorder::default());
        let service = ChatService::new(store.clone(), registry, sink.clone());
        let (chat, first) = store
            .start_conversation(StartConversation {
                text: "What is Rust?".into(),
                model: "lmstudio/fake-model".into(),
                project_id: None,
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
            .reply(&f.chat.id, &f.first.id, GenerateOptions { thinking: true })
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
