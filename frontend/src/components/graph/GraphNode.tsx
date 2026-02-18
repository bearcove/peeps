import React from "react";
import { DurationDisplay } from "../../ui/primitives/DurationDisplay";
import { canonicalNodeKind, kindIcon } from "../../nodeKindSpec";
import { type Tone } from "../../snapshot";
import "./GraphNode.css";

export type GraphNodeData = {
  kind: string;
  label: string;
  inCycle: boolean;
  selected: boolean;
  status: { label: string; tone: Tone };
  ageMs: number;
  stat?: string;
  statTone?: Tone;
  scopeHue?: number;
  ghost?: boolean;
};

function flowHalfClassForKind(kind: string): string | null {
  const canonical = canonicalNodeKind(kind);
  if (
    canonical === "request" ||
    canonical === "tx" ||
    canonical === "channel_tx" ||
    canonical === "mpsc_tx" ||
    canonical === "remote_tx" ||
    canonical === "oneshot_tx" ||
    canonical === "watch_tx"
  ) {
    return "graph-node--half-out";
  }
  if (
    canonical === "response" ||
    canonical === "rx" ||
    canonical === "channel_rx" ||
    canonical === "mpsc_rx" ||
    canonical === "remote_rx" ||
    canonical === "oneshot_rx" ||
    canonical === "watch_rx"
  ) {
    return "graph-node--half-in";
  }
  return null;
}

export function GraphNode({ data }: { data: GraphNodeData }) {
  const showScopeColor =
    data.scopeHue !== undefined && !data.inCycle && data.statTone !== "crit" && data.statTone !== "warn";
  const flowHalfClass = flowHalfClassForKind(data.kind);
  return (
    <div
        className={[
          "graph-node",
          flowHalfClass,
          data.inCycle && "graph-node--cycle",
          data.selected && "graph-node--selected",
          data.statTone === "crit" && "graph-node--stat-crit",
          data.statTone === "warn" && "graph-node--stat-warn",
          showScopeColor && "graph-node--scope",
          data.ghost && "graph-node--ghost",
        ]
          .filter(Boolean)
          .join(" ")}
        style={
          showScopeColor
            ? ({
                "--scope-h": String(data.scopeHue),
              } as React.CSSProperties)
            : undefined
        }
      >
        <span className="graph-node-icon">{kindIcon(data.kind, 18)}</span>
        <div className="graph-node-content">
          <div className="graph-node-main">
            <span className="graph-node-label">{data.label}</span>
            {data.ageMs > 3000 && (
              <>
                <span className="graph-node-dot">&middot;</span>
                <DurationDisplay ms={data.ageMs} />
              </>
            )}
            {data.stat && (
              <>
                <span className="graph-node-dot">&middot;</span>
                <span
                  className={[
                    "graph-node-stat",
                    data.statTone === "crit" && "graph-node-stat--crit",
                    data.statTone === "warn" && "graph-node-stat--warn",
                  ]
                    .filter(Boolean)
                    .join(" ")}
                >
                  {data.stat}
                </span>
              </>
            )}
          </div>
        </div>
      </div>
  );
}
