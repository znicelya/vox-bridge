import { expect, it } from "vitest";
import fixture from "../../../tests/contracts/runtime-wire.json";
import { decodeTranscriptUpdate } from "../../lib/wire";
import { emptyStreamState, reduceTranscript } from "./reducer";
const source = decodeTranscriptUpdate(fixture.reverseSourceInterim);
const translation = decodeTranscriptUpdate(fixture.reverseTranslationFinal);
const id = (n: number) =>
  `00000000-0000-0000-0000-${n.toString().padStart(12, "0")}`;
it.each([true, false])(
  "keeps reverse source and translation separate in either arrival order (%s)",
  (sourceFirst) => {
    let state = reduceTranscript(emptyStreamState(), source);
    const final = { ...source, is_final: true, text: "泵" };
    for (const row of sourceFirst ? [final, translation] : [translation, final])
      state = reduceTranscript(state, row);
    expect(state.interims).toHaveLength(0);
    expect(state.finals.map((row) => row.id)).toEqual(
      sourceFirst ? [source.id, translation.id] : [translation.id, source.id],
    );
    expect(
      state.finals.find((row) => row.id === source.id)?.details?.kind,
    ).toBe("source");
    expect(reduceTranscript(state, source)).toEqual(state);
  },
);
it("clears old epochs only in the same direction and ignores stale epoch interims", () => {
  let state = reduceTranscript(emptyStreamState(), source);
  const peer = {
    ...source,
    id: id(100),
    direction: "source_to_target" as const,
  };
  state = reduceTranscript(state, peer);
  const next = {
    ...source,
    id: id(101),
    details: { ...source.details!, stream_epoch: 3 },
  };
  state = reduceTranscript(state, next);
  state = reduceTranscript(state, { ...source, id: id(102) });
  expect(state.interims.map((row) => row.id)).toEqual([peer.id, next.id]);
});
it("bounds display memory without sorting independent directions by sequence", () => {
  let state = emptyStreamState();
  for (let n = 0; n < 501; n++)
    state = reduceTranscript(state, {
      ...source,
      id: id(n + 100),
      sequence: 501 - n,
      is_final: true,
    });
  expect(state.finals).toHaveLength(500);
  expect(state.finals[0].sequence).toBe(500);
  expect(state.finals[state.finals.length - 1]?.sequence).toBe(1);
  expect(state.hasEarlierRows).toBe(true);
  for (let n = 0; n < 129; n++)
    state = reduceTranscript(state, { ...source, id: id(n + 1000) });
  expect(state.interims).toHaveLength(128);
});
