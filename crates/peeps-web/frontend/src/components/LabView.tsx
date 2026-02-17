import { useMemo, useState } from "react";
import {
  WarningCircle,
  CaretDown,
  Check,
  CopySimple,
  ArrowSquareOut,
} from "@phosphor-icons/react";
import { Panel } from "../ui/layout/Panel";
import { PanelHeader } from "../ui/layout/PanelHeader";
import { Row } from "../ui/layout/Row";
import { Section } from "../ui/layout/Section";
import { Button } from "../ui/primitives/Button";
import { Badge, type BadgeTone } from "../ui/primitives/Badge";
import { TextInput } from "../ui/primitives/TextInput";
import { SearchInput } from "../ui/primitives/SearchInput";
import { Checkbox } from "../ui/primitives/Checkbox";
import { Select } from "../ui/primitives/Select";
import { LabeledSlider } from "../ui/primitives/Slider";
import { Menu } from "../ui/primitives/Menu";
import { SegmentedGroup } from "../ui/primitives/SegmentedGroup";
import { KeyValueRow } from "../ui/primitives/KeyValueRow";
import { RelativeTimestamp } from "../ui/primitives/RelativeTimestamp";
import { DurationDisplay } from "../ui/primitives/DurationDisplay";
import { NodeChip } from "../ui/primitives/NodeChip";
import { ProcessIdenticon } from "../ui/primitives/ProcessIdenticon";
import { Table, type Column } from "../ui/primitives/Table";
import { ActionButton } from "../ui/primitives/ActionButton";

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

