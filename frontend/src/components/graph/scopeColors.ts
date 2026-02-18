// ColorBrewer palettes from RColorBrewer (internet-sourced).
// Source: https://rdrr.io/cran/RColorBrewer/src/R/ColorBrewer.R
// Light mode: Pastel1 + Pastel2 + Set3 (excluding neutral greys).
export const SCOPE_LIGHT_RGB = [
  [251, 180, 174],
  [179, 205, 227],
  [204, 235, 197],
  [222, 203, 228],
  [254, 217, 166],
  [255, 255, 204],
  [229, 216, 189],
  [253, 218, 236],
  [179, 226, 205],
  [253, 205, 172],
  [203, 213, 232],
  [244, 202, 228],
  [230, 245, 201],
  [255, 242, 174],
  [241, 226, 204],
  [141, 211, 199],
  [255, 255, 179],
  [190, 186, 218],
  [251, 128, 114],
  [128, 177, 211],
  [253, 180, 98],
  [179, 222, 105],
  [252, 205, 229],
  [188, 128, 189],
  [204, 235, 197],
  [255, 237, 111],
] as const;

// Dark mode: muted low-saturation variant tuned for dark surfaces.
export const SCOPE_DARK_RGB = [
  [69, 102, 92],
  [140, 84, 63],
  [83, 93, 117],
  [139, 75, 114],
  [93, 112, 62],
  [121, 109, 54],
  [139, 115, 79],
  [42, 81, 69],
  [88, 60, 39],
  [75, 73, 96],
  [107, 51, 79],
  [62, 80, 42],
  [93, 80, 42],
  [81, 67, 42],
  [87, 122, 141],
  [42, 65, 81],
  [103, 132, 78],
  [47, 77, 46],
  [162, 74, 73],
  [100, 47, 48],
  [146, 111, 65],
  [102, 74, 46],
  [119, 98, 129],
  [62, 50, 74],
  [162, 162, 73],
  [81, 58, 45],
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
