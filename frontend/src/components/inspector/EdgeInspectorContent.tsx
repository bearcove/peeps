import React from "react";
import { Badge } from "../../ui/primitives/Badge";
import { KeyValueRow } from "../../ui/primitives/KeyValueRow";
import { kindIcon } from "../../nodeKindSpec";
import { formatProcessLabel } from "../../processLabel";
import type { EntityDef, EdgeDef } from "../../snapshot";
import { edgeTooltip } from "../../graph/elkAdapter";
import "./InspectorPanel.css";

export const EDGE_KIND_LABELS: Record<EdgeDef["kind"], string> = {
  touches: "Resource context",
  needs: "Causal dependency",
  holds: "Permit ownership",
  polls: "Non-blocking observation",
  closed_by: "Closure cause",
  channel_link: "Channel pairing",
  rpc_link: "RPC pairing",
};

export function EdgeInspectorContent({ edge, entityDefs }: { edge: EdgeDef; entityDefs: EntityDef[] }) {
  const srcEntity = entityDefs.find((e) => e.id === edge.source);
  const dstEntity = entityDefs.find((e) => e.id === edge.target);
  const tooltip = edgeTooltip(edge, srcEntity?.name ?? edge.source, dstEntity?.name ?? edge.target);
  const isStructural = edge.kind === "rpc_link" || edge.kind === "channel_link";

  return (
    <>
      <div className="inspector-section">
        <KeyValueRow label="From" icon={srcEntity ? kindIcon(srcEntity.kind, 12) : undefined}>
          <span className="inspector-mono">{srcEntity?.name ?? edge.source}</span>
          {srcEntity && (
            <span className="inspector-mono" style={{ fontSize: "0.75em", marginLeft: 4 }}>
              {formatProcessLabel(srcEntity.processName, srcEntity.processPid)}
            </span>
          )}
        </KeyValueRow>
        <KeyValueRow label="To" icon={dstEntity ? kindIcon(dstEntity.kind, 12) : undefined}>
          <span className="inspector-mono">{dstEntity?.name ?? edge.target}</span>
          {dstEntity && (
            <span className="inspector-mono" style={{ fontSize: "0.75em", marginLeft: 4 }}>
              {formatProcessLabel(dstEntity.processName, dstEntity.processPid)}
            </span>
          )}
        </KeyValueRow>
      </div>

      <div className="inspector-section">
        <KeyValueRow label="Meaning">
          <span className="inspector-mono">{tooltip}</span>
        </KeyValueRow>
        <KeyValueRow label="Type">
          <Badge
            tone={
              isStructural ? "neutral" : edge.kind === "needs" ? "crit" : edge.kind === "holds" ? "ok" : "warn"
            }
          >
            {isStructural ? "structural" : "causal"}
          </Badge>
        </KeyValueRow>
        {edge.opKind && (
          <KeyValueRow label="Operation">
            <span className="inspector-mono">{edge.opKind}</span>
          </KeyValueRow>
        )}
        {edge.state && (
          <KeyValueRow label="State">
            <span className="inspector-mono">{edge.state}</span>
          </KeyValueRow>
        )}
      </div>
    </>
  );
}
