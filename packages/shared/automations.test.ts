import { describe, expect, test } from 'vitest';
import { computeNextRunAt, normalizeAutomationState, parseAutomationResult } from './automations';

describe('automation normalization', () => {
  test('keeps valid definitions and preserves overdue nextRunAt values', () => {
    const state = normalizeAutomationState({
      automations: [
        {
          agentId: 'codex',
          createdAt: '2026-05-01T00:00:00.000Z',
          enabled: true,
          executionMode: { kind: 'worktree', setupCommand: 'bun install' },
          id: 'daily-review',
          name: 'Daily review',
          nextRunAt: '2026-05-02T00:00:00.000Z',
          projectIds: ['project-a'],
          prompt: 'Check for likely bugs.',
          schedule: { kind: 'daily', time: '09:30', timezone: 'Asia/Dubai' },
          updatedAt: '2026-05-01T00:00:00.000Z',
        },
        {
          agentId: 'codex',
          enabled: true,
          executionMode: { kind: 'local' },
          id: 'invalid',
          name: 'Invalid',
          projectIds: ['project-a'],
          prompt: 'Broken schedule',
          schedule: { kind: 'daily', time: '99:99' },
        },
      ],
    });

    expect(state.automations).toHaveLength(1);
    expect(state.automations[0]?.nextRunAt).toBe('2026-05-02T00:00:00.000Z');
    expect(state.automations[0]?.executionMode).toEqual({
      kind: 'worktree',
      setupCommand: 'bun install',
    });
  });

  test('normalizes run records and drops unknown statuses', () => {
    const state = normalizeAutomationState({
      automationRuns: [
        {
          automationId: 'a1',
          createdAt: '2026-05-01T00:00:00.000Z',
          id: 'run-1',
          isArchived: false,
          isUnread: true,
          projectId: 'project-a',
          status: 'needs_attention',
        },
        {
          automationId: 'a1',
          id: 'run-2',
          projectId: 'project-a',
          status: 'unknown',
        },
      ],
    });

    expect(state.runs).toHaveLength(1);
    expect(state.runs[0]?.status).toBe('needs_attention');
  });

  test('keeps thread execution expiry metadata', () => {
    const state = normalizeAutomationState({
      automations: [
        {
          agentId: 'codex',
          enabled: true,
          executionMode: {
            expiresAt: '2026-05-02T00:00:00.000Z',
            kind: 'thread',
            sessionId: 'session-a',
          },
          id: 'heartbeat',
          name: 'Heartbeat',
          projectIds: ['project-a'],
          prompt: 'Keep checking.',
          schedule: { everyMs: 15 * 60 * 1000, kind: 'interval' },
        },
      ],
    });

    expect(state.automations[0]?.executionMode).toEqual({
      expiresAt: '2026-05-02T00:00:00.000Z',
      kind: 'thread',
      sessionId: 'session-a',
    });
  });

  test('canonicalizes persisted dates to ISO UTC', () => {
    const state = normalizeAutomationState({
      automations: [
        {
          agentId: 'codex',
          createdAt: '2026-05-01T04:00:00+04:00',
          enabled: true,
          executionMode: { kind: 'local' },
          id: 'offset-date',
          name: 'Offset date',
          nextRunAt: '2026-05-02T09:30:00+04:00',
          projectIds: ['project-a'],
          prompt: 'Check the app.',
          schedule: { everyMs: 15 * 60 * 1000, kind: 'interval' },
          updatedAt: '2026-05-01T05:00:00+04:00',
        },
      ],
      automationRuns: [
        {
          automationId: 'offset-date',
          completedAt: '2026-05-02T10:00:00+04:00',
          createdAt: '2026-05-02T09:30:00+04:00',
          id: 'run-1',
          projectId: 'project-a',
          status: 'no_findings',
        },
      ],
    });

    expect(state.automations[0]).toMatchObject({
      createdAt: '2026-05-01T00:00:00.000Z',
      nextRunAt: '2026-05-02T05:30:00.000Z',
      updatedAt: '2026-05-01T01:00:00.000Z',
    });
    expect(state.runs[0]).toMatchObject({
      completedAt: '2026-05-02T06:00:00.000Z',
      createdAt: '2026-05-02T05:30:00.000Z',
    });
  });
});

describe('computeNextRunAt', () => {
  test('returns a future one-shot date only once', () => {
    const schedule = { kind: 'once', runAt: '2026-05-01T10:30:00.000Z' } as const;
    expect(computeNextRunAt(schedule, { after: new Date('2026-05-01T10:00:00.000Z') })).toBe(
      '2026-05-01T10:30:00.000Z'
    );
    expect(computeNextRunAt(schedule, { after: new Date('2026-05-01T10:30:00.000Z') })).toBeUndefined();
  });

  test('computes interval schedules from the supplied after date', () => {
    expect(
      computeNextRunAt({ everyMs: 15 * 60 * 1000, kind: 'interval' }, { after: new Date('2026-05-01T10:00:00.000Z') })
    ).toBe('2026-05-01T10:15:00.000Z');
  });

  test('computes the next daily wall-clock time', () => {
    expect(
      computeNextRunAt(
        { kind: 'daily', time: '09:30', timezone: 'UTC' },
        { from: new Date('2026-05-01T10:00:00.000Z') }
      )
    ).toBe('2026-05-02T09:30:00.000Z');
  });

  test('computes weekly schedules without relaunching in the past', () => {
    expect(
      computeNextRunAt(
        { days: [5], kind: 'weekly', time: '09:30', timezone: 'UTC' },
        { from: new Date('2026-05-01T10:00:00.000Z') }
      )
    ).toBe('2026-05-08T09:30:00.000Z');
  });

  test('computes common five-field cron expressions', () => {
    expect(
      computeNextRunAt(
        { expression: '*/15 * * * *', kind: 'cron', timezone: 'UTC' },
        { from: new Date('2026-05-01T10:07:20.000Z') }
      )
    ).toBe('2026-05-01T10:15:00.000Z');
  });
});

describe('parseAutomationResult', () => {
  test('extracts the explicit result convention and short summary', () => {
    expect(
      parseAutomationResult('Done\nAUTOMATION_RESULT: findings\nFound a stale config.\nCheck src/app.ts.')
    ).toEqual({
      result: 'findings',
      summary: 'Found a stale config.\nCheck src/app.ts.',
    });
  });

  test('returns no result for arbitrary terminal text', () => {
    expect(parseAutomationResult('Looks clean to me.')).toEqual({});
  });
});