export function LabView() {
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

  const tableRows = useMemo<DemoConnectionRow[]>(() => [
    {
      id: "conn-01",
      healthLabel: "Healthy",
      healthTone: "ok",
      connectionKind: "connection",
      connectionLabel: "example-roam-rpc-stuck-request: initiator→acceptor",
      pending: 0,
      lastRecvBasis: "P",
      lastRecvBasisLabel: "process started",
      lastRecvBasisTime: "2026-02-17T10:05:00.000Z",
      lastRecvEventTime: "2026-02-17T10:05:12.000Z",
      lastRecvTone: "ok",
      lastSentBasis: "N",
      lastSentBasisLabel: "node created",
      lastSentBasisTime: "2026-02-17T10:05:12.000Z",
      lastSentEventTime: "2026-02-17T10:05:10.000Z",
      lastSentTone: "ok",
    },
    {
      id: "conn-02",
      healthLabel: "Warning",
      healthTone: "warn",
      connectionKind: "connection",
      connectionLabel: "vx-store · channel.v1.mpsc.send",
      pending: 3,
      lastRecvBasis: "P",
      lastRecvBasisLabel: "process started",
      lastRecvBasisTime: "2026-02-17T10:05:00.000Z",
      lastRecvEventTime: "2026-02-17T10:04:38.000Z",
      lastRecvTone: "warn",
      lastSentBasis: "N",
      lastSentBasisLabel: "connection opened",
      lastSentBasisTime: "2026-02-17T10:03:20.000Z",
      lastSentEventTime: "2026-02-17T10:04:10.000Z",
      lastSentTone: "warn",
    },
    {
      id: "conn-03",
      healthLabel: "Critical",
      healthTone: "crit",
      connectionKind: "request",
      connectionLabel: "example-roam-rpc-stuck-request · DemoRpc.sleepy_forever",
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
      healthLabel: "Warning",
      healthTone: "warn",
      connectionKind: "resource",
      connectionLabel: "vxd · connection: initiator<->acceptor",
      pending: 8,
      lastRecvBasis: "P",
      lastRecvBasisLabel: "process started",
      lastRecvBasisTime: "2026-02-17T10:05:00.000Z",
      lastRecvEventTime: "2026-02-17T10:03:30.000Z",
      lastRecvTone: "warn",
      lastSentBasis: "N",
      lastSentBasisLabel: "resource created",
      lastSentBasisTime: "2026-02-17T10:03:30.000Z",
      lastSentEventTime: null,
      lastSentTone: "warn",
    },
    {
      id: "conn-05",
      healthLabel: "Healthy",
      healthTone: "ok",
      connectionKind: "connection",
      connectionLabel: "vx-vfsd · net.readable.wait",
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
    { key: "health", label: "Health", sortable: true, width: "60px", render: (row) => <Badge tone={row.healthTone}>{row.healthLabel}</Badge> },
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
      if (row.lastSentEventTime === null) return <span>—</span>;
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
    const healthOrder = {
      healthy: 1,
      warning: 2,
      critical: 3,
      ok: 1,
      warn: 2,
      crit: 3,
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
        <Section title="Typography" subtitle="UI and mono fonts in the sizes we actually use">
          <div className="ui-typography-grid">
            <div className="ui-typo-card">
              <div className="ui-typo-kicker">UI font</div>
              <div className="ui-typo-fontname ui-typo-ui">Inter</div>
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

            <div className="ui-typo-card">
              <div className="ui-typo-kicker">Mono font</div>
              <div className="ui-typo-fontname ui-typo-mono">JetBrains Mono</div>
              <div className="ui-typo-sample ui-typo-mono ui-typo-mono--xl">request:01KHNGCY…</div>
              <div className="ui-typo-sample ui-typo-mono ui-typo-mono--md">connection: initiator-&gt;acceptor</div>
              <div className="ui-typo-sample ui-typo-mono ui-typo-mono--sm ui-typo-muted">
                IDs, paths, tokens, and anything users copy/paste.
              </div>
              <div className="ui-typo-weights">
                <span className="ui-typo-pill ui-typo-mono ui-typo-w-400">400</span>
                <span className="ui-typo-pill ui-typo-mono ui-typo-w-700">700</span>
              </div>
            </div>
          </div>
        </Section>

        <Section title="Buttons" subtitle="Hierarchy and states">
          <Row>
            <Button type="button">Default</Button>
            <Button type="button" variant="primary">Primary</Button>
            <Button type="button" disabled>Disabled</Button>
            <Button type="button">
              <WarningCircle size={14} weight="bold" />
              With icon
            </Button>
            <Button type="button">
              <Check size={14} weight="bold" />
              Success
            </Button>
          </Row>
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

        <Section title="Inputs" subtitle="Text, search, checkbox, select, slider">
          <div className="ui-section-stack">
          <Row className="ui-row--field-grid">
            <TextInput
              value={textValue}
              onChange={setTextValue}
              placeholder="Type…"
              aria-label="Text input"
            />
            <SearchInput
              value={searchValue}
              onChange={setSearchValue}
              placeholder="Search…"
              aria-label="Search input"
              items={searchResults.map((item) => ({
                id: item.id,
                label: item.label,
                meta: `${item.kind} · ${item.process}`,
              }))}
              showSuggestions={showSearchResults}
              selectedId={selectedSearchId}
              resultHint={
                <>
                  <span>{searchResults.length} result(s)</span>
                  <span className="ui-search-results-hint">click to select · alt+click to filter only this kind</span>
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
          </Row>
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
          </Row>
          </div>
        </Section>

        <Section title="Menu / Dropdown" subtitle="For filters and chip actions">
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
            <div className="ui-lab-hint">Mode: {segmentedMode}</div>
          </div>
          <div className="ui-section-stack">
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
            <div className="ui-lab-hint">Severity: {segmentedSeverity}</div>
          </div>
        </Section>

        <Section title="Key-Value Rows" subtitle="Inspector-like metadata rows">
          <div className="ui-section-stack">
            <KeyValueRow label="Method" labelWidth={80}>
              DemoRpc.sleepy_forever
            </KeyValueRow>
            <KeyValueRow label="Status" labelWidth={80}>
              <Badge tone="warn">IN_FLIGHT</Badge>
            </KeyValueRow>
            <KeyValueRow label="Elapsed" labelWidth={80}>
              <DurationDisplay ms={1245000} tone="crit" />
            </KeyValueRow>
            <KeyValueRow label="Connection" labelWidth={80}>
              <NodeChip
                kind="connection"
                label="initiator→acceptor"
                onClick={() => console.log("inspect initiator→acceptor")}
                onContextMenu={(event) => {
                  event.preventDefault();
                  console.log("open context for initiator→acceptor");
                }}
              />
            </KeyValueRow>
            <KeyValueRow label="Opened" labelWidth={80}>
              <RelativeTimestamp
                basis="P"
                basisLabel="process started"
                basisTime="2026-02-17T10:06:00.000Z"
                eventTime="2026-02-17T10:06:06.000Z"
              />
            </KeyValueRow>
            <KeyValueRow label="Closed" labelWidth={80}>
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
          <div className="ui-section-stack">
            <RelativeTimestamp basis="P" basisLabel="6 seconds after process start" basisTime="2026-02-17T10:00:00.000Z" eventTime="2026-02-17T10:00:06.000Z" />
            <RelativeTimestamp basis="P" basisLabel="2 minutes 30 seconds after process start" basisTime="2026-02-17T10:00:00.000Z" eventTime="2026-02-17T10:02:30.000Z" />
            <RelativeTimestamp basis="N" basisLabel="node just created" basisTime="2026-02-17T10:00:30.000Z" eventTime="2026-02-17T10:00:30.000Z" tone="ok" />
            <RelativeTimestamp basis="N" basisLabel="1m5s after node open" basisTime="2026-02-17T10:00:30.000Z" eventTime="2026-02-17T10:01:35.000Z" tone="warn" />
            <RelativeTimestamp basis="N" basisLabel="stuck for 20 minutes" basisTime="2026-02-17T10:00:30.000Z" eventTime="2026-02-17T10:21:15.000Z" tone="crit" />
            <RelativeTimestamp basis="N" basisLabel="sub-second timing check" basisTime="2026-02-17T10:00:30.000Z" eventTime="2026-02-17T10:00:30.145Z" />
          </div>
        </Section>

        <Section title="Duration Display" subtitle="Automatic semantic coloring by magnitude">
          <div className="ui-section-stack">
            <DurationDisplay ms={200} />
            <DurationDisplay ms={6200} />
            <DurationDisplay ms={45000} />
            <DurationDisplay ms={150000} />
            <DurationDisplay ms={1245000} />
            <DurationDisplay ms={4320000} />
          </div>
        </Section>

        <Section title="Node Chips" subtitle="Inline clickable node/resource references">
          <div className="ui-section-stack">
            <NodeChip
              kind="connection"
              label="initiator→acceptor:acceptor←→initiator"
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
              kind="channel"
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
            <div className="ui-lab-hint">Left-click to navigate, right-click for actions</div>
          </div>
        </Section>

        <Section title="Process Identicons" subtitle="Name-derived 5x5 process avatars">
          <div className="ui-section-stack">
            {[16, 20, 28].map((size) => (
              <div key={size} className="ui-identicon-size-group">
                <div className="ui-identicon-size-label">{size}px</div>
                <div className="ui-identicon-row">
                  {processIdenticonNames.map((name) => (
                    <span key={`${size}-${name}`} className="ui-identicon-cell">
                      <ProcessIdenticon name={name} size={size} />
                      <span>{name}</span>
                    </span>
                  ))}
                </div>
              </div>
            ))}
          </div>
        </Section>

        <Section title="Table" subtitle="Sortable, sticky header, selectable rows">
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

        <Section title="Action Buttons" subtitle="Compact utility action buttons">
          <div className="ui-section-stack">
            <div className="ui-row">
              <ActionButton>Show raw</ActionButton>
              <ActionButton>Focus</ActionButton>
              <ActionButton>Copy JSON</ActionButton>
            </div>
            <div className="ui-row">
              <ActionButton>
                <CopySimple size={12} weight="bold" />
                Copy
              </ActionButton>
              <ActionButton>
                <ArrowSquareOut size={12} weight="bold" />
                Open
              </ActionButton>
            </div>
            <div className="ui-row">
              <ActionButton variant="ghost">clear filter</ActionButton>
              <ActionButton variant="ghost">reset</ActionButton>
            </div>
            <ActionButton
              size="sm"
              aria-label="Copy"
            >
              <CopySimple size={12} weight="bold" />
            </ActionButton>
          </div>
        </Section>

      </div>
    </Panel>
  );
}
