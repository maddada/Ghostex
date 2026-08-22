// @ts-nocheck
import { tags as t, type Tag } from '@lezer/highlight';

export const themeColorKeys = [
  'base01',
  'base02',
  'base03',
  'base04',
  'base05',
  'base06',
  'base07',
  'base08',
  'base09'
] as const;

export type ThemeColorKey = (typeof themeColorKeys)[number];
export type ThemeColors = Record<ThemeColorKey, string>;

export const minThemeLineHeight = 1;
export const maxThemeLineHeight = 3;
export const defaultThemeLineHeight = 1.5;

export type ThemeFonts = {
  liveFont: string;
  sourceFont: string;
  liveFontWeight: string;
  sourceFontWeight: string;
  liveFontSize: number | null;
  sourceFontSize: number | null;
  h1FontSize: number | null;
  h2FontSize: number | null;
  h3FontSize: number | null;
  h4FontSize: number | null;
  h5FontSize: number | null;
  h6FontSize: number | null;
  h1FontWeight: string;
  h2FontWeight: string;
  h3FontWeight: string;
  h4FontWeight: string;
  h5FontWeight: string;
  h6FontWeight: string;
  liveLineHeight: number;
  sourceLineHeight: number;
};

export type SyntaxTokenStyleSpec = {
  id: string;
  tags: Tag | readonly Tag[];
  paletteKey: ThemeColorKey;
  style: {
    fontWeight?: 'normal' | 'bold' | 'bolder' | 'lighter' | string;
    fontStyle?: 'normal' | 'italic' | 'oblique' | string;
    textDecoration?: string;
    borderBottom?: string;
  };
};

