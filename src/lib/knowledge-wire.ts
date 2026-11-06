/** Actual document serde shapes. Legacy nulls remain null; no fallback labels are persisted. */
import { decodeUuid, wireSchema } from './wire';
import { decodeRecordOrigin, decodeIntelligenceErrorCode } from './intelligence-wire';
const {object,string,integer,nullable,array,enumeration}=wireSchema;
const u32=integer(0,4294967295);
const i64=integer(Number.MIN_SAFE_INTEGER);
const field=string(32768);
export const decodeDocumentKind=enumeration('pdf','docx','text','markdown','unknown');
export const decodeDocumentStatus=enumeration('parsing','parsed','indexing','ready','repair_required','failed','deleting');
export const decodeDocumentSummary=object({
  id:decodeUuid,title:nullable(field),kind:decodeDocumentKind,version:integer(1,4294967295),
  origin:decodeRecordOrigin,sha256:field,status:decodeDocumentStatus,chunk_count:u32,
  created_at_ms:i64,updated_at_ms:nullable(i64),error:nullable(decodeIntelligenceErrorCode),
});
export const decodeDocumentDetail=object({
  summary:decodeDocumentSummary,original_path:nullable(field),managed_path:nullable(field),
  legacy_uri:nullable(field),index_identity:nullable(field),
});
export const decodeDocumentPage=object({rows:array(decodeDocumentSummary,200),next_cursor:nullable(integer(0))});
export const decodeTextLocation=object({page:nullable(u32),paragraph:u32,heading:nullable(string(8*1024*1024)),start_char:u32,end_char:u32});
export const decodeTextBlock=object({text:string(8*1024*1024),location:decodeTextLocation});
export const decodeImportDocumentArgs=object({path:field});
export const decodeReplaceDocumentArgs=object({documentId:decodeUuid,path:field});
export const decodeListDocumentsArgs=object({after:nullable(integer(0)),limit:integer(1,200)});
export const decodeGetDocumentArgs=object({documentId:decodeUuid});
export type DocumentKind=ReturnType<typeof decodeDocumentKind>;
export type DocumentStatus=ReturnType<typeof decodeDocumentStatus>;
export type DocumentSummary=ReturnType<typeof decodeDocumentSummary>;
export type DocumentDetail=ReturnType<typeof decodeDocumentDetail>;
export type DocumentPage=ReturnType<typeof decodeDocumentPage>;
export type TextLocation=ReturnType<typeof decodeTextLocation>;
export type TextBlock=ReturnType<typeof decodeTextBlock>;
