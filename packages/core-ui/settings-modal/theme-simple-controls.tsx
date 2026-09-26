import { type CSSProperties } from 'react';
import { cn } from '@/packages/components/utils';
import {
  DARK_THEME_PRESET_CONTROLS,
  DARK_THEME_PRESET_OPTIONS,
  LIGHT_THEME_PRESET_CONTROLS,
  LIGHT_THEME_PRESET_OPTIONS,
  SIDEBAR_THEME_SETTING_OPTIONS,
  getAccentColorForBackgroundTint,
  getLightAccentColorForBackgroundTint,
  getWorkAreaBackgroundForSettings,
  getSidebarTitlebarBackgroundForDarkness,
  getSidebarTitlebarForegroundForBackground,
  getSidebarTitlebarLightBackgroundForLightness,
  DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT,
  DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT,
  presetDarknessWithContrast,
  presetLightnessWithContrast,
  resolveDarkChromeControls,
  resolveLightChromeControls,
  type DarkThemePreset,
  type LightThemePreset,
  type WindowGlassMode,
  type ghostexSettings,
} from '../../shared/ghostex-settings';
import { detectghostexHotkeyPlatform } from '../../shared/ghostex-hotkeys';

/**
 * The Theme page's simple choices (Appearance, the theme colour squares, Colourfulness, transparency), shared by the
 * Settings Theme page and the onboarding's Get started panel so both offer the same options and write the same
 * settings. Each surface draws its own rows around them; the options, the colour squares, the Colourfulness steps and
 * the transparency mapping live only here.
 */

/** Appearance reads System, Light, Dark, left to right. */
const APPEARANCE_ORDER = ['system', 'plain-light', 'dark-2'] as const;

export const APPEARANCE_CHOICES: ReadonlyArray<{ label: string; value: ghostexSettings['sidebarTheme'] }> =
  APPEARANCE_ORDER.map((value) => ({
    label: SIDEBAR_THEME_SETTING_OPTIONS.find((option) => option.value === value)?.label ?? value,
    value,
  }));

/** Enable Transparency is on for every glass mode but Opaque. */
export function isTransparencyEnabled(windowGlass: WindowGlassMode): boolean {
  return windowGlass !== 'opaque';
}

/** Turning Enable Transparency on keeps an Always choice and otherwise picks Dark only. */
export function windowGlassForTransparency(current: WindowGlassMode, enabled: boolean): WindowGlassMode {
  if (!enabled) {
    return 'opaque';
  }
  return current === 'opaque' ? 'auto' : current;
}

export type ThemeScheme = 'dark' | 'light';

/** One colour square: a preset id, its name and the gradient it is drawn with. */
export type ThemeSwatch<Preset extends string> = { label: string; style: CSSProperties; value: Preset };

/** Neutral themes have no hue to glow with, so their squares take a grey sheen instead of the accent. */
const NEUTRAL_SWATCH_ACCENT: Readonly<Record<string, readonly [string, string]>> = {
  gray: ['#a3a3a8', '#6b6b70'],
  black: ['#4a4a4f', '#c8c8cc'],
  white: ['#4a4a4f', '#c8c8cc'],
};

function mixHex(from: string, to: string, amount: number): string {
  const channels = (hex: string) => [1, 3, 5].map((index) => Number.parseInt(hex.slice(index, index + 2), 16));
  const [a, b] = [channels(from), channels(to)];
  return `#${a
    .map((value, index) =>
      Math.round(value + (b[index]! - value) * amount)
        .toString(16)
        .padStart(2, '0')
    )
    .join('')}`;
}

function alphaHex(alpha: number): string {
  return Math.round(Math.min(1, Math.max(0, alpha)) * 255)
    .toString(16)
    .padStart(2, '0');
}

