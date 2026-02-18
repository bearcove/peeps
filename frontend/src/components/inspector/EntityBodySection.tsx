import React from "react";
import { PaperPlaneTilt } from "@phosphor-icons/react";
import { Badge } from "../../ui/primitives/Badge";
import { KeyValueRow } from "../../ui/primitives/KeyValueRow";
import type { EntityBody } from "../../api/types";
import type { EntityDef, Tone } from "../../snapshot";
import "./InspectorPanel.css";

type RequestBody = Extract<EntityBody, { request: unknown }>;
type ResponseBody = Extract<EntityBody, { response: unknown }>;

export function EntityBodySection({ entity }: { entity: EntityDef }) {
  const { body } = entity;

  if (typeof body === "string") {
    return (
      <div className="inspector-section">
        <KeyValueRow label="Body">
          <span className="inspector-mono inspector-muted">
            Future (no body fields)
          </span>
        </KeyValueRow>
      </div>
    );
  }

  if ("request" in body) {
    const req = (body as RequestBody).request;
    return (
      <div className="inspector-section">
        <KeyValueRow label="Args">
          <span
            className={`inspector-mono${req.args_preview === "(no args)" ? " inspector-muted" : ""}`}
          >
            {req.args_preview}
          </span>
        </KeyValueRow>
      </div>
    );
  }

  if ("response" in body) {
    const resp = (body as ResponseBody).response;
    return (
      <div className="inspector-section">
        <KeyValueRow label="Method" icon={<PaperPlaneTilt size={12} weight="bold" />}>
          <span className="inspector-mono">{resp.method}</span>
        </KeyValueRow>
        <KeyValueRow label="Status">
          <Badge tone={resp.status === "ok" ? "ok" : resp.status === "error" ? "crit" : "warn"}>
            {resp.status}
          </Badge>
        </KeyValueRow>
      </div>
    );
  }

  if ("lock" in body) {
    return (
      <div className="inspector-section">
        <KeyValueRow label="Lock kind">
          <span className="inspector-mono">{body.lock.kind}</span>
        </KeyValueRow>
      </div>
    );
  }

  if ("channel_tx" in body || "channel_rx" in body) {
    const ep = "channel_tx" in body ? body.channel_tx : body.channel_rx;
    const lc = ep.lifecycle;
    const lifecycleLabel = typeof lc === "string" ? lc : `closed (${Object.values(lc)[0]})`;
    const lifecycleTone: Tone = lc === "open" ? "ok" : "neutral";
    const mpscBuffer = "mpsc" in ep.details ? ep.details.mpsc.buffer : null;
    const segmentCount = 8;
    const ratio =
      mpscBuffer && mpscBuffer.capacity != null && mpscBuffer.capacity > 0
        ? Math.max(0, Math.min(1, mpscBuffer.occupancy / mpscBuffer.capacity))
        : 0;
    const filledSegments = Math.round(ratio * segmentCount);
    const queueToneClass =
      mpscBuffer && mpscBuffer.capacity != null
        ? mpscBuffer.occupancy >= mpscBuffer.capacity
          ? "inspector-buffer-segment--crit"
          : mpscBuffer.occupancy / mpscBuffer.capacity >= 0.75
            ? "inspector-buffer-segment--warn"
            : "inspector-buffer-segment--ok"
        : "inspector-buffer-segment--ok";
    return (
      <div className="inspector-section">
        <KeyValueRow label="Lifecycle">
          <Badge tone={lifecycleTone}>{lifecycleLabel}</Badge>
        </KeyValueRow>
        {mpscBuffer && (
          <KeyValueRow label="Queue">
            <span className="inspector-queue-value">
              <span className="inspector-mono">
                {mpscBuffer.occupancy}/{mpscBuffer.capacity ?? "∞"}
              </span>
              {mpscBuffer.capacity != null && (
                <span className="inspector-buffer-bar" aria-hidden="true">
                  {Array.from({ length: segmentCount }, (_, i) => (
                    <span
                      key={i}
                      className={[
                        "inspector-buffer-segment",
                        i < filledSegments && queueToneClass,
                      ]
                        .filter(Boolean)
                        .join(" ")}
                    />
                  ))}
                </span>
              )}
            </span>
          </KeyValueRow>
        )}
      </div>
    );
  }

  if ("semaphore" in body) {
    const { max_permits, handed_out_permits } = body.semaphore;
    return (
      <div className="inspector-section">
        <KeyValueRow label="Permits available">
          <span className="inspector-mono">
            {max_permits - handed_out_permits} / {max_permits}
          </span>
        </KeyValueRow>
      </div>
    );
  }

  if ("notify" in body) {
    return (
      <div className="inspector-section">
        <KeyValueRow label="Waiters">
          <span className="inspector-mono">{body.notify.waiter_count}</span>
        </KeyValueRow>
      </div>
    );
  }

  if ("once_cell" in body) {
    return (
      <div className="inspector-section">
        <KeyValueRow label="State">
          <Badge tone={body.once_cell.state === "initialized" ? "ok" : "warn"}>
            {body.once_cell.state}
          </Badge>
        </KeyValueRow>
        {body.once_cell.waiter_count > 0 && (
          <KeyValueRow label="Waiters">
            <span className="inspector-mono">{body.once_cell.waiter_count}</span>
          </KeyValueRow>
        )}
      </div>
    );
  }

  if ("command" in body) {
    return (
      <div className="inspector-section">
        <KeyValueRow label="Program">
          <span className="inspector-mono">{body.command.program}</span>
        </KeyValueRow>
        <KeyValueRow label="Args">
          <span className="inspector-mono">{body.command.args.join(" ") || "(none)"}</span>
        </KeyValueRow>
      </div>
    );
  }

  if ("file_op" in body) {
    return (
      <div className="inspector-section">
        <KeyValueRow label="Operation">
          <span className="inspector-mono">{body.file_op.op}</span>
        </KeyValueRow>
        <KeyValueRow label="Path">
          <span className="inspector-mono">{body.file_op.path}</span>
        </KeyValueRow>
      </div>
    );
  }

  for (const netKey of ["net_connect", "net_accept", "net_read", "net_write"] as const) {
    if (netKey in body) {
      const net = (body as Record<string, { addr: string }>)[netKey];
      return (
        <div className="inspector-section">
          <KeyValueRow label="Address">
            <span className="inspector-mono">{net.addr}</span>
          </KeyValueRow>
        </div>
      );
    }
  }

  return null;
}
