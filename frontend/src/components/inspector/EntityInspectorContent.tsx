import React, { useState } from "react";
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
import { ContextMenu, ContextMenuItem, ContextMenuSeparator } from "../../ui/primitives/ContextMenu";
import { quoteFilterValue } from "../../graphFilter";
import "./InspectorPanel.css";

type MergedSection = {
  label: string;
  entity: EntityDef;
};

function EntityDetailsSection({
  entity,
  backtrace,
  onAppendFilterToken,
  openBacktraceTrigger,
}: {
  entity: EntityDef;
  backtrace?: ResolvedSnapshotBacktrace;
  onAppendFilterToken?: (token: string) => void;
  openBacktraceTrigger?: number;
}) {
  const [crateMenu, setCrateMenu] = useState<{ x: number; y: number } | null>(null);

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

  const crate = entity.topFrame?.crate_name;

  return (
    <>
      <KeyValueRow label="Created at">
        {backtrace
          ? <BacktraceRenderer backtrace={backtrace} openTrigger={openBacktraceTrigger} />
          : <NodeChip label={`${entity.source.path.split("/").pop() ?? entity.source.path}:${entity.source.line}`} href={`zed://file${entity.source.path}:${entity.source.line}`} />
        }
      </KeyValueRow>
      {crate && (
        <KeyValueRow label="Crate">
          <NodeChip
            icon={<Package size={12} weight="bold" />}
            label={crate}
            onContextMenu={onAppendFilterToken ? (e) => {
              e.preventDefault();
              setCrateMenu({ x: e.clientX, y: e.clientY });
            } : undefined}
          />
          {crateMenu && onAppendFilterToken && (
            <ContextMenu x={crateMenu.x} y={crateMenu.y} onClose={() => setCrateMenu(null)}>
              <ContextMenuItem prefix="+" onClick={() => { onAppendFilterToken(`+crate:${quoteFilterValue(crate)}`); setCrateMenu(null); }}>
                Show only crate
              </ContextMenuItem>
              <ContextMenuItem prefix="−" onClick={() => { onAppendFilterToken(`-crate:${quoteFilterValue(crate)}`); setCrateMenu(null); }}>
                Hide crate
              </ContextMenuItem>
              <ContextMenuSeparator />
              <ContextMenuItem onClick={() => { onAppendFilterToken("colorBy:crate"); setCrateMenu(null); }}>
                Color by crates
              </ContextMenuItem>
              <ContextMenuItem onClick={() => { onAppendFilterToken("groupBy:crate"); setCrateMenu(null); }}>
                Group by crates
              </ContextMenuItem>
            </ContextMenu>
          )}
        </KeyValueRow>
      )}
      <KeyValueRow label="Age">
        <span title={birthTitle}>
          <DurationDisplay ms={entity.ageMs} />
        </span>
      </KeyValueRow>
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
  onAppendFilterToken,
  entityDiff,
}: {
  merged: EntityDef;
  sections: readonly MergedSection[];
  focusedEntityId: string | null;
  onToggleFocus: (id: string) => void;
  onOpenScopeKind?: (kind: string) => void;
  onAppendFilterToken?: (token: string) => void;
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
            onAppendFilterToken={onAppendFilterToken}
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
  onAppendFilterToken,
  entityDiff,
  openBacktraceTrigger,
}: {
  entity: EntityDef;
  backtrace?: ResolvedSnapshotBacktrace;
  focusedEntityId: string | null;
  onToggleFocus: (id: string) => void;
  onOpenScopeKind?: (kind: string) => void;
  onAppendFilterToken?: (token: string) => void;
  entityDiff?: EntityDiff | null;
  openBacktraceTrigger?: number;
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
        onAppendFilterToken={onAppendFilterToken}
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
        onAppendFilterToken={onAppendFilterToken}
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
      onAppendFilterToken={onAppendFilterToken}
      entityDiff={entityDiff}
      openBacktraceTrigger={openBacktraceTrigger}
    />
  );
}

function EntityInspectorBody({
  entity,
  backtrace,
  focusedEntityId,
  onToggleFocus,
  onOpenScopeKind,
  onAppendFilterToken,
  entityDiff,
  showActions = true,
  showScopeLinks = true,
  showMeta = true,
  openBacktraceTrigger,
}: {
  entity: EntityDef;
  backtrace?: ResolvedSnapshotBacktrace;
  focusedEntityId: string | null;
  onToggleFocus: (id: string) => void;
  onOpenScopeKind?: (kind: string) => void;
  onAppendFilterToken?: (token: string) => void;
  entityDiff?: EntityDiff | null;
  showActions?: boolean;
  showScopeLinks?: boolean;
  showMeta?: boolean;
  openBacktraceTrigger?: number;
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
        <EntityDetailsSection entity={entity} backtrace={backtrace} onAppendFilterToken={onAppendFilterToken} openBacktraceTrigger={openBacktraceTrigger} />
        <EntityBodySection entity={entity} />
      </div>
      {showScopeLinks && <EntityScopeLinksSection entity={entity} onOpenScopeKind={onOpenScopeKind} />}
      {showMeta && <MetaSection meta={entity.meta} />}
    </>
  );
}
