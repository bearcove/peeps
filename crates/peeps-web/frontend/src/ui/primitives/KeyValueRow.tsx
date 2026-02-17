import type React from "react";
import { DotOutline } from "@phosphor-icons/react";

export type KeyValueRowProps = {
  label: React.ReactNode;
  icon?: React.ReactNode;
  children: React.ReactNode;
  labelWidth?: number;
  className?: string;
};

export function KeyValueRow({
  label,
  icon,
  children,
  labelWidth = 80,
  className,
}: KeyValueRowProps) {
  const labelStyle: React.CSSProperties = { width: `${labelWidth}px` };
  return (
    <div className={["ui-key-value-row", className].filter(Boolean).join(" ")}>
      <span className="ui-key-value-row__label" style={labelStyle}>
        <span className="ui-key-value-row__icon" aria-hidden="true">
          {icon ?? <DotOutline size={12} weight="fill" />}
        </span>
        <span>{label}</span>
      </span>
      <span className="ui-key-value-row__value">{children}</span>
    </div>
  );
}
