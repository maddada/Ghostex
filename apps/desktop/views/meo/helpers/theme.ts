// @ts-nocheck
import {
  resolveTheme,
  SYNTAX_TAG_SPECS,
  type ThemeSettings,
  themeColorKeys
} from '../themeDefaults';

const vscodeEditorFontFamily = 'var(--vscode-editor-font-family)';
const vscodeEditorFontSize = 'var(--vscode-editor-font-size, 13px)';
const styleValueInjectionPattern = /[\n\r;{}]/g;
const defaultHeadingFontWeight = '600';
const headingSizeFallbacks = ['1.6em', '1.5em', '1.3em', '1.2em', '1.1em', '1em'] as const;

const resolveEditorFontWeight = (): string => {
  const rawEditorFontWeight = getComputedStyle(document.documentElement).getPropertyValue('--vscode-editor-font-weight').trim();
  return rawEditorFontWeight || 'normal';
};

const sanitizeThemeFontStyle = (value: string): string => `${value ?? ''}`.trim().replace(styleValueInjectionPattern, ' ');

const normalizeThemeLineHeight = (value: number | undefined, fallback: number): number => {
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    return fallback;
  }
  return Math.min(3, Math.max(1, value));
};

const normalizeThemeFontSize = (value: number | null | undefined): string => {
  if (typeof value !== 'number' || !Number.isFinite(value) || value <= 0) {
    return '';
  }
  return `${value}px`;
};

const normalizeThemeFontWeight = (value: string | undefined, fallback: string): string => {
  const normalized = sanitizeThemeFontStyle(value ?? '');
  if (!normalized) {
    return fallback;
  }
  if (/^var\(\s*--vscode-editor-font-weight\s*\)$/i.test(normalized)) {
    return fallback;
  }
  return normalized;
};

const parseCssRgbColor = (value: string): { r: number; g: number; b: number } | null => {
  const match = value.trim().match(/^rgba?\(\s*(.+?)\s*\)$/i);
  if (!match?.[1]) {
    return null;
  }
  const channels = match[1].split('/')[0]?.trim().split(/[\s,]+/).filter(Boolean) ?? [];
  const [r, g, b] = channels.slice(0, 3).map((channel) => Number.parseFloat(channel));
  if (![r, g, b].every(Number.isFinite)) {
    return null;
  }
  return { r, g, b };
};

const resolveCssColor = (value: string): { r: number; g: number; b: number } | null => {
  const probe = document.createElement('span');
  probe.style.color = value;
  document.documentElement.appendChild(probe);
  const resolved = window.getComputedStyle(probe).color;
  probe.remove();
  return parseCssRgbColor(resolved);
};

