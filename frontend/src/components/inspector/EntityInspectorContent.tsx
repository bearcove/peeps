import React from "react";
import { Timer, File, Crosshair } from "@phosphor-icons/react";
import { Badge } from "../../ui/primitives/Badge";
import { KeyValueRow } from "../../ui/primitives/KeyValueRow";
import { ActionButton } from "../../ui/primitives/ActionButton";
import { kindIcon, kindDisplayName } from "../../nodeKindSpec";
import { formatProcessLabel } from "../../processLabel";
import type { EntityDef } from "../../snapshot";
import type { EntityDiff } from "../../recording/unionGraph";
import { EntityBodySection } from "./EntityBodySection";
import { MetaSection } from "./MetaTree";
import { Source } from "./Source";
import "./InspectorPanel.css";

function BirthTimestamp({
  birthPtime,
  ageMs,
  birthAbsolute,
}: {
  birthPtime: number;
  ageMs: number;
  birthAbsolute: string;
}) {
  const [open, setOpen] = React.useState(false);

  return (
    <span
      className="inspector-birth-popover-anchor"
      onMouseEnter={() => setOpen(true)}
      onMouseLeave={() => setOpen(false)}
    >
      <span
        className="inspector-mono inspector-birth-trigger"
        tabIndex={0}
        onFocus={() => setOpen(true)}
        onBlur={() => setOpen(false)}
      >
        P+{birthPtime}ms ({ageMs}ms old)
      </span>
      {open && (
        <span className="inspector-tooltip" role="tooltip">
          {birthAbsolute}
        </span>
      )}
    </span>
  );
}

export function EntityInspectorContent({
  entity,
  onFocus,
  entityDiff,
}: {
  entity: EntityDef;
  onFocus: (id: string) => void;
  entityDiff?: EntityDiff | null;
}) {
  const birthAbsolute = isFinite(entity.birthApproxUnixMs) && entity.birthApproxUnixMs > 0
    ? new Date(entity.birthApproxUnixMs).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "long" })
    : null;

  if (entity.channelPair) {
    return (
      <>
        <div className="inspector-subsection-label">TX</div>
        <EntityInspectorContent entity={entity.channelPair.tx} onFocus={onFocus} />

        <div className="inspector-subsection-label">RX</div>
        <EntityInspectorContent entity={entity.channelPair.rx} onFocus={onFocus} />
      </>
    );
  }

  return (
    <>
      <div className="inspector-node-header">
        <span className="inspector-node-icon">{kindIcon(entity.kind, 16)}</span>
        <div className="inspector-node-header-text">
          <div className="inspector-node-kind">{kindDisplayName(entity.kind)}</div>
          <div className="inspector-node-label">{entity.name}</div>
        </div>
        <ActionButton onPress={() => onFocus(entity.id)}>
          <Crosshair size={14} weight="bold" />
          Focus
        </ActionButton>
      </div>

      <div className="inspector-alert-slot">
        {entity.inCycle && (
          <div className="inspector-alert inspector-alert--crit">
            Part of <code>needs</code> cycle — possible deadlock
          </div>
        )}
      </div>

      {entityDiff && (entityDiff.appeared || entityDiff.disappeared || entityDiff.statusChanged || entityDiff.statChanged) && (
        <div className="inspector-diff">
          {entityDiff.appeared && (
            <Badge tone="ok">appeared this frame</Badge>
          )}
          {entityDiff.disappeared && (
            <Badge tone="warn">disappeared this frame</Badge>
          )}
          {entityDiff.statusChanged && (
            <div className="inspector-diff-row">
              <span className="inspector-diff-label">Status</span>
              <span className="inspector-diff-from">{entityDiff.statusChanged.from}</span>
              <span className="inspector-diff-arrow">→</span>
              <span className="inspector-diff-to">{entityDiff.statusChanged.to}</span>
            </div>
          )}
          {entityDiff.statChanged && (
            <div className="inspector-diff-row">
              <span className="inspector-diff-label">Stat</span>
              <span className="inspector-diff-from">{entityDiff.statChanged.from ?? "—"}</span>
              <span className="inspector-diff-arrow">→</span>
              <span className="inspector-diff-to">{entityDiff.statChanged.to ?? "—"}</span>
            </div>
          )}
        </div>
      )}

      <div className="inspector-section">
        <KeyValueRow label="Process">
          <span className="inspector-mono">{formatProcessLabel(entity.processName, entity.processPid)}</span>
        </KeyValueRow>
        <KeyValueRow label="Source" icon={<File size={12} weight="bold" />}>
          <Source source={entity.source} />
        </KeyValueRow>
        {entity.krate && (
          <KeyValueRow label="Crate">
            <span className="inspector-mono">{entity.krate}</span>
          </KeyValueRow>
        )}
        <KeyValueRow label="Birth" icon={<Timer size={12} weight="bold" />}>
          {birthAbsolute ? (
            <BirthTimestamp birthPtime={entity.birthPtime} ageMs={entity.ageMs} birthAbsolute={birthAbsolute} />
          ) : (
            <span className="inspector-mono">P+{entity.birthPtime}ms ({entity.ageMs}ms old)</span>
          )}
        </KeyValueRow>
      </div>

      <EntityBodySection entity={entity} />
      <MetaSection meta={entity.meta} />
    </>
  );
}
