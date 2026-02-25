import { describe, expect, it } from "vitest";
import {
  graphFilterEditorReducer,
  graphFilterEditorStateFromText,
  graphFilterSuggestions,
  parseGraphFilterQuery,
  serializeGraphFilterEditorState,
  type GraphFilterEditorAction,
  type GraphFilterEditorState,
} from "./graphFilter";

const baseInput = {
  nodeIds: ["1/alpha", "1/beta", "2/worker-loop"],
  entities: [
    {
      id: "1/alpha",
      label: "sleepy forever (web:1234)",
      searchText: "sleepy forever future web 1234",
    },
    {
      id: "1/beta",
      label: "semaphore gate (web:1234)",
      searchText: "semaphore gate lock web 1234",
    },
    {
      id: "2/worker-loop",
      label: "worker loop (worker:5678)",
      searchText: "worker loop process worker 5678",
    },
  ],
  locations: ["src/main.rs:12", "crates/moire/src/enabled.rs:505"],
  crates: [
    { id: "moire-core", label: "moire-core" },
    { id: "moire-web", label: "moire-web" },
  ],
  processes: [
    { id: "1", label: "web(1234)" },
    { id: "2", label: "worker(5678)" },
  ],
  kinds: [
    { id: "request", label: "Request" },
    { id: "response", label: "Response" },
  ],
  modules: [
    { id: "moire_core::server", label: "moire_core::server" },
    { id: "moire_web::handler", label: "moire_web::handler" },
  ],
};

function reduce(
  state: GraphFilterEditorState,
  ...actions: GraphFilterEditorAction[]
): GraphFilterEditorState {
  let next = state;
  for (const action of actions) {
    next = graphFilterEditorReducer(next, action);
  }
  return next;
}

