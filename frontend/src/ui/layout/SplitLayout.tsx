import { useCallback, useEffect, useState, type ReactNode } from "react";

export type SplitLayoutProps = {
  left: ReactNode;
  right: ReactNode;
  rightWidth: number;
  onRightWidthChange: (width: number) => void;
  rightMinWidth?: number;
  rightMaxWidth?: number;
  rightCollapsed?: boolean;
  className?: string;
};

const DEFAULT_MIN = 200;
const DEFAULT_MAX = 720;

function clampWidth(
  width: number,
  min: number,
  max: number,
): number {
  const viewportMax = Math.max(min, window.innerWidth - 260);
  return Math.min(Math.min(max, viewportMax), Math.max(min, width));
}

export function SplitLayout({
  left,
  right,
  rightWidth,
  onRightWidthChange,
  rightMinWidth = DEFAULT_MIN,
  rightMaxWidth = DEFAULT_MAX,
  rightCollapsed = false,
  className,
}: SplitLayoutProps) {
  const [dragging, setDragging] = useState(false);

  useEffect(() => {
    const onResize = () => {
      onRightWidthChange(clampWidth(rightWidth, rightMinWidth, rightMaxWidth));
    };
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, [rightWidth, rightMinWidth, rightMaxWidth, onRightWidthChange]);

  const onMouseDown = useCallback(
    (event: React.MouseEvent<HTMLDivElement>) => {
      event.preventDefault();
      const startX = event.clientX;
      const startWidth = rightWidth;
      setDragging(true);

      const onMouseMove = (e: MouseEvent) => {
        const delta = startX - e.clientX;
        onRightWidthChange(
          clampWidth(startWidth + delta, rightMinWidth, rightMaxWidth),
        );
      };

      const onMouseUp = () => {
        document.body.style.cursor = "";
        document.body.style.userSelect = "";
        setDragging(false);
        window.removeEventListener("mousemove", onMouseMove);
        window.removeEventListener("mouseup", onMouseUp);
      };

      document.body.style.cursor = "col-resize";
      document.body.style.userSelect = "none";
      window.addEventListener("mousemove", onMouseMove);
      window.addEventListener("mouseup", onMouseUp);
    },
    [rightWidth, rightMinWidth, rightMaxWidth, onRightWidthChange],
  );

  const onKeyDown = useCallback(
    (event: React.KeyboardEvent<HTMLDivElement>) => {
      if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
      event.preventDefault();
      const delta = event.key === "ArrowLeft" ? 16 : -16;
      onRightWidthChange(
        clampWidth(rightWidth + delta, rightMinWidth, rightMaxWidth),
      );
    },
    [rightWidth, rightMinWidth, rightMaxWidth, onRightWidthChange],
  );

  const resizerWidth = rightCollapsed ? 0 : 10;
  const colWidth = rightCollapsed ? 32 : rightWidth;

  return (
    <div
      className={[
        "ui-split-layout",
        rightCollapsed && "ui-split-layout--right-collapsed",
        dragging && "ui-split-layout--dragging",
        className,
      ]
        .filter(Boolean)
        .join(" ")}
      style={{
        ["--split-right-width" as string]: `${colWidth}px`,
        ["--split-resizer-width" as string]: `${resizerWidth}px`,
      }}
    >
      <div className="ui-split-left">{left}</div>
      {!rightCollapsed && (
        <div
          className="ui-split-resizer"
          role="separator"
          aria-label="Resize panel"
          aria-orientation="vertical"
          aria-valuemin={rightMinWidth}
          aria-valuemax={rightMaxWidth}
          aria-valuenow={Math.round(rightWidth)}
          tabIndex={0}
          onMouseDown={onMouseDown}
          onKeyDown={onKeyDown}
        />
      )}
      <div className="ui-split-right">{right}</div>
    </div>
  );
}
