import { useCallback, useEffect, useMemo, useState } from "react";
import type { Edge, Node } from "@xyflow/react";
import { Background, Controls, MiniMap, ReactFlow } from "@xyflow/react";
import ELK from "elkjs/lib/elk.bundled.js";
import type {
  ConnectedProcessInfo,
  ConnectionsResponse,
  CutStatusResponse,
  SqlResponse,
} from "./api";
import { fetchConnections, fetchCutStatus, runSql, triggerCut } from "./api";

interface FlowNodeData {
  label: string;
  detail: string;
}

const elk = new ELK();

const FLOW_NODE_WIDTH = 210;
const FLOW_NODE_HEIGHT = 64;
const CONNECTION_POLL_MS = 1000;
const CUT_STATUS_POLL_MS = 600;

function buildFlowGraph(
  connections: ConnectedProcessInfo[],
  cutStatus: CutStatusResponse | null,
): { nodes: Node<FlowNodeData>[]; edges: Edge[] } {
  if (connections.length === 0) {
    return {
      nodes: [
        {
          id: "waiting",
          data: {
            label: "No live connections",
            detail: "Run an instrumented process with PEEPS_DASHBOARD=127.0.0.1:9119",
          },
          position: { x: 0, y: 0 },
          style: { width: FLOW_NODE_WIDTH },
        },
      ],
      edges: [],
    };
  }

  const nodes: Node<FlowNodeData>[] = connections.map((proc) => ({
    id: `conn:${proc.conn_id}`,
    data: {
      label: proc.process_name,
      detail: `conn ${proc.conn_id} | pid ${proc.pid}`,
    },
    position: { x: 0, y: 0 },
    style: { width: FLOW_NODE_WIDTH },
  }));
  const edges: Edge[] = [];

  if (cutStatus) {
    const cutNodeId = `cut:${cutStatus.cut_id}`;
    nodes.push({
      id: cutNodeId,
      data: {
        label: cutStatus.cut_id,
        detail: `${cutStatus.acked_connections} acked, ${cutStatus.pending_connections} pending`,
      },
      position: { x: 0, y: 0 },
      style: { width: FLOW_NODE_WIDTH, borderColor: "#5b21b6" },
    });

    const pending = new Set(cutStatus.pending_conn_ids);
    for (const proc of connections) {
      const pendingEdge = pending.has(proc.conn_id);
      edges.push({
        id: `${cutNodeId}->conn:${proc.conn_id}`,
        source: cutNodeId,
        target: `conn:${proc.conn_id}`,
        label: pendingEdge ? "pending" : "acked",
        type: "smoothstep",
        animated: pendingEdge,
        style: pendingEdge ? { stroke: "#f59e0b" } : { stroke: "#10b981" },
      });
    }
  }

  return { nodes, edges };
}

async function layoutGraph(
  nodes: Node<FlowNodeData>[],
  edges: Edge[],
): Promise<{ nodes: Node<FlowNodeData>[]; edges: Edge[] }> {
  const layout = await elk.layout({
    id: "root",
    layoutOptions: {
      "elk.algorithm": "layered",
      "elk.direction": "DOWN",
      "elk.layered.spacing.nodeNodeBetweenLayers": "84",
      "elk.spacing.nodeNode": "36",
    },
    children: nodes.map((node) => ({
      id: node.id,
      width: FLOW_NODE_WIDTH,
      height: FLOW_NODE_HEIGHT,
    })),
    edges: edges.map((edge) => ({
      id: edge.id,
      sources: [edge.source],
      targets: [edge.target],
    })),
  });

  const positionById = new Map((layout.children ?? []).map((child) => [child.id, child]));
  return {
    nodes: nodes.map((node) => {
      const pos = positionById.get(node.id);
      return {
        ...node,
        position: {
          x: pos?.x ?? 0,
          y: pos?.y ?? 0,
        },
      };
    }),
    edges,
  };
}

function toCellText(value: unknown): string {
  if (value === null) return "null";
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  return JSON.stringify(value);
}

function toRowCells(row: unknown): unknown[] {
  if (Array.isArray(row)) return row;
  return [row];
}

