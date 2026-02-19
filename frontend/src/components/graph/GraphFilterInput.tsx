import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Crosshair } from "@phosphor-icons/react";
import { ActionButton } from "../../ui/primitives/ActionButton";
import { Badge } from "../../ui/primitives/Badge";
import type { FilterMenuItem } from "../../ui/primitives/FilterMenu";
import {
  ensureTrailingSpaceForNewFilter,
  graphFilterEditorParts,
  graphFilterSuggestions,
  parseGraphFilterQuery,
  replaceTrailingFragment,
} from "../../graphFilter";

export function GraphFilterInput({
  focusedEntityId,
  onExitFocus,
  scopeFilterLabel,
  onClearScopeFilter,
  graphFilterText,
  onGraphFilterTextChange,
  crateItems,
  processItems,
  kindItems,
  nodeIds,
  locations,
}: {
  focusedEntityId: string | null;
  onExitFocus: () => void;
  scopeFilterLabel?: string | null;
  onClearScopeFilter?: () => void;
  graphFilterText: string;
  onGraphFilterTextChange: (next: string) => void;
  crateItems: FilterMenuItem[];
  processItems: FilterMenuItem[];
  kindItems: FilterMenuItem[];
  nodeIds: string[];
  locations: string[];
}) {
  const graphFilterInputRef = useRef<HTMLInputElement | null>(null);
  const [graphFilterSuggestionIndex, setGraphFilterSuggestionIndex] = useState(0);
  const [graphFilterSuggestOpen, setGraphFilterSuggestOpen] = useState(false);
  const [graphFilterEditing, setGraphFilterEditing] = useState(false);

  const parsedGraphFilters = useMemo(() => parseGraphFilterQuery(graphFilterText), [graphFilterText]);
  const graphFilterTokens = parsedGraphFilters.tokens;
  const filterParts = useMemo(
    () => graphFilterEditorParts(graphFilterText, graphFilterEditing),
    [graphFilterText, graphFilterEditing],
  );
  const currentFragment = useMemo(() => filterParts.fragment.trim(), [filterParts.fragment]);
  const graphFilterSuggestionsList = useMemo(
    () =>
      graphFilterSuggestions({
        fragment: currentFragment,
        nodeIds,
        locations,
        crates: crateItems.map((item) => ({ id: item.id, label: String(item.label ?? item.id) })),
        processes: processItems.map((item) => ({ id: item.id, label: String(item.label ?? item.id) })),
        kinds: kindItems.map((item) => ({ id: item.id, label: String(item.label ?? item.id) })),
      }),
    [currentFragment, nodeIds, locations, crateItems, processItems, kindItems],
  );

  const applyGraphFilterSuggestion = useCallback(
    (token: string) => {
      onGraphFilterTextChange(replaceTrailingFragment(graphFilterText, token));
      setGraphFilterSuggestOpen(false);
      setGraphFilterSuggestionIndex(0);
      graphFilterInputRef.current?.focus();
    },
    [graphFilterText, onGraphFilterTextChange],
  );

  const setFilterFragment = useCallback(
    (fragment: string) => {
      const prefix = filterParts.committed.join(" ");
      if (prefix.length === 0) {
        onGraphFilterTextChange(fragment);
        return;
      }
      if (fragment.length === 0) {
        onGraphFilterTextChange(`${prefix} `);
        return;
      }
      onGraphFilterTextChange(`${prefix} ${fragment}`);
    },
    [filterParts.committed, onGraphFilterTextChange],
  );

  useEffect(() => {
    if (graphFilterSuggestionIndex < graphFilterSuggestionsList.length) return;
    setGraphFilterSuggestionIndex(0);
  }, [graphFilterSuggestionIndex, graphFilterSuggestionsList.length]);

  return (
    <div className="graph-toolbar">
      <div className="graph-toolbar-middle">
        <div
          className="graph-filter-input"
          onMouseDown={(event) => {
            if (event.target instanceof HTMLElement && event.target.closest(".graph-filter-chip")) return;
            graphFilterInputRef.current?.focus();
          }}
        >
          {filterParts.committed.map((raw, index) => {
            const parsed = graphFilterTokens[index];
            const valid = parsed?.valid ?? false;
            return (
              <button
                key={`${raw}:${index}`}
                type="button"
                className={[
                  "graph-filter-chip",
                  valid ? "graph-filter-chip--valid" : "graph-filter-chip--invalid",
                ].join(" ")}
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => {
                  const next = filterParts.committed.filter((_, i) => i !== index);
                  onGraphFilterTextChange(next.join(" "));
                  graphFilterInputRef.current?.focus();
                }}
                title={valid ? "remove filter token" : "invalid filter token"}
              >
                {raw}
                <span className="graph-filter-chip-x" aria-hidden="true">×</span>
              </button>
            );
          })}
          <input
            ref={graphFilterInputRef}
            type="text"
            value={filterParts.fragment}
            onChange={(event) => {
              setFilterFragment(event.target.value);
              setGraphFilterSuggestOpen(true);
              setGraphFilterSuggestionIndex(0);
            }}
            onFocus={() => {
              setGraphFilterEditing(true);
              const nextText = ensureTrailingSpaceForNewFilter(graphFilterText);
              if (nextText !== graphFilterText) onGraphFilterTextChange(nextText);
              setGraphFilterSuggestOpen(true);
            }}
            onBlur={() => {
              setGraphFilterEditing(false);
              window.setTimeout(() => setGraphFilterSuggestOpen(false), 100);
            }}
            onKeyDown={(event) => {
              if (event.key === "Backspace" && filterParts.fragment.length === 0 && filterParts.committed.length > 0) {
                event.preventDefault();
                const next = filterParts.committed.slice(0, -1);
                onGraphFilterTextChange(next.join(" "));
                setGraphFilterSuggestOpen(true);
                setGraphFilterSuggestionIndex(0);
                return;
              }
              if (!graphFilterSuggestOpen || graphFilterSuggestionsList.length === 0) return;
              if (event.key === "ArrowDown") {
                event.preventDefault();
                setGraphFilterSuggestionIndex((idx) => (idx + 1) % graphFilterSuggestionsList.length);
                return;
              }
              if (event.key === "ArrowUp") {
                event.preventDefault();
                setGraphFilterSuggestionIndex(
                  (idx) => (idx + graphFilterSuggestionsList.length - 1) % graphFilterSuggestionsList.length,
                );
                return;
              }
              if (event.key === "Enter" || event.key === "Tab") {
                const choice = graphFilterSuggestionsList[graphFilterSuggestionIndex];
                if (!choice) return;
                event.preventDefault();
                applyGraphFilterSuggestion(choice.token);
              }
            }}
            placeholder={
              filterParts.committed.length === 0
                ? "filters: node:.. location:.. crate:.. process:.. kind:.. loners:on|off colorBy:.. groupBy:.."
                : "add filter…"
            }
            className="graph-filter-fragment-input"
            aria-label="Graph filter query"
          />
        </div>
        {graphFilterSuggestOpen && graphFilterSuggestionsList.length > 0 && (
          <div className="graph-filter-suggestions">
            {graphFilterSuggestionsList.map((suggestion, index) => (
              <button
                key={suggestion.token}
                type="button"
                className={[
                  "graph-filter-suggestion",
                  index === graphFilterSuggestionIndex && "graph-filter-suggestion--active",
                ].filter(Boolean).join(" ")}
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => applyGraphFilterSuggestion(suggestion.token)}
              >
                <span className="graph-filter-suggestion-token">{suggestion.token}</span>
                <span className="graph-filter-suggestion-sep"> - </span>
                <span className="graph-filter-suggestion-label">{suggestion.description}</span>
              </button>
            ))}
          </div>
        )}
      </div>
      <div className="graph-toolbar-right">
        {focusedEntityId && (
          <ActionButton size="sm" onPress={onExitFocus}>
            <Crosshair size={14} weight="bold" />
            Exit Focus
          </ActionButton>
        )}
        {scopeFilterLabel && (
          <>
            <Badge tone="warn">in:{scopeFilterLabel}</Badge>
            <ActionButton size="sm" onPress={onClearScopeFilter}>Clear scope</ActionButton>
          </>
        )}
      </div>
    </div>
  );
}
