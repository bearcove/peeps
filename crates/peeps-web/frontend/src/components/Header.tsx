import { useEffect, useMemo, useState } from "react";
import {
  Aperture,
  Camera,
  CheckCircle,
  CircleNotch,
  Clock,
  WarningCircle,
  X,
  Warning,
} from "@phosphor-icons/react";
import type { JumpNowResponse, SnapshotProgressResponse } from "../types";
import type { SnapshotProcessInfo, ProcessDebugResponse } from "../types";

const DASH = "—";
const PROCESS_STATUS_UNKNOWN = "unknown";

interface HeaderProps {
  snapshot: JumpNowResponse | null;
  loading: boolean;
  progress: SnapshotProgressResponse | null;
  snapshotProcesses: SnapshotProcessInfo[];
  canTakeSnapshot: boolean;
  onTakeSnapshot: () => void;
  onProcessDebugAction: (
    process: SnapshotProcessInfo,
    action: "sample" | "spindump",
  ) => Promise<ProcessDebugResponse>;
  processDebugMessage: string | null;
}

interface SnapshotProcessRow extends SnapshotProcessInfo {
  responded: boolean;
}

function normalizeStatus(value: string | undefined): string {
  return value?.trim().toLowerCase() ?? "";
}

function statusIsResponded(value: string | undefined): boolean {
  return normalizeStatus(value) === "responded";
}

function hasExplicitProcessStatus(value: string | undefined): boolean {
  const normalized = normalizeStatus(value);
  return normalized.length > 0 && normalized !== PROCESS_STATUS_UNKNOWN && normalized !== "pending";
}