export function App() {
  const [connections, setConnections] = useState<ConnectionsResponse | null>(null);
  const [cutStatus, setCutStatus] = useState<CutStatusResponse | null>(null);
  const [sqlPreview, setSqlPreview] = useState<SqlResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busyCut, setBusyCut] = useState(false);
  const [busySql, setBusySql] = useState(false);
  const [flow, setFlow] = useState<{ nodes: Node<FlowNodeData>[]; edges: Edge[] }>({
    nodes: [],
    edges: [],
  });

  const refreshConnections = useCallback(async () => {
    const next = await fetchConnections();
    setConnections(next);
  }, []);

  const refreshSqlPreview = useCallback(async () => {
    setBusySql(true);
    setError(null);
    try {
      const response = await runSql(
        "SELECT conn_id, process_name, pid, connected_at_ns, disconnected_at_ns " +
          "FROM connections ORDER BY connected_at_ns DESC LIMIT 8",
      );
      setSqlPreview(response);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusySql(false);
    }
  }, []);

  const runCut = useCallback(async () => {
    setBusyCut(true);
    setError(null);
    try {
      const triggered = await triggerCut();
      const status = await fetchCutStatus(triggered.cut_id);
      setCutStatus(status);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusyCut(false);
    }
  }, []);

  useEffect(() => {
    let active = true;

    const poll = async () => {
      try {
        await refreshConnections();
      } catch (err) {
        if (!active) return;
        setError(err instanceof Error ? err.message : String(err));
      }
    };

    void poll();
    const timer = window.setInterval(() => {
      void poll();
    }, CONNECTION_POLL_MS);

    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, [refreshConnections]);

  useEffect(() => {
    if (!cutStatus || cutStatus.pending_connections === 0) {
      return;
    }

    let active = true;

    const poll = async () => {
      try {
        const next = await fetchCutStatus(cutStatus.cut_id);
        if (!active) return;
        setCutStatus(next);
      } catch (err) {
        if (!active) return;
        setError(err instanceof Error ? err.message : String(err));
      }
    };

    void poll();
    const timer = window.setInterval(() => {
      void poll();
    }, CUT_STATUS_POLL_MS);

    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, [cutStatus?.cut_id, cutStatus?.pending_connections]);

  useEffect(() => {
    if (connections === null) return;
    const seed = buildFlowGraph(connections.processes, cutStatus);
    let active = true;
    layoutGraph(seed.nodes, seed.edges)
      .then((next) => {
        if (!active) return;
        setFlow(next);
      })
      .catch((err: unknown) => {
        if (!active) return;
        setError(err instanceof Error ? err.message : String(err));
      });
    return () => {
      active = false;
    };
  }, [connections, cutStatus]);

  const connectionRows = useMemo(() => connections?.processes ?? [], [connections]);

  return (
    <div className="page">
      <header className="topbar">
        <div>
          <h1>Peeps Frontend Scaffold</h1>
          <p>
            HTTP is proxied by Vite (`/api` to `127.0.0.1:9130`). Ingest remains direct on
            `127.0.0.1:9119`.
          </p>
        </div>
        <div className="topbar-actions">
          <button type="button" onClick={runCut} disabled={busyCut}>
            {busyCut ? "Triggering cut..." : "Trigger cut"}
          </button>
          <button type="button" onClick={refreshSqlPreview} disabled={busySql}>
            {busySql ? "Refreshing SQL..." : "Refresh SQL preview"}
          </button>
        </div>
      </header>

      {error && <div className="error">{error}</div>}

      <section className="grid">
        <article className="card flow-card">
          <h2>Live Topology</h2>
          <div className="flow-wrap">
            <ReactFlow nodes={flow.nodes} edges={flow.edges} fitView>
              <Background />
              <Controls />
              <MiniMap />
            </ReactFlow>
          </div>
        </article>

        <article className="card">
          <h2>Connections ({connections?.connected_processes ?? 0})</h2>
          <table>
            <thead>
              <tr>
                <th>Conn</th>
                <th>Process</th>
                <th>PID</th>
              </tr>
            </thead>
            <tbody>
              {connectionRows.map((proc) => (
                <tr key={proc.conn_id}>
                  <td>{proc.conn_id}</td>
                  <td>{proc.process_name}</td>
                  <td>{proc.pid}</td>
                </tr>
              ))}
              {connectionRows.length === 0 && (
                <tr>
                  <td colSpan={3}>No active connections yet.</td>
                </tr>
              )}
            </tbody>
          </table>
        </article>

        <article className="card">
          <h2>Latest Cut</h2>
          {!cutStatus && <p>No cut has been requested yet.</p>}
          {cutStatus && (
            <dl className="kv">
              <div>
                <dt>ID</dt>
                <dd>{cutStatus.cut_id}</dd>
              </div>
              <div>
                <dt>Acked</dt>
                <dd>{cutStatus.acked_connections}</dd>
              </div>
              <div>
                <dt>Pending</dt>
                <dd>{cutStatus.pending_connections}</dd>
              </div>
            </dl>
          )}
        </article>

        <article className="card sql-card">
          <h2>SQL Preview</h2>
          {!sqlPreview && <p>Run a read-only SQL query preview against the peeps-web SQLite store.</p>}
          {sqlPreview && (
            <div className="sql-table-wrap">
              <table>
                <thead>
                  <tr>
                    {sqlPreview.columns.map((column) => (
                      <th key={column}>{column}</th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {sqlPreview.rows.map((row, index) => (
                    <tr key={`row-${index}`}>
                      {toRowCells(row).map((cell, cellIndex) => (
                        <td key={`cell-${index}-${cellIndex}`}>{toCellText(cell)}</td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </article>
      </section>
    </div>
  );
}
