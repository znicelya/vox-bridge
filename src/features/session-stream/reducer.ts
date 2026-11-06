import type { RuntimeDiagnostic, TranscriptUpdate } from "../../lib/wire";
import type { SessionStreamState } from "./types";
export function emptyStreamState(): SessionStreamState {
  return {
    session: null,
    finals: [],
    interims: [],
    diagnostics: [],
    recovering: false,
    hasEarlierRows: false,
    error: null,
  };
}
export function reduceTranscript(
  state: SessionStreamState,
  update: TranscriptUpdate,
): SessionStreamState {
  const existing = state.finals.findIndex((row) => row.id === update.id);
  if (!update.is_final && existing >= 0) return state;
  const epoch = update.details?.stream_epoch;
  const latestEpoch = [...state.finals, ...state.interims].reduce(
    (latest, row) =>
      row.direction === update.direction
        ? Math.max(latest, row.details?.stream_epoch ?? -1)
        : latest,
    -1,
  );
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
  const diagnostics = [...state.diagnostics];
  const index = diagnostics.findIndex((row) => row.id === diagnostic.id);
  if (index < 0) diagnostics.push(diagnostic);
  else diagnostics[index] = diagnostic;
  return { ...state, diagnostics: diagnostics.slice(-200) };
}
