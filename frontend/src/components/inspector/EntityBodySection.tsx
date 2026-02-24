import React from "react";
import { PaperPlaneTilt } from "@phosphor-icons/react";
import { Badge } from "../../ui/primitives/Badge";
import { KeyValueRow } from "../../ui/primitives/KeyValueRow";
import type { EntityBody } from "../../api/types.generated";
import type { EntityDef, Tone } from "../../snapshot";

type RequestBody = Extract<EntityBody, { request: unknown }>;
type ResponseBody = Extract<EntityBody, { response: unknown }>;

export function EntityBodySection({ entity }: { entity: EntityDef }) {
  const { body } = entity;

  if ("request" in body) {
    const req = (body as RequestBody).request;
    return (
      <>
        <KeyValueRow label="Args">
          <span className="inspector-mono">{req.args_json}</span>
        </KeyValueRow>
      </>
    );
  }

  if ("response" in body) {
    const resp = (body as ResponseBody).response;
    const s = resp.status;
    const statusKey = typeof s === "string" ? s : "ok" in s ? "ok" : "error" in s ? "error" : "pending";
    const statusTone: Tone = statusKey === "ok" ? "ok" : statusKey === "error" ? "crit" : "warn";
    return (
      <>
        <KeyValueRow label="Method" icon={<PaperPlaneTilt size={12} weight="bold" />}>
          <span className="inspector-mono">{resp.service_name}.{resp.method_name}</span>
        </KeyValueRow>
        <KeyValueRow label="Status">
          <Badge tone={statusTone}>
            {statusKey}
          </Badge>
        </KeyValueRow>
      </>
    );
  }

  if ("lock" in body) {
    const isLocked = entity.status.label === "locked";
    return (
      <>
        <KeyValueRow label="Lock kind">
          <span className="inspector-mono">{body.lock.kind}</span>
        </KeyValueRow>
        <KeyValueRow label="State">
          <Badge tone={isLocked ? "warn" : "ok"}>
            {isLocked ? "locked" : "unlocked"}
          </Badge>
        </KeyValueRow>
        {isLocked && entity.holderName && (
          <KeyValueRow label="Held by">
            <span className="inspector-mono">{entity.holderName}</span>
          </KeyValueRow>
        )}
      </>
    );
  }

  if ("mpsc_tx" in body) {
    const { queue_len, capacity } = body.mpsc_tx;
    const segmentCount = 8;
    const ratio =
      capacity != null && capacity > 0
        ? Math.max(0, Math.min(1, queue_len / capacity))
        : 0;
    const filledSegments = Math.round(ratio * segmentCount);
    const queueToneClass =
      capacity != null
        ? queue_len >= capacity
          ? "inspector-buffer-segment--crit"
          : queue_len / capacity >= 0.75
            ? "inspector-buffer-segment--warn"
            : "inspector-buffer-segment--ok"
        : "inspector-buffer-segment--ok";
    return (
      <KeyValueRow label="Queue">
        <span className="inspector-queue-value">
          <span className="inspector-mono">
            {queue_len}/{capacity ?? "∞"}
          </span>
          {capacity != null && (
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
    );
  }

  if ("broadcast_tx" in body) {
    return (
      <KeyValueRow label="Capacity">
        <span className="inspector-mono">{body.broadcast_tx.capacity}</span>
      </KeyValueRow>
    );
  }

  if ("broadcast_rx" in body) {
    return (
      <KeyValueRow label="Lag">
        <span className="inspector-mono">{body.broadcast_rx.lag}</span>
      </KeyValueRow>
    );
  }

  if ("watch_tx" in body) {
    const lastUpdate = body.watch_tx.last_update_at;
    return (
      <KeyValueRow label="Last update">
        <span className="inspector-mono">
          {lastUpdate != null ? `P+${lastUpdate}ms` : "never"}
        </span>
      </KeyValueRow>
    );
  }

  if ("oneshot_tx" in body) {
    return (
      <KeyValueRow label="Sent">
        <Badge tone={body.oneshot_tx.sent ? "ok" : "neutral"}>
          {body.oneshot_tx.sent ? "yes" : "no"}
        </Badge>
      </KeyValueRow>
    );
  }

  if ("semaphore" in body) {
    const { max_permits, handed_out_permits } = body.semaphore;
    return (
      <>
        <KeyValueRow label="Permits available">
          <span className="inspector-mono">
            {max_permits - handed_out_permits} / {max_permits}
          </span>
        </KeyValueRow>
      </>
    );
  }

  if ("notify" in body) {
    return (
      <>
        <KeyValueRow label="Waiters">
          <span className="inspector-mono">{body.notify.waiter_count}</span>
        </KeyValueRow>
      </>
    );
  }

  if ("once_cell" in body) {
    return (
      <>
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
      </>
    );
  }

  if ("command" in body) {
    return (
      <>
        <KeyValueRow label="Program">
          <span className="inspector-mono">{body.command.program}</span>
        </KeyValueRow>
        <KeyValueRow label="Args">
          <span className="inspector-mono">{body.command.args.join(" ") || "(none)"}</span>
        </KeyValueRow>
      </>
    );
  }

  if ("file_op" in body) {
    return (
      <>
        <KeyValueRow label="Operation">
          <span className="inspector-mono">{body.file_op.op}</span>
        </KeyValueRow>
        <KeyValueRow label="Path">
          <span className="inspector-mono">{body.file_op.path}</span>
        </KeyValueRow>
      </>
    );
  }

  for (const netKey of ["net_connect", "net_accept", "net_read", "net_write"] as const) {
    if (netKey in body) {
      const net = (body as Record<string, { addr: string }>)[netKey];
      return (
        <>
          <KeyValueRow label="Address">
            <span className="inspector-mono">{net.addr}</span>
          </KeyValueRow>
        </>
      );
    }
  }

  if ("custom" in body) {
    const { kind, display_name, category, attrs } = body.custom;
    let parsedAttrs: Record<string, unknown> = {};
    try {
      parsedAttrs = JSON.parse(attrs);
    } catch {
      // attrs may not be valid JSON object; show raw
    }
    return (
      <>
        <KeyValueRow label="Kind">
          <span className="inspector-mono">{kind}</span>
        </KeyValueRow>
        <KeyValueRow label="Display name">
          <span className="inspector-mono">{display_name}</span>
        </KeyValueRow>
        <KeyValueRow label="Category">
          <Badge tone="neutral">{category}</Badge>
        </KeyValueRow>
        {typeof parsedAttrs === "object" && parsedAttrs !== null && !Array.isArray(parsedAttrs) ? (
          Object.entries(parsedAttrs).map(([k, v]) => (
            <KeyValueRow key={k} label={k}>
              <span className="inspector-mono">{typeof v === "string" ? v : JSON.stringify(v)}</span>
            </KeyValueRow>
          ))
        ) : (
          <KeyValueRow label="Attrs">
            <span className="inspector-mono">{attrs}</span>
          </KeyValueRow>
        )}
      </>
    );
  }

  return null;
}
