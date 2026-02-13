import type { ComponentChildren } from "preact";
import { navigateTo } from "../routes";
import { classNames } from "../util";

interface Props {
  href: string;
  active?: boolean;
  children: ComponentChildren;
  title?: string;
}

export function ResourceLink({ href, active, children, title }: Props) {
  return (
    <a
      href={href}
      title={title ?? href}
      class={classNames("resource-link", active && "active")}
      onClick={(e) => {
        if (e.defaultPrevented) return;
        if (e.button !== 0) return;
        if (e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
        e.preventDefault();
        navigateTo(href);
      }}
    >
      <span>{children}</span>
      <span class="resource-link-icon" aria-hidden="true">↗</span>
    </a>
  );
}
