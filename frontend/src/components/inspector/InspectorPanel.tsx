import React from "react";
import { CaretLeft, CaretRight, MagnifyingGlass } from "@phosphor-icons/react";
import { ActionButton } from "../../ui/primitives/ActionButton";
import type { EntityDef, EdgeDef } from "../../snapshot";
import type { UnionLayout } from "../../recording/unionGraph";
import { diffEntityBetweenFrames } from "../../recording/unionGraph";
import { EntityInspectorContent } from "./EntityInspectorContent";
import { EdgeInspectorContent } from "./EdgeInspectorContent";
import { ScopeKindInspectorContent } from "./ScopeKindInspectorContent";
import type { GraphSelection } from "../graph/GraphPanel";
import "./InspectorPanel.css";

export function InspectorPanel({
  collapsed,
  onToggleCollapse,
  selection,
  entityDefs,
  edgeDefs,
  onFocusEntity,
  scrubbingUnionLayout,
  currentFrameIndex,
  selectedScopeKind,
}: {
  collapsed: boolean;
  onToggleCollapse: () => void;
  selection: GraphSelection;
  entityDefs: EntityDef[];
  edgeDefs: EdgeDef[];
  onFocusEntity: (id: string) => void;
  scrubbingUnionLayout?: UnionLayout;
  currentFrameIndex?: number;
  selectedScopeKind?: string | null;
}) {
  if (collapsed) {
    return (
      <button
        className="inspector inspector--collapsed"
        onClick={onToggleCollapse}
        title="Expand inspector"
      >
        <CaretLeft size={14} weight="bold" />
        <span className="inspector-collapsed-label">Inspector</span>
      </button>
    );
  }

  let content: React.ReactNode;
  if (selection?.kind === "entity") {
    const entity = entityDefs.find((e) => e.id === selection.id);
    const entityDiff =
      entity && scrubbingUnionLayout && currentFrameIndex !== undefined && currentFrameIndex > 0
        ? diffEntityBetweenFrames(entity.id, currentFrameIndex, currentFrameIndex - 1, scrubbingUnionLayout)
        : null;
    content = entity ? <EntityInspectorContent entity={entity} onFocus={onFocusEntity} entityDiff={entityDiff} /> : null;
  } else if (selection?.kind === "edge") {
    const edge = edgeDefs.find((e) => e.id === selection.id);
    content = edge ? <EdgeInspectorContent edge={edge} entityDefs={entityDefs} /> : null;
  } else if (selectedScopeKind) {
    content = <ScopeKindInspectorContent kind={selectedScopeKind} />;
  } else {
    content = <div className="inspector-empty">Select an entity or edge</div>;
  }

  return (
    <div className="inspector">
      <div className="inspector-header">
        <MagnifyingGlass size={14} weight="bold" />
        <span>Inspector</span>
        <ActionButton size="sm" onPress={onToggleCollapse} aria-label="Collapse inspector">
          <CaretRight size={14} weight="bold" />
        </ActionButton>
      </div>
      <div className="inspector-body">{content}</div>
    </div>
  );
}
