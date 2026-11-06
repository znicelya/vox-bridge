/** Actual Rust serde DTOs. Decoders preserve tags, nullable fields and native units.
 * Types are inferred from these schemas so declarations cannot drift from validation.
 */
export class WireDecodeError extends Error {
  constructor() {
    super("桌面数据格式无效，请重试。");
    this.name = "WireDecodeError";
  }
}
export type Decoder<T> = (value: unknown) => T;
type Decoded<D> = D extends Decoder<infer T> ? T : never;
const invalid = (): never => {
  throw new WireDecodeError();
};
const record = (value: unknown): Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : invalid();
function object<S extends Record<string, Decoder<unknown>>>(
  shape: S,
): Decoder<{ [K in keyof S]: Decoded<S[K]> }> {
  return (value) => {
    const input = record(value);
    const keys = Object.keys(shape);
    if (Object.keys(input).length !== keys.length) return invalid();
    const result: Record<string, unknown> = {};
    for (const key of keys) {
      if (!Object.prototype.hasOwnProperty.call(input, key)) return invalid();
      result[key] = shape[key](input[key]);
    }
    return result as { [K in keyof S]: Decoded<S[K]> };
  };
}
const string =
  (max = 262144): Decoder<string> =>
  (value) =>
    typeof value === "string" &&
    value.length <= max &&
    new TextEncoder().encode(value).length <= max
      ? value
      : invalid();
const text = string();
const boolean: Decoder<boolean> = (value) =>
  typeof value === "boolean" ? value : invalid();
const integer =
  (min: number, max = Number.MAX_SAFE_INTEGER): Decoder<number> =>
  (value) =>
    typeof value === "number" &&
    Number.isSafeInteger(value) &&
    value >= min &&
    value <= max
      ? value
      : invalid();
const u64 = integer(0);
const i64 = integer(Number.MIN_SAFE_INTEGER);
const u32 = integer(0, 4294967295);
const u16 = integer(0, 65535);
const i32 = integer(-2147483648, 2147483647);
const nanos = integer(0, 999999999);
const nullable =
  <T>(decode: Decoder<T>): Decoder<T | null> =>
  (value) =>
    value === null ? null : decode(value);
const array =
  <T>(decode: Decoder<T>, max = 1000): Decoder<T[]> =>
  (value) =>
    Array.isArray(value) && value.length <= max ? value.map(decode) : invalid();
const enumeration =
  <const T extends readonly string[]>(...values: T): Decoder<T[number]> =>
  (value) =>
    typeof value === "string" && values.includes(value) ? value : invalid();
function union<const T extends readonly Decoder<unknown>[]>(
  ...decoders: T
): Decoder<Decoded<T[number]>> {
  return (value) => {
    for (const decode of decoders) {
      try {
        return decode(value) as Decoded<T[number]>;
      } catch (error) {
        if (!(error instanceof WireDecodeError)) throw error;
      }
    }
    return invalid();
  };
}
export const decodeUuid: Decoder<string> = (value) =>
  typeof value === "string" &&
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(value)
    ? value
    : invalid();
const uuid = decodeUuid;
const logId: Decoder<string> = (value) =>
  typeof value === "string" && /^[A-Za-z0-9_-]{1,128}$/.test(value)
    ? value
    : invalid();
