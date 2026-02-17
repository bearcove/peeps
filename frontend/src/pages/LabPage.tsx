import { useCallback, useMemo, useState } from "react";
import {
  WarningCircle,
  CaretDown,
  CopySimple,
  ArrowSquareOut,
  FileRs,
  CircleNotch,
  LinkSimple,
  PaperPlaneTilt,
  Timer,
} from "@phosphor-icons/react";
import { Panel } from "../ui/layout/Panel";
import { PanelHeader } from "../ui/layout/PanelHeader";
import { Row } from "../ui/layout/Row";
import { Section } from "../ui/layout/Section";
import { Badge, type BadgeTone } from "../ui/primitives/Badge";
import { TextInput } from "../ui/primitives/TextInput";
import { SearchInput } from "../ui/primitives/SearchInput";
import { Checkbox } from "../ui/primitives/Checkbox";
import { Select } from "../ui/primitives/Select";
import { LabeledSlider } from "../ui/primitives/Slider";
import { Menu } from "../ui/primitives/Menu";
import { FilterMenu, type FilterMenuItem } from "../ui/primitives/FilterMenu";
import { SegmentedGroup } from "../ui/primitives/SegmentedGroup";
import { KeyValueRow } from "../ui/primitives/KeyValueRow";
import { RelativeTimestamp } from "../ui/primitives/RelativeTimestamp";
import { DurationDisplay } from "../ui/primitives/DurationDisplay";
import { NodeChip } from "../ui/primitives/NodeChip";
import { ProcessIdenticon } from "../ui/primitives/ProcessIdenticon";
import { Table, type Column } from "../ui/primitives/Table";
import { ActionButton } from "../ui/primitives/ActionButton";
import { kindIcon } from "../nodeKindSpec";
import { DeadlockDetectorMockup } from "./DeadlockDetectorPage";

type DemoTone = "neutral" | "ok" | "warn" | "crit";
type DemoConnectionRow = {
  id: string;
  healthLabel: string;
  healthTone: DemoTone;
  connectionKind: string;
  connectionLabel: string;
  pending: number;
  lastRecvBasis: "P" | "N";
  lastRecvBasisLabel: string;
  lastRecvBasisTime: string;
  lastRecvEventTime: string;
  lastRecvTone: DemoTone;
  lastSentBasis: "P" | "N";
  lastSentBasisLabel: string;
  lastSentBasisTime: string;
  lastSentEventTime: string | null;
  lastSentTone: DemoTone;
};

