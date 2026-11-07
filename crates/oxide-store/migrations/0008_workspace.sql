-- Phase 11: agentic tasks.

-- Folders the user granted a chat; its file tools only reach paths inside them.
-- `path` is resolved (absolute, no `..`, no symbolic links).
CREATE TABLE chat_folders (
    conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    path            TEXT NOT NULL,
    name            TEXT NOT NULL,
    added_at        INTEGER NOT NULL,
    PRIMARY KEY (conversation_id, path)
);

-- Per-chat workspace options. `allow_delete`: delete_file deletes for good instead of
-- moving things into `_to_delete`.
CREATE TABLE chat_workspace (
    conversation_id TEXT PRIMARY KEY REFERENCES conversations(id) ON DELETE CASCADE,
    allow_delete    INTEGER NOT NULL DEFAULT 0
);

-- A chat's task list (the session panel's "Progress"): `items` is a JSON array of
-- `TaskItem`s.
CREATE TABLE chat_tasks (
    conversation_id TEXT PRIMARY KEY REFERENCES conversations(id) ON DELETE CASCADE,
    items           TEXT NOT NULL,
    updated_at      INTEGER NOT NULL
);
