ALTER TABLE session_summaries ADD COLUMN profile_id TEXT;
ALTER TABLE session_summaries ADD COLUMN model TEXT;
ALTER TABLE session_summaries ADD COLUMN input_hash TEXT;
ALTER TABLE session_summaries ADD COLUMN input_final_count INTEGER
    CHECK(input_final_count IS NULL OR input_final_count BETWEEN 0 AND 100000);
ALTER TABLE session_summaries ADD COLUMN snapshot_last_rowid INTEGER;
ALTER TABLE session_summaries ADD COLUMN error_code TEXT;
ALTER TABLE session_summaries ADD COLUMN raw_response_hash TEXT;
ALTER TABLE session_summaries ADD COLUMN origin TEXT NOT NULL DEFAULT 'legacy'
    CHECK(origin IN ('native','legacy'));
ALTER TABLE session_summaries ADD COLUMN job_id TEXT
    REFERENCES intelligence_jobs(id) DEFERRABLE INITIALLY DEFERRED;
CREATE UNIQUE INDEX summary_job_id ON session_summaries(job_id);
ALTER TABLE session_summaries ADD COLUMN created_at_ms INTEGER;
ALTER TABLE session_summaries ADD COLUMN raw_response_bytes INTEGER NOT NULL DEFAULT 0
    CHECK(raw_response_bytes BETWEEN 0 AND 33554432);
ALTER TABLE session_summaries ADD COLUMN deleting INTEGER NOT NULL DEFAULT 0
    CHECK(deleting IN (0,1));
-- The witness is independent of intelligence_jobs.payload_json and backend only.
ALTER TABLE session_summaries ADD COLUMN job_payload_hash TEXT
    CHECK((origin='legacy' AND job_payload_hash IS NULL) OR
          (origin='native' AND job_payload_hash IS NOT NULL AND
           length(CAST(job_payload_hash AS BLOB))=64 AND
           job_payload_hash NOT GLOB '*[^0-9a-f]*' AND
           version BETWEEN 1 AND 4294967295 AND job_id IS NOT NULL AND
           created_at_ms IS NOT NULL));

CREATE TABLE summary_responses (
    summary_id TEXT NOT NULL REFERENCES session_summaries(id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL CHECK(ordinal BETWEEN 0 AND 63),
    phase TEXT NOT NULL CHECK(phase IN ('map','reduce')),
    raw_text TEXT NOT NULL CHECK(length(CAST(raw_text AS BLOB)) <= 2097152),
    response_hash TEXT NOT NULL CHECK(length(CAST(response_hash AS BLOB))=64 AND
                                    response_hash NOT GLOB '*[^0-9a-f]*'),
    PRIMARY KEY(summary_id,ordinal)
);
CREATE TABLE summary_coverage (
    summary_id TEXT NOT NULL REFERENCES session_summaries(id) ON DELETE CASCADE,
    segment_id TEXT NOT NULL,
    part_index INTEGER NOT NULL CHECK(part_index >= 0 AND part_index < part_count),
    part_count INTEGER NOT NULL CHECK(part_count > 0 AND part_count <= 100000),
    PRIMARY KEY(summary_id,segment_id,part_index)
);
CREATE TABLE summary_version_counters (
    session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
    last_version INTEGER NOT NULL CHECK(last_version >= 0)
);
INSERT INTO summary_version_counters(session_id,last_version)
    SELECT s.id,max(0,coalesce(max(v.version),0)) FROM sessions s
    LEFT JOIN session_summaries v ON v.session_id=s.id GROUP BY s.id;

ALTER TABLE session_lifecycle ADD COLUMN summary_terminal_pending INTEGER NOT NULL DEFAULT 0
    CHECK(summary_terminal_pending IN (0,1));
ALTER TABLE session_lifecycle ADD COLUMN summary_terminal_generation TEXT;
ALTER TABLE session_lifecycle ADD COLUMN summary_terminal_error TEXT
    CHECK(summary_terminal_error IS NULL OR summary_terminal_error IN
          ('busy','interrupted','resource_limit','storage','configuration'));
ALTER TABLE session_lifecycle ADD COLUMN summary_session_deleting INTEGER NOT NULL DEFAULT 0
    CHECK(summary_session_deleting IN (0,1));
CREATE INDEX summary_snapshot_session ON translation_segments(session_id);
CREATE INDEX summary_terminal_pending ON session_lifecycle(summary_terminal_pending,session_id);
CREATE INDEX summary_terminal_eligible ON session_lifecycle(status,session_id);
