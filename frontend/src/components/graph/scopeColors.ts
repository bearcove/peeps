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

// Dark mode: Set2 + Dark2 (excluding neutral greys) + Paired.
export const SCOPE_DARK_RGB = [
  [102, 194, 165],
  [252, 141, 98],
  [141, 160, 203],
  [231, 138, 195],
  [166, 216, 84],
  [255, 217, 47],
  [229, 196, 148],
  [27, 158, 119],
  [217, 95, 2],
  [117, 112, 179],
  [231, 41, 138],
  [102, 166, 30],
  [230, 171, 2],
  [166, 118, 29],
  [166, 206, 227],
  [31, 120, 180],
  [178, 223, 138],
  [51, 160, 44],
  [251, 154, 153],
  [227, 26, 28],
  [253, 191, 111],
  [255, 127, 0],
  [202, 178, 214],
  [106, 61, 154],
  [255, 255, 153],
  [177, 89, 40],
] as const;

function rgbTripletToCss([r, g, b]: readonly [number, number, number]): string {
  return `${r} ${g} ${b}`;
}

export type ScopeColorPair = {
  light: string;
  dark: string;
};

export function hashString(value: string): number {
  // FNV-1a 32-bit gives stable distribution for short scope keys.
  let h = 0x811c9dc5;
  for (let i = 0; i < value.length; i++) {
    h ^= value.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return h >>> 0;
}

export function assignScopeColorRgbByKey(scopeKeys: Iterable<string>): Map<string, ScopeColorPair> {
  const lightPalette = SCOPE_LIGHT_RGB.map(rgbTripletToCss);
  const darkPalette = SCOPE_DARK_RGB.map(rgbTripletToCss);
  const paletteSize = Math.min(lightPalette.length, darkPalette.length);
  const uniqueKeys = Array.from(new Set(scopeKeys)).filter((key) => key.length > 0);

  if (paletteSize === 0 || uniqueKeys.length === 0) return new Map();

  const entries = uniqueKeys
    .map((key) => {
      const hash = hashString(key);
      return {
        key,
        hash,
        bucket: hash % paletteSize,
      };
    })
    .sort((a, b) => a.hash - b.hash || a.key.localeCompare(b.key));

  const usedPaletteIndexes = new Set<number>();
  const assigned = new Map<string, ScopeColorPair>();

  // First pass: reserve native hash buckets where possible.
  for (const entry of entries) {
    if (!usedPaletteIndexes.has(entry.bucket)) {
      usedPaletteIndexes.add(entry.bucket);
      assigned.set(entry.key, {
        light: lightPalette[entry.bucket],
        dark: darkPalette[entry.bucket],
      });
    }
  }

  // Second pass: collision-free linear probing if we still have free colors.
  for (const entry of entries) {
    if (assigned.has(entry.key)) continue;
    let assignedIndex: number | null = null;
    for (let offset = 1; offset < paletteSize; offset++) {
      const candidate = (entry.bucket + offset) % paletteSize;
      if (!usedPaletteIndexes.has(candidate)) {
        assignedIndex = candidate;
        break;
      }
    }

    // If scopes exceed palette size, fall back to deterministic reuse.
    const index = assignedIndex ?? entry.bucket;
    usedPaletteIndexes.add(index);
    assigned.set(entry.key, {
      light: lightPalette[index],
      dark: darkPalette[index],
    });
  }

  return assigned;
}
