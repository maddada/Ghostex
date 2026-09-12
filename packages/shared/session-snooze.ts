/**
 * CDXC:Sessions 2026-09-12 DECISION:
 * User: Snooze offers 1 hour, 3 hours, Tomorrow and Next week. A snoozed session moves to a Snoozed section below Parked and is always put to sleep there; when the wake time passes it returns to its ordinary section.
 * Snooze reuses gxserver's server-owned `snoozedUntil` lifecycle (`/api/snoozeSession`), so every client and the CLI see the same wake time and the daemon's sweep publishes the return.
 * "Tomorrow" and "Next week" wake at 9:00 local time (next day, next Monday) rather than a fixed offset, so a snooze set at night does not wake in the middle of the following night.
 * SEE-ALSO: server/src/session_lifecycle.rs, packages/core-ui/sidebar-app/project-session-section-state.ts, packages/shared/active-sessions-sort.ts.
 */
export const SESSION_SNOOZE_PRESETS = ['oneHour', 'threeHours', 'tomorrow', 'nextWeek'] as const;

export type SessionSnoozePreset = (typeof SESSION_SNOOZE_PRESETS)[number];

export const SESSION_SNOOZE_PRESET_LABELS: Record<SessionSnoozePreset, string> = {
  nextWeek: 'Next week',
  oneHour: '1 hour',
  threeHours: '3 hours',
  tomorrow: 'Tomorrow',
};

const HOUR_MS = 60 * 60 * 1_000;
const MORNING_WAKE_HOUR = 9;

function atMorning(date: Date): Date {
  const morning = new Date(date);
  morning.setHours(MORNING_WAKE_HOUR, 0, 0, 0);
  return morning;
}

/** Resolves a preset to the absolute wake time, as a `Date`, relative to `now`. */
export function resolveSessionSnoozeWakeTime(preset: SessionSnoozePreset, now: Date = new Date()): Date {
  switch (preset) {
    case 'oneHour':
      return new Date(now.getTime() + HOUR_MS);
    case 'threeHours':
      return new Date(now.getTime() + 3 * HOUR_MS);
    case 'tomorrow': {
      const tomorrow = new Date(now);
      tomorrow.setDate(tomorrow.getDate() + 1);
      return atMorning(tomorrow);
    }
    case 'nextWeek': {
      const nextMonday = new Date(now);
      const daysUntilMonday = (8 - nextMonday.getDay()) % 7 || 7;
      nextMonday.setDate(nextMonday.getDate() + daysUntilMonday);
      return atMorning(nextMonday);
    }
  }
}

/**
 * A session counts as snoozed while its wake time is still ahead of the clock. gxserver keeps
 * `snoozedUntil` on the row until its sweep clears it, so a value in the past never hides a session.
 */
export function isSidebarSessionSnoozed(
  session: { snoozedUntil?: string } | undefined,
  nowMs: number = Date.now()
): boolean {
  const wakeAtMs = session?.snoozedUntil ? Date.parse(session.snoozedUntil) : Number.NaN;
  return Number.isFinite(wakeAtMs) && wakeAtMs > nowMs;
}

export function formatSessionSnoozeWakeLabel(snoozedUntil: string, now: Date = new Date()): string {
  const wakeAt = new Date(snoozedUntil);
  if (!Number.isFinite(wakeAt.getTime())) {
    return '';
  }
  const timeLabel = wakeAt.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' });
  const sameDay = wakeAt.toDateString() === now.toDateString();
  if (sameDay) {
    return `Snoozed until ${timeLabel}`;
  }
  const dayLabel = wakeAt.toLocaleDateString(undefined, { month: 'short', weekday: 'short', day: 'numeric' });
  return `Snoozed until ${dayLabel}, ${timeLabel}`;
}