export const SYNTAX_TAG_SPECS: readonly SyntaxTokenStyleSpec[] = [
  {
    id: 'keyword',
    tags: [t.keyword, t.controlKeyword, t.moduleKeyword],
    paletteKey: 'base04',
    style: { fontWeight: 'bold' }
  },
  {
    id: 'identifier',
    tags: [t.name, t.deleted, t.character],
    paletteKey: 'base05',
    style: {}
  },
  {
    id: 'macroName',
    tags: t.macroName,
    paletteKey: 'base06',
    style: { fontStyle: 'italic' }
  },
  {
    id: 'variableName',
    tags: t.variableName,
    paletteKey: 'base01',
    style: {}
  },
  {
    id: 'propertyName',
    tags: t.propertyName,
    paletteKey: 'base09',
    style: { fontStyle: 'normal' }
  },
  {
    id: 'typeName',
    tags: t.typeName,
    paletteKey: 'base06',
    style: {}
  },
  {
    id: 'className',
    tags: t.className,
    paletteKey: 'base09',
    style: {}
  },
  {
    id: 'namespace',
    tags: t.namespace,
    paletteKey: 'base05',
    style: {}
  },
  {
    id: 'operator',
    tags: t.operator,
    paletteKey: 'base01',
    style: {}
  },
  {
    id: 'operatorKeyword',
    tags: t.operatorKeyword,
    paletteKey: 'base04',
    style: {}
  },
  {
    id: 'punctuation',
    tags: [t.bracket, t.brace, t.punctuation, t.squareBracket, t.angleBracket],
    paletteKey: 'base01',
    style: {}
  },
  {
    id: 'functionName',
    tags: t.function(t.variableName),
    paletteKey: 'base06',
    style: {}
  },
  {
    id: 'labelName',
    tags: t.labelName,
    paletteKey: 'base02',
    style: {}
  },
  {
    id: 'definitionFunction',
    tags: t.definition(t.function(t.variableName)),
    paletteKey: 'base06',
    style: {}
  },
  {
    id: 'definedVariable',
    tags: t.definition(t.variableName),
    paletteKey: 'base05',
    style: {}
  },
  {
    id: 'number',
    tags: t.number,
    paletteKey: 'base08',
    style: {}
  },
  {
    id: 'changed',
    tags: t.changed,
    paletteKey: 'base08',
    style: {}
  },
  {
    id: 'annotation',
    tags: t.annotation,
    paletteKey: 'base04',
    style: { fontStyle: 'italic' }
  },
  {
    id: 'modifier',
    tags: t.modifier,
    paletteKey: 'base04',
    style: { fontStyle: 'italic' }
  },
  {
    id: 'self',
    tags: t.self,
    paletteKey: 'base04',
    style: { fontStyle: 'italic' }
  },
  {
    id: 'color',
    tags: t.color,
    paletteKey: 'base08',
    style: {}
  },
  {
    id: 'constant',
    tags: [t.constant(t.name), t.standard(t.name)],
    paletteKey: 'base08',
    style: {}
  },
  {
    id: 'atom',
    tags: t.atom,
    paletteKey: 'base05',
    style: {}
  },
  {
    id: 'bool',
    tags: t.bool,
    paletteKey: 'base08',
    style: {}
  },
  {
    id: 'specialVariable',
    tags: t.special(t.variableName),
    paletteKey: 'base08',
    style: {}
  },
  {
    id: 'specialString',
    tags: t.special(t.string),
    paletteKey: 'base07',
    style: {}
  },
  {
    id: 'regexp',
    tags: t.regexp,
    paletteKey: 'base07',
    style: {}
  },
  {
    id: 'string',
    tags: t.string,
    paletteKey: 'base07',
    style: {}
  },
  {
    id: 'typeDefinition',
    tags: t.definition(t.typeName),
    paletteKey: 'base06',
    style: { fontWeight: 'bold' }
  },
  {
    id: 'meta',
    tags: t.meta,
    paletteKey: 'base02',
    style: {}
  },
  {
    id: 'comment',
    tags: [t.comment, t.docComment],
    paletteKey: 'base02',
    style: { fontStyle: 'italic' }
  },
  {
    id: 'tagName',
    tags: t.tagName,
    paletteKey: 'base04',
    style: {}
  },
  {
    id: 'attributeName',
    tags: t.attributeName,
    paletteKey: 'base09',
    style: {}
  },
  {
    id: 'invalid',
    tags: t.invalid,
    paletteKey: 'base01',
    style: { textDecoration: 'underline wavy', borderBottom: '1px wavy #e06c75' }
  },
  {
    id: 'deleted',
    tags: t.deleted,
    paletteKey: 'base04',
    style: {}
  },
  {
    id: 'monospace',
    tags: t.monospace,
    paletteKey: 'base07',
    style: {}
  },
  {
    id: 'heading',
    tags: t.heading,
    paletteKey: 'base04',
    style: { fontWeight: '600' }
  },
  {
    id: 'emphasis',
    tags: t.emphasis,
    paletteKey: 'base01',
    style: { fontStyle: 'italic' }
  },
  {
    id: 'strong',
    tags: t.strong,
    paletteKey: 'base07',
    style: { fontWeight: '600' }
  },
  {
    id: 'strikethrough',
    tags: t.strikethrough,
    paletteKey: 'base01',
    style: { textDecoration: 'line-through' }
  },
  {
    id: 'quote',
    tags: t.quote,
    paletteKey: 'base07',
    style: {}
  },
  {
    id: 'contentSeparator',
    tags: t.contentSeparator,
    paletteKey: 'base02',
    style: {}
  },
  {
    id: 'link',
    tags: t.link,
    paletteKey: 'base05',
    style: {}
  },
  {
    id: 'url',
    tags: t.url,
    paletteKey: 'base05',
    style: {}
  },
  {
    id: 'processingInstruction',
    tags: t.processingInstruction,
    paletteKey: 'base02',
    style: {}
  }
] as const;

export type ThemeSyntaxTokenKey = (typeof SYNTAX_TAG_SPECS)[number]['id'];
export type ThemeSyntaxTokens = Record<ThemeSyntaxTokenKey, string>;
type ThemeSyntaxTokenPalette = Record<ThemeSyntaxTokenKey, ThemeColorKey>;

export type ThemeSettings = {
  id: string;
  name: string;
  backgroundColor: string;
  colors: ThemeColors;
  syntaxTokens: ThemeSyntaxTokens;
  fonts: ThemeFonts;
};

export type ThemeSettingsPayload = Omit<ThemeSettings, 'syntaxTokens'> & {
  syntaxTokens: ThemeSyntaxTokens;
};

export const defaultThemeColors: ThemeColors = {
  base01: 'var(--vscode-editor-foreground)',
  base02: '#676f7d',
  base03: '#3e444d',
  base04: '#e06c75',
  base05: '#61afef',
  base06: '#66d9ef',
  base07: '#e5c07b',
  base08: '#c678dd',
  base09: '#98c379'
};

