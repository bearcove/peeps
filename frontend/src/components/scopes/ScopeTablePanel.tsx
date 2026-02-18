import React, { useEffect, useMemo, useState } from "react";
import { ArrowsClockwise, CircleNotch } from "@phosphor-icons/react";
import { apiClient } from "../../api";
import { Table, type Column } from "../../ui/primitives/Table";
import { ActionButton } from "../../ui/primitives/ActionButton";
import { TextInput } from "../../ui/primitives/TextInput";
import { Select } from "../../ui/primitives/Select";
import "./ScopeTablePanel.css";

type ScopeRow = {
  key: string;
  processId: string;
  processName: string;
  pid: number | null;
  streamId: string;
  scopeId: string;
  scopeName: string;
  scopeKind: string;
  memberCount: number;
};

type SortKey = "process" | "kind" | "name" | "members";
type SortDir = "asc" | "desc";

const SCOPE_ROWS_SQL = `
select
  s.conn_id as process_id,
  c.process_name,
  c.pid,
  s.stream_id,
  s.scope_id,
  json_extract(s.scope_json, '$.name') as scope_name,
  json_extract(s.scope_json, '$.body') as scope_kind,
  count(distinct l.entity_id) as member_count
from scopes s
left join connections c on c.conn_id = s.conn_id
left join entity_scope_links l
  on l.conn_id = s.conn_id
 and l.stream_id = s.stream_id
 and l.scope_id = s.scope_id
group by
  s.conn_id,
  c.process_name,
  c.pid,
  s.stream_id,
  s.scope_id,
  json_extract(s.scope_json, '$.name'),
  json_extract(s.scope_json, '$.body')
order by
  c.process_name asc,
  scope_kind asc,
  scope_name asc,
  s.scope_id asc
`;

function toRowArray(row: unknown): unknown[] | null {
  return Array.isArray(row) ? row : null;
}

function asString(value: unknown): string {
  if (typeof value === "string") return value;
  if (value == null) return "";
  return String(value);
}

function asNumber(value: unknown): number | null {
  if (typeof value === "number" && Number.isFinite(value)) return value;
  if (typeof value === "string" && value.trim() !== "") {
    const parsed = Number(value);
    if (Number.isFinite(parsed)) return parsed;
  }
  return null;
}

function parseScopeRows(rows: unknown[]): ScopeRow[] {
  const out: ScopeRow[] = [];
  for (const raw of rows) {
    const row = toRowArray(raw);
    if (!row || row.length < 8) continue;

    const processId = asString(row[0]);
    const processName = asString(row[1]);
    const pid = asNumber(row[2]);
    const streamId = asString(row[3]);
    const scopeId = asString(row[4]);
    const scopeName = asString(row[5]) || scopeId;
    const scopeKind = asString(row[6]) || "unknown";
    const memberCount = asNumber(row[7]) ?? 0;

    out.push({
      key: `${processId}:${streamId}:${scopeId}`,
      processId,
      processName,
      pid,
      streamId,
      scopeId,
      scopeName,
      scopeKind,
      memberCount,
    });
  }
  return out;
}

function compareRows(a: ScopeRow, b: ScopeRow, key: SortKey): number {
  if (key === "members") return a.memberCount - b.memberCount;
  if (key === "process") {
    return (
      a.processName.localeCompare(b.processName) ||
      (a.pid ?? Number.MAX_SAFE_INTEGER) - (b.pid ?? Number.MAX_SAFE_INTEGER) ||
      a.processId.localeCompare(b.processId)
    );
  }
  if (key === "kind") {
    return a.scopeKind.localeCompare(b.scopeKind) || a.scopeName.localeCompare(b.scopeName);
  }
  return a.scopeName.localeCompare(b.scopeName) || a.scopeId.localeCompare(b.scopeId);
}

function sortRows(rows: ScopeRow[], key: SortKey, dir: SortDir): ScopeRow[] {
  const sorted = [...rows].sort((a, b) => compareRows(a, b, key));
  return dir === "asc" ? sorted : sorted.reverse();
}

const ALL_KINDS_VALUE = "__all_scope_kinds__";

