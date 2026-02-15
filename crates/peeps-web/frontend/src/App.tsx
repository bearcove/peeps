import { useCallback, useEffect, useState } from "react";
import { WarningCircle } from "@phosphor-icons/react";
import { jumpNow, fetchStuckRequests, fetchGraph } from "./api";
import { Header } from "./components/Header";
import { RequestsTable } from "./components/RequestsTable";
import { GraphView } from "./components/GraphView";
import { Inspector } from "./components/Inspector";
import type { JumpNowResponse, StuckRequest, SnapshotGraph, SnapshotNode } from "./types";

const MIN_ELAPSED_NS = 5_000_000_000; // 5 seconds

export function App() {
  const [snapshot, setSnapshot] = useState<JumpNowResponse | null>(null);
  const [requests, setRequests] = useState<StuckRequest[]>([]);
  const [graph, setGraph] = useState<SnapshotGraph | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [selectedRequest, setSelectedRequest] = useState<StuckRequest | null>(null);
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [selectedNode, setSelectedNode] = useState<SnapshotNode | null>(null);

  const handleJumpNow = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const snap = await jumpNow();
      setSnapshot(snap);
      const [stuck, graphData] = await Promise.all([
        fetchStuckRequests(snap.snapshot_id, MIN_ELAPSED_NS),
        fetchGraph(snap.snapshot_id),
      ]);
      setRequests(stuck);
      setGraph(graphData);
      setSelectedRequest(null);
      setSelectedNode(null);
      setSelectedNodeId(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    handleJumpNow();
  }, []);

  const handleSelectRequest = useCallback((req: StuckRequest) => {
    setSelectedRequest(req);
    setSelectedNode(null);
    setSelectedNodeId(null);
  }, []);

  const handleSelectGraphNode = useCallback(
    (nodeId: string) => {
      setSelectedNodeId(nodeId);
      setSelectedRequest(null);
      const node = graph?.nodes.find((n) => n.id === nodeId) ?? null;
      setSelectedNode(node);
    },
    [graph],
  );

  return (
    <div className="app">
      <Header snapshot={snapshot} loading={loading} onJumpNow={handleJumpNow} />
      {error && (
        <div className="status-bar">
          <WarningCircle
            size={14}
            weight="bold"
            style={{ color: "light-dark(#d30000, #ff6b6b)", flexShrink: 0 }}
          />
          <span className="error-text">{error}</span>
        </div>
      )}
      <div className="main-content">
        <RequestsTable
          requests={requests}
          selectedId={selectedRequest?.id ?? null}
          onSelect={handleSelectRequest}
        />
        <GraphView
          graph={graph}
          selectedNodeId={selectedNodeId}
          onSelectNode={handleSelectGraphNode}
        />
        <Inspector selectedRequest={selectedRequest} selectedNode={selectedNode} />
      </div>
    </div>
  );
}
