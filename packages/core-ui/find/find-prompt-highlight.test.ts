import { describe, expect, it } from 'vitest';
import { flattenPromptLineWithOffsets, splitHighlightedSegments } from './find-prompt-highlight';

describe('find prompt highlighting', () => {
  it('returns one plain run when nothing matched', () => {
    expect(splitHighlightedSegments('hello', [])).toEqual([{ highlighted: false, text: 'hello' }]);
  });

  it('marks the matched characters and merges adjacent runs', () => {
    // "border" inside "a border here", matched at bytes 2..7
    expect(splitHighlightedSegments('a border here', [2, 3, 4, 5, 6, 7])).toEqual([
      { highlighted: false, text: 'a ' },
      { highlighted: true, text: 'border' },
      { highlighted: false, text: ' here' },
    ]);
  });

  it('uses byte offsets, so non-ASCII text highlights the right characters', () => {
    // "é" is two UTF-8 bytes, so "b" starts at byte 3, not index 2.
    const text = 'aébc';
    expect(splitHighlightedSegments(text, [3])).toEqual([
      { highlighted: false, text: 'aé' },
      { highlighted: true, text: 'b' },
      { highlighted: false, text: 'c' },
    ]);
  });

  it('handles astral characters without splitting a surrogate pair', () => {
    const text = 'x🙂y';
    // 🙂 occupies bytes 1..4, so "y" starts at byte 5.
    expect(splitHighlightedSegments(text, [5])).toEqual([
      { highlighted: false, text: 'x🙂' },
      { highlighted: true, text: 'y' },
    ]);
  });

  it('flattens newlines for row rendering and moves the offsets with them', () => {
    const { offsets, text } = flattenPromptLineWithOffsets('ab\n\ncd', [0, 4, 5]);
    expect(text).toBe('ab cd');
    // byte 4 was the "c" after the collapsed newline run, now byte 3.
    expect(offsets).toEqual([0, 3, 4]);
    expect(splitHighlightedSegments(text, offsets)).toEqual([
      { highlighted: true, text: 'a' },
      { highlighted: false, text: 'b ' },
      { highlighted: true, text: 'cd' },
    ]);
  });
});
