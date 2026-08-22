import { describe, expect, test } from "vitest";
import {
  formatSidebarV2SnoozeWakeDescription,
  formatSidebarV2SnoozeWakeLabel,
  resolveSidebarV2SnoozePresets,
  SIDEBAR_V2_SNOOZE_EVENING_HOUR,
  SIDEBAR_V2_SNOOZE_MORNING_HOUR,
  type SidebarV2SnoozePreset,
} from "./sidebar-v2-snooze";

const HOUR_MS = 60 * 60 * 1_000;
const DAY_MS = 24 * HOUR_MS;

/** Local-time helper so the assertions hold in any host time zone. */
function localTime(year: number, month: number, day: number, hour: number, minute = 0): number {
  return new Date(year, month - 1, day, hour, minute, 0, 0).getTime();
}

function byId(presets: readonly SidebarV2SnoozePreset[], id: SidebarV2SnoozePreset["id"]) {
  return presets.find((preset) => preset.id === id);
}

describe("resolveSidebarV2SnoozePresets", () => {
  test("morning offers all four presets", () => {
    const presets = resolveSidebarV2SnoozePresets(localTime(2026, 7, 29, 9, 30));
    expect(presets.map((preset) => preset.id)).toEqual([
      "hour",
      "evening",
      "tomorrow",
      "next-week",
    ]);
    expect(presets.map((preset) => preset.label)).toEqual([
      "In 1 hour",
      "This evening",
      "Tomorrow",
      "Next week",
    ]);
  });

  test("'In 1 hour' is exactly one hour out", () => {
    const nowMs = localTime(2026, 7, 29, 9, 30);
    expect(byId(resolveSidebarV2SnoozePresets(nowMs), "hour")?.snoozedUntilMs).toBe(
      nowMs + HOUR_MS,
    );
  });

  test("'This evening' lands on the local evening hour today", () => {
    const nowMs = localTime(2026, 7, 29, 9, 30);
    const evening = byId(resolveSidebarV2SnoozePresets(nowMs), "evening");
    const eveningDate = new Date(evening?.snoozedUntilMs ?? 0);
    expect(eveningDate.getHours()).toBe(SIDEBAR_V2_SNOOZE_EVENING_HOUR);
    expect(eveningDate.getMinutes()).toBe(0);
    expect(eveningDate.getDate()).toBe(new Date(nowMs).getDate());
  });

  test("the evening preset disappears once it is within an hour", () => {
    expect(
      byId(resolveSidebarV2SnoozePresets(localTime(2026, 7, 29, 17, 30)), "evening"),
    ).toBeUndefined();
    expect(
      byId(resolveSidebarV2SnoozePresets(localTime(2026, 7, 29, 22, 0)), "evening"),
    ).toBeUndefined();
    expect(
      byId(resolveSidebarV2SnoozePresets(localTime(2026, 7, 29, 16, 30)), "evening"),
    ).toBeDefined();
  });

  test("'Tomorrow' is the next calendar day at the morning hour", () => {
    const tomorrow = byId(
      resolveSidebarV2SnoozePresets(localTime(2026, 7, 29, 22, 30)),
      "tomorrow",
    );
    const tomorrowDate = new Date(tomorrow?.snoozedUntilMs ?? 0);
    expect(tomorrowDate.getFullYear()).toBe(2026);
    expect(tomorrowDate.getMonth()).toBe(6);
    expect(tomorrowDate.getDate()).toBe(30);
    expect(tomorrowDate.getHours()).toBe(SIDEBAR_V2_SNOOZE_MORNING_HOUR);
  });

  test("'Tomorrow' rolls the month over", () => {
    const tomorrow = byId(
      resolveSidebarV2SnoozePresets(localTime(2026, 7, 31, 23, 30)),
      "tomorrow",
    );
    const tomorrowDate = new Date(tomorrow?.snoozedUntilMs ?? 0);
    expect(tomorrowDate.getMonth()).toBe(7);
    expect(tomorrowDate.getDate()).toBe(1);
  });

  /*
  A fixed 24h offset would skip a whole day on a spring-forward date, because
  that local day is only 23 hours long. Asserting on the LOCAL calendar day
  proves the preset advances by calendar days regardless of the host zone.
  */
  test("day advances are DST-safe: late-night snoozes never skip a day", () => {
    for (const startHour of [0, 12, 23]) {
      for (const day of [7, 8, 9, 10, 11]) {
        const nowMs = localTime(2026, 3, day, startHour, 30);
        const tomorrow = byId(resolveSidebarV2SnoozePresets(nowMs), "tomorrow");
        const tomorrowDate = new Date(tomorrow?.snoozedUntilMs ?? 0);
        const expectedDate = new Date(nowMs);
        expectedDate.setDate(expectedDate.getDate() + 1);
        expect(tomorrowDate.getDate()).toBe(expectedDate.getDate());
        expect(tomorrowDate.getHours()).toBe(SIDEBAR_V2_SNOOZE_MORNING_HOUR);
      }
    }
  });

  test("'Next week' is the coming Monday morning", () => {
    // 2026-07-29 is a Wednesday.
    const nextWeek = byId(resolveSidebarV2SnoozePresets(localTime(2026, 7, 29, 9, 0)), "next-week");
    const nextWeekDate = new Date(nextWeek?.snoozedUntilMs ?? 0);
    expect(nextWeekDate.getDay()).toBe(1);
    expect(nextWeekDate.getDate()).toBe(3);
    expect(nextWeekDate.getMonth()).toBe(7);
    expect(nextWeekDate.getHours()).toBe(SIDEBAR_V2_SNOOZE_MORNING_HOUR);
  });

  test("on a Monday, 'Next week' is a full week out, never today", () => {
    const mondayMs = localTime(2026, 8, 3, 9, 0);
    const nextWeek = byId(resolveSidebarV2SnoozePresets(mondayMs), "next-week");
    const nextWeekDate = new Date(nextWeek?.snoozedUntilMs ?? 0);
    expect(nextWeekDate.getDay()).toBe(1);
    expect(nextWeekDate.getDate()).toBe(10);
    expect(nextWeek!.snoozedUntilMs).toBeGreaterThan(mondayMs);
  });

  test("every preset ships a usable ISO wake time and a non-empty when label", () => {
    for (const preset of resolveSidebarV2SnoozePresets(localTime(2026, 7, 29, 9, 30))) {
      expect(Date.parse(preset.snoozedUntil)).toBe(preset.snoozedUntilMs);
      expect(preset.whenLabel.trim().length).toBeGreaterThan(0);
    }
  });

  test("presets are strictly increasing in wake time", () => {
    const presets = resolveSidebarV2SnoozePresets(localTime(2026, 7, 29, 9, 30));
    for (let index = 1; index < presets.length; index += 1) {
      expect(presets[index]!.snoozedUntilMs).toBeGreaterThan(presets[index - 1]!.snoozedUntilMs);
    }
  });
});

