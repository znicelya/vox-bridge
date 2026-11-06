import type {
  RuntimeDiagnostic,
  SessionRecord,
  TranscriptUpdate,
} from "../../lib/wire";
export interface SessionStreamState {
  session: SessionRecord | null;
  finals: TranscriptUpdate[];
  interims: TranscriptUpdate[];
  diagnostics: RuntimeDiagnostic[];
  recovering: boolean;
  hasEarlierRows: boolean;
  error: string | null;
}
export interface SessionStreamController {
  start(sessionId: string): Promise<void>;
  stop(): Promise<void>;
  snapshot(): SessionStreamState;
  subscribe(changed: () => void): () => void;
}
