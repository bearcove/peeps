import React, { useEffect, useMemo, useState } from "react";
import { Camera, CircleNotch, Crosshair } from "@phosphor-icons/react";
import { ActionButton } from "../../ui/primitives/ActionButton";
import { FilterMenu, type FilterMenuItem } from "../../ui/primitives/FilterMenu";
import type { EntityDef, EdgeDef } from "../../snapshot";
import { layoutGraph, type SubgraphScopeMode } from "../../graph/elkAdapter";
import { measureEntityDefs } from "../../graph/render/NodeLayer";
import { GraphCanvas, useCameraContext } from "../../graph/canvas/GraphCanvas";
import { GroupLayer } from "../../graph/render/GroupLayer";
import { EdgeLayer } from "../../graph/render/EdgeLayer";
import { NodeLayer } from "../../graph/render/NodeLayer";
import type { GraphGeometry } from "../../graph/geometry";
import { scopeHueForKey } from "./scopeColors";
import type { FrameRenderResult } from "../../recording/unionGraph";
import "./GraphPanel.css";

export type GraphSelection =
  | { kind: "entity"; id: string }
  | { kind: "edge"; id: string }
  | null;

export type SnapPhase = "idle" | "cutting" | "loading" | "ready" | "error";

export type ScopeColorMode = "none" | "process" | "crate";

