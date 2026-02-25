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
import { computeNodeSublabel } from "./graphNodeData";
import type { GraphFilterLabelMode } from "../../graphFilter";
import { collapsedFrameCount } from "./GraphNode";
import { canonicalNodeKind } from "../../nodeKindSpec";
import {
  cachedFetchSourcePreview,
  getSourceLineSync,
  getSourcePreviewSync,
} from "../../api/sourceCache";
import type { SourcePreviewResponse } from "../../api/types.generated";
import "./GraphPanel.css";

export type GraphSelection = { kind: "entity"; id: string } | { kind: "edge"; id: string } | null;

export type SnapPhase = "idle" | "cutting" | "loading" | "ready" | "error";

export type ScopeColorMode = "none" | "process" | "crate";

function scopeKeyForEntity(entity: EntityDef, scopeColorMode: ScopeColorMode): string | undefined {
  if (scopeColorMode === "process") return entity.processId;
  if (scopeColorMode === "crate") return entity.topFrame?.crate_name ?? "~no-crate";
  return undefined;
}

function collectRenderedFrameIdsForSourcePreviews(
  defs: EntityDef[],
  expandedNodeIds: Set<string>,
  showSource?: boolean,
): Set<number> {
  const collapsedFrameIdsByNode = collectCollapsedSourceFrameIdsByNode(defs, showSource);
  const frameIds = new Set<number>();
  for (const def of defs) {
    const isExpanded = expandedNodeIds.has(def.id);
    const skipEntryFrames = "future" in def.body ? (def.body.future.skip_entry_frames ?? 0) : 0;

    if (isExpanded && showSource) {
      for (const frame of def.frames.slice(skipEntryFrames)) {
        if (frame.frame_id != null) frameIds.add(frame.frame_id);
      }
    }
  }
  for (const ids of collapsedFrameIdsByNode.values()) {
    for (const frameId of ids) frameIds.add(frameId);
  }
  return frameIds;
}

function collectCollapsedSourceFrameIdsByNode(
  defs: EntityDef[],
  showSource?: boolean,
): Map<string, number[]> {
  const frameIdsByNode = new Map<string, number[]>();
  for (const def of defs) {
    const needsCollapsedSource = showSource || canonicalNodeKind(def.kind) === "future";
    if (!needsCollapsedSource) continue;
    const skipEntryFrames = "future" in def.body ? (def.body.future.skip_entry_frames ?? 0) : 0;
    const effectiveFrames = skipEntryFrames > 0 ? def.frames.slice(skipEntryFrames) : def.frames;
    const collapsedFrames = effectiveFrames.slice(0, collapsedFrameCount(def.kind));
    const frameIds = collapsedFrames
      .map((frame) => frame.frame_id)
      .filter((frameId): frameId is number => frameId != null);
    if (frameIds.length > 0) frameIdsByNode.set(def.id, frameIds);
  }
  return frameIdsByNode;
}

