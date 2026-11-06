import {
  invokeDecoded,
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
  type SessionEvent,
  type SessionRecord,
  type SubscriptionTicket,
} from "../../lib/wire";
import {
  emptyStreamState,
  reduceDiagnostic,
  reduceTranscript,
} from "./reducer";
import type { SessionStreamController, SessionStreamState } from "./types";

type Run = {
  generation: number;
  sessionId: string;
  stopped: boolean;
  ticket?: SubscriptionTicket;
  unlisten?: () => void;
  cleanup?: Promise<void>;
  timer?: ReturnType<typeof setInterval>;
  recovery?: Promise<void>;
  recordRequest?: Promise<SessionRecord | null>;
  refreshing: boolean;
  revision: number;
  buffer: SessionEvent[];
  transcriptCursor: number | null;
  diagnosticCursor: number | null;
};
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
  const retiring = new Set<Run>();
  const publish = (next: SessionStreamState) => {
    state = next;
    for (const changed of listeners) changed();
  };
  const alive = (run: Run) =>
    !run.stopped && current === run && generation === run.generation;
  const fail = (run: Run, error: unknown) => {
    if (alive(run)) publish({ ...state, error: localDesktopError(error) });
  };
  function retire(run: Run): Promise<void> {
    run.stopped = true;
    if (run.timer) {
      clearInterval(run.timer);
      run.timer = undefined;
    }
    if (run.unlisten) {
      run.unlisten();
      run.unlisten = undefined;
    }
    if (run.cleanup) return run.cleanup;
    if (!run.ticket) return Promise.resolve();
    retiring.add(run);
    const ticket = run.ticket;
    run.cleanup = (async () => {
      try {
        try {
          await withDesktopDeadline(
            port.invoke("unsubscribe_session", { subscriptionId: ticket.id }),
          );
        } catch {
          await withDesktopDeadline(
            port.invoke("unsubscribe_session", { subscriptionId: ticket.id }),
          );
        }
        run.ticket = undefined;
        retiring.delete(run);
      } catch (error) {
        // Keep the ticket for a later stop/start cleanup retry; never claim it was released.
        publish({ ...state, error: localDesktopError(error) });
      } finally {
        run.cleanup = undefined;
      }
    })();
    return run.cleanup;
  }
  function readSession(run: Run): Promise<SessionRecord | null> {
    if (run.recordRequest) return run.recordRequest;
    const nativeRequest = port
      .invoke<unknown>("get_session", { sessionId: run.sessionId })
      .then((value) => {
        const session = decodeNullableSessionRecord(value);
        if (session && session.id !== run.sessionId)
          throw new WireDecodeError();
        return session;
      });
    // A JS timeout cannot cancel Tauri work. Keep the rejected shared Promise until
    // the native call settles so subsequent ticks cannot pile up native requests.
    run.recordRequest = withDesktopDeadline(nativeRequest);
    const release = () => {
      run.recordRequest = undefined;
    };
    void nativeRequest.then(release, release);
    return run.recordRequest;
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
            ? await invokeDecoded(
                port,
                "get_session_transcript",
                args,
                decodeTranscriptPage,
              )
            : await invokeDecoded(
                port,
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
          publish({ ...state, session });
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
      publish({ ...state, session });
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
    async start(sessionId) {
      const previous = current;
      generation++;
      current = undefined;
      if (previous) void retire(previous);
      for (const pending of retiring) void retire(pending);
      publish(emptyStreamState());
      const run: Run = {
        generation,
        sessionId,
        stopped: false,
        refreshing: false,
        revision: 0,
        buffer: [],
        transcriptCursor: null,
        diagnosticCursor: null,
      };
      current = run;
      try {
        decodeUuid(sessionId);
        if (retiring.size >= 16) throw new Error("cleanup capacity");
        publish({ ...state, recovering: true });
        // Attach ownership to the original Promise so timeouts and late replies still clean up.
        const subscribing = port
          .invoke<unknown>("subscribe_session", {
            sessionId,
            subscriberId: crypto.randomUUID(),
          })
          .then(async (value) => {
            const ticket = decodeSubscriptionTicket(value);
            run.ticket = ticket;
            if (!alive(run)) await retire(run);
            return ticket;
          });
        const ticket = await withDesktopDeadline(subscribing);
        if (!alive(run)) return;
        const listening = port
          .listen<unknown>(ticket.event_name, (payload) =>
            receive(run, payload),
          )
          .then((unlisten) => {
            if (!alive(run)) unlisten();
            else run.unlisten = unlisten;
          });
        await withDesktopDeadline(listening);
        if (!alive(run)) return;
        run.timer = setInterval(() => {
          void refresh(run);
        }, 2000);
        await recover(run);
      } catch (error) {
        fail(run, error);
        if (alive(run)) publish({ ...state, recovering: false });
        await retire(run);
      }
    },
    async stop() {
      generation++;
      const stoppedGeneration = generation;
      const previous = current;
      current = undefined;
      if (previous) await retire(previous);
      for (const pending of [...retiring])
        if (pending !== previous) await retire(pending);
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
