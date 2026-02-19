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
  applyToken?: string;
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

export type GraphFilterAst = string[];

export type GraphFilterEditorSelection = { kind: "chip"; index: number } | null;

export type GraphFilterEditorState = {
  ast: GraphFilterAst;
  insertionPoint: number;
  editingIndex: number | null;
  draft: string;
  selection: GraphFilterEditorSelection;
  suggestionsOpen: boolean;
  suggestionIndex: number;
  focused: boolean;
};

export type GraphFilterEditorAction =
  | { type: "sync_from_text"; text: string }
  | { type: "focus_input" }
  | { type: "blur_input" }
  | { type: "set_draft"; draft: string }
  | { type: "remove_chip"; index: number }
  | { type: "backspace_from_draft_start" }
  | { type: "apply_suggestion"; token: string }
  | { type: "move_suggestion"; delta: number; total: number }
  | { type: "open_suggestions" }
  | { type: "close_suggestions" }
  | { type: "set_suggestion_index"; index: number };

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
    const isPlaceholderValue = /^<[^>]+>$/.test(value);
    if (!value) {
      parsed.push({ raw, key, value: valueRaw, valid: false });
      continue;
    }

    let valid = false;
    if (signed !== 0 && !isPlaceholderValue && (keyLower === "node" || keyLower === "id")) {
      (signed > 0 ? includeNodeIds : excludeNodeIds).add(value);
      valid = true;
    } else if (signed !== 0 && !isPlaceholderValue && (keyLower === "location" || keyLower === "source")) {
      (signed > 0 ? includeLocations : excludeLocations).add(value);
      valid = true;
    } else if (signed !== 0 && !isPlaceholderValue && keyLower === "crate") {
      (signed > 0 ? includeCrates : excludeCrates).add(value);
      valid = true;
    } else if (signed !== 0 && !isPlaceholderValue && keyLower === "process") {
      (signed > 0 ? includeProcesses : excludeProcesses).add(value);
      valid = true;
    } else if (signed !== 0 && !isPlaceholderValue && keyLower === "kind") {
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

function clampIndex(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(value, max));
}

function normalizeInsertionPoint(ast: GraphFilterAst, insertionPoint: number): number {
  return clampIndex(insertionPoint, 0, ast.length);
}

function normalizeEditingIndex(ast: GraphFilterAst, editingIndex: number | null): number | null {
  if (editingIndex == null) return null;
  if (editingIndex < 0 || editingIndex >= ast.length) return null;
  return editingIndex;
}

function removeTokenAt(tokens: GraphFilterAst, index: number): GraphFilterAst {
  if (index < 0 || index >= tokens.length) return tokens;
  const next = [...tokens];
  next.splice(index, 1);
  return next;
}

function textToAst(text: string): GraphFilterAst {
  return tokenizeFilterQuery(text);
}

export function graphFilterEditorStateFromText(text: string): GraphFilterEditorState {
  const ast = textToAst(text);
  return {
    ast,
    insertionPoint: ast.length,
    editingIndex: null,
    draft: "",
    selection: null,
    suggestionsOpen: false,
    suggestionIndex: 0,
    focused: false,
  };
}

export function serializeGraphFilterEditorState(state: GraphFilterEditorState): string {
  const tokens = [...state.ast];
  if (state.draft.length > 0) {
    const editingIndex = normalizeEditingIndex(tokens, state.editingIndex);
    if (editingIndex != null) {
      tokens[editingIndex] = state.draft;
    } else {
      const at = normalizeInsertionPoint(tokens, state.insertionPoint);
      tokens.splice(at, 0, state.draft);
    }
  }
  return tokens.join(" ");
}

