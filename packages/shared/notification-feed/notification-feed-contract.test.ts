import { describe, expect, test } from 'vitest';
import {
  NOTIFICATION_FEED_KINDS,
  normalizeNotificationFeedItem,
  normalizeNotificationFeedState,
  notificationFeedKindLabel,
} from './notification-feed-contract';

const row = (overrides: Record<string, unknown>) => ({
  id: 'n1',
  projectId: 'P1',
  sessionId: 'S1',
  kind: 'finished',
  title: 'Thread',
  subtitle: 'Project',
  body: 'Done.',
  createdAt: '2026-10-31T12:00:00.000Z',
  read: false,
  ...overrides,
});

describe('notification feed contract', () => {
  test('keeps every kind gxserver writes, with its label', () => {
    expect(NOTIFICATION_FEED_KINDS.map(notificationFeedKindLabel)).toEqual([
      'Finished',
      'Needs input',
      'Bell',
      'Notification',
      'Not sent',
      'Failed',
      'Reset expiring',
      'Reset used',
    ]);
  });

  test('a failed row keeps its fixed body', () => {
    expect(normalizeNotificationFeedItem(row({ kind: 'failed', body: 'The agent run ended with an error.' }))).toEqual(
      row({ kind: 'failed', body: 'The agent run ended with an error.' })
    );
  });

  test('account rows keep the account they are about', () => {
    const account = row({ kind: 'resetExpiring', sessionId: 'account:acct-1', accountId: 'acct-1' });
    expect(normalizeNotificationFeedItem(account)?.accountId).toBe('acct-1');
    expect(normalizeNotificationFeedItem(row({ kind: 'resetRedeemed' }))?.kind).toBe('resetRedeemed');
    expect(normalizeNotificationFeedItem(row({}))).not.toHaveProperty('accountId');
  });

  test('an unknown kind is still dropped and the count stays the daemon’s', () => {
    const state = normalizeNotificationFeedState({
      items: [row({ id: 'a', kind: 'failed' }), row({ id: 'b', kind: 'somethingNewer' })],
      unreadCount: 2,
      nextUnreadId: 'a',
    });
    expect(state.items.map((item) => item.id)).toEqual(['a']);
    expect(state.unreadCount).toBe(2);
    expect(state.nextUnreadId).toBe('a');
  });
});
