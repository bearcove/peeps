import React from "react";
import {
  Aperture,
  Camera,
  CheckCircle,
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
  recording,
  connCount,
  waitingForProcesses,
  isBusy,
  isLive,
  onSetIsLive,
  onShowProcessModal,
  onTakeSnapshot,
  onStartRecording,
  onStopRecording,
  onExport,
  onImportClick,
  fileInputRef,
  onImportFile,
}: {
  leftPaneTab: "graph" | "scopes";
  onLeftPaneTabChange: (tab: "graph" | "scopes") => void;
  snap: SnapshotState;
  recording: RecordingState;
  connCount: number;
  waitingForProcesses: boolean;
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
}) {
  const buttonLabel =
    snap.phase === "cutting" ? "Syncing…" : snap.phase === "loading" ? "Loading…" : "Take Snapshot";

  return (
    <div className="app-header">
      <Aperture size={16} weight="bold" />
      <span className="app-header-title">peeps</span>
      <SegmentedGroup
        size="sm"
        aria-label="Primary page"
        value={leftPaneTab}
        onChange={(value) => onLeftPaneTabChange(value as "graph" | "scopes")}
        options={[
          { value: "graph", label: "Graph" },
          { value: "scopes", label: "Scopes" },
        ]}
      />
      <button
        type="button"
        className={`proc-pill${connCount > 0 ? " proc-pill--connected" : " proc-pill--disconnected"}`}
        onClick={onShowProcessModal}
        title="Click to see connected processes"
      >
        {waitingForProcesses ? (
          <>
            <CircleNotch size={11} weight="bold" className="spinning" /> waiting…
          </>
        ) : (
          <>
            {connCount} {connCount === 1 ? "process" : "processes"}
          </>
        )}
      </button>
      {apiMode === "lab" ? (
        <span className="app-header-badge">mock data</span>
      ) : snap.phase === "ready" ? (
        <span className="app-header-badge app-header-badge--active">
          <CheckCircle size={12} weight="bold" />
          snapshot
        </span>
      ) : null}
      {snap.phase === "error" && <span className="app-header-error">{snap.message}</span>}
      <span className="app-header-spacer" />
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
        <ActionButton
          variant={isLive ? "primary" : "default"}
          onPress={() => onSetIsLive((v) => !v)}
        >
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
      <ActionButton variant="primary" onPress={onTakeSnapshot} isDisabled={isBusy}>
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
