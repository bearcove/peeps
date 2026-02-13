export function fmtBytes(b: number | null | undefined): string {
  if (b == null) return "\u2014";
  if (b < 1024) return b + " B";
  if (b < 1048576) return (b / 1024).toFixed(1) + " KB";
  if (b < 1073741824) return (b / 1048576).toFixed(1) + " MB";
  return (b / 1073741824).toFixed(2) + " GB";
}

export function fmtDuration(s: number): string {
  if (s >= 60) return (s / 60).toFixed(1) + "m";
  if (s >= 1) return s.toFixed(2) + "s";
  return (s * 1000).toFixed(0) + "ms";
}

export function fmtAge(s: number): string {
  if (s >= 3600) return (s / 3600).toFixed(1) + "h";
  if (s >= 60) return (s / 60).toFixed(1) + "m";
  return s.toFixed(1) + "s";
}

const FRAME_NOISE = [
  "std::backtrace",
  "std::sys",
  "backtrace_rs",
  "backtrace::",
  "vx_dump::thread_stacks::sigprof_handler",
  "peeps_threads::imp::sigprof_handler",
  "__os_lock",
  "_pthread_",
  "pthread_",
  "tokio::runtime::park",
  "tokio::runtime::context",
  "tokio::runtime::scheduler",
  "tokio::runtime::runtime",
  "tokio::runtime::blocking",
  "core::ops::function",
  "std::panicking",
  "std::panic",
  "std::rt::lang_start",
  "<unknown>",
  "tokio::park",
  "mio::poll",
];

const IDLE_FRAME_MARKERS = [
  "parking_lot_core::thread_parker",
  "parking_lot_core::parking_lot::park",
  "parking_lot::condvar::Condvar::wait",
  "parking_lot::condvar::Condvar::wait_until_internal",
  "tokio::runtime::park::",
  "tokio::park",
  "mio::poll",
  "epoll_wait",
  "kevent",
];

export function firstUsefulFrame(bt: string | null): string | null {
  if (!bt) return null;
  for (const line of bt.split("\n")) {
    const m = line.trim().match(/^\d+:\s+(.+)/);
    if (!m) continue;
    const fn_name = m[1].split("\n")[0].trim();
    if (FRAME_NOISE.some((prefix) => fn_name.startsWith(prefix))) continue;
    return fn_name;
  }
  return null;
}

export function isLikelyIdleBacktrace(bt: string | null | undefined): boolean {
  if (!bt) return false;
  for (const line of bt.split("\n")) {
    const m = line.trim().match(/^\d+:\s+(.+)/);
    if (!m) continue;
    const fn_name = m[1].split("\n")[0].trim();
    if (FRAME_NOISE.some((prefix) => fn_name.startsWith(prefix))) continue;
    if (IDLE_FRAME_MARKERS.some((marker) => fn_name.includes(marker))) return true;
    return false;
  }
  return false;
}

export function isLikelyIdleFrameName(frame: string | null | undefined): boolean {
  if (!frame) return false;
  return IDLE_FRAME_MARKERS.some((marker) => frame.includes(marker));
}

export function classNames(
  ...args: (string | false | null | undefined)[]
): string {
  return args.filter(Boolean).join(" ");
}
