import ELK from "elkjs/lib/elk.bundled.js";
import type { ElkLayoutRequest, ElkLayoutResult, GraphNode } from "../types";

self.onmessage = async (e: MessageEvent<ElkLayoutRequest>) => {
  const { nodes, edges } = e.data;

  const graph = {
    id: "root",
    layoutOptions: {
      "elk.algorithm": "layered",
      "elk.direction": "DOWN",
      "elk.spacing.nodeNode": "20",
      "elk.layered.spacing.nodeNodeBetweenLayers": "40",
      "elk.padding": "[top=20,left=20,bottom=20,right=20]",
    },
    children: nodes.map((n) => ({
      id: n.id,
      width: n.width,
      height: n.height,
      labels: [{ text: n.label }],
    })),
    edges: edges.map((e) => ({
      id: e.id,
      sources: [e.source],
      targets: [e.target],
    })),
  };

  try {
    const elk = new ELK();
    const result = await elk.layout(graph) as Record<string, unknown>;
    const children = (result.children ?? []) as Array<{ id: string; x?: number; y?: number; width?: number; height?: number }>;
    const laidOutNodes: GraphNode[] = children.map((child) => {
      const original = nodes.find((n) => n.id === child.id)!;
      return {
        id: child.id,
        kind: original.kind,
        label: original.label,
        x: child.x ?? 0,
        y: child.y ?? 0,
        width: child.width ?? original.width,
        height: child.height ?? original.height,
      };
    });

    const layoutResult: ElkLayoutResult = {
      nodes: laidOutNodes,
      edges: edges.map((e) => ({ source: e.source, target: e.target, kind: e.kind })),
      width: (result.width as number) ?? 400,
      height: (result.height as number) ?? 400,
    };

    self.postMessage(layoutResult);
  } catch (err) {
    self.postMessage({ error: String(err) });
  }
};
