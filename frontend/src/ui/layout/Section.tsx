import type React from "react";
import { useState } from "react";
import "./Section.css";

export function Section({
  title,
  subtitle,
  wide,
  collapsible,
  defaultCollapsed,
  children,
}: {
  title: React.ReactNode;
  subtitle?: React.ReactNode;
  wide?: boolean;
  collapsible?: boolean;
  defaultCollapsed?: boolean;
  children: React.ReactNode;
}) {
  const [collapsed, setCollapsed] = useState(defaultCollapsed ?? false);

  return (
    <section className={["ui-section", wide && "ui-section--wide"].filter(Boolean).join(" ")}>
      <div
        className={["ui-section-head", collapsible && "ui-section-head--collapsible"].filter(Boolean).join(" ")}
        onClick={collapsible ? () => setCollapsed((c) => !c) : undefined}
      >
        {collapsible && (
          <span className={["ui-section-chevron", collapsed && "ui-section-chevron--collapsed"].filter(Boolean).join(" ")} aria-hidden="true">›</span>
        )}
        <span>{title}</span>
        {subtitle && <span className="ui-section-subhead">{subtitle}</span>}
      </div>
      {!collapsed && (
        <div className="ui-section-body">
          {children}
        </div>
      )}
    </section>
  );
}

