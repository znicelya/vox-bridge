/** Safe intelligence DTOs; service credentials have no frontend output schema. */
import { decodeUuid, wireSchema, WireDecodeError } from './wire';
const { object, string, boolean, integer, nullable, array, enumeration, union } = wireSchema;
const uuid = decodeUuid;
const i64 = integer(Number.MIN_SAFE_INTEGER);
const u64 = integer(0);
const u32 = integer(0, 4294967295);
export const decodeServiceKind = enumeration('llm', 'embedding', 'tavily');
export const decodeRecordOrigin = enumeration('native', 'legacy');
export const decodeJobKind = enumeration('import', 'reindex', 'delete_document', 'summary', 'research', 'research_overview');
export const decodeJobStatus = enumeration('queued', 'running', 'succeeded', 'failed', 'cancelled', 'interrupted');
export const decodeIntelligenceErrorCode = enumeration(
  'configuration', 'busy', 'cancelled', 'interrupted', 'timeout', 'network', 'authentication',
  'rate_limited', 'response_too_large', 'invalid_response', 'context_too_large',
  'unsupported_document', 'parse_failed', 'resource_limit', 'index_repair_required', 'no_final_text', 'storage',
);
export const decodeModelProfile = object({
  id: uuid, kind: decodeServiceKind, name: string(320), base_url: string(2048), model: nullable(string(800)),
  timeout_ms: integer(1000, 120000), max_context_tokens: nullable(integer(2048, 131072)),
  max_output_tokens: nullable(integer(128, 16384)), dimensions: nullable(integer(128, 4096)),
});
export const decodeConnectionProbe = object({ checked_at_ms: i64, ok: boolean, latency_ms: nullable(u64), error: nullable(decodeIntelligenceErrorCode) });
export const decodeModelProfileView = object({ profile: decodeModelProfile, credential_configured: boolean, last_probe: nullable(decodeConnectionProbe) });
export const decodeModelProfileViews = array(decodeModelProfileView, 128);
export const decodeIntelligenceSettings = object({ summary_profile_id: nullable(uuid), embedding_profile_id: nullable(uuid), tavily_profile_id: nullable(uuid), automatic_summary: boolean });
export const decodeManagedDataLocation = object({ data_dir: string(32768) });
export const decodeJobView = object({
  id: uuid, kind: decodeJobKind, status: decodeJobStatus, session_id: nullable(uuid), object_id: nullable(uuid),
  completed_units: u32, total_units: nullable(u32), error: nullable(decodeIntelligenceErrorCode), created_at_ms: i64, updated_at_ms: i64,
});
export const decodeJobPage = object({ rows: array(decodeJobView, 200), next_cursor: nullable(integer(0)) });
export const decodeIntelligenceEvent = union(
  object({ kind: enumeration('job_changed'), job: decodeJobView }),
  object({ kind: enumeration('resync_needed'), missed_events: u64 }),
);
export type ModelProfile = ReturnType<typeof decodeModelProfile>;
export type ModelProfileView = ReturnType<typeof decodeModelProfileView>;
export type IntelligenceSettings = ReturnType<typeof decodeIntelligenceSettings>;
export type ManagedDataLocation = ReturnType<typeof decodeManagedDataLocation>;
export type JobView = ReturnType<typeof decodeJobView>;
export type JobPage = ReturnType<typeof decodeJobPage>;
export type ConnectionProbe = ReturnType<typeof decodeConnectionProbe>;
export type IntelligenceEvent = ReturnType<typeof decodeIntelligenceEvent>;
export type IntelligenceErrorCode = ReturnType<typeof decodeIntelligenceErrorCode>;
/** Only the fixed subcode selects UI behavior; prose is never parsed. */
export function intelligenceErrorSubcode(value: unknown): IntelligenceErrorCode | null {
  if (typeof value !== 'string') return null;
  const match = /^\[INTELLIGENCE_ERROR\]\[([a-z_]+)\] /.exec(value);
  if (!match) return null;
  try { return decodeIntelligenceErrorCode(match[1]); }
  catch (error) { if (error instanceof WireDecodeError) return null; throw error; }
}