const getRelativeLuminance = ({ r, g, b }: { r: number; g: number; b: number }): number => {
  const normalize = (channel: number): number => {
    const value = Math.min(255, Math.max(0, channel)) / 255;
    return value <= 0.03928 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * normalize(r) + 0.7152 * normalize(g) + 0.0722 * normalize(b);
};

const getInsetBackground = (backgroundColor: string, base03: string): string => {
  const resolvedBackground = resolveCssColor(backgroundColor);
  if (!resolvedBackground || getRelativeLuminance(resolvedBackground) < 0.36) {
    return `color-mix(in srgb, ${backgroundColor} 88%, black 12%)`;
  }
  return `color-mix(in srgb, ${backgroundColor} 80%, ${base03} 20%)`;
};

const normalizeThemeHeadingSize = (value: number | undefined, fallback: string, unit: 'px' | 'em' = 'px'): string => {
  if (typeof value !== 'number' || !Number.isFinite(value) || value <= 0) {
    return fallback;
  }
  if (unit === 'em') {
    const normalized = value > 10 ? value / 16 : value;
    return `${Math.min(9, Math.max(0.5, normalized))}em`;
  }
  return `${value}px`;
};

export const applyThemeSettings = (theme?: ThemeSettings): void => {
  let resolvedTheme: ThemeSettings;
  const editorFontWeight = resolveEditorFontWeight();
  try {
    resolvedTheme = resolveTheme(theme);
  } catch (error) {
    console.error('[MEO webview] Failed to resolve theme payload, using defaults.', error);
    resolvedTheme = resolveTheme();
  }

  const rootStyle = document.documentElement.style;
  const insetBackground = getInsetBackground(resolvedTheme.backgroundColor, resolvedTheme.colors.base03);
  rootStyle.setProperty('--meo-background', resolvedTheme.backgroundColor);
  rootStyle.setProperty('--meo-inset-background', insetBackground);
  rootStyle.setProperty(
    '--meo-code-background',
    insetBackground
  );
  rootStyle.setProperty(
    '--meo-code-block-active-line-bg-live',
    `color-mix(in srgb, ${resolvedTheme.backgroundColor} 88%, ${resolvedTheme.colors.base03} 12%)`
  );
  rootStyle.setProperty(
    '--meo-surface-background',
    `color-mix(in srgb, ${resolvedTheme.backgroundColor} 86%, ${resolvedTheme.colors.base03} 14%)`
  );
  rootStyle.setProperty(
    '--meo-selection-bg',
    `color-mix(in srgb, ${resolvedTheme.colors.base05} 28%, transparent 72%)`
  );
  rootStyle.setProperty('--meo-caret-color', resolvedTheme.colors.base01);
  rootStyle.setProperty(
    '--meo-active-line-bg',
    `color-mix(in srgb, ${resolvedTheme.colors.base03} 35%, transparent 65%)`
  );

  for (const key of themeColorKeys) {
    rootStyle.setProperty(`--meo-color-${key}`, resolvedTheme.colors[key]);
  }

  for (const spec of SYNTAX_TAG_SPECS) {
    const tokenColor = resolvedTheme.syntaxTokens[spec.id];
    rootStyle.setProperty(`--meo-token-${spec.id}-color`, tokenColor);
  }

  const liveFont = sanitizeThemeFontStyle(resolvedTheme.fonts.liveFont);
  const sourceFont = sanitizeThemeFontStyle(resolvedTheme.fonts.sourceFont);
  const liveFontWeight = sanitizeThemeFontStyle(resolvedTheme.fonts.liveFontWeight);
  const sourceFontWeight = sanitizeThemeFontStyle(resolvedTheme.fonts.sourceFontWeight);
  const liveFontSize = normalizeThemeFontSize(resolvedTheme.fonts.liveFontSize);
  const sourceFontSize = normalizeThemeFontSize(resolvedTheme.fonts.sourceFontSize);
  const headingFontSizes = [
    normalizeThemeHeadingSize(resolvedTheme.fonts.h1FontSize, headingSizeFallbacks[0], 'em'),
    normalizeThemeHeadingSize(resolvedTheme.fonts.h2FontSize, headingSizeFallbacks[1], 'em'),
    normalizeThemeHeadingSize(resolvedTheme.fonts.h3FontSize, headingSizeFallbacks[2], 'em'),
    normalizeThemeHeadingSize(resolvedTheme.fonts.h4FontSize, headingSizeFallbacks[3], 'em'),
    normalizeThemeHeadingSize(resolvedTheme.fonts.h5FontSize, headingSizeFallbacks[4], 'em'),
    normalizeThemeHeadingSize(resolvedTheme.fonts.h6FontSize, headingSizeFallbacks[5], 'em')
  ];
  const headingFontWeights = [
    normalizeThemeFontWeight(resolvedTheme.fonts.h1FontWeight, defaultHeadingFontWeight),
    normalizeThemeFontWeight(resolvedTheme.fonts.h2FontWeight, defaultHeadingFontWeight),
    normalizeThemeFontWeight(resolvedTheme.fonts.h3FontWeight, defaultHeadingFontWeight),
    normalizeThemeFontWeight(resolvedTheme.fonts.h4FontWeight, defaultHeadingFontWeight),
    normalizeThemeFontWeight(resolvedTheme.fonts.h5FontWeight, defaultHeadingFontWeight),
    normalizeThemeFontWeight(resolvedTheme.fonts.h6FontWeight, defaultHeadingFontWeight)
  ];
  const liveLineHeight = normalizeThemeLineHeight(resolvedTheme.fonts.liveLineHeight, 1.5);
  const sourceLineHeight = normalizeThemeLineHeight(resolvedTheme.fonts.sourceLineHeight, 1.5);
  rootStyle.setProperty('--meo-font-live', liveFont || vscodeEditorFontFamily);
  rootStyle.setProperty('--meo-font-source', sourceFont || vscodeEditorFontFamily);
  rootStyle.setProperty('--meo-font-live-weight', liveFontWeight || editorFontWeight);
  rootStyle.setProperty('--meo-font-source-weight', sourceFontWeight || editorFontWeight);
  rootStyle.setProperty('--meo-font-live-size', liveFontSize || vscodeEditorFontSize);
  rootStyle.setProperty('--meo-font-source-size', sourceFontSize || vscodeEditorFontSize);
  for (const [index, size] of headingFontSizes.entries()) {
    rootStyle.setProperty(`--meo-heading-${index + 1}-size`, size);
  }
  for (const [index, weight] of headingFontWeights.entries()) {
    rootStyle.setProperty(`--meo-heading-${index + 1}-weight`, weight);
  }
  rootStyle.setProperty('--meo-heading-token-weight', defaultHeadingFontWeight);
  rootStyle.setProperty('--meo-line-height-live', `${liveLineHeight}`);
  rootStyle.setProperty('--meo-line-height-source', `${sourceLineHeight}`);
};