export function Header({
  snapshot,
  loading,
  progress,
  snapshotProcesses,
  canTakeSnapshot,
  onTakeSnapshot,
  onProcessDebugAction,
  processDebugMessage,
}: HeaderProps) {
  const requested = progress?.requested ?? 0;
  const responded = progress?.responded ?? 0;
  const preview = (progress?.responded_processes ?? []).slice(-4);
  const hiddenCount = Math.max(0, (progress?.responded_processes.length ?? 0) - preview.length);
  const [processModalOpen, setProcessModalOpen] = useState(false);
  const [actionInFlight, setActionInFlight] = useState<string | null>(null);
  const [actionResultUrls, setActionResultUrls] = useState<Record<string, string>>({});
  const [feedbackMessage, setFeedbackMessage] = useState<string | null>(null);

  const respondedProcessSet = useMemo(() => {
    if (!progress) return new Set<string>();
    return new Set(progress.responded_processes);
  }, [progress]);

  const progressAwareProcesses = useMemo<SnapshotProcessRow[]>(() => {
    const byProcKey = new Map<string, SnapshotProcessRow>();
    for (const proc of snapshotProcesses) {
      byProcKey.set(proc.proc_key, {
        ...proc,
        responded: false,
      });
    }

    if (progress != null) {
      for (const procKey of progress.responded_processes) {
        const existing = byProcKey.get(procKey);
        if (existing) {
          existing.responded = true;
          continue;
        }
        byProcKey.set(procKey, {
          process: procKey,
          pid: null,
          proc_key: procKey,
          status: "responded",
          recv_at_ns: null,
          error_text: null,
          command: null,
          cmd_args_preview: null,
          responded: true,
        });
      }

      for (const procKey of progress.pending_processes) {
        const existing = byProcKey.get(procKey);
        if (existing) {
          existing.responded = false;
          continue;
        }
        byProcKey.set(procKey, {
          process: procKey,
          pid: null,
          proc_key: procKey,
          status: "missing",
          recv_at_ns: null,
          error_text: null,
          command: null,
          cmd_args_preview: null,
          responded: false,
        });
      }
    }

    return Array.from(byProcKey.values());
  }, [snapshotProcesses, progress]);

  const useProgressForStatus = respondedProcessSet.size > 0 || (progress?.requested ?? 0) > 0;

  const sortedProcesses = useMemo<SnapshotProcessRow[]>(() => {
    return progressAwareProcesses
      .map((proc) => {
        if (!useProgressForStatus || hasExplicitProcessStatus(proc.status)) {
          return { ...proc, responded: statusIsResponded(proc.status) };
        }
        return { ...proc, responded: respondedProcessSet.has(proc.proc_key) };
      })
      .sort((a, b) => {
        if (a.responded !== b.responded) return a.responded ? 1 : -1;
        const byProcess = a.process.localeCompare(b.process);
        if (byProcess !== 0) return byProcess;
        return a.proc_key.localeCompare(b.proc_key);
      });
  }, [progressAwareProcesses, respondedProcessSet, useProgressForStatus]);

  const statusText = (proc: SnapshotProcessRow) => {
    if (hasExplicitProcessStatus(proc.status)) return proc.status;
    if (useProgressForStatus) return proc.responded ? "responded" : "missing";
    if (!proc.status) return DASH;
    return proc.status;
  };

  const getStatusClass = (proc: SnapshotProcessRow) => {
    if (!useProgressForStatus) return "snapshot-process-table__status--normal";
    return proc.responded ? "snapshot-process-table__status--responded" : "snapshot-process-table__status--missing";
  };

  const displayedMessage = feedbackMessage ?? processDebugMessage;

  useEffect(() => {
    if (!processModalOpen) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setProcessModalOpen(false);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [processModalOpen]);

  async function onProcessAction(proc: SnapshotProcessRow, action: "sample" | "spindump") {
    const actionKey = `${proc.proc_key}:${action}`;
    if (actionInFlight) return;
    setActionInFlight(actionKey);
    setFeedbackMessage(null);
    try {
      const response = await onProcessDebugAction(proc, action);
      setActionResultUrls((prev) => {
        if (!response.result_url) {
          if (!(actionKey in prev)) return prev;
          const next = { ...prev };
          delete next[actionKey];
          return next;
        }
        return {
          ...prev,
          [actionKey]: response.result_url,
        };
      });
      setFeedbackMessage(
        response.result_url
          ? `${action} output ready for ${proc.process || proc.proc_key}`
          : `${action} did not run for ${proc.process || proc.proc_key}: ${response.status}`,
      );
      window.setTimeout(() => setFeedbackMessage(null), 1600);
    } catch (err) {
      setFeedbackMessage(err instanceof Error ? err.message : "debug action failed");
      window.setTimeout(() => setFeedbackMessage(null), 2200);
    } finally {
      setActionInFlight((prev) => (prev === actionKey ? null : prev));
    }
  }

  return (
    <div className="header">
      <Aperture size={18} weight="bold" />
      <span className="header-title">peeps</span>
      <span className={`snapshot-badge ${snapshot ? "snapshot-badge--active" : ""}`}>
        {snapshot ? (
          <>
            <CheckCircle size={12} weight="bold" />
            snapshot #{snapshot.snapshot_id}
          </>
        ) : (
          <>
            <Clock size={12} weight="bold" />
            no snapshot yet
          </>
        )}
      </span>
      {snapshot && (
        <button
          type="button"
          className="snapshot-badge snapshot-badge--interactive"
          onClick={() => setProcessModalOpen(true)}
          title="View snapshot processes"
        >
          {snapshot.responded}/{snapshot.requested} responded
          {snapshot.timed_out > 0 && (
            <>
              <Warning size={12} weight="bold" style={{ color: "light-dark(#bf5600, #ffa94d)" }} />
              {snapshot.timed_out} timed out
            </>
          )}
          {snapshot.error > 0 && (
            <>
              <WarningCircle size={12} weight="bold" style={{ color: "light-dark(#b30000, #ff6b6b)" }} />
              {snapshot.error} errored
            </>
          )}
        </button>
      )}
      {loading && (
        <span className="snapshot-progress" role="status" aria-live="polite">
          <span className="snapshot-progress__label">
            <CircleNotch size={12} weight="bold" className="snapshot-progress__spinner" />
            {requested > 0
              ? `Received from ${responded}/${requested} process${requested === 1 ? "" : "es"}`
              : "Taking snapshot..."}
          </span>
          {(preview.length > 0 || hiddenCount > 0) && (
            <span className="snapshot-progress__meta">
              {preview.join(", ")}
              {hiddenCount > 0 ? ` +${hiddenCount}` : ""}
            </span>
          )}
          <span className="snapshot-progress__bar" />
        </span>
      )}
      <span className="header-spacer" />
      <button
        className={`btn btn--primary ${loading ? "btn--loading" : ""}`}
        onClick={onTakeSnapshot}
        disabled={loading || !canTakeSnapshot}
      >
        <Camera size={14} weight="bold" />
        {loading ? "Taking snapshot..." : "Take snapshot"}
      </button>

      {processModalOpen && (
        <div className="snapshot-process-modal-backdrop" onClick={() => setProcessModalOpen(false)}>
          <div
            className="snapshot-process-modal"
            role="dialog"
            aria-modal="true"
            aria-labelledby="snapshot-process-modal-title"
            onClick={(event) => event.stopPropagation()}
          >
            <div className="snapshot-process-modal__header">
              <div className="snapshot-process-modal__title-wrap">
                <span id="snapshot-process-modal-title" className="snapshot-process-modal__title">
                  Snapshot processes
                </span>
                <span className="snapshot-process-modal__subtitle">
                  Missing processes appear first.
                </span>
              </div>
              <button
                className="snapshot-process-modal__close"
                type="button"
                onClick={() => setProcessModalOpen(false)}
                title="Close"
              >
                <X size={14} weight="bold" />
              </button>
            </div>
            {displayedMessage && <p className="snapshot-process-modal__message">{displayedMessage}</p>}
            <div className="snapshot-process-modal__table-wrap">
              {sortedProcesses.length === 0 ? (
                <div className="snapshot-process-modal__empty">No process metadata for this snapshot.</div>
              ) : (
                <table className="snapshot-process-table">
                  <thead>
                    <tr>
                      <th>Process</th>
                      <th>Proc key</th>
                      <th>PID</th>
                      <th>Status</th>
                      <th>Command</th>
                      <th>Actions</th>
                    </tr>
                  </thead>
                  <tbody>
                    {sortedProcesses.map((proc) => {
                      const actionKey = `${proc.proc_key}:sample`;
                      const spindumpActionKey = `${proc.proc_key}:spindump`;
                      const commandText = proc.command ? proc.command : proc.cmd_args_preview ? proc.cmd_args_preview : "—";
                      const disabled = proc.pid == null;
                      return (
                        <tr key={proc.proc_key} className={`snapshot-process-table__row ${proc.responded ? "" : "snapshot-process-table__row--missing"}`}>
                          <td>{proc.process}</td>
                          <td className="snapshot-process-table__mono">{proc.proc_key}</td>
                          <td className="snapshot-process-table__mono">{proc.pid == null ? "—" : proc.pid}</td>
                          <td>
                            <span className={`snapshot-process-table__status ${getStatusClass(proc)}`}>
                              <span title={proc.error_text ?? undefined}>{statusText(proc)}</span>
                            </span>
                          </td>
                          <td className="snapshot-process-table__command" title={commandText}>
                            {commandText}
                          </td>
                          <td className="snapshot-process-table__actions">
                            {actionInFlight === actionKey ? (
                              <button
                                type="button"
                                className="snapshot-process-btn snapshot-process-btn--loading"
                                disabled
                              >
                                <CircleNotch size={10} weight="bold" className="snapshot-process-btn__spinner" />
                                sample running
                              </button>
                            ) : actionResultUrls[actionKey] ? (
                              <a
                                href={actionResultUrls[actionKey]}
                                className="snapshot-process-btn"
                                target="_blank"
                                rel="noreferrer"
                                title="Open sample result"
                              >
                                Open sample txt
                              </a>
                            ) : (
                              <button
                                type="button"
                                className="snapshot-process-btn"
                                onClick={() => void onProcessAction(proc, "sample")}
                                disabled={disabled}
                                title={disabled ? "PID not available" : "Run sample command"}
                              >
                                sample
                              </button>
                            )}
                            {actionInFlight === spindumpActionKey ? (
                              <button
                                type="button"
                                className="snapshot-process-btn snapshot-process-btn--loading"
                                disabled
                              >
                                <CircleNotch size={10} weight="bold" className="snapshot-process-btn__spinner" />
                                spindump running
                              </button>
                            ) : actionResultUrls[spindumpActionKey] ? (
                              <a
                                href={actionResultUrls[spindumpActionKey]}
                                className="snapshot-process-btn"
                                target="_blank"
                                rel="noreferrer"
                                title="Open spindump result"
                              >
                                Open spindump txt
                              </a>
                            ) : (
                              <button
                                type="button"
                                className="snapshot-process-btn"
                                onClick={() => void onProcessAction(proc, "spindump")}
                                disabled={disabled}
                                title={disabled ? "PID not available" : "Run spindump command"}
                              >
                                spindump
                              </button>
                            )}
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              )}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