export const defaultThemeBackgroundColor = 'var(--vscode-editor-background)';
export const defaultCodeBlockBackgroundColor = '#1e1e1e';

export const defaultThemeFonts: ThemeFonts = {
  liveFont: '',
  sourceFont: '',
  liveFontWeight: '',
  sourceFontWeight: '',
  liveFontSize: null,
  sourceFontSize: null,
  h1FontSize: 1.6,
  h2FontSize: 1.5,
  h3FontSize: 1.3,
  h4FontSize: 1.2,
  h5FontSize: 1.1,
  h6FontSize: 1,
  h1FontWeight: '600',
  h2FontWeight: '600',
  h3FontWeight: '600',
  h4FontWeight: '600',
  h5FontWeight: '600',
  h6FontWeight: '600',
  liveLineHeight: defaultThemeLineHeight,
  sourceLineHeight: defaultThemeLineHeight
};

const defaultSyntaxTokenPalette = SYNTAX_TAG_SPECS.reduce((acc, spec) => {
  acc[spec.id as ThemeSyntaxTokenKey] = spec.paletteKey;
  return acc;
}, {} as ThemeSyntaxTokenPalette);

const buildSyntaxTokenColors = (
  colors: ThemeColors,
  paletteOverrides: Partial<ThemeSyntaxTokenPalette> = {}
): ThemeSyntaxTokens => {
  const tokens = {} as ThemeSyntaxTokens;

  for (const tokenId of Object.keys(defaultSyntaxTokenPalette) as ThemeSyntaxTokenKey[]) {
    const paletteKey = paletteOverrides[tokenId] ?? defaultSyntaxTokenPalette[tokenId];
    tokens[tokenId] = colors[paletteKey];
  }

  return tokens;
};

const createThemeFromColors = (params: {
  id: string;
  name: string;
  backgroundColor?: string;
  colors?: Partial<ThemeColors>;
  syntaxTokenPaletteOverrides?: Partial<ThemeSyntaxTokenPalette>;
  syntaxTokenOverrides?: Partial<ThemeSyntaxTokens>;
  fonts?: Partial<ThemeFonts>;
}): ThemeSettings => {
  const colors = { ...defaultThemeColors, ...params.colors };

  return {
    id: params.id,
    name: params.name,
    backgroundColor: params.backgroundColor ?? defaultThemeBackgroundColor,
    colors,
    syntaxTokens: {
      ...buildSyntaxTokenColors(colors, params.syntaxTokenPaletteOverrides),
      ...(params.syntaxTokenOverrides ?? {})
    } as ThemeSyntaxTokens,
    fonts: { ...defaultThemeFonts, ...(params.fonts ?? {}) }
  };
};

