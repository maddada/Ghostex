export const DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_FOREGROUND_COLOR = '#d8d8d8';
export const DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_DARK_FOREGROUND_COLOR = '#262626';
export const DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_COLOR = '#0b0b0b';
export const DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_TINT_COLOR = '#808080';
export const DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT = 96;
/*
 * CDXC:Theming 2026-06-28-08:01:
 * The tint scale keeps the original 95 reference so existing saved contrast
 * values do not darken or brighten when the app default changes.
 *
 * CDXC:Theming 2026-07-22:
 * New installs used the neutral #808080 tint at 93 Background Contrast,
 * resolving to #141414 while preserving the existing calibrated scale.
 *
 * CDXC:Theming 2026-09-08 DECISION:
 * User: default background contrast and color must match my current settings: 96 contrast and neutral #808080 tint, resolving to #0b0b0b.
 * This replaces the ice tint at 98 contrast default.
 * SEE-ALSO: apps/desktop/src/app/helpers/titlebar.rs and packages/core-ui/styles/theme.css.
 */
const CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_SCALE_REFERENCE_DARKNESS_PERCENT = 95;
// CDXC:Theming 2026-09-08 WHY:
// Custom tint calibration must stay stable when the first-run background default changes.
const CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_CALIBRATION_COLOR = '#040607';
export const MIN_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT = 85;
export const MAX_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT = 100;
const CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARK_TINTS: ReadonlyMap<string, string> = new Map([
  ['#000000', '#000000'],
  ['#ffffff', '#0e0e0e'],
  ['#808080', '#0e0e0e'],
  ['#88d7ff', '#0a0f12'],
  ['#4f6672', '#0c0e10'],
  ['#884444', '#0d0005'],
  ['#8a5330', '#100502'],
  ['#8a6a2f', '#110a02'],
  ['#657a3f', '#0c1005'],
  ['#3f7a5f', '#031006'],
  ['#2f7d66', '#03100c'],
  ['#287c7f', '#031011'],
  ['#336699', '#0c0e11'],
  ['#4f5f96', '#080912'],
  ['#6c4f8f', '#0a0611'],
  ['#854f7a', '#100611'],
  ['#8a4f5f', '#100409'],
  // 2026-09-25 preset colours (Slate, Midnight, Indigo, Teal, Forest, Olive, Amber, Rose).
  ['#4a6a8a', '#070d14'],
  ['#1f3a8a', '#02061a'],
  ['#4b4fa6', '#08081c'],
  ['#2f7f7f', '#021213'],
  ['#2e6a3a', '#031205'],
  ['#6b6b35', '#0e0f03'],
  ['#8a6a2a', '#130c02'],
  ['#8a4a5c', '#12040b'],
]);

export function normalizeSidebarTitlebarHexColor(value: string, fallback: string): string {
  const normalized = value.trim().toLowerCase();
  return /^#[0-9a-f]{6}$/u.test(normalized) ? normalized : fallback;
}

function clampColorChannel(value: number): number {
  return Math.min(255, Math.max(0, Math.round(value)));
}

type SidebarTitlebarRgbColor = {
  blue: number;
  green: number;
  red: number;
};

function parseSidebarTitlebarHexColor(color: string): SidebarTitlebarRgbColor {
  const normalized = normalizeSidebarTitlebarHexColor(color, DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_COLOR);
  return {
    red: Number.parseInt(normalized.slice(1, 3), 16),
    green: Number.parseInt(normalized.slice(3, 5), 16),
    blue: Number.parseInt(normalized.slice(5, 7), 16),
  };
}

function formatSidebarTitlebarHexColor(color: SidebarTitlebarRgbColor): string {
  return `#${[color.red, color.green, color.blue]
    .map((channel) => clampColorChannel(channel).toString(16).padStart(2, '0'))
    .join('')}`;
}

function scaleSidebarTitlebarVector(color: SidebarTitlebarRgbColor, amount: number): SidebarTitlebarRgbColor {
  return {
    red: color.red * amount,
    green: color.green * amount,
    blue: color.blue * amount,
  };
}

function addSidebarTitlebarColors(
  base: SidebarTitlebarRgbColor,
  offset: SidebarTitlebarRgbColor
): SidebarTitlebarRgbColor {
  return {
    red: base.red + offset.red,
    green: base.green + offset.green,
    blue: base.blue + offset.blue,
  };
}

