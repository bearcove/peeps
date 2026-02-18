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

type MergedSection = {
  label: string;
  entity: EntityDef;
};

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

function EntityInspectorHeader({
  entity,
  focusedEntityId,
  onToggleFocus,
}: {
  entity: EntityDef;
  focusedEntityId: string | null;
  onToggleFocus: (id: string) => void;
}) {
  const isFocused = focusedEntityId === entity.id;

  return (
    <div className="inspector-node-header">
      <span className="inspector-node-icon">{kindIcon(entity.kind, 16)}</span>
      <div className="inspector-node-header-text">
        <div className="inspector-node-kind">{kindDisplayName(entity.kind)}</div>
        <div className="inspector-node-label">{entity.name}</div>
      </div>
      <ActionButton onPress={() => onToggleFocus(entity.id)}>
        <Crosshair size={14} weight="bold" />
        {isFocused ? "Unfocus" : "Focus"}
      </ActionButton>
    </div>
  );
}

function MergedEntityInspectorContent({
  merged,
  sections,
  focusedEntityId,
  onToggleFocus,
  entityDiff,
}: {
  merged: EntityDef;
  sections: readonly MergedSection[];
  focusedEntityId: string | null;
  onToggleFocus: (id: string) => void;
  entityDiff?: EntityDiff | null;
}) {
  return (
    <>
      <EntityInspectorHeader entity={merged} focusedEntityId={focusedEntityId} onToggleFocus={onToggleFocus} />
      <div className="inspector-alert-slot" />
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
      {sections.map((section) => (
        <React.Fragment key={`${merged.id}:${section.label}`}>
          <div className="inspector-subsection-label">{section.label}</div>
          <EntityInspectorBody entity={section.entity} focusedEntityId={focusedEntityId} onToggleFocus={onToggleFocus} showHeader={false} />
        </React.Fragment>
      ))}
    </>
  );
}

export function EntityInspectorContent({
  entity,
  focusedEntityId,
  onToggleFocus,
  entityDiff,
}: {
  entity: EntityDef;
  focusedEntityId: string | null;
  onToggleFocus: (id: string) => void;
  entityDiff?: EntityDiff | null;
}) {
  if (entity.channelPair) {
    return (
      <MergedEntityInspectorContent
        merged={entity}
        sections={[
          { label: "TX", entity: entity.channelPair.tx },
          { label: "RX", entity: entity.channelPair.rx },
        ]}
        focusedEntityId={focusedEntityId}
        onToggleFocus={onToggleFocus}
        entityDiff={entityDiff}
      />
    );
  }

  if (entity.rpcPair) {
    return (
      <MergedEntityInspectorContent
        merged={entity}
        sections={[
          { label: "REQ", entity: entity.rpcPair.req },
          { label: "RESP", entity: entity.rpcPair.resp },
        ]}
        focusedEntityId={focusedEntityId}
        onToggleFocus={onToggleFocus}
        entityDiff={entityDiff}
      />
    );
  }

  return (
    <EntityInspectorBody
      entity={entity}
      focusedEntityId={focusedEntityId}
      onToggleFocus={onToggleFocus}
      entityDiff={entityDiff}
    />
  );
}

function EntityInspectorBody({
  entity,
  focusedEntityId,
  onToggleFocus,
  entityDiff,
  showHeader = true,
}: {
  entity: EntityDef;
  focusedEntityId: string | null;
  onToggleFocus: (id: string) => void;
  entityDiff?: EntityDiff | null;
  showHeader?: boolean;
}) {
  const birthAbsolute = isFinite(entity.birthApproxUnixMs) && entity.birthApproxUnixMs > 0
    ? new Date(entity.birthApproxUnixMs).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "long" })
    : null;

  return (
    <>
      {showHeader && (
        <EntityInspectorHeader
          entity={entity}
          focusedEntityId={focusedEntityId}
          onToggleFocus={onToggleFocus}
        />
      )}

      {showHeader && (
        <div className="inspector-alert-slot">
          {entity.inCycle && (
            <div className="inspector-alert inspector-alert--crit">
              Part of <code>needs</code> cycle — possible deadlock
            </div>
          )}
        </div>
      )}

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
