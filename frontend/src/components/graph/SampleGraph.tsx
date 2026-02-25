import React, { useEffect, useMemo, useState } from "react";
import type { EntityDef, EdgeDef } from "../../snapshot";
import { layoutGraph } from "../../graph/elkAdapter";
import { measureGraphLayout } from "../../graph/render/NodeLayer";
import type { GraphGeometry, GeometryNode } from "../../graph/geometry";
import type { ScopeColorPair } from "./scopeColors";
import { assignScopeColorRgbByKey } from "./scopeColors";
import { GraphViewport } from "./GraphViewport";
import type { GraphSelection } from "./GraphPanel";

export type { GraphSelection };

export function SampleGraph({
  entityDefs,
  edgeDefs,
  scopeColorMode = "none",
  subgraphScopeMode = "none",
}: {
  entityDefs: EntityDef[];
  edgeDefs: EdgeDef[];
  scopeColorMode?: "none" | "process" | "crate" | "task";
  subgraphScopeMode?: "none" | "process" | "crate" | "task";
}) {
  const [layout, setLayout] = useState<GraphGeometry | null>(null);
  const [selection, setSelection] = useState<GraphSelection>(null);

  useEffect(() => {
    if (entityDefs.length === 0) return;
    measureGraphLayout(entityDefs, subgraphScopeMode)
      .then((measurements) =>
        layoutGraph(entityDefs, edgeDefs, measurements.nodeSizes, subgraphScopeMode, {
          subgraphHeaderHeight: measurements.subgraphHeaderHeight,
        }),
      )
      .then(setLayout)
      .catch(console.error);
  }, [entityDefs, edgeDefs, subgraphScopeMode]);

  const entityById = useMemo(() => new Map(entityDefs.map((e) => [e.id, e])), [entityDefs]);

  const scopeColorByKey = useMemo<Map<string, ScopeColorPair>>(() => {
    if (scopeColorMode === "none") return new Map();
    const keys = entityDefs.map((e) => {
      if (scopeColorMode === "process") return e.processId;
      if (scopeColorMode === "task") return e.taskScopeKey ?? `${e.processId}:~no-task`;
      return e.krate ?? "~no-crate";
    });
    return assignScopeColorRgbByKey(keys);
  }, [entityDefs, scopeColorMode]);

  const nodes = useMemo<GeometryNode[]>(() => {
    if (!layout) return [];
    return layout.nodes.map((n) => {
      if (scopeColorMode === "none") return n;
      const entity = entityById.get(n.id);
      const scopeKey =
        scopeColorMode === "process"
          ? entity?.processId
          : scopeColorMode === "task"
            ? (entity?.taskScopeKey ?? (entity ? `${entity.processId}:~no-task` : undefined))
            : (entity?.krate ?? "~no-crate");
      const rgb = scopeKey ? scopeColorByKey.get(scopeKey) : undefined;
      return {
        ...n,
        data: { ...n.data, scopeRgbLight: rgb?.light, scopeRgbDark: rgb?.dark },
      };
    });
  }, [layout, entityById, scopeColorByKey, scopeColorMode]);

  const groups = useMemo(() => {
    if (!layout) return [];
    return layout.groups.map((g) => {
      const scopeKey = g.data?.scopeKey as string | undefined;
      const rgb = scopeKey ? scopeColorByKey.get(scopeKey) : undefined;
      return {
        ...g,
        data: { ...g.data, scopeRgbLight: rgb?.light, scopeRgbDark: rgb?.dark },
      };
    });
  }, [layout, scopeColorByKey]);

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      <GraphViewport
        entityDefs={entityDefs}
        snapPhase="ready"
        waitingForProcesses={false}
        geometry={layout}
        groups={groups}
        nodes={nodes}
        selection={selection}
        onSelect={setSelection}
        unionModeSuppressAutoFit={false}
        entityById={entityById}
        onHideNodeFilter={() => {}}
        onHideLocationFilter={() => {}}
        onFocusConnected={() => {}}
        onAppendFilterToken={() => {}}
      />
    </div>
  );
}
