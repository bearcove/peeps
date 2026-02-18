import React from "react";
import { Timer, FileRs, Crosshair } from "@phosphor-icons/react";
import { Badge } from "../../ui/primitives/Badge";
import { KeyValueRow } from "../../ui/primitives/KeyValueRow";
import { DurationDisplay } from "../../ui/primitives/DurationDisplay";
import { ActionButton } from "../../ui/primitives/ActionButton";
import { kindIcon, kindDisplayName } from "../../nodeKindSpec";
import { formatProcessLabel } from "../../processLabel";
import type { EntityDef, Tone } from "../../snapshot";
import type { EntityDiff } from "../../recording/unionGraph";
import { ChannelPairInspectorContent } from "./ChannelPairInspectorContent";
import { EntityBodySection } from "./EntityBodySection";
import { MetaSection } from "./MetaTree";
import "./InspectorPanel.css";

export function EntityInspectorContent({
  entity,
  onFocus,
  entityDiff,
}: {
  entity: EntityDef;
  onFocus: (id: string) => void;
  entityDiff?: EntityDiff | null;
}) {
  if (entity.channelPair) {
    return <ChannelPairInspectorContent entity={entity} onFocus={onFocus} />;
  }

  const ageTone: Tone =
    entity.ageMs > 600_000 ? "crit" : entity.ageMs > 60_000 ? "warn" : "neutral";

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
        <KeyValueRow label="Source" icon={<FileRs size={12} weight="bold" />}>
          <a
            className="inspector-source-link"
            href={`zed://file${entity.source}`}
            title="Open in Zed"
          >
            {entity.source}
          </a>
        </KeyValueRow>
        {entity.krate && (
          <KeyValueRow label="Crate">
            <span className="inspector-mono">{entity.krate}</span>
          </KeyValueRow>
        )}
        <KeyValueRow label="Age" icon={<Timer size={12} weight="bold" />}>
          <DurationDisplay ms={entity.ageMs} tone={ageTone} />
        </KeyValueRow>
        <KeyValueRow label="PTime birth">
          <span className="inspector-mono">{entity.birthPtime}ms</span>
        </KeyValueRow>
        {isFinite(entity.birthApproxUnixMs) && entity.birthApproxUnixMs > 0 && (
          <KeyValueRow label="Born ~">
            <span className="inspector-mono">
              {new Date(entity.birthApproxUnixMs).toLocaleTimeString()}
            </span>
          </KeyValueRow>
        )}
      </div>

      <EntityBodySection entity={entity} />
      <MetaSection meta={entity.meta} />
    </>
  );
}
