-- A semantic binding is established only by a fenced writer publication.
-- Legacy rows remain unbound, with no invented trusted generation.
ALTER TABLE knowledge_index_generations ADD COLUMN semantic_key TEXT;
ALTER TABLE knowledge_index_generations ADD COLUMN requested_dimension INTEGER;
ALTER TABLE knowledge_index_generations ADD COLUMN updated_at_ms INTEGER;
CREATE UNIQUE INDEX knowledge_binding_unspecified
    ON knowledge_index_generations(semantic_key)
    WHERE semantic_key IS NOT NULL AND requested_dimension IS NULL;
CREATE UNIQUE INDEX knowledge_binding_explicit
    ON knowledge_index_generations(semantic_key,requested_dimension)
    WHERE requested_dimension IS NOT NULL;
CREATE INDEX documents_status ON documents(status);
CREATE INDEX document_chunks_ready ON document_chunks(index_state,embedding_identity);
-- Compact per-row publication evidence; old rows deliberately stay untrusted.
ALTER TABLE knowledge_index_work_items ADD COLUMN document_fence_json TEXT
    CHECK (document_fence_json IS NULL OR (typeof(document_fence_json)='text' AND length(CAST(document_fence_json AS BLOB))<=1024));
ALTER TABLE knowledge_index_work_items ADD COLUMN location_sha256 TEXT
    CHECK (location_sha256 IS NULL OR (typeof(location_sha256)='text' AND length(location_sha256)=64 AND location_sha256 NOT GLOB '*[^0-9a-f]*'));
ALTER TABLE knowledge_index_work_items ADD COLUMN previous_vector_id TEXT
    CHECK (previous_vector_id IS NULL OR (typeof(previous_vector_id)='text' AND length(previous_vector_id) BETWEEN 1 AND 20 AND previous_vector_id NOT GLOB '*[^0-9]*' AND (length(previous_vector_id)=1 OR substr(previous_vector_id,1,1)!='0') AND (length(previous_vector_id)<20 OR previous_vector_id<='18446744073709551615')));
