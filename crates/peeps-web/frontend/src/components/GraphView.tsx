import { useCallback, useEffect, useMemo } from "react";
import {
  ReactFlow,
  ReactFlowProvider,
  useNodesState,
  useEdgesState,
  useReactFlow,
  Background,
  BackgroundVariant,
  MarkerType,
  type Node,
  type Edge,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import ELK from "elkjs/lib/elk.bundled.js";
import { Graph as GraphIcon } from "@phosphor-icons/react";
import type { GraphNode } from "../types";

const elk = new ELK();

const elkOptions = {
  "elk.algorithm": "layered",
  "elk.direction": "DOWN",
  "elk.spacing.nodeNode": "20",
  "elk.layered.spacing.nodeNodeBetweenLayers": "40",
  "elk.padding": "[top=20,left=20,bottom=20,right=20]",
};

const kindColors: Record<string, string> = {
  request: "#0071e3",
  response: "#34c759",
  task: "#ff9500",
  future: "#af52de",
  lock: "#ff3b30",
};

// Mock graph data for prototype
function generateMockNodes(): Node[] {
  const kinds = ["request", "response", "task", "future", "lock"];
  const nodes: Node[] = [];
  for (let i = 0; i < 12; i++) {
    const kind = kinds[i % kinds.length];
    nodes.push({
      id: `node-${i}`,
      data: { label: `${kind}:${i}`, kind },
      position: { x: 0, y: 0 },
      style: {
        borderLeft: `3px solid ${kindColors[kind] ?? "#888"}`,
        fontSize: 11,
        fontFamily: "var(--font-mono)",
        padding: "4px 8px",
        borderRadius: 4,
        background: "light-dark(#ffffff, #222226)",
        border: "1px solid light-dark(#d2d2d7, #3a3a3e)",
        width: 140,
      },
    });
  }
  return nodes;
}

function generateMockEdges(): Edge[] {
  const connections = [
    [0, 2],
    [2, 3],
    [3, 4],
    [1, 5],
    [5, 6],
    [6, 7],
    [2, 8],
    [8, 9],
    [4, 10],
    [10, 11],
    [0, 1],
    [3, 7],
  ];
  return connections.map(([s, t]) => ({
    id: `e-${s}-${t}`,
    source: `node-${s}`,
    target: `node-${t}`,
    markerEnd: { type: MarkerType.ArrowClosed, width: 12, height: 12 },
    style: { stroke: "light-dark(#c7c7cc, #48484a)", strokeWidth: 1.5 },
  }));
}

async function layoutElements(
  nodes: Node[],
  edges: Edge[],
): Promise<{ nodes: Node[]; edges: Edge[] }> {
  const graph = {
    id: "root",
    layoutOptions: elkOptions,
    children: nodes.map((n) => ({
      id: n.id,
      width: 140,
      height: 28,
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
  selectedNodeId: string | null;
  onSelectNode: (nodeId: string) => void;
  hoveredNode: GraphNode | null;
  onHoverNode: (node: GraphNode | null) => void;
}

function GraphFlow({
  selectedNodeId,
  onSelectNode,
}: Pick<GraphViewProps, "selectedNodeId" | "onSelectNode">) {
  const initialNodes = useMemo(() => generateMockNodes(), []);
  const initialEdges = useMemo(() => generateMockEdges(), []);

  const [nodes, setNodes, onNodesChange] = useNodesState(initialNodes);
  const [edges, setEdges, onEdgesChange] = useEdgesState(initialEdges);
  const { fitView } = useReactFlow();

  useEffect(() => {
    layoutElements(initialNodes, initialEdges).then(({ nodes: ln, edges: le }) => {
      setNodes(ln);
      setEdges(le);
      window.requestAnimationFrame(() => fitView({ padding: 0.2 }));
    });
  }, []);

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
      fitView
      proOptions={{ hideAttribution: true }}
      minZoom={0.2}
      maxZoom={4}
    >
      <Background variant={BackgroundVariant.Dots} gap={16} size={1} />
    </ReactFlow>
  );
}

export function GraphView({
  selectedNodeId,
  onSelectNode,
  hoveredNode,
  onHoverNode,
}: GraphViewProps) {
  return (
    <div className="panel" style={{ display: "flex", flexDirection: "column" }}>
      <div className="panel-header">
        <GraphIcon size={14} weight="bold" /> Graph (mock data)
      </div>
      <div style={{ flex: 1, minHeight: 0 }}>
        <ReactFlowProvider>
          <GraphFlow selectedNodeId={selectedNodeId} onSelectNode={onSelectNode} />
        </ReactFlowProvider>
      </div>
    </div>
  );
}
