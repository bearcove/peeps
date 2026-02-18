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
  scopeHue?: number;
  ghost?: boolean;
};

export function ChannelPairNode({ data }: { data: ChannelPairNodeData }) {
  const { nodeId, channelName, selected, statTone, scopeHue, ghost } = data;
  const showScopeColor = scopeHue !== undefined && statTone !== "crit" && statTone !== "warn";

  return (
    <div
      className={[
        "channel-pair",
        "channel-pair--compact",
        selected && "channel-pair--selected",
        statTone === "crit" && "channel-pair--stat-crit",
        statTone === "warn" && "channel-pair--stat-warn",
        showScopeColor && "channel-pair--scope",
        ghost && "channel-pair--ghost",
      ]
        .filter(Boolean)
        .join(" ")}
      style={
        showScopeColor
          ? ({
              "--scope-h": String(scopeHue),
            } as React.CSSProperties)
          : undefined
      }
    >
      <span
        className="channel-pair-port channel-pair-port--in graph-port-anchor"
        data-node-id={nodeId}
        data-port-id={`${nodeId}:rx`}
        aria-hidden="true"
      />
      <div className="channel-pair-compact-main">
        <span className="channel-pair-icon">{kindIcon("channel_pair", 14)}</span>
        <span className="channel-pair-name">{channelName}</span>
      </div>
      <span
        className="channel-pair-port channel-pair-port--out graph-port-anchor"
        data-node-id={nodeId}
        data-port-id={`${nodeId}:tx`}
        aria-hidden="true"
      />
    </div>
  );
}
