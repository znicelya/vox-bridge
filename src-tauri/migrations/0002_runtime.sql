ALTER TABLE translation_segments ADD COLUMN details_json TEXT;
ALTER TABLE translation_segments ADD COLUMN occurred_at_ms INTEGER;
CREATE TABLE session_runtime_context (
    session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
    context_json TEXT NOT NULL
);
CREATE TABLE session_diagnostics (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    occurred_at_ms INTEGER NOT NULL,
    payload_json TEXT NOT NULL
);
CREATE INDEX session_diagnostics_session ON session_diagnostics(session_id);
