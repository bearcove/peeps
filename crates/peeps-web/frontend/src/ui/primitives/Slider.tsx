import type React from "react";
import {
  Label,
  Slider as AriaSlider,
  SliderOutput,
  SliderThumb,
  SliderTrack,
} from "react-aria-components";

export function Slider({
  label,
  valueLabel,
  value,
  min,
  max,
  step,
  onChange,
}: {
  label: React.ReactNode;
  valueLabel?: React.ReactNode;
  value: number;
  min: number;
  max: number;
  step?: number;
  onChange: (value: number) => void;
}) {
  return (
    <AriaSlider
      className="ui-slider"
      value={value}
      minValue={min}
      maxValue={max}
      step={step ?? 1}
      onChange={(next) => onChange(Number(next))}
    >
      <div className="ui-slider-head">
        <Label className="ui-slider-label">{label}</Label>
        {valueLabel != null && <SliderOutput className="ui-slider-value">{valueLabel}</SliderOutput>}
      </div>
      <SliderTrack className="ui-slider-track">
        {({ state }) => (
          <>
            <div className="ui-slider-track-fill" style={{ width: `${state.getThumbPercent(0) * 100}%` }} />
            <SliderThumb className="ui-slider-thumb" />
          </>
        )}
      </SliderTrack>
    </AriaSlider>
  );
}
