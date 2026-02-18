import React from "react";
import { KeyValueRow } from "../../ui/primitives/KeyValueRow";
import { scopeKindIcon } from "../../scopeKindSpec";
import "./InspectorPanel.css";

type ScopeKindInfo = {
  label: string;
  description: string;
  typicalFields: string;
  whyItMatters: string;
};

const SCOPE_KIND_INFO: Record<string, ScopeKindInfo> = {
  process: {
    label: "Process Scope",
    description: "Top-level execution container for one connected process.",
    typicalFields: "pid, process name, stream identity",
    whyItMatters: "Use this to isolate cross-process interactions and filter noise.",
  },
  thread: {
    label: "Thread Scope",
    description: "Execution context tied to one OS thread or runtime worker.",
    typicalFields: "thread name/id, scheduler metadata",
    whyItMatters: "Useful when work starvation or handoff issues are thread-local.",
  },
  task: {
    label: "Task Scope",
    description: "Logical async task context that groups related futures/resources.",
    typicalFields: "task name/id, lifecycle state",
    whyItMatters: "Helps identify stalled tasks and scope-local dependencies.",
  },
  connection: {
    label: "Connection Scope",
    description: "Network/RPC connection context and its flow-control boundaries.",
    typicalFields: "in-flight count, backpressure, peer metadata",
    whyItMatters: "This is where blocked/bottlenecked links usually show up first.",
  },
};

export function ScopeKindInspectorContent({ kind }: { kind: string }) {
  const info = SCOPE_KIND_INFO[kind] ?? {
    label: "Scope Kind",
    description: "No detailed schema hint is defined for this kind yet.",
    typicalFields: "kind-specific metadata",
    whyItMatters: "Inspect row-level values in the Scopes table for this kind.",
  };

  return (
    <>
      <div className="inspector-node-header">
        <span className="inspector-node-icon">
          {scopeKindIcon(kind, 16)}
        </span>
        <div className="inspector-node-header-text">
          <div className="inspector-node-kind">scope kind</div>
          <div className="inspector-node-label">{info.label}</div>
        </div>
      </div>

      <div className="inspector-alert-slot" />

      <div className="inspector-section">
        <KeyValueRow label="Kind">
          <span className="inspector-mono">{kind}</span>
        </KeyValueRow>
        <KeyValueRow label="Description">
          <span>{info.description}</span>
        </KeyValueRow>
        <KeyValueRow label="Typical fields">
          <span className="inspector-mono">{info.typicalFields}</span>
        </KeyValueRow>
        <KeyValueRow label="Why it matters">
          <span>{info.whyItMatters}</span>
        </KeyValueRow>
      </div>
    </>
  );
}
