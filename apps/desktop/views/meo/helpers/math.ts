// @ts-nocheck
import katex from 'katex';

export type LatexMathMode = 'inline' | 'display';

export interface LatexMathRange {
  from: number;
  to: number;
  mode: LatexMathMode;
  content: string;
  raw: string;
  fencedDisplay?: boolean;
}

export interface ParseLatexMathAtOptions {
  allowInline?: boolean;
  allowDisplay?: boolean;
}

export interface FencedDisplayMathInnerLineRange {
  innerStartLine: number;
  innerEndLine: number;
}

interface SimpleRange {
  from: number;
  to: number;
}

const MATH_RENDER_CACHE_LIMIT = 300;
const mathHtmlCache = new Map<string, string | null>();
const loggedMathRenderErrors = new Set<string>();
const INLINE_MATH_MARKDOWN_STRUCTURE_MARKERS = ['**', '__', '~~', '`', '](', '!['] as const;
const INLINE_MATH_PROSE_LENGTH_LIMIT = 120;
const INLINE_MATH_CURRENCY_CONTENT_RE = /^[0-9][0-9\s,._%+-]*$/;
const INLINE_MATH_TEX_COMMAND_RE = /\\[A-Za-z]+/;
const INLINE_MATH_TEX_SYMBOL_RE = /[_^{}]/;
const INLINE_MATH_ALPHA_WORD_RE = /\b[A-Za-z]{3,}\b/g;

function isEscaped(text: string, index: number): boolean {
  let slashCount = 0;
  for (let i = index - 1; i >= 0 && text[i] === '\\'; i -= 1) {
    slashCount += 1;
  }
  return slashCount % 2 === 1;
}

function isWhitespace(char: string | undefined): boolean {
  return char === ' ' || char === '\t' || char === '\n' || char === '\r';
}

export function resolveFencedDisplayMathInnerLineRange(
  startLine: number,
  endLine: number
): FencedDisplayMathInnerLineRange | null {
  if (!Number.isFinite(startLine) || !Number.isFinite(endLine)) {
    return null;
  }

  const innerStartLine = Math.max(1, Math.floor(startLine) + 1);
  const innerEndLine = Math.floor(endLine) - 1;
  if (innerEndLine < innerStartLine) {
    return null;
  }

  return {
    innerStartLine,
    innerEndLine,
  };
}

function lineStartAt(text: string, index: number): number {
  const previousBreak = text.lastIndexOf('\n', Math.max(0, index - 1));
  return previousBreak < 0 ? 0 : previousBreak + 1;
}

function lineEndAt(text: string, index: number): number {
  const nextBreak = text.indexOf('\n', Math.max(0, index));
  return nextBreak < 0 ? text.length : nextBreak;
}

function hasOwnLineDisplayFences(text: string, openIndex: number, closeIndex: number): boolean {
  const openLineStart = lineStartAt(text, openIndex);
  const openLineEnd = lineEndAt(text, openIndex + 2);
  if (text.slice(openLineStart, openIndex).trim()) {
    return false;
  }
  if (text.slice(openIndex + 2, openLineEnd).trim()) {
    return false;
  }

  const closeLineStart = lineStartAt(text, closeIndex);
  const closeLineEnd = lineEndAt(text, closeIndex + 2);
  if (text.slice(closeLineStart, closeIndex).trim()) {
    return false;
  }
  if (text.slice(closeIndex + 2, closeLineEnd).trim()) {
    return false;
  }

  return true;
}

function looksLikeCurrencyContent(content: string): boolean {
  return INLINE_MATH_CURRENCY_CONTENT_RE.test(content);
}

function hasTexCue(content: string): boolean {
  return INLINE_MATH_TEX_COMMAND_RE.test(content) || INLINE_MATH_TEX_SYMBOL_RE.test(content);
}

function hasMarkdownStructureMarker(content: string): boolean {
  for (const marker of INLINE_MATH_MARKDOWN_STRUCTURE_MARKERS) {
    if (content.includes(marker)) {
      return true;
    }
  }
  return false;
}

function hasAtLeastTwoAlphaWords(content: string): boolean {
  INLINE_MATH_ALPHA_WORD_RE.lastIndex = 0;
  let count = 0;
  while (INLINE_MATH_ALPHA_WORD_RE.exec(content)) {
    count += 1;
    if (count >= 2) {
      INLINE_MATH_ALPHA_WORD_RE.lastIndex = 0;
      return true;
    }
  }
  INLINE_MATH_ALPHA_WORD_RE.lastIndex = 0;
  return false;
}

function shouldRejectInlineMathCandidate(content: string): boolean {
  const trimmed = content.trim();
  if (!trimmed) {
    return true;
  }
  if (looksLikeCurrencyContent(trimmed)) {
    return true;
  }
  if (hasMarkdownStructureMarker(content)) {
    return true;
  }
  const hasTex = hasTexCue(content);
  if (!hasTex && trimmed.length > INLINE_MATH_PROSE_LENGTH_LIMIT) {
    return true;
  }
  if (!hasTex && hasAtLeastTwoAlphaWords(content)) {
    return true;
  }
  return false;
}

