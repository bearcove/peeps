import { useMemo, useState } from "react";
import { Table, type Column } from "../../ui/primitives/Table";
import { ActionButton } from "../../ui/primitives/ActionButton";
import { TextInput } from "../../ui/primitives/TextInput";
import { DurationDisplay } from "../../ui/primitives/DurationDisplay";
import { formatProcessLabel } from "../../processLabel";
import { scopeKindDisplayName, scopeKindIcon } from "../../scopeKindSpec";
import type { ScopeDef } from "../../snapshot";
import "./ScopeTablePanel.css";

// Sidebar kind ordering — known kinds first, then whatever else appears.
const KIND_ORDER = ["connection", "process", "task", "thread"];

type SortDir = "asc" | "desc";

export function ScopeTablePanel({
  scopes,
  selectedKind,
  selectedScopeKey,
  onSelectKind,
  onSelectScope,
  onShowGraphScope,
  onViewScopeEntities,
}: {
  scopes: ScopeDef[];
  selectedKind: string | null;
  selectedScopeKey: string | null;
  onSelectKind: (kind: string | null) => void;
  onSelectScope: (scope: ScopeDef | null) => void;
  onShowGraphScope: (scope: ScopeDef) => void;
  onViewScopeEntities: (scope: ScopeDef) => void;
}) {
  const [search, setSearch] = useState("");
  const [sortKey, setSortKey] = useState("process");
  const [sortDir, setSortDir] = useState<SortDir>("asc");

  // All kinds present, in preferred order.
  const allKinds = useMemo(() => {
    const present = Array.from(new Set(scopes.map((s) => s.scopeKind)));
    const known = KIND_ORDER.filter((k) => present.includes(k));
    const rest = present.filter((k) => !KIND_ORDER.includes(k)).sort();
    return [...known, ...rest];
  }, [scopes]);

  // Auto-pick first kind if none selected.
  const effectiveKind = selectedKind ?? allKinds[0] ?? null;

  const kindCounts = useMemo(() => {
    const counts = new Map<string, number>();
    for (const s of scopes) {
      counts.set(s.scopeKind, (counts.get(s.scopeKind) ?? 0) + 1);
    }
    return counts;
  }, [scopes]);

  const filteredScopes = useMemo(() => {
    const q = search.trim().toLowerCase();
    return scopes.filter((s) => {
      if (effectiveKind && s.scopeKind !== effectiveKind) return false;
      if (!q) return true;
      return (
        s.scopeName.toLowerCase().includes(q) ||
        s.processName.toLowerCase().includes(q) ||
        s.scopeId.toLowerCase().includes(q) ||
        (s.krate?.toLowerCase().includes(q) ?? false) ||
        s.source.toLowerCase().includes(q)
      );
    });
  }, [scopes, effectiveKind, search]);

  const sortedScopes = useMemo(() => {
    const dir = sortDir === "asc" ? 1 : -1;
    const key = sortKey;
    return [...filteredScopes].sort((a, b) => {
      if (key === "entities") return (a.memberEntityIds.length - b.memberEntityIds.length) * dir;
      if (key === "age") return (a.ageMs - b.ageMs) * dir;
      if (key === "name")
        return a.scopeName.localeCompare(b.scopeName) * dir;
      if (key === "source")
        return a.source.localeCompare(b.source) * dir;
      if (key === "crate")
        return (a.krate ?? "").localeCompare(b.krate ?? "") * dir;
      // default: process
      return (
        (a.processName.localeCompare(b.processName) ||
          a.processPid - b.processPid) * dir
      );
    });
  }, [filteredScopes, sortKey, sortDir]);

  function onSort(key: string) {
    if (key === sortKey) {
      setSortDir((d) => (d === "asc" ? "desc" : "asc"));
    } else {
      setSortKey(key);
      setSortDir("asc");
    }
  }

  const processCol: Column<ScopeDef> = {
    key: "process",
    label: "Process",
    sortable: true,
    width: "1.4fr",
    render: (s) => (
      <span className="scope-mono" title={formatProcessLabel(s.processName, s.processPid)}>
        {formatProcessLabel(s.processName, s.processPid)}
      </span>
    ),
  };

  const entitiesCol: Column<ScopeDef> = {
    key: "entities",
    label: "Entities",
    sortable: true,
    width: "0.5fr",
    render: (s) => <span className="scope-mono">{s.memberEntityIds.length}</span>,
  };

  const actionsCol: Column<ScopeDef> = {
    key: "actions",
    label: "Actions",
    width: "0.9fr",
    render: (s) => (
      <div className="scope-actions">
        <ActionButton
          size="sm"
          onClick={(e) => e.stopPropagation()}
          onPress={() => onShowGraphScope(s)}
        >
          Show Graph
        </ActionButton>
        <ActionButton
          size="sm"
          onClick={(e) => e.stopPropagation()}
          onPress={() => onViewScopeEntities(s)}
        >
          View Entities
        </ActionButton>
      </div>
    ),
  };

  const columnsByKind = useMemo<Record<string, readonly Column<ScopeDef>[]>>(
    () => ({
      connection: [
        processCol,
        {
          key: "name",
          label: "Connection",
          sortable: true,
          width: "2fr",
          render: (s) => (
            <span className="scope-name" title={s.scopeName}>
              {s.scopeName || s.scopeId}
            </span>
          ),
        },
        entitiesCol,
        actionsCol,
      ],
      process: [
        processCol,
        entitiesCol,
        actionsCol,
      ],
      task: [
        processCol,
        {
          key: "name",
          label: "Task",
          sortable: true,
          width: "1.8fr",
          render: (s) => (
            <span className="scope-name" title={s.scopeName}>
              {s.scopeName || s.scopeId}
            </span>
          ),
        },
        {
          key: "crate",
          label: "Crate",
          sortable: true,
          width: "0.8fr",
          render: (s) => (
            <span className="scope-mono scope-muted">{s.krate ?? "—"}</span>
          ),
        },
        {
          key: "source",
          label: "Source",
          sortable: true,
          width: "0.8fr",
          render: (s) => (
            <span className="scope-mono scope-muted" title={s.source}>
              {s.source.split("/").pop() ?? s.source}
            </span>
          ),
        },
        {
          key: "age",
          label: "Age",
          sortable: true,
          width: "0.6fr",
          render: (s) => <DurationDisplay ms={s.ageMs} />,
        },
        entitiesCol,
        actionsCol,
      ],
      thread: [
        processCol,
        {
          key: "name",
          label: "Thread",
          sortable: true,
          width: "1.8fr",
          render: (s) => (
            <span className="scope-name" title={s.scopeName}>
              {s.scopeName || s.scopeId}
            </span>
          ),
        },
        {
          key: "source",
          label: "Source",
          sortable: true,
          width: "0.8fr",
          render: (s) => (
            <span className="scope-mono scope-muted" title={s.source}>
              {s.source.split("/").pop() ?? s.source}
            </span>
          ),
        },
        entitiesCol,
        actionsCol,
      ],
    }),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [onShowGraphScope, onViewScopeEntities],
  );

  const fallbackColumns: readonly Column<ScopeDef>[] = useMemo(
    () => [
      processCol,
      {
        key: "name",
        label: "Scope",
        sortable: true,
        width: "2fr",
        render: (s) => (
          <span className="scope-name" title={s.scopeName}>
            {s.scopeName || s.scopeId}
          </span>
        ),
      },
      entitiesCol,
      actionsCol,
    ],
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [onShowGraphScope, onViewScopeEntities],
  );

  const columns = effectiveKind
    ? (columnsByKind[effectiveKind] ?? fallbackColumns)
    : fallbackColumns;

  return (
    <div className="scope-panel">
      <div className="scope-panel-toolbar">
        <span className="scope-panel-stats">
          {filteredScopes.length} / {scopes.length} scopes
        </span>
        <TextInput
          value={search}
          onChange={setSearch}
          placeholder="Search…"
          aria-label="Search scopes"
          className="scope-panel-search"
        />
      </div>

      <div className="scope-panel-body">
        <nav className="scope-panel-sidebar">
          {allKinds.length === 0 ? (
            <span className="scope-panel-empty-kinds">No scopes</span>
          ) : (
            allKinds.map((kind) => (
              <button
                key={kind}
                className={`scope-kind-btn${kind === effectiveKind ? " is-active" : ""}`}
                onClick={() => onSelectKind(kind)}
              >
                <span className="scope-kind-btn__icon">{scopeKindIcon(kind, 13)}</span>
                <span className="scope-kind-btn__label">{scopeKindDisplayName(kind)}</span>
                <span className="scope-kind-btn__count">{kindCounts.get(kind) ?? 0}</span>
              </button>
            ))
          )}
        </nav>

        <div className="scope-panel-table">
          {scopes.length === 0 ? (
            <div className="scope-panel-placeholder">
              Take a snapshot to see scopes.
            </div>
          ) : filteredScopes.length === 0 ? (
            <div className="scope-panel-placeholder">No matching scopes.</div>
          ) : (
            <Table
              aria-label="Scopes"
              columns={columns}
              rows={sortedScopes}
              rowKey={(s) => s.key}
              selectedRowKey={selectedScopeKey}
              sortKey={sortKey}
              sortDir={sortDir}
              onSort={onSort}
              onRowClick={(s) => {
                onSelectScope(s.key === selectedScopeKey ? null : s);
              }}
            />
          )}
        </div>
      </div>
    </div>
  );
}
