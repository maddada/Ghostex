// @ts-nocheck
export interface KbdTagRange {
  from: number;
  to: number;
  content: string;
}

export interface ParsedKbdTag {
  content: string;
  nextIndex: number;
}

const openingKbdTagAtRe = /^<kbd\b[^>]*>/i;
const closingKbdTagRe = /<\/kbd\s*>/i;

export function hasKbdTagMarker(text: string): boolean {
  return /<\s*\/?\s*kbd\b/i.test(text);
}

export function parseKbdTagAt(text: string, index: number): ParsedKbdTag | null {
  if (!text || index < 0 || index >= text.length || text[index] !== '<') {
    return null;
  }

  const openingMatch = openingKbdTagAtRe.exec(text.slice(index));
  if (!openingMatch) {
    return null;
  }

  const contentStart = index + openingMatch[0].length;
  const closeMatch = closingKbdTagRe.exec(text.slice(contentStart));
  if (!closeMatch) {
    return null;
  }

  const contentEnd = contentStart + closeMatch.index;
  return {
    content: text.slice(contentStart, contentEnd),
    nextIndex: contentEnd + closeMatch[0].length,
  };
}

export function collectKbdTagRangesFromText(text: string, lineFrom: number): KbdTagRange[] {
  const ranges: KbdTagRange[] = [];
  if (!text || !hasKbdTagMarker(text)) {
    return ranges;
  }

  for (let index = text.indexOf('<'); index >= 0 && index < text.length;) {
    const parsed = parseKbdTagAt(text, index);
    if (!parsed) {
      index = text.indexOf('<', index + 1);
      continue;
    }

    ranges.push({
      from: lineFrom + index,
      to: lineFrom + parsed.nextIndex,
      content: parsed.content,
    });
    index = text.indexOf('<', parsed.nextIndex);
  }

  return ranges;
}