describe("formatSidebarV2SnoozeWakeLabel", () => {
  const nowMs = Date.parse("2026-07-29T12:00:00.000Z");

  test("rounds minutes up so a hidden row never reads 0m", () => {
    expect(formatSidebarV2SnoozeWakeLabel(nowMs + 1_000, nowMs)).toBe("1m");
    expect(formatSidebarV2SnoozeWakeLabel(nowMs + 90_000, nowMs)).toBe("2m");
  });

  test("switches to hours then days", () => {
    expect(formatSidebarV2SnoozeWakeLabel(nowMs + 2 * HOUR_MS, nowMs)).toBe("2h");
    expect(formatSidebarV2SnoozeWakeLabel(nowMs + 18 * HOUR_MS, nowMs)).toBe("18h");
    expect(formatSidebarV2SnoozeWakeLabel(nowMs + 3 * DAY_MS, nowMs)).toBe("3d");
  });

  test("an elapsed or unusable wake time reads 'now'", () => {
    expect(formatSidebarV2SnoozeWakeLabel(nowMs, nowMs)).toBe("now");
    expect(formatSidebarV2SnoozeWakeLabel(nowMs - 1, nowMs)).toBe("now");
    expect(formatSidebarV2SnoozeWakeLabel(Number.NaN, nowMs)).toBe("now");
  });
});

describe("formatSidebarV2SnoozeWakeDescription", () => {
  test("distinguishes today, tomorrow, this week, and later", () => {
    const nowMs = localTime(2026, 7, 29, 9, 0);
    const options = { locale: "en-US" };
    expect(
      formatSidebarV2SnoozeWakeDescription(localTime(2026, 7, 29, 17, 30), nowMs, options),
    ).not.toMatch(/tomorrow|,/);
    expect(
      formatSidebarV2SnoozeWakeDescription(localTime(2026, 7, 30, 9, 0), nowMs, options),
    ).toMatch(/^tomorrow /);
    expect(
      formatSidebarV2SnoozeWakeDescription(localTime(2026, 8, 3, 9, 0), nowMs, options),
    ).toMatch(/^Mon /);
    expect(
      formatSidebarV2SnoozeWakeDescription(localTime(2026, 8, 20, 9, 0), nowMs, options),
    ).toMatch(/, /);
  });

  test("an unusable wake time renders nothing", () => {
    expect(formatSidebarV2SnoozeWakeDescription(Number.NaN, Date.now())).toBe("");
  });
});
