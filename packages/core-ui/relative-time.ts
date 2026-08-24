/**
 * Format a relative time string from an ISO date.
 * Returns `{ value: "20s", suffix: "ago" }` or `{ value: "just now", suffix: null }`.
 *
 * This intentionally uses a compact sidebar timestamp.
 */
const FIFTEEN_MINUTES_MS = 15 * 60 * 1000;
const THIRTY_MINUTES_MS = 30 * 60 * 1000;
const ONE_HOUR_MS = 60 * 60 * 1000;
const BRIGHT_GREEN_WEIGHT = 86;
const MID_GREEN_WEIGHT = 60;
const FADED_GREEN_WEIGHT = 36;

function getRelativeTimeDiffMs(isoDate: string, nowMs = Date.now()): number {
  return Math.max(0, nowMs - new Date(isoDate).getTime());
}

function formatCompactRelativeUnit(value: number, unit: string): string {
  return `${value}${unit}`;
}

function buildForegroundMixedGreen(greenWeight: number): string {
  return `color-mix(in srgb, #63e58f ${greenWeight}%, var(--app-foreground) ${100 - greenWeight}%)`;
}

function buildMutedMixedGreen(greenWeight: number): string {
  return `color-mix(in srgb, #63e58f ${greenWeight}%, var(--app-muted) ${100 - greenWeight}%)`;
}

export type FormatRelativeTimeOptions = {
  allowJustNow?: boolean;
  nowMs?: number;
};

export function formatRelativeTime(
  isoDate: string,
  options: FormatRelativeTimeOptions = {}
): { value: string; suffix: string | null } {
  const diffMs = getRelativeTimeDiffMs(isoDate, options.nowMs);
  const seconds = Math.floor(diffMs / 1000);
  if (options.allowJustNow !== false && seconds < 5) {
    return { value: 'just now', suffix: null };
  }

  if (seconds < 60) {
    return { value: formatCompactRelativeUnit(seconds, 's'), suffix: 'ago' };
  }

  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) {
    return { value: formatCompactRelativeUnit(minutes, 'm'), suffix: 'ago' };
  }

  const hours = Math.floor(minutes / 60);
  if (hours < 24) {
    return { value: formatCompactRelativeUnit(hours, 'h'), suffix: 'ago' };
  }

  const days = Math.floor(hours / 24);
  return { value: formatCompactRelativeUnit(days, 'd'), suffix: 'ago' };
}

export function formatRelativeTimeLabel(isoDate: string, options?: FormatRelativeTimeOptions): string {
  const relative = formatRelativeTime(isoDate, options);
  return relative.suffix ? `${relative.value} ${relative.suffix}` : relative.value;
}

export function getRelativeTimeColor(isoDate: string): string {
  const diffMs = getRelativeTimeDiffMs(isoDate);
  if (diffMs <= FIFTEEN_MINUTES_MS) {
    return buildForegroundMixedGreen(BRIGHT_GREEN_WEIGHT);
  }

  if (diffMs <= THIRTY_MINUTES_MS) {
    return buildForegroundMixedGreen(MID_GREEN_WEIGHT);
  }

  if (diffMs <= ONE_HOUR_MS) {
    return buildMutedMixedGreen(FADED_GREEN_WEIGHT);
  }

  return 'var(--app-muted)';
}
