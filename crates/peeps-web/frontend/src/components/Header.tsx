import { Aperture, Camera, CheckCircle, CircleNotch, Clock, Warning } from "@phosphor-icons/react";
import type { JumpNowResponse, SnapshotProgressResponse } from "../types";

interface HeaderProps {
  snapshot: JumpNowResponse | null;
  loading: boolean;
  progress: SnapshotProgressResponse | null;
  canTakeSnapshot: boolean;
  onTakeSnapshot: () => void;
}

export function Header({ snapshot, loading, progress, canTakeSnapshot, onTakeSnapshot }: HeaderProps) {
  const requested = progress?.requested ?? 0;
  const responded = progress?.responded ?? 0;
  const preview = (progress?.responded_processes ?? []).slice(-4);
  const hiddenCount = Math.max(0, (progress?.responded_processes.length ?? 0) - preview.length);

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
        <span className="snapshot-badge">
          {snapshot.responded}/{snapshot.requested} responded
          {snapshot.timed_out > 0 && (
            <>
              <Warning size={12} weight="bold" style={{ color: "light-dark(#bf5600, #ffa94d)" }} />
              {snapshot.timed_out} timed out
            </>
          )}
        </span>
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
    </div>
  );
}