export const decodeSessionMode = enumeration("live", "call");
export const decodeSessionDirection = enumeration(
  "source_to_target",
  "target_to_source",
  "bidirectional",
);
export const decodeSessionStatus = enumeration(
  "created",
  "starting",
  "running",
  "stopping",
  "paused",
  "failed",
  "completed",
);
export const decodeDirectionPhase = enumeration(
  "starting",
  "running",
  "reconnecting",
  "paused",
  "stopped",
);
export const decodeRuntimePhase = enumeration(
  "starting",
  "running",
  "paused",
  "stopping",
  "stopped",
);
export const decodeRuntimeFailureCode = enumeration(
  "provider",
  "storage",
  "device_unavailable",
  "audio_backend_unavailable",
  "startup_timeout",
  "finish_timeout",
  "playback_timeout",
  "cleanup_timeout",
  "interrupted",
  "invalid_event",
);
export const decodeGapCause = enumeration(
  "provider_reconnect",
  "capture_overflow",
  "output_overflow",
  "device_loss",
  "manual_resume",
);
export const decodePlaybackBoundary = enumeration(
  "callback_consumed",
  "pulse_server_drained",
  "discarded",
);
export const decodeSampleFormat = enumeration("i16", "f32");
export const decodeExportFormat = enumeration("markdown", "json", "plain_text");
export const decodeAudioDirection = enumeration("Input", "Output");
export const decodeEndpointRole = enumeration(
  "Input",
  "PhysicalOutput",
  "M1Render",
  "M2Capture",
);
export const decodeAudioBackendKind = enumeration("Cpal", "Pulse");
export const decodeDriverStatus = union(
  enumeration("NeedsPrivileges", "NeedsRestart", "Ready"),
  object({ NeedsApproval: text }),
  object({ Failed: text }),
);
export const decodeLanguagePair = object({
  source: string(128),
  target: string(128),
});
export const decodeSessionConfig = object({
  mode: decodeSessionMode,
  direction: decodeSessionDirection,
  languages: decodeLanguagePair,
});
export const decodeDeviceSelection = union(
  object({ mode: enumeration("live"), input_id: text, m1_output_id: text }),
  object({
    mode: enumeration("call"),
    a_input_id: text,
    m2_input_id: text,
    m1_output_id: text,
    physical_output_id: text,
  }),
);
export const decodeSessionLaunch = object({
  config: decodeSessionConfig,
  provider_config_id: uuid,
  devices: decodeDeviceSelection,
});
export const decodeAudioEndpoint = object({
  id: text,
  display_name: text,
  direction: decodeAudioDirection,
  sample_rates: array(u32, 256),
  channels: u16,
  ready: boolean,
});
export const decodeRuntimeEndpoint = object({
  endpoint: decodeAudioEndpoint,
  role: decodeEndpointRole,
  backend: decodeAudioBackendKind,
});
export const decodeAudioInventory = object({
  generation: uuid,
  driver: decodeDriverStatus,
  endpoints: array(decodeRuntimeEndpoint),
});
export const decodeSessionContext = object({
  launch: decodeSessionLaunch,
  provider: string(128),
  provider_name: text,
  endpoints: array(decodeRuntimeEndpoint),
});
export const decodeDirectionSnapshot = object({
  direction: decodeSessionDirection,
  phase: decodeDirectionPhase,
  stream_epoch: u64,
  failure: nullable(decodeRuntimeFailureCode),
});
export const decodeRuntimeOutcome = union(
  object({ status: enumeration("completed") }),
  object({
    status: enumeration("failed"),
    codes: array(decodeRuntimeFailureCode),
  }),
);
export const decodeRuntimeSnapshot = object({
  phase: decodeRuntimePhase,
  directions: array(decodeDirectionSnapshot, 2),
  outcome: nullable(decodeRuntimeOutcome),
});
export const decodeSessionRecord = object({
  id: uuid,
  started_at: i64,
  ended_at: nullable(i64),
  mode: decodeSessionMode,
  direction: decodeSessionDirection,
  source_language: text,
  target_language: text,
  status: decodeSessionStatus,
  context: nullable(decodeSessionContext),
  runtime: nullable(decodeRuntimeSnapshot),
});
export const decodeNullableSessionRecord = nullable(decodeSessionRecord);
export const decodeSessionList = array(decodeSessionRecord, 200);
export const decodePcmSpec = object({
  sample_rate: u32,
  channels: u16,
  sample_format: decodeSampleFormat,
});
export const decodeLaunchValidation = object({
  context: decodeSessionContext,
  input_audio: decodePcmSpec,
  output_audio: decodePcmSpec,
});
export const decodeLlmConfig = object({
  model: text,
  resource_id: nullable(text),
});
export const decodeEmbeddingConfig = object({
  model: text,
  dimensions: nullable(u32),
});
export const decodeTavilyConfig = object({
  endpoint: text,
  search_depth: nullable(text),
});
export const decodePublicProviderConfig = object({
  id: uuid,
  provider: string(128),
  display_name: text,
  endpoint: text,
  llm: nullable(decodeLlmConfig),
  embedding: nullable(decodeEmbeddingConfig),
  tavily: nullable(decodeTavilyConfig),
  language_capabilities: array(text),
  secret_reference: nullable(text),
});
export const decodeProviderConfigView = object({
  config: decodePublicProviderConfig,
  credential_configured: boolean,
});
export const decodeProviderConfigList = array(decodeProviderConfigView);
export const decodeProviderCapabilities = object({
  language_pairs: array(decodeLanguagePair),
  input_audio: decodePcmSpec,
  output_audio: decodePcmSpec,
  native_chunk_duration: object({ secs: u64, nanos }),
  supports_tts: boolean,
  supports_bidirectional: boolean,
  ready: boolean,
  unavailable_reason: nullable(text),
});
export const decodeSubscriptionTicket = object({
  id: uuid,
  event_name: string(512),
});
export const decodeTranscriptKind = enumeration("source", "translation");
export const decodeTranscriptDetails = object({
  kind: decodeTranscriptKind,
  provider: string(128),
  provider_config_id: uuid,
  stream_epoch: u64,
  provider_event_type: nullable(text),
  provider_timestamp_ms: nullable(u64),
  sentence_id: nullable(text),
  local_correlation_id: nullable(u64),
  log_id: nullable(logId),
  start_ms: nullable(u64),
  end_ms: nullable(u64),
});
export const decodeTranscriptUpdate = object({
  id: uuid,
  session_id: uuid,
  direction: decodeSessionDirection,
  sequence: u64,
  occurred_at_ms: i64,
  language: text,
  text,
  is_final: boolean,
  details: nullable(decodeTranscriptDetails),
});
export const decodeCaptureStatistics = object({
  dropped_native_blocks: u64,
  dropped_normalized_frames: u64,
  finalized: boolean,
});
export const decodeNativePlaybackStatistics = object({
  sample_rate_hz: u32,
  generated_mono_samples: u64,
  callback_consumed_mono_samples: u64,
  discarded_mono_samples: u64,
});
export const decodePlaybackStatistics = object({
  input_sample_rate_hz: u32,
  accepted_for_processing_samples: u64,
  discarded_before_conversion_samples: u64,
  native: nullable(decodeNativePlaybackStatistics),
  finalized: boolean,
  tail_disposition_known: boolean,
});
const diagnosticVariants = {
  capture_accounting: object({ statistics: decodeCaptureStatistics }),
  playback_accounting: object({
    boundary: nullable(decodePlaybackBoundary),
    statistics: decodePlaybackStatistics,
  }),
  gap: object({ cause: decodeGapCause, dropped_frames: u64 }),
  transcript_overflow: object({ dropped_interims: u64 }),
  failure: object({
    code: decodeRuntimeFailureCode,
    provider_code: nullable(i32),
    log_id: nullable(logId),
  }),
  latency: object({
    capture_to_send_ms: nullable(u64),
    response_to_enqueue_ms: nullable(u64),
    enqueue_to_callback_ms: nullable(u64),
    provider_processing_ms: nullable(u64),
  }),
  playback: object({
    boundary: decodePlaybackBoundary,
    accepted_samples: u64,
    discarded_samples: u64,
  }),
};
type KnownDiagnosticPayload = {
  [K in keyof typeof diagnosticVariants]: {
    [P in K]: Decoded<(typeof diagnosticVariants)[K]>;
  };
}[keyof typeof diagnosticVariants];
const opaqueDiagnostic = Symbol("opaqueDiagnostic");
type SafeJson =
  | null
  | string
  | boolean
  | number
  | readonly SafeJson[]
  | { readonly [key: string]: SafeJson };
