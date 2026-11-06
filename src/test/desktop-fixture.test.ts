import { expect, it } from "vitest";
import fixture from "../../tests/contracts/runtime-wire.json";
import { decodeSessionRecord, type SubscriptionTicket } from "../lib/wire";
import { runtimeDesktopFixture } from "./desktop-fixture";
it("models same-key replacement, UUID-specific cleanup, and the native event namespace", async () => {
  const port = runtimeDesktopFixture({
    callRecord: decodeSessionRecord(fixture.callRecord),
  });
  const args = {
    sessionId: fixture.callRecord.id,
    subscriberId: "stable-window",
  };
  const old = await port.invoke<SubscriptionTicket>("subscribe_session", args);
  const seenOld: unknown[] = [];
  const seenNew: unknown[] = [];
  const offOld = await port.listen(old.event_name, (event) =>
    seenOld.push(event),
  );
  const next = await port.invoke<SubscriptionTicket>("subscribe_session", args);
  const offNew = await port.listen(next.event_name, (event) =>
    seenNew.push(event),
  );
  try {
    expect(next.event_name).toBe(`voxbridge://subscription/${next.id}`);
    expect(port.activeTicketCount()).toBe(1);
    await port.invoke("unsubscribe_session", { subscriptionId: old.id });
    port.emit({ type: "transcript", update: fixture.reverseSourceFinal });
    expect(seenOld).toEqual([]);
    expect(seenNew).toEqual([
      { type: "transcript", update: fixture.reverseSourceFinal },
    ]);
  } finally {
    offOld();
    offNew();
    await port.invoke("unsubscribe_session", { subscriptionId: next.id });
  }
});
