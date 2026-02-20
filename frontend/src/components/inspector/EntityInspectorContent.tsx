import React from "react";
import { Crosshair, Package } from "@phosphor-icons/react";
import { NodeChip } from "../../ui/primitives/NodeChip";
import { Badge } from "../../ui/primitives/Badge";
import { ActionButton } from "../../ui/primitives/ActionButton";
import { KeyValueRow } from "../../ui/primitives/KeyValueRow";
import { DurationDisplay } from "../../ui/primitives/DurationDisplay";
import type { EntityDef } from "../../snapshot";
import type { EntityDiff } from "../../recording/unionGraph";
import type { ResolvedSnapshotBacktrace } from "../../snapshot";
import { EntityBodySection } from "./EntityBodySection";
import { EntityScopeLinksSection } from "./EntityScopeLinksSection";
import { MetaSection } from "./MetaTree";
import { BacktraceRenderer } from "./BacktraceRenderer";
import { Source } from "./Source";
import "./InspectorPanel.css";

type MergedSection = {
  label: string;
  entity: EntityDef;
};

function EntityDetailsSection({
  entity,
  backtrace,
}: {
  entity: EntityDef;
  backtrace?: ResolvedSnapshotBacktrace;
}) {
  const birthAbsolute =
    isFinite(entity.birthApproxUnixMs) && entity.birthApproxUnixMs > 0
      ? new Date(entity.birthApproxUnixMs).toLocaleString(undefined, {
          dateStyle: "medium",
          timeStyle: "long",
        })
      : null;
  const birthTitle = [
    `P+${entity.birthPtime}ms into process`,
    birthAbsolute,
  ].filter(Boolean).join(" · ");

  return (
    <>
      <KeyValueRow label="Backtrace ID">
        <span className="inspector-mono">{entity.backtraceId}</span>
      </KeyValueRow>
      <KeyValueRow label="Source">
        <Source source={`${entity.source.path}:${entity.source.line}`} />
      </KeyValueRow>
      {entity.topFrame?.crate_name && (
        <KeyValueRow label="Crate">
          <NodeChip
            icon={<Package size={12} weight="bold" />}
            label={entity.topFrame.crate_name}
          />
        </KeyValueRow>
      )}
      <KeyValueRow label="Age">
        <span title={birthTitle}>
          <DurationDisplay ms={entity.ageMs} />
        </span>
      </KeyValueRow>
      {backtrace && (
        <div className="inspector-backtrace-slot">
          <BacktraceRenderer backtrace={backtrace} />
        </div>
      )}
    </>
  );
}

function EntityInspectorActions({
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
    <div className="inspector-entity-actions">
      <ActionButton size="sm" className="inspector-focus-button" onPress={() => onToggleFocus(entity.id)}>
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
  onOpenScopeKind,
  entityDiff,
}: {
  merged: EntityDef;
  sections: readonly MergedSection[];
  focusedEntityId: string | null;
  onToggleFocus: (id: string) => void;
  onOpenScopeKind?: (kind: string) => void;
  entityDiff?: EntityDiff | null;
}) {
  return (
    <>
      <EntityInspectorActions entity={merged} focusedEntityId={focusedEntityId} onToggleFocus={onToggleFocus} />
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
      <EntityScopeLinksSection entity={merged} onOpenScopeKind={onOpenScopeKind} />
      {sections.map((section) => (
        <fieldset className="inspector-lane-card" key={`${merged.id}:${section.label}`}>
          <legend className="inspector-lane-legend">
            <span>{section.label}</span>
          </legend>
          <EntityInspectorBody
            entity={section.entity}
            focusedEntityId={focusedEntityId}
            onToggleFocus={onToggleFocus}
            onOpenScopeKind={onOpenScopeKind}
            showActions={false}
            showScopeLinks={false}
            showMeta={false}
          />
        </fieldset>
      ))}
    </>
  );
}

export function EntityInspectorContent({
  entity,
  backtrace,
  focusedEntityId,
  onToggleFocus,
  onOpenScopeKind,
  entityDiff,
}: {
  entity: EntityDef;
  backtrace?: ResolvedSnapshotBacktrace;
  focusedEntityId: string | null;
  onToggleFocus: (id: string) => void;
  onOpenScopeKind?: (kind: string) => void;
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
        onOpenScopeKind={onOpenScopeKind}
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
        onOpenScopeKind={onOpenScopeKind}
        entityDiff={entityDiff}
      />
    );
  }

  return (
    <EntityInspectorBody
      entity={entity}
      backtrace={backtrace}
      focusedEntityId={focusedEntityId}
      onToggleFocus={onToggleFocus}
      onOpenScopeKind={onOpenScopeKind}
      entityDiff={entityDiff}
    />
  );
}

function EntityInspectorBody({
  entity,
  backtrace,
  focusedEntityId,
  onToggleFocus,
  onOpenScopeKind,
  entityDiff,
  showActions = true,
  showScopeLinks = true,
  showMeta = true,
}: {
  entity: EntityDef;
  backtrace?: ResolvedSnapshotBacktrace;
  focusedEntityId: string | null;
  onToggleFocus: (id: string) => void;
  onOpenScopeKind?: (kind: string) => void;
  entityDiff?: EntityDiff | null;
  showActions?: boolean;
  showScopeLinks?: boolean;
  showMeta?: boolean;
}) {
  return (
    <>
      {showActions && (
        <EntityInspectorActions
          entity={entity}
          focusedEntityId={focusedEntityId}
          onToggleFocus={onToggleFocus}
        />
      )}

      {showActions && entity.inCycle && (
        <div className="inspector-alert-slot">
          <div className="inspector-alert inspector-alert--crit">
            Part of <code>waiting_on</code> cycle — possible deadlock
          </div>
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

      <div className="inspector-kv-table">
        <EntityDetailsSection entity={entity} backtrace={backtrace} />
        <EntityBodySection entity={entity} />
      </div>
      {showScopeLinks && <EntityScopeLinksSection entity={entity} onOpenScopeKind={onOpenScopeKind} />}
      {showMeta && <MetaSection meta={entity.meta} />}
    </>
  );
}
