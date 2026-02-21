import type React from "react";
import { useCallback, useEffect, useId, useRef, useState } from "react";
import {
  Button,
  Menu as AriaMenu,
  MenuItem,
  MenuTrigger,
  Popover,
} from "react-aria-components";
import "./Menu.css";

export type MenuOption = {
  id: string;
  label: React.ReactNode;
  danger?: boolean;
};

export function Menu({
  label,
  items,
  onAction,
}: {
  label: React.ReactNode;
  items: readonly MenuOption[];
  onAction?: (id: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const instanceId = useId();
  const triggerRef = useRef<HTMLButtonElement>(null);
  const popoverRef = useRef<HTMLDivElement>(null);
  const suppressTriggerCloseRef = useRef(false);
  const suppressNextOpenRef = useRef(false);
  // True when the menu was opened by pressing (not releasing) the trigger,
  // so that releasing over a menu item triggers the action without a separate click.
  const isDragOpenRef = useRef(false);

  const announceOpen = useCallback(() => {
    window.dispatchEvent(
      new CustomEvent("ui-control-menu-open", {
        detail: { id: instanceId },
      }),
    );
  }, [instanceId]);

  useEffect(() => {
    const onOtherMenuOpened = (event: Event) => {
      const detail = (event as CustomEvent<{ id?: string }>).detail;
      if (!detail || detail.id === instanceId) return;
      setOpen(false);
    };
    window.addEventListener("ui-control-menu-open", onOtherMenuOpened);
    return () => {
      window.removeEventListener("ui-control-menu-open", onOtherMenuOpened);
    };
  }, [instanceId]);

  useEffect(() => {
    if (!open) return;
    announceOpen();
  }, [open, announceOpen]);

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node | null;
      if (!target) return;
      if (triggerRef.current?.contains(target)) return;
      if (popoverRef.current?.contains(target)) return;
      setOpen(false);
    };
    window.addEventListener("pointerdown", onPointerDown, true);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown, true);
    };
  }, [open]);

  useEffect(() => {
    const clearSuppression = () => {
      setTimeout(() => {
        suppressTriggerCloseRef.current = false;
        suppressNextOpenRef.current = false;
        isDragOpenRef.current = false;
      }, 0);
    };
    window.addEventListener("pointerup", clearSuppression);
    window.addEventListener("pointercancel", clearSuppression);
    return () => {
      window.removeEventListener("pointerup", clearSuppression);
      window.removeEventListener("pointercancel", clearSuppression);
    };
  }, []);

  return (
    <MenuTrigger
      isOpen={open}
      onOpenChange={(nextOpen) => {
        if (nextOpen && suppressNextOpenRef.current) {
          suppressNextOpenRef.current = false;
          return;
        }
        if (!nextOpen && suppressTriggerCloseRef.current) return;
        setOpen(nextOpen);
      }}
    >
      <Button
        ref={triggerRef}
        className="ui-action-button ui-menu-trigger"
        onPointerDown={(event) => {
          if (event.button !== 0) return;
          if (open) {
            event.preventDefault();
            suppressTriggerCloseRef.current = false;
            suppressNextOpenRef.current = true;
            isDragOpenRef.current = false;
            setOpen(false);
            return;
          }
          suppressTriggerCloseRef.current = true;
          isDragOpenRef.current = true;
          setOpen(true);
          announceOpen();
        }}
        onPointerEnter={(event) => {
          if ((event.buttons & 1) !== 1) return;
          if (open) return;
          suppressTriggerCloseRef.current = true;
          isDragOpenRef.current = true;
          setOpen(true);
          announceOpen();
        }}
      >
        {label}
      </Button>
      <Popover
        ref={popoverRef}
        className="ui-menu-popover"
        placement="bottom start"
        offset={0}
        isNonModal
        shouldCloseOnInteractOutside={(element) => !triggerRef.current?.contains(element)}
      >
        <AriaMenu
          className="ui-menu-list"
          onAction={(key) => onAction?.(String(key))}
          items={items}
        >
          {(item) => (
            <MenuItem
              id={item.id}
              className={[
                "ui-menu-item",
                item.danger && "ui-menu-item--danger",
              ].filter(Boolean).join(" ")}
              onPointerUp={() => {
                if (!isDragOpenRef.current) return;
                isDragOpenRef.current = false;
                onAction?.(item.id);
                setOpen(false);
              }}
            >
              {item.label}
            </MenuItem>
          )}
        </AriaMenu>
      </Popover>
    </MenuTrigger>
  );
}
