/**
 * CDXC:Notifications 2026-09-11 DECISION:
 * User: Ghostex gets a notification feed with a bell in the titlebar right after the Next button, a count badge, a panel listing what each agent said, and keys to jump through unread items.
 * gxserver owns the feed (see `server/src/notification_feed`) so the desktop app, the web app, and mobile read one list with one read state.
 * This file is the wire contract every client shares: the row shape, the endpoint names, the feed-changed event, and the command vocabulary the native titlebar sends back.
 */

export const NOTIFICATION_FEED_READ_ENDPOINT = '/api/readNotificationFeed';
export const NOTIFICATION_FEED_UPDATE_ENDPOINT = '/api/updateNotificationFeed';
export const NOTIFICATION_FEED_CREATE_ENDPOINT = '/api/createNotification';

/** Announced over the events socket after every feed change; clients refetch the feed. */
export const NOTIFICATION_FEED_CHANGED_EVENT_TYPE = 'notificationFeedChanged';

/** Newest rows the read endpoint returns. */
export const NOTIFICATION_FEED_READ_LIMIT = 40;

/** Rows kept per daemon; older rows are deleted, read or unread. */
export const NOTIFICATION_FEED_RETENTION_LIMIT = 40;

/** Longest body stored and shown, in characters. */
export const NOTIFICATION_FEED_BODY_MAX_CHARS = 280;

/**
 * finished: a hook Stop entered attention (attentionSource "turnComplete").
 * needsInput: any other attention (permission, question, approval, Action Required title).
 * bell: a terminal bell entered attention (only when showNotificationOnTerminalBell is on).
 * custom: posted by `ghostex notify` or the create endpoint.
 * sendFailed: a chat message could not be sent for good ("Not sent"); no other kind replaces it while it is unread.
 * failed: the agent's run ended on an error gxserver positively identified (a hook's failure event, account recovery giving up or its quick retries failing, a program's OSC 7501 error); the body is one fixed sentence.
 * resetExpiring / resetRedeemed: account rows about a saved account's banked usage reset; they carry `accountId`, and their `sessionId` names no session.
 * security: a sensitive action recorded in the security log (server/src/audit/notify.rs); it carries `securityEntry`, and its `sessionId` names no session.
 */
export type NotificationFeedKind =
  | 'finished'
  | 'needsInput'
  | 'bell'
  | 'custom'
  | 'sendFailed'
  | 'failed'
  | 'resetExpiring'
  | 'resetRedeemed'
  | 'security';

export const NOTIFICATION_FEED_KINDS: readonly NotificationFeedKind[] = [
  'finished',
  'needsInput',
  'bell',
  'custom',
  'sendFailed',
  'failed',
  'resetExpiring',
  'resetRedeemed',
  'security',
];

export type NotificationFeedItem = {
  id: string;
  projectId: string;
  sessionId: string;
  kind: NotificationFeedKind;
  /** Session title at the time the row was written. */
  title: string;
  /** Project name at the time the row was written. */
  subtitle: string;
  /** Last assistant message text, the custom body, or a kind-specific line. */
  body: string;
  /** Agent slug when known (claude, codex, ...). */
  agentName?: string;
  /** Set on account rows: the saved account the row is about. */
  accountId?: string;
  /** Set on security rows: the security log entry the row announces. */
  securityEntry?: number;
  /** ISO timestamp. */
  createdAt: string;
  read: boolean;
};

export type NotificationFeedState = {
  /** Newest first. */
  items: NotificationFeedItem[];
  unreadCount: number;
  /**
   * The row a "jump to latest unread" key should open: the newest unread row, except that rows pushed back with deferUnread come last, oldest deferral first.
   * The daemon computes it so every client jumps in the same order.
   */
  nextUnreadId?: string;
  /** Set when the nextUnreadId row was pushed back; a desktop merging several computers' feeds ranks it last. */
  nextUnreadDeferredAt?: string;
};

