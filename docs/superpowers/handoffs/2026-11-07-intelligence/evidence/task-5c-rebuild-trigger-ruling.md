# Task5C local rebuild trigger clarification — 2026-11-07

Accepted internal trigger policy for a compatible clean/certified active
binding, before any embeddings and also in local reconcile:

- Certified metadata file>=12GiB (the already accepted watermark).
- Checked stored_count+actual intended current-version chunks>200000 with
  reclaimable records; after local copy, survivors+intended must still fit the
  unchanged200000 limit, otherwise ResourceLimit before an Embedding call.
- Obsolete/tombstoned count is at least50% of stored_count, using checked
  integer arithmetic. Count0 does not divide or repeatedly rebuild an already
  empty generation; the metadata watermark remains independent. A positive
  stored count with no surviving mappings rebuilds an empty fresh generation.

Eligible survivors and reclaimable count come from current authoritative
published mappings and matching Ready version/hash/parser/location evidence.
Do not reinterpret dirty/unsealed/missing bindings or automatically re-embed
repair documents. Preserve excluded document text/managed originals and repair
states. Rebuild uses existing bit-preserving owner ports,<=200 pages, the same
two-engine/two-worker/queue16 limits and atomic fenced pointer/mapping publish.
Reindex must consume the updated generation/pointer fence after rebuilding.

The50% threshold is an internal compaction heuristic for the approved bounded
local rebuild, not a new UI setting. It may perform an extra local generation
copy when half the records are obsolete; it cannot issue a paid request or
change the supported record/dimension caps.

Use real small-index regressions for tombstone pressure/dense survivor mapping
and last-document empty clean swap. Certified-seal/count helper arithmetic
tests may cover200000/12GiB boundaries without opening fabricated generations
or claiming a12GiB/maximum-size performance run. Metadata trust/provenance still
comes from the actual sealed owner; arithmetic alone never authorizes an open.

Cost if wrong: the50% heuristic may rebuild earlier than necessary, adding a
local copy and disk writes. Old source survives until fresh pointer publication
and actual close/cleanup acknowledgement; no source text, service key or remote
operation is affected.
