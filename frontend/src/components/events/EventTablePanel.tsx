import React, { useMemo, useState } from "react";
import { Table, type Column } from "../../ui/primitives/Table";
import { TextInput } from "../../ui/primitives/TextInput";
import { Select } from "../../ui/primitives/Select";
import { Badge } from "../../ui/primitives/Badge";
import { canonicalNodeKind, kindDisplayName, kindIcon } from "../../nodeKindSpec";
import type { EventDef } from "../../snapshot";
import "./EventTablePanel.css";

type SortKey = "time" | "kind" | "target";
type SortDir = "asc" | "desc";

const ALL_KINDS = "__all_event_kinds__";

function formatTime(unixMs: number): string {
  const d = new Date(unixMs);
  const h = String(d.getHours()).padStart(2, "0");
  const m = String(d.getMinutes()).padStart(2, "0");
  const s = String(d.getSeconds()).padStart(2, "0");
  const ms = String(d.getMilliseconds()).padStart(3, "0");
  return `${h}:${m}:${s}.${ms}`;
}

function compareEvents(a: EventDef, b: EventDef, key: SortKey): number {
  if (key === "time") return b.atApproxUnixMs - a.atApproxUnixMs;
  if (key === "kind") {
    return (
      a.kindDisplayName.localeCompare(b.kindDisplayName) ||
      b.atApproxUnixMs - a.atApproxUnixMs
    );
  }
  return a.targetName.localeCompare(b.targetName) || b.atApproxUnixMs - a.atApproxUnixMs;
}

export function EventTablePanel({
  eventDefs,
  onSelectEntity,
}: {
  eventDefs: EventDef[];
  onSelectEntity: (entityId: string) => void;
}) {
  const [search, setSearch] = useState("");
  const [selectedKind, setSelectedKind] = useState<string>(ALL_KINDS);
  const [sortKey, setSortKey] = useState<SortKey>("time");
  const [sortDir, setSortDir] = useState<SortDir>("desc");

  const kindOptions = useMemo(() => {
    const seen = new Map<string, string>();
    for (const e of eventDefs) {
      if (!seen.has(e.kindKey)) seen.set(e.kindKey, e.kindDisplayName);
    }
    const sorted = Array.from(seen.entries()).sort((a, b) => a[1].localeCompare(b[1]));
    return [
      { value: ALL_KINDS, label: "All kinds" },
      ...sorted.map(([key, label]) => ({ value: key, label })),
    ];
  }, [eventDefs]);

  const filteredRows = useMemo(() => {
    const query = search.trim().toLowerCase();
    return eventDefs.filter((event) => {
      if (selectedKind !== ALL_KINDS && event.kindKey !== selectedKind) return false;
      if (!query) return true;
      const haystack = [
        event.targetName,
        event.kindKey,
        event.kindDisplayName,
        event.targetEntityKind ?? "",
        formatTime(event.atApproxUnixMs),
      ]
        .join(" ")
        .toLowerCase();
      return haystack.includes(query);
    });
  }, [eventDefs, search, selectedKind]);

  const sortedRows = useMemo(() => {
    const sorted = [...filteredRows].sort((a, b) => compareEvents(a, b, sortKey));
    return sortDir === "desc" ? sorted : sorted.reverse();
  }, [filteredRows, sortKey, sortDir]);

  const columns = useMemo<readonly Column<EventDef>[]>(
    () => [
      {
        key: "time",
        label: "Time",
        sortable: true,
        width: "0.7fr",
        render: (row) => (
          <span className="event-table-mono">{formatTime(row.atApproxUnixMs)}</span>
        ),
      },
      {
        key: "kind",
        label: "Kind",
        sortable: true,
        width: "0.6fr",
        render: (row) => (
          <Badge tone="neutral">{row.kindDisplayName}</Badge>
        ),
      },
      {
        key: "target",
        label: "Target",
        sortable: true,
        width: "1.4fr",
        render: (row) => {
          const cellClass = row.targetRemoved ? "event-table-target event-table-removed" : "event-table-target";
          return (
            <span className={cellClass}>
              {row.targetEntityKind && (
                <>
                  {kindIcon(canonicalNodeKind(row.targetEntityKind), 12)}
                  <span className="event-table-kind-pill">{kindDisplayName(canonicalNodeKind(row.targetEntityKind))}</span>
                </>
              )}
              <span className="event-table-name">{row.targetName}</span>
            </span>
          );
        },
      },
    ],
    [],
  );

  return (
    <div className="event-table-panel">
      <div className="event-table-toolbar">
        <span className="event-table-stats">
          {filteredRows.length} / {eventDefs.length} events
        </span>
        <div className="event-table-toolbar-controls">
          <TextInput
            value={search}
            onChange={setSearch}
            placeholder="Search events..."
            aria-label="Search events"
            className="event-table-search"
          />
          <Select
            value={selectedKind}
            onChange={setSelectedKind}
            options={kindOptions}
            aria-label="Filter by kind"
          />
        </div>
      </div>
      <div className="event-table-body">
        <Table
          aria-label="Events"
          columns={columns}
          rows={sortedRows}
          rowKey={(row) => row.id}
          onRowClick={(row) => {
            if (row.targetKind === "entity") {
              onSelectEntity(row.targetId);
            }
          }}
          sortKey={sortKey}
          sortDir={sortDir}
          onSort={(key) => {
            const nextKey = key as SortKey;
            if (nextKey === sortKey) {
              setSortDir((prev) => (prev === "asc" ? "desc" : "asc"));
              return;
            }
            setSortKey(nextKey);
            setSortDir(nextKey === "time" ? "desc" : "asc");
          }}
        />
      </div>
    </div>
  );
}
