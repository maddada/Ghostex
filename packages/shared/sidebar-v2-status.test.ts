import { describe, expect, test } from 'vitest';
import type { SidebarV2Session } from './sidebar-v2-session';
import {
  firstValidTimestamp,
  firstValidTimestampMs,
  formatSidebarV2RelativeTime,
  formatWorkingDurationLabel,
  hasUnseenCompletion,
  parseTimestampMs,
  resolveSidebarV2GroupStatus,
  resolveSidebarV2Status,
  resolveSidebarV2WorkingStartedAtMs,
  SIDEBAR_V2_DONE_WINDOW_MS,
  sidebarV2StatusPriority,
} from './sidebar-v2-status';

const NOW_MS = Date.parse('2026-07-29T12:00:00.000Z');

function session(overrides: Partial<SidebarV2Session> = {}): SidebarV2Session {
  return {
    activity: 'idle',
    sessionId: 'session-1',
    ...overrides,
  };
}

describe('timestamp helpers', () => {
  test('parseTimestampMs sinks missing and malformed stamps to the epoch', () => {
    expect(parseTimestampMs('2026-07-29T12:00:00.000Z')).toBe(NOW_MS);
    expect(parseTimestampMs('not-a-date')).toBe(0);
    expect(parseTimestampMs(undefined)).toBe(0);
    expect(parseTimestampMs(null)).toBe(0);
  });

  test('firstValidTimestampMs falls through malformed candidates, not just missing ones', () => {
    expect(firstValidTimestampMs(null, 'nope', '2026-07-29T12:00:00.000Z')).toBe(NOW_MS);
    expect(firstValidTimestampMs(undefined, null)).toBeNull();
    expect(firstValidTimestampMs('nope')).toBeNull();
  });

  test('firstValidTimestamp returns the ISO string twin', () => {
    expect(firstValidTimestamp('nope', '2026-07-29T12:00:00.000Z')).toBe('2026-07-29T12:00:00.000Z');
    expect(firstValidTimestamp(null, undefined)).toBeNull();
  });
});

describe('formatWorkingDurationLabel', () => {
  test('counts seconds, then minutes, then hours plus minutes', () => {
    expect(formatWorkingDurationLabel(0)).toBe('0s');
    expect(formatWorkingDurationLabel(59_000)).toBe('59s');
    expect(formatWorkingDurationLabel(60_000)).toBe('1m');
    expect(formatWorkingDurationLabel(59 * 60_000)).toBe('59m');
    expect(formatWorkingDurationLabel(60 * 60_000)).toBe('1h 0m');
    expect(formatWorkingDurationLabel(95 * 60_000)).toBe('1h 35m');
  });

  test('clamps negative and non-finite elapsed values', () => {
    expect(formatWorkingDurationLabel(-5_000)).toBe('0s');
    expect(formatWorkingDurationLabel(Number.NaN)).toBe('0s');
  });
});

describe('formatSidebarV2RelativeTime', () => {
  test('uses one unit per magnitude', () => {
    expect(formatSidebarV2RelativeTime(0)).toBe('now');
    expect(formatSidebarV2RelativeTime(59_000)).toBe('now');
    expect(formatSidebarV2RelativeTime(5 * 60_000)).toBe('5m');
    expect(formatSidebarV2RelativeTime(3 * 60 * 60_000)).toBe('3h');
    expect(formatSidebarV2RelativeTime(2 * 24 * 60 * 60_000)).toBe('2d');
    expect(formatSidebarV2RelativeTime(21 * 24 * 60 * 60_000)).toBe('3w');
  });
});