describe("graphFilterSuggestions", () => {
  it("shows root plus/minus suggestions first", () => {
    const out = graphFilterSuggestions({ ...baseInput, fragment: "" });
    expect(out.map((s) => s.token).slice(0, 2)).toEqual(["+", "-"]);
    expect(out[0]?.description).toBe("Include only filter");
    expect(out[1]?.description).toBe("Exclude everything matching this filter");
  });

  it("suggests unsiged settings filters at root stage", () => {
    const out = graphFilterSuggestions({ ...baseInput, fragment: "co" });
    expect(out.map((s) => s.token)).toContain("colorBy:process");
    expect(out.map((s) => s.token)).toContain("colorBy:crate");
  });

  it("suggests groupBy filters at root stage", () => {
    const out = graphFilterSuggestions({ ...baseInput, fragment: "group" });
    expect(out.map((s) => s.token)).toContain("groupBy:process");
    expect(out.map((s) => s.token)).toContain("groupBy:crate");
    expect(out.map((s) => s.token)).toContain("groupBy:task");
    expect(out.map((s) => s.token)).not.toContain("groupBy:none");
  });

  it("suggests focus two-stage key", () => {
    const out = graphFilterSuggestions({ ...baseInput, fragment: "fo" });
    const focus = out.find((s) => s.token === "focus:<id>");
    expect(focus?.applyToken).toBe("focus:");
  });

  it("suggests focus concrete node values", () => {
    const out = graphFilterSuggestions({ ...baseInput, fragment: "focus:alp" });
    expect(out.map((s) => s.token)).toContain("focus:1/alpha");
    expect(out.map((s) => s.token)).not.toContain("focus:1/beta");
  });

  it("matches focus values by human label text", () => {
    const out = graphFilterSuggestions({ ...baseInput, fragment: "focus:sleepy" });
    expect(out.map((s) => s.token)).toContain("focus:1/alpha");
  });

  it("offers entity-first suggestions at root", () => {
    const out = graphFilterSuggestions({ ...baseInput, fragment: "sleepy" });
    expect(out.map((s) => s.token)).toContain("focus:1/alpha");
    expect(out.map((s) => s.token)).toContain("+node:1/alpha");
    expect(out.map((s) => s.token)).toContain("-node:1/alpha");
  });

  it("filters key suggestions when no colon is present", () => {
    const out = graphFilterSuggestions({ ...baseInput, fragment: "+" });
    expect(out.map((s) => s.token)).toContain("+node:<id>");
    expect(out.map((s) => s.token)).toContain("+kind:<kind>");
    expect(out.map((s) => s.token)).not.toContain("groupBy:process");
  });

  it("filters node suggestions by value after key", () => {
    const out = graphFilterSuggestions({ ...baseInput, fragment: "-node:alp" });
    expect(out[0]?.token).toBe("-node:1/alpha");
    expect(out.map((s) => s.token)).not.toContain("-node:1/beta");
  });

  it("supports fuzzy subsequence matching", () => {
    const out = graphFilterSuggestions({ ...baseInput, fragment: "-location:smr1" });
    expect(out.map((s) => s.token)).toContain("-location:src/main.rs:12");
  });

  it("matches process suggestions by label as well as id", () => {
    const out = graphFilterSuggestions({ ...baseInput, fragment: "-process:work" });
    expect(out[0]?.token).toBe("-process:2");
  });

  it.each([
    ["+n", "+node:<id>"],
    ["-loc", "-location:<src>"],
    ["+crate", "+crate:<name>"],
    ["-proc", "-process:<id>"],
    ["+kin", "+kind:<kind>"],
    ["+mod", "+module:<path>"],
    ["-module", "-module:<path>"],
  ])("suggests key family for fragment %s", (fragment, expectedToken) => {
    const out = graphFilterSuggestions({ ...baseInput, fragment });
    expect(out.map((s) => s.token)).toContain(expectedToken);
  });

  it("provides two-stage apply token for key placeholders", () => {
    const out = graphFilterSuggestions({ ...baseInput, fragment: "+kin" });
    const kindKey = out.find((s) => s.token === "+kind:<kind>");
    expect(kindKey?.applyToken).toBe("+kind:");
  });

  it("excludes suggestions for already applied concrete tokens", () => {
    const out = graphFilterSuggestions({
      ...baseInput,
      fragment: "+kind:",
      existingTokens: ["+kind:request"],
    });
    expect(out.map((s) => s.token)).not.toContain("+kind:request");
    expect(out.map((s) => s.token)).toContain("+kind:response");
  });

  it.each([
    ["+crate:web", "+crate:moire-web"],
    ["-crate:core", "-crate:moire-core"],
    ["+kind:req", "+kind:request"],
    ["-kind:res", "-kind:response"],
    ["+node:wrk", "+node:2/worker-loop"],
    ["-location:enabled", "-location:crates/moire/src/enabled.rs:505"],
    ["groupBy:pro", "groupBy:process"],
    ["groupBy:cr", "groupBy:crate"],
    ["groupBy:ta", "groupBy:task"],
    ["colorBy:pro", "colorBy:process"],
    ["colorBy:cr", "colorBy:crate"],
    ["colorBy:ta", "colorBy:task"],
    ["+module:server", "+module:moire_core::server"],
    ["-module:handler", "-module:moire_web::handler"],
  ])("suggests value for %s", (fragment, expectedToken) => {
    const out = graphFilterSuggestions({ ...baseInput, fragment });
    expect(out.map((s) => s.token)).toContain(expectedToken);
  });
});

