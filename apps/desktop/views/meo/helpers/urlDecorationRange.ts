// @ts-nocheck
import { normalizeSourceHref } from './rawUrls';

export type UrlDecorationRange = {
  from: number;
  to: number;
};

export function trimDecoratedUrlRange(
  rawFrom: number,
  rawTo: number,
  rawText: string,
  normalizedHref: string
): UrlDecorationRange {
  if (rawFrom >= rawTo) {
    return { from: rawFrom, to: rawTo };
  }
  if (!rawText) {
    return { from: rawFrom, to: rawTo };
  }

  const rawLength = rawTo - rawFrom;
  let trimStart = 0;
  let trimEnd = 0;
  let trimmedText = rawText;

  while (trimmedText.length >= 2) {
    const open = trimmedText[0];
    const close = trimmedText[trimmedText.length - 1];
    if ((open === '"' && close === '"') || (open === "'" && close === "'") || (open === '`' && close === '`')) {
      const unwrapped = trimmedText.slice(1, -1);
      if (normalizeSourceHref(unwrapped) === normalizedHref) {
        trimStart += 1;
        trimEnd += 1;
        trimmedText = unwrapped;
        continue;
      }
    }
    break;
  }

  while (trimmedText.length > 0) {
    const last = trimmedText[trimmedText.length - 1];
    if (last !== '"' && last !== "'" && last !== '`') {
      break;
    }
    if (normalizeSourceHref(trimmedText.slice(0, -1)) === normalizeSourceHref(trimmedText)) {
      trimEnd += 1;
      trimmedText = trimmedText.slice(0, -1);
      continue;
    }
    break;
  }

  return {
    from: rawFrom + Math.min(trimStart, rawLength),
    to: rawTo - Math.min(trimEnd, rawLength),
  };
}
