import {expect,it} from 'vitest';
import fixture from '../../tests/contracts/knowledge-wire.json';
import * as wire from './knowledge-wire';
import {WireDecodeError} from './wire';
it('decodes actual nullable native and legacy document DTOs without display fallbacks',()=>{
  fixture.kinds.forEach(v=>expect(wire.decodeDocumentKind(v)).toEqual(v));
  fixture.statuses.forEach(v=>expect(wire.decodeDocumentStatus(v)).toEqual(v));
  fixture.summaries.forEach(v=>expect(wire.decodeDocumentSummary(v)).toEqual(v));
  fixture.details.forEach(v=>expect(wire.decodeDocumentDetail(v)).toEqual(v));
  expect(wire.decodeDocumentPage(fixture.page)).toEqual(fixture.page);
  expect(wire.decodeTextLocation(fixture.location)).toEqual(fixture.location);
  expect(wire.decodeTextBlock(fixture.block)).toEqual(fixture.block);
});
it('matches exact camelCase command arguments and preserves cursor/null units',()=>{
  expect(wire.decodeImportDocumentArgs(fixture.commands.import_document)).toEqual(fixture.commands.import_document);
  expect(wire.decodeReplaceDocumentArgs(fixture.commands.replace_document)).toEqual(fixture.commands.replace_document);
  expect(wire.decodeListDocumentsArgs(fixture.commands.list_documents)).toEqual(fixture.commands.list_documents);
  expect(wire.decodeGetDocumentArgs(fixture.commands.get_document)).toEqual(fixture.commands.get_document);
});
it('rejects secrets, missing nullable fields, unsafe numbers, wrong tags and unbounded pages',()=>{
  for (const v of [{...fixture.summaries[0],credential:'secret'},{...fixture.summaries[0],version:0},{...fixture.summaries[0],version:4294967296},{...fixture.summaries[0],created_at_ms:Number.MAX_SAFE_INTEGER+1},{...fixture.summaries[0],updated_at_ms:undefined},{...fixture.summaries[0],kind:'html'},{...fixture.summaries[0],origin:'guessed'}]) expect(()=>wire.decodeDocumentSummary(v)).toThrow(WireDecodeError);
  expect(()=>wire.decodeDocumentPage({rows:Array(201).fill(fixture.summaries[0]),next_cursor:null})).toThrow(WireDecodeError);
  expect(()=>wire.decodeReplaceDocumentArgs({document_id:fixture.summaries[0].id,path:'/fixture/source.txt'})).toThrow(WireDecodeError);
  expect(()=>wire.decodeListDocumentsArgs({after:-1,limit:50})).toThrow(WireDecodeError);
  expect(()=>wire.decodeListDocumentsArgs({after:null,limit:201})).toThrow(WireDecodeError);
  expect(()=>wire.decodeGetDocumentArgs({documentId:'invalid'})).toThrow(WireDecodeError);
});
it('preserves complete legal long headings rather than truncating to a vector metadata cap',()=>{
  const v={...fixture.location,heading:'电'.repeat(6000)};
  expect(wire.decodeTextLocation(v)).toEqual(v);
});