function isInlineMathOpen(text: string, index: number): boolean {
  const next = text[index + 1];
  if (!next || next === '$') {
    return false;
  }
  if (isWhitespace(next)) {
    return false;
  }
  return true;
}

function isInlineMathClose(text: string, index: number): boolean {
  const previous = text[index - 1];
  if (!previous || isWhitespace(previous)) {
    return false;
  }
  return true;
}

function normalizeRanges(ranges: SimpleRange[]): SimpleRange[] {
  const filtered = ranges
    .filter((range) => Number.isFinite(range.from) && Number.isFinite(range.to) && range.to > range.from)
    .sort((left, right) => left.from - right.from || left.to - right.to);
  if (!filtered.length) {
    return [];
  }

  const merged: SimpleRange[] = [filtered[0]];
  for (let i = 1; i < filtered.length; i += 1) {
    const current = filtered[i];
    const last = merged[merged.length - 1];
    if (current.from <= last.to) {
      if (current.to > last.to) {
        last.to = current.to;
      }
      continue;
    }
    merged.push({ from: current.from, to: current.to });
  }

  return merged;
}

function findRangeContaining(
  ranges: ReadonlyArray<SimpleRange>,
  index: number,
  startAt: number
): { rangeIndex: number; range: SimpleRange | null } {
  let rangeIndex = startAt;
  while (rangeIndex < ranges.length && index >= ranges[rangeIndex].to) {
    rangeIndex += 1;
  }
  const range =
    rangeIndex < ranges.length && index >= ranges[rangeIndex].from && index < ranges[rangeIndex].to
      ? ranges[rangeIndex]
      : null;
  return { rangeIndex, range };
}

function findInlineMathClose(
  text: string,
  start: number,
  excludedRanges: ReadonlyArray<SimpleRange>,
  initialRangeIndex: number
): { close: number; rangeIndex: number } {
  let braceLevel = 0;
  let rangeIndex = initialRangeIndex;
  let index = start;

  while (index < text.length) {
    const withinExcluded = findRangeContaining(excludedRanges, index, rangeIndex);
    rangeIndex = withinExcluded.rangeIndex;
    if (withinExcluded.range) {
      index = withinExcluded.range.to;
      continue;
    }

    const character = text[index];
    if (character === '\n' || character === '\r') {
      return { close: -1, rangeIndex };
    }

    if (character === '$' && !isEscaped(text, index) && braceLevel <= 0 && isInlineMathClose(text, index)) {
      return { close: index, rangeIndex };
    }

    if (character === '\\') {
      index += 2;
      continue;
    }

    if (character === '{') {
      braceLevel += 1;
    } else if (character === '}' && braceLevel > 0) {
      braceLevel -= 1;
    }

    index += 1;
  }

  return { close: -1, rangeIndex };
}

function findDisplayMathClose(
  text: string,
  start: number,
  excludedRanges: ReadonlyArray<SimpleRange>,
  initialRangeIndex: number
): { close: number; rangeIndex: number } {
  let braceLevel = 0;
  let rangeIndex = initialRangeIndex;
  let index = start;

  while (index < text.length - 1) {
    const withinExcluded = findRangeContaining(excludedRanges, index, rangeIndex);
    rangeIndex = withinExcluded.rangeIndex;
    if (withinExcluded.range) {
      index = withinExcluded.range.to;
      continue;
    }

    const character = text[index];
    if (character === '$' && text[index + 1] === '$' && !isEscaped(text, index) && braceLevel <= 0) {
      return { close: index, rangeIndex };
    }

    if (character === '\\') {
      index += 2;
      continue;
    }

    if (character === '{') {
      braceLevel += 1;
    } else if (character === '}' && braceLevel > 0) {
      braceLevel -= 1;
    }

    index += 1;
  }

  return { close: -1, rangeIndex };
}

export function parseLatexMathAt(
  text: string,
  index: number,
  options: ParseLatexMathAtOptions = {}
): LatexMathRange | null {
  const { allowInline = true, allowDisplay = true } = options;
  if (!text || index < 0 || index >= text.length) {
    return null;
  }
  if (text[index] !== '$' || isEscaped(text, index)) {
    return null;
  }

  if (allowDisplay && text[index + 1] === '$') {
    const closeResult = findDisplayMathClose(text, index + 2, [], 0);
    if (closeResult.close > index + 2) {
      const rawContent = text.slice(index + 2, closeResult.close);
      const content = rawContent.trim();
      const multiline = rawContent.includes('\n') || rawContent.includes('\r');
      const fencedDisplay = hasOwnLineDisplayFences(text, index, closeResult.close);
      if (multiline && !fencedDisplay) {
        return null;
      }
      if (content) {
        return {
          from: index,
          to: closeResult.close + 2,
          mode: 'display',
          content,
          raw: text.slice(index, closeResult.close + 2),
          fencedDisplay,
        };
      }
    }
    return null;
  }

  if (!allowInline || !isInlineMathOpen(text, index)) {
    return null;
  }

  const closeResult = findInlineMathClose(text, index + 1, [], 0);
  if (closeResult.close <= index + 1) {
    return null;
  }

  const content = text.slice(index + 1, closeResult.close);
  if (shouldRejectInlineMathCandidate(content)) {
    return null;
  }

  return {
    from: index,
    to: closeResult.close + 1,
    mode: 'inline',
    content,
    raw: text.slice(index, closeResult.close + 1),
  };
}