describe('resolveSidebarV2WorkingStartedAtMs', () => {
  test('prefers the working stint, then the meaningful-activity clock', () => {
    expect(
      resolveSidebarV2WorkingStartedAtMs({
        lastInteractionAt: '2026-07-29T10:00:00.000Z',
        workingStartedAt: '2026-07-29T11:00:00.000Z',
      })
    ).toBe(Date.parse('2026-07-29T11:00:00.000Z'));
    expect(
      resolveSidebarV2WorkingStartedAtMs({
        lastInteractionAt: '2026-07-29T10:00:00.000Z',
        workingStartedAt: 'garbage',
      })
    ).toBe(Date.parse('2026-07-29T10:00:00.000Z'));
    expect(resolveSidebarV2WorkingStartedAtMs({})).toBeNull();
  });
});

describe('resolveSidebarV2Status', () => {
  test('attention resolves to amber input by default', () => {
    expect(resolveSidebarV2Status(session({ activity: 'attention' }), { nowMs: NOW_MS })).toEqual({
      hue: 'amber',
      kind: 'input',
      label: 'Input',
      pulse: false,
      recede: false,
    });
  });

  test('attentionKind refines amber into the approval class', () => {
    expect(
      resolveSidebarV2Status(session({ activity: 'attention', attentionKind: 'approval' }), {
        nowMs: NOW_MS,
      })
    ).toMatchObject({ hue: 'amber', kind: 'approval', label: 'Approval' });
  });

  test('an explicit input attentionKind is the only way to reach the quieter indigo', () => {
    expect(
      resolveSidebarV2Status(session({ activity: 'attention', attentionKind: 'input' }), {
        nowMs: NOW_MS,
      })
    ).toMatchObject({ hue: 'indigo', kind: 'input', label: 'Input' });
  });

  test('a host activity label wins over the generic status label', () => {
    expect(
      resolveSidebarV2Status(session({ activity: 'attention', activityLabel: '  Permission needed  ' }), {
        nowMs: NOW_MS,
      })
    ).toMatchObject({ hue: 'amber', label: 'Permission needed' });
  });

  test('working is sky and carries the elapsed stint', () => {
    expect(
      resolveSidebarV2Status(
        session({
          activity: 'working',
          workingStartedAt: new Date(NOW_MS - 95 * 60_000).toISOString(),
        }),
        { nowMs: NOW_MS }
      )
    ).toEqual({
      hue: 'sky',
      kind: 'working',
      label: 'Working 1h 35m',
      pulse: true,
      recede: false,
    });
  });

  test('working without any start stamp still reads as working', () => {
    expect(resolveSidebarV2Status(session({ activity: 'working' }), { nowMs: NOW_MS })).toMatchObject({
      hue: 'sky',
      kind: 'working',
      label: 'Working',
    });
  });

  test('attention outranks working', () => {
    expect(
      resolveSidebarV2Status(session({ activity: 'attention', workingStartedAt: new Date(NOW_MS).toISOString() }), {
        nowMs: NOW_MS,
      })
    ).toMatchObject({ kind: 'input' });
  });

  test('an error lifecycle is the only red state', () => {
    expect(
      resolveSidebarV2Status(session({ activity: 'idle', lifecycleState: 'error' }), {
        nowMs: NOW_MS,
      })
    ).toMatchObject({ hue: 'red', kind: 'failed', label: 'Failed', recede: false });
  });

  test('a running lifecycle is pane liveness, never a working status', () => {
    expect(
      resolveSidebarV2Status(
        session({
          activity: 'idle',
          lastInteractionAt: new Date(NOW_MS - 3 * 60 * 60_000).toISOString(),
          lifecycleState: 'running',
        }),
        { nowMs: NOW_MS }
      )
    ).toMatchObject({ kind: 'idle', label: '3h' });
  });

  test('idle with a recent completion reads Done', () => {
    expect(
      resolveSidebarV2Status(session({ lastInteractionAt: new Date(NOW_MS - 60_000).toISOString() }), { nowMs: NOW_MS })
    ).toEqual({ hue: 'neutral', kind: 'done', label: 'Done', pulse: false, recede: false });
  });

  test('Done retires to relative time once the window closes', () => {
    expect(
      resolveSidebarV2Status(
        session({
          lastInteractionAt: new Date(NOW_MS - SIDEBAR_V2_DONE_WINDOW_MS - 1_000).toISOString(),
        }),
        { nowMs: NOW_MS }
      )
    ).toMatchObject({ kind: 'idle', label: '30m', recede: true });
  });

  test('a visit after the last activity retires Done immediately', () => {
    expect(
      resolveSidebarV2Status(session({ lastInteractionAt: new Date(NOW_MS - 60_000).toISOString() }), {
        lastVisitedAtMs: NOW_MS - 30_000,
        nowMs: NOW_MS,
      })
    ).toMatchObject({ kind: 'idle', label: '1m' });
  });

  test('a visit BEFORE the last activity leaves Done standing', () => {
    expect(
      resolveSidebarV2Status(session({ lastInteractionAt: new Date(NOW_MS - 60_000).toISOString() }), {
        lastVisitedAtMs: NOW_MS - 120_000,
        nowMs: NOW_MS,
      })
    ).toMatchObject({ kind: 'done' });
  });

  test('a session with no activity clock recedes with no label', () => {
    expect(resolveSidebarV2Status(session(), { nowMs: NOW_MS })).toEqual({
      hue: 'neutral',
      kind: 'idle',
      label: '',
      pulse: false,
      recede: true,
    });
  });

  test('a clock-skewed future stamp never renders a negative duration', () => {
    expect(
      resolveSidebarV2Status(session({ lastInteractionAt: new Date(NOW_MS + 60_000).toISOString() }), { nowMs: NOW_MS })
    ).toMatchObject({ kind: 'idle', label: 'now' });
  });
});

