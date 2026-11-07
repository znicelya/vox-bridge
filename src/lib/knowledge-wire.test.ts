import { expect, it } from "vitest";
import fixture from "../../tests/contracts/knowledge-wire.json";
import * as wire from "./knowledge-wire";
import { WireDecodeError } from "./wire";
it("decodes actual nullable native and legacy document DTOs without display fallbacks", () => {
  fixture.kinds.forEach((v) => expect(wire.decodeDocumentKind(v)).toEqual(v));
  fixture.statuses.forEach((v) =>
    expect(wire.decodeDocumentStatus(v)).toEqual(v),
  );
  fixture.summaries.forEach((v) =>
    expect(wire.decodeDocumentSummary(v)).toEqual(v),
  );
  fixture.details.forEach((v) =>
    expect(wire.decodeDocumentDetail(v)).toEqual(v),
  );
  expect(wire.decodeDocumentPage(fixture.page)).toEqual(fixture.page);
  expect(wire.decodeTextLocation(fixture.location)).toEqual(fixture.location);
  expect(wire.decodeTextBlock(fixture.block)).toEqual(fixture.block);
});
it("matches exact camelCase command arguments and preserves cursor/null units", () => {
  expect(
    wire.decodeImportDocumentArgs(fixture.commands.import_document),
  ).toEqual(fixture.commands.import_document);
  expect(
    wire.decodeReplaceDocumentArgs(fixture.commands.replace_document),
  ).toEqual(fixture.commands.replace_document);
  expect(wire.decodeListDocumentsArgs(fixture.commands.list_documents)).toEqual(
    fixture.commands.list_documents,
  );
  expect(wire.decodeGetDocumentArgs(fixture.commands.get_document)).toEqual(
    fixture.commands.get_document,
  );
});
it("rejects secrets, missing nullable fields, unsafe numbers, wrong tags and unbounded pages", () => {
  for (const v of [
    { ...fixture.summaries[0], credential: "secret" },
    { ...fixture.summaries[0], version: 0 },
    { ...fixture.summaries[0], version: 4294967296 },
    { ...fixture.summaries[0], created_at_ms: Number.MAX_SAFE_INTEGER + 1 },
    { ...fixture.summaries[0], updated_at_ms: undefined },
    { ...fixture.summaries[0], kind: "html" },
    { ...fixture.summaries[0], origin: "guessed" },
  ])
    expect(() => wire.decodeDocumentSummary(v)).toThrow(WireDecodeError);
  expect(() =>
    wire.decodeDocumentPage({
      rows: Array(201).fill(fixture.summaries[0]),
      next_cursor: null,
    }),
  ).toThrow(WireDecodeError);
  expect(() =>
    wire.decodeReplaceDocumentArgs({
      document_id: fixture.summaries[0].id,
      path: "/fixture/source.txt",
    }),
  ).toThrow(WireDecodeError);
  expect(() => wire.decodeListDocumentsArgs({ after: -1, limit: 50 })).toThrow(
    WireDecodeError,
  );
  expect(() =>
    wire.decodeListDocumentsArgs({ after: null, limit: 201 }),
  ).toThrow(WireDecodeError);
  expect(() => wire.decodeGetDocumentArgs({ documentId: "invalid" })).toThrow(
    WireDecodeError,
  );
});
it("preserves complete legal long headings rather than truncating to a vector metadata cap", () => {
  const v = { ...fixture.location, heading: "电".repeat(6000) };
  expect(wire.decodeTextLocation(v)).toEqual(v);
});
it("preserves legal headings exceeding eight MiB and rejects headings beyond sixteen MiB", () => {
  expect(
    wire.decodeTextLocation({
      ...fixture.location,
      heading: "a".repeat(9 * 1024 * 1024),
    }).heading?.length,
  ).toBe(9 * 1024 * 1024);
  expect(() =>
    wire.decodeTextLocation({
      ...fixture.location,
      heading: "a".repeat(16 * 1024 * 1024 + 1),
    }),
  ).toThrow(WireDecodeError);
});
it("validates knowledge result identity, finite scores, chunk scalar and aggregate budgets", () => {
  const hit = {
    document_id: fixture.summaries[0].id,
    document_version: 1,
    chunk_id: fixture.summaries[0].id,
    title: "pump",
    location: fixture.location,
    text: "额定电压",
    score: 0.5,
  };
  const result = {
    hits: [hit],
    identity: "a".repeat(64),
    unavailable_documents: 1,
  };
  expect(wire.decodeKnowledgeResults(result)).toEqual(result);
  expect(
    wire.decodeKnowledgeResults({
      hits: [],
      identity: null,
      unavailable_documents: 0,
    }).identity,
  ).toBeNull();
  for (const v of [
    { ...result, identity: "" },
    { ...result, hits: [{ ...hit, score: NaN }] },
    { ...result, hits: [{ ...hit, text: "电".repeat(2401) }] },
    {
      ...result,
      hits: Array(4).fill({
        ...hit,
        location: { ...fixture.location, heading: "a".repeat(9 * 1024 * 1024) },
      }),
    },
  ])
    expect(() => wire.decodeKnowledgeResults(v)).toThrow(WireDecodeError);
});
it("rejects an empty submitted knowledge query while preserving exact whitespace and Unicode", () => {
  const request = {
    query: "",
    selected_segment_ids: [],
    session_id: null,
    limit: 5,
  };
  expect(() => wire.decodeKnowledgeSearch(request)).toThrow(WireDecodeError);
  expect(
    wire.decodeKnowledgeSearch({ ...request, query: "  电压\n24V  " }).query,
  ).toBe("  电压\n24V  ");
});

it("clears exact-selection provenance when the submitted query was edited", () => {
  const selection = {
    sessionId: fixture.summaries[0].id,
    segmentIds: [fixture.summaries[0].id],
    text: "few selected words",
  };
  expect(
    wire.knowledgeSearchRequest(selection.text, selection).selected_segment_ids,
  ).toEqual(selection.segmentIds);
  const edited = wire.knowledgeSearchRequest("edited query", selection);
  expect(edited.query).toBe("edited query");
  expect(edited.selected_segment_ids).toEqual([]);
  expect(edited.session_id).toBe(selection.sessionId);
});

it("accepts every finite knowledge score allowed by the contract", () => {
  const hit = {
    document_id: fixture.summaries[0].id,
    document_version: 1,
    chunk_id: fixture.summaries[0].id,
    title: "pump",
    location: fixture.location,
    text: "24V",
    score: 2,
  };
  expect(wire.decodeKnowledgeHit(hit).score).toBe(2);
  expect(wire.decodeKnowledgeHit({ ...hit, score: -2 }).score).toBe(-2);
});
