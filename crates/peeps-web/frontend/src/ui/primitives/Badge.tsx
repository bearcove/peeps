import type React from "react";

export type BadgeTone = "neutral" | "ok" | "warn" | "crit";
export type BadgeVariant = "standard" | "count";

export function Badge({
  tone = "neutral",
  variant = "standard",
  className,
  ...props
}: React.HTMLAttributes<HTMLSpanElement> & { tone?: BadgeTone; variant?: BadgeVariant }) {
  return (
    <span
      {...props}
      className={[
        "badge",
        `badge--${tone}`,
        variant === "count" && "badge--count",
        className,
      ].filter(Boolean).join(" ")}
    />
  );
}
