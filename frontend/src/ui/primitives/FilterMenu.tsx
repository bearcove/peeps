import type React from "react";
import { useState } from "react";
import "./FilterMenu.css";
import {
  Button,
  Dialog,
  DialogTrigger,
  Popover,
} from "react-aria-components";
import { Checkbox as AriaCheckbox } from "react-aria-components";
import { Check, Funnel, CaretDown } from "@phosphor-icons/react";
import { Switch } from "./Switch";

export type FilterMenuItem = {
  id: string;
  label: React.ReactNode;
  icon?: React.ReactNode;
  meta?: React.ReactNode;
};

export type FilterMenuProps = {
  label: React.ReactNode;
  items: readonly FilterMenuItem[];
  /** Set of currently-hidden item IDs */
  hiddenIds: ReadonlySet<string>;
  onToggle: (id: string) => void;
  onSolo: (id: string) => void;
  colorByActive?: boolean;
  onToggleColorBy?: () => void;
  colorByLabel?: React.ReactNode;
  subgraphsActive?: boolean;
  onToggleSubgraphs?: () => void;
  subgraphsLabel?: React.ReactNode;
  subgraphsDisabled?: boolean;
  className?: string;
};

export function FilterMenu({
  label,
  items,
  hiddenIds,
  onToggle,
  onSolo,
  colorByActive = false,
  onToggleColorBy,
  colorByLabel,
  subgraphsActive = false,
  onToggleSubgraphs,
  subgraphsLabel,
  subgraphsDisabled = false,
  className,
}: FilterMenuProps) {
  const [open, setOpen] = useState(false);
  const hiddenCount = items.filter((item) => hiddenIds.has(item.id)).length;

  return (
    <DialogTrigger isOpen={open} onOpenChange={setOpen}>
      <Button
        className={[
          "ui-filter-trigger",
          hiddenCount > 0 && "ui-filter-trigger--active",
          className,
        ]
          .filter(Boolean)
          .join(" ")}
      >
        <Funnel size={12} weight="bold" />
        <span>{label}</span>
        {hiddenCount > 0 && (
          <span className="ui-filter-badge">
            {hiddenCount} hidden
          </span>
        )}
        <CaretDown
          size={10}
          weight="bold"
          className={[
            "ui-filter-caret",
            open && "ui-filter-caret--open",
          ]
            .filter(Boolean)
            .join(" ")}
        />
      </Button>
      <Popover
        className="ui-filter-popover"
        placement="bottom start"
        offset={0}
      >
        <Dialog className="ui-filter-dialog" aria-label={`Filter ${label}`}>
          {onToggleColorBy && (
            <>
              <div className="ui-filter-setting-row">
                <Switch
                  checked={colorByActive}
                  onChange={onToggleColorBy}
                  label={colorByLabel ?? `Color by ${label}`}
                />
              </div>
              {onToggleSubgraphs && (
                <div className="ui-filter-setting-row">
                  <Switch
                    checked={subgraphsActive}
                    onChange={onToggleSubgraphs}
                    isDisabled={subgraphsDisabled}
                    label={subgraphsLabel ?? "Use subgraphs"}
                  />
                </div>
              )}
              <div className="ui-filter-divider" />
            </>
          )}
          <ul className="ui-filter-list" role="group">
            {items.map((item) => {
              const checked = !hiddenIds.has(item.id);
              return (
                <li
                  key={item.id}
                  className="ui-filter-item"
                  onPointerDownCapture={(e) => {
                    if (e.altKey) {
                      e.preventDefault();
                      e.stopPropagation();
                      onSolo(item.id);
                    }
                  }}
                >
                  <FilterCheckbox
                    checked={checked}
                    onToggle={() => onToggle(item.id)}
                    icon={item.icon}
                    label={item.label}
                    meta={item.meta}
                  />
                </li>
              );
            })}
          </ul>
        </Dialog>
      </Popover>
    </DialogTrigger>
  );
}

function FilterCheckbox({
  checked,
  onToggle,
  icon,
  label,
  meta,
}: {
  checked: boolean;
  onToggle: () => void;
  icon?: React.ReactNode;
  label: React.ReactNode;
  meta?: React.ReactNode;
}) {
  return (
    <AriaCheckbox
      className="ui-checkbox ui-filter-checkbox"
      isSelected={checked}
      onChange={onToggle}
    >
      <span className="ui-checkbox-box" aria-hidden="true">
        <Check size={11} weight="bold" className="ui-checkbox-icon" />
      </span>
      {icon && (
        <span className="ui-filter-item-icon" aria-hidden="true">
          {icon}
        </span>
      )}
      <span className="ui-filter-item-label">{label}</span>
      {meta && (
        <span className="ui-filter-item-meta">{meta}</span>
      )}
    </AriaCheckbox>
  );
}
