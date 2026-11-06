import {
  localDesktopError,
  withDesktopDeadline,
  type DesktopPort,
} from "../../lib/desktop";
import {
  decodeDiagnosticPage,
  decodeNullableSessionRecord,
  decodeSessionEvent,
  decodeSubscriptionTicket,
  decodeTranscriptPage,
  decodeUuid,
  WireDecodeError,
  type Decoder,
  type SessionEvent,
  type SessionRecord,
  type SubscriptionTicket,
} from "../../lib/wire";
import {
  emptyStreamState,
  reduceDiagnostic,
  reduceTranscript,
  reduceSessionRecord,
  observeEpoch,
} from "./reducer";
import type { SessionStreamController, SessionStreamState } from "./types";

type NativeKey =
  | "subscribe_session"
  | "unsubscribe_session"
  | "listen"
  | "get_session"
  | "get_session_transcript"
  | "get_session_diagnostics";
type NativeOperation<T> = { native: Promise<T>; local: Promise<T> };
type Run = {
  generation: number;
  sessionId: string;
  stopped: boolean;
  ticket?: SubscriptionTicket;
  unlisten?: () => void;
  requests: Map<NativeKey, NativeOperation<unknown>>;
  started: Promise<void>;
  finishStart: () => void;
  admissionTimer?: ReturnType<typeof setTimeout>;
  timer?: ReturnType<typeof setInterval>;
  recovery?: Promise<void>;
  refreshing: boolean;
  revision: number;
  buffer: SessionEvent[];
  transcriptCursor: number | null;
  diagnosticCursor: number | null;
};
// Includes retired generations whose original native operations have not settled.
const MAX_OWNED_RUNS = 4;
const terminal = (session: SessionRecord | null) =>
  session?.status === "completed" || session?.status === "failed";
const eventSession = (event: SessionEvent) =>
  event.type === "transcript"
    ? event.update.session_id
    : event.type === "diagnostic"
      ? event.diagnostic.session_id
      : event.session_id;

