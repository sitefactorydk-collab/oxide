//! Scheduled tasks (Phase 15): saved prompts that run on their own, each run in a fresh chat.
//!
//! The [`Scheduler`] keeps one loop: it sleeps until the earliest task is due (at most a
//! minute at a time, so a computer waking from sleep is noticed), starts the due runs through
//! [`ChatService`] and follows each run's chat until its reply ends, writing the outcome into
//! the task's history. A run is a normal chat: the user can open it, answer its approval
//! cards, stop it or carry on talking afterwards.
//!
//! - Runs late by more than [`MISSED_AFTER_MS`] (Oxide was closed, the computer slept) are
//!   missed: the task's [`MissedRunPolicy`] says whether one makes up for them now or the
//!   miss is only noted. Runs that were going when Oxide closed are marked failed at start.
//! - [`ApprovalMode::Auto`] runs tool calls without asking; with [`ApprovalMode::Ask`] a run
//!   waits on its approval cards and the sink hears it needs the user (to notify).
//! - Local time comes from a [`LocalZone`] the app supplies (daylight saving time included).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use oxide_core::{
    ChatStreamEvent, ContentBlock, Conversation, GenerateOptions, LocalZone, MAX_RUN_OUTPUT_CHARS,
    MAX_RUNS_KEPT, MAX_TASKS, MISSED_AFTER_MS, Message, MissedRunPolicy, RunStatus, RunTrigger,
    ScheduleChange, ScheduledTask, StartConversation, StopReason, StreamEvent, TaskInput, TaskRun,
    TaskSchedule, Timestamp,
};
use oxide_store::{Store, StoreError, new_id, now_ms};
use tokio::sync::{Notify, broadcast};

use crate::{AgentError, ChatService};

/// The longest the loop sleeps before looking at the clock again.
const MAX_SLEEP: Duration = Duration::from_secs(60);
/// How often a run's watcher checks that its reply is still going (in case it missed the end).
const CHECK_EVERY: Duration = Duration::from_secs(15);

const CLOSED_DURING_RUN: &str = "Oxide was closed while this run was going.";
const STILL_RUNNING: &str = "The previous run was still going.";

/// Hears about tasks and runs. The app crate forwards changes to the window and decides
/// which runs deserve a notification.
pub trait SchedulerSink: Send + Sync + 'static {
    /// A task or one of its runs changed.
    fn changed(&self, change: ScheduleChange);
    /// A run started a new chat.
    fn chat_started(&self, conversation: Conversation) {
        let _ = conversation;
    }
    /// A run needs the user (waiting for approval), ended (done, failed, stopped) or wasn't
    /// run (missed, skipped).
    fn run_event(&self, task: &ScheduledTask, run: &TaskRun) {
        let _ = (task, run);
    }
}

/// A run going now.
#[derive(Debug, Clone)]
struct ActiveRun {
    task_id: String,
}