/** A preset's chrome at one Colourfulness step (0 Subtle ... 4 Vivid). */
function presetChromeAtStep(scheme: ThemeScheme, preset: string, step: number): string {
  const points = COLOURFULNESS_CHOICES[Math.max(0, Math.min(4, step))]!.points;
  if (scheme === 'dark') {
    const controls = DARK_THEME_PRESET_CONTROLS[preset as Exclude<DarkThemePreset, 'custom'>];
    return getSidebarTitlebarBackgroundForDarkness(
      presetDarknessWithContrast(controls.darknessPercent, points),
      controls.tintColor
    );
  }
  const controls = LIGHT_THEME_PRESET_CONTROLS[preset as Exclude<LightThemePreset, 'custom'>];
  return getSidebarTitlebarLightBackgroundForLightness(
    presetLightnessWithContrast(controls.lightnessPercent, points),
    controls.tintColor
  );
}

/**
 * CDXC:Theming 2026-09-25 DECISION:
 * User: "I don't like the Previews that you're showing there for the different colors, please make them look like just gradient squares that look beautiful for each of the colors", then "the gradient colors look way better in light mode. Can we do something simpler like those ones in dark mode also" and "Make the color boxes (the squares) smaller". Each theme colour is a small gradient square: a lighter, more colourful top-left fading to the theme's own chrome, with a soft glow of the theme's accent. Dark squares lift their stops toward the accent so they read as clean gradients rather than near-black tiles. The squares follow the current Colourfulness step. Supersedes the 2026-09-23 small-window preview cards.
 */
function swatchStyle(scheme: ThemeScheme, preset: string, tint: string, step: number): CSSProperties {
  const neutral = NEUTRAL_SWATCH_ACCENT[preset];
  const accent = neutral
    ? neutral[scheme === 'dark' ? 0 : 1]
    : scheme === 'dark'
      ? getAccentColorForBackgroundTint(tint)
      : getLightAccentColorForBackgroundTint(tint);
  const topChrome = presetChromeAtStep(scheme, preset, Math.min(4, step + 2));
  const top = scheme === 'dark' ? mixHex(topChrome, accent, 0.34 + step * 0.03) : topChrome;
  const base =
    scheme === 'dark'
      ? mixHex(presetChromeAtStep(scheme, preset, step), accent, 0.1)
      : presetChromeAtStep(scheme, preset, step);
  const glow = alphaHex(0.26 + step * 0.07);
  const rim = alphaHex(0.1 + step * 0.03);
  return {
    background: `radial-gradient(130% 100% at 20% 8%, ${accent}${glow} 0%, ${accent}00 62%), radial-gradient(90% 70% at 100% 100%, ${accent}${rim} 0%, ${accent}00 70%), linear-gradient(150deg, ${top} 0%, ${base} 88%)`,
  };
}

/** The sixteen dark colours, drawn at the current Colourfulness. */
export function darkThemeSwatches(settings: ghostexSettings): ThemeSwatch<DarkThemePreset>[] {
  const step = colourfulnessDisplayStep(settings.themeSidebarContrast);
  return DARK_THEME_PRESET_OPTIONS.filter((option) => option.value !== 'custom').map((option) => {
    const tint = DARK_THEME_PRESET_CONTROLS[option.value as Exclude<DarkThemePreset, 'custom'>].tintColor;
    return { label: option.label, style: swatchStyle('dark', option.value, tint, step), value: option.value };
  });
}

/** The sixteen light colours, drawn at the current Colourfulness. */
export function lightThemeSwatches(settings: ghostexSettings): ThemeSwatch<LightThemePreset>[] {
  const step = colourfulnessDisplayStep(settings.themeSidebarContrast);
  return LIGHT_THEME_PRESET_OPTIONS.filter((option) => option.value !== 'custom').map((option) => {
    const tint = LIGHT_THEME_PRESET_CONTROLS[option.value as Exclude<LightThemePreset, 'custom'>].tintColor;
    return { label: option.label, style: swatchStyle('light', option.value, tint, step), value: option.value };
  });
}

