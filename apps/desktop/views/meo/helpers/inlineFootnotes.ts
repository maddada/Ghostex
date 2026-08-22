// @ts-nocheck
export interface InlineFootnoteMarkerRange {
  label: string;
  fromOffset: number;
  toOffset: number;
}

const inlineFootnoteSequencePattern = /^(?:\[\^[^\]\r\n]+\])+$/;
const inlineFootnoteMarkerPattern = /\[\^([^\]\r\n]+)\]/g;

export function collectInlineFootnoteMarkerRanges(rawText: string): InlineFootnoteMarkerRange[] {
  if (!rawText || !inlineFootnoteSequencePattern.test(rawText)) {
    return [];
  }

  const ranges: InlineFootnoteMarkerRange[] = [];
  inlineFootnoteMarkerPattern.lastIndex = 0;
  for (
    let match = inlineFootnoteMarkerPattern.exec(rawText);
    match;
    match = inlineFootnoteMarkerPattern.exec(rawText)
  ) {
    ranges.push({
      label: match[1],
      fromOffset: match.index,
      toOffset: match.index + match[0].length
    });
  }

  return ranges;
}