function normalizedSidebarTitlebarTintDirection(background: SidebarTitlebarRgbColor): SidebarTitlebarRgbColor {
  const average = (background.red + background.green + background.blue) / 3;
  const direction = {
    red: background.red - average,
    green: background.green - average,
    blue: background.blue - average,
  };
  const magnitude = Math.max(Math.abs(direction.red), Math.abs(direction.green), Math.abs(direction.blue));
  if (magnitude < 0.5) {
    return {
      red: 0,
      green: 0,
      blue: 0,
    };
  }
  return scaleSidebarTitlebarVector(direction, 1 / magnitude);
}

export function clampSidebarTitlebarBackgroundDarknessPercent(value: number): number {
  if (!Number.isFinite(value)) {
    return DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT;
  }
  return Math.min(
    MAX_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT,
    Math.max(MIN_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT, Math.round(value))
  );
}

export function getSidebarTitlebarBackgroundDarknessForColor(backgroundColor: string): number {
  const background = normalizeSidebarTitlebarHexColor(
    backgroundColor,
    DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_COLOR
  );
  const red = Number.parseInt(background.slice(1, 3), 16);
  const green = Number.parseInt(background.slice(3, 5), 16);
  const blue = Number.parseInt(background.slice(5, 7), 16);
  const luminance = (0.2126 * red + 0.7152 * green + 0.0722 * blue) / 255;
  return clampSidebarTitlebarBackgroundDarknessPercent((1 - luminance) * 100);
}

function isNeutralSidebarTitlebarColor(color: SidebarTitlebarRgbColor): boolean {
  return Math.max(color.red, color.green, color.blue) - Math.min(color.red, color.green, color.blue) < 1;
}

function getSidebarTitlebarDefaultDarkTintBackground(tint: string): SidebarTitlebarRgbColor {
  const calibratedTintBackground = CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARK_TINTS.get(tint);
  if (calibratedTintBackground) {
    return parseSidebarTitlebarHexColor(calibratedTintBackground);
  }

  const color = parseSidebarTitlebarHexColor(tint);
  if (isNeutralSidebarTitlebarColor(color)) {
    return parseSidebarTitlebarHexColor(CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_CALIBRATION_COLOR);
  }

  const direction = normalizedSidebarTitlebarTintDirection(color);
  const base = parseSidebarTitlebarHexColor(CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_CALIBRATION_COLOR);
  return addSidebarTitlebarColors(base, scaleSidebarTitlebarVector(direction, 4));
}

function scaleSidebarTitlebarDefaultDarkTintBackground(
  background: SidebarTitlebarRgbColor,
  darknessPercent: number
): SidebarTitlebarRgbColor {
  if (darknessPercent === MAX_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT) {
    return { red: 0, green: 0, blue: 0 };
  }
  const defaultRange =
    MAX_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT -
    CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_SCALE_REFERENCE_DARKNESS_PERCENT;
  const scale = (MAX_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT - darknessPercent) / defaultRange;
  return {
    red: background.red * scale,
    green: background.green * scale,
    blue: background.blue * scale,
  };
}

export function getSidebarTitlebarBackgroundForDarkness(
  darknessPercent: number,
  tintColor = DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_TINT_COLOR
): string {
  /**
   * CDXC:Theming 2026-06-15-13:45:
   * Replace the freeform custom background color picker with a contrast slider.
   * The slider controls how strongly the calibrated dark tint background is
   * applied so custom chrome can vary in contrast without turning into
   * arbitrary bright sidebar colors.
   *
   * CDXC:Theming 2026-06-15-15:01:
   * Limit the contrast slider to 85-100 so custom chrome stays in the dark
   * gray range instead of drifting into mid-gray sidebar backgrounds.
   *
   * CDXC:Theming 2026-06-15-15:15:
   * Keep the internal darkness percentage name for compatibility while the
   * visible Settings control is labeled Background Contrast.
   *
   * CDXC:Theming 2026-06-15-15:28:
   * Add a web-only tint picker without returning to arbitrary background
   * colors. Map tint choices to dark applied backgrounds so tint changes are
   * subtle and neutral #808080 preserves the original gray.
   *
   * CDXC:Theming 2026-06-16-14:28:
   * Default custom chrome should now use 95 contrast with white tint. White
   * remains neutral in the calibrated tint table because all same-channel
   * tints should keep the sidebar/titlebar background gray.
   *
   * CDXC:Theming 2026-06-19-14:20:
   * Tint swatches stay visually legible in Settings, but applied custom chrome
   * should default to calibrated very-dark backgrounds such as #0d0005 for red
   * and #0c0e11 for blue. Scale those dark targets with the Contrast slider,
   * and keep same-channel tints such as white, black, and gray neutral instead
   * of adding a blue cast.
   */
  const darkness = clampSidebarTitlebarBackgroundDarknessPercent(darknessPercent);
  const tint = normalizeSidebarTitlebarHexColor(tintColor, DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_TINT_COLOR);
  const defaultDarkTintBackground = getSidebarTitlebarDefaultDarkTintBackground(tint);
  const background = scaleSidebarTitlebarDefaultDarkTintBackground(defaultDarkTintBackground, darkness);
  const channels = [background.red, background.green, background.blue].map(clampColorChannel);
  return `#${channels.map((channel) => channel.toString(16).padStart(2, '0')).join('')}`;
}

