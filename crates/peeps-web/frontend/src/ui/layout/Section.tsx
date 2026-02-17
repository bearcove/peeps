import type React from "react";

export function Section({
  title,
  subtitle,
  wide,
  children,
}: {
  title: React.ReactNode;
  subtitle?: React.ReactNode;
  wide?: boolean;
  children: React.ReactNode;
}) {
  return (
    <section className={["ui-section", wide && "ui-section--wide"].filter(Boolean).join(" ")}>
      <div className="ui-section-head">
        <span>{title}</span>
        {subtitle && <span className="ui-section-subhead">{subtitle}</span>}
      </div>
      {children}
    </section>
  );
}

