import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Camera, CircleNotch, FileRs, Package, Terminal } from "@phosphor-icons/react";
import type { EntityDef } from "../../snapshot";
import { quoteFilterValue } from "../../graphFilter";
import { canonicalNodeKind, kindDisplayName, kindIcon } from "../../nodeKindSpec";
import { formatProcessLabel } from "../../processLabel";
import { NodeChip } from "../../ui/primitives/NodeChip";
import { GraphCanvas, useCameraContext } from "../../graph/canvas/GraphCanvas";
import { GroupLayer } from "../../graph/render/GroupLayer";
import { EdgeLayer } from "../../graph/render/EdgeLayer";
import { NodeLayer } from "../../graph/render/NodeLayer";
import type { GraphGeometry, GeometryGroup, GeometryNode } from "../../graph/geometry";
import {
  ContextMenu,
  ContextMenuItem,
  ContextMenuSeparator,
} from "../../ui/primitives/ContextMenu";

// Node interaction states: absent = collapsed, "expanding" = loading in progress, "expanded" = fully expanded
export type NodeExpandState = "expanding" | "expanded";

export function GraphViewport({
  entityDefs,
  snapPhase,
  waitingForProcesses,
  geometry,
  prevNodes,
  groups,
  nodes,
  selection,
  onSelect,
  unionModeSuppressAutoFit,
  entityById,
  onHideNodeFilter,
  onHideLocationFilter,
  onFocusConnected,
  onAppendFilterToken,
  ghostNodeIds,
  ghostEdgeIds,
  expandedNodeId,
  expandingNodeId,
  onExpandedNodeChange,
}: {
  entityDefs: EntityDef[];
  snapPhase: "idle" | "cutting" | "loading" | "ready" | "error";
  waitingForProcesses: boolean;
  geometry: GraphGeometry | null;
  prevNodes?: GeometryNode[];
  groups: GeometryGroup[];
  nodes: GeometryNode[];
  selection: { kind: "entity"; id: string } | { kind: "edge"; id: string } | null;
  onSelect: (next: { kind: "entity"; id: string } | { kind: "edge"; id: string } | null) => void;
  unionModeSuppressAutoFit: boolean;
  entityById: Map<string, EntityDef>;
  onHideNodeFilter: (entityId: string) => void;
  onHideLocationFilter: (location: string) => void;
  onFocusConnected: (entityId: string) => void;
  onAppendFilterToken: (token: string) => void;
  ghostNodeIds?: Set<string>;
  ghostEdgeIds?: Set<string>;
  expandedNodeId?: string | null;
  expandingNodeId?: string | null;
  onExpandedNodeChange?: (id: string | null) => void;
}) {
  const effectiveGhostNodeIds = useMemo(() => {
    return ghostNodeIds;
  }, [ghostNodeIds]);

  const effectiveGhostEdgeIds = useMemo(() => {
    return ghostEdgeIds;
  }, [ghostEdgeIds]);

  const portAnchors = geometry?.portAnchors ?? new Map();
  const [hasFitted, setHasFitted] = useState(false);
  const graphFlowRef = useRef<HTMLDivElement | null>(null);
  const [nodeContextMenu, setNodeContextMenu] = useState<{
    nodeId: string;
    x: number;
    y: number;
  } | null>(null);

  const geometryKey = geometry ? geometry.nodes.map((n) => n.id).join(",") : "";
  const isBusy = snapPhase === "cutting" || snapPhase === "loading";
  const isEmpty = entityDefs.length === 0;
  const closeNodeContextMenu = useCallback(() => setNodeContextMenu(null), []);
  // Per-node expand state derived from props (controlled by filter text + transient loading state).
  const nodeExpandStates = useMemo(() => {
    const m = new Map<string, NodeExpandState>();
    // "expanding" takes priority while loading; once ELK lands it transitions to "expanded".
    if (expandedNodeId) m.set(expandedNodeId, "expanded");
    if (expandingNodeId) m.set(expandingNodeId, "expanding");
    return m;
  }, [expandedNodeId, expandingNodeId]);

  const collapseAll = useCallback(() => {
    onExpandedNodeChange?.(null);
  }, [onExpandedNodeChange]);

  useEffect(() => {
    setHasFitted(false);
  }, [geometryKey]);

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      const tag = (e.target as HTMLElement).tagName;
      if (tag === "INPUT" || tag === "TEXTAREA") return;
      collapseAll();
      onSelect(null);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [collapseAll, onSelect]);

  return (
    <div className="graph-flow" ref={graphFlowRef}>
      {nodeContextMenu &&
        (() => {
          const entity = entityById.get(nodeContextMenu.nodeId);
          const location = entity ? `${entity.source.path}:${entity.source.line}` : "";
          const krate = entity?.topFrame?.crate_name;
          const processId = entity?.processId ?? "";
          const processLabel = entity
            ? formatProcessLabel(entity.processName, entity.processPid)
            : processId;
          const kind = entity ? canonicalNodeKind(entity.kind) : "";
          const kindLabel = kind ? kindDisplayName(kind) : "";
          return (
            <ContextMenu x={nodeContextMenu.x} y={nodeContextMenu.y} onClose={closeNodeContextMenu}>
              <ContextMenuItem
                onClick={() => {
                  onFocusConnected(nodeContextMenu.nodeId);
                  closeNodeContextMenu();
                }}
              >
                Show only connected
              </ContextMenuItem>
              <ContextMenuSeparator />
              <ContextMenuItem
                prefix="−"
                onClick={() => {
                  onHideNodeFilter(nodeContextMenu.nodeId);
                  closeNodeContextMenu();
                }}
              >
                Hide this node
              </ContextMenuItem>
              {location && (
                <ContextMenuItem
                  prefix="−"
                  onClick={() => {
                    onHideLocationFilter(location);
                    closeNodeContextMenu();
                  }}
                >
                  <NodeChip
                    icon={<FileRs size={12} weight="bold" />}
                    label={location.split("/").pop() ?? location}
                  />
                </ContextMenuItem>
              )}
              {krate && (
                <>
                  <ContextMenuSeparator />
                  <ContextMenuItem
                    prefix="−"
                    onClick={() => {
                      onAppendFilterToken(`-crate:${quoteFilterValue(krate)}`);
                      closeNodeContextMenu();
                    }}
                  >
                    <NodeChip icon={<Package size={12} weight="bold" />} label={krate} />
                  </ContextMenuItem>
                  <ContextMenuItem
                    prefix="+"
                    onClick={() => {
                      onAppendFilterToken(`+crate:${quoteFilterValue(krate)}`);
                      closeNodeContextMenu();
                    }}
                  >
                    <NodeChip icon={<Package size={12} weight="bold" />} label={krate} />
                  </ContextMenuItem>
                </>
              )}
              {processId && (
                <>
                  <ContextMenuSeparator />
                  <ContextMenuItem
                    prefix="−"
                    onClick={() => {
                      onAppendFilterToken(`-process:${quoteFilterValue(processId)}`);
                      closeNodeContextMenu();
                    }}
                  >
                    <NodeChip icon={<Terminal size={12} weight="bold" />} label={processLabel} />
                  </ContextMenuItem>
                  <ContextMenuItem
                    prefix="+"
                    onClick={() => {
                      onAppendFilterToken(`+process:${quoteFilterValue(processId)}`);
                      closeNodeContextMenu();
                    }}
                  >
                    <NodeChip icon={<Terminal size={12} weight="bold" />} label={processLabel} />
                  </ContextMenuItem>
                </>
              )}
              {kind && (
                <>
                  <ContextMenuSeparator />
                  <ContextMenuItem
                    prefix="−"
                    onClick={() => {
                      onAppendFilterToken(`-kind:${quoteFilterValue(kind)}`);
                      closeNodeContextMenu();
                    }}
                  >
                    <NodeChip icon={kindIcon(kind, 12)} label={kindLabel} />
                  </ContextMenuItem>
                  <ContextMenuItem
                    prefix="+"
                    onClick={() => {
                      onAppendFilterToken(`+kind:${quoteFilterValue(kind)}`);
                      closeNodeContextMenu();
                    }}
                  >
                    <NodeChip icon={kindIcon(kind, 12)} label={kindLabel} />
                  </ContextMenuItem>
                </>
              )}
            </ContextMenu>
          );
        })()}
      <GraphCanvas
        geometry={geometry}
        onBackgroundClick={() => {
          closeNodeContextMenu();
          // Clear non-pinned expand states on background click
          collapseAll();
          onSelect(null);
        }}
      >
        <GraphAutoFit
          geometryKey={geometryKey}
          onFitted={() => setHasFitted(true)}
          suppressAutoFit={unionModeSuppressAutoFit && hasFitted}
        />
        <NodeExpandPanner nodes={nodes} nodeExpandStates={nodeExpandStates} />
        {geometry && (
          <>
            <GroupLayer groups={groups} />
            <EdgeLayer
              edges={geometry.edges}
              selectedEdgeId={selection?.kind === "edge" ? selection.id : null}
              ghostEdgeIds={effectiveGhostEdgeIds}
              portAnchors={portAnchors}
              onEdgeClick={(id) => {
                closeNodeContextMenu();
                onSelect({ kind: "edge", id });
              }}
            />
            <NodeLayer
              nodes={nodes}
              prevNodes={prevNodes}
              nodeExpandStates={nodeExpandStates}
              ghostNodeIds={effectiveGhostNodeIds}
              onNodeHover={(id) => {
                if (id) {
                  // If a node is already expanded, hover on other nodes is blocked.
                  if (nodeExpandStates.size > 0 && !nodeExpandStates.has(id)) return;
                  onSelect({ kind: "entity", id });
                } else {
                  // Mouse left all nodes: deselect unless a node is expanded
                  const selectedId = selection?.kind === "entity" ? selection.id : null;
                  if (selectedId && !nodeExpandStates.has(selectedId)) {
                    onSelect(null);
                  }
                }
              }}
              onNodeClick={(id) => {
                closeNodeContextMenu();
                onExpandedNodeChange?.(expandedNodeId === id ? null : id);
                onSelect({ kind: "entity", id });
              }}
              onNodeContextMenu={(id, clientX, clientY) => {
                const graphFlow = graphFlowRef.current;
                if (!graphFlow) return;
                const rect = graphFlow.getBoundingClientRect();
                const x = Math.max(8, Math.min(clientX - rect.left, Math.max(8, rect.width - 8)));
                const y = Math.max(8, Math.min(clientY - rect.top, Math.max(8, rect.height - 8)));
                setNodeContextMenu({ nodeId: id, x, y });
              }}
            />
          </>
        )}
      </GraphCanvas>
      {isEmpty && (
        <div className="graph-empty graph-empty--overlay">
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
      )}
    </div>
  );
}

