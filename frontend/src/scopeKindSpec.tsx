import type { ComponentType, ReactNode } from "react";
import {
  ArrowsLeftRight,
  BracketsCurly,
  LinkSimple,
  StackSimple,
  Terminal,
} from "@phosphor-icons/react";

function iconFactory(Icon: ComponentType<any>): (size: number) => ReactNode {
  return (size: number) => <Icon size={size} weight="bold" />;
}

type ScopeKindSpec = {
  displayName: string;
  icon: (size: number) => ReactNode;
};

const CANONICAL_SCOPE_KIND_ALIASES: Record<string, string> = {
  proc: "process",
  process_scope: "process",
  thread_scope: "thread",
  tokio_task: "task",
  conn: "connection",
  rpc_connection: "connection",
};

const SCOPE_KIND_SPECS: Record<string, ScopeKindSpec> = {
  process: {
    displayName: "process",
    icon: iconFactory(Terminal),
  },
  thread: {
    displayName: "thread",
    icon: iconFactory(ArrowsLeftRight),
  },
  task: {
    displayName: "task",
    icon: iconFactory(BracketsCurly),
  },
  connection: {
    displayName: "connection",
    icon: iconFactory(LinkSimple),
  },
  unknown: {
    displayName: "unknown",
    icon: iconFactory(StackSimple),
  },
};

export function canonicalScopeKind(kind: unknown): string {
  let normalized = "";
  if (typeof kind === "string") {
    normalized = kind.trim().toLowerCase();
  } else if (kind && typeof kind === "object") {
    const keys = Object.keys(kind);
    normalized = (keys[0] ?? "unknown").trim().toLowerCase();
  } else {
    normalized = "unknown";
  }
  return CANONICAL_SCOPE_KIND_ALIASES[normalized] ?? normalized;
}

export function scopeKindDisplayName(kind: unknown): string {
  const canonical = canonicalScopeKind(kind);
  return SCOPE_KIND_SPECS[canonical]?.displayName ?? canonical;
}

export function scopeKindIcon(kind: unknown, size = 14): ReactNode {
  const canonical = canonicalScopeKind(kind);
  const spec = SCOPE_KIND_SPECS[canonical] ?? SCOPE_KIND_SPECS.unknown;
  return spec.icon(size);
}