/**
 * CDXC:Theming 2026-09-21 DECISION:
 * User: remove the accent color setting and make the accent color generate from the background tint color.
 * This supersedes the 2026-08-24 user-configurable accentColor setting. The accent keeps the tint's hue at the shipped sky tone's lightness, with saturation held in a readable band, so the ice tint #88d7ff resolves to about the old #86d3f8 default.
 *
 * CDXC:Theming 2026-09-21 WHY:
 * A neutral tint (white, gray, black) has no hue to follow and a gray accent stops reading as an accent next to the foreground text, so neutral tints paint the shipped sky tone.
 * SEE-ALSO: the static --ghostex-accent in packages/core-ui/styles/theme.css and apps/desktop/views/project-board/styles.ts.
 */
export const NEUTRAL_TINT_ACCENT_COLOR = '#86d3f8';
const ACCENT_LIGHTNESS = 0.75;
const MIN_ACCENT_SATURATION = 0.55;
const MAX_ACCENT_SATURATION = 0.9;

export function getAccentColorForBackgroundTint(
  tintColor = DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_TINT_COLOR
): string {
  return accentColorForTint(tintColor, ACCENT_LIGHTNESS, NEUTRAL_TINT_ACCENT_COLOR);
}

/**
 * CDXC:Theming 2026-09-23 WHY:
 * The light chat needs an accent that reads on a pale surface: the tint's hue at a dark lightness, and the
 * light chrome's foreground #262626 for a neutral tint (the value theme.css already forces in light mode).
 * SEE-ALSO: `accent_color_for_tint` in apps/desktop/src/app/helpers/titlebar.rs.
 */
export const NEUTRAL_TINT_LIGHT_ACCENT_COLOR = '#262626';
const LIGHT_ACCENT_LIGHTNESS = 0.38;

export function getLightAccentColorForBackgroundTint(
  tintColor = DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_TINT_COLOR
): string {
  return accentColorForTint(tintColor, LIGHT_ACCENT_LIGHTNESS, NEUTRAL_TINT_LIGHT_ACCENT_COLOR);
}

function accentColorForTint(tintColor: string, lightness: number, neutral: string): string {
  const tint = parseSidebarTitlebarHexColor(
    normalizeSidebarTitlebarHexColor(tintColor, DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_TINT_COLOR)
  );
  if (isNeutralSidebarTitlebarColor(tint)) {
    return neutral;
  }

  const red = tint.red / 255;
  const green = tint.green / 255;
  const blue = tint.blue / 255;
  const max = Math.max(red, green, blue);
  const min = Math.min(red, green, blue);
  const chroma = max - min;
  const tintLightness = (max + min) / 2;
  const tintSaturation = chroma / (1 - Math.abs(2 * tintLightness - 1));
  let hueSextant: number;
  if (max === red) {
    hueSextant = ((((green - blue) / chroma) % 6) + 6) % 6;
  } else if (max === green) {
    hueSextant = (blue - red) / chroma + 2;
  } else {
    hueSextant = (red - green) / chroma + 4;
  }

  const saturation = Math.min(MAX_ACCENT_SATURATION, Math.max(MIN_ACCENT_SATURATION, tintSaturation));
  const accentChroma = (1 - Math.abs(2 * lightness - 1)) * saturation;
  const secondary = accentChroma * (1 - Math.abs((hueSextant % 2) - 1));
  const offset = lightness - accentChroma / 2;
  const [accentRed, accentGreen, accentBlue] =
    hueSextant < 1
      ? [accentChroma, secondary, 0]
      : hueSextant < 2
        ? [secondary, accentChroma, 0]
        : hueSextant < 3
          ? [0, accentChroma, secondary]
          : hueSextant < 4
            ? [0, secondary, accentChroma]
            : hueSextant < 5
              ? [secondary, 0, accentChroma]
              : [accentChroma, 0, secondary];
  return formatSidebarTitlebarHexColor({
    red: (accentRed + offset) * 255,
    green: (accentGreen + offset) * 255,
    blue: (accentBlue + offset) * 255,
  });
}

