import type { SummaryData } from "../api";

interface HeaderProps {
  summary: SummaryData | null;
  filter: string;
  onFilter: (v: string) => void;
  onRefresh: () => void;
  error: string | null;
  stale: boolean;
  latestSeq: number;
  currentSeq: number;
  refreshing: boolean;
}

export function Header({
  summary,
  filter,
  onFilter,
  onRefresh,
  error,
  stale,
  latestSeq,
  currentSeq,
  refreshing,
}: HeaderProps) {
  return (
    <div class="header">
      <div class="header-brand">
        <span class="accent">peeps</span> dashboard
      </div>
      <div class="header-sep" />
      <div class="header-stats">
        <span>
          <span class="val">{summary?.process_count ?? "\u2014"}</span> processes
        </span>
        <span>
          <span class="val">{summary?.task_count ?? "\u2014"}</span> tasks
        </span>
        <span>
          <span class="val">{summary?.thread_count ?? "\u2014"}</span> threads
        </span>
      </div>
      <div class="header-spacer" />
      {stale && (
        <span class="header-stale">New diagnostics available (seq {latestSeq})</span>
      )}
      {currentSeq > 0 && (
        <span class="header-seq">seq {currentSeq}</span>
      )}
      {error && <span class="header-error">{error}</span>}
      <input
        class="search-box"
        type="text"
        placeholder="Filter..."
        autocomplete="off"
        spellcheck={false}
        value={filter}
        onInput={(e) => onFilter((e.target as HTMLInputElement).value)}
      />
      <button
        class={`expand-trigger${stale ? " header-refresh-stale" : ""}`}
        style="padding: 5px 12px; font-size: 12px"
        onClick={onRefresh}
        disabled={refreshing}
      >
        {refreshing ? "Refreshing\u2026" : stale ? "Refresh (new data)" : "Refresh"}
      </button>
    </div>
  );
}
