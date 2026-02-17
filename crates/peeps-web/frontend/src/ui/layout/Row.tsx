import type React from "react";

export function Row(props: React.HTMLAttributes<HTMLDivElement>) {
  return <div {...props} className={["ui-row", props.className].filter(Boolean).join(" ")} />;
}

