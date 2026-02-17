import type React from "react";

export type ActionButtonVariant = "default" | "ghost";
export type ActionButtonSize = "sm" | "md";

export type ActionButtonProps = React.ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: ActionButtonVariant;
  size?: ActionButtonSize;
};

export function ActionButton({
  variant = "default",
  size = "md",
  className,
  ...props
}: ActionButtonProps) {
  return (
    <button
      {...props}
      type={props.type ?? "button"}
      className={[
        "ui-action-button",
        variant === "ghost" && "ui-action-button--ghost",
        size === "sm" && "ui-action-button--sm",
        className,
      ].filter(Boolean).join(" ")}
    />
  );
}
