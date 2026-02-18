import React from "react";
import { Badge } from "../../ui/primitives/Badge";
import { DurationDisplay } from "../../ui/primitives/DurationDisplay";
import type { EntityDef, Tone } from "../../snapshot";
import "./GraphNode.css";
import "./ChannelPairNode.css";
import { kindIcon } from "../../nodeKindSpec";

export type RpcPairNodeData = {
  nodeId: string;
  req: EntityDef;
  resp: EntityDef;
  rpcName: string;
  selected: boolean;
  scopeRgbLight?: string;
  scopeRgbDark?: string;
  ghost?: boolean;
};

export function RpcPairNode({ data }: { data: RpcPairNodeData }) {
  const { nodeId, req, resp, rpcName, selected, scopeRgbLight, scopeRgbDark, ghost } = data;

  const reqBody = typeof req.body !== "string" && "request" in req.body ? req.body.request : null;
  const respBody =
    typeof resp.body !== "string" && "response" in resp.body ? resp.body.response : null;

  const respStatus = respBody ? respBody.status : "pending";
  const respTone: Tone = respStatus === "ok" ? "ok" : respStatus === "error" ? "crit" : "warn";
  const method = respBody?.method ?? reqBody?.method ?? "?";
  const showScopeColor = scopeRgbLight !== undefined && scopeRgbDark !== undefined && respStatus !== "error";

  return (
    <div
      className={[
        "graph-card",
        "channel-pair",
        selected && "graph-card--selected",
        respStatus === "error" && "graph-card--stat-crit",
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
        data-port-id={`${nodeId}:resp`}
        aria-hidden="true"
      />
      <div className="channel-pair-header">
        <span className="channel-pair-icon">{kindIcon("rpc_pair", 16)}</span>
        <span className="channel-pair-name">{rpcName}</span>
      </div>
      <div className="channel-pair-rows">
        <div className="channel-pair-row channel-pair-row--in">
          <span className="channel-pair-row-label">RESP</span>
          <Badge tone={respTone}>{respStatus}</Badge>
          {resp.ageMs > 3000 && (
            <>
              <span className="graph-node-dot">&middot;</span>
              <DurationDisplay ms={resp.ageMs} />
            </>
          )}
        </div>
      </div>
      <span
        className="channel-pair-port channel-pair-port--bottom graph-port-anchor"
        data-node-id={nodeId}
        data-port-id={`${nodeId}:req`}
        aria-hidden="true"
      />
    </div>
  );
}
