-- Phase 15: scheduled tasks. A task is a saved prompt that runs on its own, each run in a
-- fresh chat; `task_runs` is its history.
--
-- `schedule`, `folders` and `tools` are JSON (`TaskSchedule`, a list of paths, `ChatTools`).
-- A run keeps its row when its chat is deleted (`conversation_id` becomes NULL).
CREATE TABLE scheduled_tasks (
    id          TEXT PRIMARY KEY NOT NULL,
    name        TEXT NOT NULL,
    prompt      TEXT NOT NULL,
    schedule    TEXT NOT NULL,
    model       TEXT NOT NULL,
    project_id  TEXT REFERENCES projects(id) ON DELETE SET NULL,
    folders     TEXT NOT NULL DEFAULT '[]',
    tools       TEXT NOT NULL DEFAULT '{}',
    thinking    INTEGER NOT NULL DEFAULT 0 CHECK (thinking IN (0, 1)),
    approval    TEXT NOT NULL DEFAULT 'ask',
    missed      TEXT NOT NULL DEFAULT 'runOnce',
    enabled     INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    next_run_at INTEGER,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);

CREATE TABLE task_runs (
    seq             INTEGER PRIMARY KEY,
    id              TEXT NOT NULL UNIQUE,
    task_id         TEXT NOT NULL REFERENCES scheduled_tasks(id) ON DELETE CASCADE,
    conversation_id TEXT REFERENCES conversations(id) ON DELETE SET NULL,
    trigger         TEXT NOT NULL,
    scheduled_for   INTEGER,
    started_at      INTEGER NOT NULL,
    finished_at     INTEGER,
    status          TEXT NOT NULL,
    output          TEXT,
    error           TEXT
);
CREATE INDEX task_runs_by_task ON task_runs(task_id, seq DESC);
CREATE INDEX task_runs_by_conversation ON task_runs(conversation_id);
CREATE INDEX task_runs_by_status ON task_runs(status);
