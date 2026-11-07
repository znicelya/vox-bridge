# Task5C compact work association evidence ruling — 2026-11-07

This additive migration5 clarification is accepted after the exact producer
check. Existing migration4 work rows already hold document UUID/version,
chunk UUID/content hash and vector ID. They do not retain the full document
SHA/parser/import association, canonical full-location digest or pre-copy
source vector mapping. A multi-document compact rebuild/recovery plan cannot
carry all those fences in memory, and checking only current rows at publish
would accept a same-version reassociation after copy.

Add only these nullable evidence columns to knowledge_index_work_items in
Task5C migration5, keeping the existing table/RESTRICT FKs/page/count bounds:

- `document_fence_json TEXT`: actual UTF-8 serialized bytes<=1024; strict
  `DocumentFence` with no unknown fields, canonical document UUID,
  version1..=u32::MAX, document SHA exactly64 lowercase hex, nonempty parser
  identity<=128 UTF-8 bytes and nullable canonical import-job UUID. Native
  PARSER_ID is a fixed short producer value; unknown oversized historical
  associations fail safely rather than being truncated or trusted.
- `location_sha256 TEXT`: nullable or exactly64 lowercase hexadecimal bytes,
  the existing codec's canonical complete TextLocation digest. Do not embed
  headings/full location objects in the compact work evidence.
- `previous_vector_id TEXT`: nullable or the existing canonical decimal u64
  encoding,<=20 bytes with no leading zero except0. Rebuild/recovery survivor
  work requires the source ID; plain reindex may leave it NULL.

Enforce raw byte bounds in SQLite CHECK constraints and inspect borrowed
ValueRefs before decoding/copying. New writers stage this evidence in<=200-row
pages. The final atomic publication walks durable pages and rechecks current
document version/SHA/parser/import, chunk/hash, canonical location hash and
source identity/generation/previous vector mapping before any Ready/pointer
commit. Missing, malformed or changed evidence rejects the whole publication
and leaves the dirty operation owned; no partial Ready update is allowed.

Legacy NULL evidence is never a trusted backfill. Existing unpublished work
without the needed proof cannot publish Ready or dirty-delete recovery and
keeps repair-required/Deleting state and source bytes until explicit recovery.
DeleteInput retains one bounded full DocumentFence for its target; survivor
sets remain durable paged work, not an all-document JSON array in the plan.

This extends accepted D2's additive migration5 schema only to realize the
accepted compact/fenced publication and dirty-delete recovery seam. No new
cleanup table, table rebuild, global schema rewrite, embedding call or engine/
worker is introduced. Summaries remain migration6 and research migration7.

Required coverage: correct staged publication plus rollback after same-version
SHA/parser/import reassociation, location mutation and old-vector remap; NULL/
malformed/over-byte-limit evidence rejects before Ready; multi-page publication
checks every record. Existing migration5 reopen/legacy/id constraints remain.

Cost if wrong: at most1024-byte document evidence plus two compact references
per staged work row and extra local validation/writes; invalid legacy evidence
stays visibly repair-required. The original text/files and the prior pointer
remain intact. No smaller supported document/chunk/dimension limit is inferred.