export const EMPTY_NOTIFICATION_FEED_STATE: NotificationFeedState = {
  items: [],
  unreadCount: 0,
};

/**
 * Actions for the update endpoint.
 * markRead / markUnread / dismiss take `notificationId`.
 * markSessionRead takes `sessionId` and reads every row of that session.
 * deferUnread takes `sessionId`: the session's newest row becomes unread and moves to the back of the unread queue, so a jump lands on it last.
 * markAllRead and clearAll take nothing.
 */
export type NotificationFeedUpdateAction =
  | 'markRead'
  | 'markUnread'
  | 'dismiss'
  | 'markSessionRead'
  | 'deferUnread'
  | 'markAllRead'
  | 'clearAll';

export type NotificationFeedUpdateParams = {
  action: NotificationFeedUpdateAction;
  notificationId?: string;
  sessionId?: string;
};

export type NotificationFeedCreateParams = {
  projectId: string;
  sessionId: string;
  title?: string;
  body?: string;
};

export type NotificationFeedRpc = <T>(path: string, params: Record<string, unknown>) => Promise<T>;

function readString(value: unknown): string | undefined {
  return typeof value === 'string' ? value : undefined;
}

export function normalizeNotificationFeedItem(value: unknown): NotificationFeedItem | undefined {
  if (!value || typeof value !== 'object') {
    return undefined;
  }
  const raw = value as Record<string, unknown>;
  const id = readString(raw.id);
  const projectId = readString(raw.projectId);
  const sessionId = readString(raw.sessionId);
  const kind = readString(raw.kind) as NotificationFeedKind | undefined;
  const createdAt = readString(raw.createdAt);
  if (!id || !projectId || !sessionId || !createdAt || !kind || !NOTIFICATION_FEED_KINDS.includes(kind)) {
    return undefined;
  }
  const accountId = readString(raw.accountId);
  const agentName = readString(raw.agentName);
  return {
    ...(accountId ? { accountId } : {}),
    ...(agentName ? { agentName } : {}),
    body: readString(raw.body) ?? '',
    createdAt,
    id,
    kind,
    projectId,
    read: raw.read === true,
    ...(typeof raw.securityEntry === 'number' ? { securityEntry: raw.securityEntry } : {}),
    sessionId,
    subtitle: readString(raw.subtitle) ?? '',
    title: readString(raw.title) ?? '',
  };
}

export function normalizeNotificationFeedState(value: unknown): NotificationFeedState {
  if (!value || typeof value !== 'object') {
    return EMPTY_NOTIFICATION_FEED_STATE;
  }
  const raw = value as Record<string, unknown>;
  const items = Array.isArray(raw.items)
    ? raw.items.map(normalizeNotificationFeedItem).filter((item): item is NotificationFeedItem => item !== undefined)
    : [];
  // The count is the unread rows kept here, never the daemon's number, so a row a client cannot
  // show never lights the bell (gx-core notification_feed.rs, 2026-11-08).
  const unread = items.filter((item) => !item.read);
  const nextUnreadId = readString(raw.nextUnreadId);
  const nextIsListed = nextUnreadId !== undefined && unread.some((item) => item.id === nextUnreadId);
  return { ...(nextIsListed ? { nextUnreadId } : {}), items, unreadCount: unread.length };
}

/** Human label for a kind, shared by every surface that lists the feed. */
export function notificationFeedKindLabel(kind: NotificationFeedKind): string {
  switch (kind) {
    case 'finished':
      return 'Finished';
    case 'needsInput':
      return 'Needs input';
    case 'bell':
      return 'Bell';
    case 'custom':
      return 'Notification';
    case 'sendFailed':
      return 'Not sent';
    case 'failed':
      return 'Failed';
    case 'resetExpiring':
      return 'Reset expiring';
    case 'resetRedeemed':
      return 'Reset used';
    case 'security':
      return 'Security';
  }
}
