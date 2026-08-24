import { describe, expect, test } from 'vitest';
import {
  canSettleSidebarV2Session,
  canSnoozeSidebarV2Session,
  effectiveSidebarV2Settled,
  effectiveSidebarV2Snoozed,
  resolveSidebarV2AutoSettleDecision,
  resolveSidebarV2NextWakeAtMs,
  sessionLastActivityAtMs,
  SIDEBAR_V2_LIFECYCLE_CAPABILITIES_DISABLED,
  sidebarV2SessionRaisedHandWhileSnoozed,
  sidebarV2SessionWokeAtMs,
  type SidebarV2LifecycleSession,
} from './sidebar-v2-lifecycle';

const NOW_MS = Date.parse('2026-07-29T12:00:00.000Z');
const DAY_MS = 24 * 60 * 60 * 1_000;

function iso(offsetMs: number): string {
  return new Date(NOW_MS + offsetMs).toISOString();
}

function session(overrides: Partial<SidebarV2LifecycleSession> = {}): SidebarV2LifecycleSession {
  return { activity: 'idle', ...overrides };
}

const settleOptions = { autoSettleAfterDays: 3, nowMs: NOW_MS };

describe('sessionLastActivityAtMs', () => {
  test('takes the newest of the meaningful-activity and working clocks', () => {
    expect(
      sessionLastActivityAtMs({
        lastInteractionAt: iso(-2 * DAY_MS),
        workingStartedAt: iso(-1 * DAY_MS),
      })
    ).toBe(NOW_MS - DAY_MS);
  });

  test('falls through malformed stamps and reports null when nothing is usable', () => {
    expect(sessionLastActivityAtMs({ lastInteractionAt: 'nope', workingStartedAt: iso(-1_000) })).toBe(NOW_MS - 1_000);
    expect(sessionLastActivityAtMs({})).toBeNull();
  });
});

describe('canSettleSidebarV2Session / canSnoozeSidebarV2Session', () => {
  test('attention blocks both: hiding a blocked agent defeats the request', () => {
    expect(canSettleSidebarV2Session({ activity: 'attention' })).toBe(false);
    expect(canSnoozeSidebarV2Session({ activity: 'attention' })).toBe(false);
  });

  test('working blocks settle but not snooze', () => {
    expect(canSettleSidebarV2Session({ activity: 'working' })).toBe(false);
    expect(canSnoozeSidebarV2Session({ activity: 'working' })).toBe(true);
  });

  test('an idle session can do both', () => {
    expect(canSettleSidebarV2Session({ activity: 'idle' })).toBe(true);
    expect(canSnoozeSidebarV2Session({ activity: 'idle' })).toBe(true);
  });

  test('capability flags hide the affordances on older gxservers', () => {
    expect(
      canSettleSidebarV2Session({ activity: 'idle' }, { capabilities: SIDEBAR_V2_LIFECYCLE_CAPABILITIES_DISABLED })
    ).toBe(false);
    expect(
      canSnoozeSidebarV2Session({ activity: 'idle' }, { capabilities: SIDEBAR_V2_LIFECYCLE_CAPABILITIES_DISABLED })
    ).toBe(false);
  });
});

