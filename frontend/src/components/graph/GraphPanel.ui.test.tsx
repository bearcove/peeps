// @vitest-environment jsdom
import React, { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
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
      onFocusConnected={() => {}}
      onAppendFilterToken={() => {}}
    />
  );
}

describe("GraphPanel filter input interactions", () => {
  it("focus starts a new fragment instead of editing last token", async () => {
    const user = userEvent.setup();
    render(<Harness initialFilter="colorBy:crate groupBy:process loners:off" />);

    const input = screen.getByLabelText("Graph filter query") as HTMLInputElement;
    expect(input.placeholder).toBe("to add filter");
    expect(input.value).toBe("");
    await user.click(input);
    expect(input.value).toBe("");
    expect(screen.getByRole("button", { name: /Include only filter/i })).toBeTruthy();
    expect(screen.getByRole("button", { name: /Exclude everything matching this filter/i })).toBeTruthy();

    await user.type(input, "-n");
    expect(input.value).toBe("-n");
  });

  it("focuses the filter with Cmd+K", async () => {
    const user = userEvent.setup();
    render(<Harness initialFilter="colorBy:crate groupBy:process loners:off" />);

    const input = screen.getByLabelText("Graph filter query") as HTMLInputElement;
    expect(document.activeElement).not.toBe(input);
    await user.keyboard("{Meta>}{k}{/Meta}");
    expect(document.activeElement).toBe(input);
  });

  it("supports signed include/exclude autocomplete", async () => {
    const user = userEvent.setup();
    render(<Harness initialFilter="colorBy:crate groupBy:process loners:off" />);

    const input = screen.getByLabelText("Graph filter query") as HTMLInputElement;
    await user.click(input);
    await user.type(input, "-k");

    await user.click(screen.getByText("-kind:<kind>"));
    expect(input.value).toBe("-kind:");
    expect(screen.queryByRole("button", { name: /-kind:<kind>/i })).toBeNull();
    expect(screen.getByText("-kind:request")).toBeTruthy();

    await user.click(screen.getByText("-kind:request"));
    expect(input.value).toBe("");
    expect(screen.getByRole("button", { name: /-kind:request/i })).toBeTruthy();
  });

  it("backspace at end removes last chip but keeps previous chip committed", async () => {
    const user = userEvent.setup();
    render(<Harness initialFilter="colorBy:crate groupBy:process" />);

    const input = screen.getByLabelText("Graph filter query") as HTMLInputElement;
    await user.click(input);
    expect(input.value).toBe("");

    await user.keyboard("{Backspace}");

    expect(screen.queryByRole("button", { name: /groupBy:process/i })).toBeNull();
    expect(screen.getByRole("button", { name: /colorBy:crate/i })).toBeTruthy();
    expect(input.value).toBe("");
  });

  it("captures Tab and applies current autocomplete choice", async () => {
    const user = userEvent.setup();
    render(<Harness initialFilter="colorBy:crate groupBy:process loners:off" />);

    const input = screen.getByLabelText("Graph filter query") as HTMLInputElement;
    await user.click(input);
    await user.type(input, "-k");
    await user.tab();

    expect(input.value).toBe("-kind:");
    expect(screen.queryByRole("button", { name: /-kind:<kind>/i })).toBeNull();
  });

  it("captures Shift+Tab and cycles suggestions backwards", async () => {
    const user = userEvent.setup();
    render(<Harness initialFilter="colorBy:crate groupBy:process loners:off" />);

    const input = screen.getByLabelText("Graph filter query") as HTMLInputElement;
    await user.click(input);
    await user.type(input, "-");
    await user.keyboard("{Shift>}{Tab}{/Shift}");
    await user.keyboard("{Enter}");

    expect(input.value).toBe("-kind:");
    expect(screen.queryByRole("button", { name: /-kind:<kind>/i })).toBeNull();
    expect(screen.getByText("-kind:request")).toBeTruthy();
  });

  it("clicking outside unfocuses filter and closes suggestions", async () => {
    const user = userEvent.setup();
    render(<Harness initialFilter="colorBy:crate groupBy:process loners:off" />);

    const input = screen.getByLabelText("Graph filter query") as HTMLInputElement;
    await user.click(input);
    await user.type(input, "-n");
    expect(document.activeElement).toBe(input);
    expect(screen.getByText("-node:<id>")).toBeTruthy();

    await user.click(screen.getByText("No entities in snapshot"));

    await waitFor(() => {
      expect(document.activeElement).not.toBe(input);
      expect(screen.queryByText("-node:<id>")).toBeNull();
    });
  });

  it("uses the same font size for chips and add-filter input", async () => {
    const user = userEvent.setup();
    render(<Harness initialFilter="colorBy:crate groupBy:process loners:off" />);

    const input = screen.getByLabelText("Graph filter query") as HTMLInputElement;
    await user.click(input);
    await user.type(input, "-k");
    await user.click(screen.getByText("-kind:<kind>"));
    await user.click(screen.getByText("-kind:request"));

    const chip = screen.getByRole("button", { name: /-kind:request/i });
    expect(getComputedStyle(chip).fontSize).toBe(getComputedStyle(input).fontSize);
  });

  it("reopens suggestions after applying with Tab", async () => {
    const user = userEvent.setup();
    render(<Harness initialFilter="colorBy:crate groupBy:process loners:off" />);

    const input = screen.getByLabelText("Graph filter query") as HTMLInputElement;
    await user.click(input);
    await user.type(input, "-k");
    await user.tab(); // -> -kind:
    await user.tab(); // -> apply first concrete kind

    expect(screen.getByRole("button", { name: /-kind:request/i })).toBeTruthy();
    expect(input.value).toBe("");
    expect(screen.getByRole("button", { name: /Include only filter/i })).toBeTruthy();
    expect(screen.getByRole("button", { name: /Exclude everything matching this filter/i })).toBeTruthy();
  });

  it("reopens suggestions after applying with Enter", async () => {
    const user = userEvent.setup();
    render(<Harness initialFilter="colorBy:crate groupBy:process loners:off" />);

    const input = screen.getByLabelText("Graph filter query") as HTMLInputElement;
    await user.click(input);
    await user.type(input, "-k");
    await user.keyboard("{Enter}"); // -> -kind:
    await user.keyboard("{Enter}"); // -> apply first concrete kind

    expect(screen.getByRole("button", { name: /-kind:request/i })).toBeTruthy();
    expect(input.value).toBe("");
    expect(screen.getByRole("button", { name: /Include only filter/i })).toBeTruthy();
    expect(screen.getByRole("button", { name: /Exclude everything matching this filter/i })).toBeTruthy();
  });

  it("does not suggest already-applied kind include values", async () => {
    const user = userEvent.setup();
    render(<Harness initialFilter="+kind:request" />);

    const input = screen.getByLabelText("Graph filter query") as HTMLInputElement;
    await user.click(input);
    await user.type(input, "+k");
    await user.keyboard("{Enter}"); // +kind:

    const suggestionPanel = document.querySelector(".graph-filter-suggestions");
    expect(suggestionPanel).toBeTruthy();
    const panel = within(suggestionPanel as HTMLElement);
    expect(panel.queryByText("+kind:request")).toBeNull();
    expect(panel.getByText("+kind:response")).toBeTruthy();
  });

  it("clears all filters with Cmd+Backspace when input is focused", async () => {
    const user = userEvent.setup();
    render(<Harness initialFilter="+kind:request -kind:response colorBy:crate" />);

    const input = screen.getByLabelText("Graph filter query") as HTMLInputElement;
    await user.click(input);
    await user.keyboard("{Meta>}{Backspace}{/Meta}");

    expect(screen.queryByRole("button", { name: /\+kind:request/i })).toBeNull();
    expect(screen.queryByRole("button", { name: /-kind:response/i })).toBeNull();
    expect(screen.queryByRole("button", { name: /colorBy:crate/i })).toBeNull();
    expect(input.value).toBe("");
    expect(screen.getByRole("button", { name: /Include only filter/i })).toBeTruthy();
  });

  it("renders process chips with process label instead of raw process id", async () => {
    render(<Harness initialFilter="+process:1" />);
    expect(screen.getByRole("button", { name: /\+process:web\(1234\)/i })).toBeTruthy();
    expect(screen.queryByRole("button", { name: /\+process:1/i })).toBeNull();
  });
});
