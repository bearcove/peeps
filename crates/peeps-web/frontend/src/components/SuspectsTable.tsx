import { useCallback, useEffect, useRef } from "react";
import { WarningOctagon, CaretLeft, CaretRight } from "@phosphor-icons/react";

export interface SuspectItem {
  id: string;
  label: string;
  process: string;
  reason: string;
  age_ns: number | null;
  score: number;
}

interface SuspectsTableProps {
  suspects: SuspectItem[];
  selectedId: string | null;
  onSelect: (suspect: SuspectItem) => void;
  collapsed: boolean;
  onToggleCollapse: () => void;
}

function formatElapsed(ns: number | null): string {
  if (ns == null) return "—";
  const secs = ns / 1_000_000_000;
  if (secs >= 60) return `${(secs / 60).toFixed(1)}m`;
  return `${secs.toFixed(1)}s`;
}

function reasonLabel(reason: string): string {
  switch (reason) {
    case "needs_cycle":
      return "cycle";
    case "in_poll_stuck":
      return "in-poll";
    case "pending_idle":
      return "idle";
    case "contended_wait":
      return "contended";
    default:
      return reason;
  }
}

export function SuspectsTable({
  suspects,
  selectedId,
  onSelect,
  collapsed,
  onToggleCollapse,
}: SuspectsTableProps) {
  const listRef = useRef<HTMLDivElement>(null);
  const selectedIndex = suspects.findIndex((s) => s.id === selectedId);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (suspects.length === 0) return;
      const idx = selectedIndex >= 0 ? selectedIndex : 0;

      switch (e.key) {
        case "ArrowDown": {
          e.preventDefault();
          const next = Math.min(idx + 1, suspects.length - 1);
          onSelect(suspects[next]);
          scrollToRow(next);
          break;
        }
        case "ArrowUp": {
          e.preventDefault();
          const prev = Math.max(idx - 1, 0);
          onSelect(suspects[prev]);
          scrollToRow(prev);
          break;
        }
        case "Enter": {
          e.preventDefault();
          if (idx >= 0 && idx < suspects.length) onSelect(suspects[idx]);
          break;
        }
      }
    },
    [suspects, selectedIndex, onSelect],
  );

  function scrollToRow(idx: number) {
    const list = listRef.current;
    if (!list) return;
    const row = list.children[idx] as HTMLElement | undefined;
    row?.scrollIntoView({ block: "nearest" });
  }

  useEffect(() => {
    if (selectedIndex >= 0) scrollToRow(selectedIndex);
  }, [selectedId, selectedIndex]);

  if (collapsed) {
    return (
      <div className="panel panel--collapsed">
        <button className="panel-collapse-btn" onClick={onToggleCollapse} title="Expand panel">
          <CaretRight size={14} weight="bold" />
        </button>
        <span className="panel-collapsed-label">Suspects</span>
      </div>
    );
  }

  return (
    <div className="panel" tabIndex={0} onKeyDown={handleKeyDown}>
      <div className="panel-header">
        <WarningOctagon size={14} weight="bold" /> Suspects ({suspects.length})
        <button className="panel-collapse-btn" onClick={onToggleCollapse} title="Collapse panel">
          <CaretLeft size={14} weight="bold" />
        </button>
      </div>
      {suspects.length === 0 ? (
        <div
          style={{ padding: "16px 12px", color: "light-dark(#6e6e73, #98989d)", fontSize: "12px" }}
        >
          No deadlock suspects in this snapshot.
        </div>
      ) : (
        <div className="request-list" ref={listRef}>
          {suspects.map((suspect) => (
            <button
              key={suspect.id}
              type="button"
              className="request-card"
              data-selected={suspect.id === selectedId}
              onClick={() => onSelect(suspect)}
            >
              <div className="request-card-top">
                <span className="request-card-main" title={`${suspect.label} • ${suspect.process}`}>
                  {suspect.label}
                  <span className="request-card-process-inline">{suspect.process}</span>
                </span>
                <span className="request-card-elapsed elapsed-hot">{formatElapsed(suspect.age_ns)}</span>
              </div>
              <div className="request-card-bottom">
                <span className="request-card-meta" title={suspect.reason}>
                  {reasonLabel(suspect.reason)} • score {suspect.score}
                </span>
              </div>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
