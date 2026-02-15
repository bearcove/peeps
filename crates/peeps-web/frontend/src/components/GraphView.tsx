import { memo, useCallback, useEffect, useMemo } from "react";
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
  Handle,
  Position,
  MarkerType,
  type Node,
  type Edge,
  type NodeProps,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import ELK from "elkjs/lib/elk.bundled.js";
import {
  Graph as GraphIcon,
  ArrowsLeftRight,
  ArrowFatLineRight,
  GearSix,
  HourglassHigh,
  LockKey,
} from "@phosphor-icons/react";
import type { GraphNode } from "../types";

const elk = new ELK();

const elkOptions = {
  "elk.algorithm": "layered",
  "elk.direction": "DOWN",
  "elk.spacing.nodeNode": "24",
  "elk.layered.spacing.nodeNodeBetweenLayers": "48",
  "elk.padding": "[top=24,left=24,bottom=24,right=24]",
};

// Icons per node kind
const kindIcons: Record<string, React.ReactNode> = {
  request: <ArrowFatLineRight size={14} weight="bold" color="#0071e3" />,
  response: <ArrowsLeftRight size={14} weight="bold" color="#34c759" />,
  task: <GearSix size={14} weight="bold" color="#ff9500" />,
  future: <HourglassHigh size={14} weight="bold" color="#af52de" />,
  lock: <LockKey size={14} weight="bold" color="#ff3b30" />,
};

interface NodeData {
  label: string;
  kind: string;
  attrs?: Record<string, string>;
  [key: string]: unknown;
}

// Custom node component
const PeepsNode = memo(({ data }: NodeProps<Node<NodeData>>) => {
  const { label, kind, attrs } = data;
  const icon = kindIcons[kind] ?? <GearSix size={14} weight="bold" />;
  const attrEntries = attrs ? Object.entries(attrs) : [];

  return (
    <div className={`graph-node-custom kind-${kind}`}>
      <Handle type="target" position={Position.Top} style={{ visibility: "hidden" }} />
      <div className="node-header">
        <span className="node-icon">{icon}</span>
        <span className="node-label">{label}</span>
      </div>
      {attrEntries.length > 0 && (
        <div className="node-attrs">
          {attrEntries.map(([k, v]) => (
            <div key={k} className="node-attr">
              <span className="node-attr-key">{k}:</span>
              <span className="node-attr-val">{v}</span>
            </div>
          ))}
        </div>
      )}
      <Handle type="source" position={Position.Bottom} style={{ visibility: "hidden" }} />
    </div>
  );
});

const nodeTypes = { peeps: PeepsNode };

// Mock graph data for prototype - with attributes to show the richer nodes
function generateMockNodes(): Node<NodeData>[] {
  const kinds = ["request", "response", "task", "future", "lock"];
  const mockAttrs: Record<string, Record<string, string>> = {
    request: { method: "Runner.run", elapsed: "5.4m" },
    response: { status: "in_flight", correlation: "abc-123" },
    task: { name: "spawn_blocking", state: "running" },
    future: { poll_count: "142", waker: "registered" },
    lock: { holder: "task-3", waiters: "2" },
  };

  const nodes: Node<NodeData>[] = [];
  for (let i = 0; i < 12; i++) {
    const kind = kinds[i % kinds.length];
    nodes.push({
      id: `node-${i}`,
      type: "peeps",
      data: {
        label: `${kind}:${i}`,
        kind,
        attrs: mockAttrs[kind],
      },
      position: { x: 0, y: 0 },
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
  nodes: Node<NodeData>[],
  edges: Edge[],
): Promise<{ nodes: Node<NodeData>[]; edges: Edge[] }> {
  const graph = {
    id: "root",
    layoutOptions: elkOptions,
    children: nodes.map((n) => ({
      id: n.id,
      // Wider and taller to accommodate attrs
      width: 180,
      height: n.data.attrs ? 70 : 36,
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

function GraphFlow({ onSelectNode }: Pick<GraphViewProps, "onSelectNode">) {
  const initialNodes = useMemo(() => generateMockNodes(), []);
  const initialEdges = useMemo(() => generateMockEdges(), []);

  const [nodes, setNodes, onNodesChange] = useNodesState(initialNodes);
  const [edges, setEdges, onEdgesChange] = useEdgesState(initialEdges);
  const { fitView } = useReactFlow();

  useEffect(() => {
    layoutElements(initialNodes, initialEdges).then(({ nodes: ln, edges: le }) => {
      setNodes(ln);
      setEdges(le);
      window.requestAnimationFrame(() => fitView({ padding: 0.15 }));
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
      nodeTypes={nodeTypes}
      fitView
      proOptions={{ hideAttribution: true }}
      minZoom={0.1}
      maxZoom={4}
    >
      <Background variant={BackgroundVariant.Dots} gap={16} size={1} />
      <Controls showInteractive={false} />
      <MiniMap
        nodeColor={(n) => {
          const kind = (n.data as NodeData)?.kind ?? "";
          const colors: Record<string, string> = {
            request: "#0071e3",
            response: "#34c759",
            task: "#ff9500",
            future: "#af52de",
            lock: "#ff3b30",
          };
          return colors[kind] ?? "#888";
        }}
        maskColor="light-dark(rgba(245,245,247,0.7), rgba(12,12,14,0.7))"
      />
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
    <div className="panel panel--graph">
      <div className="panel-header">
        <GraphIcon size={14} weight="bold" /> Graph (mock data)
      </div>
      <div className="react-flow-wrapper">
        <ReactFlowProvider>
          <GraphFlow onSelectNode={onSelectNode} />
        </ReactFlowProvider>
      </div>
    </div>
  );
}
