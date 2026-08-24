import { describe, expect, test } from 'vitest';
import type { SidebarSessionItem } from './session-grid-contract-sidebar';
import { isSidebarV2BrowserSession, toSidebarV2Session, toSidebarV2SessionsFromGroups } from './sidebar-v2-session';

function sidebarSession(overrides: Partial<SidebarSessionItem> = {}): SidebarSessionItem {
  return {
    activity: 'idle',
    alias: 'Session',
    column: 0,
    isFocused: false,
    isRunning: true,
    isVisible: true,
    row: 0,
    sessionId: 'session-1',
    shortcutLabel: '1',
    ...overrides,
  };
}

describe('toSidebarV2Session', () => {
  test('carries the fields V2 reasons about', () => {
    expect(
      toSidebarV2Session(
        sidebarSession({
          activity: 'working',
          activityLabel: 'Running tests',
          isPinned: true,
          lastInteractionAt: '2026-07-29T11:00:00.000Z',
          lifecycleState: 'running',
          sessionKind: 'terminal',
          workingStartedAt: '2026-07-29T11:30:00.000Z',
        })
      )
    ).toEqual({
      activity: 'working',
      activityLabel: 'Running tests',
      isPinned: true,
      lastInteractionAt: '2026-07-29T11:00:00.000Z',
      lifecycleState: 'running',
      sessionId: 'session-1',
      sessionKind: 'terminal',
      workingStartedAt: '2026-07-29T11:30:00.000Z',
    });
  });

  test("derives the lifecycle state from the contract's own resolver", () => {
    expect(toSidebarV2Session(sidebarSession({ isRunning: false, isSleeping: true })).lifecycleState).toBe('sleeping');
    expect(toSidebarV2Session(sidebarSession({ isLive: false, isRunning: false })).lifecycleState).toBe('done');
    expect(toSidebarV2Session(sidebarSession({ isLive: true })).lifecycleState).toBe('running');
  });

  test('legacy browser rows keep their session kind', () => {
    expect(toSidebarV2Session(sidebarSession({ kind: 'browser' })).sessionKind).toBe('browser');
    expect(isSidebarV2BrowserSession(toSidebarV2Session(sidebarSession({ kind: 'browser' })))).toBe(true);
    expect(isSidebarV2BrowserSession(toSidebarV2Session(sidebarSession()))).toBe(false);
  });

  test('overrides supply the lifecycle fields the contract cannot carry yet', () => {
    expect(
      toSidebarV2Session(sidebarSession(), {
        attentionKind: 'approval',
        createdAt: '2026-07-29T09:00:00.000Z',
        settledAt: '2026-07-29T10:00:00.000Z',
        settledOverride: 'settled',
        snoozedAt: '2026-07-29T10:30:00.000Z',
        snoozedUntil: '2026-07-29T18:00:00.000Z',
        worktreePath: '/wt/feature',
      })
    ).toMatchObject({
      attentionKind: 'approval',
      createdAt: '2026-07-29T09:00:00.000Z',
      settledAt: '2026-07-29T10:00:00.000Z',
      settledOverride: 'settled',
      snoozedAt: '2026-07-29T10:30:00.000Z',
      snoozedUntil: '2026-07-29T18:00:00.000Z',
      worktreePath: '/wt/feature',
    });
  });

  test('a widened source row is read directly, so no adapter change is needed once the contract carries the fields', () => {
    expect(toSidebarV2Session({ ...sidebarSession(), createdAt: '2026-07-29T09:00:00.000Z' }).createdAt).toBe(
      '2026-07-29T09:00:00.000Z'
    );
  });

  test('explicit overrides win over source values', () => {
    expect(
      toSidebarV2Session(
        { ...sidebarSession(), createdAt: '2026-07-29T09:00:00.000Z' },
        { createdAt: '2026-07-01T09:00:00.000Z' }
      ).createdAt
    ).toBe('2026-07-01T09:00:00.000Z');
  });

  test('absent fields are omitted rather than set to undefined', () => {
    expect(Object.keys(toSidebarV2Session(sidebarSession())).sort()).toEqual([
      'activity',
      'lifecycleState',
      'sessionId',
    ]);
  });
});

describe('toSidebarV2SessionsFromGroups', () => {
  const groups = [
    {
      groupId: 'project-a',
      sessions: [sidebarSession({ sessionId: 'a1' }), sidebarSession({ sessionId: 'a2' })],
    },
    { groupId: 'project-b', sessions: [sidebarSession({ sessionId: 'b1' })] },
  ];

  test('flattens groups and stamps the owning project id', () => {
    expect(toSidebarV2SessionsFromGroups(groups).map((session) => [session.sessionId, session.projectId])).toEqual([
      ['a1', 'project-a'],
      ['a2', 'project-a'],
      ['b1', 'project-b'],
    ]);
  });

  test('per-session overrides are applied during the flatten', () => {
    expect(
      toSidebarV2SessionsFromGroups(groups, {
        overridesBySessionId: new Map([['a2', { settledOverride: 'settled' as const }]]),
      }).find((session) => session.sessionId === 'a2')
    ).toMatchObject({ projectId: 'project-a', settledOverride: 'settled' });
  });
});
