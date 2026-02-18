import React from "react";
import { Crosshair, Info } from "@phosphor-icons/react";
import { Badge } from "../../ui/primitives/Badge";
import { ActionButton } from "../../ui/primitives/ActionButton";
import { Popover } from "../../ui/primitives/Popover";
import { formatProcessLabel } from "../../processLabel";
import type { EntityDef } from "../../snapshot";
import type { EntityDiff } from "../../recording/unionGraph";
import { EntityBodySection } from "./EntityBodySection";
import { EntityScopeLinksSection } from "./EntityScopeLinksSection";
import { MetaSection } from "./MetaTree";
import { Source } from "./Source";
import "./InspectorPanel.css";

type MergedSection = {
  label: string;
  entity: EntityDef;
};

function DetailsInfoAffordance({
  entity,
  align = "start",
}: {
  entity: EntityDef;
  align?: "start" | "end";
}) {
  const [hovered, setHovered] = React.useState(false);
  const [pinned, setPinned] = React.useState(false);
  const anchorRef = React.useRef<HTMLSpanElement>(null);
  const birthAbsolute = isFinite(entity.birthApproxUnixMs) && entity.birthApproxUnixMs > 0
    ? new Date(entity.birthApproxUnixMs).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "long" })
    : null;
  const isOpen = hovered || pinned;

  return (
    <span
      ref={anchorRef}
      className={[
        "inspector-details-popover-anchor",
        align === "end" && "inspector-details-popover-anchor--end",
      ].filter(Boolean).join(" ")}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
    >
      <button
        type="button"
        className={["inspector-details-trigger", pinned && "inspector-details-trigger--pinned"].filter(Boolean).join(" ")}
        onClick={() => setPinned((v) => !v)}
        aria-expanded={isOpen}
        aria-label="Show details"
        title="Details"
      >
        <Info size={14} weight={pinned ? "fill" : "bold"} />
      </button>
      <Popover
        open={isOpen}
        anchorRef={anchorRef}
        onClose={() => {
          setHovered(false);
          setPinned(false);
        }}
        side="bottom"
        align={align}
        className="inspector-tooltip"
      >
        <span role="tooltip">
          <span className="inspector-tooltip-row">
            <span className="inspector-tooltip-label">Process</span>
            <span className="inspector-tooltip-value">{formatProcessLabel(entity.processName, entity.processPid)}</span>
          </span>
          <span className="inspector-tooltip-row">
            <span className="inspector-tooltip-label">Source</span>
            <span className="inspector-tooltip-value"><Source source={entity.source} /></span>
          </span>
          {entity.krate && (
            <span className="inspector-tooltip-row">
              <span className="inspector-tooltip-label">Crate</span>
              <span className="inspector-tooltip-value">{entity.krate}</span>
            </span>
          )}
          <span className="inspector-tooltip-row">
            <span className="inspector-tooltip-label">Birth</span>
            <span className="inspector-tooltip-value">
              P+{entity.birthPtime}ms ({entity.ageMs}ms old)
              {birthAbsolute && (
                <span className="inspector-tooltip-subvalue">{birthAbsolute}</span>
              )}
            </span>
          </span>
        </span>
      </Popover>
    </span>
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
      <DetailsInfoAffordance entity={entity} align="start" />
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
            <DetailsInfoAffordance entity={section.entity} />
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
  focusedEntityId,
  onToggleFocus,
  onOpenScopeKind,
  entityDiff,
}: {
  entity: EntityDef;
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
      focusedEntityId={focusedEntityId}
      onToggleFocus={onToggleFocus}
      onOpenScopeKind={onOpenScopeKind}
      entityDiff={entityDiff}
    />
  );
}

function EntityInspectorBody({
  entity,
  focusedEntityId,
  onToggleFocus,
  onOpenScopeKind,
  entityDiff,
  showActions = true,
  showScopeLinks = true,
  showMeta = true,
}: {
  entity: EntityDef;
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
            Part of <code>needs</code> cycle — possible deadlock
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

      {showScopeLinks && <EntityScopeLinksSection entity={entity} onOpenScopeKind={onOpenScopeKind} />}
      <EntityBodySection entity={entity} />
      {showMeta && <MetaSection meta={entity.meta} />}
    </>
  );
}
