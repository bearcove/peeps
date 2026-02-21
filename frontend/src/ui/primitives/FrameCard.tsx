import React from "react";
import "./FrameCard.css";
import { ScopeColorPair } from "../../components/graph/scopeColors";

export function FrameCard({ color, children }: { color?: ScopeColorPair; children: React.ReactNode }) {
  return <div className="ui-frame-card" style={{ borderLeftColor: color ? `light-dark(rgb(${color.light}), rgb(${color.dark}))` : undefined }}>{children}</div>;
}
