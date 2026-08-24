import { describe, expect, test, vi } from 'vite-plus/test';
import { formatRelativeTime, formatRelativeTimeLabel, getRelativeTimeColor } from './relative-time';

describe('formatRelativeTime', () => {
  test('should use the compact just-now threshold', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-04-06T12:00:00.000Z'));

    expect(formatRelativeTime('2026-04-06T11:59:57.000Z')).toEqual({
      suffix: null,
      value: 'just now',
    });
  });

  test('should allow callers to skip just-now and start at zero seconds', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-04-06T12:00:00.000Z'));

    expect(
      formatRelativeTime('2026-04-06T12:00:00.000Z', {
        allowJustNow: false,
      })
    ).toEqual({
      suffix: 'ago',
      value: '0s',
    });
  });

  test('should allow callers to provide the render tick clock', () => {
    expect(
      formatRelativeTime('2026-04-06T12:00:00.000Z', {
        allowJustNow: false,
        nowMs: new Date('2026-04-06T12:00:00.000Z').getTime(),
      })
    ).toEqual({
      suffix: 'ago',
      value: '0s',
    });
    expect(
      formatRelativeTime('2026-04-06T12:00:00.000Z', {
        allowJustNow: false,
        nowMs: new Date('2026-04-06T12:01:01.000Z').getTime(),
      })
    ).toEqual({
      suffix: 'ago',
      value: '1m',
    });
  });

  test('should format seconds, minutes, hours, and days with compact suffixes', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-04-06T12:00:00.000Z'));

    expect(formatRelativeTimeLabel('2026-04-06T11:59:40.000Z')).toBe('20s ago');
    expect(formatRelativeTimeLabel('2026-04-06T11:55:00.000Z')).toBe('5m ago');
    expect(formatRelativeTimeLabel('2026-04-06T10:00:00.000Z')).toBe('2h ago');
    expect(formatRelativeTimeLabel('2026-04-04T12:00:00.000Z')).toBe('2d ago');
  });

  test('should keep single-digit units unpadded', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-04-06T12:00:00.000Z'));

    expect(formatRelativeTimeLabel('2026-04-06T11:59:51.000Z')).toBe('9s ago');
  });

  test('should clamp future timestamps to just now', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-04-06T12:00:00.000Z'));

    expect(formatRelativeTimeLabel('2026-04-06T12:00:05.000Z')).toBe('just now');
  });

  test('should clamp future timestamps to zero seconds when just-now is disabled', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-04-06T12:00:00.000Z'));

    expect(
      formatRelativeTimeLabel('2026-04-06T12:00:05.000Z', {
        allowJustNow: false,
      })
    ).toBe('0s ago');
  });
});

describe('getRelativeTimeColor', () => {
  test('should keep the first fifteen minutes at the brightest green', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-04-06T12:00:00.000Z'));

    expect(getRelativeTimeColor('2026-04-06T11:45:00.000Z')).toBe(
      'color-mix(in srgb, #63e58f 86%, var(--app-foreground) 14%)'
    );
  });

  test('should use a slightly faded green between fifteen and thirty minutes', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-04-06T12:00:00.000Z'));

    expect(getRelativeTimeColor('2026-04-06T11:44:59.000Z')).toBe(
      'color-mix(in srgb, #63e58f 60%, var(--app-foreground) 40%)'
    );
  });

  test('should use a more muted green between thirty minutes and one hour', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-04-06T12:00:00.000Z'));

    expect(getRelativeTimeColor('2026-04-06T11:29:59.000Z')).toBe(
      'color-mix(in srgb, #63e58f 36%, var(--app-muted) 64%)'
    );
  });

  test('should switch to gray after one hour', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-04-06T12:00:00.000Z'));

    expect(getRelativeTimeColor('2026-04-06T10:59:59.000Z')).toBe('var(--app-muted)');
  });

  test('should keep future timestamps in the freshest color band', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-04-06T12:00:00.000Z'));

    expect(getRelativeTimeColor('2026-04-06T12:00:05.000Z')).toBe(
      'color-mix(in srgb, #63e58f 86%, var(--app-foreground) 14%)'
    );
  });
});