/** Future payloads are retained for compatibility, never interpreted or displayed. */
export type OpaqueDiagnosticPayload = Readonly<Record<string, SafeJson>> & {
  readonly [opaqueDiagnostic]: true;
};
export type DiagnosticPayload =
  | KnownDiagnosticPayload
  | OpaqueDiagnosticPayload;
function boundedOpaque(value: unknown): OpaqueDiagnosticPayload {
  let remaining = 8192;
  let nodes = 512;
  const spend = (size: number) => {
    remaining -= size;
    if (remaining < 0) invalid();
  };
  const visit = (input: unknown, depth: number): SafeJson => {
    if (--nodes < 0 || depth > 8) return invalid();
    spend(4);
    if (input === null || typeof input === "boolean") return input;
    if (typeof input === "number") return i64(input);
    if (typeof input === "string") {
      spend(new TextEncoder().encode(string(8192)(input)).length);
      return input;
    }
    if (Array.isArray(input)) {
      if (input.length > 512) return invalid();
      return Object.freeze(input.map((item) => visit(item, depth + 1)));
    }
    const source = record(input);
    const keys = Object.keys(source);
    if (keys.length > 128) return invalid();
    const result: Record<string, SafeJson> = {};
    for (const key of keys) {
      if (["__proto__", "constructor", "prototype"].includes(key))
        return invalid();
      spend(new TextEncoder().encode(string(128)(key)).length);
      result[key] = visit(source[key], depth + 1);
    }
    return Object.freeze(result);
  };
  const result = { ...record(visit(value, 0)) };
  Object.defineProperty(result, opaqueDiagnostic, { value: true });
  return Object.freeze(result) as OpaqueDiagnosticPayload;
}
export const decodeDiagnosticPayload: Decoder<DiagnosticPayload> = (value) => {
  const input = record(value);
  const keys = Object.keys(input);
  if (keys.length !== 1) return invalid();
  const key = keys[0];
  if (Object.prototype.hasOwnProperty.call(diagnosticVariants, key)) {
    const known = key as keyof typeof diagnosticVariants;
    return {
      [known]: diagnosticVariants[known](input[known]),
    } as KnownDiagnosticPayload;
  }
  return boundedOpaque(input);
};
export const decodeRuntimeDiagnostic = object({
  id: uuid,
  session_id: uuid,
  direction: nullable(decodeSessionDirection),
  stream_epoch: nullable(u64),
  occurred_at_ms: i64,
  payload: decodeDiagnosticPayload,
});
export const decodeTranscriptPage = object({
  rows: array(decodeTranscriptUpdate, 200),
  next_cursor: nullable(u64),
});
export const decodeDiagnosticPage = object({
  rows: array(decodeRuntimeDiagnostic, 200),
  next_cursor: nullable(u64),
});
export const decodeAudioSessionState = union(
  object({ state: enumeration("running") }),
  object({ state: enumeration("paused"), reason: text }),
);
const eventTime = object({ secs_since_epoch: u64, nanos_since_epoch: nanos });
export const decodeSessionEvent = union(
  object({
    type: enumeration("status"),
    event_id: uuid,
    session_id: uuid,
    occurred_at: eventTime,
    status: text,
  }),
  object({
    type: enumeration("audio_state"),
    event_id: uuid,
    session_id: uuid,
    occurred_at: eventTime,
    state: decodeAudioSessionState,
  }),
  object({ type: enumeration("transcript"), update: decodeTranscriptUpdate }),
  object({
    type: enumeration("diagnostic"),
    diagnostic: decodeRuntimeDiagnostic,
  }),
  object({
    type: enumeration("resync_needed"),
    session_id: uuid,
    missed_events: u64,
  }),
);
export function diagnosticLabel(diagnostic: RuntimeDiagnostic): string {
  if (opaqueDiagnostic in diagnostic.payload) return "其他诊断";
  const labels: Record<keyof typeof diagnosticVariants, string> = {
    capture_accounting: "采集统计",
    playback_accounting: "播放统计",
    gap: "音频间断",
    transcript_overflow: "临时文本丢失",
    failure: "运行错误",
    latency: "延迟",
    playback: "播放边界",
  };
  return labels[Object.keys(diagnostic.payload)[0] as keyof typeof labels];
}
export type SessionMode = Decoded<typeof decodeSessionMode>;
export type SessionDirection = Decoded<typeof decodeSessionDirection>;
export type SessionStatus = Decoded<typeof decodeSessionStatus>;
export type DirectionPhase = Decoded<typeof decodeDirectionPhase>;
export type RuntimePhase = Decoded<typeof decodeRuntimePhase>;
export type RuntimeFailureCode = Decoded<typeof decodeRuntimeFailureCode>;
export type GapCause = Decoded<typeof decodeGapCause>;
export type PlaybackBoundary = Decoded<typeof decodePlaybackBoundary>;
export type SampleFormat = Decoded<typeof decodeSampleFormat>;
export type ExportFormat = Decoded<typeof decodeExportFormat>;
export type DriverStatus = Decoded<typeof decodeDriverStatus>;
export type AudioDirection = Decoded<typeof decodeAudioDirection>;
export type EndpointRole = Decoded<typeof decodeEndpointRole>;
export type AudioBackendKind = Decoded<typeof decodeAudioBackendKind>;
export type LanguagePair = Decoded<typeof decodeLanguagePair>;
export type SessionConfig = Decoded<typeof decodeSessionConfig>;
export type DeviceSelection = Decoded<typeof decodeDeviceSelection>;
export type SessionLaunch = Decoded<typeof decodeSessionLaunch>;
export type AudioEndpoint = Decoded<typeof decodeAudioEndpoint>;
export type RuntimeEndpoint = Decoded<typeof decodeRuntimeEndpoint>;
export type AudioInventory = Decoded<typeof decodeAudioInventory>;
export type SessionContext = Decoded<typeof decodeSessionContext>;
export type DirectionSnapshot = Decoded<typeof decodeDirectionSnapshot>;
export type RuntimeOutcome = Decoded<typeof decodeRuntimeOutcome>;
export type RuntimeSnapshot = Decoded<typeof decodeRuntimeSnapshot>;
export type SessionRecord = Decoded<typeof decodeSessionRecord>;
export type PcmSpec = Decoded<typeof decodePcmSpec>;
export type LaunchValidation = Decoded<typeof decodeLaunchValidation>;
export type LlmConfig = Decoded<typeof decodeLlmConfig>;
export type EmbeddingConfig = Decoded<typeof decodeEmbeddingConfig>;
export type TavilyConfig = Decoded<typeof decodeTavilyConfig>;
export type PublicProviderConfig = Decoded<typeof decodePublicProviderConfig>;
export type ProviderConfig = Omit<PublicProviderConfig, "display_name"> & {
  display_name?: string | null;
};
export type ProviderConfigView = Decoded<typeof decodeProviderConfigView>;
export type CredentialChange =
  | { action: "keep" }
  | { action: "clear" }
  | { action: "replace"; api_key: string };
export type SaveProviderRequest = {
  config: ProviderConfig;
  credential: CredentialChange;
};
export type ProviderCapabilities = Decoded<typeof decodeProviderCapabilities>;
export type SubscriptionTicket = Decoded<typeof decodeSubscriptionTicket>;
export type TranscriptKind = Decoded<typeof decodeTranscriptKind>;
export type TranscriptDetails = Decoded<typeof decodeTranscriptDetails>;
export type TranscriptUpdate = Decoded<typeof decodeTranscriptUpdate>;
export type CaptureStatistics = Decoded<typeof decodeCaptureStatistics>;
export type NativePlaybackStatistics = Decoded<
  typeof decodeNativePlaybackStatistics
>;
export type PlaybackStatistics = Decoded<typeof decodePlaybackStatistics>;
export type RuntimeDiagnostic = Decoded<typeof decodeRuntimeDiagnostic>;
export type TranscriptPage = Decoded<typeof decodeTranscriptPage>;
export type DiagnosticPage = Decoded<typeof decodeDiagnosticPage>;
export type AudioSessionState = Decoded<typeof decodeAudioSessionState>;
export type SessionEvent = Decoded<typeof decodeSessionEvent>;
