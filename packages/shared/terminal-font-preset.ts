const BUNDLED_NERD_FONT_FALLBACK = '"MesloLGL Nerd Font Mono"';

function withBundledNerdFontFallback(stack: string): string {
  return `${stack}, ${BUNDLED_NERD_FONT_FALLBACK}, monospace`;
}

export const MONOSPACE_TERMINAL_FONT_FAMILY = withBundledNerdFontFallback("monospace");
export const UI_MONOSPACE_TERMINAL_FONT_FAMILY = withBundledNerdFontFallback("ui-monospace");
export const MESLO_TERMINAL_FONT_FAMILY =
  '"MesloLGL Nerd Font Mono", Menlo, Monaco, "Courier New", monospace';
export const CROSS_PLATFORM_MONO_TERMINAL_FONT_FAMILY = withBundledNerdFontFallback(
  'Consolas, Menlo, Monaco, "Liberation Mono", "DejaVu Sans Mono", "Courier New"',
);
export const CONSOLAS_TERMINAL_FONT_FAMILY = withBundledNerdFontFallback('Consolas, "Courier New"');
export const MENLO_TERMINAL_FONT_FAMILY = withBundledNerdFontFallback("Menlo");
export const MONACO_TERMINAL_FONT_FAMILY = withBundledNerdFontFallback('Monaco, "Courier New"');
export const DROID_SANS_MONO_TERMINAL_FONT_FAMILY = withBundledNerdFontFallback(
  '"Droid Sans Mono", "monospace"',
);
export const LIBERATION_MONO_TERMINAL_FONT_FAMILY =
  withBundledNerdFontFallback('"Liberation Mono"');
export const DEJAVU_SANS_MONO_TERMINAL_FONT_FAMILY =
  withBundledNerdFontFallback('"DejaVu Sans Mono"');
export const COURIER_NEW_TERMINAL_FONT_FAMILY = withBundledNerdFontFallback('"Courier New"');
export const CASCADIA_MONO_TERMINAL_FONT_FAMILY = withBundledNerdFontFallback('"Cascadia Mono"');
export const CASCADIA_CODE_TERMINAL_FONT_FAMILY = withBundledNerdFontFallback('"Cascadia Code"');
export const JETBRAINS_MONO_TERMINAL_FONT_FAMILY = withBundledNerdFontFallback('"JetBrains Mono"');
export const FIRA_CODE_TERMINAL_FONT_FAMILY = withBundledNerdFontFallback('"Fira Code"');
export const SOURCE_CODE_PRO_TERMINAL_FONT_FAMILY =
  withBundledNerdFontFallback('"Source Code Pro"');
export const IBM_PLEX_MONO_TERMINAL_FONT_FAMILY = withBundledNerdFontFallback('"IBM Plex Mono"');
export const ROBOTO_MONO_TERMINAL_FONT_FAMILY = withBundledNerdFontFallback('"Roboto Mono"');
export const NOTO_SANS_MONO_TERMINAL_FONT_FAMILY = withBundledNerdFontFallback('"Noto Sans Mono"');
export const UBUNTU_MONO_TERMINAL_FONT_FAMILY = withBundledNerdFontFallback('"Ubuntu Mono"');

