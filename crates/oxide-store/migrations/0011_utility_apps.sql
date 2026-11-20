-- Phase 20: Utility Apps. An app's files live in its folder (or ship with Oxide); its
-- settings and storage live here, so an exported app carries no grants, keys or data.
--
-- `prefs` is JSON (`AppPrefs`: text model, generation overrides, granted capabilities).
CREATE TABLE app_prefs (
    app_id     TEXT PRIMARY KEY NOT NULL,
    prefs      TEXT NOT NULL DEFAULT '{}',
    updated_at INTEGER NOT NULL
);

-- Each app's own key-value storage (`oxide.storage` in the SDK). Values are JSON text.
CREATE TABLE app_storage (
    app_id     TEXT NOT NULL,
    key        TEXT NOT NULL,
    value      TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (app_id, key)
);