/** The name of the colour a preset id stands for, or Custom. */
export function themePresetLabel(scheme: ThemeScheme, preset: string): string {
  const options = scheme === 'dark' ? DARK_THEME_PRESET_OPTIONS : LIGHT_THEME_PRESET_OPTIONS;
  return options.find((option) => option.value === preset)?.label ?? preset;
}

/** One full-width row of small gradient squares; the names live in each square's title and accessible name. */
export function ThemeSwatchGrid<Preset extends string>({
  className,
  id,
  label,
  onSelect,
  swatches,
  value,
}: {
  className?: string;
  id?: string;
  label: string;
  onSelect: (value: Preset) => void;
  swatches: ReadonlyArray<ThemeSwatch<Preset>>;
  value: Preset;
}) {
  return (
    <div aria-label={label} className={cn('theme-colour-grid', className)} id={id} role='radiogroup'>
      {swatches.map((swatch) => (
        <button
          aria-checked={swatch.value === value}
          aria-label={swatch.label}
          className={cn(
            'theme-colour-swatch',
            `theme-colour-${swatch.value}-swatch`,
            swatch.value === value && 'is-selected'
          )}
          key={swatch.value}
          onClick={() => onSelect(swatch.value)}
          role='radio'
          title={swatch.label}
          type='button'
        >
          <span aria-hidden='true' className='theme-colour-square' style={swatch.style} />
        </button>
      ))}
    </div>
  );
}

/** The Dark mode / Light mode switch above the colour squares: which appearance's colours are shown. */
export function ThemeSchemeTabs({
  labels = { dark: 'Dark mode', light: 'Light mode' },
  onChange,
  value,
}: {
  labels?: { dark: string; light: string };
  onChange: (value: ThemeScheme) => void;
  value: ThemeScheme;
}) {
  return (
    <div aria-label='Show colours for' className='theme-scheme-tabs' role='tablist'>
      {(['dark', 'light'] as const).map((scheme) => (
        <button
          aria-selected={value === scheme}
          className={cn(value === scheme && 'is-selected')}
          key={scheme}
          onClick={() => onChange(scheme)}
          role='tab'
          type='button'
        >
          {labels[scheme]}
        </button>
      ))}
    </div>
  );
}

/** The appearance whose colours show first: Light when the window is light, otherwise Dark. */
export function initialThemeScheme(sidebarTheme: string): ThemeScheme {
  if (sidebarTheme === 'system') {
    return typeof window !== 'undefined' && window.matchMedia?.('(prefers-color-scheme: light)').matches
      ? 'light'
      : 'dark';
  }
  return sidebarTheme.startsWith('dark') ? 'dark' : 'light';
}

/** Sidebar | work area in the chosen appearance's colours, so Colourfulness shows what it does. */
export function ColourfulnessPreview({ scheme, settings }: { scheme: ThemeScheme; settings: ghostexSettings }) {
  const light = scheme === 'light';
  const chrome = light
    ? (() => {
        const controls = resolveLightChromeControls(settings);
        return getSidebarTitlebarLightBackgroundForLightness(controls.lightnessPercent, controls.tintColor);
      })()
    : (() => {
        const controls = resolveDarkChromeControls(settings);
        return getSidebarTitlebarBackgroundForDarkness(controls.darknessPercent, controls.tintColor);
      })();
  const work = getWorkAreaBackgroundForSettings(settings, light);
  return (
    <span
      aria-hidden='true'
      className='theme-colour-live-preview'
      style={{ '--theme-preview-foreground': getSidebarTitlebarForegroundForBackground(chrome) } as CSSProperties}
      title='Sidebar | Work area'
    >
      <span className='theme-colour-live-preview-sidebar' style={{ background: chrome }} />
      <span className='theme-colour-live-preview-work' style={{ background: work }} />
    </span>
  );
}