/**
 * CDXC:Theming 2026-06-15-13:22:
 * The foreground is no longer user-selectable. Ignore any legacy saved
 * foreground value and recompute it from the validated background color, using
 * the standard light foreground for dark backgrounds and standard dark
 * foreground for light backgrounds.
 */
export function getSidebarTitlebarForegroundForBackground(backgroundColor: string): string {
  const background = normalizeSidebarTitlebarHexColor(
    backgroundColor,
    DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_COLOR
  );
  const red = Number.parseInt(background.slice(1, 3), 16);
  const green = Number.parseInt(background.slice(3, 5), 16);
  const blue = Number.parseInt(background.slice(5, 7), 16);
  const luminance = (0.2126 * red + 0.7152 * green + 0.0722 * blue) / 255;
  return luminance > 0.54
    ? DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_DARK_FOREGROUND_COLOR
    : DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_FOREGROUND_COLOR;
}

export type SidebarTitlebarGradientColors = {
  sidebarBottom: string;
  sidebarTop: string;
  titlebarLeft: string;
  titlebarRight: string;
};

export function getSidebarTitlebarGradientColors(backgroundColor: string): SidebarTitlebarGradientColors {
  /*
   * CDXC:Theming 2026-06-19-12:33:
   * Custom sidebar chrome should render as a fixed-strength gradient instead of
   * a flat color. Derive the hue direction from the resolved tint-adjusted
   * background, normalize it so every tint uses the same gradient degree, and
   * keep neutral white/black/gray tints on a neutral gray gradient.
   *
   * CDXC:Theming 2026-06-19-13:26:
   * The titlebar should share the sidebar's gradient stops: left side matches
   * the sidebar top stop and right side matches the sidebar bottom stop so the
   * chrome fades darker across the titlebar instead of brighter.
   *
   * CDXC:Theming 2026-06-19-14:20:
   * Same-channel tint outputs must not receive the older blue fallback
   * direction. White and black selections should leave the dark sidebar area
   * neutral instead of shifting it toward blue.
   */
  const base = parseSidebarTitlebarHexColor(backgroundColor);
  const tintDirection = normalizedSidebarTitlebarTintDirection(base);
  const sidebarTop = addSidebarTitlebarColors(base, scaleSidebarTitlebarVector(tintDirection, 2));
  const sidebarBottom = addSidebarTitlebarColors(base, scaleSidebarTitlebarVector(tintDirection, 10));
  return {
    sidebarTop: formatSidebarTitlebarHexColor(sidebarTop),
    sidebarBottom: formatSidebarTitlebarHexColor(sidebarBottom),
    titlebarLeft: formatSidebarTitlebarHexColor(sidebarTop),
    titlebarRight: formatSidebarTitlebarHexColor(sidebarBottom),
  };
}

/**
 * CDXC:Theming 2026-09-22 DECISION:
 * User: light mode gets the same background contrast and tint controls as dark mode. The light scale is the
 * mirror of the dark one: 100 is pure white, the neutral tint at 96 resolves to the shipped #f4f4f5 light
 * chrome, and calibrated pale tints stand in for the very dark ones. The stored key says lightness so it
 * cannot be confused with the dark darkness key. 2026-09-22: User: the light slider goes down to 60, not 85.
 * SEE-ALSO: apps/desktop/src/app/helpers/titlebar.rs ports this for the native chrome.
 */
