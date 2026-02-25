import React, { useEffect, useMemo, useState } from "react";
import type { FilterMenuItem } from "../../ui/primitives/FilterMenu";
import type { EntityDef, EdgeDef } from "../../snapshot";
import { layoutGraph, type SubgraphScopeMode } from "../../graph/elkAdapter";
import { measureGraphLayout } from "../../graph/render/NodeLayer";
import type { GraphGeometry } from "../../graph/geometry";
import { formatEntityPrimaryLabel, formatEntitySearchText } from "../../entityPresentation";
import type { ScopeColorPair } from "./scopeColors";
import { assignScopeColorRgbByKey } from "./scopeColors";
import type { FrameRenderResult } from "../../recording/unionGraph";
import { GraphFilterInput } from "./GraphFilterInput";
import { GraphViewport } from "./GraphViewport";
import { computeNodeSublabel } from "./graphNodeData";
import type { GraphFilterLabelMode } from "../../graphFilter";
import "./GraphPanel.css";

export type GraphSelection =
  | { kind: "entity"; id: string }
  | { kind: "edge"; id: string }
  | null;

export type SnapPhase = "idle" | "cutting" | "loading" | "ready" | "error";

export type ScopeColorMode = "none" | "process" | "crate";

function scopeKeyForEntity(entity: EntityDef, scopeColorMode: ScopeColorMode): string | undefined {
  if (scopeColorMode === "process") return entity.processId;
  if (scopeColorMode === "crate") return entity.topFrame?.crate_name ?? "~no-crate";
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
  expandedEntityId,
  onExpandedEntityChange,
  waitingForProcesses,
  crateItems,
  processItems,
  kindItems,
  moduleItems,
  scopeColorMode,
  subgraphScopeMode,
  labelByMode,
  showSource,
  scopeFilterLabel,
  onClearScopeFilter,
  unionFrameLayout,
  graphFilterText,
  onGraphFilterTextChange,
  onHideNodeFilter,
  onHideLocationFilter,
  onFocusConnected,
  onAppendFilterToken,
  floatingFilterBar = false,
}: {
  entityDefs: EntityDef[];
  edgeDefs: EdgeDef[];
  snapPhase: SnapPhase;
  selection: GraphSelection;
  onSelect: (sel: GraphSelection) => void;
  focusedEntityId: string | null;
  onExitFocus: () => void;
  expandedEntityId: string | null;
  onExpandedEntityChange: (id: string | null) => void;
  waitingForProcesses: boolean;
  crateItems: FilterMenuItem[];
  processItems: FilterMenuItem[];
  kindItems: FilterMenuItem[];
  moduleItems: FilterMenuItem[];
  scopeColorMode: ScopeColorMode;
  subgraphScopeMode: SubgraphScopeMode;
  labelByMode?: GraphFilterLabelMode;
  showSource?: boolean;
  scopeFilterLabel?: string | null;
  onClearScopeFilter?: () => void;
  unionFrameLayout?: FrameRenderResult;
  graphFilterText: string;
  onGraphFilterTextChange: (next: string) => void;
  onHideNodeFilter: (entityId: string) => void;
  onHideLocationFilter: (location: string) => void;
  onFocusConnected: (entityId: string) => void;
  onAppendFilterToken: (token: string) => void;
  floatingFilterBar?: boolean;
}) {
  const [layout, setLayout] = useState<GraphGeometry | null>(null);
  const expandedNodeIds = useMemo(
    () => (expandedEntityId ? new Set([expandedEntityId]) : new Set<string>()),
    [expandedEntityId],
  );
  // Transient: the node that is currently loading (fetching source + running ELK).
  const [expandingNodeId, setExpandingNodeId] = useState<string | null>(null);

  // Serialize expanded set for stable dependency tracking
  const expandedKey = [...expandedNodeIds].sort().join(",");

  useEffect(() => {
    if (unionFrameLayout) return;
    if (entityDefs.length === 0) return;
    // Mark the newly-requested node as "expanding" so the UI shows a spinner immediately.
    if (expandedEntityId) setExpandingNodeId(expandedEntityId);
    measureGraphLayout(entityDefs, subgraphScopeMode, labelByMode, showSource, expandedNodeIds)
      .then((measurements) =>
        layoutGraph(entityDefs, edgeDefs, measurements.nodeSizes, subgraphScopeMode, {
          subgraphHeaderHeight: measurements.subgraphHeaderHeight,
        }),
      )
      .then((geo) => {
        setExpandingNodeId(null);
        setLayout(geo);
      })
      .catch(console.error);
  // eslint-disable-next-line react-hooks/exhaustive-deps -- expandedKey is the serialized form of expandedNodeIds
  }, [entityDefs, edgeDefs, subgraphScopeMode, labelByMode, unionFrameLayout, showSource, expandedKey]);

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
      const sublabel = entity && labelByMode ? computeNodeSublabel(entity, labelByMode) : undefined;

      return {
        ...n,
        data: {
          ...n.data,
          scopeRgbLight: scopeRgb?.light,
          scopeRgbDark: scopeRgb?.dark,
          sublabel,
          showSource,
        },
      };
    });
  }, [effectiveGeometry, entityById, scopeColorByKey, scopeColorMode, labelByMode, showSource]);

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
  const focusItems = useMemo(
    () =>
      entityDefs.map((entity) => ({
        id: entity.id,
        label: formatEntityPrimaryLabel(entity),
        searchText: formatEntitySearchText(entity),
      })),
    [entityDefs],
  );
  const locationSuggestions = useMemo(
    () =>
      Array.from(
        new Set(
          entityDefs
            .map((entity) => {
              const tf = entity.topFrame;
              if (!tf) return null;
              return tf.line != null ? `${tf.source_file}:${tf.line}` : tf.source_file;
            })
            .filter((s): s is string => s != null),
        ),
      ),
    [entityDefs],
  );

  return (
    <div className={`graph-panel${floatingFilterBar ? " graph-panel--floating-filter" : ""}`}>
      <GraphFilterInput
        focusedEntityId={focusedEntityId}
        onExitFocus={onExitFocus}
        scopeFilterLabel={scopeFilterLabel}
        onClearScopeFilter={onClearScopeFilter}
        graphFilterText={graphFilterText}
        onGraphFilterTextChange={onGraphFilterTextChange}
        crateItems={crateItems}
        processItems={processItems}
        kindItems={kindItems}
        moduleItems={moduleItems}
        nodeIds={nodeSuggestions}
        locations={locationSuggestions}
        focusItems={focusItems}
      />
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
        onFocusConnected={onFocusConnected}
        onAppendFilterToken={onAppendFilterToken}
        ghostNodeIds={unionFrameLayout?.ghostNodeIds}
        ghostEdgeIds={unionFrameLayout?.ghostEdgeIds}
        expandedNodeId={expandedEntityId}
        expandingNodeId={expandingNodeId}
        onExpandedNodeChange={(id) => {
          if (!id) setExpandingNodeId(null);
          onExpandedEntityChange(id);
        }}
      />
    </div>
  );
}
