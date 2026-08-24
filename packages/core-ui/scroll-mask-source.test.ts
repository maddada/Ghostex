import { readFileSync } from 'node:fs';
import { describe, expect, test } from 'vitest';

const scrollMaskSource = readFileSync(new URL('./styles/scroll-mask.css', import.meta.url), 'utf8');

function sourceBetween(start: string, end: string): string {
  const startIndex = scrollMaskSource.indexOf(start);
  const endIndex = scrollMaskSource.indexOf(end, startIndex + start.length);
  expect(startIndex).toBeGreaterThanOrEqual(0);
  expect(endIndex).toBeGreaterThan(startIndex);
  return scrollMaskSource.slice(startIndex, endIndex);
}

describe('scroll mask source', () => {
  test('keeps scroll-linked animations off Base UI popup lifecycle elements', () => {
    /*
     * CDXC:ProjectBoardDropdowns 2026-06-20-04:38:
     * Kanban dropdown popups must not keep a scroll-linked fade animation on the same element Base UI watches for close animation completion, or selecting an option can leave the popup mounted and visible.
     */
    const scrollTimelineBlock = sourceBetween('@supports (animation-timeline: scroll()) {', '@utility scroll-mask {');

    expect(scrollTimelineBlock).toContain('.horizontal-scroll-fade-mask:not([data-open]):not([data-closed])');
    expect(scrollTimelineBlock).toContain('.vertical-scroll-fade-mask:not([data-open]):not([data-closed])');
    expect(scrollTimelineBlock).toContain('.vertical-scroll-fade-mask-top:not([data-open]):not([data-closed])');
    expect(scrollTimelineBlock).toContain('.vertical-scroll-fade-mask-bottom:not([data-open]):not([data-closed])');
  });
});