export const DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_TINT_COLOR = '#808080';
export const DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT = 96;
export const MIN_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT = 60;
export const MAX_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT = 100;
const CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_SCALE_REFERENCE_LIGHTNESS_PERCENT = 95;
const CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_CALIBRATION_COLOR = '#f1f1f2';
const CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_LIGHT_TINTS: ReadonlyMap<string, string> = new Map([
  ['#000000', '#f1f1f2'],
  ['#ffffff', '#f1f1f2'],
  ['#808080', '#f1f1f2'],
  ['#88d7ff', '#edf4fa'],
  ['#4f6672', '#eef1f3'],
  ['#884444', '#f7ecec'],
  ['#8a5330', '#f8f0e9'],
  ['#8a6a2f', '#f7f3e8'],
  ['#657a3f', '#f1f5ea'],
  ['#3f7a5f', '#ecf4ee'],
  ['#2f7d66', '#eaf4f0'],
  ['#287c7f', '#eaf4f4'],
  ['#336699', '#ecf1f7'],
  ['#4f5f96', '#eff0f7'],
  ['#6c4f8f', '#f2edf7'],
  ['#854f7a', '#f7ecf3'],
  ['#8a4f5f', '#f7ecef'],
  // 2026-09-25 preset colours (Slate, Midnight, Indigo, Teal, Forest, Olive, Amber, Rose).
  ['#4a6a8a', '#ebf1f8'],
  ['#1f3a8a', '#edeff8'],
  ['#4b4fa6', '#eeeef8'],
  ['#2f7f7f', '#ebf4f5'],
  ['#2e6a3a', '#edf7f0'],
  ['#6b6b35', '#f4f4ec'],
  ['#8a6a2a', '#f6f2ec'],
  ['#8a4a5c', '#f7edf0'],
]);

export function clampSidebarTitlebarLightBackgroundLightnessPercent(value: number): number {
  if (!Number.isFinite(value)) {
    return DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT;
  }
  return Math.min(
    MAX_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT,
    Math.max(MIN_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT, Math.round(value))
  );
}

function getSidebarTitlebarDefaultLightTintBackground(tint: string): SidebarTitlebarRgbColor {
  const calibratedTintBackground = CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_LIGHT_TINTS.get(tint);
  if (calibratedTintBackground) {
    return parseSidebarTitlebarHexColor(calibratedTintBackground);
  }

  const color = parseSidebarTitlebarHexColor(tint);
  const base = parseSidebarTitlebarHexColor(CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_CALIBRATION_COLOR);
  if (isNeutralSidebarTitlebarColor(color)) {
    return base;
  }

  const direction = normalizedSidebarTitlebarTintDirection(color);
  return addSidebarTitlebarColors(base, scaleSidebarTitlebarVector(direction, 6));
}

export function getSidebarTitlebarLightBackgroundForLightness(
  lightnessPercent: number,
  tintColor = DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_TINT_COLOR
): string {
  const lightness = clampSidebarTitlebarLightBackgroundLightnessPercent(lightnessPercent);
  if (lightness === MAX_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT) {
    return '#ffffff';
  }
  const tint = normalizeSidebarTitlebarHexColor(tintColor, DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_TINT_COLOR);
  const calibrated = getSidebarTitlebarDefaultLightTintBackground(tint);
  const scale =
    (MAX_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT - lightness) /
    (MAX_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT -
      CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_SCALE_REFERENCE_LIGHTNESS_PERCENT);
  return formatSidebarTitlebarHexColor({
    red: 255 - (255 - calibrated.red) * scale,
    green: 255 - (255 - calibrated.green) * scale,
    blue: 255 - (255 - calibrated.blue) * scale,
  });
}

export const DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_COLOR = getSidebarTitlebarLightBackgroundForLightness(
  DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT,
  DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_TINT_COLOR
);

/**
 * CDXC:Theming 2026-09-22 DECISION:
 * User: the Light theme and Dark theme dropdowns offer preset themes, and the contrast and tint controls
 * only appear when a dropdown is set to Custom. A preset is a fixed (contrast, tint) pair fed through the
 * same resolution as the custom controls, so presets and custom chrome can never drift apart. Choosing a
 * preset leaves the saved custom values alone; they come back unchanged when Custom is selected again.
 * 2026-09-22: User: all the colored dark presets default to 96 contrast.
 *
 * CDXC:Theming 2026-09-25 DECISION:
 * User: "I want more colors there. You just have a few there right now. I want more colors there. I want a darker blue
 * also." Both appearances offer the same sixteen colours (Graphite, Black/White, Slate, Midnight, Blue, Indigo, Teal,
 * Green, Forest, Olive, Amber, Orange, Red, Rose, Pink, Purple); Midnight is the darker blue. The existing ids and values
 * are unchanged so saved themes keep their look, `gray` is labelled Graphite, and the new tints have calibrated entries
 * in the dark and light tint tables so they read as richly as Blue, Green and Red.
 * SEE-ALSO: `DARK_THEME_PRESET_CONTROLS` / `LIGHT_THEME_PRESET_CONTROLS` and the tint tables in
 * apps/desktop/src/app/helpers/titlebar.rs, which must match these entry for entry.
 */
