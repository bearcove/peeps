import React from "react";
import { Badge } from "../../ui/primitives/Badge";
import { DurationDisplay } from "../../ui/primitives/DurationDisplay";
import type { EntityDef, Tone } from "../../snapshot";
import "./ChannelPairNode.css";
import { kindIcon } from "../../nodeKindSpec";

export type RpcPairNodeData = {
  req: EntityDef;
  resp: EntityDef;
  rpcName: string;
  selected: boolean;
  scopeHue?: number;
  ghost?: boolean;
};

export function RpcPairNode({ data }: { data: RpcPairNodeData }) {
  const { req, resp, rpcName, selected, scopeHue, ghost } = data;

  const reqBody = typeof req.body !== "string" && "request" in req.body ? req.body.request : null;
  const respBody =
    typeof resp.body !== "string" && "response" in resp.body ? resp.body.response : null;

  const respStatus = respBody ? respBody.status : "pending";
  const respTone: Tone = respStatus === "ok" ? "ok" : respStatus === "error" ? "crit" : "warn";
  const method = respBody?.method ?? reqBody?.method ?? "?";
  const showScopeColor = scopeHue !== undefined && respStatus !== "error";

  return (
    <div
      className={[
        "channel-pair",
        selected && "channel-pair--selected",
        respStatus === "error" && "channel-pair--stat-crit",
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
      <div className="channel-pair-header">
        <span className="channel-pair-icon">{kindIcon("rpc_pair", 14)}</span>
        <span className="channel-pair-name">{rpcName}</span>
      </div>
      <div className="channel-pair-rows">
        <div className="channel-pair-row channel-pair-row--out">
          <span className="channel-pair-row-label">REQ</span>
          <span className="inspector-mono" style={{ fontSize: "11px" }}>
            {method}
          </span>
          <span className="channel-pair-port channel-pair-port--out" aria-hidden="true" />
        </div>
        <div className="channel-pair-row channel-pair-row--in">
          <span className="channel-pair-port channel-pair-port--in" aria-hidden="true" />
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
    </div>
  );
}