pub struct Scheduler {
    store: Store,
    chat: Arc<ChatService>,
    sink: Arc<dyn SchedulerSink>,
    zone: Arc<dyn LocalZone>,
    wake: Notify,
    /// Runs going now, by chat id.
    active: Mutex<HashMap<String, ActiveRun>>,
    /// Firing and editing one at a time, so a task never runs twice for one time.
    gate: tokio::sync::Mutex<()>,
    started: AtomicBool,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Scheduler {
    pub fn new(
        store: Store,
        chat: Arc<ChatService>,
        sink: Arc<dyn SchedulerSink>,
        zone: Arc<dyn LocalZone>,
    ) -> Arc<Self> {
        Arc::new(Self {
            store,
            chat,
            sink,
            zone,
            wake: Notify::new(),
            active: Mutex::new(HashMap::new()),
            gate: tokio::sync::Mutex::new(()),
            started: AtomicBool::new(false),
        })
    }

    /// Starts the loop (once): marks runs Oxide was closed during as failed, makes up for
    /// missed runs as each task says, then runs tasks as they come due.
    pub fn start(self: &Arc<Self>) {
        if self.started.swap(true, Ordering::SeqCst) {
            return;
        }
        let this = Arc::clone(self);
        tokio::spawn(async move {
            this.recover().await;
            loop {
                let now = now_ms();
                this.tick(now).await;
                let wait = match this.store.earliest_next_run().await {
                    Ok(Some(next)) => Duration::from_millis(
                        u64::try_from((next - now_ms()).max(200)).unwrap_or(0),
                    )
                    .min(MAX_SLEEP),
                    Ok(None) => MAX_SLEEP,
                    Err(e) => {
                        tracing::warn!("scheduler couldn't read the next run: {e}");
                        MAX_SLEEP
                    }
                };
                tokio::select! {
                    () = tokio::time::sleep(wait) => {}
                    () = this.wake.notified() => {}
                }
            }
        });
    }

    /// Whether a scheduled run is going in this chat.
    pub fn is_run_chat(&self, conversation_id: &str) -> bool {
        lock(&self.active).contains_key(conversation_id)
    }

    /// Runs Oxide was closed during (still marked running) become failed.
    pub async fn recover(&self) {
        let runs = match self.store.active_task_runs().await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("couldn't read unfinished runs: {e}");
                return;
            }
        };
        for mut run in runs {
            if run
                .conversation_id
                .as_deref()
                .is_some_and(|c| self.is_run_chat(c))
            {
                continue;
            }
            run.status = RunStatus::Failed;
            run.finished_at = Some(now_ms());
            run.error = Some(CLOSED_DURING_RUN.into());
            match self.store.update_task_run(&run).await {
                Ok(run) => self.changed(&run.task_id.clone(), Some(run)),
                Err(e) => tracing::warn!("couldn't close an unfinished run: {e}"),
            }
        }
    }

    /// Runs (or notes as missed) every task due at `now`, and moves each to its next time.
    pub async fn tick(self: &Arc<Self>, now: Timestamp) {
        let _gate = self.gate.lock().await;
        let due = match self.store.due_tasks(now).await {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!("scheduler couldn't read due tasks: {e}");
                return;
            }
        };
        for task in due {
            let Some(due_at) = task.next_run_at else {
                continue;
            };
            // Move on first, so a failure below never runs the task again for this time.
            let next = task.schedule.next_after(now, self.zone.as_ref());
            let enabled = !task.schedule.is_once();
            if let Err(e) = self
                .store
                .set_task_schedule_state(&task.id, enabled, next)
                .await
            {
                tracing::warn!(task = %task.id, "couldn't move a task to its next run: {e}");
                continue;
            }
            let late = now - due_at > MISSED_AFTER_MS;
            if late && task.missed == MissedRunPolicy::Skip {
                tracing::info!(task = %task.id, "run missed; the task skips missed runs");
                let run = TaskRun {
                    id: new_id(),
                    task_id: task.id.clone(),
                    conversation_id: None,
                    trigger: RunTrigger::Schedule,
                    scheduled_for: Some(due_at),
                    started_at: now,
                    finished_at: Some(now),
                    status: RunStatus::Missed,
                    output: None,
                    error: None,
                };
                self.record(&task, run).await;
                continue;
            }
            let trigger = if late {
                RunTrigger::CatchUp
            } else {
                RunTrigger::Schedule
            };
            tracing::info!(task = %task.id, ?trigger, "scheduled task due");
            self.fire(&task, trigger, Some(due_at)).await;
        }
    }

    /// All tasks, by name.
    pub async fn list(&self) -> Result<Vec<ScheduledTask>, AgentError> {
        Ok(self.store.list_tasks().await?)
    }

    pub async fn get(&self, id: &str) -> Result<ScheduledTask, AgentError> {
        Ok(self.store.get_task(id).await?)
    }

    /// A task's runs, newest first.
    pub async fn runs(&self, task_id: &str, limit: usize) -> Result<Vec<TaskRun>, AgentError> {
        Ok(self
            .store
            .task_runs(task_id, limit.clamp(1, MAX_RUNS_KEPT))
            .await?)
    }

    /// The next few times a schedule runs (checks it too).
    pub fn preview(&self, schedule: &TaskSchedule, n: usize) -> Result<Vec<Timestamp>, AgentError> {
        schedule.validate().map_err(AgentError::Invalid)?;
        Ok(schedule.upcoming(now_ms(), n.clamp(1, 10), self.zone.as_ref()))
    }

    /// When a task with this input runs next. A one-time task in the past is refused.
    fn next_for(&self, input: &TaskInput) -> Result<Option<Timestamp>, AgentError> {
        if !input.enabled {
            return Ok(None);
        }
        match input.schedule.next_after(now_ms(), self.zone.as_ref()) {
            Some(t) => Ok(Some(t)),
            None if input.schedule.is_once() => Err(AgentError::Invalid(
                "That time has passed. Pick a time in the future.".into(),
            )),
            None => Err(AgentError::Invalid("This schedule never runs.".into())),
        }
    }

    async fn check_project(&self, input: &TaskInput) -> Result<(), AgentError> {
        if let Some(p) = &input.project_id {
            self.store.get_project(p).await.map_err(|e| match e {
                StoreError::NotFound { .. } => {
                    AgentError::Invalid("That project no longer exists.".into())
                }
                other => other.into(),
            })?;
        }
        Ok(())
    }

    pub async fn create(&self, input: TaskInput) -> Result<ScheduledTask, AgentError> {
        let input = input.clean().map_err(AgentError::Invalid)?;
        self.check_project(&input).await?;
        let _gate = self.gate.lock().await;
        if self.store.count_tasks().await? >= MAX_TASKS {
            return Err(AgentError::Invalid(format!(
                "Oxide keeps at most {MAX_TASKS} scheduled tasks. Delete one first."
            )));
        }
        let next = self.next_for(&input)?;
        let task = self.store.create_task(&input, next).await?;
        tracing::info!(task = %task.id, "scheduled task created");
        self.changed(&task.id, None);
        self.wake.notify_one();
        Ok(task)
    }

    pub async fn update(&self, id: &str, input: TaskInput) -> Result<ScheduledTask, AgentError> {
        let input = input.clean().map_err(AgentError::Invalid)?;
        self.check_project(&input).await?;
        let _gate = self.gate.lock().await;
        let next = self.next_for(&input)?;
        let task = self.store.update_task(id, &input, next).await?;
        self.changed(id, None);
        self.wake.notify_one();
        Ok(task)
    }

    /// Pauses or resumes a task. Resuming counts from now: runs missed while paused aren't
    /// made up.
    pub async fn set_enabled(&self, id: &str, enabled: bool) -> Result<ScheduledTask, AgentError> {
        let _gate = self.gate.lock().await;
        let task = self.store.get_task(id).await?;
        let next = self.next_for(&TaskInput {
            enabled,
            ..task.input()
        })?;
        self.store
            .set_task_schedule_state(id, enabled, next)
            .await?;
        self.changed(id, None);
        self.wake.notify_one();
        Ok(self.store.get_task(id).await?)
    }

    /// Deletes a task and its history. A run going now is stopped; its chat stays.
    pub async fn delete(&self, id: &str) -> Result<(), AgentError> {
        let _gate = self.gate.lock().await;
        let chats: Vec<String> = lock(&self.active)
            .iter()
            .filter(|(_, a)| a.task_id == id)
            .map(|(c, _)| c.clone())
            .collect();
        for c in chats {
            self.chat.stop(&c);
        }
        self.store.delete_task(id).await?;
        self.sink.changed(ScheduleChange {
            task_id: id.to_owned(),
            run: None,
            deleted: true,
        });
        self.wake.notify_one();
        Ok(())
    }

    /// Runs a task now (also a paused one); its schedule doesn't change.
    pub async fn run_now(self: &Arc<Self>, id: &str) -> Result<TaskRun, AgentError> {
        let task = self.store.get_task(id).await?;
        let _gate = self.gate.lock().await;
        Ok(self.fire(&task, RunTrigger::Manual, None).await)
    }

    fn changed(&self, task_id: &str, run: Option<TaskRun>) {
        self.sink.changed(ScheduleChange {
            task_id: task_id.to_owned(),
            run,
            deleted: false,
        });
    }

    /// Saves a run that's already over (missed, skipped, couldn't start) and tells the sink.
    async fn record(&self, task: &ScheduledTask, run: TaskRun) -> TaskRun {
        let run = match self.store.add_task_run(&run).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("couldn't save a run: {e}");
                return run;
            }
        };
        if let Err(e) = self.store.prune_task_runs(&task.id, MAX_RUNS_KEPT).await {
            tracing::warn!("couldn't drop old runs: {e}");
        }
        self.changed(&task.id, Some(run.clone()));
        self.sink.run_event(task, &run);
        run
    }

    /// Starts a run: a new chat with the task's prompt, model, tools and folders.
    async fn fire(
        self: &Arc<Self>,
        task: &ScheduledTask,
        trigger: RunTrigger,
        scheduled_for: Option<Timestamp>,
    ) -> TaskRun {
        let started_at = now_ms();
        let mut run = TaskRun {
            id: new_id(),
            task_id: task.id.clone(),
            conversation_id: None,
            trigger,
            scheduled_for,
            started_at,
            finished_at: None,
            status: RunStatus::Running,
            output: None,
            error: None,
        };
        let busy = lock(&self.active).values().any(|a| a.task_id == task.id);
        if busy {
            tracing::info!(task = %task.id, "skipped: the previous run is still going");
            run.status = RunStatus::Skipped;
            run.finished_at = Some(started_at);
            run.error = Some(STILL_RUNNING.into());
            return self.record(task, run).await;
        }
        let started = self
            .store
            .start_conversation(StartConversation {
                text: task.prompt.clone(),
                model: task.model.clone(),
                project_id: task.project_id.clone(),
                attachments: Vec::new(),
                folders: task.folders.clone(),
                tools: Some(task.tools.clone()),
            })
            .await;
        let (conversation, first) = match started {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(task = %task.id, "couldn't start a run's chat: {e}");
                run.status = RunStatus::Failed;
                run.finished_at = Some(now_ms());
                run.error = Some(format!("The run's chat couldn't be made: {e}"));
                return self.record(task, run).await;
            }
        };
        let cid = conversation.id.clone();
        let conversation = match self.store.rename_conversation(&cid, &task.name).await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("couldn't name a run's chat: {e}");
                conversation
            }
        };
        self.sink.chat_started(conversation);
        self.chat.set_unattended(&cid, &task.name, task.approval);
        lock(&self.active).insert(
            cid.clone(),
            ActiveRun {
                task_id: task.id.clone(),
            },
        );
        run.conversation_id = Some(cid.clone());
        let run = match self.store.add_task_run(&run).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("couldn't save a run: {e}");
                run
            }
        };
        if let Err(e) = self.store.prune_task_runs(&task.id, MAX_RUNS_KEPT).await {
            tracing::warn!("couldn't drop old runs: {e}");
        }
        self.changed(&task.id, Some(run.clone()));

        // Listen before the reply starts, so its end can't be missed.
        let signals = self.chat.subscribe();
        let options = GenerateOptions {
            thinking: task.thinking,
            ..GenerateOptions::default()
        };
        if let Err(e) = self.chat.reply(&cid, &first.id, options).await {
            tracing::warn!(task = %task.id, "couldn't start a run's reply: {e}");
            return self
                .finish(task, run, RunStatus::Failed, None, Some(e.to_string()))
                .await;
        }
        tracing::info!(task = %task.id, conversation = %cid, "scheduled run started");
        let this = Arc::clone(self);
        let task = task.clone();
        let out = run.clone();
        tokio::spawn(async move { this.follow(task, run, signals).await });
        out
    }

    /// Follows a run's chat until its reply ends.
    async fn follow(
        self: Arc<Self>,
        task: ScheduledTask,
        mut run: TaskRun,
        mut signals: broadcast::Receiver<ChatStreamEvent>,
    ) {
        let Some(cid) = run.conversation_id.clone() else {
            return;
        };
        loop {
            let signal = tokio::select! {
                s = signals.recv() => s,
                () = tokio::time::sleep(CHECK_EVERY) => {
                    if self.chat.is_running(&cid) {
                        continue;
                    }
                    // Ended without our hearing it: read the outcome from the chat.
                    let (status, message) = self.outcome_from_chat(&cid).await;
                    self.finish(&task, run, status, message, None).await;
                    return;
                }
            };
            let event = match signal {
                Ok(e) if e.conversation_id == cid => e.event,
                Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return,
            };
            match event {
                StreamEvent::ToolApproval { .. } if run.status != RunStatus::WaitingApproval => {
                    run.status = RunStatus::WaitingApproval;
                    run = self.save(run).await;
                    tracing::info!(task = %task.id, "scheduled run waits for approval");
                    self.sink.run_event(&task, &run);
                }
                StreamEvent::ToolRunning { .. } if run.status == RunStatus::WaitingApproval => {
                    let waiting = self
                        .chat
                        .pending_approvals()
                        .iter()
                        .any(|r| r.conversation_id == cid);
                    if !waiting {
                        run.status = RunStatus::Running;
                        run = self.save(run).await;
                    }
                }
                StreamEvent::Done { message } => {
                    let status = match &message {
                        Some(m) if m.stop_reason != Some(StopReason::Stopped) => RunStatus::Done,
                        _ => RunStatus::Stopped,
                    };
                    self.finish(&task, run, status, message, None).await;
                    return;
                }
                StreamEvent::Failed { error, message } => {
                    self.finish(&task, run, RunStatus::Failed, message, Some(error))
                        .await;
                    return;
                }
                _ => {}
            }
        }
    }

    /// How a run's reply ended, read from its chat (the latest reply).
    async fn outcome_from_chat(&self, cid: &str) -> (RunStatus, Option<Message>) {
        let reply = self.store.list_messages(cid).await.ok().and_then(|m| {
            m.into_iter()
                .filter(|m| m.role == oxide_core::Role::Assistant)
                .max_by_key(|m| m.created_at)
        });
        let status = match reply.as_ref().and_then(|m| m.stop_reason) {
            None | Some(StopReason::Stopped) => RunStatus::Stopped,
            Some(StopReason::Error) => RunStatus::Failed,
            Some(_) => RunStatus::Done,
        };
        (status, reply)
    }

    async fn save(&self, run: TaskRun) -> TaskRun {
        match self.store.update_task_run(&run).await {
            Ok(r) => {
                self.changed(&r.task_id.clone(), Some(r.clone()));
                r
            }
            Err(e) => {
                tracing::warn!("couldn't save a run: {e}");
                run
            }
        }
    }

    /// Ends a run: its chat becomes an ordinary chat again and the history gets the outcome.
    async fn finish(
        &self,
        task: &ScheduledTask,
        mut run: TaskRun,
        status: RunStatus,
        message: Option<Message>,
        error: Option<String>,
    ) -> TaskRun {
        if let Some(cid) = &run.conversation_id {
            self.chat.clear_unattended(cid);
            lock(&self.active).remove(cid);
            // The user deleted the chat while it ran (which stops the reply): the run left
            // the history with it.
            if matches!(
                self.store.get_task_run(&run.id).await,
                Err(StoreError::NotFound { .. })
            ) {
                tracing::info!(task = %task.id, "a scheduled run's chat was deleted while it ran");
                self.changed(&task.id, None);
                return run;
            }
        }
        run.status = status;
        run.finished_at = Some(now_ms());
        run.output = message.as_ref().and_then(|m| output_of(&m.content));
        run.error = error;
        tracing::info!(task = %task.id, status = status.as_str(), "scheduled run ended");
        let run = self.save(run).await;
        self.sink.run_event(task, &run);
        run
    }
}

