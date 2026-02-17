import type React from "react";

export type ButtonVariant = "default" | "primary";

export function Button({
  variant = "default",
  className,
  ...props
}: React.ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: ButtonVariant;
}) {
  return (
    <button
      {...props}
      className={[
        "btn",
        variant === "primary" && "btn--primary",
        className,
      ].filter(Boolean).join(" ")}
    />
  );
}

