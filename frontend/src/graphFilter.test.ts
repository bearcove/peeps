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
  locations: ["src/main.rs:12", "crates/peeps/src/enabled.rs:505"],
  crates: [
    { id: "peeps-core", label: "peeps-core" },
    { id: "peeps-web", label: "peeps-web" },
  ],
  processes: [
    { id: "1", label: "web(1234)" },
    { id: "2", label: "worker(5678)" },
  ],
  kinds: [
    { id: "request", label: "Request" },
    { id: "response", label: "Response" },
  ],
};

function reduce(state: GraphFilterEditorState, ...actions: GraphFilterEditorAction[]): GraphFilterEditorState {
  let next = state;
  for (const action of actions) {
    next = graphFilterEditorReducer(next, action);
  }
  return next;
}

describe("graphFilterSuggestions", () => {
  it("shows root plus/minus suggestions first", () => {
    const out = graphFilterSuggestions({ ...baseInput, fragment: "" });
    expect(out.map((s) => s.token)).toEqual(["+", "-"]);
    expect(out[0]?.description).toBe("Include only filter");
    expect(out[1]?.description).toBe("Exclude everything matching this filter");
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
  ])("suggests key family for fragment %s", (fragment, expectedToken) => {
    const out = graphFilterSuggestions({ ...baseInput, fragment });
    expect(out.map((s) => s.token)).toContain(expectedToken);
  });

  it("provides two-stage apply token for key placeholders", () => {
    const out = graphFilterSuggestions({ ...baseInput, fragment: "+kin" });
    const kindKey = out.find((s) => s.token === "+kind:<kind>");
    expect(kindKey?.applyToken).toBe("+kind:");
  });

  it.each([
    ["+crate:web", "+crate:peeps-web"],
    ["-crate:core", "-crate:peeps-core"],
    ["+kind:req", "+kind:request"],
    ["-kind:res", "-kind:response"],
    ["+node:wrk", "+node:2/worker-loop"],
    ["-location:enabled", "-location:crates/peeps/src/enabled.rs:505"],
    ["groupBy:pro", "groupBy:process"],
    ["groupBy:cr", "groupBy:crate"],
    ["groupBy:n", "groupBy:none"],
    ["colorBy:pro", "colorBy:process"],
    ["colorBy:cr", "colorBy:crate"],
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
});
