-- Phase 23: permission modes. A chat's mode (Manual or Auto) once someone picked one; chats
-- without a row use the default from Settings → Permissions.
CREATE TABLE chat_permission_modes (
    conversation_id TEXT PRIMARY KEY NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    mode            TEXT NOT NULL
);

-- How Auto handled each tool call (readOnly, inFolder, sandboxed, allowed, checked, stopped);
-- NULL in Manual.
ALTER TABLE tool_usage ADD COLUMN auto TEXT;
