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
  ArrowFatLineRight,
  ArrowsLeftRight,
  GearSix,
  HourglassHigh,
  LockKey,
} from "@phosphor-icons/react";
import type { SnapshotGraph } from "../types";

const elk = new ELK();

const elkOptions = {
  "elk.algorithm": "layered",
  "elk.direction": "DOWN",
  "elk.spacing.nodeNode": "24",
  "elk.layered.spacing.nodeNodeBetweenLayers": "48",
  "elk.padding": "[top=24,left=24,bottom=24,right=24]",
};

const kindIcons: Record<string, React.ReactNode> = {
  request: <ArrowFatLineRight size={14} weight="bold" color="#0071e3" />,
  response: <ArrowsLeftRight size={14} weight="bold" color="#34c759" />,
  task: <GearSix size={14} weight="bold" color="#ff9500" />,
  future: <HourglassHigh size={14} weight="bold" color="#af52de" />,
  lock: <LockKey size={14} weight="bold" color="#ff3b30" />,
};

const kindColors: Record<string, string> = {
  request: "#0071e3",
  response: "#34c759",
  task: "#ff9500",
  future: "#af52de",
  lock: "#ff3b30",
};

interface NodeData {
  label: string;
  kind: string;
  process: string;
  attrs: Record<string, unknown>;
  [key: string]: unknown;
}

// Pick the most useful attrs to display on the node (keep it short)
function pickDisplayAttrs(kind: string, attrs: Record<string, unknown>): [string, string][] {
  const result: [string, string][] = [];
  const pick = (key: string) => {
    if (attrs[key] != null) {
      const val = String(attrs[key]);
      if (val.length > 0) result.push([key, val]);
    }
  };

  switch (kind) {
    case "request":
      pick("method");
      pick("elapsed_ns");
      pick("status");
      break;
    case "response":
      pick("status");
      pick("correlation_key");
      break;
    case "task":
      pick("name");
      pick("state");
      break;
    case "future":
      pick("poll_count");
      pick("waker");
      break;
    case "lock":
      pick("holder");
      pick("waiters");
      break;
    default:
      // Show first 3 attrs for unknown kinds
      for (const [k, v] of Object.entries(attrs).slice(0, 3)) {
        if (v != null) result.push([k, String(v)]);
      }
  }
  return result;
}

function formatAttrValue(key: string, val: string): string {
  if (key === "elapsed_ns") {
    const ns = Number(val);
    if (!isNaN(ns)) {
      const secs = ns / 1_000_000_000;
      if (secs >= 60) return `${(secs / 60).toFixed(1)}m`;
      return `${secs.toFixed(1)}s`;
    }
  }
  return val;
}

const PeepsNode = memo(({ data }: NodeProps<Node<NodeData>>) => {
  const { label, kind, process, attrs } = data;
  const icon = kindIcons[kind] ?? <GearSix size={14} weight="bold" />;
  const displayAttrs = pickDisplayAttrs(kind, attrs);

  return (
    <div className={`graph-node-custom kind-${kind}`}>
      <Handle type="target" position={Position.Top} style={{ visibility: "hidden" }} />
      <div className="node-header">
        <span className="node-icon">{icon}</span>
        <span className="node-label">{label ?? kind}</span>
      </div>
      <div className="node-attrs">
        <div className="node-attr">
          <span className="node-attr-key">proc:</span>
          <span className="node-attr-val">{process}</span>
        </div>
        {displayAttrs.map(([k, v]) => (
          <div key={k} className="node-attr">
            <span className="node-attr-key">{k}:</span>
            <span className="node-attr-val">{formatAttrValue(k, v)}</span>
          </div>
        ))}
      </div>
      <Handle type="source" position={Position.Bottom} style={{ visibility: "hidden" }} />
    </div>
  );
});

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
      width: 180,
      height: 70,
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
  selectedNodeId: string | null;
  onSelectNode: (nodeId: string) => void;
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
  }, [graph]);

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
        nodeColor={(n) => kindColors[(n.data as NodeData)?.kind] ?? "#888"}
        maskColor="light-dark(rgba(245,245,247,0.7), rgba(12,12,14,0.7))"
      />
    </ReactFlow>
  );
}

export function GraphView({ graph, selectedNodeId, onSelectNode }: GraphViewProps) {
  const nodeCount = graph?.nodes.length ?? 0;
  const edgeCount = graph?.edges.length ?? 0;

  return (
    <div className="panel panel--graph">
      <div className="panel-header">
        <GraphIcon size={14} weight="bold" />
        {graph ? `Graph (${nodeCount} nodes, ${edgeCount} edges)` : "Graph"}
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