export const themePresets: readonly ThemeSettings[] = [
  createThemeFromColors({
    id: 'one-monokai',
    name: 'One Monokai (default)',
  }),
  createThemeFromColors({
    id: 'one-dark-pro',
    name: 'One Dark Pro',
    colors: {
      base02: '#5c6370',
      base03: '#3e4451',
      base04: '#c678dd',
      base05: '#61afef',
      base06: '#56b6c2',
      base07: '#e5c07b',
      base08: '#d19a66',
      base09: '#98c379'
    },
    syntaxTokenPaletteOverrides: {
      string: 'base09'
    }
  }),
  createThemeFromColors({
    id: 'dracula',
    name: 'Dracula',
    colors: {
      base02: '#6272a4',
      base03: '#44475a',
      base04: '#ff79c6',
      base05: '#bd93f9',
      base06: '#8be9fd',
      base07: '#f1fa8c',
      base08: '#50fa7b',
      base09: '#ffb86c'
    },
    syntaxTokenPaletteOverrides: {
      constant: 'base09',
      bool: 'base09'
    }
  }),
  createThemeFromColors({
    id: 'gruvbox',
    name: 'Gruvbox',
    colors: {
      base02: '#a89984',
      base03: '#3c3836',
      base04: '#fb4934',
      base05: '#83a598',
      base06: '#8ec07c',
      base07: '#fabd2f',
      base08: '#d3869b',
      base09: '#fe8019'
    }
  }),
  createThemeFromColors({
    id: 'nord',
    name: 'Nord',
    colors: {
      base02: '#616e88',
      base03: '#3b4252',
      base04: '#bf616a',
      base05: '#81a1c1',
      base06: '#88c0d0',
      base07: '#ebcb8b',
      base08: '#b48ead',
      base09: '#a3be8c'
    }
  }),
  createThemeFromColors({
    id: 'solarized-dark',
    name: 'Solarized Dark',
    colors: {
      base02: '#586e75',
      base03: '#073642',
      base04: '#dc322f',
      base05: '#268bd2',
      base06: '#2aa198',
      base07: '#b58900',
      base08: '#d33682',
      base09: '#859900'
    }
  }),
  createThemeFromColors({
    id: 'catppuccin-mocha',
    name: 'Catppuccin Mocha',
    colors: {
      base02: '#a6adc8',
      base03: '#313244',
      base04: '#f38ba8',
      base05: '#89b4fa',
      base06: '#89dceb',
      base07: '#f9e2af',
      base08: '#cba6f7',
      base09: '#a6e3a1'
    }
  }),
  createThemeFromColors({
    id: 'tokyo-night',
    name: 'Tokyo Night',
    colors: {
      base02: '#a9b1d6',
      base03: '#3b4261',
      base04: '#f7768e',
      base05: '#7aa2f7',
      base06: '#7dcfff',
      base07: '#e0af68',
      base08: '#bb9af7',
      base09: '#9ece6a'
    }
  }),
  createThemeFromColors({
    id: 'github-dark',
    name: 'GitHub Dark',
    colors: {
      base02: '#8b949e',
      base03: '#30363d',
      base04: '#ff7b72',
      base05: '#79c0ff',
      base06: '#56d4dd',
      base07: '#d29922',
      base08: '#d2a8ff',
      base09: '#56d364'
    }
  }),
  createThemeFromColors({
    id: 'github-light',
    name: 'GitHub Light',
    backgroundColor: '#ffffff',
    colors: {
      base01: '#24292f',
      base02: '#57606a',
      base03: '#d0d7de',
      base04: '#cf222e',
      base05: '#0969da',
      base06: '#1a7f37',
      base07: '#9a6700',
      base08: '#8250df',
      base09: '#116329'
    },
    syntaxTokenPaletteOverrides: {
      string: 'base09',
      comment: 'base02',
      quote: 'base02'
    }
  })
] as const;

export const defaultThemeSettings: ThemeSettings = themePresets[0] as ThemeSettings;

const hexColorRegex = /^#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/;
const rgbColorRegex = /^rgba?\(\s*(?:\d{1,3}\s*,\s*){2}\d{1,3}(?:\s*,\s*(?:0(?:\.\d+)?|1(?:\.0+)?|\d*\.?\d+))?\s*\)$/;
const hslColorRegex = /^hsla?\(\s*(?:[+\-]?\d+(?:\.\d+)?(?:deg|rad|grad|turn)?\s*,\s*){2}\d{1,3}%?(?:\s*,\s*(?:0(?:\.\d+)?|1(?:\.0+)?|\d*\.?\d+))?\s*\)$/;
const cssVarColorRegex = /^var\(\s*--[A-Za-z0-9_-]+\s*(?:,\s*[^)]+)?\)$/;

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

const normalizeString = (value: unknown, fallback: string): string => {
  const candidate = `${value ?? ''}`.trim();
  return candidate || fallback;
};

const sanitizeSyntaxTokenId = (value: string): ThemeSyntaxTokenKey | null => {
  return (SYNTAX_TAG_SPECS.find((spec) => spec.id === value) ? (value as ThemeSyntaxTokenKey) : null);
};

const isValidThemeColor = (value: string): boolean => {
  if (!value || !value.trim()) {
    return false;
  }
  const candidate = value.trim();
  return hexColorRegex.test(candidate) || rgbColorRegex.test(candidate) || hslColorRegex.test(candidate) || cssVarColorRegex.test(candidate);
};

const sanitizeThemeColor = (value: unknown, fallback: string): string => {
  if (typeof value !== 'string') {
    return fallback;
  }
  return isValidThemeColor(value) ? value.trim() : fallback;
};

const sanitizeThemeFont = (value: unknown, fallback: string): string => {
  if (value === null || value === undefined) {
    return fallback;
  }
  if (typeof value !== 'string') {
    return fallback;
  }
  const trimmed = value.trim();
  if (!trimmed) {
    return fallback;
  }
  return trimmed.replace(/[\n\r;{}]/g, ' ');
};

