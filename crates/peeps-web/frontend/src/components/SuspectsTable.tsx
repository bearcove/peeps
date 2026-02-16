import { useCallback, useEffect, useRef } from "react";
import { WarningOctagon, CaretLeft, CaretRight, Circle } from "@phosphor-icons/react";
import { kindMeta, ProcessSwatch } from "./NodeCards";

export interface SuspectItem {
  id: string;
  kind: string;
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
      return "cycle detected";
    case "in_poll_stuck":
      return "stuck in poll";
    case "pending_idle":
      return "pending + idle";
    case "contended_wait":
      return "contended";
    default:
      return reason;
  }
}

function processLabel(process: string): string {
  if (process.length <= 20) return process;
  return `${process.slice(0, 12)}…${process.slice(-6)}`;
}

function kindLabel(kind: string): string {
  return kindMeta[kind]?.displayName ?? kind;
}

function reasonTone(reason: string): "critical" | "warn" | "info" {
  if (reason === "needs_cycle" || reason === "in_poll_stuck") return "critical";
  if (reason === "contended_wait") return "warn";
  return "info";
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
        <div className="suspect-list" ref={listRef}>
          {suspects.map((suspect) => (
            <button
              key={suspect.id}
              type="button"
              className="suspect-card"
              data-selected={suspect.id === selectedId}
              data-tone={reasonTone(suspect.reason)}
              onClick={() => onSelect(suspect)}
            >
              <div className="suspect-card-top">
                <span className="suspect-kind-pill" title={kindLabel(suspect.kind)}>
                  <span className="suspect-kind-icon">{kindMeta[suspect.kind]?.icon ?? <Circle size={12} weight="fill" />}</span>
                  <span>{kindLabel(suspect.kind)}</span>
                </span>
                <span className="request-card-elapsed elapsed-hot">{formatElapsed(suspect.age_ns)}</span>
              </div>
              <div className="suspect-card-title" title={`${suspect.label} • ${suspect.process}`}>
                <span className="suspect-process-swatch">
                  <ProcessSwatch process={suspect.process} size={12} />
                </span>
                <span className="request-card-main">
                  {suspect.label}
                </span>
              </div>
              <div className="suspect-card-bottom">
                <span className="suspect-process-chip" title={suspect.process}>
                  {processLabel(suspect.process)}
                </span>
                <span className="request-card-meta" title={suspect.reason}>
                  {reasonLabel(suspect.reason)}
                </span>
                <span className="suspect-score">score {suspect.score}</span>
              </div>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
