import { MagnifyingGlass } from "@phosphor-icons/react";
import type { StuckRequest, SnapshotNode } from "../types";

interface InspectorProps {
  selectedRequest: StuckRequest | null;
  selectedNode: SnapshotNode | null;
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

export function Inspector({ selectedRequest, selectedNode }: InspectorProps) {
  return (
    <div className="panel">
      <div className="panel-header">
        <MagnifyingGlass size={14} weight="bold" /> Inspector
      </div>
      <div className="inspector">
        {selectedRequest ? (
          <RequestDetail req={selectedRequest} />
        ) : selectedNode ? (
          <NodeDetail node={selectedNode} />
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

function NodeDetail({ node }: { node: SnapshotNode }) {
  const attrEntries = Object.entries(node.attrs).filter(([, v]) => v != null);

  return (
    <dl>
      <dt>ID</dt>
      <dd>{node.id}</dd>
      <dt>Kind</dt>
      <dd>{node.kind}</dd>
      <dt>Process</dt>
      <dd>{node.process}</dd>
      <dt>Proc Key</dt>
      <dd>{node.proc_key}</dd>
      {attrEntries.map(([key, val]) => (
        <div key={key}>
          <dt>{key}</dt>
          <dd>{typeof val === "object" ? JSON.stringify(val) : String(val)}</dd>
        </div>
      ))}
    </dl>
  );
}
