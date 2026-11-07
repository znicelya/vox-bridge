# Task 5 seal/intent format and TextLocation capacity handoff

Status: read-only readiness preparation; **proposed Task 5 formats/caps, pending its architecture checkpoint/controller ruling**. This report does not approve or inspect the in-progress Task 4B implementation. It uses the accepted 4B architecture checkpoint's private m4 seal schema, the Task 5 brief/managed-caps handoff, and only the frozen reviewed 4A TextLocation/bounds contract. No source, schema, index, HEAD, stage, or commit change; no Cargo, engine, native, network, or test run.

## Decision needed

The accepted m4 **64 KiB seal / 16 KiB dirty-intent** columns comfortably hold bounded generation provenance and operation descriptors. No larger m4 JSON column is necessary for the formats below. A bulk operation must reference durable paged item/mapping rows rather than embed all vector IDs in its intent.

The proposed **16 KiB metadata value does not hold every reviewed 4A TextLocation without truncation**. A valid Markdown heading `"# " + "电".repeat(6000)` has an 18000-byte heading; the compact complete metadata example below is **18318 bytes**. The source is approximately 18 KiB, has approximately 6000 extracted scalars, and its text/heading duplication remains far below the 16 MiB parser-output limit. The heading has no independent length restriction in the frozen contract and is retained in chunk locations. Do not invent a heading limit or truncate it to make the earlier cap appear sufficient.

Task 5 must explicitly choose either bounded vector metadata referencing/digesting the authoritative full SQLite location, or larger full-location metadata/file caps. Both alternatives retain the complete reviewed location and the 200000-record / 128–4096-dimension support. These are proposals; the current Task 5 brief's full `location` metadata contract has not been silently changed.

## Accepted private m4 envelope

`knowledge_index_generation_seals(identity,generation,seal_json,dirty,intent_json,updated_at_ms)` has composite key `(identity,generation)`, an identity FK with ON DELETE CASCADE, `dirty IN (0,1)`, UTF-8 byte limits seal <=65536 and intent <=16384, valid JSON with root `version:1`, and these invariants:

- Clean: a valid non-NULL seal and NULL intent.
- Dirty: a valid non-NULL intent, with an optional retained prior seal.
- Migration synthesizes no trusted rows. Task 5 owns the exact codec, file/pin policy, hashing and trust establishment.

The SQL envelope is durable storage. Task 5 must also decode into bounded typed formats, validate every field and supported policy, and reject unknown/inconsistent provenance before writable source open. SQL-valid `version:1` alone is not a trust certificate.

## Proposed bounded seal format

Use compact UTF-8 JSON, root `version:1`, and a fixed codec with no arbitrary extension strings. Example values/hashes below are illustrative and confer no trust:

```json
{"version":1,"kind":"srvdb_generation_seal","policy_version":1,"srvdb_revision":"1eec46feb2b7e6a6e286fa3361a20cd771dd4e04","identity":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","generation":"11111111-1111-4111-8111-111111111111","embedding_identity":{"profile_id":"11111111-1111-4111-8111-111111111111","base_url":"https://example.test/v1","model":"embedding-model","dimensions":4096,"normalization_version":1},"stored_count":200000,"writer":{"vector_header_version":4,"graph_envelope_version":1,"vector_encoding":"f32","index_mode":"Auto","metadata_codec":"chunk_location_ref_v1","metadata_schema_version":2},"limits":{"stored_records":200000,"metadata_value_bytes":16384,"vectors_bytes":4915200036,"metadata_bytes":17179869184,"graph_bytes":536870912},"files":[{"name":"vectors.bin","bytes":3276800024,"sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},{"name":"metadata.db","bytes":17179869184,"sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},{"name":"hnsw.graph","bytes":536870912,"sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}]}
```

This example deliberately illustrates the **reference-metadata alternative**, with metadata schema 2; if the full-location option is chosen, pin its corresponding codec, schema 1 and larger limits instead. Root seal/intent `version:1` remains compatible with m4. The finalized writer policy must reject unsupported codecs rather than reinterpret an old generation.

Field contract:

- `identity` is 64 lowercase hexadecimal characters, hashing the agreed canonical JSON of the full EmbeddingIdentity. Store the bounded identity itself as well; rederive and compare the hash. No credential/key/header/secret enters either form.
- `generation` and `profile_id` are canonical ASCII UUIDs, 36 bytes. The reviewed configuration envelope carried into the calculation is base_url <=2048 UTF-8 bytes and model <=200 Unicode scalars, not arbitrary document text. Dimensions are 128–4096; normalization_version is u32. Actual endpoint/model normalization and canonical identity JSON must be fixed by the author checkpoint.
- `srvdb_revision` is exactly the accepted 40-hex pin. `policy_version` and the writer/limits fields name the accepted vector header v4, graph envelope v1, full-precision f32, Auto policy and exact metadata codec/caps. Auto is a mode policy, not a claim that every generation is always HNSW.
- `stored_count` is u64 in 0..=200000 **including tombstones**, matching persisted source count/header at sealing; it is not just the live chunk count. All sizes use checked u64 arithmetic and exact per-file limits.
- `files` contains exactly the required `vectors.bin` and `metadata.db`, and at most one optional `hnsw.graph`, in a fixed order. Names are these literals, hashes are 64 lowercase hex characters, lengths are u64 within accepted caps. No absolute/relative user path, per-record manifest, arbitrary filename list or lock/temp-file checksum is accepted. Handle derived graph absence only through the separately approved sealed-base/missing-graph policy; a removed manifest entry does not bless a present malformed graph.

