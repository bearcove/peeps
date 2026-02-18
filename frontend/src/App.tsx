import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import "./App.css";
import { SplitLayout } from "./ui/layout/SplitLayout";
import type { FilterMenuItem } from "./ui/primitives/FilterMenu";
import { apiClient } from "./api";
import type { ConnectionsResponse, FrameSummary } from "./api/types";
import { RecordingTimeline } from "./components/timeline/RecordingTimeline";
import {
  convertSnapshot,
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
import { ScopeTablePanel } from "./components/scopes/ScopeTablePanel";
import { ProcessModal } from "./components/ProcessModal";
import { AppHeader } from "./components/AppHeader";
import { ProcessIdenticon } from "./ui/primitives/ProcessIdenticon";
import { SegmentedGroup } from "./ui/primitives/SegmentedGroup";

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

// ── App ────────────────────────────────────────────────────────

export function App() {
  const [leftPaneTab, setLeftPaneTab] = useState<"graph" | "scopes">("graph");
  const [selectedScopeKind, setSelectedScopeKind] = useState<string | null>(null);
  const [snap, setSnap] = useState<SnapshotState>({ phase: "idle" });
  const [inspectorWidth, setInspectorWidth] = useState(340);
  const [inspectorCollapsed, setInspectorCollapsed] = useState(false);
  const [selection, setSelection] = useState<GraphSelection>(null);
  const [connections, setConnections] = useState<ConnectionsResponse | null>(null);
  const [showProcessModal, setShowProcessModal] = useState(false);
  const [focusedEntityId, setFocusedEntityId] = useState<string | null>(null);
  const [hiddenKrates, setHiddenKrates] = useState<ReadonlySet<string>>(new Set());
  const [hiddenProcesses, setHiddenProcesses] = useState<ReadonlySet<string>>(new Set());
  const [scopeColorMode, setScopeColorMode] = useState<ScopeColorMode>("crate");
  const [subgraphScopeMode, setSubgraphScopeMode] = useState<SubgraphScopeMode>("process");
  const [recording, setRecording] = useState<RecordingState>({ phase: "idle" });
  const [isLive, setIsLive] = useState(true);
  const [ghostMode, setGhostMode] = useState(false);
  const [unionFrameLayout, setUnionFrameLayout] = useState<FrameRenderResult | undefined>(undefined);
  const [downsampleInterval, setDownsampleInterval] = useState(1);
  const [builtDownsampleInterval, setBuiltDownsampleInterval] = useState(1);
  const pollingRef = useRef<number | null>(null);
  const isLiveRef = useRef(isLive);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const allEntities = snap.phase === "ready" ? snap.entities : [];
  const allEdges = snap.phase === "ready" ? snap.edges : [];

  const crateItems = useMemo<FilterMenuItem[]>(() => {
    const counts = new Map<string, number>();
    for (const e of allEntities) {
      const k = e.krate ?? "~no-crate";
      counts.set(k, (counts.get(k) ?? 0) + 1);
    }
    return Array.from(counts.keys())
      .sort()
      .map((k) => ({
        id: k,
        label: k === "~no-crate" ? "(no crate)" : k,
        meta: counts.get(k),
      }));
  }, [allEntities]);

  const processItems = useMemo<FilterMenuItem[]>(() => {
    const counts = new Map<string, number>();
    const processMeta = new Map<string, { name: string; pid: number | null }>();

    for (const e of allEntities) {
      counts.set(e.processId, (counts.get(e.processId) ?? 0) + 1);
      if (!processMeta.has(e.processId)) {
        processMeta.set(e.processId, { name: e.processName, pid: e.processPid });
      }
    }

    for (const proc of connections?.processes ?? []) {
      const processId = String(proc.conn_id);
      processMeta.set(processId, { name: proc.process_name, pid: proc.pid });
      if (!counts.has(processId)) {
        counts.set(processId, 0);
      }
    }

    const rows = Array.from(processMeta.entries()).map(([id, meta]) => ({
      id,
      name: meta.name,
      pid: meta.pid,
      count: counts.get(id) ?? 0,
    }));

    const duplicateNameCounts = new Map<string, number>();
    for (const row of rows) {
      duplicateNameCounts.set(row.name, (duplicateNameCounts.get(row.name) ?? 0) + 1);
    }

    return rows
      .sort(
        (a, b) =>
          a.name.localeCompare(b.name) ||
          (a.pid ?? Number.MAX_SAFE_INTEGER) - (b.pid ?? Number.MAX_SAFE_INTEGER) ||
          a.id.localeCompare(b.id),
      )
      .map((row) => {
        const hasDuplicateName = (duplicateNameCounts.get(row.name) ?? 0) > 1;
        const suffix = row.pid == null ? row.id : String(row.pid);
        const label = hasDuplicateName ? `${row.name}(${suffix})` : row.name;
        return {
          id: row.id,
          label,
          icon: <ProcessIdenticon name={row.name} seed={`${row.name}:${suffix}`} size={14} />,
          meta: row.count,
        };
      });
  }, [allEntities, connections]);

  const handleKrateToggle = useCallback((krate: string) => {
    setHiddenKrates((prev) => {
      const next = new Set(prev);
      if (next.has(krate)) next.delete(krate);
      else next.add(krate);
      return next;
    });
  }, []);

  const handleKrateSolo = useCallback(
    (krate: string) => {
      setHiddenKrates((prev) => {
        const otherKrates = crateItems.filter((i) => i.id !== krate).map((i) => i.id);
        const alreadySolo = otherKrates.every((id) => prev.has(id)) && !prev.has(krate);
        if (alreadySolo) return new Set();
        return new Set(otherKrates);
      });
    },
    [crateItems],
  );

  const handleProcessToggle = useCallback((pid: string) => {
    setHiddenProcesses((prev) => {
      const next = new Set(prev);
      if (next.has(pid)) next.delete(pid);
      else next.add(pid);
      return next;
    });
  }, []);

  const handleProcessSolo = useCallback(
    (pid: string) => {
      setHiddenProcesses((prev) => {
        const otherProcesses = processItems.filter((i) => i.id !== pid).map((i) => i.id);
        const alreadySolo = otherProcesses.every((id) => prev.has(id)) && !prev.has(pid);
        if (alreadySolo) return new Set();
        return new Set(otherProcesses);
      });
    },
    [processItems],
  );

  const handleToggleProcessColorBy = useCallback(() => {
    setScopeColorMode((prev) => (prev === "process" ? "none" : "process"));
  }, []);

  const handleToggleCrateColorBy = useCallback(() => {
    setScopeColorMode((prev) => (prev === "crate" ? "none" : "crate"));
  }, []);

  const handleToggleProcessSubgraphs = useCallback(() => {
    setSubgraphScopeMode((prev) => (prev === "process" ? "none" : "process"));
  }, []);

  const handleToggleCrateSubgraphs = useCallback(() => {
    setSubgraphScopeMode((prev) => (prev === "crate" ? "none" : "crate"));
  }, []);

  const { entities, edges } = useMemo(() => {
    const filtered = allEntities.filter(
      (e) =>
        (hiddenKrates.size === 0 || !hiddenKrates.has(e.krate ?? "~no-crate")) &&
        (hiddenProcesses.size === 0 || !hiddenProcesses.has(e.processId)),
    );
    if (!focusedEntityId) return { entities: filtered, edges: allEdges };
    return getConnectedSubgraph(focusedEntityId, filtered, allEdges);
  }, [focusedEntityId, allEntities, allEdges, hiddenKrates, hiddenProcesses]);

  const takeSnapshot = useCallback(async () => {
    setSnap({ phase: "cutting" });
    setSelection(null);
    setFocusedEntityId(null);
    try {
      const triggered = await apiClient.triggerCut();
      let status = await apiClient.fetchCutStatus(triggered.cut_id);
      while (status.pending_connections > 0) {
        await new Promise<void>((resolve) => window.setTimeout(resolve, 600));
        status = await apiClient.fetchCutStatus(triggered.cut_id);
      }
      setSnap({ phase: "loading" });
      const snapshot = await apiClient.fetchSnapshot();
      const converted = convertSnapshot(snapshot, subgraphScopeMode);
      setSnap({ phase: "ready", ...converted });
    } catch (err) {
      setSnap({ phase: "error", message: err instanceof Error ? err.message : String(err) });
    }
  }, [subgraphScopeMode]);

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
              const converted = convertSnapshot(frame, subgraphScopeMode);
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
  }, [subgraphScopeMode]);

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
        const converted = convertSnapshot(lastFrame, subgraphScopeMode);
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
          subgraphScopeMode,
        );
        setRecording((prev) => {
          if (prev.phase !== "stopped") return prev;
          return { ...prev, unionLayout: union, buildingUnion: false };
        });

        const snappedLast = nearestProcessedFrame(lastFrameIndex, union.processedFrameIndices);
        const unionFrame = renderFrameFromUnion(
          snappedLast,
          union,
          hiddenKrates,
          hiddenProcesses,
          focusedEntityId,
          ghostMode,
        );
        setUnionFrameLayout(unionFrame);
      }
    } catch (err) {
      console.error(err);
    }
  }, [hiddenKrates, hiddenProcesses, focusedEntityId, subgraphScopeMode]);

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
          const converted = convertSnapshot(lastFrame, subgraphScopeMode);
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
            subgraphScopeMode,
          );
          setRecording((prev) => {
            if (prev.phase !== "stopped") return prev;
            return { ...prev, unionLayout: union, buildingUnion: false };
          });

          const snappedLast = nearestProcessedFrame(lastFrameIndex, union.processedFrameIndices);
          const unionFrame = renderFrameFromUnion(
            snappedLast,
            union,
            hiddenKrates,
            hiddenProcesses,
            focusedEntityId,
          );
          setUnionFrameLayout(unionFrame);
        }
      } catch (err) {
        console.error(err);
      }
    },
    [hiddenKrates, hiddenProcesses, focusedEntityId, subgraphScopeMode],
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
          hiddenKrates,
          hiddenProcesses,
          focusedEntityId,
          ghostMode,
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
    [hiddenKrates, hiddenProcesses, focusedEntityId, ghostMode],
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
        subgraphScopeMode,
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
        hiddenKrates,
        hiddenProcesses,
        focusedEntityId,
        ghostMode,
      );
      setUnionFrameLayout(unionFrame);
      const frameData = union.frameCache.get(snapped);
      if (frameData) {
        setSnap({ phase: "ready", entities: frameData.entities, edges: frameData.edges });
      }
    } catch (err) {
      console.error(err);
    }
  }, [recording, downsampleInterval, hiddenKrates, hiddenProcesses, focusedEntityId, ghostMode, subgraphScopeMode]);

  // Re-render union frame when filters change during playback.
  useEffect(() => {
    if (recording.phase === "scrubbing") {
      const result = renderFrameFromUnion(
        recording.currentFrameIndex,
        recording.unionLayout,
        hiddenKrates,
        hiddenProcesses,
        focusedEntityId,
        ghostMode,
      );
      setUnionFrameLayout(result);
    } else if (recording.phase === "stopped" && recording.unionLayout) {
      const lastFrame = recording.frames.length - 1;
      const result = renderFrameFromUnion(
        recording.frames[lastFrame]?.frame_index ?? 0,
        recording.unionLayout,
        hiddenKrates,
        hiddenProcesses,
        focusedEntityId,
        ghostMode,
      );
      setUnionFrameLayout(result);
    }
  }, [hiddenKrates, hiddenProcesses, focusedEntityId, ghostMode, recording]);

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
  }, [takeSnapshot]);

  useEffect(() => {
    isLiveRef.current = isLive;
  }, [isLive]);

  useEffect(() => {
    return () => {
      if (pollingRef.current !== null) {
        window.clearInterval(pollingRef.current);
      }
    };
  }, []);

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape" && focusedEntityId) {
        setFocusedEntityId(null);
      } else if (e.key === "f" || e.key === "F") {
        if (selection?.kind === "entity") {
          setFocusedEntityId(selection.id);
        }
      }
    }
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [focusedEntityId, selection]);

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
        snap={snap}
        recording={recording}
        connCount={connCount}
        waitingForProcesses={waitingForProcesses}
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
      <SplitLayout
        left={
          <div className="app-left-pane">
            <div className="app-left-pane-tabs">
              <SegmentedGroup
                size="sm"
                aria-label="Left panel mode"
                value={leftPaneTab}
                onChange={(value) => setLeftPaneTab(value as "graph" | "scopes")}
                options={[
                  { value: "graph", label: "Graph" },
                  { value: "scopes", label: "Scopes" },
                ]}
              />
            </div>
            <div className="app-left-pane-body">
              {leftPaneTab === "graph" ? (
                <GraphPanel
                  entityDefs={entities}
                  edgeDefs={edges}
                  snapPhase={snap.phase}
                  selection={selection}
                  onSelect={(next) => {
                    setSelection(next);
                    if (next) setSelectedScopeKind(null);
                  }}
                  focusedEntityId={focusedEntityId}
                  onExitFocus={() => setFocusedEntityId(null)}
                  waitingForProcesses={waitingForProcesses}
                  crateItems={crateItems}
                  hiddenKrates={hiddenKrates}
                  onKrateToggle={handleKrateToggle}
                  onKrateSolo={handleKrateSolo}
                  processItems={processItems}
                  hiddenProcesses={hiddenProcesses}
                  onProcessToggle={handleProcessToggle}
                  onProcessSolo={handleProcessSolo}
                  scopeColorMode={scopeColorMode}
                  onToggleProcessColorBy={handleToggleProcessColorBy}
                  onToggleCrateColorBy={handleToggleCrateColorBy}
                  subgraphScopeMode={subgraphScopeMode}
                  onToggleProcessSubgraphs={handleToggleProcessSubgraphs}
                  onToggleCrateSubgraphs={handleToggleCrateSubgraphs}
                  unionFrameLayout={unionFrameLayout}
                />
              ) : (
                <ScopeTablePanel
                  selectedKind={selectedScopeKind}
                  onSelectKind={(kind) => {
                    setSelectedScopeKind(kind);
                    if (kind) setSelection(null);
                  }}
                />
              )}
            </div>
          </div>
        }
        right={
          <InspectorPanel
            collapsed={inspectorCollapsed}
            onToggleCollapse={() => setInspectorCollapsed((v) => !v)}
            selection={selection}
            entityDefs={allEntities}
            edgeDefs={allEdges}
            onFocusEntity={setFocusedEntityId}
            scrubbingUnionLayout={recording.phase === "scrubbing" ? recording.unionLayout : undefined}
            currentFrameIndex={recording.phase === "scrubbing" ? recording.currentFrameIndex : undefined}
            selectedScopeKind={selectedScopeKind}
          />
        }
        rightWidth={inspectorWidth}
        onRightWidthChange={setInspectorWidth}
        rightMinWidth={260}
        rightMaxWidth={600}
        rightCollapsed={inspectorCollapsed}
      />
    </div>
  );
}
