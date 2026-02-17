import { FileRs, Hash, LinkSimple, PaperPlaneTilt, Users } from "@phosphor-icons/react";
import { KeyValueRow } from "../ui/primitives/KeyValueRow";
import { NodeChip } from "../ui/primitives/NodeChip";
import { ProcessIdenticon } from "../ui/primitives/ProcessIdenticon";

const THIRTY_DAYS_NS = 30 * 24 * 60 * 60 * 1_000_000_000;
export type InspectorProcessAction = "show_only" | "hide";

function parseFiniteNumber(value: unknown): number | null {
  if (typeof value === "number" && Number.isFinite(value)) return value;
  if (typeof value === "string" && value.trim() !== "") {
    const parsed = Number(value);
    if (Number.isFinite(parsed)) return parsed;
  }
  return null;
}

export function normalizeTimestampToNs(value: number): number {
  if (!Number.isFinite(value) || value <= 0) return value;
  if (value < 100_000_000_000) return value * 1_000_000_000;
  if (value < 100_000_000_000_000) return value * 1_000_000;
  if (value < 100_000_000_000_000_000) return value * 1_000;
  return value;
}

export function getSource(attrs: Record<string, unknown>): string | undefined {
  const value = attrs.source;
  if (typeof value !== "string") return undefined;
  const trimmed = value.trim();
  return trimmed === "" ? undefined : trimmed;
}

export function getMethod(attrs: Record<string, unknown>): string | undefined {
  const value = attrs.method;
  if (typeof value !== "string") return undefined;
  const trimmed = value.trim();
  return trimmed === "" ? undefined : trimmed;
}

export function getCorrelation(attrs: Record<string, unknown>): string | undefined {
  const value = attrs.correlation;
  if (typeof value !== "string") return undefined;
  const trimmed = value.trim();
  return trimmed === "" ? undefined : trimmed;
}

export function getCreatedAtNs(attrs: Record<string, unknown>): number | undefined {
  const raw = parseFiniteNumber(attrs.created_at);
  if (raw == null || raw <= 0) return undefined;
  return normalizeTimestampToNs(raw);
}

export function resolveTimelineOriginNs(
  attrs: Record<string, unknown>,
  firstEventTsNs: number | null,
): number | null {
  const normalizedFirstEvent =
    firstEventTsNs != null && Number.isFinite(firstEventTsNs) && firstEventTsNs > 0
      ? normalizeTimestampToNs(firstEventTsNs)
      : null;
  const createdAtNs = getCreatedAtNs(attrs);

  if (createdAtNs == null) return normalizedFirstEvent;
  if (normalizedFirstEvent == null) return createdAtNs;
  if (createdAtNs > normalizedFirstEvent) return normalizedFirstEvent;
  if (normalizedFirstEvent - createdAtNs > THIRTY_DAYS_NS) return normalizedFirstEvent;
  return createdAtNs;
}

export function formatRelativeTimestampFromOrigin(tsNs: number, originNs: number | null): string {
  if (originNs == null) return formatTimelineTimestamp(tsNs);
  return formatShortDurationNs(tsNs - originNs);
}

function sourceDisplayName(location: string): string {
  const match = location.match(/^(.*[\\/])?([^\\/]+?):(\d+)(?::\d+)?$/);
  if (match) return `${match[2]}:${match[3]}`;
  const lastSlash = Math.max(location.lastIndexOf("/"), location.lastIndexOf("\\"));
  return lastSlash >= 0 ? location.slice(lastSlash + 1) : location;
}

function isFileLikeSource(source: string): boolean {
  return /[\\/]/.test(source) || /\.(rs|ts|tsx|js|jsx|go|py|java|swift|c|cc|cpp|h|hpp|kt)(:\d+)?(?::\d+)?$/i.test(source);
}

function formatTimelineTimestamp(tsNs: number): string {
  if (!Number.isFinite(tsNs)) return "—";
  const date = new Date(Math.floor(tsNs / 1_000_000));
  const micros = Math.floor((tsNs % 1_000_000) / 1_000);
  return `${date.toLocaleTimeString()}.${String(micros).padStart(3, "0")}`;
}

function formatShortDurationNs(deltaNs: number): string {
  const abs = Math.abs(deltaNs);
  const sign = deltaNs >= 0 ? "+" : "-";
  if (abs >= 1_000_000_000) return `${sign}${(abs / 1_000_000_000).toFixed(3)}s`;
  if (abs >= 1_000_000) return `${sign}${Math.round(abs / 1_000_000)}ms`;
  return `${sign}${Math.round(abs / 1_000)}us`;
}

export function CommonInspectorFields({
  id,
  process,
  attrs,
  onProcessAction,
}: {
  id: string;
  process: string;
  attrs: Record<string, unknown>;
  onProcessAction?: (action: InspectorProcessAction, process: string) => void;
}) {
  const method = getMethod(attrs);
  const correlation = getCorrelation(attrs);
  const source = getSource(attrs);

  return (
    <div className="inspect-section" data-testid="common-fields">
      <KeyValueRow
        label="ID"
        icon={<Hash size={12} weight="bold" />}
        className="common-field-id"
      >
        <span className="inspect-val inspect-val--copyable">
          <span className="inspect-val-copy-text" title={id}>
            {id}
          </span>
        </span>
      </KeyValueRow>
      <KeyValueRow
        label="Process"
        icon={<Users size={12} weight="bold" />}
        className="common-field-process"
      >
        {onProcessAction ? (
          <span className="inspect-val inspect-process-menu-wrap">
            <NodeChip
              label={process}
              icon={<ProcessIdenticon name={process} size={12} />}
              onClick={() => onProcessAction("show_only", process)}
              title={`Show only process ${process}`}
            />
            <details className="inspect-process-menu">
              <summary
                className="inspect-process-chip"
                aria-label={`Process actions for ${process}`}
                title="Process actions"
              >
                actions
              </summary>
              <div className="inspect-process-dropdown">
                <button
                  type="button"
                  className="inspect-process-action"
                  onClick={() => onProcessAction("show_only", process)}
                >
                  Show only this process
                </button>
                <button
                  type="button"
                  className="inspect-process-action"
                  onClick={() => onProcessAction("hide", process)}
                >
                  Hide this process
                </button>
              </div>
            </details>
          </span>
        ) : (
          <span className="inspect-val">
            <NodeChip
              label={process}
              icon={<ProcessIdenticon name={process} size={12} />}
              onClick={() => undefined}
              title={process}
            />
          </span>
        )}
      </KeyValueRow>
      {method && (
        <KeyValueRow
          label="Method"
          icon={<PaperPlaneTilt size={12} weight="bold" />}
          className="common-field-method"
        >
          <span className="inspect-val inspect-val--mono">{method}</span>
        </KeyValueRow>
      )}
      {correlation && (
        <KeyValueRow
          label="Correlation"
          icon={<LinkSimple size={12} weight="bold" />}
          className="common-field-correlation"
        >
          <span className="inspect-val inspect-val--mono">{correlation}</span>
        </KeyValueRow>
      )}
      {source && (
        <KeyValueRow
          label="Source"
          className="inspect-source-row"
        >
          {isFileLikeSource(source) ? (
            <NodeChip
              icon={<FileRs size={12} weight="bold" />}
              label={sourceDisplayName(source)}
              href={`zed://file/${encodeURIComponent(source)}`}
              title={source}
            />
          ) : (
            <span className="inspect-val--mono">{source}</span>
          )}
        </KeyValueRow>
      )}
    </div>
  );
}