export function createSessionStream(
  port: DesktopPort,
): SessionStreamController {
  let state = emptyStreamState();
  let generation = 0;
  let current: Run | undefined;
  const listeners = new Set<() => void>();
  const subscriberId = crypto.randomUUID();
  const owned = new Set<Run>();
  let queued: Run | undefined;
  const publish = (next: SessionStreamState) => {
    state = next;
    for (const changed of listeners) changed();
  };
  const alive = (run: Run) =>
    !run.stopped && current === run && generation === run.generation;
  const fail = (run: Run, error: unknown) => {
    if (alive(run)) publish({ ...state, error: localDesktopError(error) });
  };
  function releaseOwnership(run: Run) {
    if (run.stopped && run.requests.size === 0 && !run.ticket && !run.unlisten)
      owned.delete(run);
    pump();
  }
  function operation<T>(
    run: Run,
    key: NativeKey,
    invoke: () => Promise<T>,
  ): NativeOperation<T> {
    const pending = run.requests.get(key);
    if (pending) return pending as NativeOperation<T>;
    // Record admission before invoking native code, and release only after the
    // original Promise settles. A deadline only ends its local caller's wait.
    const native = Promise.resolve().then(invoke);
    const work = { native, local: withDesktopDeadline(native) };
    run.requests.set(key, work);
    const settled = () => {
      run.requests.delete(key);
      releaseOwnership(run);
    };
    void native.then(settled, settled);
    return work;
  }
  function invokeOwned<T>(
    run: Run,
    command: NativeKey,
    args: Record<string, unknown>,
    decode: Decoder<T>,
  ): Promise<T> {
    return operation(run, command, () =>
      port.invoke<unknown>(command, args).then(decode),
    ).local;
  }
  async function retire(run: Run, reportGeneration?: number): Promise<void> {
    run.stopped = true;
    run.finishStart();
    if (queued === run) queued = undefined;
    if (run.timer) {
      clearInterval(run.timer);
      run.timer = undefined;
    }
    if (run.unlisten) {
      run.unlisten();
      run.unlisten = undefined;
    }
    if (run.ticket) {
      const ticket = run.ticket;
      const work = operation(run, "unsubscribe_session", () =>
        port
          .invoke("unsubscribe_session", { subscriptionId: ticket.id })
          // Only a real native rejection permits another native attempt. A local
          // timeout leaves this operation shared, including its eventual retry.
          .catch(() =>
            port.invoke("unsubscribe_session", { subscriptionId: ticket.id }),
          )
          .then(() => {
            run.ticket = undefined;
          }),
      );
      try {
        await work.local;
      } catch (error) {
        if (reportGeneration === generation)
          publish({ ...state, error: localDesktopError(error) });
      }
    }
    releaseOwnership(run);
  }
  function readSession(run: Run): Promise<SessionRecord | null> {
    return invokeOwned(
      run,
      "get_session",
      { sessionId: run.sessionId },
      (value) => {
        const session = decodeNullableSessionRecord(value);
        if (session && session.id !== run.sessionId)
          throw new WireDecodeError();
        return session;
      },
    );
  }
  function pump() {
    const run = queued;
    if (!run || !alive(run) || owned.size >= MAX_OWNED_RUNS) return;
    // A late earlier same-key native subscribe must never replace a newer ticket.
    if (
      [...owned].some(
        (earlier) =>
          earlier.sessionId === run.sessionId &&
          earlier.requests.has("subscribe_session"),
      )
    )
      return;
    queued = undefined;
    if (run.admissionTimer) {
      clearTimeout(run.admissionTimer);
      run.admissionTimer = undefined;
    }
    owned.add(run);
    void launch(run);
  }
  async function launch(run: Run) {
    try {
      const ticket = await operation(run, "subscribe_session", () =>
        port
          .invoke<unknown>("subscribe_session", {
            sessionId: run.sessionId,
            subscriberId,
          })
          .then((value) => {
            const ticket = decodeSubscriptionTicket(value);
            run.ticket = ticket;
            if (!alive(run)) void retire(run);
            return ticket;
          }),
      ).local;
      if (!alive(run)) return;
      await operation(run, "listen", () =>
        port
          .listen<unknown>(ticket.event_name, (payload) =>
            receive(run, payload),
          )
          .then((unlisten) => {
            if (!alive(run)) unlisten();
            else run.unlisten = unlisten;
          }),
      ).local;
      if (!alive(run)) return;
      run.timer = setInterval(() => {
        void refresh(run);
      }, 2000);
      await recover(run);
    } catch (error) {
      fail(run, error);
      if (alive(run)) publish({ ...state, recovering: false });
      await retire(run);
    } finally {
      run.finishStart();
    }
  }
  function apply(run: Run, event: SessionEvent) {
    if (!alive(run)) return;
    if (event.type === "transcript")
      publish(reduceTranscript(state, event.update));
    else if (event.type === "diagnostic")
      publish(reduceDiagnostic(state, event.diagnostic));
    else if (event.type === "status" || event.type === "audio_state")
      void refresh(run);
  }
  async function pages(run: Run, revision: number): Promise<boolean> {
    for (const family of ["transcript", "diagnostics"] as const) {
      let count = 0;
      while (alive(run) && revision === run.revision) {
        // A corrupt/nonterminating producer cannot allocate or spin without a bound.
        if (++count > 1000) throw new WireDecodeError();
        const after =
          family === "transcript" ? run.transcriptCursor : run.diagnosticCursor;
        const args = { sessionId: run.sessionId, after, limit: 200 };
        const page =
          family === "transcript"
            ? await invokeOwned(
                run,
                "get_session_transcript",
                args,
                decodeTranscriptPage,
              )
            : await invokeOwned(
                run,
                "get_session_diagnostics",
                args,
                decodeDiagnosticPage,
              );
        if (!alive(run) || revision !== run.revision) return false;
        if (page.next_cursor !== null && page.next_cursor <= (after ?? 0))
          throw new WireDecodeError();
        if (page.rows.some((row) => row.session_id !== run.sessionId))
          throw new WireDecodeError();
        if (family === "transcript") {
          const transcript = decodeTranscriptPage(page);
          if (transcript.rows.some((row) => !row.is_final))
            throw new WireDecodeError();
          for (const row of transcript.rows)
            state = reduceTranscript(state, row);
          if (page.next_cursor !== null)
            run.transcriptCursor = page.next_cursor;
        } else {
          const diagnostics = decodeDiagnosticPage(page);
          for (const row of diagnostics.rows)
            state = reduceDiagnostic(state, row);
          if (page.next_cursor !== null)
            run.diagnosticCursor = page.next_cursor;
        }
        // Remove buffered duplicates while each page is present, before display truncation
        // can evict their IDs. The buffer itself remains capped at 512 events.
        const persisted = new Set(page.rows.map((row) => row.id));
        run.buffer = run.buffer.filter((event) =>
          family === "transcript"
            ? event.type !== "transcript" || !persisted.has(event.update.id)
            : event.type !== "diagnostic" ||
              !persisted.has(event.diagnostic.id),
        );
        publish(state);
        // Terminal pages have no high-water cursor. Retain the prior confirmed cursor.
        if (page.next_cursor === null) break;
      }
    }
    return alive(run) && revision === run.revision;
  }
  function recover(run: Run): Promise<void> {
    if (run.recovery) return run.recovery;
    publish({ ...state, recovering: true });
    run.recovery = (async () => {
      try {
        while (alive(run)) {
          const revision = run.revision;
          const session = await readSession(run);
          if (!alive(run)) return;
          if (revision !== run.revision) continue;
          if (session === null) throw "[SESSION_NOT_FOUND]";
          publish(reduceSessionRecord(state, session));
          if (!(await pages(run, revision))) continue;
          const buffered = run.buffer;
          run.buffer = [];
          for (const event of buffered) apply(run, event);
          if (revision !== run.revision) continue;
          if (terminal(state.session) && run.timer) {
            clearInterval(run.timer);
            run.timer = undefined;
          }
          break;
        }
      } catch (error) {
        fail(run, error);
      } finally {
        run.recovery = undefined;
        if (alive(run)) publish({ ...state, recovering: false });
      }
    })();
    return run.recovery;
  }
  function resync(run: Run) {
    run.revision++;
    run.buffer = run.buffer.filter(
      (event) => event.type !== "transcript" || event.update.is_final,
    );
    publish({ ...state, interims: [] });
    return recover(run);
  }
  async function refresh(run: Run) {
    if (
      !alive(run) ||
      run.refreshing ||
      run.recovery ||
      terminal(state.session)
    )
      return;
    run.refreshing = true;
    try {
      const session = await readSession(run);
      if (!alive(run)) return;
      if (session === null) throw "[SESSION_NOT_FOUND]";
      publish(reduceSessionRecord(state, session));
      if (terminal(session)) await resync(run);
    } catch (error) {
      fail(run, error);
    } finally {
      run.refreshing = false;
    }
  }
  function receive(run: Run, payload: unknown) {
    if (!alive(run)) return;
    try {
      const event = decodeSessionEvent(payload);
      if (eventSession(event) !== run.sessionId) return;
      if (event.type === "resync_needed") {
        void resync(run);
        return;
      }
      if (event.type === "transcript")
        state = observeEpoch(
          state,
          event.update.direction,
          event.update.details?.stream_epoch ?? null,
        );
      else if (
        event.type === "diagnostic" &&
        event.diagnostic.direction !== null
      )
        state = observeEpoch(
          state,
          event.diagnostic.direction,
          event.diagnostic.stream_epoch,
        );
      if (state.recovering) {
        if (run.buffer.length >= 512) {
          run.buffer = [];
          void resync(run);
        } else run.buffer.push(event);
      } else apply(run, event);
    } catch (error) {
      fail(run, error);
    }
  }
  return {
    start(sessionId) {
      const previous = current;
      generation++;
      current = undefined;
      if (previous) void retire(previous);
      for (const old of owned)
        if (old.stopped && old !== previous) void retire(old);
      publish(emptyStreamState());
      let resolveStart!: () => void;
      const started = new Promise<void>((resolve) => {
        resolveStart = resolve;
      });
      const run: Run = {
        generation,
        sessionId,
        stopped: false,
        requests: new Map(),
        started,
        finishStart: () => {
          if (run.admissionTimer) {
            clearTimeout(run.admissionTimer);
            run.admissionTimer = undefined;
          }
          resolveStart();
        },
        refreshing: false,
        revision: 0,
        buffer: [],
        transcriptCursor: null,
        diagnosticCursor: null,
      };
      current = run;
      try {
        decodeUuid(sessionId);
      } catch (error) {
        fail(run, error);
        void retire(run);
        return started;
      }
      publish({ ...state, recovering: true });
      // Exactly one latest-only local waiter, with cancellation and its own
      // deadline. It does not occupy native admission or retain older waiters.
      queued = run;
      run.admissionTimer = setTimeout(() => {
        if (queued !== run || !alive(run)) return;
        publish({
          ...state,
          recovering: false,
          error: "会话仍在连接或清理，请重试。",
        });
        void retire(run);
      }, 10000);
      pump();
      return started;
    },
    async stop() {
      generation++;
      const stoppedGeneration = generation;
      const previous = current;
      current = undefined;
      const runs = new Set(owned);
      if (previous) runs.add(previous);
      await Promise.all([...runs].map((run) => retire(run, stoppedGeneration)));
      if (generation === stoppedGeneration)
        publish({ ...state, recovering: false, interims: [] });
    },
    snapshot: () => state,
    subscribe(changed) {
      listeners.add(changed);
      return () => {
        listeners.delete(changed);
      };
    },
  };
}
