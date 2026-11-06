-- Per-chat MCP tool switches (Phase 5). Everything is on unless listed in `settings`
-- (a JSON `ChatTools` object).
CREATE TABLE chat_tools (
    conversation_id TEXT PRIMARY KEY REFERENCES conversations(id) ON DELETE CASCADE,
    settings        TEXT NOT NULL
);
