/*
CDXC:SidebarV2 2026-07-29-00:00:
Pure snooze helpers for the sidebar inbox.

Snooze preset resolution for the V2 snooze popover. Pure functions so the preset
math (evening / tomorrow / next-week boundaries) is unit-testable without a DOM.
Presets deliberately skew short: agent-session rhythms are hours (a CI run, a
review, the next work block), not days.

DST safety: every day advance goes through `Date.setDate`, never a fixed
millisecond offset. A spring-forward day is 23 hours long, so `23:30 + 24h`
skips the whole next day and "Tomorrow 9am" would land on the day after.
*/

export type SidebarV2SnoozePresetId = 'evening' | 'hour' | 'next-week' | 'tomorrow';

export type SidebarV2SnoozePreset = {
  id: SidebarV2SnoozePresetId;
  label: string;
  /** ISO wake time, ready for the gxserver snooze RPC. */
  snoozedUntil: string;
  snoozedUntilMs: number;
  /** Menu-row time column. Complements the label instead of repeating it:
      "Tomorrow" pairs with "9:00 AM", not "tomorrow 9:00 AM". */
  whenLabel: string;
};

export type SidebarV2SnoozeFormatOptions = {
  /** Locale for the human-facing time columns. Defaults to the host locale. */
  locale?: string;
};

export const SIDEBAR_V2_SNOOZE_EVENING_HOUR = 18;
export const SIDEBAR_V2_SNOOZE_MORNING_HOUR = 9;

const MINUTE_MS = 60 * 1_000;
const HOUR_MS = 60 * MINUTE_MS;
const DAY_MS = 24 * HOUR_MS;

function timeOfDayLabel(date: Date, options: SidebarV2SnoozeFormatOptions): string {
  return date.toLocaleTimeString(options.locale, { hour: 'numeric', minute: '2-digit' });
}

function atLocalHour(base: Date, hour: number): Date {
  const next = new Date(base);
  next.setHours(hour, 0, 0, 0);
  return next;
}

/** Calendar-day advance. See the DST note at the top of this file. */
function addLocalDays(base: Date, days: number): Date {
  const next = new Date(base);
  next.setDate(next.getDate() + days);
  return next;
}

function toPreset(id: SidebarV2SnoozePresetId, label: string, whenLabel: string, wakeAt: Date): SidebarV2SnoozePreset {
  return {
    id,
    label,
    snoozedUntil: wakeAt.toISOString(),
    snoozedUntilMs: wakeAt.getTime(),
    whenLabel,
  };
}

/**
 * Presets for "snooze until", computed against local time. "This evening" only
 * appears while it is still meaningfully before evening; after that the list
 * starts at "Tomorrow".
 */
export function resolveSidebarV2SnoozePresets(
  nowMs: number,
  options: SidebarV2SnoozeFormatOptions = {}
): SidebarV2SnoozePreset[] {
  const now = new Date(nowMs);
  const presets: SidebarV2SnoozePreset[] = [];

  const inAnHour = new Date(nowMs + HOUR_MS);
  presets.push(toPreset('hour', 'In 1 hour', timeOfDayLabel(inAnHour, options), inAnHour));

  const evening = atLocalHour(now, SIDEBAR_V2_SNOOZE_EVENING_HOUR);
  // Suppress the evening preset once it is within an hour (or past): it would
  // duplicate "In 1 hour" or point at the past.
  if (evening.getTime() - nowMs > HOUR_MS) {
    presets.push(toPreset('evening', 'This evening', timeOfDayLabel(evening, options), evening));
  }

  const tomorrow = atLocalHour(addLocalDays(now, 1), SIDEBAR_V2_SNOOZE_MORNING_HOUR);
  presets.push(toPreset('tomorrow', 'Tomorrow', timeOfDayLabel(tomorrow, options), tomorrow));

  // Next Monday 9:00 (a full week out when today is Monday).
  const daysUntilMonday = (1 - now.getDay() + 7) % 7 || 7;
  const nextWeek = atLocalHour(addLocalDays(now, daysUntilMonday), SIDEBAR_V2_SNOOZE_MORNING_HOUR);
  presets.push(
    toPreset(
      'next-week',
      'Next week',
      `${nextWeek.toLocaleDateString(options.locale, { weekday: 'short' })} ${timeOfDayLabel(nextWeek, options)}`,
      nextWeek
    )
  );

  return presets;
}

/**
 * Compact "wakes in" label for snoozed rows: "2h", "18h", "3d". Minutes round
 * up so a snooze never reads "0m" while the row is still hidden.
 */
export function formatSidebarV2SnoozeWakeLabel(snoozedUntilMs: number, nowMs: number): string {
  if (!Number.isFinite(snoozedUntilMs)) {
    return 'now';
  }
  const remainingMs = snoozedUntilMs - nowMs;
  if (remainingMs <= 0) {
    return 'now';
  }
  if (remainingMs < HOUR_MS) {
    return `${Math.max(1, Math.ceil(remainingMs / MINUTE_MS))}m`;
  }
  if (remainingMs < DAY_MS) {
    return `${Math.ceil(remainingMs / HOUR_MS)}h`;
  }
  return `${Math.ceil(remainingMs / DAY_MS)}d`;
}

/**
 * Human wake time for menus and toasts: "17:30" (today), "tomorrow 9:00",
 * "Mon 9:00", "Aug 12, 9:00".
 */
export function formatSidebarV2SnoozeWakeDescription(
  snoozedUntilMs: number,
  nowMs: number,
  options: SidebarV2SnoozeFormatOptions = {}
): string {
  if (!Number.isFinite(snoozedUntilMs)) {
    return '';
  }
  const wake = new Date(snoozedUntilMs);
  const time = timeOfDayLabel(wake, options);
  const startOfToday = new Date(nowMs);
  startOfToday.setHours(0, 0, 0, 0);
  // Day delta is measured between local midnights, so a DST transition inside
  // the window cannot shift the label by a day.
  const startOfWakeDay = new Date(snoozedUntilMs);
  startOfWakeDay.setHours(0, 0, 0, 0);
  const dayDelta = Math.round((startOfWakeDay.getTime() - startOfToday.getTime()) / DAY_MS);
  if (dayDelta === 0) {
    return time;
  }
  if (dayDelta === 1) {
    return `tomorrow ${time}`;
  }
  if (dayDelta > 1 && dayDelta < 7) {
    return `${wake.toLocaleDateString(options.locale, { weekday: 'short' })} ${time}`;
  }
  return `${wake.toLocaleDateString(options.locale, { day: 'numeric', month: 'short' })}, ${time}`;
}
