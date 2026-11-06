-- Phase 6: the tool loop.

-- "Allow always for this chat": tools that run without asking in one chat.
-- `tool_key` is `server/tool` for MCP tools and `builtin/<name>` for Oxide's own.
CREATE TABLE chat_tool_grants (
    conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    tool_key        TEXT NOT NULL,
    created_at      INTEGER NOT NULL,
    PRIMARY KEY (conversation_id, tool_key)
);

-- Every tool call a chat made ("Used in this chat", the Phase 11 session panel).
-- `message_id` is the reply's id; the reply is saved when it ends, so it isn't a foreign key.
CREATE TABLE tool_usage (
    id              TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    message_id      TEXT NOT NULL,
    tool_use_id     TEXT NOT NULL,
    kind            TEXT NOT NULL,
    server          TEXT,
    tool            TEXT NOT NULL,
    label           TEXT,
    is_error        INTEGER NOT NULL DEFAULT 0,
    created_at      INTEGER NOT NULL
);
CREATE INDEX tool_usage_by_chat ON tool_usage (conversation_id, created_at);
