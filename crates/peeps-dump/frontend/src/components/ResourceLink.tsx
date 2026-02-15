import type { ComponentChildren } from "preact";
import { navigateTo } from "../routes";
import type { ResourceRef } from "../routes";
import { classNames } from "../util";

interface Props {
  href: string;
  active?: boolean;
  children: ComponentChildren;
  title?: string;
  kind: ResourceRef["kind"] | "view";
}

function ResourceIcon({ kind }: { kind: Props["kind"] }) {
  const common = { width: 12, height: 12, viewBox: "0 0 16 16", fill: "none", stroke: "currentColor", strokeWidth: 1.6, strokeLinecap: "round" as const, strokeLinejoin: "round" as const };
  switch (kind) {
    case "process":
      return <svg {...common}><rect x="2.5" y="3" width="11" height="10" rx="1.5" /><path d="M5 6h6M5 9h6" /></svg>;
    case "thread":
      return <svg {...common}><circle cx="8" cy="8" r="4.5" /><path d="M8 2.5v2M8 11.5v2M2.5 8h2M11.5 8h2" /></svg>;
    case "task":
      return <svg {...common}><path d="M3.5 8l2.5 2.5 6-6" /></svg>;
    case "lock":
      return <svg {...common}><rect x="4" y="7" width="8" height="6" rx="1.2" /><path d="M5.5 7V5.8A2.5 2.5 0 0 1 8 3.3a2.5 2.5 0 0 1 2.5 2.5V7" /></svg>;
    case "mpsc":
    case "oneshot":
    case "watch":
    case "once_cell":
      return <svg {...common}><path d="M2.5 8h11M9.5 5l4 3-4 3" /></svg>;
    case "future_wait":
      return <svg {...common}><circle cx="8" cy="8" r="5" /><path d="M8 5.2v3l2 1.4" /></svg>;
    case "connection":
      return <svg {...common}><path d="M5.8 9.8l-1.6 1.6a2 2 0 1 1-2.8-2.8l1.9-1.9a2 2 0 0 1 2.8 0" /><path d="M10.2 6.2l1.6-1.6a2 2 0 1 1 2.8 2.8l-1.9 1.9a2 2 0 0 1-2.8 0" /></svg>;
    case "request":
      return <svg {...common}><path d="M2.5 8h8" /><path d="M8 4.5L12 8l-4 3.5" /></svg>;
    case "shm_segment":
      return <svg {...common}><rect x="2.5" y="4" width="11" height="8" rx="1.2" /><path d="M5 6.5h6M5 9.5h4" /></svg>;
    case "shm_peer":
      return <svg {...common}><circle cx="8" cy="6" r="2.2" /><path d="M3.5 12c.9-1.8 2.4-2.7 4.5-2.7s3.6.9 4.5 2.7" /></svg>;
    case "view":
      return <svg {...common}><path d="M2.5 8s2-3.5 5.5-3.5S13.5 8 13.5 8 11.5 11.5 8 11.5 2.5 8 2.5 8z" /><circle cx="8" cy="8" r="1.5" /></svg>;
  }
}

export function ResourceLink({ href, active, children, title, kind }: Props) {
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
      <span class="resource-link-icon" aria-hidden="true"><ResourceIcon kind={kind} /></span>
      <span class="resource-link-label">{children}</span>
    </a>
  );
}
