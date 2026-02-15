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
  MiniMap,
  MarkerType,
  type Node,
  type Edge,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import ELK from "elkjs/lib/elk.bundled.js";
import { Graph as GraphIcon, X } from "@phosphor-icons/react";
import type { SnapshotGraph } from "../types";
import { PeepsNode, processColor, estimateNodeHeight, type NodeData } from "./NodeCards";

const elk = new ELK();

const elkOptions = {
  "elk.algorithm": "layered",
  "elk.direction": "DOWN",
  "elk.spacing.nodeNode": "24",
  "elk.layered.spacing.nodeNodeBetweenLayers": "48",
  "elk.padding": "[top=24,left=24,bottom=24,right=24]",
};

const nodeTypes = { peeps: PeepsNode };

function graphToFlowElements(graph: SnapshotGraph): { nodes: Node<NodeData>[]; edges: Edge[] } {
  const nodes: Node<NodeData>[] = graph.nodes.map((n) => ({
    id: n.id,
    type: "peeps",
    data: {
      label:
        (n.attrs.label as string) ?? (n.attrs.method as string) ?? (n.attrs.name as string) ?? n.id,
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
  const graph = {
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
  };

  const result = await elk.layout(graph);
  const layoutedNodes = nodes.map((node) => {
    const elkNode = result.children?.find((c) => c.id === node.id);
    return {
      ...node,
      position: { x: elkNode?.x ?? 0, y: elkNode?.y ?? 0 },
    };
  });

  return { nodes: layoutedNodes, edges };
}

interface GraphViewProps {
  graph: SnapshotGraph | null;
  fullGraph: SnapshotGraph | null;
  filteredNodeId: string | null;
  onSelectNode: (nodeId: string) => void;
  onClearSelection: () => void;
}

function GraphFlow({
  graph,
  onSelectNode,
}: {
  graph: SnapshotGraph;
  onSelectNode: (id: string) => void;
}) {
  const { nodes: initNodes, edges: initEdges } = useMemo(() => graphToFlowElements(graph), [graph]);

  const [nodes, setNodes, onNodesChange] = useNodesState(initNodes);
  const [edges, setEdges, onEdgesChange] = useEdgesState(initEdges);
  const { fitView } = useReactFlow();

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
    >
      <Background variant={BackgroundVariant.Dots} gap={16} size={1} />
      <Controls showInteractive={false} />
      <MiniMap
        nodeColor={(n) => processColor((n.data as NodeData)?.process ?? "")}
        maskColor="light-dark(rgba(245,245,247,0.7), rgba(12,12,14,0.7))"
      />
    </ReactFlow>
  );
}

export function GraphView({ graph, fullGraph, filteredNodeId, onSelectNode, onClearSelection }: GraphViewProps) {
  const nodeCount = graph?.nodes.length ?? 0;
  const edgeCount = graph?.edges.length ?? 0;
  const isFiltered = filteredNodeId != null && fullGraph != null && graph !== fullGraph;

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
      <div className="react-flow-wrapper">
        {graph && graph.nodes.length > 0 ? (
          <ReactFlowProvider>
            <GraphFlow graph={graph} onSelectNode={onSelectNode} />
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
