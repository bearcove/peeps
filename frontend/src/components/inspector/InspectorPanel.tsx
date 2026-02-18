import React from "react";
import { X } from "@phosphor-icons/react";
import { ActionButton } from "../../ui/primitives/ActionButton";
import type { EntityDef, EdgeDef } from "../../snapshot";
import type { UnionLayout } from "../../recording/unionGraph";
import type { ScopeTableRow } from "../scopes/ScopeTablePanel";
import { diffEntityBetweenFrames } from "../../recording/unionGraph";
import { EntityInspectorContent } from "./EntityInspectorContent";
import { EdgeInspectorContent } from "./EdgeInspectorContent";
import { ScopeInspectorContent } from "./ScopeInspectorContent";
import { ScopeKindInspectorContent } from "./ScopeKindInspectorContent";
import type { GraphSelection } from "../graph/GraphPanel";
import "./InspectorPanel.css";

export function InspectorPanel({
  onClose,
  onHeaderPointerDown,
  selection,
  entityDefs,
  edgeDefs,
  focusedEntityId,
  onToggleFocusEntity,
  onOpenScopeKind,
  scrubbingUnionLayout,
  currentFrameIndex,
  selectedScopeKind,
  selectedScope,
}: {
  onClose: () => void;
  onHeaderPointerDown?: (event: React.PointerEvent<HTMLDivElement>) => void;
  selection: GraphSelection;
  entityDefs: EntityDef[];
  edgeDefs: EdgeDef[];
  focusedEntityId: string | null;
  onToggleFocusEntity: (id: string) => void;
  onOpenScopeKind: (kind: string) => void;
  scrubbingUnionLayout?: UnionLayout;
  currentFrameIndex?: number;
  selectedScopeKind?: string | null;
  selectedScope?: ScopeTableRow | null;
}) {
  let content: React.ReactNode;
  if (selection?.kind === "entity") {
    const entity = entityDefs.find((e) => e.id === selection.id);
    const entityDiff =
      entity && scrubbingUnionLayout && currentFrameIndex !== undefined && currentFrameIndex > 0
        ? diffEntityBetweenFrames(entity.id, currentFrameIndex, currentFrameIndex - 1, scrubbingUnionLayout)
        : null;
    content = entity ? (
      <EntityInspectorContent
        entity={entity}
        focusedEntityId={focusedEntityId}
        onToggleFocus={onToggleFocusEntity}
        onOpenScopeKind={onOpenScopeKind}
        entityDiff={entityDiff}
      />
    ) : null;
  } else if (selection?.kind === "edge") {
    const edge = edgeDefs.find((e) => e.id === selection.id);
    content = edge ? <EdgeInspectorContent edge={edge} entityDefs={entityDefs} /> : null;
  } else if (selectedScope) {
    content = <ScopeInspectorContent scope={selectedScope} />;
  } else if (selectedScopeKind) {
    content = <ScopeKindInspectorContent kind={selectedScopeKind} />;
  } else {
    content = <div className="inspector-empty">Select an entity or edge</div>;
  }

  return (
    <div className="inspector">
      <div className="inspector-header">
        <div className="inspector-header-drag-handle" onPointerDown={onHeaderPointerDown}>
          <span className="inspector-header-title">Inspector</span>
        </div>
        <ActionButton size="sm" onPress={onClose} aria-label="Close inspector">
          <X size={14} weight="bold" />
        </ActionButton>
      </div>
      <div className="inspector-body">{content}</div>
    </div>
  );
}
