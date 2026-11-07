# Narrow m4 durable Task5 work/mapping storage checkpoint

Proposal only; no Task5 engine/metadata codec/behavior. Existing64KiB seal and16KiB intent bounds remain. Full TextLocation stays in canonical SQLite; no4A heading restriction/truncation.

Add exactly two empty private tables to unreviewed migration4:

1. `knowledge_index_work_items(operation_id TEXT, ordinal INTEGER, identity TEXT, generation TEXT, vector_id TEXT, document_id TEXT NULL, document_version INTEGER NULL, chunk_id TEXT NULL, content_hash TEXT NULL, PRIMARY KEY(operation_id,ordinal))`.
   - Ordinal integer0..199999 limits each referenced work set to200000 items without a bulk JSON list. The bounded existing dirty intent holds operation kind/phase/item count/next ordinal; no third operation-header table is needed.
   - Required vector_id is canonical decimal u64 TEXT (same checked length/digits/no-leading-zero/max-u64 constraint as document_chunks.vector_id). ID0 remains valid. Optional bounded document/chunk/version/hash are snapshots for reconciliation, never authoritative live references; they may be NULL for orphan-vector cleanup.
   - No document/chunk/job FK: deleting a document, replacing canonical chunk rows, or retiring a job cannot erase desired prior-vector cleanup. Items remain until Task5 acknowledges actual persist/file cleanup, then explicitly deletes the work set or advances its persisted cursor.
   - Composite `(identity,generation)` FK to knowledge_index_generation_seals with **ON DELETE RESTRICT**, retaining source generation provenance and blocking identity/seal cascade deletion while work exists. Index `(identity,generation,operation_id,ordinal)`. Task5 later pages by operation_id and ordinal with LIMIT<=200 under its owner fence.

2. `knowledge_generation_mappings(identity TEXT,generation TEXT,chunk_id TEXT,document_id TEXT,document_version INTEGER,vector_id TEXT,content_hash TEXT,published INTEGER DEFAULT0, PRIMARY KEY(identity,generation,chunk_id), UNIQUE(identity,generation,vector_id))`.
   - No FK to current documents/document_chunks: historical mappings and old vector IDs survive canonical chunk-row replacement and document deletion until acknowledged cleanup.
   - Same RESTRICT composite FK to generation seals. Canonical decimal u64 TEXT, positive u32 document_version, bounded ID/hash strings and published IN(0,1). Index `(document_id,document_version,identity,generation,chunk_id)` supports paged prior-version cleanup. Stage mappings are unpublished by default.
   - A published bit alone never authorizes search. Task5 hydration must join identity+generation to knowledge_index_generations.active_generation and current documents/chunks/version/hash/Ready state. Pointer/seal/mapping/Ready publication remains one future transaction. Stale/dangling/historical rows remain recoverable storage and cannot be hydrated as current hits.

String envelopes: operation/document/chunk IDs <=36bytes, identity/generation<=160bytes, content_hash<=64bytes; non-NULL keys nonempty. Producer typed UUID/hash/identity validation remains Task5's codec boundary. Optional snapshot document_version must be positive u32 when present. Byte counts use length(CAST(... AS BLOB)); no arbitrary text/location/metadata/job JSON is stored in either table. The SQL state is empty after migration and is not a lease, deletion acknowledgement or trust certificate.

Tests in4B only: empty migration; inserts of high/u64-max IDs without numeric truncation; reject malformed/noncanonical/out-of-range IDs, bad ordinals/version/published values and duplicate keys; delete document/chunks/job and verify work/mappings survive; generation/identity deletion blocked until rows explicitly removed; paged operation reads<=200 recover all items without an array in job/intent; staged vs active-generation/current-version join excludes historical mappings. No Task5 runtime writes, vector engine, embedding, cleanup acknowledgement logic or actual mapping publication is implemented now.
