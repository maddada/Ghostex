export const DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_FOREGROUND_COLOR = '#d8d8d8';
export const DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_DARK_FOREGROUND_COLOR = '#262626';
export const DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_COLOR = '#040607';
export const DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_TINT_COLOR = '#88d7ff';
export const DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT = 98;
/*
 * CDXC:SidebarTitlebarColors 2026-06-28-08:01:
 * The tint scale keeps the original 95 reference so existing saved contrast
 * values do not darken or brighten when the app default changes.
 *
 * CDXC:SidebarTitlebarColors 2026-07-22:
 * New installs used the neutral #808080 tint at 93 Background Contrast,
 * resolving to #141414 while preserving the existing calibrated scale.
 *
 * CDXC:SidebarTitlebarColors 2026-08-30:
 * New installs use ice #88d7ff at 98 Background Contrast, resolving to
 * #040607. Neutral Gray remains a swatch at #808080; it is no longer the
 * first-run default.
 */
const CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_SCALE_REFERENCE_DARKNESS_PERCENT = 95;
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
    return parseSidebarTitlebarHexColor(DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_COLOR);
  }

  const direction = normalizedSidebarTitlebarTintDirection(color);
  const base = parseSidebarTitlebarHexColor(DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_COLOR);
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
   * CDXC:SidebarTitlebarColors 2026-06-15-13:45:
   * Replace the freeform custom background color picker with a contrast slider.
   * The slider controls how strongly the calibrated dark tint background is
   * applied so custom chrome can vary in contrast without turning into
   * arbitrary bright sidebar colors.
   *
   * CDXC:SidebarTitlebarColors 2026-06-15-15:01:
   * Limit the contrast slider to 85-100 so custom chrome stays in the dark
   * gray range instead of drifting into mid-gray sidebar backgrounds.
   *
   * CDXC:SidebarTitlebarColors 2026-06-15-15:15:
   * Keep the internal darkness percentage name for compatibility while the
   * visible Settings control is labeled Background Contrast.
   *
   * CDXC:SidebarTitlebarColors 2026-06-15-15:28:
   * Add a web-only tint picker without returning to arbitrary background
   * colors. Map tint choices to dark applied backgrounds so tint changes are
   * subtle and neutral #808080 preserves the original gray.
   *
   * CDXC:SidebarTitlebarColors 2026-06-16-14:28:
   * Default custom chrome should now use 95 contrast with white tint. White
   * remains neutral in the calibrated tint table because all same-channel
   * tints should keep the sidebar/titlebar background gray.
   *
   * CDXC:SidebarTitlebarColors 2026-06-19-14:20:
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
 * CDXC:SidebarTitlebarColors 2026-06-15-13:22:
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
   * CDXC:SidebarTitlebarColors 2026-06-19-12:33:
   * Custom sidebar chrome should render as a fixed-strength gradient instead of
   * a flat color. Derive the hue direction from the resolved tint-adjusted
   * background, normalize it so every tint uses the same gradient degree, and
   * keep neutral white/black/gray tints on a neutral gray gradient.
   *
   * CDXC:SidebarTitlebarColors 2026-06-19-13:26:
   * The titlebar should share the sidebar's gradient stops: left side matches
   * the sidebar top stop and right side matches the sidebar bottom stop so the
   * chrome fades darker across the titlebar instead of brighter.
   *
   * CDXC:SidebarTitlebarColors 2026-06-19-14:20:
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