export function ScopeTablePanel({
  selectedKind,
  onSelectKind,
}: {
  selectedKind: string | null;
  onSelectKind: (kind: string | null) => void;
}) {
  const [rows, setRows] = useState<ScopeRow[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [sortKey, setSortKey] = useState<SortKey>("process");
  const [sortDir, setSortDir] = useState<SortDir>("asc");
  const [search, setSearch] = useState("");

  const refresh = React.useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const response = await apiClient.fetchSql(SCOPE_ROWS_SQL);
      setRows(parseScopeRows(response.rows));
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
    const id = window.setInterval(() => {
      void refresh();
    }, 2000);
    return () => window.clearInterval(id);
  }, [refresh]);

  const kindOptions = useMemo(() => {
    const kinds = Array.from(new Set(rows.map((row) => row.scopeKind))).sort((a, b) => a.localeCompare(b));
    return [
      { value: ALL_KINDS_VALUE, label: "All kinds" },
      ...kinds.map((kind) => ({ value: kind, label: kind })),
    ];
  }, [rows]);

  const filteredRows = useMemo(() => {
    const query = search.trim().toLowerCase();
    return rows.filter((row) => {
      if (selectedKind && row.scopeKind !== selectedKind) return false;
      if (!query) return true;
      const haystack = [
        row.processName,
        row.processId,
        row.scopeName,
        row.scopeId,
        row.scopeKind,
        row.streamId,
      ]
        .join(" ")
        .toLowerCase();
      return haystack.includes(query);
    });
  }, [rows, selectedKind, search]);

  const sortedRows = useMemo(() => sortRows(filteredRows, sortKey, sortDir), [filteredRows, sortKey, sortDir]);

  const columns = useMemo<readonly Column<ScopeRow>[]>(
    () => [
      {
        key: "process",
        label: "Process",
        sortable: true,
        width: "1.2fr",
        render: (row) => (
          <span className="scope-table-process" title={`${row.processName} (${row.processId})`}>
            {row.processName}
            {row.pid != null ? ` (${row.pid})` : ""}
          </span>
        ),
      },
      {
        key: "kind",
        label: "Kind",
        sortable: true,
        width: "0.8fr",
        render: (row) => <span className="scope-table-mono">{row.scopeKind}</span>,
      },
      {
        key: "name",
        label: "Scope",
        sortable: true,
        width: "1.7fr",
        render: (row) => (
          <div className="scope-table-scope-cell" title={`${row.scopeName} (${row.scopeId})`}>
            <span>{row.scopeName}</span>
            <span className="scope-table-subtle">{row.scopeId}</span>
          </div>
        ),
      },
      {
        key: "members",
        label: "Members",
        sortable: true,
        width: "0.7fr",
        render: (row) => <span className="scope-table-mono">{row.memberCount}</span>,
      },
    ],
    [],
  );

  return (
    <div className="scope-table-panel">
      <div className="scope-table-toolbar">
        <span className="scope-table-stats">{filteredRows.length} / {rows.length} scopes</span>
        <div className="scope-table-toolbar-controls">
          <TextInput
            value={search}
            onChange={setSearch}
            placeholder="Search scopes…"
            aria-label="Search scopes"
            className="scope-table-search"
          />
          <Select
            value={selectedKind ?? ALL_KINDS_VALUE}
            onChange={(value) => onSelectKind(value === ALL_KINDS_VALUE ? null : value)}
            options={kindOptions}
            aria-label="Filter by scope kind"
          />
          <ActionButton size="sm" onPress={refresh} isDisabled={loading}>
            {loading ? <CircleNotch size={14} weight="bold" className="spinning" /> : <ArrowsClockwise size={14} weight="bold" />}
            Refresh
          </ActionButton>
        </div>
      </div>

      {error && <div className="scope-table-error">{error}</div>}

      <div className="scope-table-body">
        <Table
          aria-label="Scopes"
          columns={columns}
          rows={sortedRows}
          rowKey={(row) => row.key}
          sortKey={sortKey}
          sortDir={sortDir}
          onSort={(key) => {
            const nextKey = key as SortKey;
            if (nextKey === sortKey) {
              setSortDir((prev) => (prev === "asc" ? "desc" : "asc"));
              return;
            }
            setSortKey(nextKey);
            setSortDir("asc");
          }}
        />
      </div>
    </div>
  );
}
