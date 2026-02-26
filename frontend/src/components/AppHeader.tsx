import React from "react";
import {
  Camera,
  CircleNotch,
  DownloadSimple,
  Record,
  Stop,
  UploadSimple,
} from "@phosphor-icons/react";
import { ActionButton } from "../ui/primitives/ActionButton";
import { SegmentedGroup } from "../ui/primitives/SegmentedGroup";
import { formatElapsed } from "./timeline/RecordingTimeline";
import { apiMode } from "../api";
import type { RecordingState, SnapshotState } from "../App";
import "./AppHeader.css";

function formatBytes(bytes: number): string {
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

export function AppHeader({
  leftPaneTab,
  onLeftPaneTabChange,
  snap,
  snapshotProcessCount: _snapshotProcessCount,
  symbolicationProgress,
  recording,
  connCount,
  isBusy,
  isLive: _isLive,
  onSetIsLive,
  onShowProcessModal,
  onTakeSnapshot,
  onStartRecording,
  onStopRecording,
  onExport,
  onImportClick,
  fileInputRef,
  onImportFile,
  layoutAlgorithm,
  layoutAlgorithmOptions,
  onLayoutAlgorithmChange,
}: {
  leftPaneTab: "graph" | "scopes" | "entities" | "events";
  onLeftPaneTabChange: (tab: "graph" | "scopes" | "entities" | "events") => void;
  snap: SnapshotState;
  snapshotProcessCount: number;
  symbolicationProgress: { resolved: number; pending: number; total: number } | null;
  recording: RecordingState;
  connCount: number;
  isBusy: boolean;
  isLive: boolean;
  onSetIsLive: (v: boolean | ((prev: boolean) => boolean)) => void;
  onShowProcessModal: () => void;
  onTakeSnapshot: () => void;
  onStartRecording: () => void;
  onStopRecording: () => void;
  onExport: () => void;
  onImportClick: () => void;
  fileInputRef: React.RefObject<HTMLInputElement | null>;
  onImportFile: (e: React.ChangeEvent<HTMLInputElement>) => void;
  layoutAlgorithm: string;
  layoutAlgorithmOptions: Array<{ id: string; label: string }>;
  onLayoutAlgorithmChange: (algorithm: string) => void;
}) {
  const buttonLabel =
    snap.phase === "cutting" ? "Syncing…" : snap.phase === "loading" ? "Loading…" : "Take Snapshot";

  return (
    <div className="app-header">
      <img
        className="app-header-logo"
        src="/favicon-light.svg"
        alt=""
        width={16}
        height={16}
        aria-hidden="true"
      />
      <span className="app-header-title">moire</span>
      <SegmentedGroup
        aria-label="Primary page"
        value={leftPaneTab}
        onChange={(value) =>
          onLeftPaneTabChange(value as "graph" | "scopes" | "entities" | "events")
        }
        options={[
          { value: "graph", label: "Graph" },
          {
            value: "scopes",
            label: <span className="app-header-tab-label">Scopes</span>,
          },
          { value: "entities", label: "Entities" },
          { value: "events", label: "Events" },
        ]}
      />
      <div className="app-header-layout">
        <label className="app-header-layout-label" htmlFor="app-header-layout-select">
          Layout
        </label>
        <select
          id="app-header-layout-select"
          className="app-header-layout-select"
          value={layoutAlgorithm}
          onChange={(e) => onLayoutAlgorithmChange(e.target.value)}
        >
          {layoutAlgorithmOptions.map((option) => (
            <option key={option.id} value={option.id}>
              {option.label}
            </option>
          ))}
        </select>
      </div>
      {connCount > 0 && (
        <button
          type="button"
          className="proc-pill proc-pill--connected"
          onClick={onShowProcessModal}
          title="Click to see connected processes"
        >
          {connCount} connected
        </button>
      )}
      {apiMode === "lab" ? <span className="app-header-badge">mock data</span> : null}
      {snap.phase === "ready" && symbolicationProgress && (
        <span className="app-header-badge app-header-badge--warn">
          Symbolication: {symbolicationProgress.resolved}/{symbolicationProgress.total} resolved (
          {symbolicationProgress.pending} pending)
        </span>
      )}
      {snap.phase === "error" && <span className="app-header-error">{snap.message}</span>}
      <span className="app-header-spacer" />
      {connCount === 0 && (
        <span className="app-header-badge app-header-badge--warn">Live: 0 processes</span>
      )}
      {recording.phase === "recording" && (
        <span
          className={[
            "app-header-badge",
            recording.approxMemoryBytes >= recording.maxMemoryBytes * 0.75
              ? "app-header-badge--recording-warn"
              : "app-header-badge--recording",
          ].join(" ")}
        >
          <span className="recording-dot" />
          {formatElapsed(recording.elapsed)} · {recording.frameCount} frames ·{" "}
          {formatBytes(recording.approxMemoryBytes)}
        </span>
      )}
      {recording.phase === "recording" && (
        <ActionButton variant="default" onPress={() => onSetIsLive((v) => !v)}>
          Live
        </ActionButton>
      )}
      {(recording.phase === "stopped" || recording.phase === "scrubbing") && (
        <>
          <ActionButton variant="default" onPress={onExport}>
            <DownloadSimple size={14} weight="bold" />
            Export
          </ActionButton>
          <ActionButton variant="default" onPress={onImportClick}>
            <UploadSimple size={14} weight="bold" />
            Import
          </ActionButton>
        </>
      )}
      {recording.phase === "idle" ||
      recording.phase === "stopped" ||
      recording.phase === "scrubbing" ? (
        <ActionButton
          variant="default"
          onPress={onStartRecording}
          isDisabled={isBusy || connCount === 0}
        >
          <Record size={14} weight="fill" />
          Record
        </ActionButton>
      ) : (
        <ActionButton variant="default" onPress={onStopRecording}>
          <Stop size={14} weight="fill" />
          Stop
        </ActionButton>
      )}
      <ActionButton
        variant="default"
        onPress={onTakeSnapshot}
        isDisabled={isBusy || connCount === 0}
      >
        {isBusy ? <CircleNotch size={14} weight="bold" /> : <Camera size={14} weight="bold" />}
        {buttonLabel}
      </ActionButton>
      <input
        ref={fileInputRef}
        type="file"
        accept=".json,application/json"
        style={{ display: "none" }}
        onChange={onImportFile}
      />
    </div>
  );
}
