import type { SidebarSessionTag } from './session-tags';

export const GRID_COLUMN_COUNT = 3;
export const MAX_GROUP_COUNT = 20;
export const MAX_SESSION_DISPLAY_ID_COUNT = 100;
export const DEFAULT_AGENT_MANAGER_ZOOM_PERCENT = 100;
export const MIN_AGENT_MANAGER_ZOOM_PERCENT = 50;
export const MAX_AGENT_MANAGER_ZOOM_PERCENT = 200;
export const DEFAULT_MAIN_GROUP_ID = 'group-1';
export const DEFAULT_MAIN_GROUP_TITLE = 'Main';

export type VisibleSessionCount = number;

export type TerminalViewMode = 'horizontal' | 'vertical' | 'grid';

export type SessionGridDirection = 'up' | 'right' | 'down' | 'left';

export type SessionPaneSplitDirection = 'horizontal' | 'vertical';

/**
 * CDXC:NativeSplits 2026-05-10-18:30
 * Workspace panes persist as an explicit split/tab tree instead of deriving
 * native geometry from visibleSessionIds counts. This lets Cmd+D and
 * Cmd+Shift+D add the new terminal beside the targeted pane without the
 * previous four-pane auto-grid reshuffle, and gives tab grouping a durable
 * restart-safe place in the session snapshot.
 */
export type SessionPaneLayoutNode =
  | {
      kind: 'leaf';
      sessionId: string;
    }
  | {
      activeSessionId?: string;
      kind: 'tabs';
      sessionIds: string[];
    }
  | {
      children: SessionPaneLayoutNode[];
      direction: SessionPaneSplitDirection;
      kind: 'split';
      ratio?: number;
    };

export type SidebarSessionActivityState = 'idle' | 'working' | 'attention';
export type SessionLifecycleState = 'running' | 'done' | 'sleeping' | 'error';
/**
 * CDXC:SessionTitleSync 2026-04-27-17:45
 * Session titles keep provenance so restart restore can trust real terminal
 * titles and browser page titles while rejecting placeholders, shell paths,
 * command names, and legacy auto-captured noise such as mojibake.
 *
 * CDXC:BrowserPanes 2026-05-03-01:58
 * Browser pane cards and native title bars use webpage titles supplied by the
 * embedded WKWebView. Track that source separately from terminal OSC/window
 * titles so browser reloads can refresh page identity without looking like a
 * user rename.
 */
export type SessionTitleSource = 'browser-auto' | 'generated' | 'placeholder' | 'terminal-auto' | 'user';

export type SidebarTheme =
  | 'dark-1'
  | 'dark-2'
  | 'plain-dark'
  | 'plain-light'
  | 'dark-green'
  | 'dark-blue'
  | 'dark-red'
  | 'dark-pink'
  | 'dark-orange'
  | 'light-blue'
  | 'light-green'
  | 'light-pink'
  | 'light-orange';

export type SidebarThemeSetting =
  | 'auto'
  | 'plain'
  | 'dark-1'
  | 'dark-2'
  | 'plain-light'
  | 'dark-green'
  | 'dark-blue'
  | 'dark-red'
  | 'dark-pink'
  | 'dark-orange'
  | 'light-blue'
  | 'light-green'
  | 'light-pink'
  | 'light-orange';

export type SidebarThemeVariant = 'light' | 'dark';

export type SessionKind = 'browser' | 'terminal';
export type TerminalSurface = 'workspace' | 'commands';
export type CommandsPanelMode = 'floating' | 'pinned';
export type TerminalEngine = 'ghostty-native';
export type TerminalSessionPersistenceProvider = 'tmux' | 'zmx' | 'zellij';

export type BrowserSessionMetadata = {
  faviconDataUrl?: string;
  url: string;
};

export type BaseSessionRecord = {
  kind: SessionKind;
  sessionId: string;
  displayId: string;
  firstUserMessage?: string;
  title: string;
  titleSource?: SessionTitleSource;
  alias: string;
  isFavorite?: boolean;
  /**
   * CDXC:SessionTags 2026-06-05-12:30:
   * Session tags persist the expanded Favorite replacement on the canonical
   * session record so active rows, restored Previous Sessions, and local
   * Electron panes can keep the same marker after app restart.
   */
  sessionTag?: SidebarSessionTag;
  /**
   * CDXC:PinnedSessions 2026-05-28-12:04:
   * Pinned sessions are project-scoped ordering state, separate from Favorite
   * so pinning a live sidebar row does not affect previous-session filters or
   * favorite auto-sleep policy.
   */
  isPinned?: boolean;
  /**
   * CDXC:PanePopOut 2026-05-11-09:35
   * Popped-out panes keep their terminal or browser runtime alive in a native
   * ghostex window while the original workspace slot stays visible as a reattach
   * placeholder. This is presentation state, not sleep state.
   */
  isPoppedOut?: boolean;
  isSleeping?: boolean;
  slotIndex: number;
  row: number;
  column: number;
  createdAt: string;
  /**
   * CDXC:AutoSleep 2026-05-28-08:32:
   * Auto Sleep must distinguish semantic agent activity from runtime lifecycle.
   * Persist the most recent start/wake time on terminal and browser-capable
   * session records so an old idle agent does not immediately sleep again after
   * the user wakes it.
   */
  lastStartedAt?: string;
  /**
   * CDXC:AutoSleep 2026-05-28-08:32:
   * Browser-like panes sleep from user access time, not agent activity, because
   * viewing a browser/project/editor pane is the meaningful interaction even
   * when the page itself is quiet.
   */
  lastAccessedAt?: string;
};

