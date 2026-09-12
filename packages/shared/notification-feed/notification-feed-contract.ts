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

/** Newest rows the read endpoint returns; older rows stay stored until retention prunes them. */
export const NOTIFICATION_FEED_READ_LIMIT = 200;

/** Rows kept per daemon before the oldest read rows are pruned. */
export const NOTIFICATION_FEED_RETENTION_LIMIT = 500;

/** Longest body stored and shown, in characters. */
export const NOTIFICATION_FEED_BODY_MAX_CHARS = 280;

/**
 * finished: a hook Stop entered attention (attentionSource "turnComplete").
 * needsInput: any other attention (permission, question, approval, Action Required title).
 * bell: a terminal bell entered attention (only when showNotificationOnTerminalBell is on).
 * custom: posted by `ghostex notify` or the create endpoint.
 */
export type NotificationFeedKind = 'finished' | 'needsInput' | 'bell' | 'custom';

export const NOTIFICATION_FEED_KINDS: readonly NotificationFeedKind[] = ['finished', 'needsInput', 'bell', 'custom'];

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
  'markRead' | 'markUnread' | 'dismiss' | 'markSessionRead' | 'deferUnread' | 'markAllRead' | 'clearAll';

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
  const agentName = readString(raw.agentName);
  return {
    ...(agentName ? { agentName } : {}),
    body: readString(raw.body) ?? '',
    createdAt,
    id,
    kind,
    projectId,
    read: raw.read === true,
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
  const unreadCount =
    typeof raw.unreadCount === 'number' && Number.isFinite(raw.unreadCount) && raw.unreadCount >= 0
      ? Math.floor(raw.unreadCount)
      : items.filter((item) => !item.read).length;
  const nextUnreadId = readString(raw.nextUnreadId);
  return { ...(nextUnreadId ? { nextUnreadId } : {}), items, unreadCount };
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
  }
}
