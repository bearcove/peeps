import React, { useEffect, useMemo, useState } from "react";
import type { FilterMenuItem } from "../../ui/primitives/FilterMenu";
import type { EntityDef, EdgeDef } from "../../snapshot";
import { layoutGraph, type SubgraphScopeMode } from "../../graph/elkAdapter";
import { measureGraphLayout } from "../../graph/render/NodeLayer";
import type { GraphGeometry } from "../../graph/geometry";
import type { ScopeColorPair } from "./scopeColors";
import { assignScopeColorRgbByKey } from "./scopeColors";
import type { FrameRenderResult } from "../../recording/unionGraph";
import { GraphFilterInput } from "./GraphFilterInput";
import { GraphViewport } from "./GraphViewport";
import "./GraphPanel.css";

export type GraphSelection =
  | { kind: "entity"; id: string }
  | { kind: "edge"; id: string }
  | null;

export type SnapPhase = "idle" | "cutting" | "loading" | "ready" | "error";

export type ScopeColorMode = "none" | "process" | "crate";

function scopeKeyForEntity(entity: EntityDef, scopeColorMode: ScopeColorMode): string | undefined {
  if (scopeColorMode === "process") return entity.processId;
  if (scopeColorMode === "crate") return entity.krate ?? "~no-crate";
  return undefined;
}

export function GraphPanel({
  entityDefs,
  edgeDefs,
  snapPhase,
  selection,
  onSelect,
  focusedEntityId,
  onExitFocus,
  waitingForProcesses,
  crateItems,
  processItems,
  kindItems,
  scopeColorMode,
  subgraphScopeMode,
  scopeFilterLabel,
  onClearScopeFilter,
  unionFrameLayout,
  graphFilterText,
  onGraphFilterTextChange,
  onHideNodeFilter,
  onHideLocationFilter,
}: {
  entityDefs: EntityDef[];
  edgeDefs: EdgeDef[];
  snapPhase: SnapPhase;
  selection: GraphSelection;
  onSelect: (sel: GraphSelection) => void;
  focusedEntityId: string | null;
  onExitFocus: () => void;
  waitingForProcesses: boolean;
  crateItems: FilterMenuItem[];
  processItems: FilterMenuItem[];
  kindItems: FilterMenuItem[];
  scopeColorMode: ScopeColorMode;
  subgraphScopeMode: SubgraphScopeMode;
  scopeFilterLabel?: string | null;
  onClearScopeFilter?: () => void;
  unionFrameLayout?: FrameRenderResult;
  graphFilterText: string;
  onGraphFilterTextChange: (next: string) => void;
  onHideNodeFilter: (entityId: string) => void;
  onHideLocationFilter: (location: string) => void;
}) {
  const [layout, setLayout] = useState<GraphGeometry | null>(null);

  useEffect(() => {
    if (unionFrameLayout) return;
    if (entityDefs.length === 0) return;
    measureGraphLayout(entityDefs, subgraphScopeMode)
      .then((measurements) =>
        layoutGraph(entityDefs, edgeDefs, measurements.nodeSizes, subgraphScopeMode, {
          subgraphHeaderHeight: measurements.subgraphHeaderHeight,
        }),
      )
      .then(setLayout)
      .catch(console.error);
  }, [entityDefs, edgeDefs, subgraphScopeMode, unionFrameLayout]);

  const effectiveGeometry: GraphGeometry | null = unionFrameLayout?.geometry ?? layout;
  const entityById = useMemo(() => new Map(entityDefs.map((entity) => [entity.id, entity])), [entityDefs]);
  const scopeColorByKey = useMemo<Map<string, ScopeColorPair>>(() => {
    if (scopeColorMode === "none") return new Map<string, ScopeColorPair>();
    return assignScopeColorRgbByKey(entityDefs.map((entity) => scopeKeyForEntity(entity, scopeColorMode) ?? ""));
  }, [entityDefs, scopeColorMode]);

  const nodesWithScopeColor = useMemo(() => {
    if (!effectiveGeometry) return [];
    return effectiveGeometry.nodes.map((n) => {
      const entity = entityById.get(n.id);
      const scopeKey = entity ? scopeKeyForEntity(entity, scopeColorMode) : undefined;
      const scopeRgb = scopeKey ? scopeColorByKey.get(scopeKey) : undefined;
      return {
        ...n,
        data: {
          ...n.data,
          scopeRgbLight: scopeRgb?.light,
          scopeRgbDark: scopeRgb?.dark,
        },
      };
    });
  }, [effectiveGeometry, entityById, scopeColorByKey, scopeColorMode]);

  const groupsWithScopeColor = useMemo(() => {
    if (!effectiveGeometry) return [];
    return effectiveGeometry.groups.map((group) => {
      const scopeKey = group.data?.scopeKey as string | undefined;
      const scopeRgb = scopeKey ? scopeColorByKey.get(scopeKey) : undefined;
      return {
        ...group,
        data: {
          ...group.data,
          scopeRgbLight: scopeRgb?.light,
          scopeRgbDark: scopeRgb?.dark,
        },
      };
    });
  }, [effectiveGeometry, scopeColorByKey]);

  const nodeSuggestions = useMemo(() => entityDefs.map((entity) => entity.id), [entityDefs]);
  const locationSuggestions = useMemo(
    () =>
      Array.from(
        new Set(
          entityDefs
            .map((entity) => entity.source?.trim() ?? "")
            .filter((source) => source.length > 0),
        ),
      ),
    [entityDefs],
  );

  const showToolbar =
    crateItems.length > 1 || processItems.length > 0 || kindItems.length > 1 || focusedEntityId || entityDefs.length > 0;

  return (
    <div className="graph-panel">
      {showToolbar && (
        <GraphFilterInput
          entityCount={entityDefs.length}
          edgeCount={edgeDefs.length}
          focusedEntityId={focusedEntityId}
          onExitFocus={onExitFocus}
          scopeFilterLabel={scopeFilterLabel}
          onClearScopeFilter={onClearScopeFilter}
          graphFilterText={graphFilterText}
          onGraphFilterTextChange={onGraphFilterTextChange}
          crateItems={crateItems}
          processItems={processItems}
          kindItems={kindItems}
          nodeIds={nodeSuggestions}
          locations={locationSuggestions}
        />
      )}
      <GraphViewport
        entityDefs={entityDefs}
        snapPhase={snapPhase}
        waitingForProcesses={waitingForProcesses}
        geometry={effectiveGeometry}
        groups={groupsWithScopeColor}
        nodes={nodesWithScopeColor}
        selection={selection}
        onSelect={onSelect}
        unionModeSuppressAutoFit={!!unionFrameLayout}
        entityById={entityById}
        onHideNodeFilter={onHideNodeFilter}
        onHideLocationFilter={onHideLocationFilter}
        ghostNodeIds={unionFrameLayout?.ghostNodeIds}
        ghostEdgeIds={unionFrameLayout?.ghostEdgeIds}
      />
    </div>
  );
}
