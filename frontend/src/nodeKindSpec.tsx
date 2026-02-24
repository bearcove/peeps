import type { ComponentType, ReactNode } from "react";
import {
  ArrowBendDownLeft,
  ArrowLineDown,
  ArrowLineUp,
  ArrowsLeftRight,
  Bell,
  CircleDashed,
  Cloud,
  Cpu,
  Cube,
  Database,
  DownloadSimple,
  Eye,
  FileText,
  Flame,
  Gauge,
  Gear,
  Ghost,
  Globe,
  HourglassSimple,
  Key,
  Lightning,
  LinkSimple,
  ListBullets,
  Lock,
  LockOpen,
  MagnifyingGlass,
  Moon,
  PaperPlaneTilt,
  Plugs,
  Queue,
  Repeat,
  BracketsCurly,
  Pipe,
  Shield,
  Stack,
  Terminal,
  Timer,
  TreeStructure,
  UploadSimple,
  Users,
  WifiHigh,
} from "@phosphor-icons/react";

export type NodeKindCategory =
  | "async"
  | "sync"
  | "channel"
  | "rpc"
  | "net"
  | "fs"
  | "time"
  | "meta";

export type ValueFormat =
  | "string"
  | "number"
  | "bool"
  | "duration_ns"
  | "timestamp_ns"
  | "id"
  | "enum"
  | "json";

export interface ExpectedAttrSpec {
  key: string;
  label?: string;
  format?: ValueFormat;
  importance?: "primary" | "secondary" | "debug";
}

export interface NodeKindSpec {
  /** Canonical kind identifier (used for lookup + aliasing). */
  canonical: string;
  /** Human-readable label for UI. */
  displayName: string;
  category: NodeKindCategory;
  /**
   * Icon used in kind dropdowns / headers.
   * Keep it category-oriented; per-view nuance can layer on top.
   */
  icon: (size: number) => ReactNode;
  /**
   * Declarative spec we can evolve over time:
   * which attrs we expect to exist and how we intend to render them.
   */
  expectedAttrs?: ExpectedAttrSpec[];
}

const ALIASES: Record<string, string> = {
  lock: "mutex",
  once_cell: "oncecell",
  task: "future",
};

export function canonicalNodeKind(kind: string): string {
  return ALIASES[kind] ?? kind;
}

function iconFactory(Icon: ComponentType<any>): (size: number) => ReactNode {
  return (size: number) => <Icon size={size} weight="bold" />;
}

export const NODE_KIND_SPECS: Record<string, NodeKindSpec> = {
  future: {
    canonical: "future",
    displayName: "Future",
    category: "async",
    icon: iconFactory(BracketsCurly),
    expectedAttrs: [
      { key: "state", format: "enum", importance: "primary" },
      { key: "poll_count", format: "number", importance: "secondary" },
      { key: "idle_ns", label: "idle", format: "duration_ns", importance: "secondary" },
      { key: "age_ns", label: "age", format: "duration_ns", importance: "debug" },
    ],
  },
  mutex: {
    canonical: "mutex",
    displayName: "Mutex",
    category: "sync",
    icon: iconFactory(Lock),
    expectedAttrs: [
      { key: "holder", format: "string", importance: "primary" },
      { key: "waiters", format: "number", importance: "secondary" },
      { key: "held_ns", format: "duration_ns", importance: "secondary" },
    ],
  },
  rwlock: {
    canonical: "rwlock",
    displayName: "RwLock",
    category: "sync",
    icon: iconFactory(LockOpen),
  },
  mpsc_tx: {
    canonical: "mpsc_tx",
    displayName: "MPSC Tx",
    category: "channel",
    icon: iconFactory(ArrowLineUp),
  },
  mpsc_rx: {
    canonical: "mpsc_rx",
    displayName: "MPSC Rx",
    category: "channel",
    icon: iconFactory(ArrowLineDown),
  },
  oneshot_tx: {
    canonical: "oneshot_tx",
    displayName: "Oneshot Tx",
    category: "channel",
    icon: iconFactory(Lightning),
  },
  oneshot_rx: {
    canonical: "oneshot_rx",
    displayName: "Oneshot Rx",
    category: "channel",
    icon: iconFactory(Lightning),
  },
  watch_tx: {
    canonical: "watch_tx",
    displayName: "Watch Tx",
    category: "channel",
    icon: iconFactory(Eye),
  },
  watch_rx: {
    canonical: "watch_rx",
    displayName: "Watch Rx",
    category: "channel",
    icon: iconFactory(Eye),
  },
  broadcast_tx: {
    canonical: "broadcast_tx",
    displayName: "Broadcast Tx",
    category: "channel",
    icon: iconFactory(ArrowLineUp),
  },
  broadcast_rx: {
    canonical: "broadcast_rx",
    displayName: "Broadcast Rx",
    category: "channel",
    icon: iconFactory(ArrowLineDown),
  },
  semaphore: {
    canonical: "semaphore",
    displayName: "Semaphore",
    category: "sync",
    icon: iconFactory(Gauge),
  },
  oncecell: {
    canonical: "oncecell",
    displayName: "OnceCell",
    category: "sync",
    icon: iconFactory(Cube),
  },
  request: {
    canonical: "request",
    displayName: "Request",
    category: "rpc",
    icon: iconFactory(PaperPlaneTilt),
  },
  response: {
    canonical: "response",
    displayName: "Response",
    category: "rpc",
    icon: iconFactory(ArrowBendDownLeft),
  },
  rpc_pair: {
    canonical: "rpc_pair",
    displayName: "RPC Call",
    category: "rpc",
    icon: iconFactory(ArrowsLeftRight),
  },
  connection: {
    canonical: "connection",
    displayName: "Connection",
    category: "net",
    icon: iconFactory(LinkSimple),
  },
  net_connect: {
    canonical: "net_connect",
    displayName: "Connect",
    category: "net",
    icon: iconFactory(Plugs),
  },
  net_accept: {
    canonical: "net_accept",
    displayName: "Accept",
    category: "net",
    icon: iconFactory(WifiHigh),
  },
  net_read: {
    canonical: "net_read",
    displayName: "Readable",
    category: "net",
    icon: iconFactory(DownloadSimple),
  },
  net_write: {
    canonical: "net_write",
    displayName: "Writable",
    category: "net",
    icon: iconFactory(UploadSimple),
  },
  joinset: {
    canonical: "joinset",
    displayName: "JoinSet",
    category: "async",
    icon: iconFactory(Stack),
  },
  command: {
    canonical: "command",
    displayName: "Command",
    category: "meta",
    icon: iconFactory(Terminal),
  },
  file_op: {
    canonical: "file_op",
    displayName: "File Op",
    category: "fs",
    icon: iconFactory(FileText),
  },
  notify: {
    canonical: "notify",
    displayName: "Notify",
    category: "async",
    icon: iconFactory(Bell),
  },
  sleep: {
    canonical: "sleep",
    displayName: "Sleep",
    category: "time",
    icon: iconFactory(Moon),
  },
  interval: {
    canonical: "interval",
    displayName: "Interval",
    category: "time",
    icon: iconFactory(Repeat),
  },
  timeout: {
    canonical: "timeout",
    displayName: "Timeout",
    category: "time",
    icon: iconFactory(HourglassSimple),
  },
  ghost: {
    canonical: "ghost",
    displayName: "Ghost",
    category: "meta",
    icon: iconFactory(Ghost),
  },
  channel_pair: {
    canonical: "channel_pair",
    displayName: "Channel",
    category: "channel",
    icon: iconFactory(Pipe),
  },
  oneshot_pair: {
    canonical: "oneshot_pair",
    displayName: "Oneshot",
    category: "channel",
    icon: iconFactory(Lightning),
  },
  broadcast_pair: {
    canonical: "broadcast_pair",
    displayName: "Broadcast",
    category: "channel",
    icon: iconFactory(Pipe),
  },
  watch_pair: {
    canonical: "watch_pair",
    displayName: "Watch",
    category: "channel",
    icon: iconFactory(Eye),
  },
};

