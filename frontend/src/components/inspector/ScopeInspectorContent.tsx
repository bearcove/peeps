import React from "react";
import { KeyValueRow } from "../../ui/primitives/KeyValueRow";
import { formatProcessLabel } from "../../processLabel";
import type { ScopeDef } from "../../snapshot";
import "./InspectorPanel.css";

export function ScopeInspectorContent({ scope }: { scope: ScopeDef }) {
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
      {scope.source && (
        <KeyValueRow label="Source">
          <span className="inspector-mono">{scope.source}</span>
        </KeyValueRow>
      )}
      {scope.krate && (
        <KeyValueRow label="Crate">
          <span className="inspector-mono">{scope.krate}</span>
        </KeyValueRow>
      )}
      <KeyValueRow label="Members">
        <span className="inspector-mono">{scope.memberEntityIds.length}</span>
      </KeyValueRow>
    </div>
  );
}
