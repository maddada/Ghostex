/**
 * CDXC:Notifications 2026-09-11 WHY:
 * The native titlebar paints the bell and the notification panel from the
 * cached feed state this module posts over the bridge; it never issues an RPC
 * of its own. Row clicks and the jump keys come back here as one command
 * event, so marking a row read and focusing its session run on the same
 * `focusSession` path the sidebar uses, and the daemon stays the only owner of
 * read state.
 * SEE-ALSO: packages/shared/notification-feed/notification-feed-contract.ts,
 * server/src/notification_feed, apps/desktop/src/notification_feed.
 */
import {
  GPUI_SIDEBAR_NOTIFICATION_FEED_COMMAND_EVENT_NAME,
  GPUI_SIDEBAR_NOTIFICATION_FEED_STATE_MESSAGE_TYPE,
} from './constants';
import type { GpuiSidebarRuntime } from './core';
import { localGxserverSessionIdForSidebarSession } from './helpers/app-shot';
import { createGxserverPresentationProjectSessionId } from '@/packages/shared/gxserver-presentation-sidebar-projection';
import type { SidebarSessionItem } from '@/packages/shared/session-grid-contract';
import {
  NOTIFICATION_FEED_READ_ENDPOINT,
  NOTIFICATION_FEED_UPDATE_ENDPOINT,
  normalizeNotificationFeedState,
} from '@/packages/shared/notification-feed/notification-feed-contract';
import type {
  NotificationFeedItem,
  NotificationFeedState,
  NotificationFeedUpdateParams,
} from '@/packages/shared/notification-feed/notification-feed-contract';

let notificationRevealRequestId = Date.now();

const NOTIFICATION_FEED_COMMAND_ACTIONS = [
  'open',
  'dismiss',
  'markRead',
  'markUnread',
  'markAllRead',
  'clearAll',
  'jumpToLatestUnread',
  'deferAndJumpNext',
] as const;

type NotificationFeedCommandAction = (typeof NOTIFICATION_FEED_COMMAND_ACTIONS)[number];

function isNotificationFeedCommandAction(value: unknown): value is NotificationFeedCommandAction {
  return typeof value === 'string' && (NOTIFICATION_FEED_COMMAND_ACTIONS as readonly string[]).includes(value);
}

export interface GpuiSidebarRuntimeNotificationFeedMethods {
  refreshNotificationFeed(): Promise<void>;
  postNotificationFeedState(): void;
  updateNotificationFeed(params: NotificationFeedUpdateParams): Promise<NotificationFeedState | undefined>;
  openNotificationFeedItem(item: NotificationFeedItem): Promise<void>;
  jumpToLatestUnreadNotification(): Promise<void>;
  deferNotificationAndJumpNext(): Promise<void>;
  findSidebarSessionForNotificationFeedItem(sessionId: string): SidebarSessionItem | undefined;
  focusedLocalGxserverSessionId(): string | undefined;
  handleGpuiSidebarNotificationFeedCommand(event: Event): void;
}

