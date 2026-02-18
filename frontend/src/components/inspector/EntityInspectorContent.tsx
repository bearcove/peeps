import React from "react";
import { Timer, File, Crosshair, CaretRight } from "@phosphor-icons/react";
import { Badge } from "../../ui/primitives/Badge";
import { KeyValueRow } from "../../ui/primitives/KeyValueRow";
import { ActionButton } from "../../ui/primitives/ActionButton";
import { kindIcon, kindDisplayName } from "../../nodeKindSpec";
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

function inspectorKindLabel(entity: EntityDef): string {
  if (!entity.channelPair) return kindDisplayName(entity.kind);

  const body = entity.channelPair.tx.body;
  if (typeof body === "string") return kindDisplayName(entity.kind);
  if (!("channel_tx" in body) && !("channel_rx" in body)) return kindDisplayName(entity.kind);

  const ep = "channel_tx" in body ? body.channel_tx : body.channel_rx;
  const channelKind = "mpsc" in ep.details
    ? "mpsc"
    : "broadcast" in ep.details
      ? "broadcast"
      : "watch" in ep.details
        ? "watch"
        : "oneshot";
  return `${channelKind} channel`;
}

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

function DetailsInfoAffordance({ entity }: { entity: EntityDef }) {
  const [hovered, setHovered] = React.useState(false);
  const [pinned, setPinned] = React.useState(false);
  const birthAbsolute = isFinite(entity.birthApproxUnixMs) && entity.birthApproxUnixMs > 0
    ? new Date(entity.birthApproxUnixMs).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "long" })
    : null;
  const isOpen = hovered || pinned;

  return (
    <span
      className="inspector-details-popover-anchor"
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
        (i)
      </button>
      {isOpen && (
        <span className="inspector-tooltip" role="tooltip">
          <span className="inspector-tooltip-row">
            <span className="inspector-tooltip-label">Process</span>
            <span className="inspector-mono">{formatProcessLabel(entity.processName, entity.processPid)}</span>
          </span>
          <span className="inspector-tooltip-row">
            <span className="inspector-tooltip-label">Source</span>
            <Source source={entity.source} />
          </span>
          {entity.krate && (
            <span className="inspector-tooltip-row">
              <span className="inspector-tooltip-label">Crate</span>
              <span className="inspector-mono">{entity.krate}</span>
            </span>
          )}
          <span className="inspector-tooltip-row">
            <span className="inspector-tooltip-label">Birth</span>
            {birthAbsolute ? (
              <BirthTimestamp birthPtime={entity.birthPtime} ageMs={entity.ageMs} birthAbsolute={birthAbsolute} />
            ) : (
              <span className="inspector-mono">P+{entity.birthPtime}ms ({entity.ageMs}ms old)</span>
            )}
          </span>
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
      <span className="inspector-node-icon" title={inspectorKindLabel(entity)}>
        {kindIcon(entity.kind, 20)}
      </span>
      <div className="inspector-node-header-text">
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
      <EntityInspectorHeader entity={merged} focusedEntityId={focusedEntityId} onToggleFocus={onToggleFocus} />
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
            showHeader={false}
            showDetails={false}
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
  showHeader = true,
  showDetails = true,
  showScopeLinks = true,
  showMeta = true,
}: {
  entity: EntityDef;
  focusedEntityId: string | null;
  onToggleFocus: (id: string) => void;
  onOpenScopeKind?: (kind: string) => void;
  entityDiff?: EntityDiff | null;
  showHeader?: boolean;
  showDetails?: boolean;
  showScopeLinks?: boolean;
  showMeta?: boolean;
}) {
  const [detailsExpanded, setDetailsExpanded] = React.useState(false);
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

      {showHeader && entity.inCycle && (
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

      {showDetails && (
        <div className="inspector-section">
          <button
            type="button"
            className="inspector-disclosure"
            onClick={() => setDetailsExpanded((v) => !v)}
            aria-expanded={detailsExpanded}
          >
            <CaretRight
              size={12}
              weight="bold"
              className={detailsExpanded ? "inspector-disclosure-caret inspector-disclosure-caret--expanded" : "inspector-disclosure-caret"}
            />
            <span>Details</span>
          </button>
          {detailsExpanded && (
            <div className="inspector-details">
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
          )}
        </div>
      )}

      {showScopeLinks && <EntityScopeLinksSection entity={entity} onOpenScopeKind={onOpenScopeKind} />}
      <EntityBodySection entity={entity} />
      {showMeta && <MetaSection meta={entity.meta} />}
    </>
  );
}
