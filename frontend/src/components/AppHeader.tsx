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
  snapshotProcessCount,
  recording,
  connCount,
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
  leftPaneTab: "graph" | "scopes" | "entities";
  onLeftPaneTabChange: (tab: "graph" | "scopes" | "entities") => void;
  snap: SnapshotState;
  snapshotProcessCount: number;
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
}) {
  const buttonLabel =
    snap.phase === "cutting" ? "Syncing…" : snap.phase === "loading" ? "Loading…" : "Take Snapshot";

  return (
    <div className="app-header">
      <img className="app-header-logo" src="/favicon-light.svg" alt="" width={16} height={16} aria-hidden="true" />
      <span className="app-header-title">peeps</span>
      <SegmentedGroup
        size="sm"
        aria-label="Primary page"
        value={leftPaneTab}
        onChange={(value) => onLeftPaneTabChange(value as "graph" | "scopes" | "entities")}
        options={[
          { value: "graph", label: "Graph" },
          {
            value: "scopes",
            label: <span className="app-header-tab-label">Scopes</span>,
          },
          { value: "entities", label: "Entities" },
        ]}
      />
      {connCount > 0 && (
        <button
          type="button"
          className="proc-pill proc-pill--connected"
          onClick={onShowProcessModal}
          title="Click to see connected processes"
        >
          {connCount} {connCount === 1 ? "process" : "processes"}
        </button>
      )}
      {apiMode === "lab" ? (
        <span className="app-header-badge">mock data</span>
      ) : null}
      {snap.phase === "ready" && (
        <span className="app-header-badge">Snapshot: {snapshotProcessCount} processes</span>
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
        <ActionButton
          variant="default"
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
      <ActionButton variant="default" onPress={onTakeSnapshot} isDisabled={isBusy || connCount === 0}>
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
