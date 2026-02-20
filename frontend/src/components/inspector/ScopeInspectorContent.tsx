import React from "react";
import { KeyValueRow } from "../../ui/primitives/KeyValueRow";
import { formatProcessLabel } from "../../processLabel";
import type { ResolvedSnapshotBacktrace, ScopeDef } from "../../snapshot";
import { BacktraceRenderer } from "./BacktraceRenderer";
import "./InspectorPanel.css";

export function ScopeInspectorContent({
  scope,
  backtrace,
}: {
  scope: ScopeDef;
  backtrace?: ResolvedSnapshotBacktrace;
}) {
  return (
    <div className="inspector-kv-table">
      <KeyValueRow label="Kind">
        <span className="inspector-mono">{scope.scopeKind}</span>
      </KeyValueRow>
      <KeyValueRow label="Process">
        <span className="inspector-mono">
          {formatProcessLabel(scope.processName, scope.processPid)}
        </span>
      </KeyValueRow>
      <KeyValueRow label="Scope id">
        <span className="inspector-mono">{scope.scopeId}</span>
      </KeyValueRow>
      <KeyValueRow label="Backtrace ID">
        <span className="inspector-mono">{scope.backtraceId}</span>
      </KeyValueRow>
      {scope.source && (
        <KeyValueRow label="Source">
          <span className="inspector-mono">{scope.source.path}:{scope.source.line}</span>
        </KeyValueRow>
      )}
      <KeyValueRow label="Members">
        <span className="inspector-mono">{scope.memberEntityIds.length}</span>
      </KeyValueRow>
      {backtrace && (
        <div className="inspector-backtrace-slot">
          <BacktraceRenderer backtrace={backtrace} />
        </div>
      )}
    </div>
  );
}
