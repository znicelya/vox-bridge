import { StrictMode, useEffect } from "react";
import { render, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import fixture from "../../../tests/contracts/runtime-wire.json";
import {
  decodeSessionRecord,
  decodeTranscriptUpdate,
  decodeRuntimeDiagnostic,
  diagnosticLabel,
} from "../../lib/wire";
import { runtimeDesktopFixture } from "../../test/desktop-fixture";
import { createSessionStream } from "./controller";
import type { SessionStreamController } from "./types";
const wire = {
  callRecord: decodeSessionRecord(fixture.callRecord),
  reverseSourceFinal: decodeTranscriptUpdate(fixture.reverseSourceFinal),
};
const id = (n: number) =>
  `00000000-0000-0000-0000-${n.toString().padStart(12, "0")}`;
const streams: SessionStreamController[] = [];
const setup = () => {
  const port = runtimeDesktopFixture(wire);
  const stream = createSessionStream(port);
  streams.push(stream);
  return { port, stream };
};
afterEach(async () => {
  for (const stream of streams) await stream.stop();
  streams.length = 0;
  vi.useRealTimers();
});
it("recovers a final emitted before listener attachment and releases tickets", async () => {
  const { port, stream } = setup();
  port.beforeSubscribeReply(() => {
    port.persistFinal(wire.reverseSourceFinal);
    port.emit({ type: "transcript", update: wire.reverseSourceFinal });
  });
  await stream.start(wire.callRecord.id);
  expect(stream.snapshot().finals).toEqual([wire.reverseSourceFinal]);
  expect(port.calls.slice(0, 2).map((call) => call.command)).toEqual([
    "subscribe_session",
    "get_session",
  ]);
  expect(
    port.calls.find((call) => call.command === "get_session_transcript")?.args,
  ).toEqual({ sessionId: wire.callRecord.id, after: null, limit: 200 });
  await stream.stop();
  expect(port.liveListenerCount()).toBe(0);
  expect(port.activeTicketCount()).toBe(0);
  expect(port.calls.some((call) => call.command === "stop_session")).toBe(
    false,
  );
});
it("deduplicates the reread final page using the last non-null table-specific cursor", async () => {
  const { port, stream } = setup();
  for (let n = 0; n < 401; n++)
    port.persistFinal({
      ...wire.reverseSourceFinal,
      id: id(n + 100),
      sequence: 401 - n,
    });
  for (let n = 0; n < 201; n++)
    port.persistDiagnostic({
      ...decodeRuntimeDiagnostic(fixture.diagnostics[0]),
      id: id(n + 1000),
    });
  await stream.start(wire.callRecord.id);
  port.persistFinal({ ...wire.reverseSourceFinal, id: id(900), sequence: 1 });
  port.emit({
    type: "resync_needed",
    session_id: wire.callRecord.id,
    missed_events: 1,
  });
  await waitFor(() => expect(stream.snapshot().finals).toHaveLength(402));
  expect(
    port.calls
      .filter((call) => call.command === "get_session_transcript")
      .map((call) => call.args?.after),
  ).toEqual([null, 200, 400, 400]);
  expect(
    port.calls
      .filter((call) => call.command === "get_session_diagnostics")
      .map((call) => call.args?.after),
  ).toEqual([null, 200, 200]);
  expect(stream.snapshot().diagnostics).toHaveLength(200);
  expect(stream.snapshot().finals[0].sequence).toBe(401);
});
it("restarts a snapshot on resync during an in-flight page and retains buffered finals", async () => {
  const { port, stream } = setup();
  const page = port.deferNext("get_session_transcript");
  const started = stream.start(wire.callRecord.id);
  await page.requested;
  port.persistFinal(wire.reverseSourceFinal);
  port.emit({
    type: "resync_needed",
    session_id: wire.callRecord.id,
    missed_events: 2,
  });
  port.emit({
    type: "transcript",
    update: decodeTranscriptUpdate(fixture.reverseTranslationFinal),
  });
  page.release();
  await started;
  expect(stream.snapshot().finals.map((row) => row.id)).toEqual([
    wire.reverseSourceFinal.id,
    fixture.reverseTranslationFinal.id,
  ]);
  expect(
    port.calls.filter((call) => call.command === "get_session_transcript"),
  ).toHaveLength(2);
});
it("bounds overflow recovery, clears interims, and recovers all durable finals", async () => {
  const { port, stream } = setup();
  const page = port.deferNext("get_session_transcript");
  const started = stream.start(wire.callRecord.id);
  await page.requested;
  for (let n = 0; n < 520; n++) {
    const row = { ...wire.reverseSourceFinal, id: id(n + 100) };
    port.persistFinal(row);
    port.emit({ type: "transcript", update: row });
  }
  page.release();
  await started;
  expect(stream.snapshot().finals).toHaveLength(500);
  expect(new Set(stream.snapshot().finals.map((row) => row.id)).size).toBe(500);
  expect(stream.snapshot().finals[499].id).toBe(id(619));
  expect(stream.snapshot().hasEarlierRows).toBe(true);
  expect(stream.snapshot().interims).toEqual([]);
});
it("continues after unknown diagnostics and malformed data with local errors", async () => {
  const { port, stream } = setup();
  await stream.start(wire.callRecord.id);
  port.emit({
    type: "diagnostic",
    diagnostic: {
      ...fixture.diagnostics[0],
      payload: { future_metric: { note: "synthetic" } },
    },
  });
  port.emit({
    type: "transcript",
    update: {
      ...fixture.reverseSourceFinal,
      sequence: Number.MAX_SAFE_INTEGER + 1,
    },
  });
  port.emit({ type: "transcript", update: fixture.reverseSourceFinal });
  expect(stream.snapshot().finals).toEqual([wire.reverseSourceFinal]);
  expect(diagnosticLabel(stream.snapshot().diagnostics[0])).toBe("其他诊断");
  expect(stream.snapshot().error).toBeTruthy();
  expect(stream.snapshot().error).not.toContain("synthetic");
});
it("ignores other sessions in events and rejects mismatched snapshot identities", async () => {
  const { port, stream } = setup();
  await stream.start(wire.callRecord.id);
  port.emit({
    type: "transcript",
    update: { ...fixture.reverseSourceFinal, session_id: id(999) },
  });
  expect(stream.snapshot().finals).toEqual([]);
  port.replyNext("get_session", { ...fixture.callRecord, id: id(999) });
  port.emit({
    type: "resync_needed",
    session_id: wire.callRecord.id,
    missed_events: 1,
  });
  await waitFor(() => expect(stream.snapshot().error).toBeTruthy());
  expect(stream.snapshot().session?.id).toBe(wire.callRecord.id);
});
it("cleans a late subscribe ticket after stop or session switch", async () => {
  const { port, stream } = setup();
  const gate = port.deferNext("subscribe_session");
  const oldStart = stream.start(wire.callRecord.id);
  await gate.requested;
  await stream.stop();
  const next = { ...wire.callRecord, id: id(777) };
  port.setSession(next);
  await stream.start(next.id);
  gate.release();
  await oldStart;
  await waitFor(() => expect(port.activeTicketCount()).toBe(1));
  expect(stream.snapshot().session?.id).toBe(next.id);
  expect(port.activeTicketCount()).toBe(1);
  expect(port.liveListenerCount()).toBe(1);
  await stream.stop();
  expect(port.activeTicketCount()).toBe(0);
});
it("cleans a late listener and ignores a stale page completion", async () => {
  const { port, stream } = setup();
  const listener = port.deferNext("listen");
  const oldStart = stream.start(wire.callRecord.id);
  await listener.requested;
  await stream.stop();
  listener.release();
  await oldStart;
  await waitFor(() => expect(port.liveListenerCount()).toBe(0));
  expect(port.liveListenerCount()).toBe(0);
  expect(port.activeTicketCount()).toBe(0);
  const page = port.deferNext("get_session_transcript");
  const first = stream.start(wire.callRecord.id);
  await page.requested;
  const next = { ...wire.callRecord, id: id(777) };
  port.setSession(next);
  await stream.start(next.id);
  page.release();
  await first;
  await waitFor(() => expect(port.pendingNativeCount()).toBe(0));
  expect(stream.snapshot().session?.id).toBe(next.id);
  expect(stream.snapshot().finals).toEqual([]);
});
it.each(["subscribe_session", "listen", "get_session_transcript"])(
  "handles %s rejection without leaking listeners, tickets, or private error text",
  async (command) => {
    const { port, stream } = setup();
    port.rejectNext(command, "[INTERNAL_ERROR] secret detail");
    await stream.start(wire.callRecord.id);
    expect(stream.snapshot().error).toBeTruthy();
    expect(stream.snapshot().error).not.toContain("secret");
    await stream.stop();
    expect(port.activeTicketCount()).toBe(0);
    expect(port.liveListenerCount()).toBe(0);
  },
);
it("survives StrictMode effect mount and unmount", async () => {
  const { port, stream } = setup();
  function Mounted() {
    useEffect(() => {
      void stream.start(wire.callRecord.id);
      return () => {
        void stream.stop();
      };
    }, []);
    return null;
  }
  const rendered = render(
    <StrictMode>
      <Mounted />
    </StrictMode>,
  );
  await waitFor(() =>
    expect(stream.snapshot().session?.id).toBe(wire.callRecord.id),
  );
  expect(port.activeTicketCount()).toBe(1);
  expect(port.liveListenerCount()).toBe(1);
  rendered.unmount();
  await waitFor(() => expect(port.activeTicketCount()).toBe(0));
  expect(port.liveListenerCount()).toBe(0);
});
it("refreshes directional state and catches terminal status without a final event, without overlapping polls", async () => {
  vi.useFakeTimers();
  const { port, stream } = setup();
  await stream.start(wire.callRecord.id);
  const gate = port.deferNext("get_session");
  await vi.advanceTimersByTimeAsync(2000);
  await gate.requested;
  await vi.advanceTimersByTimeAsync(6000);
  expect(
    port.calls.filter((call) => call.command === "get_session"),
  ).toHaveLength(2);
  gate.release();
  await vi.advanceTimersByTimeAsync(0);
  port.setSession({
    ...wire.callRecord,
    runtime: {
      ...wire.callRecord.runtime!,
      directions: [
        {
          direction: "target_to_source",
          phase: "paused",
          stream_epoch: 3,
          failure: "device_unavailable",
        },
      ],
    },
  });
  await vi.advanceTimersByTimeAsync(2000);
  expect(stream.snapshot().session?.runtime?.directions[0].phase).toBe(
    "paused",
  );
  port.persistFinal(wire.reverseSourceFinal);
  port.setSession(decodeSessionRecord(fixture.terminalRecord));
  await vi.advanceTimersByTimeAsync(2000);
  expect(stream.snapshot().session?.status).toBe("failed");
  expect(stream.snapshot().session?.runtime).toBeNull();
  expect(stream.snapshot().finals).toEqual([wire.reverseSourceFinal]);
  const count = port.calls.length;
  await vi.advanceTimersByTimeAsync(6000);
  expect(port.calls).toHaveLength(count);
});
it("does not reinsert old buffered finals after snapshot display truncation", async () => {
  const { port, stream } = setup();
  const record = port.deferNext("get_session");
  const started = stream.start(wire.callRecord.id);
  await record.requested;
  for (let n = 0; n < 600; n++) {
    const row = { ...wire.reverseSourceFinal, id: id(n + 100) };
    port.persistFinal(row);
    if (n < 512) port.emit({ type: "transcript", update: row });
  }
  record.release();
  await started;
  expect(stream.snapshot().finals).toHaveLength(500);
  expect(stream.snapshot().finals[0].id).toBe(id(200));
  expect(stream.snapshot().finals[499].id).toBe(id(699));
});
it("retries a transient unsubscribe rejection and retains persistent failures for later cleanup", async () => {
  const { port, stream } = setup();
  await stream.start(wire.callRecord.id);
  port.rejectNext("unsubscribe_session");
  await stream.stop();
  expect(port.activeTicketCount()).toBe(0);
  expect(port.liveListenerCount()).toBe(0);
  await stream.start(wire.callRecord.id);
  port.rejectNext("unsubscribe_session");
  port.rejectNext("unsubscribe_session");
  await stream.stop();
  expect(stream.snapshot().error).toBeTruthy();
  expect(port.activeTicketCount()).toBe(1);
  await stream.stop();
  expect(port.activeTicketCount()).toBe(0);
});
it("does not let late stop cleanup overwrite the next active session", async () => {
  const { port, stream } = setup();
  await stream.start(wire.callRecord.id);
  const unsubscribe = port.deferNext("unsubscribe_session");
  const stopped = stream.stop();
  await unsubscribe.requested;
  await stream.start(wire.callRecord.id);
  port.emit({ type: "transcript", update: fixture.reverseSourceInterim });
  unsubscribe.release();
  await stopped;
  expect(stream.snapshot().interims).toHaveLength(1);
});
it("cleans a ticket that arrives after the bounded subscribe timeout", async () => {
  vi.useFakeTimers();
  const { port, stream } = setup();
  const gate = port.deferNext("subscribe_session");
  const started = stream.start(wire.callRecord.id);
  await gate.requested;
  await vi.advanceTimersByTimeAsync(10000);
  await started;
  expect(stream.snapshot().recovering).toBe(false);
  expect(stream.snapshot().error).toContain("超时");
  gate.release();
  await vi.advanceTimersByTimeAsync(0);
  expect(port.activeTicketCount()).toBe(0);
  expect(port.liveListenerCount()).toBe(0);
});
it("rejects malformed/nonadvancing pages and leaves previously confirmed finals intact", async () => {
  const { port, stream } = setup();
  port.persistFinal(wire.reverseSourceFinal);
  await stream.start(wire.callRecord.id);
  port.replyNext("get_session_transcript", { rows: [], next_cursor: 0 });
  port.emit({
    type: "resync_needed",
    session_id: wire.callRecord.id,
    missed_events: 1,
  });
  await waitFor(() => expect(stream.snapshot().error).toBeTruthy());
  expect(stream.snapshot().finals).toEqual([wire.reverseSourceFinal]);
});
it("keeps one native status request outstanding even after its local deadline expires", async () => {
  vi.useFakeTimers();
  const { port, stream } = setup();
  await stream.start(wire.callRecord.id);
  const gate = port.deferNext("get_session");
  await vi.advanceTimersByTimeAsync(2000);
  await gate.requested;
  await vi.advanceTimersByTimeAsync(14000);
  expect(stream.snapshot().error).toContain("超时");
  expect(
    port.calls.filter((call) => call.command === "get_session"),
  ).toHaveLength(2);
  gate.release();
  await vi.advanceTimersByTimeAsync(2000);
  expect(
    port.calls.filter((call) => call.command === "get_session"),
  ).toHaveLength(3);
});
it("retains direction epochs after peer finals evict every higher-epoch display row", async () => {
  const { port, stream } = setup();
  const record = port.deferNext("get_session");
  const started = stream.start(wire.callRecord.id);
  await record.requested;
  const old = { ...wire.reverseSourceFinal, id: id(900), is_final: false };
  const peer = { ...old, id: id(901), direction: "source_to_target" as const };
  port.emit({ type: "transcript", update: old });
  port.emit({ type: "transcript", update: peer });
  const newer = {
    ...wire.reverseSourceFinal,
    details: { ...wire.reverseSourceFinal.details!, stream_epoch: 3 },
  };
  port.persistFinal(newer);
  port.emit({ type: "transcript", update: newer });
  for (let n = 0; n < 500; n++)
    port.persistFinal({
      ...wire.reverseSourceFinal,
      id: id(100 + n),
      direction: "source_to_target",
    });
  record.release();
  await started;
  expect(stream.snapshot().finals).toHaveLength(500);
  expect(stream.snapshot().finals[0].id).toBe(id(100));
  expect(stream.snapshot().interims).toEqual([peer]);
  await stream.stop();
  const next = { ...wire.callRecord, id: id(888), runtime: null };
  port.setSession(next);
  await stream.start(next.id);
  const nextInterim = { ...old, session_id: next.id };
  port.emit({ type: "transcript", update: nextInterim }, next.id);
  expect(stream.snapshot().interims).toEqual([nextInterim]);
});
it("uses runtime snapshot epochs before any row exists and after a status refresh", async () => {
  vi.useFakeTimers();
  const { port, stream } = setup();
  const record = {
    ...wire.callRecord,
    runtime: {
      ...wire.callRecord.runtime!,
      directions: [
        {
          direction: "target_to_source" as const,
          phase: "running" as const,
          stream_epoch: 3,
          failure: null,
        },
      ],
    },
  };
  port.setSession(record);
  await stream.start(record.id);
  port.emit({ type: "transcript", update: fixture.reverseSourceInterim });
  expect(stream.snapshot().interims).toEqual([]);
  const current = {
    ...decodeTranscriptUpdate(fixture.reverseSourceInterim),
    details: { ...wire.reverseSourceFinal.details!, stream_epoch: 3 },
  };
  port.emit({ type: "transcript", update: current });
  expect(stream.snapshot().interims).toEqual([current]);
  port.setSession({
    ...record,
    runtime: {
      ...record.runtime,
      directions: [{ ...record.runtime.directions[0], stream_epoch: 4 }],
    },
  });
  await vi.advanceTimersByTimeAsync(2000);
  expect(stream.snapshot().interims).toEqual([]);
});
it("discards pre-resync buffered interims and preserves post-resync interims", async () => {
  const { port, stream } = setup();
  const page = port.deferNext("get_session_transcript");
  const started = stream.start(wire.callRecord.id);
  await page.requested;
  port.emit({ type: "transcript", update: fixture.reverseSourceInterim });
  port.emit({
    type: "resync_needed",
    session_id: wire.callRecord.id,
    missed_events: 1,
  });
  const later = {
    ...decodeTranscriptUpdate(fixture.reverseSourceInterim),
    id: id(902),
  };
  port.emit({ type: "transcript", update: later });
  page.release();
  await started;
  expect(stream.snapshot().interims).toEqual([later]);
});
it("reuses one subscriber token throughout a controller lifetime", async () => {
  const { port, stream } = setup();
  await stream.start(wire.callRecord.id);
  await stream.stop();
  await stream.start(wire.callRecord.id);
  const tokens = port.calls
    .filter((call) => call.command === "subscribe_session")
    .map((call) => call.args?.subscriberId);
  expect(tokens).toHaveLength(2);
  expect(new Set(tokens).size).toBe(1);
});
it("bounds unticketed native subscribe work after repeated local deadlines", async () => {
  vi.useFakeTimers();
  const { port, stream } = setup();
  const gates = Array.from({ length: 24 }, () =>
    port.deferNext("subscribe_session"),
  );
  try {
    for (let n = 0; n < 24; n++) {
      const starting = stream.start(wire.callRecord.id);
      await vi.advanceTimersByTimeAsync(10000);
      await starting;
      await stream.stop();
    }
    expect(
      port.calls.filter((call) => call.command === "subscribe_session"),
    ).toHaveLength(1);
  } finally {
    for (const gate of gates) gate.release();
    await vi.advanceTimersByTimeAsync(0);
    await stream.stop();
  }
  expect(port.activeTicketCount()).toBe(0);
  expect(port.liveListenerCount()).toBe(0);
});
it("shares a native unsubscribe through local timeouts and retries only its actual rejection", async () => {
  vi.useFakeTimers();
  const { port, stream } = setup();
  await stream.start(wire.callRecord.id);
  // The command will reject only when released; no retry is valid before then.
  port.rejectNext("unsubscribe_session");
  const gate = port.deferNext("unsubscribe_session");
  try {
    for (let n = 0; n < 4; n++) {
      const stopping = stream.stop();
      await vi.advanceTimersByTimeAsync(20000);
      await stopping;
    }
    expect(
      port.calls.filter((call) => call.command === "unsubscribe_session"),
    ).toHaveLength(1);
    expect(stream.snapshot().error).toContain("超时");
  } finally {
    gate.release();
    await vi.advanceTimersByTimeAsync(0);
    await stream.stop();
  }
  expect(
    port.calls.filter((call) => call.command === "unsubscribe_session"),
  ).toHaveLength(2);
  expect(port.activeTicketCount()).toBe(0);
});
