import type React from "react";
import { RadioGroup, Radio } from "react-aria-components";

export type SegmentedOption = {
  value: string;
  label: React.ReactNode;
};

export type SegmentedGroupProps = {
  value: string;
  onChange: (value: string) => void;
  options: readonly SegmentedOption[];
  size?: "sm" | "md";
  "aria-label"?: string;
};

export function SegmentedGroup({
  value,
  onChange,
  options,
  size = "md",
  "aria-label": ariaLabel,
}: SegmentedGroupProps) {
  return (
    <RadioGroup
      className={["ui-segmented-group", size === "sm" && "ui-segmented-group--sm"].filter(Boolean).join(" ")}
      aria-label={ariaLabel}
      value={value}
      onChange={onChange}
    >
      {options.map((option) => (
        <Radio
          key={option.value}
          value={option.value}
          className={["ui-segmented-option", option.value === value && "ui-segmented-option--selected"].filter(Boolean).join(" ")}
        >
          {option.label}
        </Radio>
      ))}
    </RadioGroup>
  );
}
