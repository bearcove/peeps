import { MagnifyingGlass } from "@phosphor-icons/react";
import type { StuckRequest, GraphNode } from "../types";

interface InspectorProps {
  selectedRequest: StuckRequest | null;
  selectedGraphNode: GraphNode | null;
}

function formatElapsedFull(ns: number): string {
  const ms = ns / 1_000_000;
  const secs = ns / 1_000_000_000;
  if (secs >= 60) {
    const mins = Math.floor(secs / 60);
    const rem = secs % 60;
    return `${mins}m ${rem.toFixed(1)}s (${ms.toLocaleString()}ms)`;
  }
  return `${secs.toFixed(3)}s (${ms.toLocaleString()}ms)`;
}

export function Inspector({ selectedRequest, selectedGraphNode }: InspectorProps) {
  return (
    <div className="panel">
      <div className="panel-header">
        <MagnifyingGlass size={14} weight="bold" /> Inspector
      </div>
      <div className="inspector">
        {selectedRequest ? (
          <RequestDetail req={selectedRequest} />
        ) : selectedGraphNode ? (
          <GraphNodeDetail node={selectedGraphNode} />
        ) : (
          <div className="inspector-empty">
            Select a request or graph node to inspect.
            <br />
            <br />
            Keyboard: arrows to navigate, enter to select, esc to deselect.
          </div>
        )}
      </div>
    </div>
  );
}

function RequestDetail({ req }: { req: StuckRequest }) {
  return (
    <dl>
      <dt>ID</dt>
      <dd>{req.id}</dd>
      <dt>Method</dt>
      <dd>{req.method ?? "unknown"}</dd>
      <dt>Process</dt>
      <dd>{req.process}</dd>
      <dt>Elapsed</dt>
      <dd>{formatElapsedFull(req.elapsed_ns)}</dd>
      <dt>Task ID</dt>
      <dd>{req.task_id ?? "—"}</dd>
      <dt>Correlation Key</dt>
      <dd>{req.correlation_key ?? "—"}</dd>
    </dl>
  );
}

function GraphNodeDetail({ node }: { node: GraphNode }) {
  return (
    <dl>
      <dt>Node ID</dt>
      <dd>{node.id}</dd>
      <dt>Kind</dt>
      <dd>{node.kind}</dd>
      <dt>Label</dt>
      <dd>{node.label}</dd>
      <dt>Position</dt>
      <dd>
        x={node.x.toFixed(0)}, y={node.y.toFixed(0)}
      </dd>
      <dt>Size</dt>
      <dd>
        {node.width} x {node.height}
      </dd>
    </dl>
  );
}
