CREATE TABLE intelligence_jobs (
    id TEXT PRIMARY KEY, kind TEXT NOT NULL, status TEXT NOT NULL,
    session_id TEXT REFERENCES sessions(id) ON DELETE CASCADE,
    object_id TEXT, exclusion_key TEXT NOT NULL, permanent_key TEXT UNIQUE,
    payload_json TEXT NOT NULL,
    completed_units INTEGER NOT NULL DEFAULT 0, total_units INTEGER,
    error_code TEXT, created_at_ms INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL
);
CREATE INDEX intelligence_jobs_status ON intelligence_jobs(status, created_at_ms);
CREATE UNIQUE INDEX intelligence_jobs_in_flight
    ON intelligence_jobs(exclusion_key) WHERE status IN ('queued', 'running');
CREATE TABLE service_probes (
    profile_id TEXT PRIMARY KEY, checked_at_ms INTEGER NOT NULL, result_json TEXT NOT NULL
);
CREATE TABLE intelligence_meta (key TEXT PRIMARY KEY, value_json TEXT NOT NULL);
