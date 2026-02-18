import React from "react";
import type { GeometryGroup } from "../geometry";
import "../../components/graph/ScopeGroupNode.css";
import { scopeKindIcon } from "../../scopeKindSpec";

interface GroupLayerProps {
  groups: GeometryGroup[];
}

export function GroupLayer({ groups }: GroupLayerProps) {
  if (groups.length === 0) return null;

  return (
    <>
      {groups.map((group) => {
        const { x, y, width, height } = group.worldRect;
        const count = (group.data?.count as number | undefined) ?? group.members.length;
        const scopeHue = group.data?.scopeHue as number | undefined;

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
                scopeHue !== undefined
                  ? ({ "--scope-h": String(scopeHue) } as React.CSSProperties)
                  : undefined
              }
              >
              <div className="scope-group-header">
                <span className="scope-group-label">
                  <span className="scope-group-icon">{scopeKindIcon(group.scopeKind, 12)}</span>
                  <span>{group.label}</span>
                </span>
                <span className="scope-group-meta">{count}</span>
              </div>
            </div>
          </foreignObject>
        );
      })}
    </>
  );
}