export type ThemePresetColor =
  | 'gray'
  | 'slate'
  | 'midnight'
  | 'blue'
  | 'indigo'
  | 'teal'
  | 'green'
  | 'forest'
  | 'olive'
  | 'amber'
  | 'orange'
  | 'red'
  | 'rose'
  | 'pink'
  | 'purple';
export type DarkThemePreset = ThemePresetColor | 'black' | 'custom';
export type LightThemePreset = ThemePresetColor | 'white' | 'custom';
export type DarkChromeControls = { darknessPercent: number; tintColor: string };
export type LightChromeControls = { lightnessPercent: number; tintColor: string };

export const DEFAULT_DARK_THEME_PRESET: DarkThemePreset = 'gray';
export const DEFAULT_LIGHT_THEME_PRESET: LightThemePreset = 'gray';

export const DARK_THEME_PRESET_CONTROLS: Readonly<Record<Exclude<DarkThemePreset, 'custom'>, DarkChromeControls>> = {
  gray: {
    darknessPercent: DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT,
    tintColor: DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_TINT_COLOR,
  },
  black: { darknessPercent: 100, tintColor: '#000000' },
  slate: { darknessPercent: 94, tintColor: '#4a6a8a' },
  midnight: { darknessPercent: 94, tintColor: '#1f3a8a' },
  blue: { darknessPercent: 96, tintColor: '#336699' },
  indigo: { darknessPercent: 94, tintColor: '#4b4fa6' },
  teal: { darknessPercent: 94, tintColor: '#2f7f7f' },
  green: { darknessPercent: 96, tintColor: '#3f7a5f' },
  forest: { darknessPercent: 94, tintColor: '#2e6a3a' },
  olive: { darknessPercent: 94, tintColor: '#6b6b35' },
  amber: { darknessPercent: 94, tintColor: '#8a6a2a' },
  orange: { darknessPercent: 96, tintColor: '#8a5330' },
  red: { darknessPercent: 96, tintColor: '#884444' },
  rose: { darknessPercent: 94, tintColor: '#8a4a5c' },
  pink: { darknessPercent: 96, tintColor: '#854f7a' },
  purple: { darknessPercent: 96, tintColor: '#6c4f8f' },
};

export const LIGHT_THEME_PRESET_CONTROLS: Readonly<Record<Exclude<LightThemePreset, 'custom'>, LightChromeControls>> = {
  gray: {
    lightnessPercent: DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT,
    tintColor: DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_TINT_COLOR,
  },
  white: { lightnessPercent: 100, tintColor: '#ffffff' },
  slate: { lightnessPercent: 95, tintColor: '#4a6a8a' },
  midnight: { lightnessPercent: 94, tintColor: '#1f3a8a' },
  blue: { lightnessPercent: 95, tintColor: '#336699' },
  indigo: { lightnessPercent: 95, tintColor: '#4b4fa6' },
  teal: { lightnessPercent: 95, tintColor: '#2f7f7f' },
  green: { lightnessPercent: 95, tintColor: '#3f7a5f' },
  forest: { lightnessPercent: 94, tintColor: '#2e6a3a' },
  olive: { lightnessPercent: 95, tintColor: '#6b6b35' },
  amber: { lightnessPercent: 95, tintColor: '#8a6a2a' },
  orange: { lightnessPercent: 95, tintColor: '#8a5330' },
  red: { lightnessPercent: 95, tintColor: '#884444' },
  rose: { lightnessPercent: 95, tintColor: '#8a4a5c' },
  pink: { lightnessPercent: 95, tintColor: '#854f7a' },
  purple: { lightnessPercent: 95, tintColor: '#6c4f8f' },
};

/**
 * CDXC:Theming 2026-09-23 DECISION:
 * User: "please add a color contrast 5 options slider in the setup", then "make the contrast show as a slider in the
 * advanced and make sidebar and main contrast different please". Contrast is two values in points, one for the sidebar
 * (`themeSidebarContrast`) and one for the work area (`themeWorkAreaContrast`), from -12 to 4 with 0 the theme's own
 * contrast; higher makes dark backgrounds darker and light backgrounds whiter. The sidebar value shifts a preset
 * theme's chrome (Custom keeps its own slider); the work area colour is the chrome at the work area value instead.
 * Supersedes the same day's single five-step `themeContrast` (-2 to 2), which migrates as -8, -4, 0, 2 or 4 points for
 * both.
 * SEE-ALSO: `theme_contrast_points` in apps/desktop/src/app/helpers/titlebar.rs.
 */
