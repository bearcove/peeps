import React from "react";
import { createPortal } from "react-dom";

type PopoverSide = "top" | "bottom";
type PopoverAlign = "start" | "end";

export function Popover({
  open,
  anchorRef,
  onClose,
  side = "bottom",
  align = "start",
  offset = 6,
  className,
  children,
}: {
  open: boolean;
  anchorRef: React.RefObject<HTMLElement | null>;
  onClose?: () => void;
  side?: PopoverSide;
  align?: PopoverAlign;
  offset?: number;
  className?: string;
  children: React.ReactNode;
}) {
  const popoverRef = React.useRef<HTMLDivElement>(null);
  const [style, setStyle] = React.useState<React.CSSProperties>({
    position: "fixed",
    left: 0,
    top: 0,
    visibility: "hidden",
  });

  const updatePosition = React.useCallback(() => {
    const anchor = anchorRef.current;
    const popover = popoverRef.current;
    if (!anchor || !popover) return;

    const margin = 8;
    const anchorRect = anchor.getBoundingClientRect();
    const popoverRect = popover.getBoundingClientRect();
    const viewportWidth = window.innerWidth;
    const viewportHeight = window.innerHeight;

    const bottomY = anchorRect.bottom + offset;
    const topY = anchorRect.top - popoverRect.height - offset;
    const canPlaceBottom = bottomY + popoverRect.height <= viewportHeight - margin;
    const canPlaceTop = topY >= margin;

    let effectiveSide: PopoverSide = side;
    if (side === "bottom" && !canPlaceBottom && canPlaceTop) effectiveSide = "top";
    if (side === "top" && !canPlaceTop && canPlaceBottom) effectiveSide = "bottom";

    let x = align === "end"
      ? anchorRect.right - popoverRect.width
      : anchorRect.left;
    x = Math.min(Math.max(x, margin), Math.max(margin, viewportWidth - popoverRect.width - margin));

    let y = effectiveSide === "top" ? topY : bottomY;
    y = Math.min(Math.max(y, margin), Math.max(margin, viewportHeight - popoverRect.height - margin));

    setStyle({
      position: "fixed",
      left: Math.round(x),
      top: Math.round(y),
      visibility: "visible",
    });
  }, [align, anchorRef, offset, side]);

  React.useLayoutEffect(() => {
    if (!open) return;
    updatePosition();

    const onScrollOrResize = () => {
      updatePosition();
    };

    window.addEventListener("resize", onScrollOrResize);
    window.addEventListener("scroll", onScrollOrResize, true);

    const anchor = anchorRef.current;
    const popover = popoverRef.current;
    const resizeObserver = new ResizeObserver(() => updatePosition());
    if (anchor) resizeObserver.observe(anchor);
    if (popover) resizeObserver.observe(popover);

    return () => {
      window.removeEventListener("resize", onScrollOrResize);
      window.removeEventListener("scroll", onScrollOrResize, true);
      resizeObserver.disconnect();
    };
  }, [open, updatePosition, anchorRef]);

  React.useEffect(() => {
    if (!open || !onClose) return;

    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node | null;
      if (!target) return;
      const anchor = anchorRef.current;
      const popover = popoverRef.current;
      if (anchor?.contains(target) || popover?.contains(target)) return;
      onClose();
    };

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };

    window.addEventListener("pointerdown", onPointerDown, true);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown, true);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [open, onClose, anchorRef]);

  if (!open) return null;

  return createPortal(
    <div ref={popoverRef} className={className} style={style}>
      {children}
    </div>,
    document.body,
  );
}
