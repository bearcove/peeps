import { Children, type ReactNode } from "react";
import { Button, type ButtonProps } from "react-aria-components";

export type ActionButtonVariant = "default" | "primary" | "ghost";
export type ActionButtonSize = "sm" | "md";

export type ActionButtonProps = Omit<ButtonProps, "children"> & {
  variant?: ActionButtonVariant;
  size?: ActionButtonSize;
  children?: ReactNode;
};

export function ActionButton({
  variant = "default",
  size = "md",
  className,
  children,
  ...props
}: ActionButtonProps) {
  const childrenArray = Children.toArray(children);
  const hasTextChild = childrenArray.some(
    (child) => typeof child === "string" || typeof child === "number",
  );
  const isIconOnly = childrenArray.length === 1 && !hasTextChild;

  return (
    <Button
      {...props}
      className={[
        "ui-action-button",
        variant === "primary" && "ui-action-button--primary",
        variant === "ghost" && "ui-action-button--ghost",
        size === "sm" && "ui-action-button--sm",
        isIconOnly && "ui-action-button--icon-only",
        className,
      ].filter(Boolean).join(" ")}
    >
      {children}
    </Button>
  );
}
