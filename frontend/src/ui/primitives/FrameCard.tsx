import React from "react";
import "./FrameCard.css";
import { ScopeColorPair } from "../../components/graph/scopeColors";

export function FrameCard({ color, children }: { color?: ScopeColorPair; children: React.ReactNode }) {
  const style = color
    ? { "--frame-color": `light-dark(rgb(${color.light}), rgb(${color.dark}))` } as React.CSSProperties
    : undefined;
  return <div className="ui-frame-card" style={style}>{children}</div>;
}
