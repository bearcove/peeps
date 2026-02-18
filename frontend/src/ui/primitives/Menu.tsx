import type React from "react";
import { useEffect, useId, useRef, useState } from "react";
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
  const suppressTriggerCloseRef = useRef(false);
  const instanceId = useId();

  const announceOpen = () => {
    window.dispatchEvent(
      new CustomEvent("ui-control-menu-open", {
        detail: { id: instanceId },
      }),
    );
  };

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
    <MenuTrigger
      isOpen={open}
      onOpenChange={(nextOpen) => {
        if (!nextOpen && suppressTriggerCloseRef.current) return;
        setOpen(nextOpen);
      }}
    >
      <Button
        className="ui-action-button ui-menu-trigger"
        onPointerDown={(event) => {
          if (event.button !== 0) return;
          suppressTriggerCloseRef.current = true;
          setOpen(true);
          announceOpen();
        }}
        onPointerEnter={(event) => {
          if ((event.buttons & 1) !== 1) return;
          suppressTriggerCloseRef.current = true;
          setOpen(true);
          announceOpen();
        }}
      >
        {label}
      </Button>
      <Popover className="ui-menu-popover" placement="bottom start" offset={0} isNonModal>
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
            >
              {item.label}
            </MenuItem>
          )}
        </AriaMenu>
      </Popover>
    </MenuTrigger>
  );
}
