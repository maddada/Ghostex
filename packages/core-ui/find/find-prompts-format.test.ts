import { describe, expect, it } from 'vitest';
import {
  formatDayHeader,
  formatLastActiveCompact,
  formatLastActiveFull,
  formatPromptMetaLine,
  findPromptDayKey,
} from './find-prompts-format';

const DAY = 86_400;
const NOW = 20_000 * DAY;

describe('find prompt formatting', () => {
  it("matches the terminal picker's compact last-active wording", () => {
    expect(formatLastActiveCompact(0, NOW)).toBe('unknown');
    expect(formatLastActiveCompact(NOW - 5, NOW)).toBe('now');
    expect(formatLastActiveCompact(NOW - 120, NOW)).toBe('2m ago');
    expect(formatLastActiveCompact(NOW - 7_200, NOW)).toBe('2h ago');
    expect(formatLastActiveCompact(NOW - 2 * DAY, NOW)).toBe('2d ago');
    expect(formatLastActiveCompact(NOW - 30 * DAY, NOW)).toMatch(/^[A-Z][a-z]{2} \d+$/u);
  });

  it("matches the terminal picker's day headers", () => {
    const today = findPromptDayKey(NOW);
    expect(formatDayHeader(today, NOW)).toBe('Today');
    expect(formatDayHeader(today - 1, NOW)).toBe('Yesterday');
    expect(formatDayHeader(today - 3, NOW)).toBe('3 days ago');
    expect(formatDayHeader(Number.NEGATIVE_INFINITY, NOW)).toBe('Unknown day');
  });

  it('renders the full last-active line in UTC', () => {
    expect(formatLastActiveFull(0)).toBe('last active unknown');
    expect(formatLastActiveFull(1_755_000_000)).toMatch(/^last active [A-Z][a-z]{2} \d+ \d{2}:\d{2} UTC$/u);
  });

  it('builds the usage tail only from present fields', () => {
    expect(
      formatPromptMetaLine({
        model: '',
        plan: '',
        provider: '',
        thinking: '',
        usage: {
          cacheRead: 0,
          cacheWrite: 0,
          contextWindow: 0,
          cost: 0,
          input: 0,
          output: 0,
          ratePercent: 0,
        },
      })
    ).toBe('');

    expect(
      formatPromptMetaLine({
        model: 'opus',
        plan: 'pro',
        provider: 'anthropic',
        thinking: 'high',
        usage: {
          cacheRead: 3,
          cacheWrite: 0,
          contextWindow: 200000,
          cost: 1.5,
          input: 10,
          output: 20,
          ratePercent: 12.5,
        },
      })
    ).toBe('↑10 ↓20 R3 $1.500 (pro) 12.5%/200000 (anthropic) opus • high');
  });
});
