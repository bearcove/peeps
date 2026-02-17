import type React from "react";

export function Section({
  title,
  subtitle,
  children,
}: {
  title: React.ReactNode;
  subtitle?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <section className="ui-section">
      <div className="ui-section-head">
        <span>{title}</span>
        {subtitle && <span className="ui-section-subhead">{subtitle}</span>}
      </div>
      {children}
    </section>
  );
}

