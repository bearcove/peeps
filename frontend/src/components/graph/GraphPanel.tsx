import React, { useEffect, useMemo, useRef, useState } from "react";
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
import { computeNodeSublabel, graphNodeDataFromEntity } from "./graphNodeData";
import type { GraphFilterLabelMode } from "../../graphFilter";
import { cachedFetchSourcePreviews } from "../../api/sourceCache";
import "./GraphPanel.css";

export type GraphSelection = { kind: "entity"; id: string } | { kind: "edge"; id: string } | null;

export type SnapPhase = "idle" | "cutting" | "loading" | "ready" | "error";

export type ScopeColorMode = "none" | "process" | "crate";

function scopeKeyForEntity(entity: EntityDef, scopeColorMode: ScopeColorMode): string | undefined {
  if (scopeColorMode === "process") return entity.processId;
  if (scopeColorMode === "crate") return entity.topFrame?.crate_name ?? "~no-crate";
  return undefined;
}

function collectExpandedFrameIds(
  defs: EntityDef[],
  expandedNodeIds: Set<string>,
  showSource?: boolean,
): Set<number> {
  if (!showSource || expandedNodeIds.size === 0) return new Set<number>();
  const frameIds = new Set<number>();
  for (const def of defs) {
    if (!expandedNodeIds.has(def.id)) continue;
    const skipEntryFrames = "future" in def.body ? (def.body.future.skip_entry_frames ?? 0) : 0;
    for (const frame of def.frames.slice(skipEntryFrames)) {
      if (frame.frame_id != null) frameIds.add(frame.frame_id);
    }
  }
  return frameIds;
}

