import { describe, expect, it } from "vitest";
import {
  ensureTrailingSpaceForNewFilter,
  graphFilterEditorParts,
  graphFilterSuggestions,
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

describe("graphFilterSuggestions", () => {
  it("filters key suggestions when no colon is present", () => {
    const out = graphFilterSuggestions({ ...baseInput, fragment: "col" });
    expect(out.map((s) => s.token)).toContain("colorBy:process");
    expect(out.map((s) => s.token)).toContain("colorBy:crate");
    expect(out.map((s) => s.token)).not.toContain("groupBy:process");
  });

  it("filters node suggestions by value after key", () => {
    const out = graphFilterSuggestions({ ...baseInput, fragment: "node:alp" });
    expect(out[0]?.token).toBe("node:1/alpha");
    expect(out.map((s) => s.token)).not.toContain("node:1/beta");
  });

  it("supports fuzzy subsequence matching", () => {
    const out = graphFilterSuggestions({ ...baseInput, fragment: "location:smr1" });
    expect(out.map((s) => s.token)).toContain("location:src/main.rs:12");
  });

  it("matches process suggestions by label as well as id", () => {
    const out = graphFilterSuggestions({ ...baseInput, fragment: "process:work" });
    expect(out[0]?.token).toBe("process:2");
  });

  it.each([
    ["n", "node:"],
    ["loc", "location:"],
    ["crate", "crate:"],
    ["proc", "process:"],
    ["kin", "kind:"],
    ["lon", "loners:on"],
    ["col", "colorBy:process"],
    ["group", "groupBy:process"],
  ])("suggests key family for fragment %s", (fragment, expectedToken) => {
    const out = graphFilterSuggestions({ ...baseInput, fragment });
    expect(out.map((s) => s.token)).toContain(expectedToken);
  });

  it.each([
    ["crate:web", "crate:peeps-web"],
    ["crate:core", "crate:peeps-core"],
    ["kind:req", "kind:request"],
    ["kind:res", "kind:response"],
    ["node:wrk", "node:2/worker-loop"],
    ["location:enabled", "location:crates/peeps/src/enabled.rs:505"],
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

describe("ensureTrailingSpaceForNewFilter", () => {
  it.each([
    ["", ""],
    ["node:1/a", "node:1/a "],
    ["node:1/a ", "node:1/a "],
    ["node:1/a  ", "node:1/a  "],
    ['location:"a b"', 'location:"a b" '],
    ['location:"a b" ', 'location:"a b" '],
  ])("normalizes %s", (input, expected) => {
    expect(ensureTrailingSpaceForNewFilter(input)).toBe(expected);
  });
});

describe("graphFilterEditorParts", () => {
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

  const sequences = makeSequences(3); // 1 + 5 + 25 + 125 = 156 sequences
  const cases: Array<{
    name: string;
    text: string;
    editing: boolean;
    committed: string[];
    fragment: string;
  }> = [];

  for (const tokens of sequences) {
    const joined = tokens.join(" ");
    cases.push({
      name: `view:${joined || "<empty>"}`,
      text: joined,
      editing: false,
      committed: tokens,
      fragment: "",
    });
    cases.push({
      name: `edit-trailing:${joined || "<empty>"}`,
      text: joined.length === 0 ? "" : `${joined} `,
      editing: true,
      committed: tokens,
      fragment: "",
    });
    cases.push({
      name: `edit-fragment:${joined || "<empty>"}`,
      text: joined,
      editing: true,
      committed: tokens.length > 0 ? tokens.slice(0, -1) : [],
      fragment: tokens.length > 0 ? tokens[tokens.length - 1] : "",
    });
  }

  // 156 * 3 = 468 cases
  it.each(cases)("%s", ({ text, editing, committed, fragment }) => {
    const out = graphFilterEditorParts(text, editing);
    expect(out.committed).toEqual(committed);
    expect(out.fragment).toBe(fragment);
  });

  it("keeps quoted token intact when editing fragment", () => {
    const out = graphFilterEditorParts('node:1/a location:"src/main.rs:42"', true);
    expect(out.committed).toEqual(["node:1/a"]);
    expect(out.fragment).toBe('location:"src/main.rs:42"');
  });

  it("treats quoted token as committed when not editing", () => {
    const out = graphFilterEditorParts('node:1/a location:"src/main.rs:42"', false);
    expect(out.committed).toEqual(["node:1/a", 'location:"src/main.rs:42"']);
    expect(out.fragment).toBe("");
  });

  it("creates empty fragment when text ends with space", () => {
    const out = graphFilterEditorParts("node:1/a location:src/main.rs:42 ", true);
    expect(out.committed).toEqual(["node:1/a", "location:src/main.rs:42"]);
    expect(out.fragment).toBe("");
  });
});
