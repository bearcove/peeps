import React, { useEffect, useMemo, useState } from "react";
import { Camera, CircleNotch, Crosshair } from "@phosphor-icons/react";
import { ActionButton } from "../../ui/primitives/ActionButton";
import { Badge } from "../../ui/primitives/Badge";
import { FilterMenu, type FilterMenuItem } from "../../ui/primitives/FilterMenu";
import { Switch } from "../../ui/primitives/Switch";
import type { EntityDef, EdgeDef } from "../../snapshot";
import { layoutGraph, type SubgraphScopeMode } from "../../graph/elkAdapter";
import { measureGraphLayout } from "../../graph/render/NodeLayer";
import { GraphCanvas, useCameraContext } from "../../graph/canvas/GraphCanvas";
import { GroupLayer } from "../../graph/render/GroupLayer";
import { EdgeLayer } from "../../graph/render/EdgeLayer";
import { NodeLayer } from "../../graph/render/NodeLayer";
import type { GraphGeometry, Point } from "../../graph/geometry";
import type { ScopeColorPair } from "./scopeColors";
import { assignScopeColorRgbByKey } from "./scopeColors";
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
  hiddenKrates,
  onKrateToggle,
  onKrateSolo,
  processItems,
  hiddenProcesses,
  onProcessToggle,
  onProcessSolo,
  kindItems,
  hiddenKinds,
  onKindToggle,
  onKindSolo,
  scopeColorMode,
  onToggleProcessColorBy,
  onToggleCrateColorBy,
  subgraphScopeMode,
  onToggleProcessSubgraphs,
  onToggleCrateSubgraphs,
  showLoners,
  onToggleShowLoners,
  scopeFilterLabel,
  onClearScopeFilter,
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
  kindItems: FilterMenuItem[];
  hiddenKinds: ReadonlySet<string>;
  onKindToggle: (kind: string) => void;
  onKindSolo: (kind: string) => void;
  scopeColorMode: ScopeColorMode;
  onToggleProcessColorBy: () => void;
  onToggleCrateColorBy: () => void;
  subgraphScopeMode: SubgraphScopeMode;
  onToggleProcessSubgraphs: () => void;
  onToggleCrateSubgraphs: () => void;
  showLoners: boolean;
  onToggleShowLoners: () => void;
  scopeFilterLabel?: string | null;
  onClearScopeFilter?: () => void;
  /** When provided, use this pre-computed layout (union mode) instead of measuring + ELK. */
  unionFrameLayout?: FrameRenderResult;
}) {
  const [layout, setLayout] = useState<GraphGeometry | null>(null);
  const [portAnchors, setPortAnchors] = useState<Map<string, Point>>(new Map());

  // In snapshot mode (no unionFrameLayout), measure and lay out from scratch.
  React.useEffect(() => {
    if (unionFrameLayout) return; // skip — union mode provides layout directly
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

  const crateItemsWithSwatches = useMemo<FilterMenuItem[]>(() => {
    if (scopeColorMode !== "crate") return crateItems;
    return crateItems.map((item) => {
      const scopeRgb = scopeColorByKey.get(item.id);
      if (!scopeRgb) return item;
      return {
        ...item,
        icon: (
          <span
            className="ui-filter-item-swatch"
            style={{
              background: `light-dark(rgb(${scopeRgb.light}), rgb(${scopeRgb.dark}))`,
            }}
          />
        ),
      };
    });
  }, [crateItems, scopeColorByKey, scopeColorMode]);

  const ghostNodeIds = unionFrameLayout?.ghostNodeIds;
  const ghostEdgeIds = unionFrameLayout?.ghostEdgeIds;

  const isBusy = snapPhase === "cutting" || snapPhase === "loading";
  const showToolbar =
    crateItems.length > 1 || processItems.length > 0 || kindItems.length > 1 || focusedEntityId || entityDefs.length > 0;

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
                <span className="graph-stat-sep" aria-hidden="true">|</span>
                <span className="graph-stat">{edgeDefs.length} edges</span>
              </>
            )}
          </div>
          <div className="graph-toolbar-right">
            <Switch
              checked={showLoners}
              onChange={(checked) => {
                if (checked !== showLoners) onToggleShowLoners();
              }}
              label="Show loners"
            />
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
                items={crateItemsWithSwatches}
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
            {kindItems.length > 1 && (
              <FilterMenu
                label="Kind"
                items={kindItems}
                hiddenIds={hiddenKinds}
                onToggle={onKindToggle}
                onSolo={onKindSolo}
              />
            )}
            {focusedEntityId && (
              <ActionButton size="sm" onPress={onExitFocus}>
                <Crosshair size={14} weight="bold" />
                Exit Focus
              </ActionButton>
            )}
            {scopeFilterLabel && (
              <>
                <Badge tone="warn">in:{scopeFilterLabel}</Badge>
                <ActionButton size="sm" onPress={onClearScopeFilter}>Clear scope</ActionButton>
              </>
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
                <GroupLayer groups={groupsWithScopeColor} />
                <GraphPortAnchors
                  geometryKey={geometryKey}
                  onAnchorsChange={setPortAnchors}
                />
                <EdgeLayer
                  edges={effectiveGeometry.edges}
                  selectedEdgeId={selection?.kind === "edge" ? selection.id : null}
                  ghostEdgeIds={ghostEdgeIds}
                  portAnchors={portAnchors}
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

function GraphPortAnchors({
  geometryKey,
  onAnchorsChange,
}: {
  geometryKey: string;
  onAnchorsChange: (anchors: Map<string, Point>) => void;
}) {
  const { clientToGraph } = useCameraContext();

  useEffect(() => {
    if (!geometryKey) {
      onAnchorsChange(new Map());
      return;
    }
    const raf = window.requestAnimationFrame(() => {
      const anchors = new Map<string, Point>();
      const nodes = document.querySelectorAll<HTMLElement>(".graph-port-anchor[data-port-id]");
      nodes.forEach((node) => {
        const portId = node.dataset.portId;
        if (!portId) return;
        const rect = node.getBoundingClientRect();
        const centerX = rect.left + rect.width / 2;
        const centerY = rect.top + rect.height / 2;
        const world = clientToGraph(centerX, centerY);
        if (!world) return;
        anchors.set(portId, world);
      });
      onAnchorsChange(anchors);
    });
    return () => window.cancelAnimationFrame(raf);
  }, [clientToGraph, geometryKey, onAnchorsChange]);

  return null;
}