const sanitizeThemeOptionalPositiveNumber = (value: unknown, fallback: number | null): number | null => {
  if (value === null || value === undefined) {
    return fallback;
  }
  if (typeof value !== 'number' || !Number.isFinite(value) || value <= 0) {
    return fallback;
  }
  return value;
};

const sanitizeThemeOptionalNumberInRange = (
  value: unknown,
  fallback: number | null,
  min: number,
  max: number
): number | null => {
  if (value === null || value === undefined) {
    return fallback;
  }
  if (typeof value !== 'number' || !Number.isFinite(value) || value < min || value > max) {
    return fallback;
  }
  return value;
};

const sanitizeThemeLineHeight = (value: unknown, fallback: number): number => {
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    return fallback;
  }
  return Math.min(maxThemeLineHeight, Math.max(minThemeLineHeight, value));
};

type HeadingFontSizeKey = `h${1 | 2 | 3 | 4 | 5 | 6}FontSize`;
type HeadingFontWeightKey = `h${1 | 2 | 3 | 4 | 5 | 6}FontWeight`;

const headingFontSizeKeys = [
  'h1FontSize',
  'h2FontSize',
  'h3FontSize',
  'h4FontSize',
  'h5FontSize',
  'h6FontSize'
] as const satisfies readonly HeadingFontSizeKey[];

const headingFontWeightKeys = [
  'h1FontWeight',
  'h2FontWeight',
  'h3FontWeight',
  'h4FontWeight',
  'h5FontWeight',
  'h6FontWeight'
] as const satisfies readonly HeadingFontWeightKey[];

const themeFontKeys = [
  'liveFont',
  'sourceFont',
  'liveFontWeight',
  'sourceFontWeight',
  'liveFontSize',
  'sourceFontSize',
  ...headingFontSizeKeys,
  ...headingFontWeightKeys,
  'liveLineHeight',
  'sourceLineHeight'
] as const satisfies readonly (keyof ThemeFonts)[];

const resolveThemeFonts = (raw?: unknown): ThemeFonts => {
  const value = isRecord(raw) ? raw : {};
  const resolvedHeadingFontSizes = headingFontSizeKeys.reduce((acc, key) => {
    acc[key] = sanitizeThemeOptionalNumberInRange(value[key], defaultThemeFonts[key], 1, 3);
    return acc;
  }, {} as Pick<ThemeFonts, HeadingFontSizeKey>);
  const resolvedHeadingFontWeights = headingFontWeightKeys.reduce((acc, key) => {
    acc[key] = sanitizeThemeFont(value[key], defaultThemeFonts[key]);
    return acc;
  }, {} as Pick<ThemeFonts, HeadingFontWeightKey>);

  return {
    liveFont: sanitizeThemeFont(value.liveFont, defaultThemeFonts.liveFont),
    sourceFont: sanitizeThemeFont(value.sourceFont, defaultThemeFonts.sourceFont),
    liveFontWeight: sanitizeThemeFont(value.liveFontWeight, defaultThemeFonts.liveFontWeight),
    sourceFontWeight: sanitizeThemeFont(value.sourceFontWeight, defaultThemeFonts.sourceFontWeight),
    liveFontSize: sanitizeThemeOptionalPositiveNumber(value.liveFontSize, defaultThemeFonts.liveFontSize),
    sourceFontSize: sanitizeThemeOptionalPositiveNumber(value.sourceFontSize, defaultThemeFonts.sourceFontSize),
    ...resolvedHeadingFontSizes,
    ...resolvedHeadingFontWeights,
    liveLineHeight: sanitizeThemeLineHeight(value.liveLineHeight, defaultThemeFonts.liveLineHeight),
    sourceLineHeight: sanitizeThemeLineHeight(value.sourceLineHeight, defaultThemeFonts.sourceLineHeight)
  };
};

const resolveThemeColors = (raw?: unknown): ThemeColors => {
  const value = isRecord(raw) ? raw : {};
  const colors = {} as ThemeColors;

  for (const key of themeColorKeys) {
    colors[key] = sanitizeThemeColor(value[key], defaultThemeColors[key]);
  }

  return colors;
};

