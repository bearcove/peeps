import type React from "react";
import { kindIcon } from "../../nodeKindSpec";

export type NodeChipProps = {
  label: React.ReactNode;
  kind?: string;
  onClick?: () => void;
  onContextMenu?: (event: React.MouseEvent<HTMLElement>) => void;
  className?: string;
  href?: string;
  target?: React.AnchorHTMLAttributes<HTMLAnchorElement>["target"];
  rel?: React.AnchorHTMLAttributes<HTMLAnchorElement>["rel"];
  icon?: React.ReactNode;
  title?: string;
};

export function NodeChip({
  label,
  kind,
  onClick,
  onContextMenu,
  className,
  href,
  target,
  rel,
  icon,
  title,
}: NodeChipProps) {
  const chipClassName = ["ui-node-chip", className].filter(Boolean).join(" ");
  const iconNode = icon ?? (kind ? kindIcon(kind, 12) : null);

  if (href != null) {
    return (
      <a
        href={href}
        target={target}
        rel={rel}
        title={title}
        className={chipClassName}
        onContextMenu={onContextMenu}
        onClick={onClick}
      >
        {iconNode && (
          <span className="ui-node-chip__icon" aria-hidden="true">
            {iconNode}
          </span>
        )}
        <span className="ui-node-chip__label">{label}</span>
      </a>
    );
  }

  return (
    <button
      type="button"
      className={chipClassName}
      onClick={onClick}
      onContextMenu={onContextMenu}
    >
      {iconNode && (
        <span className="ui-node-chip__icon" aria-hidden="true">
          {iconNode}
        </span>
      )}
      <span className="ui-node-chip__label">{label}</span>
    </button>
  );
}
