-- Phase 8: artifacts made by the `artifact` tool, with every version kept.

CREATE TABLE artifacts (
    id              TEXT PRIMARY KEY NOT NULL,
    conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    -- The id the model chose; unique within the chat.
    key             TEXT NOT NULL,
    kind            TEXT NOT NULL
        CHECK (kind IN ('markdown', 'code', 'html', 'svg', 'mermaid', 'react')),
    title           TEXT NOT NULL,
    language        TEXT,
    version         INTEGER NOT NULL,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL,
    UNIQUE (conversation_id, key)
);
CREATE INDEX artifacts_by_updated ON artifacts(updated_at DESC);

CREATE TABLE artifact_versions (
    artifact_id TEXT NOT NULL REFERENCES artifacts(id) ON DELETE CASCADE,
    version     INTEGER NOT NULL,
    kind        TEXT NOT NULL,
    title       TEXT NOT NULL,
    language    TEXT,
    content     TEXT NOT NULL,
    -- The reply that made it (saved when the reply finishes, so no foreign key).
    message_id  TEXT,
    created_at  INTEGER NOT NULL,
    PRIMARY KEY (artifact_id, version)
);
