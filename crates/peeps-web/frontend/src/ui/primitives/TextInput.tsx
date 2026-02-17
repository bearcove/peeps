import type React from "react";
import { Input, TextField } from "react-aria-components";

export function TextInput({
  value,
  onChange,
  placeholder,
  className,
  "aria-label": ariaLabel,
}: {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  className?: string;
  "aria-label"?: string;
}) {
  return (
    <TextField className="ui-text-field" aria-label={ariaLabel}>
      <Input
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        className={["ui-input", className].filter(Boolean).join(" ")}
      />
    </TextField>
  );
}
