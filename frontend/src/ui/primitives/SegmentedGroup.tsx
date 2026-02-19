import type React from "react";
import { RadioGroup, Radio } from "react-aria-components";
import "./SegmentedGroup.css";

export type SegmentedOption = {
  value: string;
  label: React.ReactNode;
};

export type SegmentedGroupProps = {
  value: string;
  onChange: (value: string) => void;
  options: readonly SegmentedOption[];
  "aria-label"?: string;
};

export function SegmentedGroup({
  value,
  onChange,
  options,
  "aria-label": ariaLabel,
}: SegmentedGroupProps) {
  return (
    <RadioGroup
      className="ui-segmented-group"
      aria-label={ariaLabel}
      value={value}
      onChange={onChange}
    >
      {options.map((option) => (
        <Radio
          key={option.value}
          value={option.value}
          className={({ isSelected }) =>
            ["ui-segmented-option", isSelected && "ui-segmented-option--selected"].filter(Boolean).join(" ")
          }
        >
          {option.label}
        </Radio>
      ))}
    </RadioGroup>
  );
}
