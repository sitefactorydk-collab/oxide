//! Typed events sent to the frontend (registered in `specta_builder`).

use oxide_agent::EventSink;
use oxide_core::{
    BrowserFrame, BrowserStatus, ChatStreamEvent, Conversation, ConversationChanged, DesktopAction,
    McpEvent, PythonStatus, SandboxChange, TaskList,
};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
use tauri_specta::Event;

/// A step of a reply being written (`events.chatStream` in TypeScript).
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct ChatStream(pub ChatStreamEvent);

/// A chat changed in the background, e.g. it got an automatic title
/// (`events.conversationUpdated`).
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct ConversationUpdated(pub ConversationChanged);

/// Something changed in the MCP layer: a server's state, its lists, a log line, progress,
/// or a question for the user (`events.mcpChanged`).
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct McpChanged(pub McpEvent);

/// Memory items were learned in the background (`events.memoryChanged`).
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct MemoryChanged {
    pub added: u32,
}

/// The model set a chat's task list (`events.taskListChanged`).
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct TaskListChanged(pub TaskList);

/// The managed Python environment is installing, or finished or failed doing so
/// (`events.pythonChanged`).
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct PythonChanged(pub PythonStatus);

/// Docker's status or a chat's sandbox changed (`events.sandboxChanged`): the frontend reads
/// it again.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct SandboxChanged(pub SandboxChange);

/// The browser agent is being set up, or a chat's browser opened or closed
/// (`events.browserChanged`).
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct BrowserChanged(pub BrowserStatus);

/// A live screenshot of a chat's browser while a browser call runs (`events.browserLive`).
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct BrowserLive(pub BrowserFrame);

/// The Chrome link changed: listening, a Chrome connected or went, pairing
/// (`events.chromeChanged`).
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct ChromeChanged(pub oxide_core::ChromeStatus);

/// A scheduled task or one of its runs changed (`events.scheduleChanged`).
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct ScheduleChanged(pub oxide_core::ScheduleChange);

/// The tray, a menu, quick entry or a deep link asks the main window to do something
/// (`events.desktopRequest`; sent to the main window only).
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct DesktopRequest(pub DesktopAction);

/// Forwards the Python environment's progress to every window.
pub struct TauriPythonSink {
    app: AppHandle,
}

impl TauriPythonSink {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl oxide_pyenv::PythonSink for TauriPythonSink {
    fn changed(&self, status: &PythonStatus) {
        if let Err(e) = PythonChanged(status.clone()).emit(&self.app) {
            tracing::warn!("couldn't send a Python update: {e}");
        }
    }
}

/// Forwards the sandbox's changes to every window.
pub struct TauriSandboxSink {
    app: AppHandle,
}

impl TauriSandboxSink {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl oxide_sandbox::SandboxSink for TauriSandboxSink {
    fn changed(&self, conversation_id: Option<&str>) {
        let change = SandboxChange {
            conversation_id: conversation_id.map(str::to_owned),
        };
        if let Err(e) = SandboxChanged(change).emit(&self.app) {
            tracing::warn!("couldn't send a sandbox update: {e}");
        }
    }
}

/// Forwards the browser agent's setup progress and live screenshots to every window.
pub struct TauriBrowserSink {
    app: AppHandle,
}

impl TauriBrowserSink {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl oxide_pyenv::BrowserSink for TauriBrowserSink {
    fn status(&self, status: &BrowserStatus) {
        if let Err(e) = BrowserChanged(status.clone()).emit(&self.app) {
            tracing::warn!("couldn't send a browser update: {e}");
        }
    }

    fn frame(&self, frame: BrowserFrame) {
        if let Err(e) = BrowserLive(frame).emit(&self.app) {
            tracing::warn!("couldn't send a browser screenshot: {e}");
        }
    }
}

/// Forwards the Chrome bridge's changes to every window; Stop pressed in a tab stops that
/// chat's reply.
pub struct TauriChromeSink {
    app: AppHandle,
}

impl TauriChromeSink {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl oxide_bridge::BridgeSink for TauriChromeSink {
    fn changed(&self, status: &oxide_core::ChromeStatus) {
        if let Err(e) = ChromeChanged(status.clone()).emit(&self.app) {
            tracing::warn!("couldn't send a Chrome update: {e}");
        }
    }

    fn stopped(&self, session: &str) {
        use tauri::Manager;
        if let Some(state) = self.app.try_state::<crate::state::AppState>() {
            tracing::info!(conversation = %session, "stopped from Chrome");
            state.chat.stop(session);
        }
    }
}

/// Forwards the scheduler's changes to every window and notifies about runs that need the
/// user or ended.
pub struct TauriScheduleSink {
    app: AppHandle,
}

impl TauriScheduleSink {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl oxide_agent::SchedulerSink for TauriScheduleSink {
    fn changed(&self, change: oxide_core::ScheduleChange) {
        if let Err(e) = ScheduleChanged(change).emit(&self.app) {
            tracing::warn!("couldn't send a schedule update: {e}");
        }
    }

    fn chat_started(&self, conversation: Conversation) {
        if let Err(e) = ConversationUpdated(ConversationChanged { conversation }).emit(&self.app) {
            tracing::warn!("couldn't send a conversation update: {e}");
        }
    }

    fn run_event(&self, task: &oxide_core::ScheduledTask, run: &oxide_core::TaskRun) {
        crate::desktop::notify::on_task_run(&self.app, task, run);
    }
}

/// Forwards reply events from `oxide-agent` to every window.
pub struct TauriSink {
    app: AppHandle,
}

impl TauriSink {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl EventSink for TauriSink {
    fn stream_event(&self, event: ChatStreamEvent) {
        crate::desktop::notify::on_stream_event(&self.app, &event);
        if let Err(e) = ChatStream(event).emit(&self.app) {
            tracing::warn!("couldn't send a chat stream event: {e}");
        }
    }

    fn conversation_changed(&self, conversation: Conversation) {
        if let Err(e) = ConversationUpdated(ConversationChanged { conversation }).emit(&self.app) {
            tracing::warn!("couldn't send a conversation update: {e}");
        }
    }

    fn memory_changed(&self, added: u32) {
        if let Err(e) = (MemoryChanged { added }).emit(&self.app) {
            tracing::warn!("couldn't send a memory update: {e}");
        }
    }

    fn tasks_changed(&self, list: TaskList) {
        if let Err(e) = TaskListChanged(list).emit(&self.app) {
            tracing::warn!("couldn't send a task list update: {e}");
        }
    }
}
