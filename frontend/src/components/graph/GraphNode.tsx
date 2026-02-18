import React from "react";
import { DurationDisplay } from "../../ui/primitives/DurationDisplay";
import { kindIcon } from "../../nodeKindSpec";
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
  scopeRgb?: string;
  ghost?: boolean;
};

export function GraphNode({ data }: { data: GraphNodeData }) {
  const showScopeColor =
    data.scopeRgb !== undefined && !data.inCycle && data.statTone !== "crit" && data.statTone !== "warn";
  return (
    <div
        className={[
          "graph-card",
          "graph-node",
          data.inCycle && "graph-node--cycle",
          data.selected && "graph-card--selected",
          data.statTone === "crit" && "graph-card--stat-crit",
          data.statTone === "warn" && "graph-card--stat-warn",
          showScopeColor && "graph-card--scope",
          data.ghost && "graph-card--ghost",
        ]
          .filter(Boolean)
          .join(" ")}
        style={
          showScopeColor
            ? ({
                "--scope-rgb": data.scopeRgb,
              } as React.CSSProperties)
            : undefined
        }
      >
        <span className="graph-node-icon">{kindIcon(data.kind, 16)}</span>
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
