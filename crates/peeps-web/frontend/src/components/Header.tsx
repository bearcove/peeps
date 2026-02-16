import { Aperture, Camera, CheckCircle, CircleNotch, Clock, Warning } from "@phosphor-icons/react";
import type { JumpNowResponse } from "../types";

interface HeaderProps {
  snapshot: JumpNowResponse | null;
  loading: boolean;
  onTakeSnapshot: () => void;
}

export function Header({ snapshot, loading, onTakeSnapshot }: HeaderProps) {
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
            Taking snapshot…
          </span>
          <span className="snapshot-progress__bar" />
        </span>
      )}
      <span className="header-spacer" />
      <button
        className={`btn btn--primary ${loading ? "btn--loading" : ""}`}
        onClick={onTakeSnapshot}
        disabled={loading}
      >
        <Camera size={14} weight="bold" />
        {loading ? "Taking snapshot..." : "Take snapshot"}
      </button>
    </div>
  );
}