Pure static compact JSON encoding measured the example at **1141 bytes**; using a 2048-byte URL, 200 supplementary-plane model scalars and maximal u32 normalization_version gives **3960 bytes**. Even replacing both strings by a conservative JSON-escape envelope of six bytes per source URL byte / model scalar gives **14600 bytes**, still below 65536. Supported numeric widths and fixed enum variations leave ample headroom; enforce the final serialized bound anyway. These are format-size calculations, not an engine/RSS measurement.

## Proposed bounded dirty-intent format

The intent describes one owned operation and references any durable work set. It does not contain a copied seal or vectors/metadata/text:

```json
{"version":1,"kind":"srvdb_generation_intent","policy_version":1,"operation_id":"11111111-1111-4111-8111-111111111111","operation":"rebuild","identity":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","generation":"11111111-1111-4111-8111-111111111111","source_generation":"11111111-1111-4111-8111-111111111111","prior_seal_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","phase":"copying","dimensions":4096,"expected_stored_count":200000,"document":null,"scan":{"next_id":200000,"scan_end":200000,"batch_limit":200},"work_ref":{"operation_id":"11111111-1111-4111-8111-111111111111","item_count":200000,"next_ordinal":200000}}
```

Here the repeated source/target UUIDs are placeholders; a real rebuild requires distinct generations. Use canonical identity/generation/operation UUID/hash bounds as above. `operation` is a fixed enum (create/open/index/delete/rebuild/cleanup), and `phase` is a fixed operation-appropriate enum (for example created/opening/embedding/mutating/persisting/copying/validating/publishing/cleanup). No free-text error, path, whole job payload or unbounded history array enters the intent.

`prior_seal_sha256` is NULL for a new empty source or is a digest of the exact retained prior seal JSON bytes; it references the seal already stored in `seal_json`, rather than duplicating up to 64 KiB into 16 KiB. `source_generation`, `document`, `scan`, and `work_ref` can be NULL where inapplicable. A single-document operation may carry `document:{document_id:UUID,document_version:u32,content_hash:64-hex}`. For a rebuild, also validate the source's separately stored clean provenance and owner lease; the target's intent is not proof that the source is safe.

`scan_end` is a fenced snapshot <=200000, `next_id <= scan_end`, and ID scan is half-open `[next_id,scan_end)`. ID 0 is valid. `batch_limit` is 1..=200. A `work_ref` identifies durable operation rows by operation UUID, with <=200000 items per bounded generation work set and `next_ordinal <= item_count`; the cursor only advances with the corresponding durable acknowledged operation state. If the final operation spans larger aggregate work, shard bounded work sets and define that explicitly; do not hide an unbounded list in JSON.

The example is **671 bytes**, or **833 bytes** with the bounded single-document descriptor instead of NULL. This fits 16384 with generous room for fixed bounded fence/revision fields. One generation owner serializes one operation; do not accumulate pending operations into this blob.

Bulk-ID counterexamples calculated as compact JSON arrays: IDs 0..9999 require **48891 bytes**, IDs 190000..199999 require **70001 bytes**, and IDs 0..199999 require **1288891 bytes**, before any intent envelope. Neither a 16 KiB intent nor a 64 KiB intelligence job payload can generally store the required prior-ID work. Reference paged durable operation-item/staging-mapping rows indexed by operation/generation; keep every item recoverable until the relevant persist/pointer publication/cleanup acknowledgement succeeds.

## Full TextLocation mismatch and safe alternatives

The frozen 4A location is exactly `{page:Option<u32>,paragraph:u32,heading:Option<String>,start_char:u32,end_char:u32}`. Its heading is preserved from normalized Markdown/DOCX headings. Overall limits include input 32 MiB, parser output 16 MiB, extracted text 2000000 Unicode scalars, 50000 paragraphs and 10000 chunks. None implies a 16 KiB bound on one heading.

With the 6000-CJK heading, compact location JSON is **18071 bytes** and the brief's complete metadata with UUIDs, maximum document_version and hash is **18318 bytes**, above 16384. This is sufficient to disprove universal fit; no broader source investigation or parser restriction is needed.

### Alternative A: bounded vector metadata with canonical SQLite location reference

Propose an explicit new metadata codec `chunk_location_ref_v1` and `schema_version:2`, keeping the existing outer IDs/version/content_hash and replacing full location with a bounded reference/digest descriptor:

```json
{"schema_version":2,"document_id":"11111111-1111-4111-8111-111111111111","document_version":4294967295,"chunk_id":"11111111-1111-4111-8111-111111111111","content_hash":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","location":{"kind":"sqlite_chunk_location","codec":"text_location_v1","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}
```

This is **382 bytes** with maximal numeric document_version. Keep the 16 KiB metadata-value / 16 GiB physical envelope as proposed, subject to later actual resource fixtures. The outer document_id/document_version/chunk_id identifies the authoritative SQLite row; do not add paths or include the heading here.

At the checkpoint, fix `text_location_v1` canonical bytes as compact UTF-8 JSON of the full typed TextLocation in field order page,paragraph,heading,start_char,end_char, explicit NULL options, using the agreed serde_json encoding. Hash those exact canonical bytes. The author must confirm consistency with the reviewed canonical SQLite `location_json`, or canonicalize the same typed location identically when computing/comparing its digest; this proposal does not require rewriting or truncating frozen 4B rows. Do not normalize the heading a second time or confuse NULL with an empty heading.

Hydration must first match metadata schema/IDs/version/content_hash, active identity/generation and SQLite Ready mapping, then compare the location digest against the full authoritative SQLite location. Return that complete SQLite TextLocation and chunk text/title. A missing row, unsupported reference codec or digest mismatch is a local inconsistency: exclude the affected document and surface its repair state, rather than trusting vector metadata to supply a heading. A digest detects mismatch within the existing trusted-SQLite model; it is not independent authentication or a live ownership lease.

This changes the current Task 5 brief's full-location schema; it requires a recorded controller/author checkpoint ruling. No heading truncation, lower parser cap, record reduction, dimension reduction or new m4 column is inherent in this option. The existing canonical SQLite location remains authoritative.

### Alternative B: enlarge full-location metadata and file caps together

Retain the brief's full-location schema 1 and pin its exact compact raw-UTF-8 serde_json codec. For accepted normalized headings, forbidden controls are not introduced, permitted newline/tab JSON escapes use two bytes, and every other scalar contributes at most four UTF-8 bytes. The whole 2000000-scalar extracted cap therefore conservatively bounds the heading at **8000000 encoded bytes**. Maximal numeric fields plus outer IDs/hash add **348 bytes** in the chosen fixed codec. An **8 MiB = 8388608-byte per-value limit** fits that bound without truncation. A codec that ASCII-escapes all Unicode would need a different calculation; do not reuse the raw-UTF-8 bound for it.

That per-value cap is **512 times** the earlier 16 KiB proposal. At 200000 records, its logical maximum is **1677721600000 bytes** (approximately 1.526 TiB). Retaining the earlier **5.24288-times** physical-envelope factor raises the proposed metadata.db cap from 16 GiB to **8 TiB = 8796093022208 bytes**, with the previous 75% preventive watermark correspondingly becoming 6 TiB. This is a deliberately generous envelope based on all values reaching their cap; it is not measured normal output or a proven redb file-size bound. Parser output constraints may make that combination difficult to reach, but this report does not claim a cross-document/history storage theorem permitting a smaller cap.

Larger physical limits also permit much more hashing/I/O and eager metadata allocation. File size still cannot certify arbitrary redb tombstone/cardinality/page declarations or total RSS. This option is therefore a real storage/resource-policy change, not a small local constant bump; validate it with the actual trusted writer and preserve fail-local behavior for unsealed input. It cannot be presented as a low-memory guarantee.

## Additive schema and lifecycle implications

**No additive m4 change is necessary now for the bounded seal or reference-style dirty intent.** The accepted root version/byte budgets suffice. If bulk cleanup/rebuild must survive row deletion or pointer publication, Task 5 needs durable paged operation-item/staging-mapping storage with defined keys, fence, lifetime and batch reads. It may add such private tables additively at its checkpoint; the two reserved generation tables alone do not provide that item storage. Do not fit these rows into seal/intent JSON or assume job payloads solve their bounds.

Deletion cleanup must retain prior vector IDs/identity until actual source deletion/persist acknowledgement, even after document rows disappear. The seal table has no document FK, but its **identity FK cascades**: prohibit deletion of the parent identity while owned dirty operations/files remain, or use separately retained/non-cascading operation storage if identity deletion is part of the supported lifecycle. Never use cascade disappearance as evidence that index files were deleted.

Carry forward the managed-caps handoff's provenance lifecycle: create dirty intent before every mutating source operation, including writable/recovery **open**; retain the prior seal; derive a new clean seal only from a controlled bounded current writer after actual successful persistence/validation under the owning lock; publish seal/mappings/Ready state atomically. A dirty flag/operation cursor is durable desired work, not an actual process lease, engine-exit acknowledgement or safe-restart certificate. A hash alone does not trust foreign bytes. Dirty/unsealed restart must fail-local before arbitrary redb open, preserve original files/full text and require explicit repair as already proposed; live trusted retry still retains its real engine/lock and successful persistence requirement.

Verification performed: Python standard-library compact UTF-8 JSON size arithmetic only, plus report write/readback. No execution of parser/index/native implementation, no source audit, and no Task 4B review or performance/RSS claim.