/// A reply's text for the history: its last words matter most (the report), so a long text
/// keeps its end.
fn output_of(content: &[ContentBlock]) -> Option<String> {
    let text = content
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Text { text } => Some(text.trim()),
            _ => None,
        })
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    if text.is_empty() {
        return None;
    }
    let n = text.chars().count();
    if n <= MAX_RUN_OUTPUT_CHARS {
        return Some(text);
    }
    let tail: String = text.chars().skip(n - (MAX_RUN_OUTPUT_CHARS - 1)).collect();
    Some(format!("…{}", tail.trim_start()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::tests::{FakeProvider, Recorder, Step, fake_connector};
    use crate::tools::tests::{FakeMcp, tool};
    use crate::{ModelRegistry, Tools};
    use oxide_core::{ApprovalMode, ChatTools, FixedZone, Usage};
    use oxide_llm::ChatEvent;
    use oxide_store::{MemorySecrets, SecretStore};

    #[derive(Default)]
    struct Heard {
        changes: Mutex<Vec<ScheduleChange>>,
        events: Mutex<Vec<(String, RunStatus)>>,
        chats: Mutex<Vec<Conversation>>,
        notify: Notify,
    }

    impl SchedulerSink for Heard {
        fn changed(&self, change: ScheduleChange) {
            lock(&self.changes).push(change);
            self.notify.notify_waiters();
        }
        fn chat_started(&self, conversation: Conversation) {
            lock(&self.chats).push(conversation);
        }
        fn run_event(&self, task: &ScheduledTask, run: &TaskRun) {
            lock(&self.events).push((task.name.clone(), run.status));
            self.notify.notify_waiters();
        }
    }

    impl Heard {
        fn statuses(&self) -> Vec<RunStatus> {
            lock(&self.events).iter().map(|(_, s)| *s).collect()
        }

        async fn wait_for(&self, pred: impl Fn(&Heard) -> bool) {
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let n = self.notify.notified();
                    if pred(self) {
                        return;
                    }
                    n.await;
                }
            })
            .await
            .expect("arrives in time");
        }
    }

    struct Fx {
        store: Store,
        chat: Arc<ChatService>,
        fake: FakeProvider,
        heard: Arc<Heard>,
        sched: Arc<Scheduler>,
        mcp: Arc<FakeMcp>,
    }

    fn ev(e: ChatEvent) -> Step {
        Step::Event(e)
    }

    fn answer(t: &str) -> Vec<Step> {
        vec![
            ev(ChatEvent::TextDelta(t.into())),
            ev(ChatEvent::Usage(Usage::default())),
            ev(ChatEvent::Stop(StopReason::EndTurn)),
        ]
    }

    fn write_call() -> Vec<Step> {
        vec![
            ev(ChatEvent::TextDelta("Writing the note.".into())),
            ev(ChatEvent::ToolUseStart {
                id: "w1".into(),
                name: "fs__write".into(),
            }),
            ev(ChatEvent::ToolUseDelta {
                id: "w1".into(),
                partial_json: "{\"path\":\"note.txt\"}".into(),
            }),
            ev(ChatEvent::ToolUseEnd { id: "w1".into() }),
            ev(ChatEvent::Stop(StopReason::ToolUse)),
        ]
    }

    async fn fx(scripts: Vec<Vec<Step>>) -> Fx {
        let fake = FakeProvider::new(scripts);
        let store = Store::open_in_memory().await.expect("store");
        let secrets: Arc<dyn SecretStore> = Arc::new(MemorySecrets::default());
        let registry = Arc::new(ModelRegistry::with_connector(
            store.clone(),
            secrets.clone(),
            fake_connector(fake.clone()),
        ));
        let mcp = Arc::new(FakeMcp {
            tools: vec![tool("fs", "write", false)],
            ..FakeMcp::default()
        });
        let tools = Tools::new(
            Some(mcp.clone() as Arc<dyn crate::tools::McpTools>),
            secrets,
            None,
        );
        let chat = ChatService::new(
            store.clone(),
            registry,
            Arc::new(Recorder::default()),
            tools,
        );
        let heard = Arc::new(Heard::default());
        let sched = Scheduler::new(
            store.clone(),
            chat.clone(),
            heard.clone(),
            Arc::new(FixedZone(3600)),
        );
        Fx {
            store,
            chat,
            fake,
            heard,
            sched,
            mcp,
        }
    }

    fn daily(name: &str) -> TaskInput {
        TaskInput {
            name: name.into(),
            prompt: "Write today's note".into(),
            schedule: TaskSchedule::Cron {
                expr: "0 9 * * *".into(),
            },
            model: "lmstudio/fake-model".into(),
            project_id: None,
            folders: vec![],
            tools: ChatTools::default(),
            thinking: false,
            approval: ApprovalMode::Ask,
            missed: MissedRunPolicy::RunOnce,
            enabled: true,
        }
    }

    /// Makes a task due at `at` (as if it had been scheduled for then).
    async fn due_at(f: &Fx, id: &str, at: Timestamp) {
        f.store
            .set_task_schedule_state(id, true, Some(at))
            .await
            .expect("due");
    }

    async fn ended(f: &Fx, n: usize) {
        f.heard
            .wait_for(|h| h.statuses().iter().filter(|s| !s.is_active()).count() >= n)
            .await;
    }

    #[tokio::test]
    async fn a_daily_task_runs_on_time_in_a_new_chat_and_keeps_its_output() {
        let f = fx(vec![answer("Today: nothing new. All good.")]).await;
        let task = f
            .sched
            .create(daily("Morning note"))
            .await
            .expect("creates");
        let next = task.next_run_at.expect("has a next run");
        assert!(next > now_ms());
        // 09:00 local in a UTC+1 zone.
        assert_eq!((next / 1000 + 3600).rem_euclid(86_400), 9 * 3600);

        // Not due yet: nothing happens.
        f.sched.tick(next - 1).await;
        assert!(
            f.store
                .task_runs(&task.id, 10)
                .await
                .expect("runs")
                .is_empty()
        );

        f.sched.tick(next).await;
        ended(&f, 1).await;
        let runs = f.store.task_runs(&task.id, 10).await.expect("runs");
        assert_eq!(runs.len(), 1);
        let run = &runs[0];
        assert_eq!(run.status, RunStatus::Done);
        assert_eq!(run.trigger, RunTrigger::Schedule);
        assert_eq!(run.scheduled_for, Some(next));
        assert_eq!(run.output.as_deref(), Some("Today: nothing new. All good."));
        assert!(run.finished_at.is_some());

        // A fresh chat named after the task, with the prompt as its first message.
        let cid = run.conversation_id.clone().expect("a chat");
        let chat = f.store.get_conversation(&cid).await.expect("chat");
        assert_eq!(chat.title, "Morning note");
        let msgs = f.store.list_messages(&cid).await.expect("messages");
        assert_eq!(msgs.len(), 2);
        assert!(lock(&f.heard.chats).iter().any(|c| c.id == cid));
        // The model was told it runs unattended.
        let req = f.fake.requests();
        assert!(
            req[0]
                .system
                .as_deref()
                .unwrap_or_default()
                .contains("scheduled run of the user's task “Morning note”")
        );
        // Afterwards it's an ordinary chat.
        assert!(!f.chat.is_unattended(&cid));
        assert!(!f.sched.is_run_chat(&cid));

        // Moved to tomorrow's 09:00.
        let t = f.store.get_task(&task.id).await.expect("task");
        assert_eq!(t.next_run_at, Some(next + 86_400_000));
        assert_eq!(t.last_run.map(|r| r.id), Some(run.id.clone()));
    }

    #[tokio::test]
    async fn ask_mode_waits_for_approval_and_auto_mode_does_not() {
        let f = fx(vec![
            write_call(),
            answer("Saved the note."),
            write_call(),
            answer("Saved it again."),
        ])
        .await;
        let ask = f.sched.create(daily("Asks")).await.expect("creates");
        f.sched.run_now(&ask.id).await.expect("starts");
        f.heard
            .wait_for(|h| h.statuses().contains(&RunStatus::WaitingApproval))
            .await;
        let run = f.store.task_runs(&ask.id, 1).await.expect("runs").remove(0);
        assert_eq!(run.status, RunStatus::WaitingApproval);
        assert_eq!(run.trigger, RunTrigger::Manual);
        let cid = run.conversation_id.clone().expect("chat");
        assert!(f.sched.is_run_chat(&cid));
        let pending = f.chat.pending_approvals();
        assert_eq!(pending.len(), 1);
        assert!(lock(&f.mcp.calls).is_empty());
        assert!(f.chat.answer_approval(
            &cid,
            &pending[0].tool_use_id,
            oxide_core::tools::ApprovalDecision::AllowOnce
        ));
        ended(&f, 1).await;
        let run = f.store.get_task_run(&run.id).await.expect("run");
        assert_eq!(run.status, RunStatus::Done);
        assert_eq!(
            run.output.as_deref(),
            Some("Writing the note.\n\nSaved the note.")
        );
        assert_eq!(lock(&f.mcp.calls).len(), 1);

        let mut input = daily("Runs alone");
        input.approval = ApprovalMode::AllowAll;
        let auto = f.sched.create(input).await.expect("creates");
        f.sched.run_now(&auto.id).await.expect("starts");
        ended(&f, 2).await;
        let run = f
            .store
            .task_runs(&auto.id, 1)
            .await
            .expect("runs")
            .remove(0);
        assert_eq!(run.status, RunStatus::Done);
        assert_eq!(lock(&f.mcp.calls).len(), 2);
        assert!(
            !lock(&f.heard.events)
                .iter()
                .any(|(n, s)| n == "Runs alone" && *s == RunStatus::WaitingApproval)
        );
        // Grants weren't saved: the chat asks again if the user carries on.
        let cid = run.conversation_id.expect("chat");
        assert!(
            f.store
                .chat_tool_grants(&cid)
                .await
                .expect("grants")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn auto_mode_runs_checked_calls_and_pauses_on_a_stopped_one() {
        let f = fx(vec![
            write_call(),
            answer("Saved the note."),
            write_call(),
            answer("Saved it again."),
        ])
        .await;
        let mut input = daily("Checks");
        input.approval = ApprovalMode::Auto;
        let task = f.sched.create(input).await.expect("creates");
        assert_eq!(task.approval, ApprovalMode::Auto);

        // The check lets the first run's write through: no card, no pause.
        f.sched.run_now(&task.id).await.expect("starts");
        ended(&f, 1).await;
        let run = f
            .store
            .task_runs(&task.id, 1)
            .await
            .expect("runs")
            .remove(0);
        assert_eq!(run.status, RunStatus::Done);
        assert_eq!(lock(&f.mcp.calls).len(), 1);
        assert_eq!(f.fake.safety_requests().len(), 1);
        let cid = run.conversation_id.clone().expect("chat");
        let used = f.store.tool_usage(&cid).await.expect("usage");
        assert_eq!(used[0].auto, Some(oxide_core::AutoMark::Checked));

        // The second run's check stops it: the run waits for approval, with the reason.
        f.fake.safety_says(vec![
            crate::chat::tests::SafetyAnswer::Text("FLAG"),
            crate::chat::tests::SafetyAnswer::Text(
                "DECISION: ASK | it overwrites a note the user didn't mention",
            ),
        ]);
        f.sched.run_now(&task.id).await.expect("starts");
        f.heard
            .wait_for(|h| {
                h.statuses()
                    .iter()
                    .filter(|s| **s == RunStatus::WaitingApproval)
                    .count()
                    >= 1
            })
            .await;
        let pending = f.chat.pending_approvals();
        assert_eq!(pending.len(), 1);
        assert_eq!(
            pending[0].auto_reason.as_deref(),
            Some("it overwrites a note the user didn't mention")
        );
        assert_eq!(lock(&f.mcp.calls).len(), 1);
        assert!(f.chat.answer_approval(
            &pending[0].conversation_id,
            &pending[0].tool_use_id,
            oxide_core::tools::ApprovalDecision::AllowOnce
        ));
        ended(&f, 2).await;
        assert_eq!(lock(&f.mcp.calls).len(), 2);
    }

    #[tokio::test]
    async fn missed_runs_follow_the_task_policy() {
        let f = fx(vec![answer("Caught up.")]).await;
        let once = f.sched.create(daily("Makes up")).await.expect("a");
        let mut skip = daily("Skips");
        skip.missed = MissedRunPolicy::Skip;
        let skip = f.sched.create(skip).await.expect("b");
        let now = now_ms();
        // Both were due three days ago (Oxide was closed).
        due_at(&f, &once.id, now - 3 * 86_400_000).await;
        due_at(&f, &skip.id, now - 3 * 86_400_000).await;
        f.sched.tick(now).await;
        ended(&f, 2).await;

        let made_up = f.store.task_runs(&once.id, 10).await.expect("runs");
        assert_eq!(made_up.len(), 1, "one run makes up for all missed ones");
        assert_eq!(made_up[0].trigger, RunTrigger::CatchUp);
        assert_eq!(made_up[0].status, RunStatus::Done);

        let skipped = f.store.task_runs(&skip.id, 10).await.expect("runs");
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].status, RunStatus::Missed);
        assert_eq!(skipped[0].conversation_id, None);

        // Both wait for their next time, after now.
        for id in [&once.id, &skip.id] {
            let t = f.store.get_task(id).await.expect("task");
            assert!(t.next_run_at.expect("next") > now);
        }
        // Only one model request was made.
        assert_eq!(f.fake.requests().len(), 1);
    }

    #[tokio::test]
    async fn runs_oxide_closed_during_are_marked_failed_at_start() {
        let f = fx(vec![]).await;
        let t = f.sched.create(daily("t")).await.expect("task");
        let run = TaskRun {
            id: new_id(),
            task_id: t.id.clone(),
            conversation_id: None,
            trigger: RunTrigger::Schedule,
            scheduled_for: Some(1),
            started_at: 1,
            finished_at: None,
            status: RunStatus::WaitingApproval,
            output: None,
            error: None,
        };
        f.store.add_task_run(&run).await.expect("adds");
        f.sched.recover().await;
        let r = f.store.get_task_run(&run.id).await.expect("run");
        assert_eq!(r.status, RunStatus::Failed);
        assert_eq!(r.error.as_deref(), Some(CLOSED_DURING_RUN));
    }

    #[tokio::test]
    async fn one_time_tasks_switch_off_after_their_run() {
        let f = fx(vec![answer("Reminder sent.")]).await;
        let mut input = daily("Once");
        let at = now_ms() + 3_600_000;
        input.schedule = TaskSchedule::Once { at };
        let t = f.sched.create(input.clone()).await.expect("creates");
        assert_eq!(t.next_run_at, Some(at));
        f.sched.tick(at).await;
        ended(&f, 1).await;
        let t = f.store.get_task(&t.id).await.expect("task");
        assert!(!t.enabled);
        assert_eq!(t.next_run_at, None);
        // A one-time task in the past is refused.
        input.schedule = TaskSchedule::Once { at: now_ms() - 1 };
        assert!(f.sched.create(input).await.is_err());
    }

    #[tokio::test]
    async fn pausing_editing_and_deleting() {
        let f = fx(vec![]).await;
        let t = f.sched.create(daily("t")).await.expect("creates");
        let paused = f.sched.set_enabled(&t.id, false).await.expect("pauses");
        assert!(!paused.enabled);
        assert_eq!(paused.next_run_at, None);
        // A paused task never comes due.
        f.sched.tick(now_ms() + 10 * 86_400_000).await;
        assert!(f.store.task_runs(&t.id, 10).await.expect("runs").is_empty());

        let resumed = f.sched.set_enabled(&t.id, true).await.expect("resumes");
        assert!(resumed.next_run_at.expect("next") > now_ms());

        let mut changed = daily("renamed");
        changed.schedule = TaskSchedule::Cron {
            expr: "30 7 * * 1-5".into(),
        };
        let u = f.sched.update(&t.id, changed).await.expect("updates");
        assert_eq!(u.name, "renamed");
        let next = u.next_run_at.expect("next");
        assert_eq!((next / 1000 + 3600).rem_euclid(86_400), 7 * 3600 + 1800);

        let mut bad = daily("x");
        bad.schedule = TaskSchedule::Cron {
            expr: "every day".into(),
        };
        assert!(matches!(
            f.sched.update(&t.id, bad).await,
            Err(AgentError::Invalid(_))
        ));

        f.sched.delete(&t.id).await.expect("deletes");
        assert!(f.sched.list().await.expect("lists").is_empty());
        assert!(lock(&f.heard.changes).iter().any(|c| c.deleted));
    }

    #[tokio::test]
    async fn a_run_is_skipped_while_the_previous_one_is_still_going() {
        let f = fx(vec![write_call(), answer("done")]).await;
        let t = f.sched.create(daily("slow")).await.expect("creates");
        f.sched.run_now(&t.id).await.expect("starts");
        f.heard
            .wait_for(|h| h.statuses().contains(&RunStatus::WaitingApproval))
            .await;
        let second = f.sched.run_now(&t.id).await.expect("records");
        assert_eq!(second.status, RunStatus::Skipped);
        assert_eq!(second.error.as_deref(), Some(STILL_RUNNING));
        // Stop the first: it ends as stopped.
        let first = f.store.task_runs(&t.id, 10).await.expect("runs");
        let cid = first
            .iter()
            .find_map(|r| r.conversation_id.clone())
            .expect("chat");
        assert!(f.chat.stop(&cid));
        ended(&f, 2).await;
        let runs = f.store.task_runs(&t.id, 10).await.expect("runs");
        let statuses: Vec<RunStatus> = runs.iter().map(|r| r.status).collect();
        assert!(statuses.contains(&RunStatus::Skipped));
        assert!(
            statuses.contains(&RunStatus::Stopped),
            "statuses: {statuses:?}"
        );
    }

    #[tokio::test]
    async fn deleting_a_runs_chat_stops_the_run_and_removes_it_from_the_history() {
        let f = fx(vec![write_call(), answer("never")]).await;
        let t = f.sched.create(daily("t")).await.expect("creates");
        let run = f.sched.run_now(&t.id).await.expect("starts");
        f.heard
            .wait_for(|h| h.statuses().contains(&RunStatus::WaitingApproval))
            .await;
        let cid = run.conversation_id.clone().expect("chat");
        // What `delete_conversation` does.
        f.chat.forget(&cid);
        f.store.delete_conversation(&cid).await.expect("deletes");
        // The run leaves the history with its chat, and the scheduler lets go of it.
        tokio::time::timeout(Duration::from_secs(5), async {
            while f.sched.is_run_chat(&cid) {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("the run ends");
        assert!(matches!(
            f.store.get_task_run(&run.id).await,
            Err(StoreError::NotFound { .. })
        ));
        assert!(f.store.task_runs(&t.id, 10).await.expect("runs").is_empty());
        assert!(!f.chat.is_unattended(&cid));
        // No "stopped" or "failed" notification for a run the user threw away.
        assert_eq!(f.heard.statuses(), vec![RunStatus::WaitingApproval]);
    }

    #[tokio::test]
    async fn a_run_whose_model_is_gone_fails_with_the_reason() {
        let f = fx(vec![]).await;
        let mut input = daily("Bad model");
        input.model = "nowhere/model".into();
        let t = f.sched.create(input).await.expect("creates");
        f.sched.run_now(&t.id).await.expect("starts");
        ended(&f, 1).await;
        let run = f.store.task_runs(&t.id, 1).await.expect("runs").remove(0);
        assert_eq!(run.status, RunStatus::Failed);
        assert!(run.error.is_some());
    }

    #[tokio::test]
    async fn the_loop_runs_a_task_when_its_time_comes() {
        let f = fx(vec![answer("On time.")]).await;
        f.sched.start();
        f.sched.start(); // only one loop
        let mut input = daily("Soon");
        let at = now_ms() + 1_500;
        input.schedule = TaskSchedule::Once { at };
        let t = f.sched.create(input).await.expect("creates");
        ended(&f, 1).await;
        let run = f.store.task_runs(&t.id, 1).await.expect("runs").remove(0);
        assert_eq!(run.status, RunStatus::Done);
        assert!(run.started_at >= at, "not early");
        assert!(run.started_at - at < 2_000, "on time");
        assert_eq!(f.fake.requests().len(), 1);
    }

    /// Tauri commands need these futures to be `Send`.
    #[allow(dead_code, clippy::let_underscore_future)]
    fn command_futures_are_send(s: Arc<Scheduler>) {
        fn send<T: Send>(_: T) {}
        send(s.list());
        send(s.get("t"));
        send(s.runs("t", 10));
        send(s.create(daily("x")));
        send(s.update("t", daily("x")));
        send(s.set_enabled("t", true));
        send(s.delete("t"));
        send(s.run_now("t"));
        send(s.recover());
        send(s.tick(0));
    }

    #[test]
    fn long_outputs_keep_their_end() {
        let long = format!("{}THE END", "x".repeat(MAX_RUN_OUTPUT_CHARS * 2));
        let out = output_of(&[ContentBlock::text(long.clone())]).expect("text");
        assert_eq!(out.chars().count(), MAX_RUN_OUTPUT_CHARS);
        assert!(out.starts_with('…') && out.ends_with("THE END"));
        assert_eq!(output_of(&[]), None);
    }
}
