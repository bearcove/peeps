import React from "react";
import "./FrameCard.css";

export function FrameCard({ children }: { children: React.ReactNode }) {
  return <div className="ui-frame-card">{children}</div>;
}
