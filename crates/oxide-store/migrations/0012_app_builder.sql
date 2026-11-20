-- Phase 21: the Utility App builder. Each app has one builder chat (hidden from Recents,
-- the chat list and search) and checkpoints of its draft. A checkpoint keeps every file of
-- the draft as JSON (`[{"path": …, "data": <base64>}]`); `digest` skips identical versions.
CREATE TABLE app_builder_chats (
    app_id          TEXT PRIMARY KEY NOT NULL,
    conversation_id TEXT NOT NULL UNIQUE REFERENCES conversations(id) ON DELETE CASCADE
);

CREATE TABLE app_checkpoints (
    seq        INTEGER PRIMARY KEY AUTOINCREMENT,
    id         TEXT NOT NULL UNIQUE,
    app_id     TEXT NOT NULL,
    label      TEXT NOT NULL,
    kind       TEXT NOT NULL,
    file_count INTEGER NOT NULL,
    files      TEXT NOT NULL,
    digest     TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX app_checkpoints_by_app ON app_checkpoints(app_id, seq);
