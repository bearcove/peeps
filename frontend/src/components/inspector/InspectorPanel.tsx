import React from "react";
import { LinkSimple, X } from "@phosphor-icons/react";
import { ActionButton } from "../../ui/primitives/ActionButton";
import type { EntityDef, EdgeDef, ScopeDef } from "../../snapshot";
import type { UnionLayout } from "../../recording/unionGraph";
import { diffEntityBetweenFrames } from "../../recording/unionGraph";
import { canonicalNodeKind, kindIcon } from "../../nodeKindSpec";
import { scopeKindIcon } from "../../scopeKindSpec";
import { EntityInspectorContent } from "./EntityInspectorContent";
import { EdgeInspectorContent, EDGE_KIND_LABELS } from "./EdgeInspectorContent";
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
  selectedScope?: ScopeDef | null;
}) {
  let content: React.ReactNode;
  let titleIcon: React.ReactNode = null;
  let titleText = "Inspector";
  if (selection?.kind === "entity") {
    const entity = entityDefs.find((e) => e.id === selection.id);
    const entityDiff =
      entity && scrubbingUnionLayout && currentFrameIndex !== undefined && currentFrameIndex > 0
        ? diffEntityBetweenFrames(entity.id, currentFrameIndex, currentFrameIndex - 1, scrubbingUnionLayout)
        : null;
    if (entity) {
      titleIcon = kindIcon(canonicalNodeKind(entity.kind), 12);
      titleText = entity.name;
    }
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
    if (edge) {
      titleIcon = <LinkSimple size={12} weight="bold" />;
      titleText = EDGE_KIND_LABELS[edge.kind];
    }
    content = edge ? <EdgeInspectorContent edge={edge} entityDefs={entityDefs} /> : null;
  } else if (selectedScope) {
    titleIcon = scopeKindIcon(selectedScope.scopeKind, 12);
    titleText = selectedScope.scopeName || selectedScope.scopeId;
    content = <ScopeInspectorContent scope={selectedScope} />;
  } else if (selectedScopeKind) {
    titleIcon = scopeKindIcon(selectedScopeKind, 12);
    titleText = `${selectedScopeKind} scope`;
    content = <ScopeKindInspectorContent kind={selectedScopeKind} />;
  } else {
    content = <div className="inspector-empty">Select an entity or edge</div>;
  }

  return (
    <div className="inspector">
      <div className="inspector-header">
        <div className="inspector-header-drag-handle" onPointerDown={onHeaderPointerDown}>
          <span className="inspector-header-title">
            {titleIcon ? <span className="inspector-header-title-icon">{titleIcon}</span> : null}
            <span className="inspector-header-title-label">{titleText}</span>
          </span>
        </div>
        <ActionButton
          variant="ghost"
          onPress={onClose}
          aria-label="Close inspector"
          className="inspector-close-button"
        >
          <X size={14} weight="bold" />
        </ActionButton>
      </div>
      <div className="inspector-body">{content}</div>
    </div>
  );
}
