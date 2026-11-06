import type { DesktopPort } from "../lib/desktop";
import type {
  RuntimeDiagnostic,
  SessionRecord,
  TranscriptUpdate,
} from "../lib/wire";

type Call = { command: string; args?: Record<string, unknown> };
type Reply = { value: unknown } | { error: unknown };
const fixtures = new Set<() => void>();
export function resetDesktopFixtures() {
  for (const reset of fixtures) reset();
  fixtures.clear();
}

/** Offline backend protocol: exclusive rowid paging and independently owned tickets/listeners. */
export function runtimeDesktopFixture(wire: { callRecord: SessionRecord }) {
  const records = new Map([[wire.callRecord.id, wire.callRecord]]);
  const transcripts: TranscriptUpdate[] = [];
  const diagnostics: RuntimeDiagnostic[] = [];
  const listeners = new Map<string, Set<(payload: unknown) => void>>();
  const tickets = new Map<
    string,
    { sessionId: string; subscriberId: string; event_name: string }
  >();
  const pendingNative = new Map<string, number>();
  const queued = new Map<string, Reply[]>();
  const gates = new Map<
    string,
    {
      wait: Promise<void>;
      release: () => void;
      entered: () => void;
      phase: "effect" | "reply";
    }[]
  >();
  const releases = new Set<() => void>();
  const calls: Call[] = [];
  let nextTicket = 5000;
  let beforeReply: (() => void) | undefined;
  const queue = (command: string, reply: Reply) =>
    queued.set(command, [...(queued.get(command) ?? []), reply]);
  const page = <T extends { session_id: string }>(
    rows: T[],
    args: Record<string, unknown>,
  ) => {
    const { sessionId, after = null, limit = null } = args;
    const size = limit ?? 100;
    const cursor = after ?? 0;
    if (
      !Number.isInteger(size) ||
      Number(size) < 1 ||
      Number(size) > 200 ||
      !Number.isSafeInteger(cursor) ||
      Number(cursor) < 0
    )
      throw Error("[INVALID_REQUEST] page");
    if (!records.has(String(sessionId)))
      throw Error("[SESSION_NOT_FOUND] missing");
    const found = rows
      .map((row, index) => ({ row, rowid: index + 1 }))
      .filter(
        ({ row, rowid }) =>
          row.session_id === sessionId && rowid > Number(cursor),
      );
    return {
      rows: found.slice(0, Number(size)).map(({ row }) => row),
      next_cursor:
        found.length > Number(size) ? found[Number(size) - 1].rowid : null,
    };
  };
  const fixture = {
    calls,
    async invoke<T>(
      command: string,
      args: Record<string, unknown> = {},
    ): Promise<T> {
      if (calls.length >= 10000) throw Error("fixture call capacity");
      calls.push({ command, args });
      pendingNative.set(command, (pendingNative.get(command) ?? 0) + 1);
      try {
        const gate = gates.get(command)?.shift();
        const programmed = queued.get(command)?.shift();
        if (gate?.phase === "effect") {
          gate.entered();
          await gate.wait;
        }
        let value: unknown;
        if (programmed && "error" in programmed) {
          if (gate?.phase === "reply") {
            gate.entered();
            await gate.wait;
          }
          throw programmed.error;
        }
        if (programmed) value = programmed.value;
        else
          switch (command) {
            case "subscribe_session": {
              if (!records.has(String(args.sessionId)))
                throw Error("[SESSION_NOT_FOUND] missing");
              if (typeof args.subscriberId !== "string")
                throw Error("invalid subscriber");
              for (const [id, ticket] of tickets)
                if (
                  ticket.sessionId === args.sessionId &&
                  ticket.subscriberId === args.subscriberId
                )
                  tickets.delete(id);
              if (tickets.size >= 16)
                throw Error("fixture ticket capacity or invalid subscriber");
              const id = `00000000-0000-0000-0000-${(nextTicket++).toString().padStart(12, "0")}`;
              const event_name = `voxbridge://subscription/${id}`;
              tickets.set(id, {
                sessionId: String(args.sessionId),
                subscriberId: args.subscriberId,
                event_name,
              });
              value = { id, event_name };
              beforeReply?.();
              beforeReply = undefined;
              break;
            }
            case "unsubscribe_session":
              tickets.delete(String(args.subscriptionId));
              value = null;
              break;
            case "get_session":
              value = records.get(String(args.sessionId)) ?? null;
              break;
            case "get_session_transcript":
              value = page(transcripts, args);
              break;
            case "get_session_diagnostics":
              value = page(diagnostics, args);
              break;
            default:
              throw Error(`unsupported fixture command ${command}`);
          }
        if (gate?.phase === "reply") {
          gate.entered();
          await gate.wait;
        }
        return structuredClone(value) as T;
      } finally {
        pendingNative.set(command, (pendingNative.get(command) ?? 1) - 1);
      }
    },
    async listen<T>(
      event: string,
      handler: (payload: T) => void,
    ): Promise<() => void> {
      const reply = queued.get("listen")?.shift();
      if (reply && "error" in reply) throw reply.error;
      if (
        !Array.from(tickets.values()).some(
          (ticket) => ticket.event_name === event,
        ) ||
        fixture.liveListenerCount() >= 16
      )
        throw Error("invalid fixture listener");
      const handlers = listeners.get(event) ?? new Set();
      handlers.add(handler as (payload: unknown) => void);
      listeners.set(event, handlers);
      const gate = gates.get("listen")?.shift();
      gate?.entered();
      if (gate) await gate.wait;
      return () => {
        handlers.delete(handler as (payload: unknown) => void);
        if (!handlers.size) listeners.delete(event);
      };
    },
    beforeSubscribeReply(callback: () => void) {
      beforeReply = callback;
    },
    persistFinal(row: TranscriptUpdate) {
      if (!row.is_final) throw Error("interims are not durable");
      const index = transcripts.findIndex((stored) => stored.id === row.id);
      if (index < 0) transcripts.push(structuredClone(row));
      else transcripts[index] = structuredClone(row);
    },
    persistDiagnostic(row: RuntimeDiagnostic) {
      const index = diagnostics.findIndex((stored) => stored.id === row.id);
      if (index < 0) diagnostics.push(structuredClone(row));
      else diagnostics[index] = structuredClone(row);
    },
    setSession(record: SessionRecord) {
      records.set(record.id, structuredClone(record));
    },
    emit(payload: unknown, sessionId = wire.callRecord.id) {
      for (const ticket of tickets.values())
        if (ticket.sessionId === sessionId) {
          for (const handler of listeners.get(ticket.event_name) ?? [])
            handler(structuredClone(payload));
        }
    },
    rejectNext(
      command: string,
      error: unknown = "[INTERNAL_ERROR] synthetic private detail",
    ) {
      queue(command, { error });
    },
    replyNext(command: string, value: unknown) {
      queue(command, { value });
    },
    deferNext(command: string, phase: "effect" | "reply" = "reply") {
      let release!: () => void;
      let entered!: () => void;
      const wait = new Promise<void>((resolve) => {
        release = resolve;
      });
      const requested = new Promise<void>((resolve) => {
        entered = resolve;
      });
      releases.add(release);
      gates.set(command, [
        ...(gates.get(command) ?? []),
        { wait, release, entered, phase },
      ]);
      return {
        requested,
        release: () => {
          releases.delete(release);
          release();
        },
      };
    },
    pendingNativeCount: (command?: string) =>
      command
        ? (pendingNative.get(command) ?? 0)
        : Array.from(pendingNative.values()).reduce(
            (total, count) => total + count,
            0,
          ),
    activeTicketCount: () => tickets.size,
    liveListenerCount: () =>
      Array.from(listeners.values()).reduce(
        (count, set) => count + set.size,
        0,
      ),
  } satisfies DesktopPort & Record<string, unknown>;
  fixtures.add(() => {
    listeners.clear();
    tickets.clear();
    for (const release of releases) release();
    releases.clear();
  });
  return fixture;
}
