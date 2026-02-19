import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Crosshair } from "@phosphor-icons/react";
import { ActionButton } from "../../ui/primitives/ActionButton";
import { Badge } from "../../ui/primitives/Badge";
import type { FilterMenuItem } from "../../ui/primitives/FilterMenu";
import {
  graphFilterEditorReducer,
  graphFilterEditorStateFromText,
  graphFilterSuggestions,
  parseGraphFilterQuery,
  serializeGraphFilterEditorState,
  type GraphFilterEditorAction,
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
  const graphFilterRootRef = useRef<HTMLDivElement | null>(null);
  const graphFilterTextRef = useRef(graphFilterText);
  const [editorState, setEditorState] = useState(() => graphFilterEditorStateFromText(graphFilterText));
  const editorStateRef = useRef(editorState);
  const pendingOutboundTextRef = useRef<string | null>(null);

  useEffect(() => {
    graphFilterTextRef.current = graphFilterText;
    const localText = serializeGraphFilterEditorState(editorStateRef.current);
    if (graphFilterText === localText) return;
    if (pendingOutboundTextRef.current === graphFilterText) {
      pendingOutboundTextRef.current = null;
      return;
    }
    const next = graphFilterEditorStateFromText(graphFilterText);
    editorStateRef.current = next;
    setEditorState(next);
  }, [graphFilterText]);

  const applyEditorAction = useCallback(
    (action: GraphFilterEditorAction, emitChange = true) => {
      const prev = editorStateRef.current;
      const next = graphFilterEditorReducer(prev, action);
      if (next === prev) return;
      editorStateRef.current = next;
      setEditorState(next);
      if (!emitChange) return;
      const nextText = serializeGraphFilterEditorState(next);
      if (nextText === graphFilterTextRef.current) return;
      pendingOutboundTextRef.current = nextText;
      onGraphFilterTextChange(nextText);
    },
    [onGraphFilterTextChange],
  );

  const graphFilterTokens = useMemo(
    () =>
      editorState.ast.map((raw) => {
        const parsed = parseGraphFilterQuery(raw).tokens[0];
        return parsed ?? { raw, key: null, value: null, valid: false };
      }),
    [editorState.ast],
  );
  const currentFragment = useMemo(() => editorState.draft.trim(), [editorState.draft]);
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
  const activeSuggestionIndex =
    graphFilterSuggestionsList.length === 0
      ? 0
      : Math.min(editorState.suggestionIndex, graphFilterSuggestionsList.length - 1);

  useEffect(() => {
    if (graphFilterSuggestionsList.length === 0) {
      if (editorState.suggestionIndex !== 0) {
        applyEditorAction({ type: "set_suggestion_index", index: 0 }, false);
      }
      return;
    }
    if (editorState.suggestionIndex < graphFilterSuggestionsList.length) return;
    applyEditorAction({ type: "set_suggestion_index", index: 0 }, false);
  }, [applyEditorAction, editorState.suggestionIndex, graphFilterSuggestionsList.length]);

  const applyGraphFilterSuggestion = useCallback(
    (token: string) => {
      if (token === "+" || token === "-" || token.endsWith(":")) {
        applyEditorAction({ type: "set_draft", draft: token });
        graphFilterInputRef.current?.focus();
        return;
      }
      applyEditorAction({ type: "apply_suggestion", token });
      graphFilterInputRef.current?.focus();
    },
    [applyEditorAction],
  );

  useEffect(() => {
    function onPointerDown(event: PointerEvent) {
      const root = graphFilterRootRef.current;
      if (!root) return;
      if (event.target instanceof Node && root.contains(event.target)) return;
      applyEditorAction({ type: "blur_input" }, false);
      if (document.activeElement === graphFilterInputRef.current) {
        graphFilterInputRef.current?.blur();
      }
    }
    document.addEventListener("pointerdown", onPointerDown);
    return () => document.removeEventListener("pointerdown", onPointerDown);
  }, [applyEditorAction]);

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "k" && (event.metaKey || event.ctrlKey)) {
        event.preventDefault();
        graphFilterInputRef.current?.focus();
      }
    }
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, []);

  return (
    <div className="graph-toolbar">
      <div className="graph-toolbar-middle" ref={graphFilterRootRef}>
        <div
          className="graph-filter-input"
          onMouseDown={(event) => {
            if (event.target === graphFilterInputRef.current) return;
            if (event.target instanceof HTMLElement && event.target.closest(".graph-filter-chip")) return;
            event.preventDefault();
            graphFilterInputRef.current?.focus();
          }}
        >
          {editorState.ast.map((raw, index) => {
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
                  applyEditorAction({ type: "remove_chip", index });
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
            value={editorState.draft}
            onChange={(event) => {
              applyEditorAction({ type: "set_draft", draft: event.target.value });
            }}
            onFocus={() => {
              applyEditorAction({ type: "focus_input" }, false);
            }}
            onBlur={() => {
              applyEditorAction({ type: "blur_input" }, false);
            }}
            onKeyDown={(event) => {
              if (event.key === "Backspace" && editorState.draft.length === 0 && editorState.insertionPoint > 0) {
                event.preventDefault();
                applyEditorAction({ type: "backspace_from_draft_start" });
                return;
              }
              if (event.key === "Tab") {
                event.preventDefault();
                if (!editorState.suggestionsOpen || graphFilterSuggestionsList.length === 0) {
                  applyEditorAction({ type: "open_suggestions" }, false);
                  return;
                }
                if (event.shiftKey) {
                  applyEditorAction({ type: "move_suggestion", delta: -1, total: graphFilterSuggestionsList.length }, false);
                  return;
                }
                const choice = graphFilterSuggestionsList[activeSuggestionIndex];
                if (!choice) return;
                applyGraphFilterSuggestion(choice.applyToken ?? choice.token);
                return;
              }
              if (!editorState.suggestionsOpen || graphFilterSuggestionsList.length === 0) return;
              if (event.key === "ArrowDown") {
                event.preventDefault();
                applyEditorAction({ type: "move_suggestion", delta: 1, total: graphFilterSuggestionsList.length }, false);
                return;
              }
              if (event.key === "ArrowUp") {
                event.preventDefault();
                applyEditorAction({ type: "move_suggestion", delta: -1, total: graphFilterSuggestionsList.length }, false);
                return;
              }
              if (event.key === "Escape") {
                event.preventDefault();
                applyEditorAction({ type: "close_suggestions" }, false);
                return;
              }
              if (event.key === "Enter") {
                const choice = graphFilterSuggestionsList[activeSuggestionIndex];
                if (!choice) return;
                event.preventDefault();
                applyGraphFilterSuggestion(choice.applyToken ?? choice.token);
              }
            }}
            placeholder={
              editorState.ast.length === 0
                ? "filters: node:.. location:.. crate:.. process:.. kind:.. loners:on|off colorBy:.. groupBy:.."
                : "add filter…"
            }
            className="graph-filter-fragment-input"
            aria-label="Graph filter query"
          />
          {!editorState.focused && editorState.draft.length === 0 && (
            <kbd className="graph-filter-shortcut">⌘K</kbd>
          )}
        </div>
        {editorState.suggestionsOpen && graphFilterSuggestionsList.length > 0 && (
          <div className="graph-filter-suggestions">
            {graphFilterSuggestionsList.map((suggestion, index) => (
              <button
                key={suggestion.token}
                type="button"
                className={[
                  "graph-filter-suggestion",
                  index === activeSuggestionIndex && "graph-filter-suggestion--active",
                ].filter(Boolean).join(" ")}
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => applyGraphFilterSuggestion(suggestion.applyToken ?? suggestion.token)}
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