const resolveThemeSyntaxTokens = (raw: unknown, colors: ThemeColors): ThemeSyntaxTokens => {
  const value = isRecord(raw) ? raw : {};
  const defaults = buildSyntaxTokenColors(colors);
  const tokens = {} as ThemeSyntaxTokens;

  for (const key of Object.keys(defaults) as ThemeSyntaxTokenKey[]) {
    tokens[key] = sanitizeThemeColor(value[key], defaults[key]);
  }

  return tokens;
};

export const serializeThemeSettings = (theme: ThemeSettings): ThemeSettingsPayload => {
  const defaults = buildSyntaxTokenColors(theme.colors);
  const syntaxTokens = {} as ThemeSyntaxTokens;

  for (const key of Object.keys(defaults) as ThemeSyntaxTokenKey[]) {
    syntaxTokens[key] = theme.syntaxTokens[key] === defaults[key] ? '' : theme.syntaxTokens[key];
  }

  return {
    id: theme.id,
    name: theme.name,
    backgroundColor: theme.backgroundColor,
    colors: { ...theme.colors },
    syntaxTokens,
    fonts: { ...theme.fonts }
  };
};

export const resolveTheme = (themeOverride?: Partial<ThemeSettings>): ThemeSettings => {
  const colors = resolveThemeColors(themeOverride?.colors);
  return {
    id: normalizeString(themeOverride?.id, defaultThemeSettings.id),
    name: normalizeString(themeOverride?.name, defaultThemeSettings.name),
    backgroundColor: sanitizeThemeColor(themeOverride?.backgroundColor, defaultThemeBackgroundColor),
    colors,
    syntaxTokens: resolveThemeSyntaxTokens(themeOverride?.syntaxTokens, colors),
    fonts: resolveThemeFonts(themeOverride?.fonts)
  };
};

export type ThemeValidationResult = {
  success: true;
  theme: ThemeSettings;
} | {
  success: false;
  errors: string[];
};

