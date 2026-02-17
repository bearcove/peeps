import type React from "react";
import {
  Button,
  Menu as AriaMenu,
  MenuItem,
  MenuTrigger,
  Popover,
} from "react-aria-components";

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
  return (
    <MenuTrigger>
      <Button className="ui-menu-trigger">{label}</Button>
      <Popover className="ui-menu-popover" placement="bottom start" offset={6}>
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
