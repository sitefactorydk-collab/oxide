-- Phase 26: what each builder checkpoint changed since the one before it, as JSON
-- (`[{"path", "kind": "added"|"changed"|"deleted", "added", "removed"}]`), shown in the
-- builder's Checkpoints menu. Older checkpoints have none.
ALTER TABLE app_checkpoints ADD COLUMN changes TEXT NOT NULL DEFAULT '[]';
