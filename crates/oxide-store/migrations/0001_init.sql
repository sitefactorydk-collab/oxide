-- Phase 2: projects, conversations, messages (a tree via parent_id), settings, providers,
-- and FTS5 indexes over chat titles and message text.
--
-- Tables that feed an FTS index have an explicit INTEGER PRIMARY KEY (`seq`) so the rowids the
-- index points at stay stable across VACUUM. Public ids are UUIDv7 strings.

CREATE TABLE projects (
    id           TEXT PRIMARY KEY NOT NULL,
    name         TEXT NOT NULL,
    description  TEXT NOT NULL DEFAULT '',
    instructions TEXT NOT NULL DEFAULT '',
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL
);

CREATE TABLE conversations (
    seq        INTEGER PRIMARY KEY,
    id         TEXT NOT NULL UNIQUE,
    title      TEXT NOT NULL,
    project_id TEXT REFERENCES projects(id) ON DELETE SET NULL,
    model      TEXT NOT NULL,
    starred    INTEGER NOT NULL DEFAULT 0 CHECK (starred IN (0, 1)),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX conversations_by_updated ON conversations(updated_at DESC);
CREATE INDEX conversations_by_project ON conversations(project_id);

CREATE TABLE messages (
    seq             INTEGER PRIMARY KEY,
    id              TEXT NOT NULL UNIQUE,
    conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    parent_id       TEXT REFERENCES messages(id) ON DELETE CASCADE,
    role            TEXT NOT NULL CHECK (role IN ('user', 'assistant')),
    content         TEXT NOT NULL,            -- JSON array of ContentBlock
    search_text     TEXT NOT NULL DEFAULT '', -- plain text of the content, for FTS
    model           TEXT,
    created_at      INTEGER NOT NULL
);
CREATE INDEX messages_by_conversation ON messages(conversation_id, seq);
CREATE INDEX messages_by_parent ON messages(parent_id);

CREATE TABLE settings (
    key        TEXT PRIMARY KEY NOT NULL,
    value      TEXT NOT NULL, -- JSON
    updated_at INTEGER NOT NULL
);

CREATE TABLE providers (
    id         TEXT PRIMARY KEY NOT NULL,
    kind       TEXT NOT NULL CHECK (kind IN ('anthropic', 'openai', 'lmstudio', 'strata')),
    name       TEXT NOT NULL,
    base_url   TEXT NOT NULL,
    enabled    INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    key_hint   TEXT, -- masked key (e.g. '••••1a2b'); the key itself lives in the OS keychain
    position   INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

-- The four built-in endpoints with their default URLs. Users can edit or add more.
INSERT INTO providers (id, kind, name, base_url, enabled, position, created_at, updated_at)
VALUES
    ('anthropic', 'anthropic', 'Anthropic', 'https://api.anthropic.com/v1', 1, 0,
        CAST(strftime('%s', 'now') AS INTEGER) * 1000, CAST(strftime('%s', 'now') AS INTEGER) * 1000),
    ('openai', 'openai', 'OpenAI', 'https://api.openai.com/v1', 1, 1,
        CAST(strftime('%s', 'now') AS INTEGER) * 1000, CAST(strftime('%s', 'now') AS INTEGER) * 1000),
    ('lmstudio', 'lmstudio', 'LM Studio', 'http://127.0.0.1:1234/v1', 1, 2,
        CAST(strftime('%s', 'now') AS INTEGER) * 1000, CAST(strftime('%s', 'now') AS INTEGER) * 1000),
    ('strata', 'strata', 'Strata', 'http://127.0.0.1:47831/v1', 1, 3,
        CAST(strftime('%s', 'now') AS INTEGER) * 1000, CAST(strftime('%s', 'now') AS INTEGER) * 1000);

-- Full-text search. External-content tables: the text lives once, in the source tables.
-- unicode61 with remove_diacritics 2 matches "cafe" to "café" and handles æ/ø/å.
CREATE VIRTUAL TABLE conversations_fts USING fts5(
    title,
    content = 'conversations',
    content_rowid = 'seq',
    tokenize = 'unicode61 remove_diacritics 2'
);
CREATE VIRTUAL TABLE messages_fts USING fts5(
    search_text,
    content = 'messages',
    content_rowid = 'seq',
    tokenize = 'unicode61 remove_diacritics 2'
);

CREATE TRIGGER conversations_fts_ai AFTER INSERT ON conversations BEGIN
    INSERT INTO conversations_fts (rowid, title) VALUES (new.seq, new.title);
END;
CREATE TRIGGER conversations_fts_ad AFTER DELETE ON conversations BEGIN
    INSERT INTO conversations_fts (conversations_fts, rowid, title)
    VALUES ('delete', old.seq, old.title);
END;
CREATE TRIGGER conversations_fts_au AFTER UPDATE OF title ON conversations BEGIN
    INSERT INTO conversations_fts (conversations_fts, rowid, title)
    VALUES ('delete', old.seq, old.title);
    INSERT INTO conversations_fts (rowid, title) VALUES (new.seq, new.title);
END;

CREATE TRIGGER messages_fts_ai AFTER INSERT ON messages BEGIN
    INSERT INTO messages_fts (rowid, search_text) VALUES (new.seq, new.search_text);
END;
CREATE TRIGGER messages_fts_ad AFTER DELETE ON messages BEGIN
    INSERT INTO messages_fts (messages_fts, rowid, search_text)
    VALUES ('delete', old.seq, old.search_text);
END;
CREATE TRIGGER messages_fts_au AFTER UPDATE OF search_text ON messages BEGIN
    INSERT INTO messages_fts (messages_fts, rowid, search_text)
    VALUES ('delete', old.seq, old.search_text);
    INSERT INTO messages_fts (rowid, search_text) VALUES (new.seq, new.search_text);
END;
