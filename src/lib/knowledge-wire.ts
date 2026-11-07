/** Actual document serde shapes. Legacy nulls remain null; no fallback labels are persisted. */
import { decodeUuid, wireSchema, WireDecodeError, type Decoder } from "./wire";
import {
  decodeRecordOrigin,
  decodeIntelligenceErrorCode,
} from "./intelligence-wire";
const { object, string, integer, nullable, array, enumeration } = wireSchema;
const u32 = integer(0, 4294967295);
const i64 = integer(Number.MIN_SAFE_INTEGER);
const field = string(32768);
export const decodeDocumentKind = enumeration(
  "pdf",
  "docx",
  "text",
  "markdown",
  "unknown",
);
export const decodeDocumentStatus = enumeration(
  "parsing",
  "parsed",
  "indexing",
  "ready",
  "repair_required",
  "failed",
  "deleting",
);
export const decodeDocumentSummary = object({
  id: decodeUuid,
  title: nullable(field),
  kind: decodeDocumentKind,
  version: integer(1, 4294967295),
  origin: decodeRecordOrigin,
  sha256: field,
  status: decodeDocumentStatus,
  chunk_count: u32,
  created_at_ms: i64,
  updated_at_ms: nullable(i64),
  error: nullable(decodeIntelligenceErrorCode),
});
export const decodeDocumentDetail = object({
  summary: decodeDocumentSummary,
  original_path: nullable(field),
  managed_path: nullable(field),
  legacy_uri: nullable(field),
  index_identity: nullable(field),
});
export const decodeDocumentPage = object({
  rows: array(decodeDocumentSummary, 200),
  next_cursor: nullable(integer(0)),
});
export const decodeTextLocation = object({
  page: nullable(u32),
  paragraph: u32,
  heading: nullable(string(16 * 1024 * 1024)),
  start_char: u32,
  end_char: u32,
});
export const decodeTextBlock = object({
  text: string(8 * 1024 * 1024),
  location: decodeTextLocation,
});
export const decodeImportDocumentArgs = object({ path: field });
export const decodeReplaceDocumentArgs = object({
  documentId: decodeUuid,
  path: field,
});
export const decodeListDocumentsArgs = object({
  after: nullable(integer(0)),
  limit: integer(1, 200),
});
export const decodeGetDocumentArgs = object({ documentId: decodeUuid });
export type DocumentKind = ReturnType<typeof decodeDocumentKind>;
export type DocumentStatus = ReturnType<typeof decodeDocumentStatus>;
export type DocumentSummary = ReturnType<typeof decodeDocumentSummary>;
export type DocumentDetail = ReturnType<typeof decodeDocumentDetail>;
export type DocumentPage = ReturnType<typeof decodeDocumentPage>;
export type TextLocation = ReturnType<typeof decodeTextLocation>;
export type TextBlock = ReturnType<typeof decodeTextBlock>;

const scalarText =
  (max: number, bytes: number, min = 0): Decoder<string> =>
  (value) => {
    const text = string(bytes)(value);
    let count = 0;
    for (const _scalar of text) if (++count > max) throw new WireDecodeError();
    if (count < min) throw new WireDecodeError();
    return text;
  };
const identity: Decoder<string> = (value) => {
  if (typeof value !== "string" || !/^[0-9a-f]{64}$/.test(value))
    throw new WireDecodeError();
  return value;
};
const score: Decoder<number> = (value) => {
  if (typeof value !== "number" || !Number.isFinite(value))
    throw new WireDecodeError();
  return value;
};
export const decodeKnowledgeSearch = object({
  query: scalarText(4096, 16384, 1),
  selected_segment_ids: array(decodeUuid, 200),
  session_id: nullable(decodeUuid),
  limit: integer(1, 20),
});
export const decodeKnowledgeHit = object({
  document_id: decodeUuid,
  document_version: integer(1, 4294967295),
  chunk_id: decodeUuid,
  title: field,
  location: decodeTextLocation,
  text: scalarText(2400, 9600),
  score,
});
const results = object({
  hits: array(decodeKnowledgeHit, 20),
  identity: nullable(identity),
  unavailable_documents: u32,
});
export const decodeKnowledgeResults: Decoder<ReturnType<typeof results>> = (
  value,
) => {
  const decoded = results(value);
  // Count escaped UTF-8 fields, plus generous fixed overhead per hit/envelope.
  // Floating-point encoding differs across Rust/JS, so reserve rather than compare it.
  const encoder = new TextEncoder();
  let bytes = 512;
  for (const hit of decoded.hits) {
    bytes += 1024;
    for (const field of [hit.title, hit.text, hit.location.heading]) {
      if (field !== null) bytes += encoder.encode(JSON.stringify(field)).length;
    }
    if (bytes > 32 * 1024 * 1024) throw new WireDecodeError();
  }
  return decoded;
};
export type KnowledgeSearch = ReturnType<typeof decodeKnowledgeSearch>;
export type KnowledgeHit = ReturnType<typeof decodeKnowledgeHit>;
export type KnowledgeResults = ReturnType<typeof decodeKnowledgeResults>;

/** Exact-selection context for the explicit search submission boundary. */
export type KnowledgeSelection = {
  sessionId: string;
  segmentIds: string[];
  text: string;
};
export function knowledgeSearchRequest(
  query: string,
  selection: KnowledgeSelection | null,
  limit = 5,
): KnowledgeSearch {
  return decodeKnowledgeSearch({
    query,
    selected_segment_ids: selection?.text === query ? selection.segmentIds : [],
    session_id: selection?.sessionId ?? null,
    limit,
  });
}
