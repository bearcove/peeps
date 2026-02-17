import type {
  ConnectionsResponse,
  CutStatusResponse,
  SnapshotCutResponse,
  TriggerCutResponse,
} from "./types";

export type ApiMode = "live" | "lab";

export interface ApiClient {
  fetchConnections(): Promise<ConnectionsResponse>;
  triggerCut(): Promise<TriggerCutResponse>;
  fetchCutStatus(cutId: string): Promise<CutStatusResponse>;
  fetchSnapshot(): Promise<SnapshotCutResponse>;
}