async function preloadExpandedSourcePreviews(
  defs: EntityDef[],
  expandedNodeIds: Set<string>,
  showSource?: boolean,
): Promise<void> {
  const frameIds = collectRenderedFrameIdsForSourcePreviews(defs, expandedNodeIds, showSource);
  if (frameIds.size === 0) return;
  await Promise.all([...frameIds].map((frameId) => cachedFetchSourcePreview(frameId)));
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
  const sourceLoadRunIdRef = useRef(0);
  layoutRef.current = layout;
  const expandedNodeIds = useMemo(
    () => (expandedEntityId ? new Set([expandedEntityId]) : new Set<string>()),
    [expandedEntityId],
  );
  // Transient: the node that is currently loading (fetching source + running ELK).
  const [expandingNodeId, setExpandingNodeId] = useState<string | null>(null);
  const [sourceLoadingNodeIds, setSourceLoadingNodeIds] = useState<Set<string>>(new Set());
  const [sourcePreviewVersion, setSourcePreviewVersion] = useState(0);

  // Serialize expanded set for stable dependency tracking
  const expandedKey = [...expandedNodeIds].sort().join(",");

  useEffect(() => {
    if (entityDefs.length === 0) {
      setSourceLoadingNodeIds(new Set());
      return;
    }

    let cancelled = false;
    const runId = ++sourceLoadRunIdRef.current;
    const collapsedFrameIdsByNode = collectCollapsedSourceFrameIdsByNode(entityDefs, showSource);
    const allRenderedFrameIds = collectRenderedFrameIdsForSourcePreviews(
      entityDefs,
      expandedNodeIds,
      showSource,
    );
    const missingPreviewIds = [...allRenderedFrameIds].filter(
      (frameId) => !getSourcePreviewSync(frameId),
    );

    const loadingNodeIds = new Set<string>();
    for (const [nodeId, frameIds] of collapsedFrameIdsByNode.entries()) {
      if (frameIds.some((frameId) => getSourceLineSync(frameId) == null)) {
        loadingNodeIds.add(nodeId);
      }
    }
    setSourceLoadingNodeIds(loadingNodeIds);

    if (missingPreviewIds.length === 0) {
      return;
    }

    void Promise.allSettled(
      missingPreviewIds.map((frameId) => cachedFetchSourcePreview(frameId)),
    ).then(() => {
      if (cancelled || sourceLoadRunIdRef.current !== runId) return;
      const nextLoadingNodeIds = new Set<string>();
      for (const [nodeId, frameIds] of collapsedFrameIdsByNode.entries()) {
        if (frameIds.some((frameId) => getSourceLineSync(frameId) == null)) {
          nextLoadingNodeIds.add(nodeId);
        }
      }
      setSourceLoadingNodeIds(nextLoadingNodeIds);
      setSourcePreviewVersion((v) => v + 1);
    });

    return () => {
      cancelled = true;
    };
  }, [entityDefs, showSource, expandedKey, expandedNodeIds]);

  useEffect(() => {
    if (unionFrameLayout) {
      setExpandingNodeId(null);
      return;
    }
    if (entityDefs.length === 0) return;
    let cancelled = false;
    const runId = ++layoutRunIdRef.current;
    const runExpandedEntityId = expandedEntityId;
    // Mark the newly-requested node as "expanding" so the UI shows a spinner immediately.
    setExpandingNodeId(runExpandedEntityId);

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
        // Save current layout as prev for FLIP animation before updating
        setPrevLayout(layoutRef.current);
        setLayout(geo);
        setExpandingNodeId((current) => (current === runExpandedEntityId ? null : current));
      })
      .catch((error) => {
        if (cancelled || layoutRunIdRef.current !== runId) return;
        setExpandingNodeId((current) => (current === runExpandedEntityId ? null : current));
        console.error(error);
      });

    return () => {
      cancelled = true;
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
  ]);

  const effectiveGeometry: GraphGeometry | null = unionFrameLayout?.geometry ?? layout;
  const effectivePrevGeometry: GraphGeometry | null = unionFrameLayout ? null : prevLayout;
  const sourcePreviewByFrameId = useMemo<Map<number, SourcePreviewResponse>>(() => {
    const frameIds = collectRenderedFrameIdsForSourcePreviews(
      entityDefs,
      expandedNodeIds,
      showSource,
    );
    const previews = new Map<number, SourcePreviewResponse>();
    for (const frameId of frameIds) {
      const preview = getSourcePreviewSync(frameId);
      if (preview) previews.set(frameId, preview);
    }
    return previews;
    // eslint-disable-next-line react-hooks/exhaustive-deps -- sourcePreviewVersion triggers re-run when cache fills; expandedKey is serialized expandedNodeIds
  }, [entityDefs, expandedKey, showSource, sourcePreviewVersion, expandedNodeIds]);
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
        sourcePreviewByFrameId={sourcePreviewByFrameId}
        sourceLoadingNodeIds={sourceLoadingNodeIds}
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
