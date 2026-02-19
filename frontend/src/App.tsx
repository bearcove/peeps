import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import "./App.css";
import type { FilterMenuItem } from "./ui/primitives/FilterMenu";
import { apiClient } from "./api";
import type { ConnectionsResponse, FrameSummary } from "./api/types";
import { RecordingTimeline } from "./components/timeline/RecordingTimeline";
import {
  collapseEdgesThroughHiddenNodes,
  convertSnapshot,
  filterLoners,
  getConnectedSubgraph,
  type EntityDef,
  type EdgeDef,
} from "./snapshot";
import type { SubgraphScopeMode } from "./graph/elkAdapter";
import {
  buildUnionLayout,
  computeChangeFrames,
  computeChangeSummaries,
  nearestProcessedFrame,
  renderFrameFromUnion,
  type FrameChangeSummary,
  type FrameRenderResult,
  type UnionLayout,
} from "./recording/unionGraph";
import { GraphPanel, type GraphSelection, type ScopeColorMode, type SnapPhase } from "./components/graph/GraphPanel";
import { InspectorPanel } from "./components/inspector/InspectorPanel";
import { ScopeTablePanel, type ScopeTableRow } from "./components/scopes/ScopeTablePanel";
import { EntityTablePanel } from "./components/entities/EntityTablePanel";
import { ProcessModal } from "./components/ProcessModal";
import { AppHeader } from "./components/AppHeader";
import { formatProcessLabel } from "./processLabel";
import { canonicalNodeKind, kindDisplayName, kindIcon } from "./nodeKindSpec";
import { canonicalScopeKind, scopeKindIcon } from "./scopeKindSpec";
import {
  appendFilterToken,
  parseGraphFilterQuery,
  quoteFilterValue,
  tokenizeFilterQuery,
} from "./graphFilter";

// ── Snapshot state machine ─────────────────────────────────────

export type SnapshotState =
  | { phase: "idle" }
  | { phase: "cutting" }
  | { phase: "loading" }
  | { phase: "ready"; entities: EntityDef[]; edges: EdgeDef[] }
  | { phase: "error"; message: string };

// ── Recording state ────────────────────────────────────────────

export type RecordingState =
  | { phase: "idle" }
  | {
      phase: "recording";
      sessionId: string;
      startedAt: number;
      frameCount: number;
      elapsed: number;
      approxMemoryBytes: number;
      maxMemoryBytes: number;
    }
  | {
      phase: "stopped";
      sessionId: string;
      frameCount: number;
      frames: FrameSummary[];
      unionLayout: UnionLayout | null;
      buildingUnion: boolean;
      buildProgress?: [number, number];
      avgCaptureMs: number;
      maxCaptureMs: number;
      totalCaptureMs: number;
    }
  | {
      phase: "scrubbing";
      sessionId: string;
      frameCount: number;
      frames: FrameSummary[];
      currentFrameIndex: number;
      unionLayout: UnionLayout;
      avgCaptureMs: number;
      maxCaptureMs: number;
      totalCaptureMs: number;
    };

type ScopeEntityFilter = {
  scopeToken: string;
  scopeLabel: string;
  entityIds: Set<string>;
};

