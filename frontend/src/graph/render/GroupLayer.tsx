import React from "react";
import type { GeometryGroup } from "../geometry";
import "../../components/graph/ScopeGroupNode.css";
import { scopeKindIcon } from "../../scopeKindSpec";
import { ProcessIdenticon } from "../../ui/primitives/ProcessIdenticon";

interface GroupLayerProps {
  groups: GeometryGroup[];
}

export function GroupLayer({ groups }: GroupLayerProps) {
  if (groups.length === 0) return null;

  return (
    <>
      {groups.map((group) => {
        const { x, y, width, height } = group.worldRect;
        const scopeRgbLight = group.data?.scopeRgbLight as string | undefined;
        const scopeRgbDark = group.data?.scopeRgbDark as string | undefined;
        const scopeKey = group.data?.scopeKey as string | undefined;
        const isProcessGroup = group.scopeKind === "process";

        return (
          <foreignObject
            key={group.id}
            x={x}
            y={y}
            width={width}
            height={height}
            style={{ pointerEvents: "none" }}
          >
            {/* xmlns required for HTML content inside SVG foreignObject */}
            <div
              // @ts-expect-error xmlns is valid in SVG foreignObject context
              xmlns="http://www.w3.org/1999/xhtml"
              className="scope-group"
              style={
                scopeRgbLight !== undefined && scopeRgbDark !== undefined
                  ? ({
                      "--scope-rgb-light": scopeRgbLight,
                      "--scope-rgb-dark": scopeRgbDark,
                    } as React.CSSProperties)
                  : undefined
              }
              >
              <div className="scope-group-header">
                <span className="scope-group-label">
                  <span className="scope-group-icon">
                    {isProcessGroup
                      ? <ProcessIdenticon name={group.label} seed={scopeKey ?? group.label} size={12} />
                      : scopeKindIcon(group.scopeKind, 12)}
                  </span>
                  <span>{group.label}</span>
                </span>
              </div>
            </div>
          </foreignObject>
        );
      })}
    </>
  );
}
