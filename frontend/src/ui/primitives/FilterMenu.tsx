import type React from "react";
import { useEffect, useId, useRef, useState } from "react";
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
  const [dragSelectActive, setDragSelectActive] = useState(false);
  const suppressTriggerCloseRef = useRef(false);
  const instanceId = useId();
  const hiddenCount = items.filter((item) => hiddenIds.has(item.id)).length;

  const announceOpen = () => {
    window.dispatchEvent(
      new CustomEvent("ui-control-menu-open", {
        detail: { id: instanceId },
      }),
    );
  };

  useEffect(() => {
    if (!dragSelectActive) return;
    const stop = () => setDragSelectActive(false);
    window.addEventListener("pointerup", stop);
    window.addEventListener("pointercancel", stop);
    return () => {
      window.removeEventListener("pointerup", stop);
      window.removeEventListener("pointercancel", stop);
    };
  }, [dragSelectActive]);

  useEffect(() => {
    const clearSuppression = () => {
      setTimeout(() => {
        suppressTriggerCloseRef.current = false;
      }, 0);
    };
    window.addEventListener("pointerup", clearSuppression);
    window.addEventListener("pointercancel", clearSuppression);
    return () => {
      window.removeEventListener("pointerup", clearSuppression);
      window.removeEventListener("pointercancel", clearSuppression);
    };
  }, []);

  useEffect(() => {
    const onOtherMenuOpened = (event: Event) => {
      const detail = (event as CustomEvent<{ id?: string }>).detail;
      if (!detail || detail.id === instanceId) return;
      setOpen(false);
      setDragSelectActive(false);
    };
    window.addEventListener("ui-control-menu-open", onOtherMenuOpened);
    return () => {
      window.removeEventListener("ui-control-menu-open", onOtherMenuOpened);
    };
  }, [instanceId]);

  useEffect(() => {
    if (!open) return;
    announceOpen();
  }, [open]);

  return (
    <DialogTrigger
      isOpen={open}
      onOpenChange={(nextOpen) => {
        if (!nextOpen && suppressTriggerCloseRef.current) return;
        setOpen(nextOpen);
      }}
    >
      <Button
        onPointerDown={(event) => {
          if (event.button !== 0) return;
          suppressTriggerCloseRef.current = true;
          setOpen(true);
          announceOpen();
          setDragSelectActive(true);
        }}
        onPointerEnter={(event) => {
          if ((event.buttons & 1) !== 1) return;
          suppressTriggerCloseRef.current = true;
          setOpen(true);
          announceOpen();
          setDragSelectActive(true);
        }}
        className={[
          "ui-action-button",
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
        isNonModal
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
                  onPointerUpCapture={(e) => {
                    if (!dragSelectActive) return;
                    e.preventDefault();
                    e.stopPropagation();
                    setDragSelectActive(false);
                    if (e.altKey) onSolo(item.id);
                    else onToggle(item.id);
                  }}
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
  const showMeta = typeof meta === "number";

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
      {showMeta && (
        <span className="ui-filter-item-meta">{meta}</span>
      )}
    </AriaCheckbox>
  );
}
