-- Existing source references and nullable legacy metadata are preserved verbatim.
ALTER TABLE documents ADD COLUMN kind TEXT NOT NULL DEFAULT 'unknown'
    CHECK (kind IN ('unknown','pdf','docx','text','markdown'));
ALTER TABLE documents ADD COLUMN origin TEXT NOT NULL DEFAULT 'legacy'
    CHECK (origin IN ('legacy','native'));
ALTER TABLE documents ADD COLUMN version INTEGER NOT NULL DEFAULT 1
    CHECK (version BETWEEN 1 AND 4294967295);
ALTER TABLE documents ADD COLUMN original_path TEXT;
ALTER TABLE documents ADD COLUMN managed_path TEXT;
ALTER TABLE documents ADD COLUMN status TEXT NOT NULL DEFAULT 'repair_required'
    CHECK (status IN ('parsing','parsed','indexing','ready','repair_required','failed','deleting'));
ALTER TABLE documents ADD COLUMN error_code TEXT;
ALTER TABLE documents ADD COLUMN updated_at_ms INTEGER;
ALTER TABLE documents ADD COLUMN index_identity TEXT;
ALTER TABLE documents ADD COLUMN parser_identity TEXT;
ALTER TABLE documents ADD COLUMN import_job_id TEXT REFERENCES intelligence_jobs(id) ON DELETE SET NULL;

ALTER TABLE document_chunks ADD COLUMN location_json TEXT;
ALTER TABLE document_chunks ADD COLUMN content_hash TEXT;
ALTER TABLE document_chunks ADD COLUMN document_version INTEGER
    CHECK (document_version IS NULL OR document_version BETWEEN 1 AND 4294967295);
ALTER TABLE document_chunks ADD COLUMN embedding_identity TEXT;
ALTER TABLE document_chunks ADD COLUMN vector_id TEXT CHECK (
    vector_id IS NULL OR (
        typeof(vector_id)='text' AND length(vector_id) BETWEEN 1 AND 20
        AND vector_id NOT GLOB '*[^0-9]*'
        AND (length(vector_id)=1 OR substr(vector_id,1,1)!='0')
        AND (length(vector_id)<20 OR vector_id<='18446744073709551615')
    )
);
ALTER TABLE document_chunks ADD COLUMN index_state TEXT NOT NULL DEFAULT 'repair_required';
CREATE INDEX documents_parser_content ON documents(sha256,parser_identity);
CREATE INDEX document_chunks_index_identity ON document_chunks(embedding_identity,index_state);

CREATE TABLE knowledge_index_generations (
    identity TEXT PRIMARY KEY,
    profile_id TEXT,
    dimension INTEGER CHECK (dimension IS NULL OR dimension BETWEEN 128 AND 4096),
    active_generation TEXT,
    state TEXT NOT NULL
);
CREATE TABLE knowledge_repairs (
    document_id TEXT PRIMARY KEY REFERENCES documents(id) ON DELETE CASCADE,
    reason TEXT NOT NULL
);
INSERT INTO knowledge_repairs (document_id,reason)
    SELECT id,'legacy_source_required' FROM documents WHERE origin='legacy';

-- Only Task5's controlled writer may publish trusted versioned seals. Migration
-- creates no seal or creation intent for any existing directory or legacy row.
CREATE TABLE knowledge_index_generation_seals (
    identity TEXT NOT NULL REFERENCES knowledge_index_generations(identity) ON DELETE CASCADE,
    generation TEXT NOT NULL,
    seal_json TEXT CHECK (
        CASE WHEN seal_json IS NULL THEN 1
             WHEN typeof(seal_json)!='text' OR length(CAST(seal_json AS BLOB))>65536 THEN 0
             WHEN NOT json_valid(seal_json) THEN 0
             ELSE COALESCE(json_type(seal_json,'$.version')='integer' AND json_extract(seal_json,'$.version')=1,0)
        END
    ),
    dirty INTEGER NOT NULL CHECK (dirty IN (0,1)),
    intent_json TEXT CHECK (
        CASE WHEN intent_json IS NULL THEN 1
             WHEN typeof(intent_json)!='text' OR length(CAST(intent_json AS BLOB))>16384 THEN 0
             WHEN NOT json_valid(intent_json) THEN 0
             ELSE COALESCE(json_type(intent_json,'$.version')='integer' AND json_extract(intent_json,'$.version')=1,0)
        END
    ),
    updated_at_ms INTEGER NOT NULL,
    PRIMARY KEY(identity,generation),
    CHECK ((dirty=0 AND seal_json IS NOT NULL AND intent_json IS NULL)
        OR (dirty=1 AND intent_json IS NOT NULL))
);