// Curated map of Phosphor icons that custom entities can reference by name.
const CUSTOM_ICON_MAP: Record<string, ComponentType<any>> = {
  ArrowBendDownLeft,
  ArrowLineDown,
  ArrowLineUp,
  ArrowsLeftRight,
  Bell,
  BracketsCurly,
  Cloud,
  Cpu,
  Cube,
  Database,
  DownloadSimple,
  Eye,
  FileText,
  Flame,
  Gauge,
  Gear,
  Ghost,
  Globe,
  HourglassSimple,
  Key,
  Lightning,
  LinkSimple,
  ListBullets,
  Lock,
  LockOpen,
  MagnifyingGlass,
  Moon,
  PaperPlaneTilt,
  Pipe,
  Plugs,
  Queue,
  Repeat,
  Shield,
  Stack,
  Terminal,
  Timer,
  TreeStructure,
  UploadSimple,
  Users,
  WifiHigh,
};

export function resolveCustomIcon(name: string): ComponentType<any> {
  const icon = CUSTOM_ICON_MAP[name];
  if (!icon && name) {
    console.warn(`[moire] unknown custom icon "${name}", using default`);
  }
  return icon ?? CircleDashed;
}

/**
 * Register a custom entity kind spec at runtime. Called during snapshot conversion
 * when a custom entity body is encountered.
 */
export function registerCustomKindSpec(
  kind: string,
  displayName: string,
  category: string,
  iconName: string,
): void {
  if (NODE_KIND_SPECS[kind]) return;
  const validCategory = (
    ["async", "sync", "channel", "rpc", "net", "fs", "time", "meta"] as const
  ).includes(category as NodeKindCategory)
    ? (category as NodeKindCategory)
    : "meta";
  const Icon = resolveCustomIcon(iconName);
  NODE_KIND_SPECS[kind] = {
    canonical: kind,
    displayName,
    category: validCategory,
    icon: iconFactory(Icon),
  };
}

export function kindDisplayName(kind: string): string {
  const canonical = canonicalNodeKind(kind);
  return NODE_KIND_SPECS[canonical]?.displayName ?? kind;
}

export function kindIcon(kind: string, size: number): React.ReactNode {
  const canonical = canonicalNodeKind(kind);
  const spec = NODE_KIND_SPECS[canonical];
  return spec ? spec.icon(size) : <CircleDashed size={size} weight="bold" />;
}

export function kindMetaFor(kind: string): { icon: React.ReactNode; displayName: string } {
  return { icon: kindIcon(kind, 14), displayName: kindDisplayName(kind) };
}
