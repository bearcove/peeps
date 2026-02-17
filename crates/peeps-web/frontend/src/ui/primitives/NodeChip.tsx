import type React from "react";

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
      onContextMenu={(event) => {
        onContextMenu?.(event);
      }}
    >
      {kind && (
        <span className="ui-node-chip__kind">
          {kind}
          <span className="ui-node-chip__sep">·</span>
        </span>
      )}
      <span className="ui-node-chip__label">{label}</span>
    </button>
  );
}