async function preloadExpandedSourcePreviews(
  defs: EntityDef[],
  expandedNodeIds: Set<string>,
  showSource?: boolean,
): Promise<void> {
  const frameIds = collectExpandedFrameIds(defs, expandedNodeIds, showSource);
  if (frameIds.size === 0) return;
  await cachedFetchSourcePreviews([...frameIds]);
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
  const [prevLayout, setPrevLayout] = useState<GraphGeometry | null>(null);
  const layoutRef = useRef<GraphGeometry | null>(null);
  const layoutRunIdRef = useRef(0);
  const lastLaidOutExpandedIdRef = useRef<string | null>(null);
  const lastLayoutInputsKeyRef = useRef<string>("");
  const activeLayoutRunRef = useRef<{ runId: number; key: string } | null>(null);
  layoutRef.current = layout;
  const expandedNodeIds = useMemo(
    () => (expandedEntityId ? new Set([expandedEntityId]) : new Set<string>()),
    [expandedEntityId],
  );
  // Transient: the node that is currently loading (fetching source + running ELK).
  const [expandingNodeId, setExpandingNodeId] = useState<string | null>(null);

  // Serialize expanded set for stable dependency tracking
  const expandedKey = [...expandedNodeIds].sort().join(",");
  const layoutInputsKey = useMemo(() => {
    const entityKey = entityDefs
      .map((entity) => `${entity.id}:${entity.name}:${entity.kind}`)
      .join("|");
    const edgeKey = edgeDefs.map((edge) => `${edge.id}:${edge.source}->${edge.target}`).join("|");
    return `${entityKey}::${edgeKey}::${subgraphScopeMode}::${labelByMode ?? ""}::${showSource ? "1" : "0"}::${expandedKey}`;
  }, [entityDefs, edgeDefs, subgraphScopeMode, labelByMode, showSource, expandedKey]);

  useEffect(() => {
    if (unionFrameLayout) {
      setExpandingNodeId(null);
      return;
    }
    if (entityDefs.length === 0) return;
    if (layoutInputsKey === lastLayoutInputsKeyRef.current) return;
    if (activeLayoutRunRef.current?.key === layoutInputsKey) return;
    let cancelled = false;
    const runId = ++layoutRunIdRef.current;
    activeLayoutRunRef.current = { runId, key: layoutInputsKey };
    const clearActiveRun = () => {
      if (activeLayoutRunRef.current?.runId === runId) activeLayoutRunRef.current = null;
    };
    const runExpandedEntityId = expandedEntityId;
    const shouldEnterExpanding =
      runExpandedEntityId != null && runExpandedEntityId !== lastLaidOutExpandedIdRef.current;
    // Mark the newly-requested node as "expanding" so the UI shows a spinner immediately.
    if (shouldEnterExpanding) setExpandingNodeId(runExpandedEntityId);

    void preloadExpandedSourcePreviews(entityDefs, expandedNodeIds, showSource)
      .then(() => {
        if (cancelled || layoutRunIdRef.current !== runId) return null;
        return measureGraphLayout(
          entityDefs,
          subgraphScopeMode,
          labelByMode,
          showSource,
          expandedNodeIds,
        );
      })
      .then((measurements) => {
        if (!measurements) return null;
        if (cancelled || layoutRunIdRef.current !== runId) return null;
        return layoutGraph(entityDefs, edgeDefs, measurements.nodeSizes, subgraphScopeMode, {
          subgraphHeaderHeight: measurements.subgraphHeaderHeight,
        });
      })
      .then((geo) => {
        if (!geo) return;
        if (cancelled || layoutRunIdRef.current !== runId) return;
        const invalidNode = geo.nodes.find((node) => {
          const { width, height, x, y } = node.worldRect;
          return (
            !Number.isFinite(x) ||
            !Number.isFinite(y) ||
            !Number.isFinite(width) ||
            !Number.isFinite(height) ||
            width <= 0 ||
            height <= 0
          );
        });
        if (invalidNode) {
          throw new Error(
            `[graph-layout] refusing geometry with invalid node size ${invalidNode.id}: ${invalidNode.worldRect.width}x${invalidNode.worldRect.height}`,
          );
        }
        // Save current layout as prev for FLIP animation before updating
        setPrevLayout(layoutRef.current);
        setLayout(geo);
        lastLayoutInputsKeyRef.current = layoutInputsKey;
        lastLaidOutExpandedIdRef.current = runExpandedEntityId;
        clearActiveRun();
        if (shouldEnterExpanding) {
          setExpandingNodeId((current) => (current === runExpandedEntityId ? null : current));
        }
      })
      .catch((error) => {
        if (cancelled || layoutRunIdRef.current !== runId) return;
        clearActiveRun();
        if (shouldEnterExpanding) {
          setExpandingNodeId((current) => (current === runExpandedEntityId ? null : current));
        }
        console.error(error);
      });

    return () => {
      cancelled = true;
      clearActiveRun();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- expandedKey is the serialized form of expandedNodeIds
  }, [
    entityDefs,
    edgeDefs,
    subgraphScopeMode,
    labelByMode,
    unionFrameLayout,
    showSource,
    expandedKey,
    layoutInputsKey,
  ]);

  const effectiveGeometry: GraphGeometry | null = unionFrameLayout?.geometry ?? layout;
  const effectivePrevGeometry: GraphGeometry | null = unionFrameLayout ? null : prevLayout;
  const entityById = useMemo(
    () => new Map(entityDefs.map((entity) => [entity.id, entity])),
    [entityDefs],
  );

  const scopeColorByKey = useMemo<Map<string, ScopeColorPair>>(() => {
    if (scopeColorMode === "none") return new Map<string, ScopeColorPair>();
    return assignScopeColorRgbByKey(
      entityDefs.map((entity) => scopeKeyForEntity(entity, scopeColorMode) ?? ""),
    );
  }, [entityDefs, scopeColorMode]);

  const nodesWithScopeColor = useMemo(() => {
    if (!effectiveGeometry) return [];
    return effectiveGeometry.nodes.map((n) => {
      const entity = entityById.get(n.id);
      const scopeKey = entity ? scopeKeyForEntity(entity, scopeColorMode) : undefined;
      const scopeRgb = scopeKey ? scopeColorByKey.get(scopeKey) : undefined;
      const sublabel = entity && labelByMode ? computeNodeSublabel(entity, labelByMode) : undefined;
      const liveNodeData = entity ? graphNodeDataFromEntity(entity) : n.data;

      return {
        ...n,
        data: {
          ...liveNodeData,
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
        prevNodes={effectivePrevGeometry?.nodes}
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
          setExpandingNodeId(id);
          onExpandedEntityChange(id);
        }}
      />
    </div>
  );
}
