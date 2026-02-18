import { describe, expect, it } from "vitest";
import type { EdgeDef } from "./snapshot";
import { collapseEdgesThroughHiddenNodes } from "./snapshot";

function edge(id: string, source: string, target: string): EdgeDef {
  return {
    id,
    source,
    target,
    kind: "needs",
    meta: {},
  };
}

describe("collapseEdgesThroughHiddenNodes", () => {
  it("keeps visible direct edges", () => {
    const edges = [edge("ab", "a", "b"), edge("bc", "b", "c")];
    const visible = new Set(["a", "b"]);

    const collapsed = collapseEdgesThroughHiddenNodes(edges, visible);

    expect(collapsed.map((e) => e.id)).toEqual(["ab"]);
  });

  it("synthesizes across hidden nodes in directed paths", () => {
    const edges = [edge("ah", "a", "h"), edge("hb", "h", "b")];
    const visible = new Set(["a", "b"]);

    const collapsed = collapseEdgesThroughHiddenNodes(edges, visible);

    expect(collapsed).toContainEqual(
      expect.objectContaining({
        id: "collapsed-a-b",
        source: "a",
        target: "b",
        kind: "touches",
      }),
    );
  });

  it("synthesizes when the hidden intermediary only has incoming edges from visible nodes", () => {
    const edges = [edge("ha", "h", "a"), edge("hb", "h", "b")];
    const visible = new Set(["a", "b"]);

    const collapsed = collapseEdgesThroughHiddenNodes(edges, visible);

    expect(collapsed).toContainEqual(
      expect.objectContaining({
        id: "collapsed-a-b",
        source: "a",
        target: "b",
        kind: "touches",
      }),
    );
  });
});
