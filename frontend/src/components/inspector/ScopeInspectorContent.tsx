import React from "react";
import { KeyValueRow } from "../../ui/primitives/KeyValueRow";
import type { MetaValue } from "../../snapshot";
import { formatProcessLabel } from "../../processLabel";
import type { ScopeTableRow } from "../scopes/ScopeTablePanel";
import { MetaSection } from "./MetaTree";
import "./InspectorPanel.css";

function scopeMeta(raw: string): Record<string, MetaValue> | null {
  try {
    const parsed = JSON.parse(raw) as { meta?: unknown };
    if (!parsed.meta || typeof parsed.meta !== "object" || Array.isArray(parsed.meta)) {
      return null;
    }
    return parsed.meta as Record<string, MetaValue>;
  } catch {
    return null;
  }
}

export function ScopeInspectorContent({ scope }: { scope: ScopeTableRow }) {
  const meta = scopeMeta(scope.scopeJson);

  return (
    <>
      <div className="inspector-section">
        <KeyValueRow label="Kind">
          <span className="inspector-mono">{scope.scopeKind}</span>
        </KeyValueRow>
        <KeyValueRow label="Process">
          <span className="inspector-mono">{formatProcessLabel(scope.processName, scope.pid)}</span>
        </KeyValueRow>
        <KeyValueRow label="Scope id">
          <span className="inspector-mono">{scope.scopeId}</span>
        </KeyValueRow>
        <KeyValueRow label="Stream">
          <span className="inspector-mono">{scope.streamId}</span>
        </KeyValueRow>
        <KeyValueRow label="Members">
          <span className="inspector-mono">{scope.memberCount}</span>
        </KeyValueRow>
      </div>
      <MetaSection meta={meta} />
    </>
  );
}
