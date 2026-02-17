import type React from "react";
import {
  Button,
  ListBox,
  ListBoxItem,
  Popover,
  Select as AriaSelect,
  SelectValue,
} from "react-aria-components";
import { CaretDown } from "@phosphor-icons/react";

export type SelectOption = {
  value: string;
  label: React.ReactNode;
};

export function Select({
  value,
  onChange,
  options,
  className,
  "aria-label": ariaLabel,
}: {
  value: string;
  onChange: (value: string) => void;
  options: readonly SelectOption[];
  className?: string;
  "aria-label"?: string;
}) {
  return (
    <AriaSelect
      className={["ui-select", className].filter(Boolean).join(" ")}
      aria-label={ariaLabel}
      selectedKey={value}
      onSelectionChange={(key) => {
        if (key != null) onChange(String(key));
      }}
    >
      <Button className="ui-select-trigger">
        <SelectValue />
        <CaretDown size={12} weight="bold" />
      </Button>
      <Popover className="ui-select-popover" placement="bottom start" offset={0}>
        <ListBox className="ui-select-list">
          {options.map((option) => (
            <ListBoxItem id={option.value} key={option.value} className="ui-select-item">
              {option.label}
            </ListBoxItem>
          ))}
        </ListBox>
      </Popover>
    </AriaSelect>
  );
}