export const THEME_CONTRAST_MIN_POINTS = -12;
export const THEME_CONTRAST_MAX_POINTS = 4;
const LEGACY_THEME_CONTRAST_POINTS: Readonly<Record<number, number>> = { [-2]: -8, [-1]: -4, 0: 0, 1: 2, 2: 4 };

export function clampThemeContrastPoints(value: number): number {
  return Math.max(THEME_CONTRAST_MIN_POINTS, Math.min(THEME_CONTRAST_MAX_POINTS, Math.round(value)));
}

/** A saved contrast value, or the retired five-step `themeContrast` carried over when it is missing. */
export function readThemeContrastPoints(source: Record<string, unknown>, key: string): number {
  const value = source[key];
  if (typeof value === 'number' && Number.isFinite(value)) {
    return clampThemeContrastPoints(value);
  }
  const legacy = source.themeContrast;
  const step =
    typeof legacy === 'number' && Number.isFinite(legacy) ? Math.max(-2, Math.min(2, Math.round(legacy))) : 0;
  return LEGACY_THEME_CONTRAST_POINTS[step] ?? 0;
}

/** A preset's own contrast shifted by contrast points. */
export function presetDarknessWithContrast(darknessPercent: number, points: number | undefined): number {
  return clampSidebarTitlebarBackgroundDarknessPercent(darknessPercent + clampThemeContrastPoints(points ?? 0));
}

export function presetLightnessWithContrast(lightnessPercent: number, points: number | undefined): number {
  return clampSidebarTitlebarLightBackgroundLightnessPercent(lightnessPercent + clampThemeContrastPoints(points ?? 0));
}

export function normalizeDarkThemePreset(value: unknown): DarkThemePreset | undefined {
  return value === 'custom' || (typeof value === 'string' && value in DARK_THEME_PRESET_CONTROLS)
    ? (value as DarkThemePreset)
    : undefined;
}

export function normalizeLightThemePreset(value: unknown): LightThemePreset | undefined {
  return value === 'custom' || (typeof value === 'string' && value in LIGHT_THEME_PRESET_CONTROLS)
    ? (value as LightThemePreset)
    : undefined;
}

export function resolveDarkChromeControls(settings: {
  darkThemePreset: DarkThemePreset;
  customSidebarTitlebarBackgroundDarknessPercent: number;
  customSidebarTitlebarBackgroundTintColor: string;
  themeSidebarContrast?: number;
}): DarkChromeControls {
  if (settings.darkThemePreset !== 'custom') {
    const preset = DARK_THEME_PRESET_CONTROLS[settings.darkThemePreset];
    return {
      ...preset,
      darknessPercent: presetDarknessWithContrast(preset.darknessPercent, settings.themeSidebarContrast),
    };
  }
  return {
    darknessPercent: clampSidebarTitlebarBackgroundDarknessPercent(
      settings.customSidebarTitlebarBackgroundDarknessPercent
    ),
    tintColor: normalizeSidebarTitlebarHexColor(
      settings.customSidebarTitlebarBackgroundTintColor,
      DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_TINT_COLOR
    ),
  };
}

export function resolveLightChromeControls(settings: {
  lightThemePreset: LightThemePreset;
  customSidebarTitlebarLightBackgroundLightnessPercent: number;
  customSidebarTitlebarLightBackgroundTintColor: string;
  themeSidebarContrast?: number;
}): LightChromeControls {
  if (settings.lightThemePreset !== 'custom') {
    const preset = LIGHT_THEME_PRESET_CONTROLS[settings.lightThemePreset];
    return {
      ...preset,
      lightnessPercent: presetLightnessWithContrast(preset.lightnessPercent, settings.themeSidebarContrast),
    };
  }
  return {
    lightnessPercent: clampSidebarTitlebarLightBackgroundLightnessPercent(
      settings.customSidebarTitlebarLightBackgroundLightnessPercent
    ),
    tintColor: normalizeSidebarTitlebarHexColor(
      settings.customSidebarTitlebarLightBackgroundTintColor,
      DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_TINT_COLOR
    ),
  };
}

/** The accent follows the tint the dark chrome actually paints: the preset's, or the custom one. */
export function getAccentColorForSettings(
  settings:
    | {
        darkThemePreset: DarkThemePreset;
        customSidebarTitlebarBackgroundDarknessPercent: number;
        customSidebarTitlebarBackgroundTintColor: string;
      }
    | undefined
): string {
  return getAccentColorForBackgroundTint(
    settings ? resolveDarkChromeControls(settings).tintColor : DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_TINT_COLOR
  );
}

