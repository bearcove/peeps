export type GraphFilterMode = "process" | "crate";

export type ParsedGraphFilterToken = {
  raw: string;
  key: string | null;
  value: string | null;
  valid: boolean;
};

export type GraphFilterParseResult = {
  tokens: ParsedGraphFilterToken[];
  includeNodeIds: Set<string>;
  excludeNodeIds: Set<string>;
  includeLocations: Set<string>;
  excludeLocations: Set<string>;
  includeCrates: Set<string>;
  excludeCrates: Set<string>;
  includeProcesses: Set<string>;
  excludeProcesses: Set<string>;
  includeKinds: Set<string>;
  excludeKinds: Set<string>;
  showLoners?: boolean;
  colorBy?: GraphFilterMode;
  groupBy?: GraphFilterMode | "none";
};

export type GraphFilterSuggestion = {
  token: string;
  description: string;
};

export type GraphFilterSuggestionItem = {
  id: string;
  label: string;
};

export type GraphFilterSuggestionInput = {
  fragment: string;
  nodeIds: readonly string[];
  locations: readonly string[];
  crates: readonly GraphFilterSuggestionItem[];
  processes: readonly GraphFilterSuggestionItem[];
  kinds: readonly GraphFilterSuggestionItem[];
};

export type GraphFilterEditorParts = {
  committed: string[];
  fragment: string;
};

export function tokenizeFilterQuery(input: string): string[] {
  const tokens: string[] = [];
  let current = "";
  let inQuotes = false;
  let escaped = false;

  for (const ch of input) {
    if (escaped) {
      current += ch;
      escaped = false;
      continue;
    }
    if (ch === "\\") {
      current += ch;
      escaped = true;
      continue;
    }
    if (ch === "\"") {
      current += ch;
      inQuotes = !inQuotes;
      continue;
    }
    if (/\s/.test(ch) && !inQuotes) {
      if (current.trim().length > 0) tokens.push(current.trim());
      current = "";
      continue;
    }
    current += ch;
  }

  if (current.trim().length > 0) tokens.push(current.trim());
  return tokens;
}

export function stripFilterQuotes(value: string): string {
  const trimmed = value.trim();
  if (trimmed.length >= 2 && trimmed.startsWith("\"") && trimmed.endsWith("\"")) {
    return trimmed.slice(1, -1).replace(/\\"/g, "\"").replace(/\\\\/g, "\\");
  }
  return trimmed;
}

