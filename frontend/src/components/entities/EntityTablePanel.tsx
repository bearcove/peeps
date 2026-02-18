import React, { useMemo, useState } from "react";
import { Table, type Column } from "../../ui/primitives/Table";
import { TextInput } from "../../ui/primitives/TextInput";
import { Select } from "../../ui/primitives/Select";
import { ActionButton } from "../../ui/primitives/ActionButton";
import { Badge } from "../../ui/primitives/Badge";
import { kindDisplayName } from "../../nodeKindSpec";
import { canonicalNodeKind, kindIcon } from "../../nodeKindSpec";
import { formatProcessLabel } from "../../processLabel";
import type { EntityDef } from "../../snapshot";
import "./EntityTablePanel.css";

type SortKey = "process" | "kind" | "name" | "status";
type SortDir = "asc" | "desc";

const ALL_PROCESSES = "__all_processes__";
const ALL_KINDS = "__all_entity_kinds__";

function compareEntities(a: EntityDef, b: EntityDef, key: SortKey): number {
  if (key === "process") {
    return (
      a.processName.localeCompare(b.processName) ||
      a.processPid - b.processPid ||
      a.name.localeCompare(b.name)
    );
  }
  if (key === "kind") {
    return (
      kindDisplayName(canonicalNodeKind(a.kind)).localeCompare(kindDisplayName(canonicalNodeKind(b.kind))) ||
      a.name.localeCompare(b.name)
    );
  }
  if (key === "status") {
    return a.status.label.localeCompare(b.status.label) || a.name.localeCompare(b.name);
  }
  return a.name.localeCompare(b.name);
}

export function EntityTablePanel({
  entityDefs,
  selectedEntityId,
  onSelectEntity,
  scopeFilterLabel,
  onClearScopeFilter,
}: {
  entityDefs: EntityDef[];
  selectedEntityId: string | null;
  onSelectEntity: (entityId: string) => void;
  scopeFilterLabel?: string | null;
  onClearScopeFilter?: () => void;
}) {
  const [search, setSearch] = useState("");
  const [selectedProcess, setSelectedProcess] = useState<string>(ALL_PROCESSES);
  const [selectedKind, setSelectedKind] = useState<string>(ALL_KINDS);
  const [sortKey, setSortKey] = useState<SortKey>("process");
  const [sortDir, setSortDir] = useState<SortDir>("asc");

  const processOptions = useMemo(() => {
    const seen = new Map<string, { name: string; pid: number }>();
    for (const entity of entityDefs) {
      if (!seen.has(entity.processId)) {
        seen.set(entity.processId, { name: entity.processName, pid: entity.processPid });
      }
    }
    return [
      { value: ALL_PROCESSES, label: "All processes" },
      ...Array.from(seen.entries())
        .sort((a, b) => a[1].name.localeCompare(b[1].name) || a[1].pid - b[1].pid)
        .map(([id, meta]) => ({ value: id, label: formatProcessLabel(meta.name, meta.pid) })),
    ];
  }, [entityDefs]);

  const kindOptions = useMemo(() => {
    const kinds = Array.from(new Set(entityDefs.map((entity) => canonicalNodeKind(entity.kind))))
      .sort((a, b) => kindDisplayName(a).localeCompare(kindDisplayName(b)));
    return [
      { value: ALL_KINDS, label: "All kinds" },
      ...kinds.map((kind) => ({ value: kind, label: kindDisplayName(kind) })),
    ];
  }, [entityDefs]);

  const filteredRows = useMemo(() => {
    const query = search.trim().toLowerCase();
    return entityDefs.filter((entity) => {
      if (selectedProcess !== ALL_PROCESSES && entity.processId !== selectedProcess) return false;
      const canonicalKind = canonicalNodeKind(entity.kind);
      if (selectedKind !== ALL_KINDS && canonicalKind !== selectedKind) return false;
      if (!query) return true;
      const haystack = [
        entity.name,
        entity.kind,
        canonicalKind,
        entity.processName,
        entity.processId,
        entity.source,
        entity.status.label,
      ].join(" ").toLowerCase();
      return haystack.includes(query);
    });
  }, [entityDefs, search, selectedProcess, selectedKind]);

  const sortedRows = useMemo(() => {
    const sorted = [...filteredRows].sort((a, b) => compareEntities(a, b, sortKey));
    return sortDir === "asc" ? sorted : sorted.reverse();
  }, [filteredRows, sortKey, sortDir]);

  const columns = useMemo<readonly Column<EntityDef>[]>(() => [
    {
      key: "process",
      label: "Process",
      sortable: true,
      width: "1.2fr",
      render: (row) => (
        <span className="entity-table-process">{formatProcessLabel(row.processName, row.processPid)}</span>
      ),
    },
    {
      key: "kind",
      label: "Kind",
      sortable: true,
      width: "1fr",
      render: (row) => {
        const canonicalKind = canonicalNodeKind(row.kind);
        return (
          <span className="entity-table-kind">
            {kindIcon(canonicalKind, 12)}
            <span className="entity-table-mono">{kindDisplayName(canonicalKind)}</span>
          </span>
        );
      },
    },
    {
      key: "name",
      label: "Entity",
      sortable: true,
      width: "1.6fr",
      render: (row) => (
        <div className="entity-table-entity-cell">
          <span className="entity-table-name">{row.name}</span>
          <span className="entity-table-subtle">{row.source}</span>
        </div>
      ),
    },
    {
      key: "status",
      label: "Status",
      sortable: true,
      width: "0.8fr",
      render: (row) => <Badge tone={row.status.tone}>{row.status.label}</Badge>,
    },
  ], []);

  return (
    <div className="entity-table-panel">
      <div className="entity-table-toolbar">
        <span className="entity-table-stats">{filteredRows.length} / {entityDefs.length} entities</span>
        <div className="entity-table-toolbar-controls">
          {scopeFilterLabel && (
            <>
              <span className="entity-table-scope-filter">Scope filter: {scopeFilterLabel}</span>
              <ActionButton size="sm" onPress={onClearScopeFilter}>Clear</ActionButton>
            </>
          )}
          <TextInput
            value={search}
            onChange={setSearch}
            placeholder="Search entities..."
            aria-label="Search entities"
            className="entity-table-search"
          />
          <Select value={selectedProcess} onChange={setSelectedProcess} options={processOptions} aria-label="Filter by process" />
          <Select value={selectedKind} onChange={setSelectedKind} options={kindOptions} aria-label="Filter by kind" />
        </div>
      </div>
      <div className="entity-table-body">
        <Table
          aria-label="Entities"
          columns={columns}
          rows={sortedRows}
          rowKey={(row) => row.id}
          selectedRowKey={selectedEntityId}
          onRowClick={(row) => onSelectEntity(row.id)}
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
