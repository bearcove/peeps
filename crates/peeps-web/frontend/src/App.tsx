import { useCallback, useEffect, useState } from "react";
import { WarningCircle } from "@phosphor-icons/react";
import { jumpNow, fetchStuckRequests } from "./api";
import { Header } from "./components/Header";
import { RequestsTable } from "./components/RequestsTable";
import { GraphView } from "./components/GraphView";
import { Inspector } from "./components/Inspector";
import type { JumpNowResponse, StuckRequest, GraphNode } from "./types";

const MIN_ELAPSED_NS = 5_000_000_000; // 5 seconds

export function App() {
  const [snapshot, setSnapshot] = useState<JumpNowResponse | null>(null);
  const [requests, setRequests] = useState<StuckRequest[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [selectedRequest, setSelectedRequest] = useState<StuckRequest | null>(null);
  const [selectedGraphNodeId, setSelectedGraphNodeId] = useState<string | null>(null);
  const [hoveredGraphNode, setHoveredGraphNode] = useState<GraphNode | null>(null);
  const [selectedGraphNode, setSelectedGraphNode] = useState<GraphNode | null>(null);

  const handleJumpNow = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const snap = await jumpNow();
      setSnapshot(snap);
      const stuck = await fetchStuckRequests(snap.snapshot_id, MIN_ELAPSED_NS);
      setRequests(stuck);
      setSelectedRequest(null);
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
    setSelectedGraphNode(null);
    setSelectedGraphNodeId(null);
  }, []);

  const handleSelectGraphNode = useCallback(
    (nodeId: string) => {
      setSelectedGraphNodeId(nodeId);
      setSelectedRequest(null);
      if (hoveredGraphNode?.id === nodeId) {
        setSelectedGraphNode(hoveredGraphNode);
      }
    },
    [hoveredGraphNode],
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
          selectedNodeId={selectedGraphNodeId}
          onSelectNode={handleSelectGraphNode}
          hoveredNode={hoveredGraphNode}
          onHoverNode={setHoveredGraphNode}
        />
        <Inspector selectedRequest={selectedRequest} selectedGraphNode={selectedGraphNode} />
      </div>
    </div>
  );
}
