import React, { useEffect } from "react";
import { ActionButton } from "../ui/primitives/ActionButton";
import { Table, type Column } from "../ui/primitives/Table";
import type { ConnectedProcessInfo, ConnectionsResponse } from "../api/types";
import { formatProcessLabel } from "../processLabel";
import "./ProcessModal.css";

const PROCESS_COLUMNS: readonly Column<ConnectedProcessInfo>[] = [
  { key: "conn_id", label: "Conn", width: "60px", render: (r) => r.conn_id },
  { key: "process", label: "Process", render: (r) => formatProcessLabel(r.process_name, r.pid) },
];

export function ProcessModal({
  connections,
  onClose,
}: {
  connections: ConnectionsResponse;
  onClose: () => void;
}) {
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") onClose();
    }
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-label="Connected processes"
      >
        <div className="modal-header">
          <span className="modal-title">Connected processes</span>
          <ActionButton size="sm" onPress={onClose}>
            ✕
          </ActionButton>
        </div>
        <div className="modal-body">
          <Table
            columns={PROCESS_COLUMNS}
            rows={connections.processes}
            rowKey={(r) => String(r.conn_id)}
            aria-label="Connected processes"
          />
          {connections.processes.length === 0 && (
            <div className="modal-empty">No processes connected</div>
          )}
        </div>
      </div>
    </div>
  );
}
