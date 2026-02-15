import { useCallback, useEffect, useRef } from "react";
import { ListBullets } from "@phosphor-icons/react";
import type { StuckRequest } from "../types";

interface RequestsTableProps {
  requests: StuckRequest[];
  selectedId: string | null;
  onSelect: (req: StuckRequest) => void;
}

function formatElapsed(ns: number): string {
  const secs = ns / 1_000_000_000;
  if (secs >= 60) return `${(secs / 60).toFixed(1)}m`;
  return `${secs.toFixed(1)}s`;
}

function elapsedClass(ns: number): string {
  const secs = ns / 1_000_000_000;
  if (secs >= 30) return "elapsed-hot";
  if (secs >= 10) return "elapsed-warm";
  return "";
}

export function RequestsTable({ requests, selectedId, onSelect }: RequestsTableProps) {
  const tbodyRef = useRef<HTMLTableSectionElement>(null);

  const selectedIndex = requests.findIndex((r) => r.id === selectedId);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (requests.length === 0) return;
      const idx = selectedIndex >= 0 ? selectedIndex : 0;

      switch (e.key) {
        case "ArrowDown": {
          e.preventDefault();
          const next = Math.min(idx + 1, requests.length - 1);
          onSelect(requests[next]);
          scrollToRow(next);
          break;
        }
        case "ArrowUp": {
          e.preventDefault();
          const prev = Math.max(idx - 1, 0);
          onSelect(requests[prev]);
          scrollToRow(prev);
          break;
        }
        case "Enter": {
          e.preventDefault();
          if (idx >= 0 && idx < requests.length) {
            onSelect(requests[idx]);
          }
          break;
        }
      }
    },
    [requests, selectedIndex, onSelect],
  );

  function scrollToRow(idx: number) {
    const tbody = tbodyRef.current;
    if (!tbody) return;
    const row = tbody.children[idx] as HTMLElement | undefined;
    row?.scrollIntoView({ block: "nearest" });
  }

  useEffect(() => {
    if (selectedIndex >= 0) scrollToRow(selectedIndex);
  }, [selectedId]);

  return (
    <div className="panel" tabIndex={0} onKeyDown={handleKeyDown}>
      <div className="panel-header">
        <ListBullets size={14} weight="bold" /> Stuck requests ({requests.length})
      </div>
      {requests.length === 0 ? (
        <div
          style={{ padding: "16px 12px", color: "light-dark(#6e6e73, #98989d)", fontSize: "12px" }}
        >
          No stuck requests found.
        </div>
      ) : (
        <table className="requests-table">
          <thead>
            <tr>
              <th>method</th>
              <th>elapsed</th>
              <th>process</th>
              <th>task</th>
            </tr>
          </thead>
          <tbody ref={tbodyRef}>
            {requests.map((req) => (
              <tr key={req.id} data-selected={req.id === selectedId} onClick={() => onSelect(req)}>
                <td>{req.method ?? "—"}</td>
                <td className={elapsedClass(req.elapsed_ns)}>{formatElapsed(req.elapsed_ns)}</td>
                <td>{req.process}</td>
                <td style={{ fontSize: "11px", opacity: 0.7 }}>{req.task_id ?? "—"}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
