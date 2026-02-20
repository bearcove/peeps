import type {
  ConnectionsResponse,
  CutStatusResponse,
  RecordCurrentResponse,
  RecordingSessionInfo,
  RecordStartRequest,
  SnapshotSymbolicationUpdate,
  SqlResponse,
  SnapshotCutResponse,
  TriggerCutResponse,
} from "./types.generated";

export type ApiMode = "live" | "lab";

export interface ApiClient {
  fetchConnections(): Promise<ConnectionsResponse>; // f[impl api.connections]
  fetchSql(sql: string): Promise<SqlResponse>; // f[impl api.sql]
  triggerCut(): Promise<TriggerCutResponse>; // f[impl api.cuts.trigger]
  fetchCutStatus(cutId: string): Promise<CutStatusResponse>; // f[impl api.cuts.status]
  fetchExistingSnapshot(): Promise<SnapshotCutResponse | null>; // f[impl api.snapshot.current]
  fetchSnapshot(): Promise<SnapshotCutResponse>; // f[impl api.snapshot.trigger]
  streamSnapshotSymbolication(
    snapshotId: number,
    onUpdate: (update: SnapshotSymbolicationUpdate) => void,
    onError?: (error: Error) => void,
  ): () => void;
  startRecording(req?: RecordStartRequest): Promise<RecordingSessionInfo>; // f[impl api.record.start]
  stopRecording(): Promise<RecordingSessionInfo>; // f[impl api.record.stop]
  fetchRecordingCurrent(): Promise<RecordCurrentResponse>; // f[impl api.record.current]
  fetchRecordingFrame(frameIndex: number): Promise<SnapshotCutResponse>; // f[impl api.record.frame]
  exportRecording(): Promise<Blob>; // f[impl api.record.export] f[impl recording.export]
  importRecording(file: File): Promise<RecordingSessionInfo>; // f[impl api.record.import] f[impl recording.import]
}
