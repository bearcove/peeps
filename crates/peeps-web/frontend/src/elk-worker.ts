import ELK from "elkjs/lib/elk.bundled.js";

const elk = new ELK();

export interface ElkLayoutRequest {
  id: number;
  graph: {
    id: string;
    layoutOptions: Record<string, string>;
    children: { id: string; width: number; height: number }[];
    edges: { id: string; sources: string[]; targets: string[] }[];
  };
}

export interface ElkLayoutResponse {
  id: number;
  children: { id: string; x: number; y: number }[];
}

self.onmessage = async (e: MessageEvent<ElkLayoutRequest>) => {
  const { id, graph } = e.data;
  const result = await elk.layout(graph);
  const children = (result.children ?? []).map((c) => ({
    id: c.id,
    x: c.x ?? 0,
    y: c.y ?? 0,
  }));
  const response: ElkLayoutResponse = { id, children };
  self.postMessage(response);
};
