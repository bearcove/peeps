// Curated categorical palette for graph scopes.
// Keep this intentionally small and hue-separated to preserve legibility.
export const SCOPE_DARK_RGB = [
  [191, 224, 255],
  [207, 201, 255],
  [255, 214, 205],
  [252, 236, 186],
  [194, 237, 219],
  [255, 206, 227],
  [201, 235, 255],
  [224, 245, 180],
  [230, 208, 253],
  [255, 223, 180],
] as const;

// Dark mode: crisp, high-separation categorical colors tuned for dark surfaces.
export const SCOPE_LIGHT_RGB = [
  [79, 163, 255],
  [124, 108, 255],
  [255, 122, 92],
  [242, 201, 76],
  [90, 209, 164],
  [255, 93, 162],
  [107, 203, 255],
  [163, 230, 53],
  [192, 132, 252],
  [255, 159, 28],
] as const;

function rgbTripletToCss([r, g, b]: readonly [number, number, number]): string {
  return `${r} ${g} ${b}`;
}

export type ScopeColorPair = {
  light: string;
  dark: string;
};

export function assignScopeColorRgbByKey(scopeKeys: Iterable<string>): Map<string, ScopeColorPair> {
  const lightPalette = SCOPE_LIGHT_RGB.map(rgbTripletToCss);
  const darkPalette = SCOPE_DARK_RGB.map(rgbTripletToCss);
  const paletteSize = Math.min(lightPalette.length, darkPalette.length);
  const uniqueKeys = Array.from(new Set(scopeKeys))
    .filter((key) => key.length > 0)
    .sort((a, b) => a.localeCompare(b));

  if (paletteSize === 0 || uniqueKeys.length === 0) return new Map();

  const assigned = new Map<string, ScopeColorPair>();
  uniqueKeys.forEach((key, index) => {
    const paletteIndex = index % paletteSize;
    assigned.set(key, {
      light: lightPalette[paletteIndex],
      dark: darkPalette[paletteIndex],
    });
  });

  return assigned;
}