export const validateThemePayload = (value: unknown): ThemeValidationResult => {
  if (!isRecord(value)) {
    return { success: false, errors: ['Theme payload must be an object.'] };
  }

  const errors: string[] = [];

  if (typeof value.id !== 'string' || !value.id.trim()) {
    errors.push('Theme "id" must be a non-empty string.');
  }

  if (typeof value.name !== 'string' || !value.name.trim()) {
    errors.push('Theme "name" must be a non-empty string.');
  }

  if (value.backgroundColor !== undefined) {
    if (typeof value.backgroundColor !== 'string' || !isValidThemeColor(value.backgroundColor)) {
      errors.push('Theme "backgroundColor" must be a valid hex, rgb, hsl, or var(--...) color string.');
    }
  }

  const rawColors = value.colors;
  if (!isRecord(rawColors)) {
    errors.push('Theme "colors" must be an object.');
  } else {
    for (const key of themeColorKeys) {
      if (!isValidThemeColor(rawColors[key] as string)) {
        errors.push(`Theme color "${key}" must be a valid hex, rgb, hsl, or var(--...) color string.`);
      }
    }
    const unknownColorKeys = Object.keys(rawColors).filter((key) => !(themeColorKeys as readonly string[]).includes(key));
    if (unknownColorKeys.length) {
      errors.push(`Theme colors contains unknown keys: ${unknownColorKeys.join(', ')}.`);
    }
  }

  const rawSyntaxTokens = value.syntaxTokens;
  if (!isRecord(rawSyntaxTokens)) {
    errors.push('Theme "syntaxTokens" must be an object.');
  } else {
    for (const [tokenId, tokenColor] of Object.entries(rawSyntaxTokens)) {
      const normalizedTokenId = sanitizeSyntaxTokenId(tokenId);
      if (normalizedTokenId === null) {
        continue;
      }
      if (typeof tokenColor !== 'string') {
        errors.push(`Theme token "${tokenId}" must be a string.`);
        continue;
      }
      if (!tokenColor.trim()) {
        continue;
      }
      if (!isValidThemeColor(tokenColor)) {
        errors.push(`Theme token "${tokenId}" must be a valid hex, rgb, hsl, or var(--...) color string.`);
      }
    }
    const unknownTokens = Object.keys(rawSyntaxTokens).filter((tokenId) => sanitizeSyntaxTokenId(tokenId) === null);
    if (unknownTokens.length) {
      errors.push(`Theme syntaxTokens contains unknown keys: ${unknownTokens.join(', ')}.`);
    }
  }

  const rawFonts = value.fonts;
  if (!isRecord(rawFonts)) {
    errors.push('Theme "fonts" must be an object.');
  } else {
    if (typeof rawFonts.liveFont !== 'string') {
      errors.push('Theme font "liveFont" must be a string.');
    }
    if (typeof rawFonts.sourceFont !== 'string') {
      errors.push('Theme font "sourceFont" must be a string.');
    }
    if (rawFonts.liveFontWeight !== undefined && typeof rawFonts.liveFontWeight !== 'string') {
      errors.push('Theme font "liveFontWeight" must be a string.');
    }
    if (rawFonts.sourceFontWeight !== undefined && typeof rawFonts.sourceFontWeight !== 'string') {
      errors.push('Theme font "sourceFontWeight" must be a string.');
    }
    if (rawFonts.liveFontSize !== null && rawFonts.liveFontSize !== undefined
      && (typeof rawFonts.liveFontSize !== 'number' || !Number.isFinite(rawFonts.liveFontSize) || rawFonts.liveFontSize <= 0)) {
      errors.push('Theme font "liveFontSize" must be null or a positive number.');
    }
    if (rawFonts.sourceFontSize !== null && rawFonts.sourceFontSize !== undefined
      && (typeof rawFonts.sourceFontSize !== 'number' || !Number.isFinite(rawFonts.sourceFontSize) || rawFonts.sourceFontSize <= 0)) {
      errors.push('Theme font "sourceFontSize" must be null or a positive number.');
    }
    for (const key of headingFontSizeKeys) {
      validateOptionalThemeHeadingSize(rawFonts, key, errors, 1, 3);
    }
    for (const key of headingFontWeightKeys) {
      validateOptionalThemeFontString(rawFonts, key, errors);
    }
    if (typeof rawFonts.liveLineHeight !== 'number' || !Number.isFinite(rawFonts.liveLineHeight)) {
      errors.push('Theme font "liveLineHeight" must be a number.');
    } else if (rawFonts.liveLineHeight < minThemeLineHeight || rawFonts.liveLineHeight > maxThemeLineHeight) {
      errors.push(`Theme font "liveLineHeight" must be between ${minThemeLineHeight} and ${maxThemeLineHeight}.`);
    }
    if (typeof rawFonts.sourceLineHeight !== 'number' || !Number.isFinite(rawFonts.sourceLineHeight)) {
      errors.push('Theme font "sourceLineHeight" must be a number.');
    } else if (rawFonts.sourceLineHeight < minThemeLineHeight || rawFonts.sourceLineHeight > maxThemeLineHeight) {
      errors.push(`Theme font "sourceLineHeight" must be between ${minThemeLineHeight} and ${maxThemeLineHeight}.`);
    }
    const unknownFontKeys = Object.keys(rawFonts).filter((key) => !(themeFontKeys as readonly string[]).includes(key));
    if (unknownFontKeys.length) {
      errors.push(`Theme fonts contains unknown keys: ${unknownFontKeys.join(', ')}.`);
    }
  }

  if (errors.length > 0) {
    return { success: false, errors };
  }

  return { success: true, theme: resolveTheme(value as Partial<ThemeSettings>) };
};

function validateOptionalThemeHeadingSize(
  rawFonts: Record<string, unknown>,
  key: HeadingFontSizeKey,
  errors: string[],
  min?: number,
  max?: number
): void {
  const value = rawFonts[key];
  if (value === undefined || value === null) {
    return;
  }
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    errors.push(`Theme font "${key}" must be null or a number.`);
    return;
  }
  if (min !== undefined && max !== undefined) {
    if (value < min || value > max) {
      errors.push(`Theme font "${key}" must be null or a number between ${min} and ${max}.`);
    }
    return;
  }
  if (value <= 0) {
    errors.push(`Theme font "${key}" must be null or a positive number.`);
  }
}

function validateOptionalThemeFontString(
  rawFonts: Record<string, unknown>,
  key: HeadingFontWeightKey,
  errors: string[]
): void {
  const value = rawFonts[key];
  if (value !== undefined && typeof value !== 'string') {
    errors.push(`Theme font "${key}" must be a string.`);
  }
}
