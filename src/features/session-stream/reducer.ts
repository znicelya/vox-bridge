import type {
  RuntimeDiagnostic,
  TranscriptUpdate,
  SessionDirection,
  SessionRecord,
} from "../../lib/wire";
import type { SessionStreamState } from "./types";
export function emptyStreamState(): SessionStreamState {
  return {
    session: null,
    epochs: {},
    finals: [],
    interims: [],
    diagnostics: [],
    recovering: false,
    hasEarlierRows: false,
    error: null,
  };
}
export function observeEpoch(
  state: SessionStreamState,
  direction: SessionDirection,
  epoch: number | null,
): SessionStreamState {
  if (epoch === null || epoch <= (state.epochs[direction] ?? -1)) return state;
  return {
    ...state,
    epochs: { ...state.epochs, [direction]: epoch },
    interims: state.interims.filter(
      (row) =>
        row.direction !== direction ||
        (row.details?.stream_epoch ?? -1) >= epoch,
    ),
  };
}
export function reduceSessionRecord(
  state: SessionStreamState,
  session: SessionRecord,
): SessionStreamState {
  for (const direction of session.runtime?.directions ?? [])
    state = observeEpoch(state, direction.direction, direction.stream_epoch);
  return { ...state, session };
}
export function reduceTranscript(
  state: SessionStreamState,
  update: TranscriptUpdate,
): SessionStreamState {
  const epoch = update.details?.stream_epoch;
  state = observeEpoch(state, update.direction, epoch ?? null);
  const existing = state.finals.findIndex((row) => row.id === update.id);
  if (!update.is_final && existing >= 0) return state;
  const latestEpoch = state.epochs[update.direction] ?? -1;
  if (!update.is_final && epoch !== undefined && epoch < latestEpoch)
    return state;
  const interims = state.interims.filter(
    (row) =>
      row.id !== update.id &&
      !(
        epoch !== undefined &&
        row.direction === update.direction &&
        (row.details?.stream_epoch ?? -1) < epoch
      ),
  );
  if (!update.is_final)
    return { ...state, interims: [...interims, update].slice(-128) };
  const finals = [...state.finals];
  if (existing >= 0) finals[existing] = update;
  else finals.push(update);
  return {
    ...state,
    interims,
    finals: finals.slice(-500),
    hasEarlierRows: state.hasEarlierRows || finals.length > 500,
  };
}
export function reduceDiagnostic(
  state: SessionStreamState,
  diagnostic: RuntimeDiagnostic,
): SessionStreamState {
  if (diagnostic.direction !== null)
    state = observeEpoch(state, diagnostic.direction, diagnostic.stream_epoch);
  const diagnostics = [...state.diagnostics];
  const index = diagnostics.findIndex((row) => row.id === diagnostic.id);
  if (index < 0) diagnostics.push(diagnostic);
  else diagnostics[index] = diagnostic;
  return { ...state, diagnostics: diagnostics.slice(-200) };
}