const GRAPH_EMPTY_MESSAGES: Record<SnapPhase, string> = {
  idle: "Take a snapshot to see the current state",
  cutting: "Waiting for all processes to sync…",
  loading: "Loading snapshot data…",
  ready: "No entities in snapshot",
  error: "Snapshot failed",
};

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
  hiddenKrates,
  onKrateToggle,
  onKrateSolo,
  processItems,
  hiddenProcesses,
  onProcessToggle,
  onProcessSolo,
  scopeColorMode,
  onToggleProcessColorBy,
  onToggleCrateColorBy,
  subgraphScopeMode,
  onToggleProcessSubgraphs,
  onToggleCrateSubgraphs,
  showLoners,
  onToggleShowLoners,
  unionFrameLayout,
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
  hiddenKrates: ReadonlySet<string>;
  onKrateToggle: (krate: string) => void;
  onKrateSolo: (krate: string) => void;
  processItems: FilterMenuItem[];
  hiddenProcesses: ReadonlySet<string>;
  onProcessToggle: (pid: string) => void;
  onProcessSolo: (pid: string) => void;
  scopeColorMode: ScopeColorMode;
  onToggleProcessColorBy: () => void;
  onToggleCrateColorBy: () => void;
  subgraphScopeMode: SubgraphScopeMode;
  onToggleProcessSubgraphs: () => void;
  onToggleCrateSubgraphs: () => void;
  showLoners: boolean;
  onToggleShowLoners: () => void;
  /** When provided, use this pre-computed layout (union mode) instead of measuring + ELK. */
  unionFrameLayout?: FrameRenderResult;
}) {
  const [layout, setLayout] = useState<GraphGeometry | null>(null);

  // In snapshot mode (no unionFrameLayout), measure and lay out from scratch.
  React.useEffect(() => {
    if (unionFrameLayout) return; // skip — union mode provides layout directly
    if (entityDefs.length === 0) return;
    measureEntityDefs(entityDefs)
      .then((sizes) => layoutGraph(entityDefs, edgeDefs, sizes, subgraphScopeMode))
      .then(setLayout)
      .catch(console.error);
  }, [entityDefs, edgeDefs, subgraphScopeMode, unionFrameLayout]);

  const effectiveGeometry: GraphGeometry | null = unionFrameLayout?.geometry ?? layout;

  const entityById = useMemo(() => new Map(entityDefs.map((entity) => [entity.id, entity])), [entityDefs]);

  const nodesWithScopeColor = useMemo(() => {
    if (!effectiveGeometry) return [];
    return effectiveGeometry.nodes.map((n) => {
      const entity = entityById.get(n.id);
      const scopeKey =
        !entity
          ? undefined
          : scopeColorMode === "process"
            ? entity.processId
            : scopeColorMode === "crate"
              ? (entity.krate ?? "~no-crate")
              : undefined;
      return {
        ...n,
        data: {
          ...n.data,
          scopeHue: scopeKey ? scopeHueForKey(scopeKey) : undefined,
        },
      };
    });
  }, [effectiveGeometry, entityById, scopeColorMode]);

  const ghostNodeIds = unionFrameLayout?.ghostNodeIds;
  const ghostEdgeIds = unionFrameLayout?.ghostEdgeIds;

  const isBusy = snapPhase === "cutting" || snapPhase === "loading";
  const showToolbar = crateItems.length > 1 || processItems.length > 0 || focusedEntityId || entityDefs.length > 0;

  // Keep track of whether we've fitted the view at least once for this layout.
  const [hasFitted, setHasFitted] = useState(false);
  const geometryKey = effectiveGeometry
    ? effectiveGeometry.nodes.map((n) => n.id).join(",")
    : "";

  // Reset fit state when geometry changes structure.
  useEffect(() => {
    setHasFitted(false);
  }, [geometryKey]);

  return (
    <div className="graph-panel">
      {showToolbar && (
        <div className="graph-toolbar">
          <div className="graph-toolbar-left">
            {entityDefs.length > 0 && (
              <>
                <span className="graph-stat">{entityDefs.length} entities</span>
                <span className="graph-stat">{edgeDefs.length} edges</span>
              </>
            )}
          </div>
          <div className="graph-toolbar-right">
            {processItems.length > 0 && (
              <FilterMenu
                label="Process"
                items={processItems}
                hiddenIds={hiddenProcesses}
                onToggle={onProcessToggle}
                onSolo={onProcessSolo}
                colorByActive={scopeColorMode === "process"}
                onToggleColorBy={onToggleProcessColorBy}
                colorByLabel="Use process colors"
                subgraphsActive={subgraphScopeMode === "process"}
                onToggleSubgraphs={onToggleProcessSubgraphs}
                subgraphsLabel="Use subgraphs"
              />
            )}
            {crateItems.length > 1 && (
              <FilterMenu
                label="Crate"
                items={crateItems}
                hiddenIds={hiddenKrates}
                onToggle={onKrateToggle}
                onSolo={onKrateSolo}
                colorByActive={scopeColorMode === "crate"}
                onToggleColorBy={onToggleCrateColorBy}
                colorByLabel="Use crate colors"
                subgraphsActive={subgraphScopeMode === "crate"}
                onToggleSubgraphs={onToggleCrateSubgraphs}
                subgraphsLabel="Use subgraphs"
              />
            )}
            <ActionButton
              variant={showLoners ? "default" : "ghost"}
              onPress={onToggleShowLoners}
            >
              Show loners: {showLoners ? "on" : "off"}
            </ActionButton>
            {focusedEntityId && (
              <ActionButton onPress={onExitFocus}>
                <Crosshair size={14} weight="bold" />
                Exit Focus
              </ActionButton>
            )}
          </div>
        </div>
      )}
      {entityDefs.length === 0 ? (
        <div className="graph-empty">
          {isBusy ? (
            <>
              <CircleNotch size={24} weight="bold" className="spinning graph-empty-icon" />{" "}
              {GRAPH_EMPTY_MESSAGES[snapPhase]}
            </>
          ) : snapPhase === "idle" && waitingForProcesses ? (
            <>
              <CircleNotch size={24} weight="bold" className="spinning graph-empty-icon" />
              <span>Waiting for a process to connect…</span>
            </>
          ) : snapPhase === "idle" ? (
            <>
              <Camera size={32} weight="thin" className="graph-empty-icon" />
              <span>{GRAPH_EMPTY_MESSAGES[snapPhase]}</span>
              <span className="graph-empty-hint">
                Press "Take Snapshot" to capture the current state of all connected processes
              </span>
            </>
          ) : (
            GRAPH_EMPTY_MESSAGES[snapPhase]
          )}
        </div>
      ) : (
        <div className="graph-flow">
          <GraphCanvas
            geometry={effectiveGeometry}
            onBackgroundClick={() => onSelect(null)}
          >
            <GraphAutoFit
              geometryKey={geometryKey}
              hasFitted={hasFitted}
              onFitted={() => setHasFitted(true)}
              suppressAutoFit={!!unionFrameLayout && hasFitted}
            />
            {effectiveGeometry && (
              <>
                <GroupLayer groups={effectiveGeometry.groups} />
                <EdgeLayer
                  edges={effectiveGeometry.edges}
                  selectedEdgeId={selection?.kind === "edge" ? selection.id : null}
                  ghostEdgeIds={ghostEdgeIds}
                  onEdgeClick={(id) => onSelect({ kind: "edge", id })}
                />
                <NodeLayer
                  nodes={nodesWithScopeColor}
                  selectedNodeId={selection?.kind === "entity" ? selection.id : null}
                  ghostNodeIds={ghostNodeIds}
                  onNodeClick={(id) => onSelect({ kind: "entity", id })}
                />
              </>
            )}
          </GraphCanvas>
        </div>
      )}
    </div>
  );
}

// ── GraphAutoFit ───────────────────────────────────────────────

/**
 * Renders nothing; uses useCameraContext() to trigger fitView on geometry changes.
 * Must be rendered inside <GraphCanvas>.
 */
function GraphAutoFit({
  geometryKey,
  hasFitted,
  onFitted,
  suppressAutoFit,
}: {
  geometryKey: string;
  hasFitted: boolean;
  onFitted: () => void;
  suppressAutoFit: boolean;
}) {
  const { fitView } = useCameraContext();

  useEffect(() => {
    if (suppressAutoFit) return;
    if (!geometryKey) return;
    fitView();
    onFitted();
  }, [geometryKey]); // eslint-disable-line react-hooks/exhaustive-deps

  // Also wire up F key to fit view
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "f" && !e.metaKey && !e.ctrlKey && !e.altKey) {
        const tag = (e.target as HTMLElement).tagName;
        if (tag === "INPUT" || tag === "TEXTAREA") return;
        fitView();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [fitView]);

  return null;
}
