import { useCallback, useEffect, useMemo, useState } from "react";
import { WarningCircle } from "@phosphor-icons/react";
import { jumpNow, fetchStuckRequests, fetchGraph } from "./api";
import { Header } from "./components/Header";
import { RequestsTable } from "./components/RequestsTable";
import { GraphView } from "./components/GraphView";
import { Inspector } from "./components/Inspector";
import type { JumpNowResponse, StuckRequest, SnapshotGraph, SnapshotNode } from "./types";

function useSessionState(key: string, initial: boolean): [boolean, () => void] {
  const [value, setValue] = useState(() => {
    const stored = sessionStorage.getItem(key);
    return stored !== null ? stored === "true" : initial;
  });
  const toggle = useCallback(() => {
    setValue((v) => {
      sessionStorage.setItem(key, String(!v));
      return !v;
    });
  }, [key]);
  return [value, toggle];
}

const MIN_ELAPSED_NS = 5_000_000_000; // 5 seconds

/** BFS from a seed node, collecting all reachable nodes (both directions). */
function connectedSubgraph(graph: SnapshotGraph, seedId: string): SnapshotGraph {
  const adj = new Map<string, Set<string>>();
  for (const e of graph.edges) {
    let s = adj.get(e.src_id);
    if (!s) { s = new Set(); adj.set(e.src_id, s); }
    s.add(e.dst_id);
    let d = adj.get(e.dst_id);
    if (!d) { d = new Set(); adj.set(e.dst_id, d); }
    d.add(e.src_id);
  }

  const visited = new Set<string>();
  const queue = [seedId];
  while (queue.length > 0) {
    const id = queue.pop()!;
    if (visited.has(id)) continue;
    visited.add(id);
    const neighbors = adj.get(id);
    if (neighbors) {
      for (const n of neighbors) {
        if (!visited.has(n)) queue.push(n);
      }
    }
  }

  return {
    nodes: graph.nodes.filter((n) => visited.has(n.id)),
    edges: graph.edges.filter((e) => visited.has(e.src_id) && visited.has(e.dst_id)),
  };
}

function searchGraphNodes(graph: SnapshotGraph, needle: string): SnapshotNode[] {
  const q = needle.trim().toLowerCase();
  if (!q) return [];
  return graph.nodes.filter((n) => JSON.stringify(n).toLowerCase().includes(q));
}

export function App() {
  const [snapshot, setSnapshot] = useState<JumpNowResponse | null>(null);
  const [requests, setRequests] = useState<StuckRequest[]>([]);
  const [graph, setGraph] = useState<SnapshotGraph | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [selectedRequest, setSelectedRequest] = useState<StuckRequest | null>(null);
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [filteredNodeId, setFilteredNodeId] = useState<string | null>(null);
  const [graphSearchQuery, setGraphSearchQuery] = useState("");
  const [selectedNode, setSelectedNode] = useState<SnapshotNode | null>(null);

  // Keep graph/inspector focus-first: left and right panels are collapsed by default,
  // but users can expand them and the state is sticky for the current browser session.
  const [leftCollapsed, toggleLeft] = useSessionState("peeps-left-collapsed", true);
  const [rightCollapsed, toggleRight] = useSessionState("peeps-right-collapsed", true);

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
      setFilteredNodeId(null);
      setGraphSearchQuery("");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    handleJumpNow();
  }, [handleJumpNow]);

  const handleSelectRequest = useCallback((req: StuckRequest) => {
    setSelectedRequest(req);
    setSelectedNode(null);
    setSelectedNodeId(req.id);
    setFilteredNodeId(req.id);
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

  const handleClearSelection = useCallback(() => {
    setSelectedRequest(null);
    setSelectedNode(null);
    setSelectedNodeId(null);
    setFilteredNodeId(null);
  }, []);

  // Compute the displayed graph: full graph normally,
  // connected subgraph only when filtering via stuck request click.
  const displayGraph = useMemo(() => {
    if (!graph) return null;
    if (!filteredNodeId) return graph;
    if (!graph.nodes.some((n) => n.id === filteredNodeId)) return graph;
    return connectedSubgraph(graph, filteredNodeId);
  }, [graph, filteredNodeId]);

  const searchResults = useMemo(() => {
    if (!graph) return [];
    return searchGraphNodes(graph, graphSearchQuery).slice(0, 100);
  }, [graph, graphSearchQuery]);

  const handleSelectSearchResult = useCallback(
    (nodeId: string) => {
      setFilteredNodeId(null);
      handleSelectGraphNode(nodeId);
    },
    [handleSelectGraphNode],
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
      <div
        className={[
          "main-content",
          leftCollapsed && "main-content--left-collapsed",
          rightCollapsed && "main-content--right-collapsed",
        ].filter(Boolean).join(" ")}
      >
        <RequestsTable
          requests={requests}
          selectedId={selectedRequest?.id ?? null}
          onSelect={handleSelectRequest}
          collapsed={leftCollapsed}
          onToggleCollapse={toggleLeft}
        />
        <GraphView
          graph={displayGraph}
          fullGraph={graph}
          filteredNodeId={filteredNodeId}
          selectedNodeId={selectedNodeId}
          searchQuery={graphSearchQuery}
          searchResults={searchResults}
          onSearchQueryChange={setGraphSearchQuery}
          onSelectSearchResult={handleSelectSearchResult}
          onSelectNode={handleSelectGraphNode}
          onClearSelection={handleClearSelection}
        />
        <Inspector
          selectedRequest={selectedRequest}
          selectedNode={selectedNode}
          filteredNodeId={filteredNodeId}
          onFocusNode={setFilteredNodeId}
          collapsed={rightCollapsed}
          onToggleCollapse={toggleRight}
        />
      </div>
    </div>
  );
}
