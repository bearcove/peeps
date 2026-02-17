import type React from "react";

export function PanelHeader({
  title,
  hint,
  right,
}: {
  title: React.ReactNode;
  hint?: React.ReactNode;
  right?: React.ReactNode;
}) {
  return (
    <div className="panel-header">
      {title}
      {hint && <span className="ui-header-hint">{hint}</span>}
      {right}
    </div>
  );
}

