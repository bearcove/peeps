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
  const hiddenNodeIds = new Set<string>();
  const hiddenLocations = new Set<string>();
  const hiddenCrates = new Set<string>();
  const hiddenProcesses = new Set<string>();
  const hiddenKinds = new Set<string>();
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

    const key = raw.slice(0, colon);
    const keyLower = key.toLowerCase();
    const valueRaw = stripFilterQuotes(raw.slice(colon + 1));
    const value = valueRaw.trim();
    if (!value) {
      parsed.push({ raw, key, value: valueRaw, valid: false });
      continue;
    }

    let valid = false;
    if (keyLower === "node" || keyLower === "id") {
      hiddenNodeIds.add(value);
      valid = true;
    } else if (keyLower === "location" || keyLower === "source") {
      hiddenLocations.add(value);
      valid = true;
    } else if (keyLower === "crate") {
      hiddenCrates.add(value);
      valid = true;
    } else if (keyLower === "process") {
      hiddenProcesses.add(value);
      valid = true;
    } else if (keyLower === "kind") {
      hiddenKinds.add(value);
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
    } else if (keyLower === "hide") {
      const secondColon = value.indexOf(":");
      if (secondColon > 0) {
        const subKey = value.slice(0, secondColon).toLowerCase();
        const subValue = stripFilterQuotes(value.slice(secondColon + 1)).trim();
        if (subValue.length > 0) {
          if (subKey === "node" || subKey === "id") {
            hiddenNodeIds.add(subValue);
            valid = true;
          } else if (subKey === "location" || subKey === "source") {
            hiddenLocations.add(subValue);
            valid = true;
          } else if (subKey === "crate") {
            hiddenCrates.add(subValue);
            valid = true;
          } else if (subKey === "process") {
            hiddenProcesses.add(subValue);
            valid = true;
          } else if (subKey === "kind") {
            hiddenKinds.add(subValue);
            valid = true;
          }
        }
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
    hiddenNodeIds,
    hiddenLocations,
    hiddenCrates,
    hiddenProcesses,
    hiddenKinds,
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

function uniquePush(out: GraphFilterSuggestion[], label: string, token: string): void {
  if (out.some((item) => item.token === token)) return;
  out.push({ label, token });
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

  if (!fragment || !fragment.includes(":")) {
    const keySuggestions: readonly { key: string; label: string }[] = [
      { key: "node:", label: "hide node (`node:`)" },
      { key: "location:", label: "hide location (`location:`)" },
      { key: "crate:", label: "hide crate (`crate:`)" },
      { key: "process:", label: "hide process (`process:`)" },
      { key: "kind:", label: "hide kind (`kind:`)" },
      { key: "hide:", label: "hide (2-stage)" },
      { key: "loners:on", label: "show loners (`loners:on`)" },
      { key: "loners:off", label: "hide loners (`loners:off`)" },
      { key: "colorBy:process", label: "color by process" },
      { key: "colorBy:crate", label: "color by crate" },
      { key: "groupBy:process", label: "group by process" },
      { key: "groupBy:crate", label: "group by crate" },
      { key: "groupBy:none", label: "disable subgraphs" },
    ];

    const matched = sortedMatches(keySuggestions, lowerFragment, (entry) => `${entry.key} ${entry.label}`);
    for (const entry of matched) uniquePush(out, entry.label, entry.key);
    return out;
  }

  const colon = fragment.indexOf(":");
  const keyLower = fragment.slice(0, colon).toLowerCase();
  const rawValue = fragment.slice(colon + 1);
  const valueLower = rawValue.replace(/^"/, "").toLowerCase();

  if (keyLower === "node" || keyLower === "id") {
    for (const id of sortedMatches(input.nodeIds, valueLower, (v) => v)) {
      uniquePush(out, `node ${id}`, `node:${quoteFilterValue(id)}`);
    }
    return out;
  }
  if (keyLower === "hide") {
    const secondColon = rawValue.indexOf(":");
    if (secondColon < 1) {
      const stageOne = sortedMatches(
        [
          { sub: "node", label: "node" },
          { sub: "location", label: "location" },
          { sub: "crate", label: "crate" },
          { sub: "process", label: "process" },
          { sub: "kind", label: "kind" },
        ],
        valueLower,
        (v) => v.sub,
      );
      for (const row of stageOne) {
        uniquePush(out, `hide ${row.label}`, `hide:${row.sub}:`);
      }
      return out;
    }

    const subKey = rawValue.slice(0, secondColon).toLowerCase();
    const subValueLower = rawValue.slice(secondColon + 1).replace(/^"/, "").toLowerCase();
    if (subKey === "node" || subKey === "id") {
      for (const id of sortedMatches(input.nodeIds, subValueLower, (v) => v)) {
        uniquePush(out, `hide node ${id}`, `hide:node:${quoteFilterValue(id)}`);
      }
      return out;
    }
    if (subKey === "location" || subKey === "source") {
      for (const location of sortedMatches(input.locations, subValueLower, (v) => v)) {
        uniquePush(out, `hide location ${location}`, `hide:location:${quoteFilterValue(location)}`);
      }
      return out;
    }
    if (subKey === "crate") {
      for (const item of sortedMatches(input.crates, subValueLower, (v) => `${v.id} ${v.label}`)) {
        uniquePush(out, `hide crate ${item.label}`, `hide:crate:${quoteFilterValue(item.id)}`);
      }
      return out;
    }
    if (subKey === "process") {
      for (const item of sortedMatches(input.processes, subValueLower, (v) => `${v.id} ${v.label}`)) {
        uniquePush(out, `hide process ${item.label}`, `hide:process:${quoteFilterValue(item.id)}`);
      }
      return out;
    }
    if (subKey === "kind") {
      for (const item of sortedMatches(input.kinds, subValueLower, (v) => `${v.id} ${v.label}`)) {
        uniquePush(out, `hide kind ${item.label}`, `hide:kind:${quoteFilterValue(item.id)}`);
      }
      return out;
    }
    return out;
  }
  if (keyLower === "location" || keyLower === "source") {
    for (const location of sortedMatches(input.locations, valueLower, (v) => v)) {
      uniquePush(out, `location ${location}`, `location:${quoteFilterValue(location)}`);
    }
    return out;
  }
  if (keyLower === "crate") {
    for (const item of sortedMatches(input.crates, valueLower, (v) => `${v.id} ${v.label}`)) {
      uniquePush(out, `crate ${item.label}`, `crate:${quoteFilterValue(item.id)}`);
    }
    return out;
  }
  if (keyLower === "process") {
    for (const item of sortedMatches(input.processes, valueLower, (v) => `${v.id} ${v.label}`)) {
      uniquePush(out, `process ${item.label}`, `process:${quoteFilterValue(item.id)}`);
    }
    return out;
  }
  if (keyLower === "kind") {
    for (const item of sortedMatches(input.kinds, valueLower, (v) => `${v.id} ${v.label}`)) {
      uniquePush(out, `kind ${item.label}`, `kind:${quoteFilterValue(item.id)}`);
    }
    return out;
  }
  if (keyLower === "loners") {
    for (const mode of sortedMatches(["on", "off"], valueLower, (v) => v)) {
      uniquePush(out, `loners ${mode}`, `loners:${mode}`);
    }
    return out;
  }
  if (keyLower === "colorby") {
    for (const mode of sortedMatches(["process", "crate"], valueLower, (v) => v)) {
      uniquePush(out, `color by ${mode}`, `colorBy:${mode}`);
    }
    return out;
  }
  if (keyLower === "groupby") {
    for (const mode of sortedMatches(["process", "crate", "none"], valueLower, (v) => v)) {
      uniquePush(out, `group by ${mode}`, `groupBy:${mode}`);
    }
    return out;
  }

  const fallbackKeys: readonly { key: string; label: string }[] = [
    { key: "node:", label: "hide node (`node:`)" },
    { key: "location:", label: "hide location (`location:`)" },
    { key: "crate:", label: "hide crate (`crate:`)" },
    { key: "process:", label: "hide process (`process:`)" },
    { key: "kind:", label: "hide kind (`kind:`)" },
    { key: "hide:", label: "hide (2-stage)" },
    { key: "loners:", label: "show/hide loners (`loners:on|off`)" },
    { key: "colorBy:", label: "color by (`colorBy:`)" },
    { key: "groupBy:", label: "group by (`groupBy:`)" },
  ];
  for (const entry of sortedMatches(fallbackKeys, lowerFragment, (v) => `${v.key} ${v.label}`)) {
    uniquePush(out, entry.label, entry.key);
  }
  return out;
}
