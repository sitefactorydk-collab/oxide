-- Phase 13b: context management. A compaction's summary stands in for the start of a chat
-- in later requests (the transcript keeps every message).
--
-- It covers the thread up to and including `message_id`, or, with `after_tool_use_id`, that
-- reply only up to the step that ended with that tool call. `message_id` has no foreign key:
-- a reply compacted while it's written is saved after its compaction.
CREATE TABLE compactions (
    id                TEXT PRIMARY KEY NOT NULL,
    conversation_id   TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    message_id        TEXT NOT NULL,
    after_tool_use_id TEXT,
    summary           TEXT NOT NULL,
    trigger           TEXT NOT NULL,
    tokens_before     INTEGER NOT NULL DEFAULT 0,
    tokens_after      INTEGER NOT NULL DEFAULT 0,
    model             TEXT,
    created_at        INTEGER NOT NULL
);
CREATE INDEX compactions_by_conversation ON compactions(conversation_id, created_at);
