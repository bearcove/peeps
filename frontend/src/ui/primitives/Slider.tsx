import type React from "react";
import {
  Slider as AriaSlider,
  SliderThumb,
  SliderTrack,
} from "react-aria-components";
import "./Slider.css";

export function Slider({
  className,
  value,
  min,
  max,
  step,
  onChange,
  "aria-label": ariaLabel,
}: {
  className?: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  onChange: (value: number) => void;
  "aria-label"?: string;
}) {
  return (
    <AriaSlider
      className={["ui-slider", className].filter(Boolean).join(" ")}
      value={value}
      minValue={min}
      maxValue={max}
      step={step ?? 1}
      onChange={(next) => onChange(Number(next))}
      aria-label={ariaLabel}
    >
      <SliderTrack className="ui-slider-track">
        {({ state }) => (
          <>
            <div className="ui-slider-track-rail">
              <div className="ui-slider-track-fill" style={{ width: `${state.getThumbPercent(0) * 100}%` }} />
            </div>
            <SliderThumb className="ui-slider-thumb" />
          </>
        )}
      </SliderTrack>
    </AriaSlider>
  );
}

export function LabeledSlider({
  label,
  valueLabel,
  className,
  ...sliderProps
}: {
  label: React.ReactNode;
  valueLabel?: React.ReactNode;
  className?: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  onChange: (value: number) => void;
  "aria-label"?: string;
}) {
  return (
    <div className={["ui-slider-panel", className].filter(Boolean).join(" ")}>
      <div className="ui-slider-head">
        <span className="ui-slider-label">{label}</span>
        {valueLabel != null && <span className="ui-slider-value">{valueLabel}</span>}
      </div>
      <Slider {...sliderProps} />
    </div>
  );
}
