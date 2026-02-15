import { Aperture, Camera, CheckCircle, Clock, Warning } from "@phosphor-icons/react";
import type { JumpNowResponse } from "../types";

interface HeaderProps {
  snapshot: JumpNowResponse | null;
  loading: boolean;
  onJumpNow: () => void;
}

export function Header({ snapshot, loading, onJumpNow }: HeaderProps) {
  return (
    <div class="header">
      <Aperture size={18} weight="bold" />
      <span class="header-title">peeps</span>
      <span class={`snapshot-badge ${snapshot ? "snapshot-badge--active" : ""}`}>
        {snapshot ? (
          <>
            <CheckCircle size={12} weight="bold" />
            snapshot #{snapshot.snapshot_id}
          </>
        ) : (
          <>
            <Clock size={12} weight="bold" />
            no snapshot
          </>
        )}
      </span>
      {snapshot && (
        <span class="snapshot-badge">
          {snapshot.responded}/{snapshot.requested} responded
          {snapshot.timed_out > 0 && (
            <>
              <Warning size={12} weight="bold" style={{ color: "light-dark(#bf5600, #ffa94d)" }} />
              {snapshot.timed_out} timed out
            </>
          )}
        </span>
      )}
      <span class="header-spacer" />
      <button
        class={`btn btn--primary ${loading ? "btn--loading" : ""}`}
        onClick={onJumpNow}
        disabled={loading}
      >
        <Camera size={14} weight="bold" />
        {loading ? "Jumping..." : "Jump to now"}
      </button>
    </div>
  );
}