export function collectLatexMathRanges(
  text: string,
  options: {
    baseOffset?: number;
    excludedRanges?: SimpleRange[];
  } = {}
): LatexMathRange[] {
  const { baseOffset = 0, excludedRanges = [] } = options;
  const ranges: LatexMathRange[] = [];
  const excluded = normalizeRanges(excludedRanges);
  let rangeIndex = 0;

  for (let index = 0; index < text.length;) {
    const withinExcluded = findRangeContaining(excluded, index, rangeIndex);
    rangeIndex = withinExcluded.rangeIndex;
    if (withinExcluded.range) {
      index = withinExcluded.range.to;
      continue;
    }

    if (text[index] !== '$' || isEscaped(text, index)) {
      index += 1;
      continue;
    }

    if (text[index + 1] === '$') {
      const closeResult = findDisplayMathClose(text, index + 2, excluded, rangeIndex);
      rangeIndex = closeResult.rangeIndex;
      if (closeResult.close > index + 2) {
        const rawContent = text.slice(index + 2, closeResult.close);
        const content = rawContent.trim();
        const multiline = rawContent.includes('\n') || rawContent.includes('\r');
        const fencedDisplay = hasOwnLineDisplayFences(text, index, closeResult.close);
        if (multiline && !fencedDisplay) {
          index = closeResult.close + 2;
          continue;
        }
        if (content) {
          ranges.push({
            from: baseOffset + index,
            to: baseOffset + closeResult.close + 2,
            mode: 'display',
            content,
            raw: text.slice(index, closeResult.close + 2),
            fencedDisplay,
          });
        }
        index = closeResult.close + 2;
        continue;
      }
      index += 2;
      continue;
    }

    if (!isInlineMathOpen(text, index)) {
      index += 1;
      continue;
    }

    const closeResult = findInlineMathClose(text, index + 1, excluded, rangeIndex);
    rangeIndex = closeResult.rangeIndex;
    if (closeResult.close <= index + 1) {
      index += 1;
      continue;
    }

    const content = text.slice(index + 1, closeResult.close);
    if (shouldRejectInlineMathCandidate(content)) {
      // Skip only the opening '$' so later inline candidates on this line are still discovered.
      index += 1;
      continue;
    }

    ranges.push({
      from: baseOffset + index,
      to: baseOffset + closeResult.close + 1,
      mode: 'inline',
      content,
      raw: text.slice(index, closeResult.close + 1),
    });
    index = closeResult.close + 1;
  }

  return ranges;
}

function pushMathRenderCache(key: string, value: string | null): void {
  if (mathHtmlCache.has(key)) {
    mathHtmlCache.delete(key);
  }
  mathHtmlCache.set(key, value);
  if (mathHtmlCache.size <= MATH_RENDER_CACHE_LIMIT) {
    return;
  }
  const oldestKey = mathHtmlCache.keys().next().value;
  if (oldestKey !== undefined) {
    mathHtmlCache.delete(oldestKey);
  }
}

function getMathRenderCache(key: string): string | null | undefined {
  const cached = mathHtmlCache.get(key);
  if (cached === undefined) {
    return undefined;
  }
  mathHtmlCache.delete(key);
  mathHtmlCache.set(key, cached);
  return cached;
}

export function renderLatexMathToHtml(content: string, mode: LatexMathMode): string | null {
  const normalized = String(content ?? '').trim();
  if (!normalized) {
    return null;
  }

  const cacheKey = `${mode}:${normalized}`;
  const cached = getMathRenderCache(cacheKey);
  if (cached !== undefined) {
    return cached;
  }

  try {
    const html = katex.renderToString(normalized, {
      displayMode: mode === 'display',
      throwOnError: true,
      strict: 'ignore',
      output: 'html',
    });
    pushMathRenderCache(cacheKey, html);
    return html;
  } catch (error: unknown) {
    const message = String(error instanceof Error ? error.message : error);
    const logKey = `${mode}:${normalized}:${message}`;
    if (!loggedMathRenderErrors.has(logKey)) {
      loggedMathRenderErrors.add(logKey);
      console.warn('[MEO math] KaTeX render failed', {
        mode,
        message,
        expression: normalized.slice(0, 160),
      });
    }
    pushMathRenderCache(cacheKey, null);
    return null;
  }
}

export function createLatexMathElement(content: string, mode: LatexMathMode): HTMLElement | null {
  const html = renderLatexMathToHtml(content, mode);
  if (!html) {
    return null;
  }

  const wrapper = document.createElement('span');
  wrapper.className = `meo-md-math meo-md-math-${mode}`;
  wrapper.innerHTML = html;
  return wrapper;
}
