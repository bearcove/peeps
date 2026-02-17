import { useMemo, useState } from "react";
import {
  WarningCircle,
  CaretDown,
  Check,
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
import { Slider } from "../ui/primitives/Slider";
import { Menu } from "../ui/primitives/Menu";

export function LabView() {
  const [textValue, setTextValue] = useState("Hello");
  const [searchValue, setSearchValue] = useState("");
  const [checked, setChecked] = useState(true);
  const [selectValue, setSelectValue] = useState("all");
  const [sliderValue, setSliderValue] = useState(1);
  const [searchOnlyKind, setSearchOnlyKind] = useState<string | null>(null);
  const [selectedSearchId, setSelectedSearchId] = useState<string | null>(null);
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
              <div className="ui-typo-fontname ui-typo-ui">Manrope</div>
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
              <div className="ui-typo-fontname ui-typo-mono">Space Mono</div>
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
            <Slider
              value={sliderValue}
              min={0}
              max={2}
              step={1}
              onChange={(v) => setSliderValue(v)}
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

      </div>
    </Panel>
  );
}