describe('hasUnseenCompletion', () => {
  test('a never-visited session does not count as unseen', () => {
    expect(hasUnseenCompletion({ activity: 'idle', lastInteractionAt: new Date(NOW_MS).toISOString() }, {})).toBe(
      false
    );
  });

  test('activity newer than the last visit is unseen', () => {
    expect(
      hasUnseenCompletion(
        { activity: 'idle', lastInteractionAt: new Date(NOW_MS).toISOString() },
        { lastVisitedAtMs: NOW_MS - 1 }
      )
    ).toBe(true);
    expect(
      hasUnseenCompletion(
        { activity: 'idle', lastInteractionAt: new Date(NOW_MS).toISOString() },
        { lastVisitedAtMs: NOW_MS }
      )
    ).toBe(false);
  });

  test('a still-working session has nothing completed to miss', () => {
    expect(
      hasUnseenCompletion(
        { activity: 'working', lastInteractionAt: new Date(NOW_MS).toISOString() },
        { lastVisitedAtMs: NOW_MS - 1 }
      )
    ).toBe(false);
  });
});

describe('resolveSidebarV2GroupStatus', () => {
  test('rolls up the loudest non-receding status', () => {
    const statuses = [
      resolveSidebarV2Status(session({ activity: 'working' }), { nowMs: NOW_MS }),
      resolveSidebarV2Status(session({ activity: 'attention', attentionKind: 'approval' }), {
        nowMs: NOW_MS,
      }),
      resolveSidebarV2Status(session({ lifecycleState: 'error' }), { nowMs: NOW_MS }),
    ];
    expect(resolveSidebarV2GroupStatus(statuses)).toMatchObject({ kind: 'approval' });
  });

  test('an all-resting group has no indicator', () => {
    expect(resolveSidebarV2GroupStatus([resolveSidebarV2Status(session(), { nowMs: NOW_MS })])).toBeNull();
  });

  test('priority keeps approval > input > working > failed order', () => {
    expect(sidebarV2StatusPriority('approval')).toBeGreaterThan(sidebarV2StatusPriority('input'));
    expect(sidebarV2StatusPriority('input')).toBeGreaterThan(sidebarV2StatusPriority('working'));
    expect(sidebarV2StatusPriority('working')).toBeGreaterThan(sidebarV2StatusPriority('failed'));
    expect(sidebarV2StatusPriority('failed')).toBeGreaterThan(sidebarV2StatusPriority('done'));
  });
});
