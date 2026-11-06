import { describe, expect, it } from "vitest";
import fixture from "../../tests/contracts/runtime-wire.json";
import {
  decodeSessionLaunch,
  decodeSessionRecord,
  decodeTranscriptUpdate,
  decodeRuntimeDiagnostic,
  decodeSessionEvent,
  decodeAudioInventory,
  decodeSubscriptionTicket,
  decodeTranscriptPage,
  decodeDiagnosticPage,
  decodeProviderCapabilities,
  decodeDriverStatus,
  decodeRuntimeOutcome,
  diagnosticLabel,
  WireDecodeError,
  decodeProviderConfigView,
  decodeLaunchValidation,
  decodeSessionStatus,
  decodeDirectionPhase,
  decodeRuntimePhase,
  decodeRuntimeFailureCode,
  decodeGapCause,
  decodePlaybackBoundary,
  decodeSampleFormat,
  decodeExportFormat,
  decodeAudioSessionState,
} from "./wire";

describe("shared Rust runtime contract", () => {
  it("decodes launches, nullable records, transcript identities and all mixed tags verbatim", () => {
    for (const value of [fixture.liveLaunch, fixture.callLaunch])
      expect(decodeSessionLaunch(value)).toEqual(value);
    for (const value of [
      fixture.callRecord,
      fixture.terminalRecord,
      fixture.legacyRecord,
    ])
      expect(decodeSessionRecord(value)).toEqual(value);
    for (const value of [
      fixture.reverseSourceInterim,
      fixture.reverseSourceFinal,
      fixture.reverseTranslationFinal,
      fixture.legacyRow,
    ])
      expect(decodeTranscriptUpdate(value)).toEqual(value);
    for (const value of fixture.diagnostics)
      expect(decodeRuntimeDiagnostic(value)).toEqual(value);
    for (const value of fixture.events)
      expect(decodeSessionEvent(value)).toEqual(value);
    for (const value of fixture.driverStatuses)
      expect(decodeDriverStatus(value)).toEqual(value);
    for (const value of fixture.outcomes)
      expect(decodeRuntimeOutcome(value)).toEqual(value);
    expect(decodeAudioInventory(fixture.inventory)).toEqual(fixture.inventory);
    expect(decodeSubscriptionTicket(fixture.ticket)).toEqual(fixture.ticket);
    expect(decodeTranscriptPage(fixture.transcriptPage)).toEqual(
      fixture.transcriptPage,
    );
    expect(decodeDiagnosticPage(fixture.diagnosticPage)).toEqual(
      fixture.diagnosticPage,
    );
    expect(decodeProviderCapabilities(fixture.capabilities)).toEqual(
      fixture.capabilities,
    );
  });
  it("pins every lifecycle/failure variant and public provider fields without credentials", () => {
    const variants = fixture.enumVariants;
    expect(variants.sessionStatus.map(decodeSessionStatus)).toEqual(
      variants.sessionStatus,
    );
    expect(variants.directionPhase.map(decodeDirectionPhase)).toEqual(
      variants.directionPhase,
    );
    expect(variants.runtimePhase.map(decodeRuntimePhase)).toEqual(
      variants.runtimePhase,
    );
    expect(variants.failureCode.map(decodeRuntimeFailureCode)).toEqual(
      variants.failureCode,
    );
    expect(variants.gapCause.map(decodeGapCause)).toEqual(variants.gapCause);
    expect(variants.playbackBoundary.map(decodePlaybackBoundary)).toEqual(
      variants.playbackBoundary,
    );
    expect(variants.sampleFormat.map(decodeSampleFormat)).toEqual(
      variants.sampleFormat,
    );
    expect(variants.exportFormat.map(decodeExportFormat)).toEqual(
      variants.exportFormat,
    );
    expect(variants.audioState.map(decodeAudioSessionState)).toEqual(
      variants.audioState,
    );
    expect(decodeProviderConfigView(fixture.providerConfigView)).toEqual(
      fixture.providerConfigView,
    );
    expect(decodeLaunchValidation(fixture.launchValidation)).toEqual(
      fixture.launchValidation,
    );
  });
  it.each([NaN, Infinity, 1.5, -1, Number.MAX_SAFE_INTEGER + 1])(
    "rejects unsafe sequence %s",
    (sequence) => {
      expect(() =>
        decodeTranscriptUpdate({ ...fixture.reverseSourceFinal, sequence }),
      ).toThrow(WireDecodeError);
    },
  );
  it("requires nullable fields without converting missing latency or time units", () => {
    const latency = fixture.diagnostics[5];
    expect(() =>
      decodeRuntimeDiagnostic({
        ...latency,
        payload: { latency: { response_to_enqueue_ms: 3 } },
      }),
    ).toThrow(WireDecodeError);
    expect(decodeSessionRecord(fixture.callRecord).started_at).toBe(1730000000);
    expect(
      decodeTranscriptUpdate(fixture.reverseSourceFinal).occurred_at_ms,
    ).toBe(1730000000123);
    expect(decodeRuntimeDiagnostic(latency).payload).toEqual({
      latency: {
        capture_to_send_ms: null,
        response_to_enqueue_ms: 3,
        enqueue_to_callback_ms: null,
        provider_processing_ms: null,
      },
    });
  });
  it("rejects bad UUIDs, numeric ranges, missing fields and malformed known variants", () => {
    expect(() =>
      decodeSessionRecord({ ...fixture.callRecord, id: "bad" }),
    ).toThrow(WireDecodeError);
    expect(() =>
      decodeSessionRecord({ ...fixture.callRecord, runtime: undefined }),
    ).toThrow(WireDecodeError);
    expect(() =>
      decodeSessionEvent({
        ...fixture.events[0],
        occurred_at: { secs_since_epoch: 1, nanos_since_epoch: 1000000000 },
      }),
    ).toThrow(WireDecodeError);
    expect(() =>
      decodeRuntimeDiagnostic({
        ...fixture.diagnostics[4],
        payload: {
          failure: {
            code: "provider",
            provider_code: 2147483648,
            log_id: null,
          },
        },
      }),
    ).toThrow(WireDecodeError);
    expect(() =>
      decodeAudioInventory({ ...fixture.inventory, driver: "ready" }),
    ).toThrow(WireDecodeError);
    expect(() => decodeTranscriptPage({ rows: [], next_cursor: -1 })).toThrow(
      WireDecodeError,
    );
    expect(() =>
      decodeTranscriptPage({
        rows: Array(201).fill(fixture.reverseSourceFinal),
        next_cursor: null,
      }),
    ).toThrow(WireDecodeError);
  });
  it("retains a bounded opaque unknown diagnostic and offers only a generic label", () => {
    const payload = { future_metric: { count: 12, note: "synthetic" } };
    const decoded = decodeRuntimeDiagnostic({
      ...fixture.diagnostics[0],
      payload,
    });
    expect(decoded.payload).toEqual(payload);
    expect(diagnosticLabel(decoded)).toBe("其他诊断");
    expect(() =>
      decodeRuntimeDiagnostic({
        ...fixture.diagnostics[0],
        payload: { future_metric: "x".repeat(8193) },
      }),
    ).toThrow(WireDecodeError);
    expect(() =>
      decodeRuntimeDiagnostic({
        ...fixture.diagnostics[0],
        payload: { future_metric: { number: Number.MAX_SAFE_INTEGER + 1 } },
      }),
    ).toThrow(WireDecodeError);
    const cyclic: Record<string, unknown> = {};
    cyclic.self = cyclic;
    expect(() =>
      decodeRuntimeDiagnostic({
        ...fixture.diagnostics[0],
        payload: { future_metric: cyclic },
      }),
    ).toThrow(WireDecodeError);
  });
});
