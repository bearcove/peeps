import React from "react";
import type { EntityDef, Tone } from "../../snapshot";
import { kindIcon } from "../../nodeKindSpec";
import "./GraphNode.css";
import "./ChannelPairNode.css";

export type ChannelPairNodeData = {
  nodeId: string;
  tx: EntityDef;
  rx: EntityDef;
  channelName: string;
  selected: boolean;
  statTone?: Tone;
  scopeRgbLight?: string;
  scopeRgbDark?: string;
  ghost?: boolean;
};

export function ChannelPairNode({ data }: { data: ChannelPairNodeData }) {
  const { nodeId, channelName, selected, statTone, scopeRgbLight, scopeRgbDark, ghost } = data;
  const showScopeColor = scopeRgbLight !== undefined && scopeRgbDark !== undefined && statTone !== "crit" && statTone !== "warn";

  return (
    <div
      className={[
        "graph-card",
        "channel-pair",
        "channel-pair--compact",
        selected && "graph-card--selected",
        statTone === "crit" && "graph-card--stat-crit",
        statTone === "warn" && "graph-card--stat-warn",
        showScopeColor && "graph-card--scope",
        ghost && "graph-card--ghost",
      ]
        .filter(Boolean)
        .join(" ")}
      style={
        showScopeColor
          ? ({
              "--scope-rgb-light": scopeRgbLight,
              "--scope-rgb-dark": scopeRgbDark,
            } as React.CSSProperties)
          : undefined
      }
    >
      <span
        className="channel-pair-port channel-pair-port--top graph-port-anchor"
        data-node-id={nodeId}
        data-port-id={`${nodeId}:rx`}
        aria-hidden="true"
      />
      <div className="channel-pair-compact-main">
        <span className="channel-pair-icon">{kindIcon("channel_pair", 16)}</span>
        <span className="channel-pair-name">{channelName}</span>
      </div>
      <span
        className="channel-pair-port channel-pair-port--bottom graph-port-anchor"
        data-node-id={nodeId}
        data-port-id={`${nodeId}:tx`}
        aria-hidden="true"
      />
    </div>
  );
}