-- Generated filenames only; source names never select filesystem cleanup paths.
CREATE TABLE knowledge_managed_files (
    file_id TEXT PRIMARY KEY,
    relative_name TEXT NOT NULL UNIQUE CHECK (
        length(relative_name) BETWEEN 1 AND 96
        AND relative_name NOT GLOB '*[^a-z0-9.-]*'
        AND relative_name NOT IN ('.','..')
    ),
    document_id TEXT REFERENCES documents(id) ON DELETE SET NULL,
    document_version INTEGER CHECK (document_version IS NULL OR document_version BETWEEN 1 AND 4294967295),
    job_id TEXT REFERENCES intelligence_jobs(id) ON DELETE SET NULL,
    state TEXT NOT NULL CHECK (state IN ('copying','retained','delete_pending','awaiting_reap')),
    created_at_ms INTEGER NOT NULL
);
CREATE INDEX knowledge_managed_files_cleanup ON knowledge_managed_files(state,created_at_ms,file_id);
CREATE INDEX knowledge_managed_files_document ON knowledge_managed_files(document_id,document_version);
CREATE TABLE knowledge_document_imports (
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    document_version INTEGER NOT NULL CHECK (document_version BETWEEN 1 AND 4294967295),
    job_id TEXT NOT NULL UNIQUE REFERENCES intelligence_jobs(id) ON DELETE CASCADE,
    parser_identity TEXT NOT NULL,
    sha256 TEXT NOT NULL,
    file_id TEXT REFERENCES knowledge_managed_files(file_id) ON DELETE SET NULL,
    created_at_ms INTEGER NOT NULL,
    PRIMARY KEY(document_id,document_version)
);

-- Future Task5 work sets and historical mappings are intentionally independent
-- of live document/chunk/job rows. Their IDs must survive replacement/deletion
-- until the owning operation acknowledges persistence and filesystem cleanup.
CREATE TABLE knowledge_index_work_items (
    operation_id TEXT NOT NULL CHECK ((typeof(operation_id)='text' AND length(CAST(operation_id AS BLOB)) BETWEEN 1 AND 36)),
    ordinal INTEGER NOT NULL CHECK (typeof(ordinal)='integer' AND ordinal BETWEEN 0 AND 199999),
    identity TEXT NOT NULL CHECK ((typeof(identity)='text' AND length(CAST(identity AS BLOB)) BETWEEN 1 AND 160)),
    generation TEXT NOT NULL CHECK ((typeof(generation)='text' AND length(CAST(generation AS BLOB)) BETWEEN 1 AND 160)),
    vector_id TEXT NOT NULL CHECK (
        typeof(vector_id)='text' AND length(vector_id) BETWEEN 1 AND 20
        AND vector_id NOT GLOB '*[^0-9]*'
        AND (length(vector_id)=1 OR substr(vector_id,1,1)!='0')
        AND (length(vector_id)<20 OR vector_id<='18446744073709551615')
    ),
    document_id TEXT CHECK (document_id IS NULL OR (typeof(document_id)='text' AND length(CAST(document_id AS BLOB)) BETWEEN 1 AND 36)),
    document_version INTEGER CHECK (document_version IS NULL OR (typeof(document_version)='integer' AND document_version BETWEEN 1 AND 4294967295)),
    chunk_id TEXT CHECK (chunk_id IS NULL OR (typeof(chunk_id)='text' AND length(CAST(chunk_id AS BLOB)) BETWEEN 1 AND 36)),
    content_hash TEXT CHECK (content_hash IS NULL OR (typeof(content_hash)='text' AND length(CAST(content_hash AS BLOB)) BETWEEN 1 AND 64)),
    PRIMARY KEY(operation_id,ordinal),
    FOREIGN KEY(identity,generation) REFERENCES knowledge_index_generation_seals(identity,generation) ON DELETE RESTRICT
);
CREATE INDEX knowledge_index_work_items_generation
    ON knowledge_index_work_items(identity,generation,operation_id,ordinal);

CREATE TABLE knowledge_generation_mappings (
    identity TEXT NOT NULL CHECK ((typeof(identity)='text' AND length(CAST(identity AS BLOB)) BETWEEN 1 AND 160)),
    generation TEXT NOT NULL CHECK ((typeof(generation)='text' AND length(CAST(generation AS BLOB)) BETWEEN 1 AND 160)),
    chunk_id TEXT NOT NULL CHECK ((typeof(chunk_id)='text' AND length(CAST(chunk_id AS BLOB)) BETWEEN 1 AND 36)),
    document_id TEXT NOT NULL CHECK ((typeof(document_id)='text' AND length(CAST(document_id AS BLOB)) BETWEEN 1 AND 36)),
    document_version INTEGER NOT NULL CHECK (typeof(document_version)='integer' AND document_version BETWEEN 1 AND 4294967295),
    vector_id TEXT NOT NULL CHECK (
        typeof(vector_id)='text' AND length(vector_id) BETWEEN 1 AND 20
        AND vector_id NOT GLOB '*[^0-9]*'
        AND (length(vector_id)=1 OR substr(vector_id,1,1)!='0')
        AND (length(vector_id)<20 OR vector_id<='18446744073709551615')
    ),
    content_hash TEXT NOT NULL CHECK ((typeof(content_hash)='text' AND length(CAST(content_hash AS BLOB)) BETWEEN 1 AND 64)),
    published INTEGER NOT NULL DEFAULT 0 CHECK (typeof(published)='integer' AND published IN (0,1)),
    PRIMARY KEY(identity,generation,chunk_id),
    UNIQUE(identity,generation,vector_id),
    FOREIGN KEY(identity,generation) REFERENCES knowledge_index_generation_seals(identity,generation) ON DELETE RESTRICT
);
CREATE INDEX knowledge_generation_mappings_document
    ON knowledge_generation_mappings(document_id,document_version,identity,generation,chunk_id);
