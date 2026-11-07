# Task5C dirty delete recovery ruling — 2026-11-07

The interrupted indexed-delete case is required local recovery, not complete
when left permanently Deleting. Never construct SrvDB from dirty bytes.
Preserving Deleting/work/files is the mandatory fallback when proof is absent.

## Pinned evidence

At the approved SrvDB revision, `engine/delete.rs:14–27` only changes the
in-memory tombstone set and pending delete IDs. It does not change vectors or
stored count. `engine/commit.rs:14–27` flushes the vector storage and then
commits metadata/count/tombstones. `storage/flat.rs:62–63` delegates to
`RecordFile::sync`; `storage/record_file.rs:156–167` returns without writing
when count equals its last synced count. Therefore a delete-only mutation can
retain a byte-identical vector file even while metadata/seal state is dirty.
Treat this as a hypothesis proved by the exact file witness at runtime, never
as permission to assume all dirty generations are readable.

The retained prior Seal certifies `vectors.bin` name, complete byte length,
SHA-256, full f32 encoding, count/dimension and exact pinned policy. The dirty
Intent binds the prior seal digest and descriptor. This is enough for a
narrow vector-only read if all of that exact evidence still matches.

## Allowed recovery path

1. Under the application process lock and the actual current owner lease,
   validate the stored dirty intent/prior seal association and the expected
   active-generation pointer. This is read-only evidence of the old epoch;
   do not republish that epoch's receipt or make the dirty seal clean.
2. On the dedicated index owner/pool, take the source directory's exclusive
   `srvdb.lock`. A live SrvDB holding it yields Busy. Validate regular-file /
   symlink policy, current vector geometry/count/dimension/caps, exact byte
   length and streamed whole-file SHA-256 against the prior certified vector
   file. Open only an ordinary read-only File for vectors; never open dirty
   metadata/graph, VectorStorage, a writable recovery constructor or SrvDB.
3. Retain the source lock and reader through a bounded recovery ticket, using
   the same cancellation-safe reader ownership principles as maintenance.
   Copy at most200 records per page with checked offsets and bit-preserving
   little-endian f32 reads. All of this executes in the existing owner/pool;
   there is no extra execution worker or source SrvDB engine. Revalidate the
   vector witness after copying before approving publication.
4. Use the authoritative current published SQLite mappings / matching
   chunk-version/content-hash/parser associations to select survivors.
   Exclude every intended Deleting document and obsolete/unmapped records.
   Preserve otherwise-valid surviving documents that startup quarantined
   solely because the generation became unsealed; malformed/unmatched rows
   stay repair-required. Rebuild metadata from bounded authoritative SQLite
   locations. Do not infer survivors or metadata from dirty redb/graph bytes.
5. Create a fresh generation through the existing SrvDB owner, stage new
   mappings in bounded SQLite pages and persist/validate it. In one fenced
   publication transaction switch the generation pointer, publish every
   survivor mapping/Ready state and complete the intended document deletion
   / managed-file delete_pending state. Fence the exact old pointer, dirty
   intent/prior-seal evidence and current deletion/document associations.
   The new generation's persisted exclusion is the deletion durability
   proof; never pretend the old dirty generation was reopened or tombstoned
   again. An empty surviving library uses an empty fresh generation.
6. After destination publication/ack and the actual source-reader close,
   retire the unreferenced old managed generation, then acknowledge its
   retained work/mappings and perform managed-file cleanup in the existing
   durable sequence. A crash after pointer publication resumes cleanup
   locally; a failure beforehand keeps the old pointer/files/intents.

This is a narrow new reader/queue/publication seam allowed for5C. Keep the
queue16, exactly two Rayon workers, two-engine cap, one bounded reader lease,
200-record pages, record/file caps and owned completion. It does not relax
the earlier rule for writable VectorStorage readers or dirty SrvDB opens.
No embedding/profile credential is required for this local recovery.

## Failure fallback and required evidence

If the prior seal/intent/vector witness, geometry, current mappings or fences
cannot be proved, preserve bytes and the Deleting cleanup intent, expose
IndexRepairRequired and keep automatic embedding calls at zero. Never erase
the only good source, invent Ready rows or bless a changed vector file. This
is the explicit dirty/tampered-data exception; it cannot stand in for the
normal witnessed delete-only crash case.

Add real-index regressions for: two indexed documents, crash after actual
delete persist before publication, reopen and local recovery deleting one
while preserving the other's vector f32 bits/searchability; last-document
delete with empty destination; vector-witness mismatch leaves all source bytes
and Deleting/work state intact; crash after destination persist before pointer
and after pointer before retired cleanup; repeated reopen idempotence;
zero embedding calls during all local recovery. Verify no dirty SrvDB open
and no extra engine/worker is used. Keep the source-lock overlap and reader
completion evidence explicit.

Cost if wrong: a new strictly read-only recovery lease/port and an extra local
generation copy; witness mismatch remains visibly repair-required. Source
text and the user's original files stay intact.