export const TERMINAL_FONT_PRESETS = [
  { preset: "Monospace", fontFamily: MONOSPACE_TERMINAL_FONT_FAMILY },
  { preset: "UI Monospace", fontFamily: UI_MONOSPACE_TERMINAL_FONT_FAMILY },
  { preset: "Meslo", fontFamily: MESLO_TERMINAL_FONT_FAMILY },
  { preset: "Cross Platform Mono", fontFamily: CROSS_PLATFORM_MONO_TERMINAL_FONT_FAMILY },
  { preset: "Consolas (Windows Default)", fontFamily: CONSOLAS_TERMINAL_FONT_FAMILY },
  { preset: "Menlo", fontFamily: MENLO_TERMINAL_FONT_FAMILY },
  { preset: "Monaco (macOS Default)", fontFamily: MONACO_TERMINAL_FONT_FAMILY },
  { preset: "Droid Sans Mono (Linux Default)", fontFamily: DROID_SANS_MONO_TERMINAL_FONT_FAMILY },
  { preset: "Liberation Mono", fontFamily: LIBERATION_MONO_TERMINAL_FONT_FAMILY },
  { preset: "DejaVu Sans Mono", fontFamily: DEJAVU_SANS_MONO_TERMINAL_FONT_FAMILY },
  { preset: "Courier New", fontFamily: COURIER_NEW_TERMINAL_FONT_FAMILY },
  { preset: "Cascadia Mono", fontFamily: CASCADIA_MONO_TERMINAL_FONT_FAMILY },
  { preset: "Cascadia Code", fontFamily: CASCADIA_CODE_TERMINAL_FONT_FAMILY },
  { preset: "JetBrains Mono", fontFamily: JETBRAINS_MONO_TERMINAL_FONT_FAMILY },
  { preset: "Fira Code", fontFamily: FIRA_CODE_TERMINAL_FONT_FAMILY },
  { preset: "Source Code Pro", fontFamily: SOURCE_CODE_PRO_TERMINAL_FONT_FAMILY },
  { preset: "IBM Plex Mono", fontFamily: IBM_PLEX_MONO_TERMINAL_FONT_FAMILY },
  { preset: "Roboto Mono", fontFamily: ROBOTO_MONO_TERMINAL_FONT_FAMILY },
  { preset: "Noto Sans Mono", fontFamily: NOTO_SANS_MONO_TERMINAL_FONT_FAMILY },
  { preset: "Ubuntu Mono", fontFamily: UBUNTU_MONO_TERMINAL_FONT_FAMILY },
] as const;

export type TerminalFontPreset = (typeof TERMINAL_FONT_PRESETS)[number]["preset"];

export const DEFAULT_TERMINAL_FONT_PRESET: TerminalFontPreset = "JetBrains Mono";

const normalizeComparableValue = (value: string): string =>
  value
    .trim()
    .replace(/\s+/g, " ")
    .replace(/^['"]|['"]$/g, "")
    .toLowerCase();

const TERMINAL_FONT_PRESET_BY_NORMALIZED_VALUE = new Map<string, TerminalFontPreset>(
  TERMINAL_FONT_PRESETS.flatMap(({ fontFamily, preset }) => [
    [normalizeComparableValue(preset), preset],
    [normalizeComparableValue(fontFamily), preset],
  ]),
);

TERMINAL_FONT_PRESET_BY_NORMALIZED_VALUE.set("ui-monospace", "UI Monospace");
TERMINAL_FONT_PRESET_BY_NORMALIZED_VALUE.set("consolas", "Consolas (Windows Default)");
TERMINAL_FONT_PRESET_BY_NORMALIZED_VALUE.set("monaco", "Monaco (macOS Default)");
TERMINAL_FONT_PRESET_BY_NORMALIZED_VALUE.set("droid sans mono", "Droid Sans Mono (Linux Default)");

export function normalizeTerminalFontPreset(value: string | undefined): TerminalFontPreset {
  const normalizedValue = normalizeComparableValue(value ?? "");
  return (
    TERMINAL_FONT_PRESET_BY_NORMALIZED_VALUE.get(normalizedValue) ?? DEFAULT_TERMINAL_FONT_PRESET
  );
}

export function getTerminalFontFamilyForPreset(preset: TerminalFontPreset): string {
  return (
    TERMINAL_FONT_PRESETS.find((candidate) => candidate.preset === preset)?.fontFamily ??
    MONOSPACE_TERMINAL_FONT_FAMILY
  );
}

/**
 * CDXC:TerminalSettings 2026-04-26-18:36
 * ghostex terminal font choices now write into the user's real Ghostty config.
 * Ghostty expects a single font family name, while the web UI uses CSS fallback
 * stacks, so sync the first concrete family instead of copying browser syntax.
 */
export function getGhosttyFontFamilyForPreset(preset: TerminalFontPreset): string {
  const fontFamily = getTerminalFontFamilyForPreset(preset);
  return (
    fontFamily
      .split(",")
      .map((part) => part.trim().replace(/^['"]|['"]$/g, ""))
      .find((part) => part && part !== "monospace" && part !== "ui-monospace") ?? "monospace"
  );
}

export function getTerminalFontPresetFromFontFamily(
  fontFamily: string | undefined,
): TerminalFontPreset {
  return normalizeTerminalFontPreset(fontFamily);
}
