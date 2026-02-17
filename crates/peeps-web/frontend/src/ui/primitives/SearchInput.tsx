import {
  Button,
  ComboBox,
  Input,
  ListBox,
  ListBoxItem,
  Popover,
} from "react-aria-components";
import { MagnifyingGlass, X } from "@phosphor-icons/react";
import type React from "react";

export type SearchSuggestion = {
  id: string;
  label: React.ReactNode;
  meta?: React.ReactNode;
};

export function SearchInput({
  value,
  onChange,
  items,
  showSuggestions,
  selectedId,
  onSelect,
  onAltSelect,
  resultHint,
  filterBadge,
  onClearFilter,
  placeholder,
  className,
  "aria-label": ariaLabel,
}: {
  value: string;
  onChange: (value: string) => void;
  items: readonly SearchSuggestion[];
  showSuggestions?: boolean;
  selectedId?: string | null;
  onSelect?: (id: string) => void;
  onAltSelect?: (id: string) => void;
  resultHint?: React.ReactNode;
  filterBadge?: React.ReactNode;
  onClearFilter?: () => void;
  placeholder?: string;
  className?: string;
  "aria-label"?: string;
}) {
  return (
    <ComboBox
      inputValue={value}
      onInputChange={onChange}
      menuTrigger="input"
      allowsEmptyCollection
      className={["ui-search-autocomplete", className].filter(Boolean).join(" ")}
      aria-label={ariaLabel}
    >
      <div className="ui-search">
        <MagnifyingGlass size={14} weight="bold" className="ui-search-icon" />
        <Input
          className="ui-input ui-search-input"
          placeholder={placeholder}
        />
        <Button
          className="ui-search-clear"
          aria-label="Clear search"
          onPress={() => onChange("")}
        >
          <X size={12} weight="bold" />
        </Button>
      </div>
      {showSuggestions && (
        <Popover className="ui-search-popover" placement="bottom start" offset={6}>
          <div className="ui-search-results">
            {resultHint && (
              <div className="ui-search-results-head">{resultHint}</div>
            )}
            {filterBadge && (
              <div className="ui-search-filter">
                {filterBadge}
                <button
                  type="button"
                  className="ui-search-filter-clear"
                  onClick={() => onClearFilter?.()}
                >
                  clear filter
                </button>
              </div>
            )}
            {items.length === 0 ? (
              <div className="ui-search-empty">No matches.</div>
            ) : (
              <ListBox className="ui-search-results-list">
                {items.map((item) => (
                  <ListBoxItem
                    key={item.id}
                    id={item.id}
                    className={[
                      "ui-search-result-item",
                      selectedId === item.id && "ui-search-result-item--active",
                    ].filter(Boolean).join(" ")}
                    textValue={String(item.label)}
                    onPress={(event) => {
                      if (event.altKey) {
                        onAltSelect?.(item.id);
                      } else {
                        onSelect?.(item.id);
                      }
                    }}
                  >
                    <span className="ui-search-result-label">{item.label}</span>
                    {item.meta && <span className="ui-search-result-meta">{item.meta}</span>}
                  </ListBoxItem>
                ))}
              </ListBox>
            )}
          </div>
        </Popover>
      )}
    </ComboBox>
  );
}
