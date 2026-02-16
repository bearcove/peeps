import { useCallback, useEffect, useMemo } from "react";
import {
  ReactFlow,
  ReactFlowProvider,
  useNodesState,
  useEdgesState,
  useReactFlow,
  Background,
  BackgroundVariant,
  Controls,
  MarkerType,
  type Node,
  type Edge,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import ELK from "elkjs/lib/elk-api.js";
import elkWorkerUrl from "elkjs/lib/elk-worker.min.js?url";
import { Graph as GraphIcon, MagnifyingGlass, X } from "@phosphor-icons/react";
import type { SnapshotGraph } from "../types";
import { PeepsNode, processColor, estimateNodeHeight, type NodeData } from "./NodeCards";

const elkOptions = {
  "elk.algorithm": "layered",
  "elk.direction": "DOWN",
  "elk.spacing.nodeNode": "24",
  "elk.layered.spacing.nodeNodeBetweenLayers": "48",
  "elk.padding": "[top=24,left=24,bottom=24,right=24]",
};

const nodeTypes = { peeps: PeepsNode };

// Use ELK's worker-based API (off main thread). Avoid nesting ELK inside our own Worker.
const elk = new ELK({ workerUrl: elkWorkerUrl });

function firstString(
  attrs: Record<string, unknown>,
  keys: string[],
): string | undefined {
  for (const k of keys) {
    const v = attrs[k];
    if (v != null && v !== "") return String(v);
  }
  return undefined;
}

function graphToFlowElements(graph: SnapshotGraph): { nodes: Node<NodeData>[]; edges: Edge[] } {
  const methodByCorrelationKey = new Map<string, string>();
  for (const n of graph.nodes) {
    if (n.kind !== "request") continue;
    const method = firstString(n.attrs, ["method", "request.method"]);
    const corr = firstString(n.attrs, ["correlation_key", "request.correlation_key"]);
    if (method && corr) methodByCorrelationKey.set(corr, method);
  }

  const nodes: Node<NodeData>[] = graph.nodes.map((n) => ({
    id: n.id,
    type: "peeps",
    data: {
      label: (() => {
        if (n.kind === "request") {
          return (
            firstString(n.attrs, ["method", "request.method"]) ??
            firstString(n.attrs, ["label", "name"]) ??
            n.id
          );
        }

        if (n.kind === "response") {
          const corr = firstString(n.attrs, ["correlation_key", "response.correlation_key", "request.correlation_key"]);
          return (
            firstString(n.attrs, ["method", "response.method", "request.method"]) ??
            (corr ? methodByCorrelationKey.get(corr) : undefined) ??
            firstString(n.attrs, ["label", "name"]) ??
            n.id
          );
        }

        return (
          firstString(n.attrs, ["label", "method", "name"]) ??
          n.id
        );
      })(),
      kind: n.kind,
      process: n.process,
      attrs: n.attrs,
    },
    position: { x: 0, y: 0 },
  }));

  const nodeIds = new Set(graph.nodes.map((n) => n.id));
  const edges: Edge[] = graph.edges
    .filter((e) => nodeIds.has(e.src_id) && nodeIds.has(e.dst_id))
    .map((e) => ({
      id: `${e.src_id}->${e.dst_id}`,
      source: e.src_id,
      target: e.dst_id,
      markerEnd: { type: MarkerType.ArrowClosed, width: 12, height: 12 },
      style: { stroke: "light-dark(#c7c7cc, #48484a)", strokeWidth: 1.5 },
    }));

  return { nodes, edges };
}

async function layoutElements(
  nodes: Node<NodeData>[],
  edges: Edge[],
): Promise<{ nodes: Node<NodeData>[]; edges: Edge[] }> {
  const result = await elk.layout({
    id: "root",
    layoutOptions: elkOptions,
    children: nodes.map((n) => ({
      id: n.id,
      width: 250,
      height: estimateNodeHeight(n.data.kind),
    })),
    edges: edges.map((e) => ({
      id: e.id,
      sources: [e.source],
      targets: [e.target],
    })),
  });

  const posMap = new Map(
    (result.children ?? []).map((c) => [c.id, { x: c.x ?? 0, y: c.y ?? 0 }]),
  );
  const layoutedNodes = nodes.map((node) => ({
    ...node,
    position: posMap.get(node.id) ?? { x: 0, y: 0 },
  }));
  return { nodes: layoutedNodes, edges };
}

interface GraphViewProps {
  graph: SnapshotGraph | null;
  fullGraph: SnapshotGraph | null;
  filteredNodeId: string | null;
  selectedNodeId: string | null;
  searchQuery: string;
  searchResults: SnapshotGraph["nodes"];
  onSearchQueryChange: (value: string) => void;
  onSelectSearchResult: (nodeId: string) => void;
  onSelectNode: (nodeId: string) => void;
  onClearSelection: () => void;
}

function GraphFlow({
  graph,
  selectedNodeId,
  onSelectNode,
}: {
  graph: SnapshotGraph;
  selectedNodeId: string | null;
  onSelectNode: (id: string) => void;
}) {
  const { nodes: initNodes, edges: initEdges } = useMemo(() => graphToFlowElements(graph), [graph]);

  const [nodes, setNodes, onNodesChange] = useNodesState(initNodes);
  const [edges, setEdges, onEdgesChange] = useEdgesState(initEdges);
  const { fitView, setCenter, getZoom } = useReactFlow();

  useEffect(() => {
    const { nodes: n, edges: e } = graphToFlowElements(graph);
    if (n.length === 0) {
      setNodes([]);
      setEdges([]);
      return;
    }
    layoutElements(n, e).then(({ nodes: ln, edges: le }) => {
      setNodes(ln);
      setEdges(le);
      window.requestAnimationFrame(() => fitView({ padding: 0.15 }));
    });
  }, [graph, setNodes, setEdges, fitView]);

  useEffect(() => {
    setNodes((curr) =>
      curr.map((node) => ({
        ...node,
        selected: selectedNodeId != null && node.id === selectedNodeId,
      })),
    );
  }, [selectedNodeId, setNodes]);

  useEffect(() => {
    if (!selectedNodeId) return;
    const selected = nodes.find((n) => n.id === selectedNodeId);
    if (!selected) return;
    const cx = selected.position.x + 125;
    const cy = selected.position.y + estimateNodeHeight(selected.data.kind) / 2;
    setCenter(cx, cy, { duration: 220, zoom: Math.max(getZoom(), 0.7) });
  }, [selectedNodeId, nodes, setCenter, getZoom]);

  const onNodeClick = useCallback(
    (_: React.MouseEvent, node: Node) => {
      onSelectNode(node.id);
    },
    [onSelectNode],
  );

  return (
    <ReactFlow
      nodes={nodes}
      edges={edges}
      onNodesChange={onNodesChange}
      onEdgesChange={onEdgesChange}
      onNodeClick={onNodeClick}
      nodeTypes={nodeTypes}
      fitView
      proOptions={{ hideAttribution: true }}
      minZoom={0.1}
      maxZoom={4}
      // Pan by dragging the empty canvas (helps when side panels are collapsed).
      panOnDrag
    >
      <Background variant={BackgroundVariant.Dots} gap={16} size={1} />
      <Controls showInteractive={false} />
    </ReactFlow>
  );
}

export function GraphView({
  graph,
  fullGraph,
  filteredNodeId,
  selectedNodeId,
  searchQuery,
  searchResults,
  onSearchQueryChange,
  onSelectSearchResult,
  onSelectNode,
  onClearSelection,
}: GraphViewProps) {
  const nodeCount = graph?.nodes.length ?? 0;
  const edgeCount = graph?.edges.length ?? 0;
  const isFiltered = filteredNodeId != null && fullGraph != null && graph !== fullGraph;
  const hasSearch = searchQuery.trim().length > 0;

  return (
    <div className="panel panel--graph">
      <div className="panel-header">
        <GraphIcon size={14} weight="bold" />
        {graph ? `Graph (${nodeCount} nodes, ${edgeCount} edges)` : "Graph"}
        {isFiltered && (
          <button
            className="filter-clear-btn"
            onClick={onClearSelection}
            title="Show full graph"
          >
            <X size={12} weight="bold" />
            filtered
          </button>
        )}
      </div>
      <div className="graph-filter-row">
        <label className="graph-filter-input-wrap" title="Contains match across all node and edge fields">
          <MagnifyingGlass size={12} weight="bold" />
          <input
            className="graph-filter-input"
            type="text"
            placeholder="Search graph (contains any field)"
            value={searchQuery}
            onChange={(e) => onSearchQueryChange(e.target.value)}
          />
        </label>
        {hasSearch && (
          <div className="graph-search-results">
            <div className="graph-search-results-head">{searchResults.length} result(s)</div>
            {searchResults.length === 0 ? (
              <div className="graph-search-empty">No matches</div>
            ) : (
              searchResults.map((node) => (
                <button
                  key={node.id}
                  className={`graph-search-item${selectedNodeId === node.id ? " graph-search-item--active" : ""}`}
                  onClick={() => onSelectSearchResult(node.id)}
                  title={node.id}
                >
                  <span className="graph-search-item-kind">{node.kind}</span>
                  <span className="graph-search-item-label">
                    {String(node.attrs["method"] ?? node.attrs["request.method"] ?? node.attrs["name"] ?? node.id)}
                  </span>
                  <span className="graph-search-item-id">{node.id}</span>
                </button>
              ))
            )}
          </div>
        )}
      </div>
      <div className="react-flow-wrapper">
        {graph && graph.nodes.length > 0 ? (
          <ReactFlowProvider>
            <GraphFlow graph={graph} selectedNodeId={selectedNodeId} onSelectNode={onSelectNode} />
          </ReactFlowProvider>
        ) : (
          <div style={{ padding: 16, color: "light-dark(#6e6e73, #98989d)", fontSize: 12 }}>
            {graph ? "No graph data in this snapshot." : "Take a snapshot to see the graph."}
          </div>
        )}
      </div>
    </div>
  );
}