describe('effectiveSidebarV2Settled', () => {
  test('attention and working hold a session active even when explicitly settled', () => {
    expect(
      effectiveSidebarV2Settled(session({ activity: 'attention', settledOverride: 'settled' }), settleOptions)
    ).toBe(false);
    expect(effectiveSidebarV2Settled(session({ activity: 'working', settledOverride: 'settled' }), settleOptions)).toBe(
      false
    );
  });

  test('a running lifecycle never blocks a settle — that is pane liveness', () => {
    expect(
      effectiveSidebarV2Settled(session({ lifecycleState: 'running', settledOverride: 'settled' }), settleOptions)
    ).toBe(true);
  });

  test('the explicit override wins in both directions', () => {
    expect(effectiveSidebarV2Settled(session({ settledOverride: 'settled' }), settleOptions)).toBe(true);
    expect(
      effectiveSidebarV2Settled(
        session({ lastInteractionAt: iso(-10 * DAY_MS), settledOverride: 'active' }),
        settleOptions
      )
    ).toBe(false);
  });

  test('a merged or closed pull request settles immediately', () => {
    expect(
      effectiveSidebarV2Settled(session({ lastInteractionAt: iso(-1_000) }), {
        ...settleOptions,
        changeRequestState: 'merged',
      })
    ).toBe(true);
    expect(
      effectiveSidebarV2Settled(session({ lastInteractionAt: iso(-1_000) }), {
        ...settleOptions,
        changeRequestState: 'open',
      })
    ).toBe(false);
  });

  test('inactivity past the window auto-settles, inside it does not', () => {
    expect(effectiveSidebarV2Settled(session({ lastInteractionAt: iso(-3 * DAY_MS - 1_000) }), settleOptions)).toBe(
      true
    );
    expect(effectiveSidebarV2Settled(session({ lastInteractionAt: iso(-3 * DAY_MS + 1_000) }), settleOptions)).toBe(
      false
    );
  });

  test('a null auto-settle window disables inactivity settling', () => {
    expect(
      effectiveSidebarV2Settled(session({ lastInteractionAt: iso(-100 * DAY_MS) }), {
        autoSettleAfterDays: null,
        nowMs: NOW_MS,
      })
    ).toBe(false);
  });

  test('a session with no activity clock never surprise-settles', () => {
    expect(effectiveSidebarV2Settled(session(), settleOptions)).toBe(false);
  });

  test('nothing auto-settles when the server lacks the capability', () => {
    expect(
      effectiveSidebarV2Settled(session({ lastInteractionAt: iso(-100 * DAY_MS) }), {
        ...settleOptions,
        capabilities: SIDEBAR_V2_LIFECYCLE_CAPABILITIES_DISABLED,
      })
    ).toBe(false);
  });
});

describe('resolveSidebarV2AutoSettleDecision', () => {
  test('reports why a session would auto-settle', () => {
    expect(resolveSidebarV2AutoSettleDecision(session({ lastInteractionAt: iso(-4 * DAY_MS) }), settleOptions)).toEqual(
      { reason: 'inactivity', shouldAutoSettle: true }
    );
    expect(
      resolveSidebarV2AutoSettleDecision(session({ lastInteractionAt: iso(-1_000) }), {
        ...settleOptions,
        changeRequestState: 'closed',
      })
    ).toEqual({ reason: 'changeRequest', shouldAutoSettle: true });
  });

  test('a keep-active pin and blocked work suppress auto-settle', () => {
    expect(
      resolveSidebarV2AutoSettleDecision(
        session({ lastInteractionAt: iso(-4 * DAY_MS), settledOverride: 'active' }),
        settleOptions
      )
    ).toEqual({ reason: null, shouldAutoSettle: false });
    expect(
      resolveSidebarV2AutoSettleDecision(
        session({ activity: 'attention', lastInteractionAt: iso(-4 * DAY_MS) }),
        settleOptions
      )
    ).toEqual({ reason: null, shouldAutoSettle: false });
  });
});

describe('sidebarV2SessionRaisedHandWhileSnoozed', () => {
  test('a blocked agent always raises its hand', () => {
    expect(sidebarV2SessionRaisedHandWhileSnoozed(session({ activity: 'attention' }))).toBe(true);
  });

  test('only a FRESH failure raises the hand', () => {
    expect(
      sidebarV2SessionRaisedHandWhileSnoozed(
        session({
          lastInteractionAt: iso(-30 * 60_000),
          lifecycleState: 'error',
          snoozedAt: iso(-10 * 60_000),
        })
      )
    ).toBe(false);
    expect(
      sidebarV2SessionRaisedHandWhileSnoozed(
        session({
          lastInteractionAt: iso(-5 * 60_000),
          lifecycleState: 'error',
          snoozedAt: iso(-10 * 60_000),
        })
      )
    ).toBe(true);
  });

  test('work that completed after the snooze raises the hand', () => {
    expect(
      sidebarV2SessionRaisedHandWhileSnoozed(
        session({ lastInteractionAt: iso(-1 * 60_000), snoozedAt: iso(-10 * 60_000) })
      )
    ).toBe(true);
  });

  test('a session still working after the snooze stays quiet until it finishes', () => {
    expect(
      sidebarV2SessionRaisedHandWhileSnoozed(
        session({
          activity: 'working',
          lastInteractionAt: iso(-1 * 60_000),
          snoozedAt: iso(-10 * 60_000),
        })
      )
    ).toBe(false);
  });
});

