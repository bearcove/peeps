import type {
  ConnectionsResponse,
  CutStatusResponse,
  RecordCurrentResponse,
  RecordingSessionInfo,
  RecordStartRequest,
  SqlResponse,
  SnapshotCutResponse,
  TriggerCutResponse,
} from "./types";

export type ApiMode = "live" | "lab";

export interface ApiClient {
  fetchConnections(): Promise<ConnectionsResponse>;
  fetchSql(sql: string): Promise<SqlResponse>;
  triggerCut(): Promise<TriggerCutResponse>;
  fetchCutStatus(cutId: string): Promise<CutStatusResponse>;
  fetchSnapshot(): Promise<SnapshotCutResponse>;
  startRecording(req?: RecordStartRequest): Promise<RecordingSessionInfo>;
  stopRecording(): Promise<RecordingSessionInfo>;
  fetchRecordingCurrent(): Promise<RecordCurrentResponse>;
  fetchRecordingFrame(frameIndex: number): Promise<SnapshotCutResponse>;
  exportRecording(): Promise<Blob>;
  importRecording(file: File): Promise<RecordingSessionInfo>;
}
