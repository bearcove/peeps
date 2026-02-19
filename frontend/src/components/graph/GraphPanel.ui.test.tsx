// @vitest-environment jsdom
import React, { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { GraphPanel } from "./GraphPanel";

vi.mock("../../graph/elkAdapter", () => ({
  layoutGraph: vi.fn(async () => ({
    nodes: [],
    groups: [],
    edges: [],
    bounds: { x: 0, y: 0, width: 0, height: 0 },
  })),
}));

afterEach(() => cleanup());

function Harness({ initialFilter }: { initialFilter: string }) {
  const [graphFilterText, setGraphFilterText] = useState(initialFilter);
  const [subgraphScopeMode] = useState<"none" | "process" | "crate">("none");

  return (
    <GraphPanel
      entityDefs={[]}
      edgeDefs={[]}
      snapPhase="ready"
      selection={null}
      onSelect={() => {}}
      focusedEntityId={null}
      onExitFocus={() => {}}
      waitingForProcesses={false}
      crateItems={[
        { id: "crate-a", label: "crate-a", meta: 1 },
        { id: "crate-b", label: "crate-b", meta: 1 },
      ]}
      processItems={[
        { id: "1", label: "web(1234)", meta: 1 },
      ]}
      kindItems={[
        { id: "request", label: "request", meta: 1 },
        { id: "response", label: "response", meta: 1 },
      ]}
      scopeColorMode={"none"}
      subgraphScopeMode={subgraphScopeMode}
      scopeFilterLabel={null}
      onClearScopeFilter={() => {}}
      unionFrameLayout={undefined}
      graphFilterText={graphFilterText}
      onGraphFilterTextChange={setGraphFilterText}
      onHideNodeFilter={() => {}}
      onHideLocationFilter={() => {}}
    />
  );
}

describe("GraphPanel filter input interactions", () => {
  it("focus starts a new fragment instead of editing last token", async () => {
    const user = userEvent.setup();
    render(<Harness initialFilter="colorBy:crate groupBy:process loners:off" />);

    const input = screen.getByLabelText("Graph filter query") as HTMLInputElement;
    expect(input.value).toBe("");
    await user.click(input);
    expect(input.value).toBe("");

    await user.type(input, "-n");
    expect(input.value).toBe("-n");
  });

  it("supports signed include/exclude autocomplete", async () => {
    const user = userEvent.setup();
    render(<Harness initialFilter="colorBy:crate groupBy:process loners:off" />);

    const input = screen.getByLabelText("Graph filter query") as HTMLInputElement;
    await user.click(input);
    await user.type(input, "-n");

    await user.click(screen.getByText("-node:<id>"));
    expect(input.value).toBe("");
    expect(screen.getByRole("button", { name: /-node:<id>/i })).toBeTruthy();
  });
});