/**
 * Window glass is drawn by the macOS and Windows apps, so surfaces that can hide the switch elsewhere ask here.
 *
 * CDXC:Theming 2026-09-25 DECISION:
 * User: "let's enable transparency on windows please also if possible. like it works on mac exactly." The glass
 * controls show on macOS and Windows. Glass shows (Wallpaper only, Custom image) stays macOS-only because only the
 * macOS window backend can draw a picture behind the glass, and on Windows turning glass on takes effect at the next
 * launch (see `note_main_window_background` in apps/desktop/src/app/helpers/window_glass.rs).
 */
export function windowGlassAvailable(): boolean {
  const platform = detectghostexHotkeyPlatform();
  return platform === 'mac' || platform === 'windows';
}

/** Whether the glass can show the wallpaper or a chosen picture instead of what is behind the window (macOS). */
export function windowGlassPicturesAvailable(): boolean {
  return detectghostexHotkeyPlatform() === 'mac';
}

/** A sentence for glass controls on Windows, where turning glass on waits for the next launch. */
export function windowGlassRestartNote(): string {
  return detectghostexHotkeyPlatform() === 'windows'
    ? ' On Windows, turning it on takes effect the next time Ghostex starts.'
    : '';
}

/**
 * CDXC:Theming 2026-09-25 DECISION:
 * User: "I don't like the low, medium, high contrast. I feel this doesn't represent what's happening to the colors, so you can say more. You can switch it between more colorful and less colorful". Background contrast becomes Colourfulness, five steps from Subtle to Vivid: lower contrast makes the chrome lighter and lets more of the theme colour show, higher makes it deeper and nearly neutral. Each step sets the sidebar and work area contrast together (Subtle +4, Soft 0 = the shipped default, Balanced -4, Rich -8, Vivid -12 points); More colour options can set the two areas apart. Supersedes the 2026-09-23 Background contrast (Lowest to Highest).
 */
export const COLOURFULNESS_CHOICES: readonly { label: string; points: number }[] = [
  { label: 'Subtle', points: 4 },
  { label: 'Soft', points: 0 },
  { label: 'Balanced', points: -4 },
  { label: 'Rich', points: -8 },
  { label: 'Vivid', points: -12 },
];

/** The step a contrast value sits on, or -1 between steps. */
export function colourfulnessStepForPoints(points: number): number {
  return COLOURFULNESS_CHOICES.findIndex((choice) => choice.points === points);
}

/** The nearest step to a contrast value, for drawing a slider or a square when it sits between steps. */
export function colourfulnessDisplayStep(points: number): number {
  let best = 0;
  COLOURFULNESS_CHOICES.forEach((choice, index) => {
    if (Math.abs(choice.points - points) < Math.abs(COLOURFULNESS_CHOICES[best]!.points - points)) {
      best = index;
    }
  });
  return best;
}

/** The one step both areas share, or -1 once they are set apart or sit between steps. */
export function colourfulnessStepIndex(settings: ghostexSettings): number {
  if (settings.themeSidebarContrast !== settings.themeWorkAreaContrast) {
    return -1;
  }
  return colourfulnessStepForPoints(settings.themeSidebarContrast);
}

/**
 * CDXC:Theming 2026-09-23 DECISION:
 * User: "add transparency strength selection" to the setup and the Theme page, then "it needs to make the sidebar
 * darker than main not vice versa", then "make it 7 point difference and make it a slider with more options".
 * Transparency strength is one 0-100 slider in steps of 5 (higher shows more of the desktop) that sets the four glass
 * tint sliders at once: the sidebar's tint falls from 95 (dark) / 98 (light) as it rises, and the work area is always
 * 7 points more see-through than the sidebar. The tint sliders under Settings -> Theme -> More transparency options stay the exact
 * controls.
 */
export const TRANSPARENCY_STRENGTH_MIN = 0;
export const TRANSPARENCY_STRENGTH_MAX = 100;
export const TRANSPARENCY_STRENGTH_STEP = 5;
const TRANSPARENCY_WORK_AREA_GAP = 7;