const GRAPH_EMPTY_MESSAGES: Record<"idle" | "cutting" | "loading" | "ready" | "error", string> = {
  idle: "Take a snapshot to see the current state",
  cutting: "Waiting for all processes to sync…",
  loading: "Loading snapshot data…",
  ready: "No entities in snapshot",
  error: "Snapshot failed",
};

function NodeExpandPanner({
  nodes,
  nodeExpandStates,
}: {
  nodes: GeometryNode[];
  nodeExpandStates: Map<string, NodeExpandState>;
}) {
  const { panTo, animateCameraTo, getManualInteractionVersion, viewportHeight, camera } =
    useCameraContext();
  const prevStatesRef = useRef<Map<string, NodeExpandState>>(new Map());
  // Camera position saved when expansion starts; restored on collapse unless user manually moved.
  const savedCameraRef = useRef<typeof camera | null>(null);
  const savedManualVersionRef = useRef<number | null>(null);
  const canRestoreRef = useRef(false);
  const didAutoPanRef = useRef(false);

  useEffect(() => {
    const prev = prevStatesRef.current;
    const wasEmpty = prev.size === 0;
    const isEmpty = nodeExpandStates.size === 0;

    if (!isEmpty && wasEmpty) {
      savedCameraRef.current = camera;
      savedManualVersionRef.current = getManualInteractionVersion();
      canRestoreRef.current = true;
      didAutoPanRef.current = false;
    }

    if (
      !isEmpty &&
      canRestoreRef.current &&
      savedManualVersionRef.current != null &&
      getManualInteractionVersion() !== savedManualVersionRef.current
    ) {
      // User manually panned/zoomed: forget pan-back target immediately.
      savedCameraRef.current = null;
      savedManualVersionRef.current = null;
      canRestoreRef.current = false;
      didAutoPanRef.current = false;
    }

    if (isEmpty && !wasEmpty) {
      // All nodes collapsed — restore only if we auto-panned and user never moved manually.
      if (canRestoreRef.current && didAutoPanRef.current && savedCameraRef.current) {
        animateCameraTo(savedCameraRef.current);
      }
      savedCameraRef.current = null;
      savedManualVersionRef.current = null;
      canRestoreRef.current = false;
      didAutoPanRef.current = false;
    } else if (!isEmpty && canRestoreRef.current && !didAutoPanRef.current) {
      const expandedEntry = [...nodeExpandStates].find(([, state]) => state === "expanded");
      if (expandedEntry) {
        const [id] = expandedEntry;
        const prevState = prev.get(id);
        const justFinishedExpand = prevState !== "expanded";
        if (justFinishedExpand) {
          const node = nodes.find((n) => n.id === id);
          if (node) {
            const { x, y, width } = node.worldRect;
            // Keep the node top around 20% viewport height so there is room for expanded content.
            const offsetY = (viewportHeight * 0.3) / camera.zoom;
            panTo(x + width / 2, y + offsetY);
            didAutoPanRef.current = true;
          }
        }
      }
    }

    prevStatesRef.current = new Map(nodeExpandStates);
  }, [
    nodeExpandStates,
    nodes,
    panTo,
    animateCameraTo,
    getManualInteractionVersion,
    viewportHeight,
    camera,
  ]);

  return null;
}

function GraphAutoFit({
  geometryKey,
  onFitted,
  suppressAutoFit,
}: {
  geometryKey: string;
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