export function graphFilterEditorReducer(
  state: GraphFilterEditorState,
  action: GraphFilterEditorAction,
): GraphFilterEditorState {
  switch (action.type) {
    case "sync_from_text": {
      const next = graphFilterEditorStateFromText(action.text);
      return {
        ...next,
        focused: state.focused,
      };
    }
    case "focus_input":
      return {
        ...state,
        focused: true,
        selection: null,
        editingIndex: null,
        insertionPoint: state.ast.length,
        suggestionsOpen: true,
        suggestionIndex: 0,
      };
    case "blur_input":
      return {
        ...state,
        focused: false,
        selection: null,
        editingIndex: null,
        insertionPoint: state.ast.length,
        suggestionsOpen: false,
      };
    case "set_draft":
      return {
        ...state,
        draft: action.draft,
        selection: null,
        suggestionsOpen: true,
        suggestionIndex: 0,
      };
    case "remove_chip": {
      const ast = removeTokenAt(state.ast, action.index);
      const insertionPoint = normalizeInsertionPoint(
        ast,
        action.index < state.insertionPoint ? state.insertionPoint - 1 : state.insertionPoint,
      );
      const editingIndex = normalizeEditingIndex(
        ast,
        state.editingIndex == null
          ? null
          : action.index < state.editingIndex
            ? state.editingIndex - 1
            : state.editingIndex,
      );
      const shouldClearDraft = state.editingIndex === action.index;
      return {
        ...state,
        ast,
        insertionPoint,
        editingIndex,
        draft: shouldClearDraft ? "" : state.draft,
        selection: null,
      };
    }
    case "backspace_from_draft_start": {
      if (state.draft.length > 0) return state;
      if (state.editingIndex != null) {
        const ast = removeTokenAt(state.ast, state.editingIndex);
        const insertionPoint = normalizeInsertionPoint(ast, state.editingIndex);
        return {
          ...state,
          ast,
          insertionPoint,
          editingIndex: null,
          draft: "",
          selection: null,
          suggestionsOpen: true,
          suggestionIndex: 0,
        };
      }
      const at = normalizeInsertionPoint(state.ast, state.insertionPoint);
      if (at === 0) return state;
      const ast = removeTokenAt(state.ast, at - 1);
      return {
        ...state,
        ast,
        insertionPoint: at - 1,
        selection: null,
        suggestionsOpen: true,
        suggestionIndex: 0,
      };
    }
    case "apply_suggestion": {
      const token = action.token.trim();
      if (!token) return state;
      const ast = [...state.ast];
      const editingIndex = normalizeEditingIndex(ast, state.editingIndex);
      let insertionPoint = normalizeInsertionPoint(ast, state.insertionPoint);
      if (editingIndex != null) {
        ast[editingIndex] = token;
        insertionPoint = editingIndex + 1;
      } else {
        ast.splice(insertionPoint, 0, token);
        insertionPoint += 1;
      }
      return {
        ...state,
        ast,
        insertionPoint,
        editingIndex: null,
        draft: "",
        selection: null,
        suggestionsOpen: true,
        suggestionIndex: 0,
      };
    }
    case "move_suggestion": {
      if (action.total <= 0) return state;
      const nextIndex = (state.suggestionIndex + action.delta + action.total) % action.total;
      return {
        ...state,
        suggestionsOpen: true,
        suggestionIndex: nextIndex,
      };
    }
    case "open_suggestions":
      return { ...state, suggestionsOpen: true };
    case "close_suggestions":
      return { ...state, suggestionsOpen: false };
    case "set_suggestion_index":
      return {
        ...state,
        suggestionIndex: Math.max(0, action.index),
      };
  }
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

function uniquePush(out: GraphFilterSuggestion[], token: string, description: string, applyToken?: string): void {
  if (out.some((item) => item.token === token)) return;
  out.push({ token, description, applyToken });
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

  if (!signed && !fragment.includes(":")) {
    const rootSuggestions = [
      { token: "+", description: "Include only filter", applyToken: "+" },
      { token: "-", description: "Exclude everything matching this filter", applyToken: "-" },
    ];
    for (const item of sortedMatches(rootSuggestions, lowerFragment, (v) => `${v.token} ${v.description}`)) {
      uniquePush(out, item.token, item.description, item.applyToken);
    }
    return out;
  }

  if (!unsignedFragment || !unsignedFragment.includes(":")) {
    const keySuggestions: readonly { key: string; label: string; applyToken?: string }[] = [
      { key: "+node:<id>", label: "Include only matching nodes by entity id", applyToken: "+node:" },
      { key: "-node:<id>", label: "Exclude matching nodes by entity id", applyToken: "-node:" },
      { key: "+location:<src>", label: "Include only matching source locations", applyToken: "+location:" },
      { key: "-location:<src>", label: "Exclude matching source locations", applyToken: "-location:" },
      { key: "+crate:<name>", label: "Include only matching crates", applyToken: "+crate:" },
      { key: "-crate:<name>", label: "Exclude matching crates", applyToken: "-crate:" },
      { key: "+process:<id>", label: "Include only matching processes", applyToken: "+process:" },
      { key: "-process:<id>", label: "Exclude matching processes", applyToken: "-process:" },
      { key: "+kind:<kind>", label: "Include only matching kinds", applyToken: "+kind:" },
      { key: "-kind:<kind>", label: "Exclude matching kinds", applyToken: "-kind:" },
      { key: "loners:on", label: "Show unconnected nodes" },
      { key: "loners:off", label: "Hide unconnected nodes" },
      { key: "colorBy:process", label: "Color nodes by process" },
      { key: "colorBy:crate", label: "Color nodes by crate" },
      { key: "groupBy:process", label: "Group by process subgraphs" },
      { key: "groupBy:crate", label: "Group by crate subgraphs" },
      { key: "groupBy:none", label: "Disable grouping" },
    ];
    const matched = sortedMatches(keySuggestions, lowerFragment, (entry) => `${entry.key} ${entry.label}`);
    for (const entry of matched) uniquePush(out, entry.key, entry.label, entry.applyToken);
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

  const fallbackKeys: readonly { key: string; label: string; applyToken?: string }[] = signed
    ? [
        { key: `${signed}node:<id>`, label: "Filter by node id", applyToken: `${signed}node:` },
        { key: `${signed}location:<src>`, label: "Filter by source location", applyToken: `${signed}location:` },
        { key: `${signed}crate:<name>`, label: "Filter by crate", applyToken: `${signed}crate:` },
        { key: `${signed}process:<id>`, label: "Filter by process", applyToken: `${signed}process:` },
        { key: `${signed}kind:<kind>`, label: "Filter by kind", applyToken: `${signed}kind:` },
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
    uniquePush(out, entry.key, entry.label, entry.applyToken);
  }
  if (!signed && (lowerFragment === "+" || lowerFragment === "-")) {
    uniquePush(out, `${lowerFragment}node:<id>`, "Filter by node id", `${lowerFragment}node:`);
    uniquePush(out, `${lowerFragment}location:<src>`, "Filter by source location", `${lowerFragment}location:`);
    uniquePush(out, `${lowerFragment}crate:<name>`, "Filter by crate", `${lowerFragment}crate:`);
    uniquePush(out, `${lowerFragment}process:<id>`, "Filter by process", `${lowerFragment}process:`);
    uniquePush(out, `${lowerFragment}kind:<kind>`, "Filter by kind", `${lowerFragment}kind:`);
  }
  return out;
}
