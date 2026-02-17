import type React from "react";

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
    <div
      className={["ui-segmented-group", size === "sm" && "ui-segmented-group--sm"].filter(Boolean).join(" ")}
      role="radiogroup"
      aria-label={ariaLabel}
      onKeyDown={(event) => {
        if (event.defaultPrevented) return;
        const currentIndex = options.findIndex((option) => option.value === value);
        if (currentIndex < 0) return;

        if (event.key === "ArrowRight" || event.key === "ArrowDown") {
          const nextIndex = (currentIndex + 1) % options.length;
          const next = options[nextIndex];
          onChange(next.value);
          const nextButton = event.currentTarget.querySelector(`[data-index="${nextIndex}"]`) as HTMLButtonElement | null;
          nextButton?.focus();
          event.preventDefault();
          return;
        }

        if (event.key === "ArrowLeft" || event.key === "ArrowUp") {
          const prevIndex = (currentIndex - 1 + options.length) % options.length;
          const prev = options[prevIndex];
          onChange(prev.value);
          const prevButton = event.currentTarget.querySelector(`[data-index="${prevIndex}"]`) as HTMLButtonElement | null;
          prevButton?.focus();
          event.preventDefault();
        }
      }}
    >
      {options.map((option, index) => {
        const isSelected = option.value === value;
        return (
          <button
            type="button"
            role="radio"
            aria-checked={isSelected}
            key={option.value}
            data-index={index}
            tabIndex={isSelected ? 0 : -1}
            className={[
              "ui-segmented-option",
              isSelected && "ui-segmented-option--selected",
            ].filter(Boolean).join(" ")}
            onClick={() => {
              if (!isSelected) onChange(option.value);
            }}
            onKeyDown={(event) => {
              if (event.key === " " || event.key === "Enter") {
                event.preventDefault();
                if (!isSelected) onChange(option.value);
              }
            }}
          >
            {option.label}
          </button>
        );
      })}
    </div>
  );
}
