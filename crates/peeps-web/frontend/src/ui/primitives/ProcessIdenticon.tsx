import type React from "react";

function djb2(value: string): number {
  let h = 5381;
  for (let i = 0; i < value.length; i++) {
    h = ((h << 5) + h + value.charCodeAt(i)) >>> 0;
  }
  return h;
}

function makeForegroundColor(processName: string): string {
  const hash = djb2(processName);
  const hue = hash % 360;
  const saturation = 45 + (hash % 21);
  const isDarkMode = typeof window !== "undefined"
    && window.matchMedia("(prefers-color-scheme: dark)").matches;
  const lightness = isDarkMode ? 55 + ((hash >> 8) % 11) : 45 + ((hash >> 8) % 11);
  return `hsl(${hue} ${saturation}% ${lightness}%)`;
}

export type ProcessIdenticonProps = {
  name: string;
  size?: number;
}

export function ProcessIdenticon({ name, size = 20 }: ProcessIdenticonProps) {
  const hash = djb2(name);
  const fill = makeForegroundColor(name);
  const pixels: Array<{ x: number; y: number }> = [];

  for (let row = 0; row < 5; row++) {
    for (let col = 0; col < 3; col++) {
      const bitIndex = row * 3 + col;
      const on = ((hash >>> bitIndex) & 1) === 1;
      if (!on) continue;
      const mirrorColumn = col === 2 ? 2 : 4 - col;
      pixels.push({ x: col, y: row });
      if (col !== 2) pixels.push({ x: mirrorColumn, y: row });
    }
  }

  return (
    <svg
      className="ui-process-identicon"
      width={size}
      height={size}
      viewBox="0 0 5 5"
      role="img"
      aria-label={name}
      title={name}
      style={{ flexShrink: 0 }}
    >
      {pixels.map((pixel) => (
        <rect
          key={`${pixel.x}-${pixel.y}`}
          x={pixel.x}
          y={pixel.y}
          width={1}
          height={1}
          fill={fill}
        />
      ))}
    </svg>
  );
}