export function LabPage() {
  const [textValue, setTextValue] = useState("Hello");
  const [searchValue, setSearchValue] = useState("");
  const [checked, setChecked] = useState(true);
  const [selectValue, setSelectValue] = useState("all");
  const [sliderValue, setSliderValue] = useState(1);
  const [searchOnlyKind, setSearchOnlyKind] = useState<string | null>(null);
  const [selectedSearchId, setSelectedSearchId] = useState<string | null>(null);
  const [segmentedMode, setSegmentedMode] = useState("graph");
  const [segmentedSeverity, setSegmentedSeverity] = useState("all");
  const [tableSortKey, setTableSortKey] = useState("health");
  const [tableSortDir, setTableSortDir] = useState<"asc" | "desc">("desc");
  const [selectedTableRow, setSelectedTableRow] = useState<string | null>(null);
  const [hiddenKinds, setHiddenKinds] = useState<Set<string>>(new Set());
  const [hiddenProcesses, setHiddenProcesses] = useState<Set<string>>(new Set());
  const tones = useMemo<BadgeTone[]>(() => ["neutral", "ok", "warn", "crit"], []);
  const searchDataset = useMemo(() => [
    { id: "future:store.incoming.recv", label: "store.incoming.recv", kind: "future", process: "vx-store" },
    { id: "request:demorpc.sleepy", label: "DemoRpc.sleepy_forever", kind: "request", process: "example-roam-rpc-stuck-request" },
    { id: "request:demorpc.ping", label: "DemoRpc.ping", kind: "request", process: "example-roam-rpc-stuck-request" },
    { id: "channel:mpsc.tx", label: "channel.v1.mpsc.send", kind: "channel", process: "vx-runner" },
    { id: "channel:mpsc.rx", label: "channel.v1.mpsc.recv", kind: "channel", process: "vx-vfsd" },
    { id: "oneshot:recv", label: "channel.v1.oneshot.recv", kind: "oneshot", process: "vx-store" },
    { id: "resource:conn", label: "connection initiator->acceptor", kind: "resource", process: "vxd" },
    { id: "net:read", label: "net.readable.wait", kind: "net", process: "vxd" },
  ], []);

  const processIdenticonNames = useMemo(
    () => [
      "example-roam-rpc-stuck-request",
      "vx-store",
      "vx-runner",
      "vx-vfsd",
      "vxd",
      "peeps-collector",
    ],
    [],
  );

  const filterKindItems = useMemo<FilterMenuItem[]>(() => [
    { id: "connection", label: "Connection", icon: kindIcon("connection", 14), meta: "connection" },
    { id: "mutex", label: "Mutex", icon: kindIcon("mutex", 14), meta: "lock" },
    { id: "request", label: "Request", icon: kindIcon("request", 14), meta: "request" },
    { id: "response", label: "Response", icon: kindIcon("response", 14), meta: "response" },
    { id: "channel_rx", label: "Channel Rx", icon: kindIcon("channel_rx", 14), meta: "rx" },
    { id: "channel_tx", label: "Channel Tx", icon: kindIcon("channel_tx", 14), meta: "tx" },
  ], []);

  const filterProcessItems = useMemo<FilterMenuItem[]>(() => [
    { id: "vx-store", label: "vx-store" },
    { id: "vx-runner", label: "vx-runner" },
    { id: "vx-vfsd", label: "vx-vfsd" },
    { id: "vxd", label: "vxd" },
    { id: "peeps-collector", label: "peeps-collector" },
  ], []);

  const toggleKind = useCallback((id: string) => {
    setHiddenKinds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const soloKind = useCallback((id: string) => {
    setHiddenKinds((prev) => {
      const othersAllHidden = filterKindItems.every((item) => item.id === id || prev.has(item.id));
      if (othersAllHidden && !prev.has(id)) return new Set();
      return new Set(filterKindItems.filter((item) => item.id !== id).map((item) => item.id));
    });
  }, [filterKindItems]);

  const toggleProcess = useCallback((id: string) => {
    setHiddenProcesses((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const soloProcess = useCallback((id: string) => {
    setHiddenProcesses((prev) => {
      const othersAllHidden = filterProcessItems.every((item) => item.id === id || prev.has(item.id));
      if (othersAllHidden && !prev.has(id)) return new Set();
      return new Set(filterProcessItems.filter((item) => item.id !== id).map((item) => item.id));
    });
  }, [filterProcessItems]);

  const tableRows = useMemo<DemoConnectionRow[]>(() => [
    {
      id: "conn-01",
      healthLabel: "OK",
      healthTone: "ok",
      connectionKind: "connection",
      connectionLabel: "example-roam-rpc-stuck-request: initiator\u2192acceptor",
      pending: 0,
      lastRecvBasis: "P",
      lastRecvBasisLabel: "process started",
      lastRecvBasisTime: "2026-02-17T10:05:00.000Z",
      lastRecvEventTime: "2026-02-17T10:05:12.000Z",
      lastRecvTone: "ok",
      lastSentBasis: "N",
      lastSentBasisLabel: "node created",
      lastSentBasisTime: "2026-02-17T10:05:12.000Z",
      lastSentEventTime: "2026-02-17T10:05:18.000Z",
      lastSentTone: "ok",
    },
    {
      id: "conn-02",
      healthLabel: "WARN",
      healthTone: "warn",
      connectionKind: "channel_tx",
      connectionLabel: "vx-store \u00b7 channel.v1.mpsc.send",
      pending: 3,
      lastRecvBasis: "P",
      lastRecvBasisLabel: "process started",
      lastRecvBasisTime: "2026-02-17T10:05:00.000Z",
      lastRecvEventTime: "2026-02-17T10:05:22.000Z",
      lastRecvTone: "warn",
      lastSentBasis: "N",
      lastSentBasisLabel: "connection opened",
      lastSentBasisTime: "2026-02-17T10:03:20.000Z",
      lastSentEventTime: "2026-02-17T10:04:10.000Z",
      lastSentTone: "warn",
    },
    {
      id: "conn-03",
      healthLabel: "CRIT",
      healthTone: "crit",
      connectionKind: "request",
      connectionLabel: "example-roam-rpc-stuck-request \u00b7 DemoRpc.sleepy_forever",
      pending: 12,
      lastRecvBasis: "N",
      lastRecvBasisLabel: "node opened",
      lastRecvBasisTime: "2026-02-17T09:20:00.000Z",
      lastRecvEventTime: "2026-02-17T09:27:55.000Z",
      lastRecvTone: "crit",
      lastSentBasis: "N",
      lastSentBasisLabel: "node opened",
      lastSentBasisTime: "2026-02-17T09:20:00.000Z",
      lastSentEventTime: "2026-02-17T09:24:22.000Z",
      lastSentTone: "crit",
    },
    {
      id: "conn-04",
      healthLabel: "WARN",
      healthTone: "warn",
      connectionKind: "connection",
      connectionLabel: "vxd \u00b7 connection: initiator<->acceptor",
      pending: 8,
      lastRecvBasis: "P",
      lastRecvBasisLabel: "process started",
      lastRecvBasisTime: "2026-02-17T10:05:00.000Z",
      lastRecvEventTime: "2026-02-17T10:05:30.000Z",
      lastRecvTone: "warn",
      lastSentBasis: "N",
      lastSentBasisLabel: "resource created",
      lastSentBasisTime: "2026-02-17T10:03:30.000Z",
      lastSentEventTime: null,
      lastSentTone: "warn",
    },
    {
      id: "conn-05",
      healthLabel: "OK",
      healthTone: "ok",
      connectionKind: "request",
      connectionLabel: "vx-vfsd \u00b7 net.readable.wait",
      pending: 1,
      lastRecvBasis: "N",
      lastRecvBasisLabel: "socket opened",
      lastRecvBasisTime: "2026-02-17T10:04:10.000Z",
      lastRecvEventTime: "2026-02-17T10:04:14.000Z",
      lastRecvTone: "ok",
      lastSentBasis: "N",
      lastSentBasisLabel: "socket opened",
      lastSentBasisTime: "2026-02-17T10:04:10.000Z",
      lastSentEventTime: "2026-02-17T10:04:12.000Z",
      lastSentTone: "ok",
    },
  ], []);

  const tableColumns = useMemo<readonly Column<DemoConnectionRow>[]>(() => [
    { key: "health", label: "Health", sortable: true, width: "80px", render: (row) => <Badge tone={row.healthTone}>{row.healthLabel}</Badge> },
    { key: "connection", label: "Connection", sortable: true, width: "1fr", render: (row) => (
      <NodeChip
        kind={row.connectionKind}
        label={row.connectionLabel}
        onClick={() => console.log(`select connection ${row.id}`)}
        onContextMenu={(event) => {
          event.preventDefault();
          console.log(`connection context menu ${row.id}`);
        }}
      />
    ) },
    { key: "pending", label: "Pending Req", sortable: true, width: "80px", render: (row) => row.pending },
    { key: "lastRecv", label: "Last Recv", sortable: true, width: "100px", render: (row) => (
      <RelativeTimestamp
        basis={row.lastRecvBasis}
        basisLabel={row.lastRecvBasisLabel}
        basisTime={row.lastRecvBasisTime}
        eventTime={row.lastRecvEventTime}
        tone={row.lastRecvTone}
      />
    ) },
    { key: "lastSent", label: "Last Sent", sortable: true, width: "100px", render: (row) => {
      if (row.lastSentEventTime === null) return <span>&mdash;</span>;
      return (
        <RelativeTimestamp
          basis={row.lastSentBasis}
          basisLabel={row.lastSentBasisLabel}
          basisTime={row.lastSentBasisTime}
          eventTime={row.lastSentEventTime}
          tone={row.lastSentTone}
        />
      );
    } },
  ], []);

  const tableSortedRows = useMemo(() => {
    const healthOrder: Record<string, number> = {
      healthy: 1, warning: 2, critical: 3, ok: 1, warn: 2, crit: 3,
    };
    const by = tableSortKey === "connection" ? (row: DemoConnectionRow) => row.connectionLabel
      : tableSortKey === "pending" ? (row: DemoConnectionRow) => row.pending
      : tableSortKey === "lastRecv" ? (row: DemoConnectionRow) => Date.parse(row.lastRecvEventTime)
      : tableSortKey === "lastSent" ? (row: DemoConnectionRow) => row.lastSentEventTime === null ? Number.NEGATIVE_INFINITY : Date.parse(row.lastSentEventTime)
      : (row: DemoConnectionRow) => healthOrder[row.healthTone];
    const direction = tableSortDir === "asc" ? 1 : -1;

    return [...tableRows].sort((a, b) => {
      const aValue = by(a);
      const bValue = by(b);
      if (typeof aValue === "number" && typeof bValue === "number") return (aValue - bValue) * direction;
      return String(aValue).localeCompare(String(bValue), undefined, { numeric: true }) * direction;
    });
  }, [tableRows, tableSortDir, tableSortKey]);

  function onTableSort(key: string) {
    if (!tableColumns.some((column) => column.key === key && column.sortable)) return;
    if (tableSortKey === key) {
      setTableSortDir((prev) => (prev === "asc" ? "desc" : "asc"));
      return;
    }
    setTableSortKey(key);
    setTableSortDir("desc");
  }

  const searchResults = useMemo(() => {
    const needle = searchValue.trim().toLowerCase();
    return searchDataset
      .filter((item) => !searchOnlyKind || item.kind === searchOnlyKind)
      .filter((item) => {
        if (needle.length === 0) return true;
        return (
          item.label.toLowerCase().includes(needle)
          || item.id.toLowerCase().includes(needle)
          || item.process.toLowerCase().includes(needle)
          || item.kind.toLowerCase().includes(needle)
        );
      })
      .slice(0, 6);
  }, [searchDataset, searchOnlyKind, searchValue]);
  const showSearchResults = searchValue.trim().length > 0 || searchOnlyKind !== null;
  const selectOptions = useMemo(() => [
    { value: "all", label: "All" },
    { value: "warn", label: "Warning+" },
    { value: "crit", label: "Critical" },
  ], []);
  const nodeTypeMenu = useMemo(() => [
    { id: "show-kind", label: "Show only this kind" },
    { id: "hide-kind", label: "Hide this kind" },
    { id: "reset", label: "Reset filters", danger: true },
  ], []);
  const processMenu = useMemo(() => [
    { id: "open-resources", label: "Open in Resources" },
    { id: "show-process", label: "Show only this process" },
    { id: "hide-process", label: "Hide this process" },
  ], []);

  return (
    <Panel variant="lab">
      <PanelHeader title="Lab" hint="Primitives and tone language" />
      <div className="lab-body">
        <Section title="Deadlock Detector" subtitle="Full app layout mockup with resizable inspector" wide>
          <DeadlockDetectorMockup />
        </Section>

        <Section title="UI font — Manrope" subtitle="UI font in the sizes we actually use" wide>
          <div className="ui-typo-card">
            <div className="ui-typo-sample ui-typo-ui ui-typo-ui--xl">Take a snapshot</div>
            <div className="ui-typo-sample ui-typo-ui ui-typo-ui--md">Inspector, Graph, Timeline, Resources</div>
            <div className="ui-typo-sample ui-typo-ui ui-typo-ui--sm ui-typo-muted">
              Buttons, labels, helper text, and navigation should mostly live here.
            </div>
            <div className="ui-typo-weights">
              <span className="ui-typo-pill ui-typo-ui ui-typo-w-400">400</span>
              <span className="ui-typo-pill ui-typo-ui ui-typo-w-700">700</span>
            </div>
          </div>
        </Section>

        <Section title="Mono font — Jetbrains Mono" subtitle="Mono font in the sizes we actually use" wide>
          <div className="ui-typo-card">
            <div className="ui-typo-sample ui-typo-mono ui-typo-mono--xl">request:01KHNGCY&hellip;</div>
            <div className="ui-typo-sample ui-typo-mono ui-typo-mono--md">connection: initiator-&gt;acceptor</div>
            <div className="ui-typo-sample ui-typo-mono ui-typo-mono--sm ui-typo-muted">
              IDs, paths, tokens, and anything users copy/paste.
            </div>
            <div className="ui-typo-weights">
              <span className="ui-typo-pill ui-typo-mono ui-typo-w-400">400</span>
              <span className="ui-typo-pill ui-typo-mono ui-typo-w-700">700</span>
            </div>
          </div>
        </Section>

        <Section title="Buttons" subtitle="Variants, sizes, and icon combinations">
          <div className="ui-section-stack">
            <Row>
              <ActionButton>Default</ActionButton>
              <ActionButton variant="primary">Primary</ActionButton>
              <ActionButton variant="ghost">Ghost</ActionButton>
              <ActionButton isDisabled>Disabled</ActionButton>
            </Row>
            <Row>
              <ActionButton>
                <WarningCircle size={14} weight="bold" />
                With icon
              </ActionButton>
              <ActionButton>
                <CopySimple size={12} weight="bold" />
                Copy
              </ActionButton>
              <ActionButton>
                <ArrowSquareOut size={12} weight="bold" />
                Open
              </ActionButton>
            </Row>
            <Row>
              <ActionButton size="sm">Small</ActionButton>
              <ActionButton
                size="sm"
                aria-label="Copy"
              >
                <CopySimple size={12} weight="bold" />
              </ActionButton>
            </Row>
          </div>
        </Section>

        <Section title="Badges" subtitle="Single token primitive with variants">
          <div className="ui-section-stack">
            <Row>
              {tones.map((tone) => (
                <Badge key={`standard-${tone}`} tone={tone}>
                  {tone.toUpperCase()}
                </Badge>
              ))}
            </Row>
            <Row>
              {tones.map((tone) => (
                <Badge key={`count-${tone}`} tone={tone} variant="count">
                  {tone === "neutral" ? "0" : tone === "ok" ? "3" : tone === "warn" ? "7" : "118"}
                </Badge>
              ))}
            </Row>
          </div>
        </Section>

        <Section title="Text Input" subtitle="Plain text field">
          <TextInput
            value={textValue}
            onChange={setTextValue}
            placeholder="Type\u2026"
            aria-label="Text input"
          />
        </Section>

        <Section title="Search" subtitle="Autocomplete with results, filters, and selection" wide>
          <SearchInput
            value={searchValue}
            onChange={setSearchValue}
            placeholder="Search nodes\u2026"
            aria-label="Search input"
            items={searchResults.map((item) => ({
              id: item.id,
              label: <NodeChip kind={item.kind} label={item.label} />,
              meta: item.process,
            }))}
            showSuggestions={showSearchResults}
            selectedId={selectedSearchId}
            resultHint={
              <>
                <span>{searchResults.length} result(s)</span>
                <span className="ui-search-results-hint">click to select &middot; alt+click to filter only this kind</span>
              </>
            }
            filterBadge={searchOnlyKind ? <Badge tone="neutral">{`kind:${searchOnlyKind}`}</Badge> : undefined}
            onClearFilter={() => setSearchOnlyKind(null)}
            onSelect={(id) => setSelectedSearchId(id)}
            onAltSelect={(id) => {
              const item = searchResults.find((entry) => entry.id === id);
              if (!item) return;
              setSearchOnlyKind((prev) => (prev === item.kind ? null : item.kind));
            }}
          />
        </Section>

        <Section title="Controls" subtitle="Checkbox, select">
          <Row className="ui-row--controls">
            <Checkbox
              checked={checked}
              onChange={setChecked}
              label="Show resources"
            />
            <Select
              value={selectValue}
              onChange={(next) => setSelectValue(next)}
              aria-label="Select"
              options={selectOptions}
            />
          </Row>
        </Section>

        <Section title="Slider" subtitle="Labeled slider with discrete steps">
          <LabeledSlider
            value={sliderValue}
            min={0}
            max={2}
            step={1}
            onChange={(v) => setSliderValue(v)}
            aria-label="Detail level"
            label="Detail"
            valueLabel={sliderValue === 0 ? "info" : sliderValue === 1 ? "debug" : "trace"}
          />
        </Section>

        <Section title="Menu" subtitle="Action menus for context operations">
          <Row>
            <Menu
              label={
                <span className="ui-menu-label">
                  <span>Node types</span>
                  <CaretDown size={12} weight="bold" />
                </span>
              }
              items={nodeTypeMenu}
            />
            <Menu
              label={<span className="ui-menu-label">Process <CaretDown size={12} weight="bold" /></span>}
              items={processMenu}
            />
          </Row>
        </Section>

        <Section title="Filter Menu" subtitle="Multi-select with checkboxes, alt-click to solo">
          <Row>
            <FilterMenu
              label="Node types"
              items={filterKindItems}
              hiddenIds={hiddenKinds}
              onToggle={toggleKind}
              onSolo={soloKind}
            />
            <FilterMenu
              label="Processes"
              items={filterProcessItems}
              hiddenIds={hiddenProcesses}
              onToggle={toggleProcess}
              onSolo={soloProcess}
            />
          </Row>
        </Section>

        <Section title="Segmented Group" subtitle="Mutually-exclusive mode and severity controls">
          <div className="ui-section-stack">
            <SegmentedGroup
              value={segmentedMode}
              onChange={setSegmentedMode}
              options={[
                { value: "graph", label: "Graph" },
                { value: "timeline", label: "Timeline" },
                { value: "resources", label: "Resources" },
              ]}
              aria-label="Mode switcher"
            />
            <SegmentedGroup
              value={segmentedSeverity}
              onChange={setSegmentedSeverity}
              size="sm"
              options={[
                { value: "all", label: "All" },
                { value: "warn", label: "Warning+" },
                { value: "crit", label: "Critical" },
              ]}
              aria-label="Severity filter"
            />
          </div>
        </Section>

        <Section title="Key-Value Rows" subtitle="Inspector-like metadata rows">
          <div className="ui-section-stack">
            <KeyValueRow
              label="Method"
              labelWidth={80}
              icon={<PaperPlaneTilt size={12} weight="bold" />}
            >
              DemoRpc.sleepy_forever
            </KeyValueRow>
            <KeyValueRow
              label="Source"
              labelWidth={80}
            >
              <NodeChip
                icon={<FileRs size={12} weight="bold" />}
                label="main.rs:20"
                href="zed://file/%2Fapp%2Fsrc%2Fmain.rs%3A20"
                title="Open /app/src/main.rs:20 in editor"
              />
            </KeyValueRow>
            <KeyValueRow
              label="Status"
              labelWidth={80}
              icon={<CircleNotch size={12} weight="bold" />}
            >
              <Badge tone="warn">IN_FLIGHT</Badge>
            </KeyValueRow>
            <KeyValueRow label="Elapsed" labelWidth={80}>
              <DurationDisplay ms={1245000} tone="crit" />
            </KeyValueRow>
            <KeyValueRow
              label="Connection"
              labelWidth={80}
              icon={<LinkSimple size={12} weight="bold" />}
            >
              <NodeChip
                kind="connection"
                label="initiator\u2192acceptor"
                onClick={() => console.log("inspect initiator\u2192acceptor")}
                onContextMenu={(event) => {
                  event.preventDefault();
                  console.log("open context for initiator\u2192acceptor");
                }}
              />
            </KeyValueRow>
            <KeyValueRow
              label="Opened"
              labelWidth={80}
              icon={<Timer size={12} weight="bold" />}
            >
              <RelativeTimestamp
                basis="P"
                basisLabel="process started"
                basisTime="2026-02-17T10:06:00.000Z"
                eventTime="2026-02-17T10:06:06.000Z"
              />
            </KeyValueRow>
            <KeyValueRow
              label="Closed"
              labelWidth={80}
              icon={<Timer size={12} weight="bold" />}
            >
              <RelativeTimestamp
                basis="N"
                basisLabel="connection opened"
                basisTime="2026-02-17T10:06:00.000Z"
                eventTime="2026-02-17T10:07:05.000Z"
              />
            </KeyValueRow>
            <KeyValueRow label="Pending" labelWidth={80}>
              3
            </KeyValueRow>
          </div>
        </Section>

        <Section title="Relative Timestamps" subtitle="P/N deltas with tooltip context">
          <Row className="ui-relative-timestamp-group">
            <div className="ui-relative-timestamp-item">
              <RelativeTimestamp basis="P" basisLabel="6 seconds after process start" basisTime="2026-02-17T10:00:00.000Z" eventTime="2026-02-17T10:00:06.000Z" />
              <span className="ui-relative-timestamp-caption">process start</span>
            </div>
            <div className="ui-relative-timestamp-item">
              <RelativeTimestamp basis="P" basisLabel="2 minutes 30 seconds after process start" basisTime="2026-02-17T10:00:00.000Z" eventTime="2026-02-17T10:02:30.000Z" />
              <span className="ui-relative-timestamp-caption">process start</span>
            </div>
            <div className="ui-relative-timestamp-item">
              <RelativeTimestamp basis="N" basisLabel="node just created" basisTime="2026-02-17T10:00:30.000Z" eventTime="2026-02-17T10:00:30.000Z" tone="ok" />
              <span className="ui-relative-timestamp-caption">node created</span>
            </div>
            <div className="ui-relative-timestamp-item">
              <RelativeTimestamp basis="N" basisLabel="1m5s after node open" basisTime="2026-02-17T10:00:30.000Z" eventTime="2026-02-17T10:01:35.000Z" tone="warn" />
              <span className="ui-relative-timestamp-caption">node-relative</span>
            </div>
            <div className="ui-relative-timestamp-item">
              <RelativeTimestamp basis="N" basisLabel="stuck for 20 minutes" basisTime="2026-02-17T10:00:30.000Z" eventTime="2026-02-17T10:21:15.000Z" tone="crit" />
              <span className="ui-relative-timestamp-caption">stuck 20m</span>
            </div>
            <div className="ui-relative-timestamp-item">
              <RelativeTimestamp basis="N" basisLabel="sub-second timing check" basisTime="2026-02-17T10:00:30.000Z" eventTime="2026-02-17T10:00:30.145Z" />
              <span className="ui-relative-timestamp-caption">sub-second</span>
            </div>
          </Row>
        </Section>

        <Section title="Duration Display" subtitle="Automatic semantic coloring by magnitude">
          <Row className="ui-duration-row">
            <DurationDisplay ms={200} />
            <DurationDisplay ms={6200} />
            <DurationDisplay ms={45000} />
            <DurationDisplay ms={150000} />
            <DurationDisplay ms={1245000} />
            <DurationDisplay ms={4320000} />
          </Row>
        </Section>

        <Section title="Node Chips" subtitle="Inline clickable node/resource references">
          <Row>
            <NodeChip
              kind="connection"
              label="initiator\u2192acceptor:acceptor\u2194\u2192initiator"
              onClick={() => console.log("open connection chip")}
              onContextMenu={(event) => {
                event.preventDefault();
                console.log("show connection context menu");
              }}
            />
            <NodeChip
              kind="request"
              label="DemoRpc.sleepy_forever"
              onClick={() => console.log("open request chip")}
              onContextMenu={(event) => {
                event.preventDefault();
                console.log("show request context menu");
              }}
            />
            <NodeChip
              kind="channel_tx"
              label="mpsc.send"
              onClick={() => console.log("open channel chip")}
              onContextMenu={(event) => {
                event.preventDefault();
                console.log("show channel context menu");
              }}
            />
            <NodeChip
              label="example-roam-rpc-stuck-request"
              onClick={() => console.log("open generic chip")}
              onContextMenu={(event) => {
                event.preventDefault();
                console.log("show generic chip context menu");
              }}
            />
          </Row>
          <div className="ui-lab-hint">Left-click to navigate, right-click for actions</div>
        </Section>

        <Section title="Process Identicons" subtitle="Name-derived 5x5 process avatars">
          <div className="ui-identicon-list">
            {processIdenticonNames.map((name) => (
              <span key={name} className="ui-identicon-cell">
                <ProcessIdenticon name={name} size={20} />
                <span>{name}</span>
              </span>
            ))}
          </div>
        </Section>

        <Section title="Table" subtitle="Sortable, sticky header, selectable rows" wide>
          <Table
            columns={tableColumns}
            rows={tableSortedRows}
            rowKey={(row) => row.id}
            sortKey={tableSortKey}
            sortDir={tableSortDir}
            selectedRowKey={selectedTableRow}
            onSort={onTableSort}
            onRowClick={(row) => setSelectedTableRow(row.id)}
            aria-label="Demo connections table"
          />
        </Section>

      </div>
    </Panel>
  );
}