/** The four tint sliders one strength sets. */
export function transparencyStrengthPatch(strength: number): Partial<ghostexSettings> {
  const value = Math.max(TRANSPARENCY_STRENGTH_MIN, Math.min(TRANSPARENCY_STRENGTH_MAX, strength));
  const sidebarDark = Math.round(95 - value * 0.35);
  const sidebarLight = Math.round(98 - value * 0.25);
  return {
    windowGlassSidebarOpacityDark: sidebarDark,
    windowGlassWorkAreaTintDark: sidebarDark - TRANSPARENCY_WORK_AREA_GAP,
    windowGlassSidebarOpacityLight: sidebarLight,
    windowGlassWorkAreaTintLight: sidebarLight - TRANSPARENCY_WORK_AREA_GAP,
  };
}

/**
 * The strength the four tint sliders came from, or undefined once they were tuned by hand. `nearest` is where the
 * slider sits in that case, read from the dark sidebar tint.
 */
export function transparencyStrengthFromSettings(settings: ghostexSettings): { exact?: number; nearest: number } {
  const raw = (95 - settings.windowGlassSidebarOpacityDark) / 0.35;
  const nearest = Math.max(
    TRANSPARENCY_STRENGTH_MIN,
    Math.min(TRANSPARENCY_STRENGTH_MAX, Math.round(raw / TRANSPARENCY_STRENGTH_STEP) * TRANSPARENCY_STRENGTH_STEP)
  );
  const patch = transparencyStrengthPatch(nearest);
  const exact =
    patch.windowGlassSidebarOpacityDark === settings.windowGlassSidebarOpacityDark &&
    patch.windowGlassWorkAreaTintDark === settings.windowGlassWorkAreaTintDark &&
    patch.windowGlassSidebarOpacityLight === settings.windowGlassSidebarOpacityLight &&
    patch.windowGlassWorkAreaTintLight === settings.windowGlassWorkAreaTintLight
      ? nearest
      : undefined;
  return { exact, nearest };
}

/**
 * The settings patch for one Colourfulness step: the sidebar and work area contrast together, and the dark and light
 * Custom colour contrast moved in step, so the friendly control drives whatever is painted.
 */
export function colourfulnessPatch(step: number): Partial<ghostexSettings> {
  const points = COLOURFULNESS_CHOICES[Math.max(0, Math.min(COLOURFULNESS_CHOICES.length - 1, step))]!.points;
  return {
    themeSidebarContrast: points,
    themeWorkAreaContrast: points,
    customSidebarTitlebarBackgroundDarknessPercent: presetDarknessWithContrast(
      DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT,
      points
    ),
    customSidebarTitlebarLightBackgroundLightnessPercent: presetLightnessWithContrast(
      DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT,
      points
    ),
  };
}

/** Each dark colour's match in light mode, so picking one look fills in the other appearance to match. */
export const LIGHT_PRESET_FOR_DARK: Readonly<Record<Exclude<DarkThemePreset, 'custom'>, LightThemePreset>> = {
  gray: 'gray',
  black: 'white',
  slate: 'slate',
  midnight: 'midnight',
  blue: 'blue',
  indigo: 'indigo',
  teal: 'teal',
  green: 'green',
  forest: 'forest',
  olive: 'olive',
  amber: 'amber',
  orange: 'orange',
  red: 'red',
  rose: 'rose',
  pink: 'pink',
  purple: 'purple',
};

export const DARK_PRESET_FOR_LIGHT: Readonly<Record<Exclude<LightThemePreset, 'custom'>, DarkThemePreset>> = {
  gray: 'gray',
  white: 'black',
  slate: 'slate',
  midnight: 'midnight',
  blue: 'blue',
  indigo: 'indigo',
  teal: 'teal',
  green: 'green',
  forest: 'forest',
  olive: 'olive',
  amber: 'amber',
  orange: 'orange',
  red: 'red',
  rose: 'rose',
  pink: 'pink',
  purple: 'purple',
};
