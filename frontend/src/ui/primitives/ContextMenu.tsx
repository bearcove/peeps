import type React from "react";
import { useEffect, useRef } from "react";
import "./ContextMenu.css";

export function ContextMenu({
  x,
  y,
  onClose,
  children,
}: {
  x: number;
  y: number;
  onClose: () => void;
  children: React.ReactNode;
}) {
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node | null;
      if (target && menuRef.current?.contains(target)) return;
      onClose();
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    const onResize = () => onClose();
    window.addEventListener("pointerdown", onPointerDown, true);
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("resize", onResize);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown, true);
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("resize", onResize);
    };
  }, [onClose]);

  return (
    <div ref={menuRef} className="ui-context-menu" style={{ left: x, top: y }}>
      {children}
    </div>
  );
}

export function ContextMenuItem({
  prefix,
  onClick,
  disabled,
  danger,
  children,
}: {
  prefix?: React.ReactNode;
  onClick?: () => void;
  disabled?: boolean;
  danger?: boolean;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      className={["ui-context-menu-item", danger && "ui-context-menu-item--danger"]
        .filter(Boolean)
        .join(" ")}
      onClick={onClick}
      disabled={disabled}
    >
      {prefix != null && (
        <span className="ui-context-menu-item__prefix">{prefix}</span>
      )}
      {children}
    </button>
  );
}

export function ContextMenuSeparator() {
  return <div className="ui-context-menu-separator" />;
}
