// Mirrors peeps-types/src/lib.rs

export interface ProcessDump {
  process_name: string;
  pid: number;
  timestamp: string;
  tasks: TaskSnapshot[];
  threads: ThreadStackSnapshot[];
  locks: LockSnapshot | null;
  sync: SyncSnapshot | null;
  roam: SessionSnapshot | null;
  shm: ShmSnapshot | null;
  custom: Record<string, string>;
}

// Tasks

export interface TaskSnapshot {
  id: number;
  name: string;
  state: TaskState;
  spawned_at_secs: number;
  age_secs: number;
  spawn_backtrace: string;
  poll_events: PollEvent[];
  parent_task_id: number | null;
  parent_task_name: string | null;
}

export type TaskState = "Pending" | "Polling" | "Completed";

export interface PollEvent {
  started_at_secs: number;
  duration_secs: number | null;
  result: PollResult;
  backtrace: string | null;
}

export type PollResult = "Pending" | "Ready";

// Threads

export interface ThreadStackSnapshot {
  name: string;
  backtrace: string | null;
  samples: number;
  responded: number;
  same_location_count: number;
  dominant_frame: string | null;
}

// Locks

export interface LockSnapshot {
  locks: LockInfoSnapshot[];
}

export interface LockInfoSnapshot {
  name: string;
  acquires: number;
  releases: number;
  holders: LockHolderSnapshot[];
  waiters: LockWaiterSnapshot[];
}

export type LockAcquireKind = "Read" | "Write" | "Mutex";

export interface LockHolderSnapshot {
  kind: LockAcquireKind;
  held_secs: number;
  backtrace: string | null;
  task_id: number | null;
  task_name: string | null;
}

export interface LockWaiterSnapshot {
  kind: LockAcquireKind;
  waiting_secs: number;
  backtrace: string | null;
  task_id: number | null;
  task_name: string | null;
}

// Sync

export interface SyncSnapshot {
  mpsc_channels: MpscChannelSnapshot[];
  oneshot_channels: OneshotChannelSnapshot[];
  watch_channels: WatchChannelSnapshot[];
  once_cells: OnceCellSnapshot[];
}

export interface MpscChannelSnapshot {
  name: string;
  bounded: boolean;
  capacity: number | null;
  sent: number;
  received: number;
  send_waiters: number;
  sender_count: number;
  sender_closed: boolean;
  receiver_closed: boolean;
  age_secs: number;
  creator_task_id: number | null;
  creator_task_name: string | null;
}

export interface OneshotChannelSnapshot {
  name: string;
  state: OneshotState;
  age_secs: number;
  creator_task_id: number | null;
  creator_task_name: string | null;
}

export type OneshotState =
  | "Pending"
  | "Sent"
  | "Received"
  | "SenderDropped"
  | "ReceiverDropped";

export interface WatchChannelSnapshot {
  name: string;
  changes: number;
  receiver_count: number;
  age_secs: number;
  creator_task_id: number | null;
  creator_task_name: string | null;
}

export interface OnceCellSnapshot {
  name: string;
  state: OnceCellState;
  age_secs: number;
  init_duration_secs: number | null;
}

export type OnceCellState = "Empty" | "Initializing" | "Initialized";

// Roam session

export type Direction = "Outgoing" | "Incoming";
export type ChannelDir = "Tx" | "Rx";

export interface SessionSnapshot {
  connections: ConnectionSnapshot[];
  method_names: Record<string, string>;
}

export interface ConnectionSnapshot {
  name: string;
  peer_name: string | null;
  age_secs: number;
  total_completed: number;
  max_concurrent_requests: number;
  initial_credit: number;
  in_flight: RequestSnapshot[];
  recent_completions: CompletionSnapshot[];
  channels: ChannelSnapshot[];
  transport: TransportStats;
  channel_credits: ChannelCreditSnapshot[];
}

export interface RequestSnapshot {
  request_id: number;
  method_name: string | null;
  method_id: number;
  direction: Direction;
  elapsed_secs: number;
  task_id: number | null;
  task_name: string | null;
  metadata: Record<string, string> | null;
  args: Record<string, string> | null;
  backtrace: string | null;
}

export interface CompletionSnapshot {
  method_name: string | null;
  method_id: number;
  direction: Direction;
  duration_secs: number;
  age_secs: number;
}

export interface ChannelSnapshot {
  channel_id: number;
  direction: ChannelDir;
  age_secs: number;
  request_id: number | null;
}

export interface TransportStats {
  frames_sent: number;
  frames_received: number;
  bytes_sent: number;
  bytes_received: number;
  last_sent_ago_secs: number | null;
  last_recv_ago_secs: number | null;
}

export interface ChannelCreditSnapshot {
  channel_id: number;
  incoming_credit: number;
  outgoing_credit: number;
}

// SHM

export interface ShmSnapshot {
  segments: ShmSegmentSnapshot[];
  channels: ChannelQueueSnapshot[];
}

export interface ShmSegmentSnapshot {
  segment_path: string | null;
  total_size: number;
  current_size: number;
  max_peers: number;
  host_goodbye: boolean;
  peers: ShmPeerSnapshot[];
  var_pool: VarSlotClassSnapshot[];
}

export interface ShmPeerSnapshot {
  peer_id: number;
  state: ShmPeerState;
  name: string | null;
  bipbuf_capacity: number;
  bytes_sent: number;
  bytes_received: number;
  calls_sent: number;
  calls_received: number;
  time_since_heartbeat_ms: number | null;
}

export type ShmPeerState =
  | "Empty"
  | "Reserved"
  | "Attached"
  | "Goodbye"
  | "Unknown";

export interface VarSlotClassSnapshot {
  slot_size: number;
  slots_per_extent: number;
  extent_count: number;
  free_slots_approx: number;
  total_slots: number;
}

export interface ChannelQueueSnapshot {
  name: string;
  len: number;
  capacity: number;
}
