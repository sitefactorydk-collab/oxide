//! Typed events sent to the frontend (registered in `specta_builder`).

use oxide_agent::EventSink;
use oxide_core::{ChatStreamEvent, Conversation, ConversationChanged};
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
        if let Err(e) = ChatStream(event).emit(&self.app) {
            tracing::warn!("couldn't send a chat stream event: {e}");
        }
    }

    fn conversation_changed(&self, conversation: Conversation) {
        if let Err(e) = ConversationUpdated(ConversationChanged { conversation }).emit(&self.app) {
            tracing::warn!("couldn't send a conversation update: {e}");
        }
    }
}
