import { afterEach, expect, it, vi } from "vitest";
import fixture from "../../../tests/contracts/runtime-wire.json";
import { decodeSessionRecord } from "../../lib/wire";
import { runtimeDesktopFixture } from "../../test/desktop-fixture";
import { createSessionStream } from "./controller";
import type { SessionStreamController } from "./types";
const record = decodeSessionRecord(fixture.callRecord);
const streams: SessionStreamController[] = [];
const setup = () => {
  vi.useFakeTimers();
  const port = runtimeDesktopFixture({ callRecord: record });
  const stream = createSessionStream(port);
  streams.push(stream);
  return { port, stream };
};
afterEach(async () => {
  for (const stream of streams) await stream.stop();
  streams.length = 0;
  vi.useRealTimers();
});
it("serializes the same subscriber key until an earlier native subscribe actually completes", async () => {
  const { port, stream } = setup();
  const gate = port.deferNext("subscribe_session", "effect");
  const old = stream.start(record.id);
  await gate.requested;
  await stream.stop();
  const latest = stream.start(record.id);
  try {
    await vi.advanceTimersByTimeAsync(0);
    expect(
      port.calls.filter((call) => call.command === "subscribe_session"),
    ).toHaveLength(1);
    expect(port.activeTicketCount()).toBe(0);
  } finally {
    gate.release();
    await vi.advanceTimersByTimeAsync(0);
    await old;
    await latest;
  }
  expect(port.activeTicketCount()).toBe(1);
  expect(port.liveListenerCount()).toBe(1);
  port.emit({ type: "transcript", update: fixture.reverseSourceFinal });
  expect(stream.snapshot().finals).toEqual([fixture.reverseSourceFinal]);
  await stream.stop();
  expect(port.pendingNativeCount()).toBe(0);
  expect(port.activeTicketCount()).toBe(0);
});
it("keeps only the latest queued start and settles superseded or stopped local callers", async () => {
  const { port, stream } = setup();
  const gate = port.deferNext("subscribe_session", "effect");
  const first = stream.start(record.id);
  await gate.requested;
  let superseded = false;
  let latestStopped = false;
  const middle = stream.start(record.id).then(() => {
    superseded = true;
  });
  const latest = stream.start(record.id).then(() => {
    latestStopped = true;
  });
  try {
    await vi.advanceTimersByTimeAsync(0);
    expect(superseded).toBe(true);
    expect(latestStopped).toBe(false);
    await stream.stop();
    await vi.advanceTimersByTimeAsync(0);
    expect(latestStopped).toBe(true);
    expect(
      port.calls.filter((call) => call.command === "subscribe_session"),
    ).toHaveLength(1);
  } finally {
    gate.release();
    await vi.advanceTimersByTimeAsync(0);
    await Promise.all([first, middle, latest]);
  }
  expect(port.activeTicketCount()).toBe(0);
  expect(port.liveListenerCount()).toBe(0);
});
it("bounds original native subscriptions across different sessions until settlement", async () => {
  const { port, stream } = setup();
  const gates = Array.from({ length: 12 }, () =>
    port.deferNext("subscribe_session", "effect"),
  );
  try {
    for (let n = 0; n < 12; n++) {
      const next = {
        ...record,
        id: `00000000-0000-0000-0000-${(800 + n).toString().padStart(12, "0")}`,
      };
      port.setSession(next);
      const started = stream.start(next.id);
      await vi.advanceTimersByTimeAsync(10000);
      await started;
      await stream.stop();
    }
    expect(port.pendingNativeCount("subscribe_session")).toBe(4);
    expect(
      port.calls.filter((call) => call.command === "subscribe_session"),
    ).toHaveLength(4);
    expect(stream.snapshot().error).toBeTruthy();
  } finally {
    for (const gate of gates) gate.release();
    await vi.advanceTimersByTimeAsync(0);
    await stream.stop();
  }
  expect(port.pendingNativeCount()).toBe(0);
  expect(port.activeTicketCount()).toBe(0);
});
it.each(["get_session", "listen"])(
  "keeps retired generations with pending %s work inside the same ownership bound",
  async (command) => {
    const { port, stream } = setup();
    const gates = Array.from({ length: 12 }, () => port.deferNext(command));
    try {
      for (let n = 0; n < 12; n++) {
        const started = stream.start(record.id);
        await vi.advanceTimersByTimeAsync(10000);
        await started;
        await stream.stop();
      }
      expect(
        port.calls.filter((call) => call.command === "subscribe_session"),
      ).toHaveLength(4);
    } finally {
      for (const gate of gates) gate.release();
      await vi.advanceTimersByTimeAsync(0);
      await stream.stop();
    }
    expect(port.pendingNativeCount()).toBe(0);
    expect(port.activeTicketCount()).toBe(0);
    expect(port.liveListenerCount()).toBe(0);
  },
);
it("shares a pending old-ticket unsubscribe while a same-key replacement continues receiving events", async () => {
  const { port, stream } = setup();
  await stream.start(record.id);
  const gate = port.deferNext("unsubscribe_session", "effect");
  const stopped = stream.stop();
  await gate.requested;
  await stream.start(record.id);
  try {
    await vi.advanceTimersByTimeAsync(20000);
    await stopped;
    expect(
      port.calls.filter((call) => call.command === "unsubscribe_session"),
    ).toHaveLength(1);
    expect(stream.snapshot().error).toBeNull();
  } finally {
    gate.release();
    await vi.advanceTimersByTimeAsync(0);
  }
  port.emit({ type: "transcript", update: fixture.reverseSourceFinal });
  expect(stream.snapshot().finals).toEqual([fixture.reverseSourceFinal]);
  expect(port.activeTicketCount()).toBe(1);
  await stream.stop();
  expect(port.pendingNativeCount()).toBe(0);
  expect(port.activeTicketCount()).toBe(0);
});