describe('effectiveSidebarV2Snoozed', () => {
  test('hidden while the wake time is in the future', () => {
    expect(
      effectiveSidebarV2Snoozed(session({ snoozedAt: iso(-60_000), snoozedUntil: iso(60_000) }), {
        nowMs: NOW_MS,
      })
    ).toBe(true);
  });

  test('stops classifying as snoozed once the timer elapses', () => {
    expect(
      effectiveSidebarV2Snoozed(session({ snoozedAt: iso(-60_000), snoozedUntil: iso(-1) }), {
        nowMs: NOW_MS,
      })
    ).toBe(false);
  });

  test('missing or malformed data never hides a session', () => {
    expect(effectiveSidebarV2Snoozed(session(), { nowMs: NOW_MS })).toBe(false);
    expect(effectiveSidebarV2Snoozed(session({ snoozedUntil: 'nope' }), { nowMs: NOW_MS })).toBe(false);
  });

  test('a raised hand surfaces the row without clearing the server fields', () => {
    expect(
      effectiveSidebarV2Snoozed(
        session({ activity: 'attention', snoozedAt: iso(-60_000), snoozedUntil: iso(60_000) }),
        { nowMs: NOW_MS }
      )
    ).toBe(false);
  });

  test('snooze is inert when the server lacks the capability', () => {
    expect(
      effectiveSidebarV2Snoozed(session({ snoozedUntil: iso(60_000) }), {
        capabilities: SIDEBAR_V2_LIFECYCLE_CAPABILITIES_DISABLED,
        nowMs: NOW_MS,
      })
    ).toBe(false);
  });
});

describe('sidebarV2SessionWokeAtMs', () => {
  test('a timer wake reports the scheduled wake time', () => {
    expect(
      sidebarV2SessionWokeAtMs(session({ snoozedAt: iso(-60_000), snoozedUntil: iso(-1_000) }), {
        nowMs: NOW_MS,
      })
    ).toBe(NOW_MS - 1_000);
  });

  test('a still-snoozed session has not woken', () => {
    expect(
      sidebarV2SessionWokeAtMs(session({ snoozedAt: iso(-60_000), snoozedUntil: iso(60_000) }), {
        nowMs: NOW_MS,
      })
    ).toBeNull();
  });

  test('an early hand-raise wake reports the triggering activity, not the timer', () => {
    expect(
      sidebarV2SessionWokeAtMs(
        session({
          activity: 'attention',
          lastInteractionAt: iso(-30_000),
          snoozedAt: iso(-60_000),
          snoozedUntil: iso(60_000),
        }),
        { nowMs: NOW_MS }
      )
    ).toBe(NOW_MS - 30_000);
  });

  test('a session that never snoozed has no wake stamp', () => {
    expect(sidebarV2SessionWokeAtMs(session(), { nowMs: NOW_MS })).toBeNull();
  });
});

describe('resolveSidebarV2NextWakeAtMs', () => {
  test('schedules the exact boundary for a hidden snoozed row', () => {
    expect(
      resolveSidebarV2NextWakeAtMs(session({ snoozedAt: iso(-60_000), snoozedUntil: iso(90_000) }), {
        nowMs: NOW_MS,
      })
    ).toBe(NOW_MS + 90_000);
  });

  test('no timer for a row that is already visible', () => {
    expect(resolveSidebarV2NextWakeAtMs(session({ snoozedUntil: iso(-1_000) }), { nowMs: NOW_MS })).toBeNull();
  });
});