function sqlEscape(value: string): string {
  return value.replace(/'/g, "''");
}

function entityMatchesScopeFilter(entity: EntityDef, scopeEntityIds: ReadonlySet<string>): boolean {
  if (scopeEntityIds.has(entity.id)) return true;
  if (entity.channelPair) {
    return scopeEntityIds.has(entity.channelPair.tx.id) || scopeEntityIds.has(entity.channelPair.rx.id);
  }
  if (entity.rpcPair) {
    return scopeEntityIds.has(entity.rpcPair.req.id) || scopeEntityIds.has(entity.rpcPair.resp.id);
  }
  return false;
}

const INSPECTOR_MARGIN = 12;

// ── App ────────────────────────────────────────────────────────

export function App() {
  const [leftPaneTab, setLeftPaneTab] = useState<"graph" | "scopes" | "entities">("graph");
  const [selectedScopeKind, setSelectedScopeKind] = useState<string | null>(null);
  const [selectedScope, setSelectedScope] = useState<ScopeTableRow | null>(null);
  const [scopeEntityFilter, setScopeEntityFilter] = useState<ScopeEntityFilter | null>(null);
  const [snap, setSnap] = useState<SnapshotState>({ phase: "idle" });
  const [inspectorOpen, setInspectorOpen] = useState(false);
  const [inspectorPosition, setInspectorPosition] = useState<{ x: number; y: number } | null>(null);
  const [selection, setSelection] = useState<GraphSelection>(null);
  const [connections, setConnections] = useState<ConnectionsResponse | null>(null);
  const [showProcessModal, setShowProcessModal] = useState(false);
  const [graphFilterText, setGraphFilterText] = useState("colorBy:crate groupBy:process loners:off");
  const [recording, setRecording] = useState<RecordingState>({ phase: "idle" });
  const [isLive, setIsLive] = useState(true);
  const [ghostMode, setGhostMode] = useState(false);
  const [unionFrameLayout, setUnionFrameLayout] = useState<FrameRenderResult | undefined>(undefined);
  const [downsampleInterval, setDownsampleInterval] = useState(1);
  const [builtDownsampleInterval, setBuiltDownsampleInterval] = useState(1);
  const pollingRef = useRef<number | null>(null);
  const isLiveRef = useRef(isLive);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const mainPaneRef = useRef<HTMLDivElement>(null);
  const inspectorOverlayRef = useRef<HTMLDivElement>(null);

  const clampInspectorPosition = useCallback((x: number, y: number) => {
    const main = mainPaneRef.current;
    const overlay = inspectorOverlayRef.current;
    if (!main || !overlay) return { x, y };
    const maxX = Math.max(INSPECTOR_MARGIN, main.clientWidth - overlay.offsetWidth - INSPECTOR_MARGIN);
    const maxY = Math.max(INSPECTOR_MARGIN, main.clientHeight - overlay.offsetHeight - INSPECTOR_MARGIN);
    return {
      x: Math.min(Math.max(x, INSPECTOR_MARGIN), maxX),
      y: Math.min(Math.max(y, INSPECTOR_MARGIN), maxY),
    };
  }, []);

  const computeDefaultInspectorPosition = useCallback(() => {
    const main = mainPaneRef.current;
    const overlay = inspectorOverlayRef.current;
    if (!main || !overlay) return null;

    const mainRect = main.getBoundingClientRect();
    const graphFlow = main.querySelector(".graph-flow") as HTMLElement | null;
    if (leftPaneTab === "graph") {
      if (!graphFlow) return null;
      const flowRect = graphFlow.getBoundingClientRect();
      const flowLeft = flowRect.left - mainRect.left;
      const flowTop = flowRect.top - mainRect.top;
      const flowRight = flowRect.right - mainRect.left;
      const flowBottom = flowRect.bottom - mainRect.top;
      const toolbar = main.querySelector(".graph-toolbar") as HTMLElement | null;
      const toolbarClearance = toolbar
        ? toolbar.getBoundingClientRect().bottom - mainRect.top
        : flowTop;
      const preferredX = flowRight - overlay.offsetWidth - INSPECTOR_MARGIN;
      const preferredY = toolbarClearance + INSPECTOR_MARGIN;
      const clamped = clampInspectorPosition(preferredX, preferredY);
      const minY = toolbarClearance + INSPECTOR_MARGIN;
      const maxY = Math.max(minY, flowBottom - overlay.offsetHeight - INSPECTOR_MARGIN);
      return {
        x: Math.max(clamped.x, flowLeft + INSPECTOR_MARGIN),
        y: Math.max(minY, Math.min(clamped.y, maxY)),
      };
    }

    const startX = main.clientWidth - overlay.offsetWidth - INSPECTOR_MARGIN;
    const startY = INSPECTOR_MARGIN;
    return clampInspectorPosition(startX, startY);
  }, [clampInspectorPosition, leftPaneTab]);

  const handleInspectorHeaderPointerDown = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      if (event.button !== 0) return;
      const main = mainPaneRef.current;
      const overlay = inspectorOverlayRef.current;
      if (!main || !overlay) return;
      event.preventDefault();

      const mainRect = main.getBoundingClientRect();
      const overlayRect = overlay.getBoundingClientRect();
      const offsetX = event.clientX - overlayRect.left;
      const offsetY = event.clientY - overlayRect.top;

      const onPointerMove = (moveEvent: PointerEvent) => {
        const rawX = moveEvent.clientX - mainRect.left - offsetX;
        const rawY = moveEvent.clientY - mainRect.top - offsetY;
        setInspectorPosition(clampInspectorPosition(rawX, rawY));
      };

      const onPointerUp = () => {
        window.removeEventListener("pointermove", onPointerMove);
        window.removeEventListener("pointerup", onPointerUp);
      };

      window.addEventListener("pointermove", onPointerMove);
      window.addEventListener("pointerup", onPointerUp);
    },
    [clampInspectorPosition],
  );

  const allEntities = snap.phase === "ready" ? snap.entities : [];
  const allEdges = snap.phase === "ready" ? snap.edges : [];
  const graphTextFilters = useMemo(() => parseGraphFilterQuery(graphFilterText), [graphFilterText]);
  const effectiveHiddenKrates = graphTextFilters.excludeCrates;
  const effectiveHiddenProcesses = graphTextFilters.excludeProcesses;
  const effectiveHiddenKinds = graphTextFilters.excludeKinds;
  const effectiveShowLoners = graphTextFilters.showLoners ?? false;
  const effectiveScopeColorMode: ScopeColorMode = graphTextFilters.colorBy ?? "none";
  const effectiveSubgraphScopeMode: SubgraphScopeMode =
    graphTextFilters.groupBy === "none" ? "none" : (graphTextFilters.groupBy ?? "none");
  const focusedEntityId = graphTextFilters.focusedNodeId ?? null;

  const setFocusedEntityFilter = useCallback((entityId: string | null) => {
    setGraphFilterText((prev) => {
      const tokens = tokenizeFilterQuery(prev).filter((token) => {
        const key = token.startsWith("+") || token.startsWith("-") ? token.slice(1) : token;
        return !key.toLowerCase().startsWith("focus:");
      });
      if (entityId) tokens.push(`focus:${quoteFilterValue(entityId)}`);
      return tokens.join(" ");
    });
  }, []);
  const applyBaseFilters = useCallback(
    (ignore: "crate" | "process" | "kind" | null) => {
      let entities = allEntities.filter(
        (e) =>
          (graphTextFilters.includeCrates.size === 0 || graphTextFilters.includeCrates.has(e.krate ?? "~no-crate")) &&
          (graphTextFilters.includeProcesses.size === 0 || graphTextFilters.includeProcesses.has(e.processId)) &&
          (graphTextFilters.includeKinds.size === 0 || graphTextFilters.includeKinds.has(canonicalNodeKind(e.kind))) &&
          (graphTextFilters.includeNodeIds.size === 0 || graphTextFilters.includeNodeIds.has(e.id)) &&
          (graphTextFilters.includeLocations.size === 0 || graphTextFilters.includeLocations.has(e.source)) &&
          (ignore === "crate" || effectiveHiddenKrates.size === 0 || !effectiveHiddenKrates.has(e.krate ?? "~no-crate")) &&
          (ignore === "process" || effectiveHiddenProcesses.size === 0 || !effectiveHiddenProcesses.has(e.processId)) &&
          (ignore === "kind" || effectiveHiddenKinds.size === 0 || !effectiveHiddenKinds.has(canonicalNodeKind(e.kind))) &&
          !graphTextFilters.excludeNodeIds.has(e.id) &&
          !graphTextFilters.excludeLocations.has(e.source),
      );
      const entityIds = new Set(entities.map((entity) => entity.id));
      let edges = collapseEdgesThroughHiddenNodes(allEdges, entityIds);
      if (!effectiveShowLoners) {
        const withoutLoners = filterLoners(entities, edges);
        entities = withoutLoners.entities;
        edges = withoutLoners.edges;
      }
      if (scopeEntityFilter) {
        entities = entities.filter((entity) => entityMatchesScopeFilter(entity, scopeEntityFilter.entityIds));
        const entityIds = new Set(entities.map((entity) => entity.id));
        edges = collapseEdgesThroughHiddenNodes(allEdges, entityIds);
      }
      return { entities, edges };
    },
    [
      allEntities,
      allEdges,
      effectiveHiddenKrates,
      effectiveHiddenProcesses,
      effectiveHiddenKinds,
      effectiveShowLoners,
      scopeEntityFilter,
      graphTextFilters,
    ],
  );

  const hideNodeViaTextFilter = useCallback((entityId: string) => {
    setGraphFilterText((prev) => appendFilterToken(prev, `-node:${quoteFilterValue(entityId)}`));
  }, []);

  const hideLocationViaTextFilter = useCallback((location: string) => {
    setGraphFilterText((prev) => appendFilterToken(prev, `-location:${quoteFilterValue(location)}`));
  }, []);

  const appendFilterTokenCallback = useCallback((token: string) => {
    setGraphFilterText((prev) => appendFilterToken(prev, token));
  }, []);

  const crateItems = useMemo<FilterMenuItem[]>(() => {
    const counts = new Map<string, number>();
    for (const entity of applyBaseFilters("crate").entities) {
      const crate = entity.krate ?? "~no-crate";
      counts.set(crate, (counts.get(crate) ?? 0) + 1);
    }
    return Array.from(counts.keys())
      .sort()
      .map((crate) => ({
        id: crate,
        label: crate === "~no-crate" ? "(no crate)" : crate,
        meta: counts.get(crate) ?? 0,
      }));
  }, [applyBaseFilters]);

  const processItems = useMemo<FilterMenuItem[]>(() => {
    const counts = new Map<string, number>();
    const processMeta = new Map<string, { name: string; pid: number | null }>();

    for (const entity of applyBaseFilters("process").entities) {
      counts.set(entity.processId, (counts.get(entity.processId) ?? 0) + 1);
      if (!processMeta.has(entity.processId)) {
        processMeta.set(entity.processId, { name: entity.processName, pid: entity.processPid });
      }
    }

    for (const proc of connections?.processes ?? []) {
      const processId = String(proc.conn_id);
      processMeta.set(processId, { name: proc.process_name, pid: proc.pid });
      if (!counts.has(processId)) counts.set(processId, 0);
    }

    const rows = Array.from(processMeta.entries()).map(([id, meta]) => ({
      id,
      name: meta.name,
      pid: meta.pid,
      count: counts.get(id) ?? 0,
    }));

    return rows
      .sort(
        (a, b) =>
          a.name.localeCompare(b.name) ||
          (a.pid ?? Number.MAX_SAFE_INTEGER) - (b.pid ?? Number.MAX_SAFE_INTEGER) ||
          a.id.localeCompare(b.id),
      )
      .map((row) => {
        const suffix = row.pid == null ? "?" : String(row.pid);
        return {
          id: row.id,
          label: formatProcessLabel(row.name, row.pid),
          icon: scopeKindIcon("process", 14),
          meta: row.count,
        };
      });
  }, [applyBaseFilters, connections]);

  const kindItems = useMemo<FilterMenuItem[]>(() => {
    const counts = new Map<string, number>();
    for (const entity of applyBaseFilters("kind").entities) {
      const kind = canonicalNodeKind(entity.kind);
      counts.set(kind, (counts.get(kind) ?? 0) + 1);
    }
    return Array.from(counts.entries())
      .sort(([a], [b]) => kindDisplayName(a).localeCompare(kindDisplayName(b)))
      .map(([kind, count]) => ({
        id: kind,
        label: kindDisplayName(kind),
        icon: kindIcon(kind, 14),
        meta: count,
      }));
  }, [applyBaseFilters]);

  const { entities, edges } = useMemo(() => {
    const { entities: filteredEntities, edges: filteredEdges } = applyBaseFilters(null);
    if (!focusedEntityId) return { entities: filteredEntities, edges: filteredEdges };
    return getConnectedSubgraph(focusedEntityId, filteredEntities, filteredEdges);
  }, [applyBaseFilters, focusedEntityId]);

  const queryEntities = useMemo(() => {
    if (!scopeEntityFilter) return allEntities;
    return allEntities.filter((entity) => entityMatchesScopeFilter(entity, scopeEntityFilter.entityIds));
  }, [allEntities, scopeEntityFilter]);

  const snapshotProcessCount = useMemo(() => {
    return new Set(allEntities.map((entity) => entity.processId)).size;
  }, [allEntities]);

  const applyScopeEntityFilter = useCallback(async (scope: ScopeTableRow) => {
    const connId = Number(scope.processId);
    if (!Number.isFinite(connId)) return;
    const streamId = sqlEscape(scope.streamId);
    const scopeId = sqlEscape(scope.scopeId);
    const sql = `
select l.entity_id
from entity_scope_links l
where l.conn_id = ${connId}
  and l.stream_id = '${streamId}'
  and l.scope_id = '${scopeId}'
`;
    const response = await apiClient.fetchSql(sql);
    const ids = new Set<string>();
    for (const row of response.rows) {
      if (!Array.isArray(row) || row.length < 1) continue;
      const rawEntityId = String(row[0]);
      ids.add(`${scope.processId}/${rawEntityId}`);
    }
    setScopeEntityFilter({
      scopeToken: scope.scopeId,
      scopeLabel:
        scope.scopeKind === "process"
          ? formatProcessLabel(scope.processName, scope.pid)
          : (scope.scopeName || scope.scopeId),
      entityIds: ids,
    });
  }, []);

  const takeSnapshot = useCallback(async () => {
    setSnap({ phase: "cutting" });
    setSelection(null);
    setFocusedEntityFilter(null);
    try {
      const triggered = await apiClient.triggerCut();
      let status = await apiClient.fetchCutStatus(triggered.cut_id);
      while (status.pending_connections > 0) {
        await new Promise<void>((resolve) => window.setTimeout(resolve, 600));
        status = await apiClient.fetchCutStatus(triggered.cut_id);
      }
      setSnap({ phase: "loading" });
      const snapshot = await apiClient.fetchSnapshot();
      const converted = convertSnapshot(snapshot, effectiveSubgraphScopeMode);
      setSnap({ phase: "ready", ...converted });
    } catch (err) {
      setSnap({ phase: "error", message: err instanceof Error ? err.message : String(err) });
    }
  }, [effectiveSubgraphScopeMode, setFocusedEntityFilter]);

  const handleStartRecording = useCallback(async () => {
    try {
      const session = await apiClient.startRecording();
      const startedAt = Date.now();
      setRecording({
        phase: "recording",
        sessionId: session.session_id,
        startedAt,
        frameCount: session.frame_count,
        elapsed: 0,
        approxMemoryBytes: session.approx_memory_bytes,
        maxMemoryBytes: session.max_memory_bytes,
      });
      pollingRef.current = window.setInterval(() => {
        void (async () => {
          try {
            const current = await apiClient.fetchRecordingCurrent();
            if (!current.session) return;
            const elapsed = Date.now() - startedAt;
            setRecording((prev) => {
              if (prev.phase !== "recording") return prev;
              return {
                ...prev,
                frameCount: current.session!.frame_count,
                elapsed,
                approxMemoryBytes: current.session!.approx_memory_bytes,
              };
            });
            if (isLiveRef.current && current.session.frame_count > 0) {
              const frameIndex = current.session.frame_count - 1;
              const frame = await apiClient.fetchRecordingFrame(frameIndex);
              const converted = convertSnapshot(frame, effectiveSubgraphScopeMode);
              setSnap({ phase: "ready", ...converted });
            }
          } catch (e) {
            console.error(e);
          }
        })();
      }, 1000);
    } catch (err) {
      console.error(err);
    }
  }, [effectiveSubgraphScopeMode]);

  const handleStopRecording = useCallback(async () => {
    if (pollingRef.current !== null) {
      window.clearInterval(pollingRef.current);
      pollingRef.current = null;
    }
    try {
      const session = await apiClient.stopRecording();
      const autoInterval =
        session.frame_count > 500 ? 5 : session.frame_count >= 100 ? 2 : 1;
      setDownsampleInterval(autoInterval);
      setBuiltDownsampleInterval(autoInterval);
      setRecording({
        phase: "stopped",
        sessionId: session.session_id,
        frameCount: session.frame_count,
        frames: session.frames,
        unionLayout: null,
        buildingUnion: true,
        buildProgress: [0, session.frame_count],
        avgCaptureMs: session.avg_capture_ms,
        maxCaptureMs: session.max_capture_ms,
        totalCaptureMs: session.total_capture_ms,
      });
      if (session.frame_count > 0) {
        const lastFrameIndex = session.frame_count - 1;
        const lastFrame = await apiClient.fetchRecordingFrame(lastFrameIndex);
        const converted = convertSnapshot(lastFrame, effectiveSubgraphScopeMode);
        setSnap({ phase: "ready", ...converted });

        const union = await buildUnionLayout(
          session.frames,
          apiClient,
          (loaded, total) => {
            setRecording((prev) => {
              if (prev.phase !== "stopped") return prev;
              return { ...prev, buildProgress: [loaded, total] };
            });
          },
          autoInterval,
          effectiveSubgraphScopeMode,
        );
        setRecording((prev) => {
          if (prev.phase !== "stopped") return prev;
          return { ...prev, unionLayout: union, buildingUnion: false };
        });

        const snappedLast = nearestProcessedFrame(lastFrameIndex, union.processedFrameIndices);
        const unionFrame = renderFrameFromUnion(
          snappedLast,
          union,
          graphTextFilters.includeCrates,
          graphTextFilters.includeProcesses,
          graphTextFilters.includeKinds,
          graphTextFilters.includeNodeIds,
          graphTextFilters.includeLocations,
          effectiveHiddenKrates,
          effectiveHiddenProcesses,
          effectiveHiddenKinds,
          graphTextFilters.excludeNodeIds,
          graphTextFilters.excludeLocations,
          focusedEntityId,
          ghostMode,
          effectiveShowLoners,
        );
        setUnionFrameLayout(unionFrame);
      }
    } catch (err) {
      console.error(err);
    }
  }, [effectiveHiddenKrates, effectiveHiddenProcesses, effectiveHiddenKinds, graphTextFilters, focusedEntityId, ghostMode, effectiveShowLoners, effectiveSubgraphScopeMode]);

  const handleExport = useCallback(async () => {
    try {
      const blob = await apiClient.exportRecording();
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      const sessionId =
        recording.phase === "stopped" || recording.phase === "scrubbing"
          ? recording.sessionId.replace(/:/g, "_")
          : "recording";
      a.href = url;
      a.download = `recording-${sessionId}.json`;
      a.click();
      URL.revokeObjectURL(url);
    } catch (err) {
      console.error(err);
    }
  }, [recording]);

  const handleImportFile = useCallback(
    async (e: React.ChangeEvent<HTMLInputElement>) => {
      const file = e.target.files?.[0];
      if (!file) return;
      e.target.value = "";
      try {
        const session = await apiClient.importRecording(file);
        const autoInterval =
          session.frame_count > 500 ? 5 : session.frame_count >= 100 ? 2 : 1;
        setDownsampleInterval(autoInterval);
        setBuiltDownsampleInterval(autoInterval);
        setRecording({
          phase: "stopped",
          sessionId: session.session_id,
          frameCount: session.frame_count,
          frames: session.frames,
          unionLayout: null,
          buildingUnion: true,
          buildProgress: [0, session.frame_count],
          avgCaptureMs: session.avg_capture_ms,
          maxCaptureMs: session.max_capture_ms,
          totalCaptureMs: session.total_capture_ms,
        });
        if (session.frames.length > 0) {
          const lastFrameIndex = session.frames[session.frames.length - 1].frame_index;
          const lastFrame = await apiClient.fetchRecordingFrame(lastFrameIndex);
          const converted = convertSnapshot(lastFrame, effectiveSubgraphScopeMode);
          setSnap({ phase: "ready", ...converted });

          const union = await buildUnionLayout(
            session.frames,
            apiClient,
            (loaded, total) => {
              setRecording((prev) => {
                if (prev.phase !== "stopped") return prev;
                return { ...prev, buildProgress: [loaded, total] };
              });
            },
            autoInterval,
            effectiveSubgraphScopeMode,
          );
          setRecording((prev) => {
            if (prev.phase !== "stopped") return prev;
            return { ...prev, unionLayout: union, buildingUnion: false };
          });

          const snappedLast = nearestProcessedFrame(lastFrameIndex, union.processedFrameIndices);
          const unionFrame = renderFrameFromUnion(
            snappedLast,
            union,
            graphTextFilters.includeCrates,
            graphTextFilters.includeProcesses,
            graphTextFilters.includeKinds,
            graphTextFilters.includeNodeIds,
            graphTextFilters.includeLocations,
            effectiveHiddenKrates,
            effectiveHiddenProcesses,
            effectiveHiddenKinds,
            graphTextFilters.excludeNodeIds,
            graphTextFilters.excludeLocations,
            focusedEntityId,
            ghostMode,
            effectiveShowLoners,
          );
          setUnionFrameLayout(unionFrame);
        }
      } catch (err) {
        console.error(err);
      }
    },
    [effectiveHiddenKrates, effectiveHiddenProcesses, effectiveHiddenKinds, graphTextFilters, focusedEntityId, ghostMode, effectiveShowLoners, effectiveSubgraphScopeMode],
  );

  const handleScrub = useCallback(
    (frameIndex: number) => {
      setRecording((prev) => {
        if (prev.phase !== "stopped" && prev.phase !== "scrubbing") return prev;
        const { frames, frameCount, sessionId, avgCaptureMs, maxCaptureMs, totalCaptureMs } = prev;
        const unionLayout =
          prev.phase === "stopped" ? prev.unionLayout : prev.unionLayout;
        if (!unionLayout) return prev;

        const result = renderFrameFromUnion(
          frameIndex,
          unionLayout,
          graphTextFilters.includeCrates,
          graphTextFilters.includeProcesses,
          graphTextFilters.includeKinds,
          graphTextFilters.includeNodeIds,
          graphTextFilters.includeLocations,
          effectiveHiddenKrates,
          effectiveHiddenProcesses,
          effectiveHiddenKinds,
          graphTextFilters.excludeNodeIds,
          graphTextFilters.excludeLocations,
          focusedEntityId,
          ghostMode,
          effectiveShowLoners,
        );
        setUnionFrameLayout(result);

        const snapped = nearestProcessedFrame(frameIndex, unionLayout.processedFrameIndices);
        const frameData = unionLayout.frameCache.get(snapped);
        if (frameData) {
          setSnap({ phase: "ready", entities: frameData.entities, edges: frameData.edges });
        }

        return {
          phase: "scrubbing" as const,
          sessionId,
          frameCount,
          frames,
          currentFrameIndex: frameIndex,
          unionLayout,
          avgCaptureMs,
          maxCaptureMs,
          totalCaptureMs,
        };
      });
    },
    [effectiveHiddenKrates, effectiveHiddenProcesses, effectiveHiddenKinds, graphTextFilters, focusedEntityId, ghostMode, effectiveShowLoners],
  );

  const handleRebuildUnion = useCallback(async () => {
    if (recording.phase !== "stopped" && recording.phase !== "scrubbing") return;
    const { frames, sessionId, frameCount, avgCaptureMs, maxCaptureMs, totalCaptureMs } =
      recording;
    setRecording({
      phase: "stopped",
      sessionId,
      frameCount,
      frames,
      unionLayout: null,
      buildingUnion: true,
      buildProgress: [0, frames.length],
      avgCaptureMs,
      maxCaptureMs,
      totalCaptureMs,
    });
    try {
      const union = await buildUnionLayout(
        frames,
        apiClient,
        (loaded, total) => {
          setRecording((prev) => {
            if (prev.phase !== "stopped") return prev;
            return { ...prev, buildProgress: [loaded, total] };
          });
        },
        downsampleInterval,
        effectiveSubgraphScopeMode,
      );
      setBuiltDownsampleInterval(downsampleInterval);
      setRecording((prev) => {
        if (prev.phase !== "stopped") return prev;
        return { ...prev, unionLayout: union, buildingUnion: false };
      });
      const lastFrameIdx = frames[frames.length - 1]?.frame_index ?? 0;
      const snapped = nearestProcessedFrame(lastFrameIdx, union.processedFrameIndices);
      const unionFrame = renderFrameFromUnion(
        snapped,
        union,
        graphTextFilters.includeCrates,
        graphTextFilters.includeProcesses,
        graphTextFilters.includeKinds,
        graphTextFilters.includeNodeIds,
        graphTextFilters.includeLocations,
        effectiveHiddenKrates,
        effectiveHiddenProcesses,
        effectiveHiddenKinds,
        graphTextFilters.excludeNodeIds,
        graphTextFilters.excludeLocations,
        focusedEntityId,
        ghostMode,
        effectiveShowLoners,
      );
      setUnionFrameLayout(unionFrame);
      const frameData = union.frameCache.get(snapped);
      if (frameData) {
        setSnap({ phase: "ready", entities: frameData.entities, edges: frameData.edges });
      }
    } catch (err) {
      console.error(err);
    }
  }, [recording, downsampleInterval, effectiveHiddenKrates, effectiveHiddenProcesses, effectiveHiddenKinds, graphTextFilters, focusedEntityId, ghostMode, effectiveShowLoners, effectiveSubgraphScopeMode]);

  // Re-render union frame when filters change during playback.
  useEffect(() => {
    if (recording.phase === "scrubbing") {
      const result = renderFrameFromUnion(
        recording.currentFrameIndex,
        recording.unionLayout,
        graphTextFilters.includeCrates,
        graphTextFilters.includeProcesses,
        graphTextFilters.includeKinds,
        graphTextFilters.includeNodeIds,
        graphTextFilters.includeLocations,
        effectiveHiddenKrates,
        effectiveHiddenProcesses,
        effectiveHiddenKinds,
        graphTextFilters.excludeNodeIds,
        graphTextFilters.excludeLocations,
        focusedEntityId,
        ghostMode,
        effectiveShowLoners,
      );
      setUnionFrameLayout(result);
    } else if (recording.phase === "stopped" && recording.unionLayout) {
      const lastFrame = recording.frames.length - 1;
      const result = renderFrameFromUnion(
        recording.frames[lastFrame]?.frame_index ?? 0,
        recording.unionLayout,
        graphTextFilters.includeCrates,
        graphTextFilters.includeProcesses,
        graphTextFilters.includeKinds,
        graphTextFilters.includeNodeIds,
        graphTextFilters.includeLocations,
        effectiveHiddenKrates,
        effectiveHiddenProcesses,
        effectiveHiddenKinds,
        graphTextFilters.excludeNodeIds,
        graphTextFilters.excludeLocations,
        focusedEntityId,
        ghostMode,
        effectiveShowLoners,
      );
      setUnionFrameLayout(result);
    }
  }, [effectiveHiddenKrates, effectiveHiddenProcesses, effectiveHiddenKinds, graphTextFilters, focusedEntityId, ghostMode, effectiveShowLoners, recording]);

  // Clear union frame layout when going back to idle or starting a new recording.
  useEffect(() => {
    if (recording.phase === "idle" || recording.phase === "recording") {
      setUnionFrameLayout(undefined);
    }
  }, [recording.phase]);

  useEffect(() => {
    let cancelled = false;
    async function poll() {
      while (!cancelled) {
        try {
          const conns = await apiClient.fetchConnections();
          if (cancelled) break;
          setConnections(conns);
          const existingSnapshot = await apiClient.fetchExistingSnapshot();
          if (cancelled) break;
          if (existingSnapshot) {
            const converted = convertSnapshot(existingSnapshot, effectiveSubgraphScopeMode);
            setSnap({ phase: "ready", ...converted });
            break;
          }
          if (conns.connected_processes > 0) {
            takeSnapshot();
            break;
          }
        } catch (e) {
          console.error(e);
        }
        await new Promise<void>((resolve) => setTimeout(resolve, 2000));
      }
    }
    poll();
    return () => {
      cancelled = true;
    };
  }, [takeSnapshot, effectiveSubgraphScopeMode]);

  useEffect(() => {
    isLiveRef.current = isLive;
  }, [isLive]);

  useEffect(() => {
    if (!inspectorOpen || inspectorPosition) return;
    const start = computeDefaultInspectorPosition();
    if (!start) return;
    setInspectorPosition(start);
  }, [inspectorOpen, inspectorPosition, computeDefaultInspectorPosition, leftPaneTab, entities.length]);

  useEffect(() => {
    if (!inspectorOpen) return;
    const onResize = () => {
      if (!inspectorPosition) return;
      setInspectorPosition((prev) => {
        if (!prev) return prev;
        return clampInspectorPosition(prev.x, prev.y);
      });
    };
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, [inspectorOpen, inspectorPosition, clampInspectorPosition]);

  useEffect(() => {
    return () => {
      if (pollingRef.current !== null) {
        window.clearInterval(pollingRef.current);
      }
    };
  }, []);

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      const target = e.target as HTMLElement | null;
      if (target) {
        const tag = target.tagName;
        if (target.isContentEditable || tag === "INPUT" || tag === "TEXTAREA") return;
      }
      if (e.key === "Escape" && focusedEntityId) {
        setFocusedEntityFilter(null);
      } else if (e.key === "f" || e.key === "F") {
        if (selection?.kind === "entity") {
          setFocusedEntityFilter(selection.id);
        }
      }
    }
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [focusedEntityId, selection, setFocusedEntityFilter]);

  const isBusy = snap.phase === "cutting" || snap.phase === "loading";
  const connCount = connections?.connected_processes ?? 0;
  const waitingForProcesses = connCount === 0 && snap.phase === "idle";

  const currentFrameIndex =
    recording.phase === "scrubbing"
      ? recording.currentFrameIndex
      : recording.phase === "stopped"
        ? recording.frames.length - 1
        : 0;

  const unionLayoutForDerived =
    (recording.phase === "stopped" || recording.phase === "scrubbing") && recording.unionLayout
      ? recording.unionLayout
      : null;

  const snappedFrameIndex = unionLayoutForDerived
    ? nearestProcessedFrame(currentFrameIndex, unionLayoutForDerived.processedFrameIndices)
    : currentFrameIndex;

  const processedFrameCount = unionLayoutForDerived?.processedFrameIndices.length;

  const changeSummaries = useMemo<Map<number, FrameChangeSummary> | null>(() => {
    return unionLayoutForDerived ? computeChangeSummaries(unionLayoutForDerived) : null;
  }, [recording]); // eslint-disable-line react-hooks/exhaustive-deps

  const changeFrames = useMemo<number[] | null>(() => {
    return unionLayoutForDerived ? computeChangeFrames(unionLayoutForDerived) : null;
  }, [recording]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    if (recording.phase !== "stopped" && recording.phase !== "scrubbing") return;
    function onKey(e: KeyboardEvent) {
      const tag = (e.target as HTMLElement).tagName;
      if (tag === "INPUT" || tag === "TEXTAREA") return;
      if (e.key === "[" && changeFrames) {
        const prev = changeFrames.filter((f) => f < currentFrameIndex).at(-1);
        if (prev !== undefined) handleScrub(prev);
      } else if (e.key === "]" && changeFrames) {
        const next = changeFrames.find((f) => f > currentFrameIndex);
        if (next !== undefined) handleScrub(next);
      }
    }
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [recording.phase, changeFrames, currentFrameIndex, handleScrub]);

  return (
    <div className="app">
      {showProcessModal && connections && (
        <ProcessModal connections={connections} onClose={() => setShowProcessModal(false)} />
      )}
      <AppHeader
        leftPaneTab={leftPaneTab}
        onLeftPaneTabChange={setLeftPaneTab}
        snap={snap}
        snapshotProcessCount={snapshotProcessCount}
        recording={recording}
        connCount={connCount}
        isBusy={isBusy}
        isLive={isLive}
        onSetIsLive={setIsLive}
        onShowProcessModal={() => setShowProcessModal(true)}
        onTakeSnapshot={takeSnapshot}
        onStartRecording={handleStartRecording}
        onStopRecording={handleStopRecording}
        onExport={handleExport}
        onImportClick={() => fileInputRef.current?.click()}
        fileInputRef={fileInputRef}
        onImportFile={handleImportFile}
      />
      {(recording.phase === "stopped" || recording.phase === "scrubbing") &&
        recording.frames.length > 0 && (
          <RecordingTimeline
            frames={recording.frames}
            frameCount={recording.frameCount}
            currentFrameIndex={currentFrameIndex}
            onScrub={handleScrub}
            buildingUnion={recording.phase === "stopped" && recording.buildingUnion}
            buildProgress={recording.phase === "stopped" ? recording.buildProgress : undefined}
            changeSummary={changeSummaries?.get(snappedFrameIndex)}
            changeFrames={changeFrames ?? undefined}
            avgCaptureMs={recording.avgCaptureMs}
            maxCaptureMs={recording.maxCaptureMs}
            totalCaptureMs={recording.totalCaptureMs}
            ghostMode={ghostMode}
            onGhostToggle={() => setGhostMode((v) => !v)}
            processedFrameCount={processedFrameCount}
            downsampleInterval={downsampleInterval}
            onDownsampleChange={setDownsampleInterval}
            canRebuild={downsampleInterval !== builtDownsampleInterval}
            onRebuild={handleRebuildUnion}
          />
        )}
      <div className="app-main" ref={mainPaneRef}>
        <div className="app-left-pane">
          <div className="app-left-pane-body">
            {leftPaneTab === "graph" ? (
              <GraphPanel
                entityDefs={entities}
                edgeDefs={edges}
                snapPhase={snap.phase}
                selection={selection}
                onSelect={(next) => {
                  setSelection(next);
                  if (next) {
                    if (!inspectorOpen) {
                      setInspectorPosition(null);
                      setInspectorOpen(true);
                    }
                    setSelectedScopeKind(null);
                    setSelectedScope(null);
                  } else {
                    setFocusedEntityFilter(null);
                    setInspectorOpen(false);
                  }
                }}
                focusedEntityId={focusedEntityId}
                onExitFocus={() => {
                  setFocusedEntityFilter(null);
                  setSelection(null);
                  setInspectorOpen(false);
                }}
                waitingForProcesses={waitingForProcesses}
                crateItems={crateItems}
                processItems={processItems}
                kindItems={kindItems}
                scopeColorMode={effectiveScopeColorMode}
                subgraphScopeMode={effectiveSubgraphScopeMode}
                scopeFilterLabel={scopeEntityFilter?.scopeToken ?? null}
                onClearScopeFilter={() => setScopeEntityFilter(null)}
                unionFrameLayout={unionFrameLayout}
                graphFilterText={graphFilterText}
                onGraphFilterTextChange={setGraphFilterText}
                onHideNodeFilter={hideNodeViaTextFilter}
                onHideLocationFilter={hideLocationViaTextFilter}
                onFocusConnected={setFocusedEntityFilter}
                onAppendFilterToken={appendFilterTokenCallback}
                floatingFilterBar
              />
            ) : leftPaneTab === "scopes" ? (
              <ScopeTablePanel
                selectedKind={selectedScopeKind}
                selectedScopeKey={selectedScope?.key ?? null}
                onSelectKind={(kind) => {
                  setSelectedScopeKind(kind);
                  if (kind) {
                    setSelection(null);
                    setSelectedScope(null);
                  }
                }}
                onSelectScope={(scope) => {
                  setSelectedScope(scope);
                  if (scope) {
                    setSelection(null);
                  }
                }}
                onShowGraphScope={(scope) => {
                  void (async () => {
                    await applyScopeEntityFilter(scope);
                    setLeftPaneTab("graph");
                    setSelection(null);
                    setFocusedEntityFilter(null);
                  })();
                }}
                onViewScopeEntities={(scope) => {
                  void (async () => {
                    await applyScopeEntityFilter(scope);
                    setLeftPaneTab("entities");
                    setSelection(null);
                    setFocusedEntityFilter(null);
                  })();
                }}
              />
            ) : (
              <EntityTablePanel
                entityDefs={queryEntities}
                selectedEntityId={selection?.kind === "entity" ? selection.id : null}
                scopeFilterLabel={scopeEntityFilter?.scopeLabel ?? null}
                onClearScopeFilter={() => setScopeEntityFilter(null)}
                onSelectEntity={(entityId) => {
                  setSelection({ kind: "entity", id: entityId });
                  if (!inspectorOpen) {
                    setInspectorPosition(null);
                    setInspectorOpen(true);
                  }
                  setLeftPaneTab("graph");
                }}
              />
            )}
          </div>
        </div>
        {inspectorOpen && (
          <div
            className="app-inspector-overlay"
            ref={inspectorOverlayRef}
            style={
              inspectorPosition
                ? { left: inspectorPosition.x, top: inspectorPosition.y }
                : { visibility: "hidden", pointerEvents: "none" }
            }
          >
            <InspectorPanel
              onClose={() => setInspectorOpen(false)}
              onHeaderPointerDown={handleInspectorHeaderPointerDown}
              selection={selection}
              entityDefs={allEntities}
              edgeDefs={allEdges}
              focusedEntityId={focusedEntityId}
              onToggleFocusEntity={(id) => {
                setFocusedEntityFilter(focusedEntityId === id ? null : id);
              }}
              onOpenScopeKind={(kind) => {
                setLeftPaneTab("scopes");
                setSelection(null);
                setSelectedScope(null);
                setSelectedScopeKind(canonicalScopeKind(kind));
              }}
              scrubbingUnionLayout={recording.phase === "scrubbing" ? recording.unionLayout : undefined}
              currentFrameIndex={recording.phase === "scrubbing" ? recording.currentFrameIndex : undefined}
              selectedScopeKind={selectedScopeKind}
              selectedScope={selectedScope}
            />
          </div>
        )}
      </div>
    </div>
  );
}
