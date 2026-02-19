import React from "react";
import { apiClient } from "../../api";
import { formatProcessLabel } from "../../processLabel";
import type { EntityDef } from "../../snapshot";
import { scopeKindIcon, canonicalScopeKind } from "../../scopeKindSpec";
import { NodeChip } from "../../ui/primitives/NodeChip";
import "./InspectorPanel.css";

type EntityScopeLink = {
  processId: string;
  processName: string;
  pid: number | null;
  streamId: string;
  scopeId: string;
  scopeName: string;
  scopeKind: string;
};

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

function sqlEscape(value: string): string {
  return value.replace(/'/g, "''");
}

function parseScopeLinks(rows: unknown[]): EntityScopeLink[] {
  const out: EntityScopeLink[] = [];
  for (const raw of rows) {
    if (!Array.isArray(raw) || raw.length < 7) continue;
    out.push({
      processId: asString(raw[0]),
      processName: asString(raw[1]),
      pid: asNumber(raw[2]),
      streamId: asString(raw[3]),
      scopeId: asString(raw[4]),
      scopeName: asString(raw[5]),
      scopeKind: asString(raw[6]),
    });
  }
  return out;
}

export function EntityScopeLinksSection({
  entity,
  onOpenScopeKind,
}: {
  entity: EntityDef;
  onOpenScopeKind?: (kind: string) => void;
}) {
  const [loading, setLoading] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [links, setLinks] = React.useState<EntityScopeLink[]>([]);

  React.useEffect(() => {
    let cancelled = false;
    async function load() {
      setLoading(true);
      setError(null);
      try {
        const connId = Number(entity.processId);
        if (!Number.isFinite(connId)) {
          setLinks([]);
          return;
        }
        const entityId = sqlEscape(entity.rawEntityId);
        const sql = `
select
  l.conn_id as process_id,
  c.process_name,
  c.pid,
  l.stream_id,
  l.scope_id,
  coalesce(json_extract(s.scope_json, '$.name'), l.scope_id) as scope_name,
  coalesce(json_extract(s.scope_json, '$.body'), 'unknown') as scope_kind
from entity_scope_links l
left join scopes s
  on s.conn_id = l.conn_id
 and s.stream_id = l.stream_id
 and s.scope_id = l.scope_id
left join connections c on c.conn_id = l.conn_id
where l.conn_id = ${connId}
  and l.entity_id = '${entityId}'
order by l.stream_id asc, l.scope_id asc
`;
        const response = await apiClient.fetchSql(sql);
        if (cancelled) return;
        setLinks(parseScopeLinks(response.rows));
      } catch (err) {
        if (cancelled) return;
        setLinks([]);
        setError(err instanceof Error ? err.message : String(err));
      } finally {
        if (!cancelled) setLoading(false);
      }
    }
    void load();
    return () => {
      cancelled = true;
    };
  }, [entity.processId, entity.rawEntityId]);

  if (loading) {
    return (
      <div className="inspector-section">
        <div className="inspector-scope-links-head">Scopes</div>
        <div className="inspector-scope-link-meta">loading…</div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="inspector-section">
        <div className="inspector-scope-links-head">Scopes</div>
        <div className="inspector-crit">unavailable</div>
      </div>
    );
  }

  if (links.length === 0) {
    return null;
  }

  return (
    <div className="inspector-section">
      <div className="inspector-scope-links-head">Scopes</div>
      <div className="inspector-scope-links">
        {links.map((link) => {
          const kind = canonicalScopeKind(link.scopeKind);
          const displayName =
            kind === "process"
              ? formatProcessLabel(link.processName, link.pid)
              : (link.scopeName || link.scopeId);
          return (
            <NodeChip
              key={`${link.processId}:${link.streamId}:${link.scopeId}`}
              icon={scopeKindIcon(kind, 12)}
              label={displayName}
              onClick={() => onOpenScopeKind?.(kind)}
              title={`${formatProcessLabel(link.processName, link.pid)} · ${link.streamId}/${link.scopeId}`}
            />
          );
        })}
      </div>
    </div>
  );
}
