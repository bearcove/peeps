import type React from "react";
import "./KeyValueRow.css";

export type KeyValueRowProps = {
  label: React.ReactNode;
  children: React.ReactNode;
  className?: string;
  /** Optional icon shown to the left of the label text. */
  icon?: React.ReactNode;
  /** Fix the label column to this width in pixels. */
  labelWidth?: number;
};

export function KeyValueRow({ label, children, className, icon, labelWidth }: KeyValueRowProps) {
  return (
    <div className={["ui-key-value-row", className].filter(Boolean).join(" ")}>
      <span
        className="ui-key-value-row__label"
        style={labelWidth !== undefined ? { width: labelWidth } : undefined}
      >
        {icon ? <span className="ui-key-value-row__label-icon">{icon}</span> : null}
        {label}
      </span>
      <span className="ui-key-value-row__value">{children}</span>
    </div>
  );
}