export type TerminalSessionRecord = BaseSessionRecord & {
  agentName?: string;
  /**
   * CDXC:CloseAfterDone 2026-06-15-21:00:
   * Close After Done is a per-terminal arming flag, not a lifecycle fallback.
   * Keep it on the terminal record so the sidebar can wait for a continuous
   * Done state and then close through the normal session close path.
   */
  closeAfterDone?: boolean;
  /**
   * CDXC:DelayedSend 2026-05-21-12:21:
   * Provider-backed delayed sends must survive app restart with the terminal
   * session that owns the prompt. Persist the absolute deadline on the terminal
   * record so restore can wake the session and re-arm the pending Enter key.
   *
   * CDXC:DelayedSend 2026-06-19-14:55:
   * Restart should resume Delayed Send near its last in-app countdown position
   * instead of consuming time while Ghostex is closed. Persist the latest
   * remaining-duration checkpoint beside the live deadline so native can build
   * a fresh deadline on startup.
   */
  delayedSendDeadlineAt?: string;
  delayedSendRemainingMs?: number;
  /**
   * CDXC:SessionLastActive 2026-05-17-02:45:
   * Last Active is durable sidebar metadata for terminal sessions. Persist it
   * on the canonical session record so sleeping or unmounted sessions can keep
   * correct timestamps and Last Active ordering immediately after app restart.
   */
  lastActivityAt?: string;
  /**
   * CDXC:StartupRestore 2026-05-21-13:04:
   * Runtime working/attention state is normally inferred from terminal output,
   * but app quit needs a durable hint so interrupted work wakes on next launch.
   * Attention is restored visually; working only wakes the session and is then
   * cleared because the resumed terminal may no longer be active.
   */
  restoreActivity?: Extract<SidebarSessionActivityState, 'attention' | 'working'>;
  /**
   * CDXC:CommandPanes 2026-05-16-15:08:
   * Command-pane reuse is keyed by the configured action title rather than the
   * mutable command id. Persist the title owner on command terminal records so
   * Ghostex can rediscover the correct idle pane after restart or state hydrate.
   */
  commandTitle?: string;
  /**
   * CDXC:PiAgent 2026-05-08-09:42
   * Some agents need a durable conversation identity that is not the sidebar
   * title or terminal-provider session name. Pi restore/fork uses its session
   * jsonl path/id, so store that metadata on the terminal record.
   *
   * CDXC:CodexAgent 2026-05-11-07:35
   * Codex can publish its conversation UUID before a human title exists. Store
   * that UUID here for restore while the display-title layer keeps UUID-looking
   * titles rendered as unnamed `Codex Session` cards.
   */
  agentSessionId?: string;
  agentSessionPath?: string;
  kind: 'terminal';
  sessionPersistenceName?: string;
  sessionPersistenceProvider?: TerminalSessionPersistenceProvider;
  surface?: TerminalSurface;
  terminalEngine: TerminalEngine;
  /** @deprecated use sessionPersistenceName for tmux, zmx, and zellij providers. */
  tmuxSessionName?: string;
};

export type BrowserSessionRecord = BaseSessionRecord & {
  browser: BrowserSessionMetadata;
  kind: 'browser';
};

export type SessionRecord = BrowserSessionRecord | TerminalSessionRecord;

export type CreateSessionRecordOptions =
  | {
      browser: BrowserSessionMetadata;
      displayId?: string;
      initialPresentation?: 'background' | 'focused';
      kind: 'browser';
      sessionId?: string;
      title?: string;
      titleSource?: SessionTitleSource;
    }
  | {
      agentName?: string;
      agentSessionId?: string;
      agentSessionPath?: string;
      closeAfterDone?: boolean;
      commandTitle?: string;
      delayedSendDeadlineAt?: string;
      delayedSendRemainingMs?: number;
      displayId?: string;
      initialPresentation?: 'background' | 'focused';
      kind?: 'terminal';
      sessionId?: string;
      sessionTag?: SidebarSessionTag;
      sessionPersistenceName?: string;
      sessionPersistenceProvider?: TerminalSessionPersistenceProvider;
      surface?: TerminalSurface;
      terminalEngine?: TerminalEngine;
      /** @deprecated use sessionPersistenceName for tmux, zmx, and zellij providers. */
      tmuxSessionName?: string;
      title?: string;
      titleSource?: SessionTitleSource;
    };

export type SessionGridSnapshot = {
  focusedSessionId?: string;
  fullscreenRestoreVisibleCount?: VisibleSessionCount;
  paneLayout?: SessionPaneLayoutNode;
  sessions: SessionRecord[];
  visibleCount: VisibleSessionCount;
  visibleSessionIds: string[];
  viewMode: TerminalViewMode;
};

export type CommandsPanelState = {
  activeSessionId?: string;
  heightRatio: number;
  isVisible: boolean;
  mode: CommandsPanelMode;
  paneLayout?: SessionPaneLayoutNode;
  sessions: TerminalSessionRecord[];
};

export type SessionGroupRecord = {
  groupId: string;
  snapshot: SessionGridSnapshot;
  title: string;
};

export type GroupedSessionWorkspaceSnapshot = {
  activeGroupId: string;
  groups: SessionGroupRecord[];
  nextGroupNumber: number;
  nextSessionDisplayId: number;
  nextSessionNumber: number;
};
