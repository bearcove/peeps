import { useMemo } from "react";
import { Flame, MinusCircle, PlusCircle, WarningCircle } from "@phosphor-icons/react";

export type LabTone = "neutral" | "ok" | "warn" | "crit";

function parseTone(tone: string): LabTone {
  if (tone === "ok" || tone === "warn" || tone === "crit") return tone;
  return "neutral";
}

function toneLabel(tone: LabTone): string {
  if (tone === "ok") return "OK";
  if (tone === "warn") return "WARN";
  if (tone === "crit") return "CRIT";
  return "NEUTRAL";
}

function toneIcon(tone: LabTone) {
  if (tone === "ok") return <PlusCircle size={14} weight="bold" />;
  if (tone === "warn") return <WarningCircle size={14} weight="bold" />;
  if (tone === "crit") return <Flame size={14} weight="bold" />;
  return <MinusCircle size={14} weight="bold" />;
}

export function LabView({
  tone,
  onToneChange,
}: {
  tone: LabTone;
  onToneChange: (tone: LabTone) => void;
}) {
  const toneClass = useMemo(() => `pill--${parseTone(tone)}`, [tone]);

  return (
    <div className="panel panel--lab">
      <div className="panel-header">
        Lab
        <span className="lab-header-hint">Primitives and tone language</span>
      </div>
      <div className="lab-body">
        <div className="lab-section">
          <div className="lab-section-head">
            <span>Tone</span>
            <span className="lab-section-subhead">Controls the “current tone” samples</span>
          </div>
          <div className="lab-row">
            {(["neutral", "ok", "warn", "crit"] as const).map((t) => (
              <button
                key={t}
                type="button"
                className={`btn lab-tone-btn${tone === t ? " btn--primary" : ""}`}
                onClick={() => onToneChange(t)}
                title={`Set tone to ${t}`}
              >
                {toneIcon(t)}
                {toneLabel(t)}
              </button>
            ))}
          </div>
        </div>

        <div className="lab-section">
          <div className="lab-section-head">
            <span>Buttons</span>
            <span className="lab-section-subhead">Hierarchy and states</span>
          </div>
          <div className="lab-row">
            <button type="button" className="btn">Default</button>
            <button type="button" className="btn btn--primary">Primary</button>
            <button type="button" className="btn" disabled>Disabled</button>
            <button type="button" className="btn">
              <WarningCircle size={14} weight="bold" />
              With icon
            </button>
          </div>
        </div>

        <div className="lab-section">
          <div className="lab-section-head">
            <span>Pills, Badges, Counters</span>
            <span className="lab-section-subhead">Scan-friendly tokens</span>
          </div>
          <div className="lab-row">
            <span className={`pill ${toneClass}`}>Current: {toneLabel(tone)}</span>
            <span className="pill pill--neutral">NEUTRAL</span>
            <span className="pill pill--ok">OK</span>
            <span className="pill pill--warn">WARN</span>
            <span className="pill pill--crit">CRIT</span>
          </div>
          <div className="lab-row">
            <span className="badge">badge</span>
            <span className="badge badge--crit">badge--crit</span>
            <span className="lab-counter">0</span>
            <span className="lab-counter lab-counter--warn">7</span>
            <span className="lab-counter lab-counter--crit">118</span>
          </div>
        </div>
      </div>
    </div>
  );
}