/** The light chat's accent, from the tint the light chrome actually paints. */
export function getLightAccentColorForSettings(
  settings:
    | {
        lightThemePreset: LightThemePreset;
        customSidebarTitlebarLightBackgroundLightnessPercent: number;
        customSidebarTitlebarLightBackgroundTintColor: string;
      }
    | undefined
): string {
  return getLightAccentColorForBackgroundTint(
    settings
      ? resolveLightChromeControls(settings).tintColor
      : DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_TINT_COLOR
  );
}

/** The resolved chrome background for one appearance, light or dark, from normalized settings. */
export function getSidebarTitlebarBackgroundForVariant(
  settings: { customSidebarTitlebarBackgroundColor: string; customSidebarTitlebarLightBackgroundColor: string },
  variant: 'light' | 'dark'
): string {
  return variant === 'light'
    ? settings.customSidebarTitlebarLightBackgroundColor
    : settings.customSidebarTitlebarBackgroundColor;
}

function blendSidebarTitlebarTowardWhite(color: string, amount: number): string {
  const base = parseSidebarTitlebarHexColor(color);
  return formatSidebarTitlebarHexColor({
    red: base.red + (255 - base.red) * amount,
    green: base.green + (255 - base.green) * amount,
    blue: base.blue + (255 - base.blue) * amount,
  });
}

function isLightSidebarTitlebarBackground(color: string): boolean {
  return getSidebarTitlebarForegroundForBackground(color) === DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_DARK_FOREGROUND_COLOR;
}

/**
 * CDXC:Theming 2026-09-22 DECISION:
 * User: the theme also colors the sidebar's dropdown menus and the chat view background. Both are a fixed
 * step off the resolved chrome background so a tinted chrome carries its hue into them: the menu sits 5%
 * toward white on dark chrome and 70% on light chrome, the chat 1% on dark chrome and 25% on light
 * (2026-09-22: User: keep the dark mode's two tones in light mode too, the sidebar a tiny bit more
 * contrast than the rest; this replaces the same-day exact match and the earlier 70% / 40% steps). With
 * the shipped neutral chrome these land next to the previous fixed menu colours (#171717 for #191919 /
 * #ffffff), exactly on the previous #0d0d0d dark chat, and on #f7f7f7 for the light chat.
 * SEE-ALSO: apps/desktop/src/app/helpers/titlebar.rs and apps/desktop/src/app/native_chat/appearance.rs
 * paint the native menu and chat.
 */
export function getSidebarTitlebarMenuBackgroundForChrome(chromeColor: string): string {
  return blendSidebarTitlebarTowardWhite(chromeColor, isLightSidebarTitlebarBackground(chromeColor) ? 0.7 : 0.05);
}

export function getSessionChatBackgroundForChrome(chromeColor: string): string {
  return blendSidebarTitlebarTowardWhite(chromeColor, isLightSidebarTitlebarBackground(chromeColor) ? 0.25 : 0.01);
}

type WorkAreaColorSettings = Parameters<typeof resolveDarkChromeControls>[0] &
  Parameters<typeof resolveLightChromeControls>[0] & { themeWorkAreaContrast?: number };

/**
 * The work area's colour for one appearance: the sidebar's chrome with the work area contrast in place of the sidebar
 * contrast, stepped toward white like every chat and terminal background.
 * SEE-ALSO: `work_area_background_for_variant` in apps/desktop/src/app/helpers/titlebar.rs.
 */
export function getWorkAreaBackgroundForSettings(settings: WorkAreaColorSettings, light: boolean): string {
  const delta = (settings.themeWorkAreaContrast ?? 0) - (settings.themeSidebarContrast ?? 0);
  if (light) {
    const controls = resolveLightChromeControls(settings);
    return getSessionChatBackgroundForChrome(
      getSidebarTitlebarLightBackgroundForLightness(
        clampSidebarTitlebarLightBackgroundLightnessPercent(controls.lightnessPercent + delta),
        controls.tintColor
      )
    );
  }
  const controls = resolveDarkChromeControls(settings);
  return getSessionChatBackgroundForChrome(
    getSidebarTitlebarBackgroundForDarkness(
      clampSidebarTitlebarBackgroundDarknessPercent(controls.darknessPercent + delta),
      controls.tintColor
    )
  );
}
