import type React from "react";

export type KeyValueRowProps = {
  label: React.ReactNode;
  children: React.ReactNode;
  labelWidth?: number;
};

export function KeyValueRow({
  label,
  children,
  labelWidth = 80,
}: KeyValueRowProps) {
  const labelStyle: React.CSSProperties = { width: `${labelWidth}px` };
  return (
    <div className="ui-key-value-row">
      <span className="ui-key-value-row__label" style={labelStyle}>{label}</span>
      <span className="ui-key-value-row__value">{children}</span>
    </div>
  );
}