export const gpuiSidebarRuntimeNotificationFeedMethods = {
  async refreshNotificationFeed(this: GpuiSidebarRuntime): Promise<void> {
    const client = this.client;
    if (!client) {
      return;
    }
    let state: NotificationFeedState;
    try {
      state = normalizeNotificationFeedState(await client.rpc<unknown>(NOTIFICATION_FEED_READ_ENDPOINT, {}));
    } catch {
      return;
    }
    this.notificationFeedState = state;
    this.postNotificationFeedState();
  },

  /** Deduplicated so a refetch that changed nothing does not wake the bridge. */
  postNotificationFeedState(this: GpuiSidebarRuntime): void {
    const state = this.notificationFeedState;
    const message = {
      items: state.items,
      ...(state.nextUnreadId ? { nextUnreadId: state.nextUnreadId } : {}),
      type: GPUI_SIDEBAR_NOTIFICATION_FEED_STATE_MESSAGE_TYPE,
      unreadCount: state.unreadCount,
    };
    const payload = JSON.stringify(message);
    if (payload === this.lastNotificationFeedStatePayload) {
      return;
    }
    this.lastNotificationFeedStatePayload = payload;
    window.webkit?.messageHandlers?.ghostexNativeHost?.postMessage(message);
  },

  async updateNotificationFeed(
    this: GpuiSidebarRuntime,
    params: NotificationFeedUpdateParams
  ): Promise<NotificationFeedState | undefined> {
    const client = this.client;
    if (!client) {
      return undefined;
    }
    let state: NotificationFeedState;
    try {
      state = normalizeNotificationFeedState(await client.rpc<unknown>(NOTIFICATION_FEED_UPDATE_ENDPOINT, params));
    } catch {
      return undefined;
    }
    this.notificationFeedState = state;
    this.postNotificationFeedState();
    return state;
  },

  /**
   * The feed stores raw daemon session ids while the sidebar addresses local
   * sessions by their presentation id (sometimes project-scoped), so the match
   * goes through the same id projection every other local lookup uses.
   */
  findSidebarSessionForNotificationFeedItem(
    this: GpuiSidebarRuntime,
    sessionId: string
  ): SidebarSessionItem | undefined {
    for (const group of this.latestGroups) {
      for (const session of group.sessions) {
        if (localGxserverSessionIdForSidebarSession(session) === sessionId) {
          return session;
        }
      }
    }
    return undefined;
  },

  focusedLocalGxserverSessionId(this: GpuiSidebarRuntime): string | undefined {
    const focusedSessionId = this.focusedSessionId;
    if (!focusedSessionId) {
      return undefined;
    }
    for (const group of this.latestGroups) {
      for (const session of group.sessions) {
        if (session.sessionId === focusedSessionId) {
          return localGxserverSessionIdForSidebarSession(session);
        }
      }
    }
    return undefined;
  },

  /**
   * CDXC:Notifications 2026-09-11 DECISION:
   * User: clicking a notification always takes you to that session and reveals it in the sidebar.
   * The row's raw ids are turned into the sidebar's project-scoped session id without requiring the row to be in the current snapshot, so a session hidden by a Space, a filter, or a stale snapshot still gets focused; the reveal request then selects its Space, clears filters, and scrolls to it once the row is published. The daemon drops rows for deleted sessions on read, so a missing session never strands a row.
   */
  async openNotificationFeedItem(this: GpuiSidebarRuntime, item: NotificationFeedItem): Promise<void> {
    const sidebarSessionId =
      this.findSidebarSessionForNotificationFeedItem(item.sessionId)?.sessionId ??
      createGxserverPresentationProjectSessionId(item.projectId, item.sessionId);
    await this.updateNotificationFeed({ action: 'markRead', notificationId: item.id });
    void this.focusSession(sidebarSessionId, { sessionId: sidebarSessionId, type: 'focusSession' });
    this.messageSource.postMessage({
      requestId: ++notificationRevealRequestId,
      sessionId: sidebarSessionId,
      type: 'revealSidebarSession',
    });
  },

  async jumpToLatestUnreadNotification(this: GpuiSidebarRuntime): Promise<void> {
    await this.refreshNotificationFeed();
    const nextUnreadId = this.notificationFeedState.nextUnreadId;
    const item = nextUnreadId ? this.notificationFeedState.items.find((entry) => entry.id === nextUnreadId) : undefined;
    if (item) {
      await this.openNotificationFeedItem(item);
    }
  },

  /**
   * "I am not done with this one; come back to it last." The focused session's
   * newest row goes to the back of the unread queue, then the jump opens
   * whatever the daemon now says is next, unless that is the same session.
   */
  async deferNotificationAndJumpNext(this: GpuiSidebarRuntime): Promise<void> {
    const sessionId = this.focusedLocalGxserverSessionId();
    const state = sessionId
      ? await this.updateNotificationFeed({ action: 'deferUnread', sessionId })
      : (await this.refreshNotificationFeed(), this.notificationFeedState);
    if (!state?.nextUnreadId) {
      return;
    }
    const item = state.items.find((entry) => entry.id === state.nextUnreadId);
    if (!item || item.sessionId === sessionId) {
      return;
    }
    await this.openNotificationFeedItem(item);
  },

  handleGpuiSidebarNotificationFeedCommand(this: GpuiSidebarRuntime, event: Event): void {
    const detail = (event as CustomEvent<unknown>).detail;
    if (!detail || typeof detail !== 'object') {
      return;
    }
    const { action, notificationId } = detail as { action?: unknown; notificationId?: unknown };
    if (!isNotificationFeedCommandAction(action)) {
      return;
    }
    const id = typeof notificationId === 'string' && notificationId.length > 0 ? notificationId : undefined;
    switch (action) {
      case 'open': {
        const item = id ? this.notificationFeedState.items.find((entry) => entry.id === id) : undefined;
        if (item) {
          void this.openNotificationFeedItem(item);
        }
        return;
      }
      case 'dismiss':
      case 'markRead':
      case 'markUnread':
        if (id) {
          void this.updateNotificationFeed({ action, notificationId: id });
        }
        return;
      case 'markAllRead':
      case 'clearAll':
        void this.updateNotificationFeed({ action });
        return;
      case 'jumpToLatestUnread':
        void this.jumpToLatestUnreadNotification();
        return;
      case 'deferAndJumpNext':
        void this.deferNotificationAndJumpNext();
        return;
    }
  },
};

export { GPUI_SIDEBAR_NOTIFICATION_FEED_COMMAND_EVENT_NAME };
