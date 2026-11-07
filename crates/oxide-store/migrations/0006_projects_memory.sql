-- Phase 7: archived projects, knowledge files with BM25 search, and memory.

ALTER TABLE projects ADD COLUMN archived INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0, 1));

-- A project's knowledge: the extracted text of each file.
CREATE TABLE project_files (
    id         TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    kind       TEXT NOT NULL CHECK (kind IN ('text', 'pdf', 'note')),
    media_type TEXT NOT NULL,
    chars      INTEGER NOT NULL,
    pages      INTEGER,
    note       TEXT,
    text       TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX project_files_by_project ON project_files(project_id, created_at);

-- Passages of the files, indexed for `project_search` (FTS5, ranked with bm25()).
CREATE TABLE knowledge_chunks (
    seq        INTEGER PRIMARY KEY,
    file_id    TEXT NOT NULL REFERENCES project_files(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL,
    position   INTEGER NOT NULL,
    start      INTEGER NOT NULL,
    text       TEXT NOT NULL
);
CREATE INDEX knowledge_chunks_by_file ON knowledge_chunks(file_id, position);
CREATE INDEX knowledge_chunks_by_project ON knowledge_chunks(project_id);

CREATE VIRTUAL TABLE knowledge_fts USING fts5(
    text,
    content = 'knowledge_chunks',
    content_rowid = 'seq',
    tokenize = 'unicode61 remove_diacritics 2'
);
CREATE TRIGGER knowledge_fts_ai AFTER INSERT ON knowledge_chunks BEGIN
    INSERT INTO knowledge_fts (rowid, text) VALUES (new.seq, new.text);
END;
CREATE TRIGGER knowledge_fts_ad AFTER DELETE ON knowledge_chunks BEGIN
    INSERT INTO knowledge_fts (knowledge_fts, rowid, text) VALUES ('delete', old.seq, old.text);
END;

-- Memory items. `project_id` NULL = every chat; a project's items go with the project.
CREATE TABLE memories (
    id              TEXT PRIMARY KEY NOT NULL,
    project_id      TEXT REFERENCES projects(id) ON DELETE CASCADE,
    text            TEXT NOT NULL,
    conversation_id TEXT,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL
);
CREATE INDEX memories_by_project ON memories(project_id, created_at);
