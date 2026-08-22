/*
CDXC:AgentHistorySearch 2026-08-20:
The matcher reports highlight positions as BYTE offsets into UTF-8 prompt text,
because it matches bytes. JavaScript strings are indexed by UTF-16 code units,
so a prompt containing an emoji or any non-ASCII character would highlight the
wrong letters if the offsets were used directly. This maps one to the other.
*/

export interface FindPromptSegment {
  highlighted: boolean;
  text: string;
}

/** UTF-8 byte length of a single code point. */
function utf8Length(codePoint: number): number {
  if (codePoint < 0x80) return 1;
  if (codePoint < 0x800) return 2;
  if (codePoint < 0x10000) return 3;
  return 4;
}

/**
 * Split `text` into alternating plain and highlighted runs.
 *
 * A character is highlighted when its first byte is one of `byteOffsets`, which
 * is the same rule the terminal picker paints with.
 */
export function splitHighlightedSegments(
  text: string,
  byteOffsets: readonly number[],
): FindPromptSegment[] {
  if (!text) {
    return [];
  }
  const marks = new Set(byteOffsets);
  if (marks.size === 0) {
    return [{ highlighted: false, text }];
  }

  const segments: FindPromptSegment[] = [];
  let byteOffset = 0;
  let runHighlighted = false;
  let run = "";

  for (const character of text) {
    const highlighted = marks.has(byteOffset);
    if (run && highlighted !== runHighlighted) {
      segments.push({ highlighted: runHighlighted, text: run });
      run = "";
    }
    runHighlighted = highlighted;
    run += character;
    byteOffset += utf8Length(character.codePointAt(0) ?? 0);
  }
  if (run) {
    segments.push({ highlighted: runHighlighted, text: run });
  }
  return segments;
}

/**
 * Collapse newlines and tabs to single spaces for one-line row rendering, the
 * same sanitizing the terminal picker does before drawing a result row.
 */
export function flattenPromptLine(text: string): string {
  return text.replace(/[\r\n\t]+/gu, " ");
}

/**
 * Byte offsets shift when the row text is flattened only if characters are
 * removed; `flattenPromptLine` replaces runs, so offsets are remapped here.
 */
export function flattenPromptLineWithOffsets(
  text: string,
  byteOffsets: readonly number[],
): { offsets: number[]; text: string } {
  const marks = new Set(byteOffsets);
  const nextOffsets: number[] = [];
  let byteOffset = 0;
  let nextByteOffset = 0;
  let out = "";
  let pendingWhitespace = false;

  for (const character of text) {
    const size = utf8Length(character.codePointAt(0) ?? 0);
    const isCollapsible = character === "\n" || character === "\r" || character === "\t";
    if (isCollapsible) {
      if (!pendingWhitespace) {
        out += " ";
        nextByteOffset += 1;
        pendingWhitespace = true;
      }
      byteOffset += size;
      continue;
    }
    pendingWhitespace = false;
    if (marks.has(byteOffset)) {
      nextOffsets.push(nextByteOffset);
    }
    out += character;
    byteOffset += size;
    nextByteOffset += size;
  }
  return { offsets: nextOffsets, text: out };
}