export function quoteFilterValue(value: string): string {
  if (/^[^\s"]+$/.test(value)) return value;
  return `"${value.replace(/\\/g, "\\\\").replace(/"/g, "\\\"")}"`;
}

export function parseGraphFilterQuery(filterText: string): GraphFilterParseResult {
  const includeNodeIds = new Set<string>();
  const excludeNodeIds = new Set<string>();
  const includeLocations = new Set<string>();
  const excludeLocations = new Set<string>();
  const includeCrates = new Set<string>();
  const excludeCrates = new Set<string>();
  const includeProcesses = new Set<string>();
  const excludeProcesses = new Set<string>();
  const includeKinds = new Set<string>();
  const excludeKinds = new Set<string>();
  const tokens = tokenizeFilterQuery(filterText);
  const parsed: ParsedGraphFilterToken[] = [];
  let colorBy: GraphFilterMode | undefined;
  let groupBy: GraphFilterMode | "none" | undefined;
  let showLoners: boolean | undefined;

  for (const raw of tokens) {
    const colon = raw.indexOf(":");
    if (colon < 1) {
      parsed.push({ raw, key: null, value: null, valid: false });
      continue;
    }

    let signed = 0;
    if (raw.startsWith("+")) signed = 1;
    if (raw.startsWith("-")) signed = -1;
    const signedRaw = signed === 0 ? raw : raw.slice(1);
    const signedColon = signedRaw.indexOf(":");
    const key = signedRaw.slice(0, signedColon);
    const keyLower = key.toLowerCase();
    const valueRaw = stripFilterQuotes(signedRaw.slice(signedColon + 1));
    const value = valueRaw.trim();
    if (!value) {
      parsed.push({ raw, key, value: valueRaw, valid: false });
      continue;
    }

    let valid = false;
    if (signed !== 0 && (keyLower === "node" || keyLower === "id")) {
      (signed > 0 ? includeNodeIds : excludeNodeIds).add(value);
      valid = true;
    } else if (signed !== 0 && (keyLower === "location" || keyLower === "source")) {
      (signed > 0 ? includeLocations : excludeLocations).add(value);
      valid = true;
    } else if (signed !== 0 && keyLower === "crate") {
      (signed > 0 ? includeCrates : excludeCrates).add(value);
      valid = true;
    } else if (signed !== 0 && keyLower === "process") {
      (signed > 0 ? includeProcesses : excludeProcesses).add(value);
      valid = true;
    } else if (signed !== 0 && keyLower === "kind") {
      (signed > 0 ? includeKinds : excludeKinds).add(value);
      valid = true;
    } else if (keyLower === "loners") {
      if (value === "on" || value === "true" || value === "yes") {
        showLoners = true;
        valid = true;
      } else if (value === "off" || value === "false" || value === "no") {
        showLoners = false;
        valid = true;
      }
    } else if (keyLower === "colorby") {
      if (value === "process" || value === "crate") {
        colorBy = value;
        valid = true;
      }
    } else if (keyLower === "groupby") {
      if (value === "process" || value === "crate" || value === "none") {
        groupBy = value;
        valid = true;
      }
    }

    parsed.push({ raw, key, value: valueRaw, valid });
  }

  return {
    tokens: parsed,
    includeNodeIds,
    excludeNodeIds,
    includeLocations,
    excludeLocations,
    includeCrates,
    excludeCrates,
    includeProcesses,
    excludeProcesses,
    includeKinds,
    excludeKinds,
    showLoners,
    colorBy,
    groupBy,
  };
}

export function appendFilterToken(filterText: string, token: string): string {
  const tokens = tokenizeFilterQuery(filterText);
  if (tokens.includes(token)) return filterText;
  return tokens.length === 0 ? token : `${tokens.join(" ")} ${token}`;
}

export function removeFilterTokenAtIndex(filterText: string, index: number): string {
  const tokens = tokenizeFilterQuery(filterText);
  if (index < 0 || index >= tokens.length) return filterText;
  tokens.splice(index, 1);
  return tokens.join(" ");
}

export function replaceTrailingFragment(filterText: string, replacement: string): string {
  const match = filterText.match(/^(.*?)(\S*)$/s);
  if (!match) return `${replacement} `;
  const prefix = match[1];
  return `${prefix}${replacement} `;
}

export function ensureTrailingSpaceForNewFilter(filterText: string): string {
  if (filterText.length === 0) return "";
  if (/\s$/.test(filterText)) return filterText;
  return `${filterText} `;
}

export function graphFilterEditorParts(filterText: string, editing: boolean): GraphFilterEditorParts {
  const tokens = tokenizeFilterQuery(filterText);
  if (!editing) return { committed: tokens, fragment: "" };
  const trailingSpace = /\s$/.test(filterText);
  if (trailingSpace) return { committed: tokens, fragment: "" };
  if (tokens.length === 0) return { committed: [], fragment: "" };
  return {
    committed: tokens.slice(0, Math.max(0, tokens.length - 1)),
    fragment: tokens[tokens.length - 1] ?? "",
  };
}

function fuzzySubsequenceMatch(needle: string, haystack: string): boolean {
  if (needle.length === 0) return true;
  let i = 0;
  for (let j = 0; j < haystack.length && i < needle.length; j++) {
    if (needle[i] === haystack[j]) i++;
  }
  return i === needle.length;
}

function rankMatch(queryLower: string, targetLower: string): number {
  if (queryLower.length === 0) return 0;
  if (targetLower.startsWith(queryLower)) return 0;
  if (targetLower.includes(queryLower)) return 1;
  if (fuzzySubsequenceMatch(queryLower, targetLower)) return 2;
  return Number.POSITIVE_INFINITY;
}

function uniquePush(out: GraphFilterSuggestion[], token: string, description: string): void {
  if (out.some((item) => item.token === token)) return;
  out.push({ token, description });
}

function sortedMatches<T>(
  values: readonly T[],
  queryLower: string,
  target: (v: T) => string,
  limit = 12,
): T[] {
  return values
    .map((value, idx) => ({
      value,
      idx,
      rank: rankMatch(queryLower, target(value).toLowerCase()),
    }))
    .filter((row) => Number.isFinite(row.rank))
    .sort((a, b) => a.rank - b.rank || a.idx - b.idx)
    .slice(0, limit)
    .map((row) => row.value);
}

export function graphFilterSuggestions(input: GraphFilterSuggestionInput): GraphFilterSuggestion[] {
  const fragment = input.fragment.trim();
  const lowerFragment = fragment.toLowerCase();
  const out: GraphFilterSuggestion[] = [];

  const signed = fragment.startsWith("+") ? "+" : fragment.startsWith("-") ? "-" : "";
  const unsignedFragment = signed ? fragment.slice(1) : fragment;
  const unsignedLower = unsignedFragment.toLowerCase();
  const signedDesc = signed === "+" ? "Include only matching" : "Exclude matching";

  if (!unsignedFragment || !unsignedFragment.includes(":")) {
    const keySuggestions: readonly { key: string; label: string }[] = [
      { key: "+node:<id>", label: "Include only matching nodes by entity id" },
      { key: "-node:<id>", label: "Exclude matching nodes by entity id" },
      { key: "+location:<src>", label: "Include only matching source locations" },
      { key: "-location:<src>", label: "Exclude matching source locations" },
      { key: "+crate:<name>", label: "Include only matching crates" },
      { key: "-crate:<name>", label: "Exclude matching crates" },
      { key: "+process:<id>", label: "Include only matching processes" },
      { key: "-process:<id>", label: "Exclude matching processes" },
      { key: "+kind:<kind>", label: "Include only matching kinds" },
      { key: "-kind:<kind>", label: "Exclude matching kinds" },
      { key: "loners:on", label: "Show unconnected nodes" },
      { key: "loners:off", label: "Hide unconnected nodes" },
      { key: "colorBy:process", label: "Color nodes by process" },
      { key: "colorBy:crate", label: "Color nodes by crate" },
      { key: "groupBy:process", label: "Group by process subgraphs" },
      { key: "groupBy:crate", label: "Group by crate subgraphs" },
      { key: "groupBy:none", label: "Disable grouping" },
    ];
    const matched = sortedMatches(keySuggestions, lowerFragment, (entry) => `${entry.key} ${entry.label}`);
    for (const entry of matched) uniquePush(out, entry.key, entry.label);
    return out;
  }

  const colon = unsignedFragment.indexOf(":");
  const keyLower = unsignedFragment.slice(0, colon).toLowerCase();
  const rawValue = unsignedFragment.slice(colon + 1);
  const valueLower = rawValue.replace(/^"/, "").toLowerCase();

  if ((keyLower === "node" || keyLower === "id") && signed) {
    for (const id of sortedMatches(input.nodeIds, valueLower, (v) => v)) {
      uniquePush(out, `${signed}node:${quoteFilterValue(id)}`, `${signedDesc} node ${id}`);
    }
    return out;
  }
  if ((keyLower === "location" || keyLower === "source") && signed) {
    for (const location of sortedMatches(input.locations, valueLower, (v) => v)) {
      uniquePush(out, `${signed}location:${quoteFilterValue(location)}`, `${signedDesc} location ${location}`);
    }
    return out;
  }
  if (keyLower === "crate" && signed) {
    for (const item of sortedMatches(input.crates, valueLower, (v) => `${v.id} ${v.label}`)) {
      uniquePush(out, `${signed}crate:${quoteFilterValue(item.id)}`, `${signedDesc} crate ${item.label}`);
    }
    return out;
  }
  if (keyLower === "process" && signed) {
    for (const item of sortedMatches(input.processes, valueLower, (v) => `${v.id} ${v.label}`)) {
      uniquePush(out, `${signed}process:${quoteFilterValue(item.id)}`, `${signedDesc} process ${item.label}`);
    }
    return out;
  }
  if (keyLower === "kind" && signed) {
    for (const item of sortedMatches(input.kinds, valueLower, (v) => `${v.id} ${v.label}`)) {
      uniquePush(out, `${signed}kind:${quoteFilterValue(item.id)}`, `${signedDesc} kind ${item.label}`);
    }
    return out;
  }
  if (keyLower === "loners") {
    for (const mode of sortedMatches(["on", "off"], valueLower, (v) => v)) {
      uniquePush(out, `loners:${mode}`, mode === "on" ? "Show unconnected nodes" : "Hide unconnected nodes");
    }
    return out;
  }
  if (keyLower === "colorby") {
    for (const mode of sortedMatches(["process", "crate"], valueLower, (v) => v)) {
      uniquePush(out, `colorBy:${mode}`, `Color nodes by ${mode}`);
    }
    return out;
  }
  if (keyLower === "groupby") {
    for (const mode of sortedMatches(["process", "crate", "none"], valueLower, (v) => v)) {
      uniquePush(out, `groupBy:${mode}`, mode === "none" ? "Disable grouping" : `Group by ${mode}`);
    }
    return out;
  }

  const fallbackKeys: readonly { key: string; label: string }[] = signed
    ? [
        { key: `${signed}node:<id>`, label: "Filter by node id" },
        { key: `${signed}location:<src>`, label: "Filter by source location" },
        { key: `${signed}crate:<name>`, label: "Filter by crate" },
        { key: `${signed}process:<id>`, label: "Filter by process" },
        { key: `${signed}kind:<kind>`, label: "Filter by kind" },
      ]
    : [
        { key: "loners:on", label: "Show unconnected nodes" },
        { key: "loners:off", label: "Hide unconnected nodes" },
        { key: "colorBy:process", label: "Color nodes by process" },
        { key: "colorBy:crate", label: "Color nodes by crate" },
        { key: "groupBy:process", label: "Group by process subgraphs" },
        { key: "groupBy:crate", label: "Group by crate subgraphs" },
        { key: "groupBy:none", label: "Disable grouping" },
      ];
  for (const entry of sortedMatches(fallbackKeys, signed ? unsignedLower : lowerFragment, (v) => `${v.key} ${v.label}`)) {
    uniquePush(out, entry.key, entry.label);
  }
  if (!signed && (lowerFragment === "+" || lowerFragment === "-")) {
    uniquePush(out, `${lowerFragment}node:<id>`, "Filter by node id");
    uniquePush(out, `${lowerFragment}location:<src>`, "Filter by source location");
    uniquePush(out, `${lowerFragment}crate:<name>`, "Filter by crate");
    uniquePush(out, `${lowerFragment}process:<id>`, "Filter by process");
    uniquePush(out, `${lowerFragment}kind:<kind>`, "Filter by kind");
  }
  return out;
}
