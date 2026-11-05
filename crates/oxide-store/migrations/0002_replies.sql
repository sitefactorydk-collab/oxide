-- Phase 3: reply metadata and the tool-calling probe cache.

-- Why an assistant reply ended (StopReason, camelCase) and its token counts (Usage JSON).
ALTER TABLE messages ADD COLUMN stop_reason TEXT;
ALTER TABLE messages ADD COLUMN usage TEXT;

-- Results of probing OpenAI-compatible servers that don't report capabilities (Strata):
-- one tiny tool-call request per model, remembered until the user refreshes. Keyed by the
-- endpoint URL so a changed URL is probed again.
CREATE TABLE capability_probes (
    base_url   TEXT NOT NULL,
    model      TEXT NOT NULL,
    tools      INTEGER NOT NULL CHECK (tools IN (0, 1)),
    checked_at INTEGER NOT NULL,
    PRIMARY KEY (base_url, model)
);
