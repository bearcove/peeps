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
import { ContextMenu, ContextMenuItem, ContextMenuSeparator } from "../../ui/primitives/ContextMenu";

// Node interaction states: unselected → selected → expanded → pinned
export type NodeExpandState = "collapsed" | "expanded" | "pinned";

export function GraphViewport({
  entityDefs,
  snapPhase,
  waitingForProcesses,
  geometry,
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
  onPinnedNodesChange,
}: {
  entityDefs: EntityDef[];
  snapPhase: "idle" | "cutting" | "loading" | "ready" | "error";
  waitingForProcesses: boolean;
  geometry: GraphGeometry | null;
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
  onPinnedNodesChange?: (pinnedIds: Set<string>) => void;
}) {
  const effectiveGhostNodeIds = useMemo(() => {
    if (!geometry || selection?.kind !== "entity") return ghostNodeIds;
    const selected = selection.id;
    const s = new Set<string>(ghostNodeIds);
    for (const node of geometry.nodes) {
      if (node.id !== selected) s.add(node.id);
    }
    return s;
  }, [geometry, selection, ghostNodeIds]);

  const effectiveGhostEdgeIds = useMemo(() => {
    if (!geometry || selection?.kind !== "entity") return ghostEdgeIds;
    const selected = selection.id;
    const s = new Set<string>(ghostEdgeIds);
    for (const edge of geometry.edges) {
      if (edge.sourceId !== selected && edge.targetId !== selected) s.add(edge.id);
    }
    return s;
  }, [geometry, selection, ghostEdgeIds]);

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
  // Per-node expand state: absent = collapsed, "expanded" = visual overlay, "pinned" = relayouted
  const [nodeExpandStates, setNodeExpandStates] = useState<Map<string, NodeExpandState>>(new Map());
  const pinnedNodeIds = useMemo(() => {
    const s = new Set<string>();
    for (const [id, state] of nodeExpandStates) {
      if (state === "pinned") s.add(id);
    }
    return s;
  }, [nodeExpandStates]);

  useEffect(() => {
    setHasFitted(false);
  }, [geometryKey]);

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      const tag = (e.target as HTMLElement).tagName;
      if (tag === "INPUT" || tag === "TEXTAREA") return;
      setNodeExpandStates((prev) => {
        const next = new Map<string, NodeExpandState>();
        for (const [id, state] of prev) {
          if (state === "pinned") next.set(id, "pinned");
        }
        return next.size === prev.size ? prev : next;
      });
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  return (
    <div className="graph-flow" ref={graphFlowRef}>
      {nodeContextMenu && (() => {
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
            <ContextMenuItem onClick={() => { onFocusConnected(nodeContextMenu.nodeId); closeNodeContextMenu(); }}>
              Show only connected
            </ContextMenuItem>
            <ContextMenuSeparator />
            <ContextMenuItem prefix="−" onClick={() => { onHideNodeFilter(nodeContextMenu.nodeId); closeNodeContextMenu(); }}>
              Hide this node
            </ContextMenuItem>
            {location && (
              <ContextMenuItem prefix="−" onClick={() => { onHideLocationFilter(location); closeNodeContextMenu(); }}>
                <NodeChip icon={<FileRs size={12} weight="bold" />} label={location.split("/").pop() ?? location} />
              </ContextMenuItem>
            )}
            {krate && (
              <>
                <ContextMenuSeparator />
                <ContextMenuItem prefix="−" onClick={() => { onAppendFilterToken(`-crate:${quoteFilterValue(krate)}`); closeNodeContextMenu(); }}>
                  <NodeChip icon={<Package size={12} weight="bold" />} label={krate} />
                </ContextMenuItem>
                <ContextMenuItem prefix="+" onClick={() => { onAppendFilterToken(`+crate:${quoteFilterValue(krate)}`); closeNodeContextMenu(); }}>
                  <NodeChip icon={<Package size={12} weight="bold" />} label={krate} />
                </ContextMenuItem>
              </>
            )}
            {processId && (
              <>
                <ContextMenuSeparator />
                <ContextMenuItem prefix="−" onClick={() => { onAppendFilterToken(`-process:${quoteFilterValue(processId)}`); closeNodeContextMenu(); }}>
                  <NodeChip icon={<Terminal size={12} weight="bold" />} label={processLabel} />
                </ContextMenuItem>
                <ContextMenuItem prefix="+" onClick={() => { onAppendFilterToken(`+process:${quoteFilterValue(processId)}`); closeNodeContextMenu(); }}>
                  <NodeChip icon={<Terminal size={12} weight="bold" />} label={processLabel} />
                </ContextMenuItem>
              </>
            )}
            {kind && (
              <>
                <ContextMenuSeparator />
                <ContextMenuItem prefix="−" onClick={() => { onAppendFilterToken(`-kind:${quoteFilterValue(kind)}`); closeNodeContextMenu(); }}>
                  <NodeChip icon={kindIcon(kind, 12)} label={kindLabel} />
                </ContextMenuItem>
                <ContextMenuItem prefix="+" onClick={() => { onAppendFilterToken(`+kind:${quoteFilterValue(kind)}`); closeNodeContextMenu(); }}>
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
          setNodeExpandStates((prev) => {
            const next = new Map<string, NodeExpandState>();
            for (const [id, state] of prev) {
              if (state === "pinned") next.set(id, "pinned");
            }
            return next.size === prev.size ? prev : next;
          });
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
              nodeExpandStates={nodeExpandStates}
              ghostNodeIds={effectiveGhostNodeIds}
              onNodeHover={(id) => {
                if (id) {
                  // If any node is expanded (non-pinned), hover on other nodes is blocked.
                  const hasExpanded = [...nodeExpandStates.values()].some((s) => s === "expanded");
                  if (hasExpanded && !nodeExpandStates.has(id)) return;

                  // Hover selects the node (clear non-pinned expand from other nodes)
                  setNodeExpandStates((prev) => {
                    const next = new Map<string, NodeExpandState>();
                    for (const [nid, state] of prev) {
                      if (state === "pinned") next.set(nid, state);
                      else if (nid === id) next.set(nid, state);
                    }
                    return next;
                  });
                  onSelect({ kind: "entity", id });
                } else {
                  // Mouse left all nodes: deselect unless something is expanded/pinned
                  const selectedId = selection?.kind === "entity" ? selection.id : null;
                  if (selectedId && !nodeExpandStates.has(selectedId)) {
                    onSelect(null);
                  }
                }
              }}
              onNodeClick={(id) => {
                closeNodeContextMenu();
                const currentState = nodeExpandStates.get(id);

                if (!currentState || currentState === "collapsed") {
                  // Click 1: expand (visual overlay)
                  setNodeExpandStates((prev) => {
                    const next = new Map(prev);
                    next.set(id, "expanded");
                    return next;
                  });
                } else if (currentState === "expanded") {
                  // Click 2: pin (triggers relayout)
                  setNodeExpandStates((prev) => {
                    const next = new Map(prev);
                    next.set(id, "pinned");
                    return next;
                  });
                  onPinnedNodesChange?.(new Set([...pinnedNodeIds, id]));
                } else if (currentState === "pinned") {
                  // Click on pinned node: unpin (triggers relayout)
                  setNodeExpandStates((prev) => {
                    const next = new Map(prev);
                    next.delete(id);
                    return next;
                  });
                  const nextPinned = new Set(pinnedNodeIds);
                  nextPinned.delete(id);
                  onPinnedNodesChange?.(nextPinned);
                }
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
  const { panTo, viewportHeight, camera } = useCameraContext();
  const prevStatesRef = useRef<Map<string, NodeExpandState>>(new Map());

  useEffect(() => {
    const prev = prevStatesRef.current;
    for (const [id, state] of nodeExpandStates) {
      if (state === "expanded" && prev.get(id) !== "expanded") {
        const node = nodes.find((n) => n.id === id);
        if (node) {
          const { x, y, width } = node.worldRect;
          // Position the node top ~20% from the top of the viewport so the
          // expanded card (which can be up to 35em tall) has room below.
          // camera.y = worldY puts worldY at viewport center (50%).
          // We want node top at 20%, so shift camera.y down by 30% of viewport in world units.
          const offsetY = (viewportHeight * 0.3) / camera.zoom;
          panTo(x + width / 2, y + offsetY);
        }
        break;
      }
    }
    prevStatesRef.current = new Map(nodeExpandStates);
  }, [nodeExpandStates, nodes, panTo, viewportHeight, camera.zoom]);

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