describe("graphFilterEditorReducer", () => {
  const atoms = ["a", "bb", "ccc", "d4", "z"];

  function makeSequences(maxLen: number): string[][] {
    const out: string[][] = [[]];
    for (let len = 1; len <= maxLen; len++) {
      const next: string[][] = [];
      for (const prefix of out.filter((seq) => seq.length === len - 1)) {
        for (const atom of atoms) next.push([...prefix, atom]);
      }
      out.push(...next);
    }
    return out;
  }

  const sequences = makeSequences(3); // 156
  const cases: Array<{
    name: string;
    run: () => void;
  }> = [];

  for (const tokens of sequences) {
    const joined = tokens.join(" ");

    cases.push({
      name: `from-text-roundtrip:${joined || "<empty>"}`,
      run: () => {
        const state = graphFilterEditorStateFromText(joined);
        expect(state.ast).toEqual(tokens);
        expect(state.insertionPoint).toBe(tokens.length);
        expect(state.draft).toBe("");
        expect(serializeGraphFilterEditorState(state)).toBe(joined);
      },
    });

    cases.push({
      name: `focus-and-draft:${joined || "<empty>"}`,
      run: () => {
        const state = reduce(
          graphFilterEditorStateFromText(joined),
          { type: "focus_input" },
          { type: "set_draft", draft: "-node:alp" },
        );
        const expectedText = joined.length > 0 ? `${joined} -node:alp` : "-node:alp";
        expect(state.ast).toEqual(tokens);
        expect(state.draft).toBe("-node:alp");
        expect(serializeGraphFilterEditorState(state)).toBe(expectedText);
      },
    });

    cases.push({
      name: `backspace-removes-previous-chip:${joined || "<empty>"}`,
      run: () => {
        const state = reduce(
          graphFilterEditorStateFromText(joined),
          { type: "focus_input" },
          { type: "backspace_from_draft_start" },
        );
        const expectedTokens = tokens.slice(0, Math.max(0, tokens.length - 1));
        expect(state.ast).toEqual(expectedTokens);
        expect(state.draft).toBe("");
        expect(state.insertionPoint).toBe(expectedTokens.length);
        expect(serializeGraphFilterEditorState(state)).toBe(expectedTokens.join(" "));
      },
    });
  }

  // 156 * 3 = 468 matrix cases
  it.each(cases)("%s", ({ run }) => run());

  it("applies suggestion by inserting at current insertion point", () => {
    const state = reduce(
      graphFilterEditorStateFromText("colorBy:crate"),
      { type: "focus_input" },
      { type: "set_draft", draft: "-n" },
      { type: "apply_suggestion", token: "-node:<id>" },
    );
    expect(state.ast).toEqual(["colorBy:crate", "-node:<id>"]);
    expect(state.draft).toBe("");
    expect(state.suggestionsOpen).toBe(true);
    expect(serializeGraphFilterEditorState(state)).toBe("colorBy:crate -node:<id>");
  });

  it("move_suggestion cycles using total", () => {
    const state = reduce(
      graphFilterEditorStateFromText(""),
      { type: "move_suggestion", delta: 1, total: 5 },
      { type: "move_suggestion", delta: -1, total: 5 },
      { type: "move_suggestion", delta: -1, total: 5 },
    );
    expect(state.suggestionIndex).toBe(4);
  });
});

describe("parseGraphFilterQuery include/exclude syntax", () => {
  it("parses +node", () => {
    const out = parseGraphFilterQuery("+node:1/alpha");
    expect(out.includeNodeIds.has("1/alpha")).toBe(true);
  });

  it("parses -location", () => {
    const out = parseGraphFilterQuery('-location:"src/main.rs:12"');
    expect(out.excludeLocations.has("src/main.rs:12")).toBe(true);
  });

  it("treats placeholder value as invalid for signed filters", () => {
    const out = parseGraphFilterQuery("+kind:<kind>");
    expect(out.includeKinds.size).toBe(0);
    expect(out.tokens[0]?.valid).toBe(false);
  });

  it("parses focus token", () => {
    const out = parseGraphFilterQuery('focus:"1/alpha"');
    expect(out.focusedNodeId).toBe("1/alpha");
    expect(out.tokens[0]?.valid).toBe(true);
  });

  it("treats groupBy:none as valid no-op", () => {
    const out = parseGraphFilterQuery("groupBy:none");
    expect(out.groupBy).toBeUndefined();
    expect(out.tokens[0]?.valid).toBe(true);
  });

  it("parses groupBy:task", () => {
    const out = parseGraphFilterQuery("groupBy:task");
    expect(out.groupBy).toBe("task");
    expect(out.tokens[0]?.valid).toBe(true);
  });

  it("parses +module", () => {
    const out = parseGraphFilterQuery("+module:moire_core::server");
    expect(out.includeModules.has("moire_core::server")).toBe(true);
    expect(out.tokens[0]?.valid).toBe(true);
  });

  it("parses -module", () => {
    const out = parseGraphFilterQuery("-module:moire_web::handler");
    expect(out.excludeModules.has("moire_web::handler")).toBe(true);
    expect(out.tokens[0]?.valid).toBe(true);
  });

  it("treats placeholder module value as invalid", () => {
    const out = parseGraphFilterQuery("+module:<path>");
    expect(out.includeModules.size).toBe(0);
    expect(out.tokens[0]?.valid).toBe(false);
  });
});
