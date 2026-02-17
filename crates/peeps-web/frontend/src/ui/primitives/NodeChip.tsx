import type React from "react";
import { kindIcon } from "../../nodeKindSpec";

export type NodeChipProps = {
  label: React.ReactNode;
  kind?: string;
  onClick?: () => void;
  onContextMenu?: (event: React.MouseEvent<HTMLButtonElement>) => void;
  className?: string;
};

export function NodeChip({
  label,
  kind,
  onClick,
  onContextMenu,
  className,
}: NodeChipProps) {
  return (
    <button
      type="button"
      className={["ui-node-chip", className].filter(Boolean).join(" ")}
      onClick={onClick}
      onContextMenu={onContextMenu}
    >
      {kind && (
        <span className="ui-node-chip__icon" aria-hidden="true">
          {kindIcon(kind, 12)}
        </span>
      )}
      <span className="ui-node-chip__label">{label}</span>
    </button>
  );
}
