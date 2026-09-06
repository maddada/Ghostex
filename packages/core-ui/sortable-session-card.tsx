import {
  IconArchive,
  IconChevronRight,
  IconCheck,
  IconCopy,
  IconClock,
  IconDeviceMobile,
  IconDots,
  IconFileExport,
  IconFocus2,
  IconGitFork,
  IconLayoutColumns,
  IconLayoutSidebarRightExpand,
  IconMessageCircle,
  IconMoon,
  IconNote,
  IconPencil,
  IconPinned,
  IconPinnedOff,
  IconPlayerPlay,
  IconRefresh,
  IconSparkles,
  IconSwitchHorizontal,
  IconTag,
  IconX,
} from '@tabler/icons-react';
import { Modifier, type DragOperation } from '@dnd-kit/abstract';
import { KeyboardSensor, PointerSensor } from '@dnd-kit/dom';
import { SortableKeyboardPlugin } from '@dnd-kit/dom/sortable';
import { useDroppable } from '@dnd-kit/react';
import { useSortable } from '@dnd-kit/react/sortable';
import {
  Fragment,
  useCallback,
  useEffect,
  useEffectEvent,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
  type KeyboardEvent as ReactKeyboardEvent,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent as ReactPointerEvent,
} from 'react';
import { getSidebarSessionLifecycleState, type SidebarSessionItem } from '../shared/session-grid-contract';
import { SessionChatHostActionAgentIcon } from './chat/session-chat-host-action-agent-icon';
import { resolveSessionChatTranscriptAgent } from '../shared/session-chat';
import { getEnabledVisibleSidebarSessionTagSections, type SidebarSessionTagListItem } from '../shared/session-tags';
import { buildSidebarSessionDetailsClipboardText } from '../shared/session-details-copy';
import {
  getSessionCardTitleTooltip,
  OverflowTooltipText,
  SessionCardContent,
  SessionFloatingAgentIcon,
  shouldShowTerminalSessionIcon,
} from './session-card-content';
import { getSessionStatusAnchorName } from './session-status-anchor';
import { createSessionDragData, createSessionDropTargetData, createSessionDropTargetId } from './sidebar-dnd';
import { closeAppModal, openAppModal } from './app-modal-host-bridge';
import { SidebarContextMenuPortal } from './sidebar-context-menu-portal';
import { postSidebarRefreshDebugLog } from './sidebar-refresh-debug-log';
import { getSidebarReorderActivationConstraints } from './sidebar-reorder-activation';
import { useSidebarItemTooltipDelayMs } from './tooltip-delay';
import { useSidebarStore, type SidebarGroupRecord } from './sidebar-store';
import {
  getEffectiveSessionTag,
  getSidebarSessionTagLabel,
  SessionTagIcon,
  type SidebarSessionTag,
} from './session-tag-ui';
import type { WebviewApi } from './webview-api';
import { createPortal, flushSync } from 'react-dom';

const CONTEXT_MENU_MARGIN_PX = 12;
const CONTEXT_MENU_WIDTH_PX = 178;
const CONTEXT_MENU_ITEM_HEIGHT_PX = 34;
const CONTEXT_MENU_DIVIDER_HEIGHT_PX = 13;
const CONTEXT_MENU_VERTICAL_PADDING_PX = 12;
/*
 * CDXC:ContextMenus 2026-08-26:
 * A group heading row: 11px text plus its 4px/2px insets and the section grid
 * gap. Only the pre-render placement estimate uses it; the portal still clamps
 * against the rendered height once the menu exists.
 */
const CONTEXT_MENU_GROUP_LABEL_HEIGHT_PX = 21;
const POINTER_ALIGNED_CONTEXT_MENU_MIN_SIDEBAR_WIDTH_PX = 235;
const COMPLETION_FLASH_DURATION_MS = 3_000;
const SESSION_CARD_IMMEDIATE_FOCUS_CLICK_SUPPRESSION_MS = 1_500;
const SLEEP_BELOW_DEBUG_EVENT_PREFIX = 'sleepBelow';
const SESSION_CARD_POINTER_FOCUS_BLOCKING_SELECTOR = [
  'button',
  'input',
  'select',
  'textarea',
  'a',
  "[role='button']",
  "[role='menu']",
  "[role='menuitem']",
  "[data-session-card-pointer-focus-blocking='true']",
].join(', ');
const SESSION_CARD_DND_INTERACTIVE_SELECTOR = [
  'input:not([disabled])',
  'select:not([disabled])',
  'textarea:not([disabled])',
  'button:not([disabled])',
  'a[href]',
  "[contenteditable]:not([contenteditable='false'])",
].join(', ');
const SESSION_CARD_PIN_DRAG_HANDLE_SELECTOR = '.session-pinned-floating-button';
const DND_SESSION_CARD_AX_ATTRIBUTES = [
  'aria-describedby',
  'aria-disabled',
  'aria-grabbed',
  'aria-pressed',
  'aria-roledescription',
] as const;
const DND_SESSION_FRAME_AX_ATTRIBUTES = [...DND_SESSION_CARD_AX_ATTRIBUTES, 'role', 'tabindex'] as const;
const EMPTY_SESSION_IDS: readonly string[] = [];

/*
 * CDXC:Sidebar 2026-07-02-13:05:
 * Session rows only reorder vertically, so the drag ghost stays locked to the
 * row's horizontal position instead of following pointer drift sideways.
 */
class RestrictSessionDragToVerticalAxis extends Modifier {
  apply({ transform }: DragOperation) {
    return { x: 0, y: transform.y };
  }
}

const sessionCardModifiers = [RestrictSessionDragToVerticalAxis];

const sessionCardSensors = [
  PointerSensor.configure({
    activationConstraints: getSidebarReorderActivationConstraints,
    preventActivation(event, source) {
      return shouldPreventSessionCardDragActivation(event, source.element);
    },
  }),
  KeyboardSensor,
];

type ContextMenuPosition = {
  x: number;
  y: number;
};

type SessionContextMenuAction = {
  danger?: boolean;
  icon: ReactNode;
  key: string;
  label: string;
  onClick: (event: ReactMouseEvent<HTMLButtonElement>) => void;
  submenu?: 'advanced' | 'session-tags' | 'switch-account';
};

export type SidebarSessionSelectionChangeRequest = {
  groupId: string;
  mode: 'additive' | 'clear' | 'range';
  reason?: string;
  sessionId: string;
};

export type SortableSessionCardSharedSettings = {
  enableSessionParking: boolean;
  hideBrowserFaviconUntilHover: boolean;
  hideSessionAgentIconUntilHover: boolean;
  renameSessionOnDoubleClick: boolean;
  showCloseButton: boolean;
  showDebugSessionNumbers: boolean;
  showLastActiveTime: boolean;
  showSessionCloseContextMenuAction: boolean;
  showSessionCommandCopyActions: boolean;
  showSessionDetailsCopyAction: boolean;
};

export type SortableSessionCardProps = {
  completionFlashNonce?: number;
  dragDisabled?: boolean;
  dropDisabled?: boolean;
  forcedDropPosition?: 'before' | 'after';
  groupId: string;
  index: number;
  isProjectSessionListOverflowRow?: boolean;
  isSearchSelected?: boolean;
  onFocusRequested?: (groupId: string, sessionId: string) => void;
  onSessionSelectionChange?: (request: SidebarSessionSelectionChangeRequest) => void;
  projectSessionListMoreRow?: {
    count: number;
    onReveal: () => void;
  };
  sessionCardSettings: SortableSessionCardSharedSettings;
  sessionGroup?: SidebarGroupRecord;
  sessionTagListItems?: readonly SidebarSessionTagListItem[];
  sessionIdsBelowSource?: readonly string[];
  sessionIdsBelowStartIndex?: number;
  sessionId: string;
  selectedSessionIds?: readonly string[];
  shouldKeepLastProjectSessionVisibleOnClose?: boolean;
  showGroupDropTargetChrome?: boolean;
  showGroupConnector?: boolean;
  showDropPositionIndicator?: boolean;
  vscode: WebviewApi;
};

export function resolveSessionCardSessionIdsBelow({
  sessionIdsBelowSource,
  sessionIdsBelowStartIndex,
}: {
  sessionIdsBelowSource?: readonly string[];
  sessionIdsBelowStartIndex?: number;
}): readonly string[] {
  /*
   * CDXC:ContextMenus 2026-06-30-02:45:
   * Session-card below actions stay scoped to the group/project-visible order,
   * but large project lists must not slice that order once per rendered row.
   * Keep the shared source list plus the row's next index, then materialize the
   * below list only when the context menu opens or an open menu action runs.
   */
  if (
    !sessionIdsBelowSource ||
    sessionIdsBelowStartIndex === undefined ||
    sessionIdsBelowStartIndex < 0 ||
    sessionIdsBelowStartIndex >= sessionIdsBelowSource.length
  ) {
    return EMPTY_SESSION_IDS;
  }

  return sessionIdsBelowSource.slice(sessionIdsBelowStartIndex);
}

export function getSessionCardAccessibleLabel({ isFocused, title }: { isFocused: boolean; title: string }): string {
  const fallbackTitle = title.trim() || 'Session';
  return isFocused ? `${fallbackTitle}, current session` : fallbackTitle;
}

export function getSessionTagSubmenuSections({
  currentSessionTag,
  sessionTagListItems,
}: {
  currentSessionTag?: SidebarSessionTag;
  sessionTagListItems?: readonly SidebarSessionTagListItem[];
}) {
  /*
   * CDXC:Sessions 2026-06-15-22:23:
   * Session-card Tag as menus should use the same enabled-and-visible tag set
   * as sidebar tag filters so default-off tags do not keep appearing in the
   * assignment menu. Include the current tag even when it is hidden so older or
   * custom-tagged sessions can still clear their selected marker.
   */
  return getEnabledVisibleSidebarSessionTagSections(sessionTagListItems, {
    includeTags: currentSessionTag ? [currentSessionTag] : [],
  });
}

export type SidebarBulkContextMenuScheduler = (operation: () => void) => void;

type SleepBelowDebugDetailsInput = {
  clickedSessionKind?: string;
  debugInstanceId: number | string;
  elapsedSinceRequestMs?: number;
  event: 'nextFrame' | 'posted' | 'requested' | 'skipped';
  flushDurationMs?: number;
  frameDelayMs?: number;
  postMessageDurationMs?: number;
  resolveDurationMs?: number;
  skippedCount: number;
  sourceIndex: number;
  targetCount: number;
  visibleBelowCount: number;
};

type SidebarBulkSessionContextMenuAvailability = {
  closableSessionIds: string[];
  fullReloadableSessionIds: string[];
  pinnableSessionIds: string[];
  sleepableSessionIds: string[];
  taggableSessionIds: string[];
  unpinnableSessionIds: string[];
  wakeableSessionIds: string[];
};

export type SidebarSessionPointerDownFocusInput = {
  altKey: boolean;
  button: number;
  ctrlKey: boolean;
  isInteractiveDescendant?: boolean;
  isPrimary?: boolean;
  isSessionDragActivationEnabled?: boolean;
  isProjectSessionListMoreRow: boolean;
  isProjectSessionListOverflowRow: boolean;
  metaKey: boolean;
  renameSessionOnDoubleClick: boolean;
  shiftKey: boolean;
};

export function shouldFocusSidebarSessionOnPointerDown(input: SidebarSessionPointerDownFocusInput): boolean {
  /*
   * CDXC:FocusRouting 2026-06-26-06:25:
   * When Double-click session cards to rename is off, normal sidebar session
   * selection should start from the primary pointer-down so users do not wait
   * for a possible second click. Keep modified clicks, auxiliary buttons,
   * overflow placeholders, and Show more rows on their existing click-specific
   * behavior.
   *
   * CDXC:Sessions 2026-07-01-00:47:
   * Pinned project sessions remain draggable while the sidebar is sorted by Last Active. If a row can start a drag, do not focus the terminal on pointer-down; focus stealing can cancel WebKit's delayed drag stream before dnd-kit emits dragStart. Non-drag clicks still focus through the normal click handler.
   */
  return (
    !input.renameSessionOnDoubleClick &&
    input.isSessionDragActivationEnabled !== true &&
    !input.isProjectSessionListOverflowRow &&
    !input.isProjectSessionListMoreRow &&
    input.button === 0 &&
    input.metaKey === false &&
    input.ctrlKey === false &&
    input.altKey === false &&
    input.shiftKey === false &&
    input.isInteractiveDescendant !== true &&
    (input.isPrimary ?? true)
  );
}

function shouldPreventSessionCardDragActivation(
  event: {
    ctrlKey?: boolean;
    metaKey?: boolean;
    shiftKey?: boolean;
    target: EventTarget | null;
  },
  sourceElement: Element | undefined
): boolean {
  /*
   * CDXC:Sessions 2026-07-02-06:52:
   * Shift/Cmd/Ctrl pointer-downs are selection and context-menu gestures, never
   * drag starts. If the hold-delay sensor arms on them, dnd-kit captures the
   * pointer to document.body once the delay elapses, the browser retargets the
   * follow-up click there, and the row's shift/cmd selection click never fires.
   */
  if (event.shiftKey === true || event.metaKey === true || event.ctrlKey === true) {
    return true;
  }

  const targetElement = event.target instanceof Element ? event.target : undefined;
  if (!targetElement || targetElement === sourceElement) {
    return false;
  }

  /*
   * CDXC:Sessions 2026-06-30-11:33:
   * The floating pin is a button for click-to-pin, but it is also the visible drag affordance users expect for pinned-session reorder in Last Active sorting. Let DnD activate from that button while preserving default interactive blocking for close, delayed-send, links, inputs, and other child controls.
   */
  if (targetElement.closest(SESSION_CARD_PIN_DRAG_HANDLE_SELECTOR)) {
    return false;
  }

  return targetElement.closest(SESSION_CARD_DND_INTERACTIVE_SELECTOR) !== null;
}

function isSessionCardPointerFocusBlockedByDescendant({
  currentTarget,
  target,
}: {
  currentTarget: HTMLElement;
  target: EventTarget | null;
}): boolean {
  const targetNode = target instanceof Node ? target : undefined;
  const targetElement = targetNode instanceof Element ? targetNode : (targetNode?.parentElement ?? undefined);
  if (!targetElement || !currentTarget.contains(targetElement)) {
    return false;
  }

  const blockingElement = targetElement.closest(SESSION_CARD_POINTER_FOCUS_BLOCKING_SELECTOR);
  return Boolean(blockingElement && blockingElement !== currentTarget);
}

export function shouldRenameSidebarSessionOnDoubleClick({
  isBrowserSession,
  isProjectSessionListMoreRow,
  isProjectSessionListOverflowRow,
  renameSessionOnDoubleClick,
}: {
  isBrowserSession: boolean;
  isProjectSessionListMoreRow: boolean;
  isProjectSessionListOverflowRow: boolean;
  renameSessionOnDoubleClick: boolean;
}): boolean {
  /*
   * CDXC:Sessions 2026-06-26-06:25:
   * Session-card double-click is reserved for the explicit rename preference.
   * It must not enter workspace focus mode; Focus stays available as the
   * context-menu command for users who want to zoom a pane tab group.
   */
  return (
    renameSessionOnDoubleClick && !isBrowserSession && !isProjectSessionListOverflowRow && !isProjectSessionListMoreRow
  );
}

export type SidebarSessionContextMenuEligibilityInput = {
  isProjectSessionListMoreRow: boolean;
  isRemoteSession: boolean;
  session: SidebarSessionItem | undefined;
  showSessionCommandCopyActions: boolean;
  showSessionDetailsCopyAction: boolean;
};

export type SidebarSessionContextMenuEligibility = {
  canCloseAfterDone: boolean;
  canCopyAttachCommand: boolean;
  canCopyResumeCommand: boolean;
  canCopySessionDetails: boolean;
  canDelayedSend: boolean;
  canExportTranscript: boolean;
  canForkSession: boolean;
  canFullReloadSession: boolean;
  canGenerateSessionTitle: boolean;
  /**
   * CDXC:SessionNotes 2026-08-24:
   * Notes are keyed by the session's provider conversation id, so a row that
   * has not captured one yet has nothing to file a note against.
   */
  canOpenSessionNote: boolean;
  canPinSession: boolean;
  canPopOutPane: boolean;
  canRenameSession: boolean;
  canSleepSession: boolean;
  /**
   * CDXC:Workarea 2026-09-04 DECISION:
   * User: Advanced > Split Right opens the session in a pane to the right of
   * the focused agents pane, for local and remote machine rows alike. The Rust
   * workspace owns pane topology, so the item needs the GPUI bridge and is
   * hidden in the web app.
   */
  canSplitSessionRight: boolean;
  canTagSession: boolean;
  isBrowserSession: boolean;
};

export function getSidebarSessionContextMenuEligibility({
  isProjectSessionListMoreRow,
  isRemoteSession,
  session,
  showSessionCommandCopyActions,
  showSessionDetailsCopyAction,
}: SidebarSessionContextMenuEligibilityInput): SidebarSessionContextMenuEligibility {
  const isBrowserSession = isSidebarBrowserSession(session);
  const hasSession = session !== undefined;
  const isDraftSession = session?.isDraft === true;
  const isConcreteSessionRow = hasSession && !isProjectSessionListMoreRow;
  const canUseTerminalAgentMenuAction = isConcreteSessionRow && !isBrowserSession;

  /*
   * CDXC:RemoteMachines 2026-06-30-15:22:
   * Remote session rows share the local context-menu renderer, but local AppKit and host-timer actions must opt in through explicit row capabilities. Keep ordinary gxserver-backed actions visible from the remote group signal while avoiding frontend guesses for Pop Out Pane, Delayed Send, and Close After Done.
   */
  return {
    canCloseAfterDone:
      canUseTerminalAgentMenuAction && hasSession && supportsCloseAfterDoneMenuAction(session, isRemoteSession),
    canCopyAttachCommand:
      showSessionCommandCopyActions &&
      canUseTerminalAgentMenuAction &&
      Boolean(session?.sessionPersistenceProvider && session.sessionPersistenceName),
    canCopyResumeCommand:
      showSessionCommandCopyActions &&
      canUseTerminalAgentMenuAction &&
      hasSession &&
      supportsResumeCommandCopy(session),
    canCopySessionDetails: isConcreteSessionRow && showSessionDetailsCopyAction,
    canDelayedSend:
      canUseTerminalAgentMenuAction && hasSession && supportsDelayedSendMenuAction(session, isRemoteSession),
    canExportTranscript:
      canUseTerminalAgentMenuAction && hasSession && !isDraftSession && supportsTranscriptExport(session),
    /*
     * CDXC:Drafts 2026-08-28:
     * A draft has no conversation and no prompt yet, so Fork has nothing to
     * fork from and Full reload has nothing to reload into: both would only
     * ever produce an empty agent. Hide them here — the ONE resolver both the
     * V1 card menu and the V2 row menu read — so the two menus cannot disagree.
     * Rename, Sleep, Pin, Tag, and Close stay available on drafts.
     */
    canForkSession: canUseTerminalAgentMenuAction && hasSession && !isDraftSession && supportsFork(session),
    canSplitSessionRight:
      canUseTerminalAgentMenuAction && hasSession && !isDraftSession && gpuiWorkspaceTerminalFocusBridgeAvailable(),
    canFullReloadSession:
      canUseTerminalAgentMenuAction &&
      hasSession &&
      !isDraftSession &&
      supportsFullReloadMenuAction(session, isRemoteSession),
    canGenerateSessionTitle:
      canUseTerminalAgentMenuAction &&
      hasSession &&
      supportsGeneratedName(session) &&
      Boolean(session.firstUserMessage?.trim()),
    canOpenSessionNote: canUseTerminalAgentMenuAction && Boolean(session?.agentSessionId?.trim()),
    canPinSession: isConcreteSessionRow,
    canPopOutPane:
      isConcreteSessionRow &&
      hasSession &&
      supportsPopOutPaneMenuAction(session, {
        isBrowserSession,
        isRemoteSession,
      }),
    canRenameSession: canUseTerminalAgentMenuAction,
    canSleepSession: isConcreteSessionRow && (canSleepSidebarSession(session) || canWakeSidebarSession(session)),
    canTagSession: canUseTerminalAgentMenuAction,
    isBrowserSession,
  };
}

function isSidebarBrowserSession(session: SidebarSessionItem | undefined): boolean {
  return session?.sessionKind === 'browser' || session?.kind === 'browser';
}

export function createSleepBelowDebugDetails(input: SleepBelowDebugDetailsInput): Record<string, unknown> {
  /*
   * CDXC:SessionSleep 2026-06-13-12:59:
   * Sleep below lag diagnostics must prove the click path timing without writing
   * session ids, titles, paths, URLs, command text, prompts, or other user-owned
   * content. Keep this payload to counts, enum-like labels, timing, and the
   * component debug instance id.
   */
  return {
    action: 'sleepBelow',
    clickedSessionKind: input.clickedSessionKind ?? 'unknown',
    debugInstanceId: input.debugInstanceId,
    elapsedSinceRequestMs: roundSleepBelowDebugMs(input.elapsedSinceRequestMs),
    event: input.event,
    flushDurationMs: roundSleepBelowDebugMs(input.flushDurationMs),
    frameDelayMs: roundSleepBelowDebugMs(input.frameDelayMs),
    postMessageDurationMs: roundSleepBelowDebugMs(input.postMessageDurationMs),
    resolveDurationMs: roundSleepBelowDebugMs(input.resolveDurationMs),
    skippedCount: input.skippedCount,
    sourceIndex: input.sourceIndex,
    targetCount: input.targetCount,
    visibleBelowCount: input.visibleBelowCount,
  };
}

function roundSleepBelowDebugMs(value: number | undefined): number | undefined {
  return typeof value === 'number' && Number.isFinite(value) ? Math.round(value * 10) / 10 : undefined;
}

/**
 * CDXC:ContextMenus 2026-06-07-13:00:
 * Sleep below and Close below can fan out to many native lifecycle messages.
 * Run those targets from scheduled background tasks so the menu click returns
 * immediately, the context menu can dismiss before lifecycle work starts, and
 * the sidebar remains responsive between target operations.
 *
 * CDXC:ContextMenus 2026-06-07-13:09:
 * Bulk lifecycle menu items should still close like normal menu actions. Only
 * the fan-out work runs in the background; menu visibility should not be used
 * as an operation progress indicator.
 */
export function runSidebarBulkContextMenuActionInBackground(
  sessionIds: readonly string[],
  runForSessionId: (sessionId: string) => void,
  scheduler: SidebarBulkContextMenuScheduler = (operation) => {
    globalThis.setTimeout(operation, 0);
  }
): void {
  const pendingSessionIds = [...sessionIds];
  const runNext = () => {
    const nextSessionId = pendingSessionIds.shift();
    if (!nextSessionId) {
      return;
    }

    runForSessionId(nextSessionId);
    if (pendingSessionIds.length > 0) {
      scheduler(runNext);
    }
  };

  if (pendingSessionIds.length > 0) {
    scheduler(runNext);
  }
}

function postSidebarSessionCloseInBackground(vscode: WebviewApi, sessionId: string): void {
  /*
  CDXC:StateSync 2026-06-12-06:22:
  Native sidebar message delivery is synchronous in the macOS host. Close clicks must flush the local card removal before asking the host to tear down the terminal or browser runtime, otherwise closeTerminal work can block the same user gesture and make the sidebar feel delayed.
  */
  globalThis.setTimeout(() => {
    vscode.postMessage({
      sessionId,
      type: 'closeSession',
    });
  }, 0);
}

function suppressCloseDrivenFocusedSessionScroll(sessionIds: readonly string[]): void {
  const store = useSidebarStore.getState();
  if (!sessionIds.some((sessionId) => store.sessionsById[sessionId]?.isFocused === true)) {
    return;
  }

  store.suppressNextFocusedSessionScroll('sessionClose');
}

function postSidebarSessionsCloseInBackground(vscode: WebviewApi, sessionIds: readonly string[]): void {
  globalThis.setTimeout(() => {
    vscode.postMessage({
      sessionIds: [...sessionIds],
      type: 'closeSessions',
    });
  }, 0);
}

function clampContextMenuPosition(
  clientX: number | undefined,
  clientY: number,
  itemCount: number,
  dividerCount: number,
  labelCount: number
): ContextMenuPosition {
  const menuHeight =
    CONTEXT_MENU_VERTICAL_PADDING_PX +
    itemCount * CONTEXT_MENU_ITEM_HEIGHT_PX +
    dividerCount * CONTEXT_MENU_DIVIDER_HEIGHT_PX +
    labelCount * CONTEXT_MENU_GROUP_LABEL_HEIGHT_PX;
  return {
    /*
     * CDXC:ContextMenus 2026-07-21:
     * At the GPUI sidebar's 235px default width and above, session context
     * menus follow the right-click x coordinate just like section, project,
     * and collection menus. The portal still clamps the rendered menu inside
     * the real sidebar viewport. Narrow sidebars and keyboard menu gestures
     * keep the centered placement because they have no usable pointer anchor.
     */
    x:
      clientX !== undefined && window.innerWidth >= POINTER_ALIGNED_CONTEXT_MENU_MIN_SIDEBAR_WIDTH_PX
        ? Math.max(
            CONTEXT_MENU_MARGIN_PX,
            Math.min(clientX, window.innerWidth - CONTEXT_MENU_WIDTH_PX - CONTEXT_MENU_MARGIN_PX)
          )
        : getCenteredSidebarMenuX(CONTEXT_MENU_WIDTH_PX),
    y: Math.max(CONTEXT_MENU_MARGIN_PX, Math.min(clientY, window.innerHeight - menuHeight - CONTEXT_MENU_MARGIN_PX)),
  };
}

function getCenteredSidebarMenuX(menuWidth: number): number {
  /*
   * CDXC:ContextMenus 2026-06-05-21:23:
   * Session context menus and the Tag as submenu should be horizontally
   * centered in the sidebar webview rather than opening at the pointer x.
   * Clamp against the sidebar viewport so narrow sidebars cap menu width at
   * the available sidebar width instead of overflowing into native surfaces.
   */
  const availableWidth = Math.max(0, window.innerWidth - CONTEXT_MENU_MARGIN_PX * 2);
  const renderedWidth = Math.min(menuWidth, availableWidth);
  return Math.max(CONTEXT_MENU_MARGIN_PX, (window.innerWidth - renderedWidth) / 2);
}

export function SortableSessionCard({
  completionFlashNonce = 0,
  dragDisabled = false,
  dropDisabled = dragDisabled,
  forcedDropPosition,
  groupId,
  index,
  isProjectSessionListOverflowRow = false,
  isSearchSelected = false,
  onFocusRequested,
  onSessionSelectionChange,
  projectSessionListMoreRow,
  sessionCardSettings,
  sessionGroup,
  sessionTagListItems,
  sessionIdsBelowSource,
  sessionIdsBelowStartIndex,
  sessionId,
  selectedSessionIds = EMPTY_SESSION_IDS,
  shouldKeepLastProjectSessionVisibleOnClose = false,
  showGroupDropTargetChrome = true,
  showGroupConnector = false,
  showDropPositionIndicator = true,
  vscode,
}: SortableSessionCardProps) {
  const sidebarItemTooltipDelayMs = useSidebarItemTooltipDelayMs();
  const [contextMenuPosition, setContextMenuPosition] = useState<ContextMenuPosition>();
  const [contextMenuSessionIdsBelow, setContextMenuSessionIdsBelow] = useState<readonly string[]>(EMPTY_SESSION_IDS);
  const [contextMenuSleepableSessionIdsBelow, setContextMenuSleepableSessionIdsBelow] =
    useState<readonly string[]>(EMPTY_SESSION_IDS);
  const [contextMenuSelectedSessionIds, setContextMenuSelectedSessionIds] =
    useState<readonly string[]>(EMPTY_SESSION_IDS);
  const effectiveSessionIdsBelow = contextMenuPosition ? contextMenuSessionIdsBelow : EMPTY_SESSION_IDS;
  const sleepableSessionIdsBelow = contextMenuPosition ? contextMenuSleepableSessionIdsBelow : EMPTY_SESSION_IDS;
  const effectiveSelectedSessionIds = contextMenuPosition ? contextMenuSelectedSessionIds : EMPTY_SESSION_IDS;
  const isBulkContextMenu = effectiveSelectedSessionIds.length > 1;
  const storedSession = useSidebarStore((state) => state.sessionsById[sessionId]);
  const isProjectSessionListMoreRow = projectSessionListMoreRow !== undefined;
  const isMultiSelected = selectedSessionIds.includes(sessionId);
  const projectSessionListMoreLabel = isProjectSessionListMoreRow
    ? `Show ${projectSessionListMoreRow.count} more`
    : undefined;
  const session: SidebarSessionItem | undefined =
    storedSession ??
    (isProjectSessionListMoreRow
      ? {
          activity: 'idle',
          alias: projectSessionListMoreLabel ?? 'Show more',
          column: 0,
          displayTitle: projectSessionListMoreLabel ?? 'Show more',
          isFocused: false,
          isLive: false,
          isRunning: false,
          isVisible: false,
          lifecycleState: 'done',
          nativePaneState: 'unmounted',
          providerSessionState: 'missing',
          row: 0,
          sessionId,
          sessionKind: 'terminal',
          shortcutLabel: '',
        }
      : undefined);
  const {
    enableSessionParking,
    hideSessionAgentIconUntilHover,
    hideBrowserFaviconUntilHover,
    renameSessionOnDoubleClick,
    showCloseButton,
    showDebugSessionNumbers,
    showLastActiveTime,
    showSessionCloseContextMenuAction,
    showSessionCommandCopyActions,
    showSessionDetailsCopyAction,
  } = sessionCardSettings;
  const canFocusMode = sessionGroup?.canFocusMode === true;
  const [tagSubmenuPosition, setTagSubmenuPosition] = useState<ContextMenuPosition>();
  const [advancedSubmenuPosition, setAdvancedSubmenuPosition] = useState<ContextMenuPosition>();
  const [switchAccountSubmenuPosition, setSwitchAccountSubmenuPosition] = useState<ContextMenuPosition>();
  const [completionFlashRunId, setCompletionFlashRunId] = useState(0);
  const menuRef = useRef<HTMLDivElement>(null);
  const aliasHeadingRef = useRef<HTMLDivElement>(null);
  const sessionFrameRef = useRef<HTMLDivElement | null>(null);
  const sessionCardRef = useRef<HTMLElement | null>(null);
  const debugInstanceIdRef = useRef(createSidebarDebugInstanceId());
  const lastAgentIconRenderDebugKeyRef = useRef<string | undefined>(undefined);
  const immediateFocusClickSuppressionRef = useRef<{ sessionId: string; timeoutId: number } | undefined>(undefined);
  /*
  CDXC:RemoteMachines 2026-06-30-00:11:
  Remote session rows need visible lifecycle chrome and a non-debug state tooltip while keeping local session cards unchanged. Derive that from the owning group so the session model does not need a separate remote-only flag.
  */
  const isRemoteSession = Boolean(sessionGroup?.remoteMachineContext);
  /*
  CDXC:RemoteMachines 2026-07-12:
  Stale groups show the last-seen state of a disconnected remote machine.
  Terminal/agent rows have no reachable backing session, so focus clicks are
  inert; browser rows stay clickable because their tabs are local CEF panes.
  */
  const isStaleRemoteRow = sessionGroup?.isStale === true && session?.sessionKind !== 'browser';
  const {
    canCloseAfterDone,
    canCopyAttachCommand,
    canCopyResumeCommand,
    canCopySessionDetails,
    canDelayedSend,
    canExportTranscript,
    canForkSession,
    canFullReloadSession,
    canGenerateSessionTitle,
    canOpenSessionNote,
    canPinSession,
    canRenameSession,
    canSleepSession,
    canSplitSessionRight,
    canTagSession,
    isBrowserSession,
  } = getSidebarSessionContextMenuEligibility({
    isProjectSessionListMoreRow,
    isRemoteSession,
    session,
    showSessionCommandCopyActions,
    showSessionDetailsCopyAction,
  });
  const postSessionDragDebugLog = useEffectEvent((event: string, details: Record<string, unknown>) => {
    if (!showDebugSessionNumbers || isProjectSessionListMoreRow) {
      return;
    }

    vscode.postMessage({
      details: {
        debugInstanceId: debugInstanceIdRef.current,
        groupId,
        index,
        sessionId,
        ...details,
      },
      event,
      scenarioId: 'native.pane.reorder',
      type: 'sidebarDebugLog',
    });
  });
  const postMultiSelectDebugLog = useEffectEvent((event: string, details: Record<string, unknown>) => {
    /*
     * CDXC:Sessions 2026-07-02-07:32:
     * Shift/Cmd selection repros need per-gesture breadcrumbs that persist
     * regardless of the sidebar Debugging Mode toggle. Post unconditionally;
     * the native.sidebar.refresh diagnostic scenario (Settings > Diagnostic
     * logging > "Sidebar refresh and hydration") is the single persist gate,
     * and payloads stay limited to ids, indexes, counts, and booleans.
     */
    vscode.postMessage({
      details: {
        groupId,
        index,
        selectedCount: selectedSessionIds.length,
        sessionId,
        ...details,
      },
      event: `repro.sidebarMultiSelect.${event}`,
      scenarioId: 'native.sidebar.refresh',
      type: 'sidebarDebugLog',
    });
  });
  const sortable = useSortable({
    accept: 'session',
    data: createSessionDragData(groupId, session?.sessionId ?? sessionId),
    disabled:
      isProjectSessionListMoreRow ||
      isProjectSessionListOverflowRow ||
      dragDisabled ||
      isBrowserSession ||
      contextMenuPosition !== undefined,
    feedback: 'clone',
    group: groupId,
    id: sessionId,
    index,
    modifiers: sessionCardModifiers,
    plugins: [SortableKeyboardPlugin],
    sensors: sessionCardSensors,
    type: 'session',
  });
  const isSessionReorderDisabled =
    isProjectSessionListMoreRow ||
    isProjectSessionListOverflowRow ||
    !session ||
    dropDisabled ||
    contextMenuPosition !== undefined;
  const beforeDropTarget = useDroppable({
    accept: 'session',
    data: createSessionDropTargetData({
      groupId,
      kind: 'session',
      position: 'before',
      sessionId,
    }),
    disabled: isSessionReorderDisabled,
    id: createSessionDropTargetId({
      groupId,
      kind: 'session',
      position: 'before',
      sessionId,
    }),
  });
  const afterDropTarget = useDroppable({
    accept: 'session',
    data: createSessionDropTargetData({
      groupId,
      kind: 'session',
      position: 'after',
      sessionId,
    }),
    disabled: isSessionReorderDisabled,
    id: createSessionDropTargetId({
      groupId,
      kind: 'session',
      position: 'after',
      sessionId,
    }),
  });
  /*
   * CDXC:Sidebar 2026-07-02-13:05:
   * The pointer-resolved indicator owned by sidebar-app is the only visual
   * source. Falling back to dnd-kit's rect-overlap isDropTarget state drew a
   * second line that disagreed with the pointer and flickered; the droppable
   * halves above remain registered purely as dnd-kit drop targets.
   */
  const dropPosition = sortable.isDragging ? undefined : forcedDropPosition;
  const visibleDropPosition = showDropPositionIndicator ? dropPosition : undefined;
  const isVisibleDropTarget = showDropPositionIndicator && Boolean(visibleDropPosition);
  const shouldShowGroupDropTargetChrome = showGroupDropTargetChrome && isVisibleDropTarget;

  if (!session) {
    return null;
  }

  const currentSessionTag = getEffectiveSessionTag(session);
  const sidebarSessionsByIdForMenu = useSidebarStore.getState().sessionsById;
  const bulkActionAvailability = isBulkContextMenu
    ? getSidebarBulkSessionContextMenuAvailability({
        sessionIds: effectiveSelectedSessionIds,
        sessionsById: sidebarSessionsByIdForMenu,
      })
    : undefined;
  const contextMenuSessionTag = bulkActionAvailability
    ? getSharedSelectedSidebarSessionTag({
        sessionIds: bulkActionAvailability.taggableSessionIds,
        sessionsById: sidebarSessionsByIdForMenu,
      })
    : currentSessionTag;
  const sessionTagSubmenuSections = getSessionTagSubmenuSections({
    currentSessionTag: contextMenuSessionTag,
    sessionTagListItems,
  });
  const sessionTagSubmenuItemCount = sessionTagSubmenuSections.reduce(
    (count, section) => count + section.options.length,
    0
  );
  const sessionTitleTooltip = getSessionCardTitleTooltip({
    alwaysShowStateTooltip: isRemoteSession,
    session,
    showDebugSessionNumbers,
  });
  const sessionAccessibleLabel = getSessionCardAccessibleLabel({
    isFocused: session.isFocused,
    title: sessionTitleTooltip.headingText,
  });
  const lifecycleState = getSidebarSessionLifecycleState(session);
  const showTerminalSessionIcon = shouldShowTerminalSessionIcon(session);
  const hasSessionTimerIcon = Boolean(
    session.delayedSendRemainingLabel ||
    session.delayedSendDeadlineAt ||
    session.closeAfterDone ||
    session.closeAfterDoneRemainingLabel ||
    session.closeAfterDoneDeadlineAt
  );
  const hasSessionCardIcon =
    !isProjectSessionListMoreRow &&
    (session.isPinned === true ||
      Boolean(currentSessionTag) ||
      hasSessionTimerIcon ||
      Boolean(session.agentIcon) ||
      showTerminalSessionIcon ||
      session.isReloading === true);
  const sessionAnchorStyle = isProjectSessionListMoreRow
    ? undefined
    : ({
        anchorName: getSessionStatusAnchorName(sessionId),
      } as CSSProperties);
  const isSessionDragActivationEnabled =
    !isProjectSessionListMoreRow &&
    !isProjectSessionListOverflowRow &&
    !dragDisabled &&
    !isBrowserSession &&
    contextMenuPosition === undefined;
  const setSessionFrameElement = useCallback(
    (element: HTMLDivElement | null) => {
      sessionFrameRef.current = element;
      sortable.ref(element);
    },
    [sortable]
  );
  const setSessionCardElement = useCallback(
    (element: HTMLElement | null) => {
      sessionCardRef.current = element;
      sortable.sourceRef(element);
    },
    [sortable]
  );

  useEffect(() => {
    setContextMenuPosition(undefined);
    setTagSubmenuPosition(undefined);
    setAdvancedSubmenuPosition(undefined);
    setSwitchAccountSubmenuPosition(undefined);
  }, [session.alias, session.sessionId]);

  useEffect(() => {
    return () => {
      if (immediateFocusClickSuppressionRef.current) {
        window.clearTimeout(immediateFocusClickSuppressionRef.current.timeoutId);
      }
    };
  }, []);

  useEffect(() => {
    const targets: Array<{ attributes: readonly string[]; element: HTMLElement }> = [];
    if (sessionFrameRef.current) {
      targets.push({
        attributes: DND_SESSION_FRAME_AX_ATTRIBUTES,
        element: sessionFrameRef.current,
      });
    }
    if (sessionCardRef.current) {
      targets.push({
        attributes: DND_SESSION_CARD_AX_ATTRIBUTES,
        element: sessionCardRef.current,
      });
    }
    if (targets.length === 0) {
      return;
    }

    const scrubDndAccessibilityAttributes = (target: { attributes: readonly string[]; element: HTMLElement }) => {
      for (const attribute of target.attributes) {
        target.element.removeAttribute(attribute);
      }
    };

    for (const target of targets) {
      scrubDndAccessibilityAttributes(target);
    }

    const observers = targets.map((target) => {
      const observer = new MutationObserver((mutations) => {
        if (
          mutations.some(
            (mutation) =>
              mutation.type === 'attributes' &&
              mutation.attributeName !== null &&
              target.attributes.includes(mutation.attributeName)
          )
        ) {
          window.queueMicrotask(() => scrubDndAccessibilityAttributes(target));
        }
      });

      observer.observe(target.element, {
        attributeFilter: [...target.attributes],
        attributes: true,
      });

      return observer;
    });

    return () => {
      for (const observer of observers) {
        observer.disconnect();
      }
    };
  }, [sessionAccessibleLabel]);

  useEffect(() => {
    if (completionFlashNonce <= 0) {
      return;
    }

    setCompletionFlashRunId(completionFlashNonce);
  }, [completionFlashNonce]);

  useEffect(() => {
    if (completionFlashRunId <= 0) {
      return;
    }

    const timeout = window.setTimeout(() => {
      setCompletionFlashRunId((previous) => (previous === completionFlashRunId ? 0 : previous));
    }, COMPLETION_FLASH_DURATION_MS);

    return () => {
      window.clearTimeout(timeout);
    };
  }, [completionFlashRunId]);

  useEffect(() => {
    postSessionDragDebugLog('session.cardMounted', {
      dropPosition,
      isBrowserSession,
    });

    return () => {
      postSessionDragDebugLog('session.cardUnmounted', {
        dropPosition,
        isBrowserSession,
      });
    };
  }, [isBrowserSession, postSessionDragDebugLog]);

  useEffect(() => {
    if (sortable.isDragging) {
      /*
       * CDXC:Sessions 2026-07-02-07:32:
       * A drag activation during a selection gesture captures the pointer to
       * document.body and retargets the follow-up click away from this row, so
       * the selection click never fires. Persist every drag start so a repro
       * log can prove whether a "lost" shift/cmd click was swallowed by drag.
       */
      postMultiSelectDebugLog('dragStarted', {});
    }
  }, [postMultiSelectDebugLog, sortable.isDragging]);

  useEffect(() => {
    postSessionDragDebugLog('session.dropPositionChanged', {
      dropPosition,
      isDragging: sortable.isDragging,
      isDropTarget: sortable.isDropTarget,
    });
  }, [dropPosition, postSessionDragDebugLog, sortable.isDragging, sortable.isDropTarget]);

  useEffect(() => {
    if (!hasSessionCardIcon) {
      return;
    }

    const hasLastInteractionLabel = showLastActiveTime && Boolean(session.lastInteractionAt);
    const showHeaderLoadingSpinner = session.isReloading === true || session.isGeneratingFirstPromptTitle === true;
    const hasHeaderAgentIcon = Boolean(session.agentIcon) || showTerminalSessionIcon || showHeaderLoadingSpinner;
    const defaultTrailingDisplay = hasHeaderAgentIcon ? 'icon' : hasLastInteractionLabel ? 'time' : 'icon';
    const shouldKeepLoadingIconVisible = showHeaderLoadingSpinner && hasHeaderAgentIcon;
    const hoverTrailingDisplay = shouldKeepLoadingIconVisible
      ? 'icon'
      : defaultTrailingDisplay === 'icon'
        ? hasLastInteractionLabel
          ? 'time'
          : 'icon'
        : hasHeaderAgentIcon
          ? 'icon'
          : 'time';
    const debugKey = JSON.stringify({
      agentIcon: session.agentIcon,
      defaultTrailingDisplay,
      hasHeaderAgentIcon,
      hasLastInteractionLabel,
      hoverTrailingDisplay,
      isGeneratingFirstPromptTitle: session.isGeneratingFirstPromptTitle === true,
      isReloading: session.isReloading === true,
      primaryTitle: session.primaryTitle,
      sessionId: session.sessionId,
      showTerminalSessionIcon,
      terminalTitle: session.terminalTitle,
    });
    if (lastAgentIconRenderDebugKeyRef.current === debugKey) {
      return;
    }
    lastAgentIconRenderDebugKeyRef.current = debugKey;

    /*
     * CDXC:AgentProviders 2026-04-27-07:43
     * Agent identity is confirmed at the native/webview/store boundary. Log
     * the card render decision and actual DOM state so missing sidebar icons
     * can be traced without guessing at CSS or projection state.
     */
    postSidebarAgentIconRenderDebugLog(vscode, 'sidebar.agentIcon.cardRenderState', {
      agentIcon: session.agentIcon,
      defaultTrailingDisplay,
      groupId,
      hasHeaderAgentIcon,
      hasLastInteractionLabel,
      hoverTrailingDisplay,
      isGeneratingFirstPromptTitle: session.isGeneratingFirstPromptTitle === true,
      isReloading: session.isReloading === true,
      primaryTitle: session.primaryTitle,
      sessionActivity: session.activity,
      sessionId: session.sessionId,
      sessionKind: session.sessionKind,
      terminalTitle: session.terminalTitle,
    });

    const animationFrame = window.requestAnimationFrame(() => {
      const card = findSessionCardElement(session.sessionId);
      const frame = card?.closest<HTMLElement>('.session-frame');
      const trailing = card?.querySelector<HTMLElement>('.session-head-trailing');
      const headerIcon = card?.querySelector<HTMLElement>(
        '.session-header-agent-icon, .session-header-agent-tabler-icon, .session-header-reloading-icon'
      );
      const floatingIcon = frame?.querySelector<HTMLElement>(
        '.session-floating-agent-icon, .session-floating-agent-tabler-icon, .session-floating-reloading-icon'
      );

      postSidebarAgentIconRenderDebugLog(vscode, 'sidebar.agentIcon.cardDomState', {
        agentIcon: session.agentIcon,
        card: summarizeAgentIconElement(card),
        defaultTrailingDisplay,
        floatingIcon: summarizeAgentIconElement(floatingIcon),
        frame: summarizeAgentIconElement(frame),
        groupId,
        hasCardElement: Boolean(card),
        hasFloatingIconElement: Boolean(floatingIcon),
        hasHeaderIconElement: Boolean(headerIcon),
        headerIcon: summarizeAgentIconElement(headerIcon),
        hoverTrailingDisplay,
        sessionId: session.sessionId,
        trailing: summarizeAgentIconElement(trailing),
      });
    });

    return () => {
      window.cancelAnimationFrame(animationFrame);
    };
  }, [
    groupId,
    hasSessionCardIcon,
    session.activity,
    session.agentIcon,
    session.isGeneratingFirstPromptTitle,
    session.isReloading,
    session.lastInteractionAt,
    session.primaryTitle,
    session.sessionId,
    session.sessionKind,
    session.terminalTitle,
    showLastActiveTime,
    showTerminalSessionIcon,
    vscode,
  ]);

  const readLatestSessionIdsBelow = () =>
    contextMenuPosition
      ? contextMenuSessionIdsBelow
      : resolveSessionCardSessionIdsBelow({
          sessionIdsBelowSource,
          sessionIdsBelowStartIndex,
        });

  const getSleepableSessionIds = (candidateSessionIds: readonly string[]) =>
    candidateSessionIds.filter((candidateSessionId) =>
      canSleepSidebarSession(useSidebarStore.getState().sessionsById[candidateSessionId])
    );

  const getContextMenuCountsForSessionIdsBelow = (nextSessionIdsBelow: readonly string[]) => {
    const nextSleepableSessionIdsBelow = getSleepableSessionIds(nextSessionIdsBelow);
    const nextBelowActionCount =
      nextSessionIdsBelow.length > 0 ? 1 + Number(nextSleepableSessionIdsBelow.length > 0) : 0;
    const nextPrimaryCount =
      Number(canRenameSession) +
      Number(canSleepSession) +
      Number(canPinSession) +
      Number(enableSessionParking && canPinSession && !isBrowserSession) +
      Number(canOpenSessionNote) +
      Number(canTagSession && sessionTagSubmenuItemCount > 0);
    const nextAdvancedNestedCount =
      Number(canDelayedSend) +
      Number(canCloseAfterDone) +
      Number(canFullReloadSession) +
      Number(canForkSession) +
      Number(canExportTranscript) +
      Number(Boolean(session.firstUserMessage?.trim())) +
      Number(canGenerateSessionTitle) +
      Number(sessionGroup?.canCreateSessionGroup === true) +
      Number(canSplitSessionRight) +
      Number(canFocusMode) +
      Number(canCopySessionDetails) +
      Number(canCopyResumeCommand) +
      Number(canCopyAttachCommand) +
      nextBelowActionCount;
    const nextSectionLengths = [
      nextPrimaryCount,
      Number(nextAdvancedNestedCount > 0),
      Number(showSessionCloseContextMenuAction),
    ].filter((count) => count > 0);
    return {
      dividerCount: Math.max(0, nextSectionLengths.length - 1),
      itemCount: nextSectionLengths.reduce((count, sectionLength) => count + sectionLength, 0),
      labelCount: 0,
      sleepableSessionIdsBelow: nextSleepableSessionIdsBelow,
    };
  };

  const readSelectedSessionIdsForContextMenu = () => {
    const sessionsById = useSidebarStore.getState().sessionsById;
    const nextSelectedSessionIds: string[] = [];
    const seenSessionIds = new Set<string>();
    for (const selectedSessionId of selectedSessionIds) {
      if (seenSessionIds.has(selectedSessionId) || !sessionsById[selectedSessionId]) {
        continue;
      }
      seenSessionIds.add(selectedSessionId);
      nextSelectedSessionIds.push(selectedSessionId);
    }
    return nextSelectedSessionIds;
  };

  const openContextMenu = (clientY: number, clientX?: number) => {
    const nextSelectedSessionIds = readSelectedSessionIdsForContextMenu();
    /*
     * CDXC:Sessions 2026-07-01-18:33:
     * Right-clicking one of several selected sessions should open a bulk action
     * menu for exactly that selected set. Right-clicking outside the selected set
     * returns to the normal single-session menu and clears the transient selection.
     */
    const shouldOpenBulkContextMenu =
      nextSelectedSessionIds.length > 1 && nextSelectedSessionIds.includes(session.sessionId);
    const nextSessionIdsBelow = shouldOpenBulkContextMenu ? EMPTY_SESSION_IDS : readLatestSessionIdsBelow();
    let nextSleepableSessionIdsBelow: readonly string[] = EMPTY_SESSION_IDS;
    const nextMenuCounts = shouldOpenBulkContextMenu
      ? getSidebarBulkSessionContextMenuCounts({
          availability: getSidebarBulkSessionContextMenuAvailability({
            sessionIds: nextSelectedSessionIds,
            sessionsById: useSidebarStore.getState().sessionsById,
          }),
          hasSessionTagSubmenu: sessionTagSubmenuItemCount > 0,
        })
      : (() => {
          const belowMenuCounts = getContextMenuCountsForSessionIdsBelow(nextSessionIdsBelow);
          nextSleepableSessionIdsBelow = belowMenuCounts.sleepableSessionIdsBelow;
          return belowMenuCounts;
        })();
    setTagSubmenuPosition(undefined);
    setAdvancedSubmenuPosition(undefined);
    setSwitchAccountSubmenuPosition(undefined);
    setContextMenuSessionIdsBelow(nextSessionIdsBelow);
    setContextMenuSleepableSessionIdsBelow(nextSleepableSessionIdsBelow);
    setContextMenuSelectedSessionIds(shouldOpenBulkContextMenu ? nextSelectedSessionIds : EMPTY_SESSION_IDS);
    if (selectedSessionIds.length > 0) {
      postMultiSelectDebugLog('contextMenu', {
        isBulk: shouldOpenBulkContextMenu,
        resolvedSelectedCount: nextSelectedSessionIds.length,
        willClearSelection: !shouldOpenBulkContextMenu,
      });
    }
    if (!shouldOpenBulkContextMenu && selectedSessionIds.length > 0) {
      onSessionSelectionChange?.({
        groupId,
        mode: 'clear',
        reason: 'contextMenuOutsideSelection',
        sessionId: session.sessionId,
      });
    }
    setContextMenuPosition(
      clampContextMenuPosition(
        clientX,
        clientY,
        nextMenuCounts.itemCount,
        nextMenuCounts.dividerCount,
        nextMenuCounts.labelCount
      )
    );
  };

  const requestRename = () => {
    if (isBrowserSession) {
      return;
    }

    setContextMenuPosition(undefined);
    /**
     * CDXC:AppModal 2026-04-27-14:25
     * Rename must always use the full-window modal host. Missing host is an
     * error, not a reason to show the old squeezed sidebar dialog.
     *
     * CDXC:Settings 2026-06-15-14:07:
     * Opening Rename from a sidebar session row should replace any open
     * Settings workspace modal instead of stacking the rename flow behind it.
     */
    closeAppModal('SettingsDismissal:sessionRowRename');
    openAppModal({
      initialTitle: getSessionRenameInitialTitle(session),
      modal: 'renameSession',
      sessionAgentIcon: session.agentIcon,
      sessionId: session.sessionId,
      type: 'open',
    });
  };

  const requestOpenSessionNote = () => {
    if (!canOpenSessionNote) {
      return;
    }

    setContextMenuPosition(undefined);
    /**
     * CDXC:SessionNotes 2026-08-24:
     * Same modal-host contract as Rename: the note editor is a full-window app
     * modal, and opening it from a row replaces any modal already open instead
     * of stacking behind it. The row hands over the note it is currently
     * rendering so the dialog opens filled in without a round trip; the host
     * resolves which agent conversation the write is filed under.
     */
    closeAppModal('SettingsDismissal:sessionRowNote');
    openAppModal({
      initialNote: session.sessionNote ?? '',
      modal: 'sessionNote',
      sessionId: session.sessionId,
      sessionTitle: getSessionRenameInitialTitle(session),
      type: 'open',
    });
  };

  const requestClose = (_source: 'context-menu' | 'middle-click' | 'programmatic') => {
    flushSync(() => {
      setContextMenuPosition(undefined);
      if (shouldKeepLastProjectSessionVisibleOnClose) {
        /*
        CDXC:StateSync 2026-06-01-20:52:
        Closing a project's final sidebar session parks it instead of removing it. Keep the card visible immediately and mark it sleeping locally so the project does not blink out before gxserver publishes the parked-session presentation.
        */
        useSidebarStore.getState().setSessionSleepingLocally(session.sessionId, true);
      } else {
        suppressCloseDrivenFocusedSessionScroll([session.sessionId]);
        useSidebarStore.getState().hideSessionLocally(session.sessionId);
      }
    });
    postSidebarSessionCloseInBackground(vscode, session.sessionId);
  };

  const requestCopyResumeCommand = () => {
    setContextMenuPosition(undefined);
    vscode.postMessage({
      sessionId: session.sessionId,
      type: 'copyResumeCommand',
    });
  };

  const requestCopyAttachCommand = () => {
    setContextMenuPosition(undefined);
    /**
     * CDXC:Workarea 2026-05-07-20:32
     * Provider-backed tmux/zmx/zellij session cards expose the native attach
     * command alongside resume copying, using the stored provider/name pair
     * rather than the current global Settings provider.
     */
    vscode.postMessage({
      sessionId: session.sessionId,
      type: 'copyAttachCommand',
    });
  };

  const requestCopySessionDetails = () => {
    setContextMenuPosition(undefined);
    vscode.postMessage({
      detailsText: buildSidebarSessionDetailsClipboardText(session, sessionGroup),
      sessionId: session.sessionId,
      type: 'copySessionDetails',
    });
  };

  const requestForkSession = () => {
    setContextMenuPosition(undefined);
    vscode.postMessage({
      sessionId: session.sessionId,
      type: 'forkSession',
    });
  };

  const requestExportTranscript = () => {
    setContextMenuPosition(undefined);
    vscode.postMessage({
      sessionId: session.sessionId,
      type: 'exportSessionTranscript',
    });
  };

  const requestFocusMode = () => {
    setContextMenuPosition(undefined);
    /**
     * CDXC:FocusMode 2026-05-23-09:28:
     * Context-menu Focus should zoom the clicked session's pane tab group. Route
     * through the controller so it can switch to Agents mode and later restore
     * the prior Code/Browser/Project/Manage surface on unfocus.
     *
     * CDXC:FocusMode 2026-06-26-06:25:
     * Sidebar session double-click no longer enters focus mode. Keep this
     * command behind the explicit Focus menu item so row double-click remains
     * available for rename when that preference is enabled.
     */
    vscode.postMessage({
      sessionId: session.sessionId,
      type: 'focusSessionMode',
    });
  };

  const requestDelayedSend = () => {
    if (!canDelayedSend) {
      return;
    }

    setContextMenuPosition(undefined);
    /**
     * CDXC:DelayedSend 2026-05-11-11:56
     * Terminal session context menus mirror the native title-bar clock action:
     * open the full-window timer modal and let native press Enter later for
     * the command text already staged in that terminal.
     */
    openAppModal({
      agentIcon: session.agentIcon,
      closeAfterDoneActive: session.closeAfterDone === true,
      delayedSendDeadlineAt: session.delayedSendDeadlineAt,
      delayedSendRemainingLabel: session.delayedSendRemainingLabel,
      modal: 'delayedSend',
      sendWhenAllProjectSessionsStopActive: session.sendWhenAllProjectSessionsStopActive === true,
      sendWhenAgentStopsActive: session.sendWhenAgentStopsActive === true,
      sessionId: session.sessionId,
      supportsSendWhenAgentStops: true,
      supportsSendWhenAllProjectSessionsStop: true,
      title: getSessionRenameInitialTitle(session),
      type: 'open',
    });
  };

  const requestToggleCloseAfterDone = () => {
    if (!canCloseAfterDone) {
      return;
    }

    setContextMenuPosition(undefined);
    /**
     * CDXC:Sessions 2026-06-15-21:00:
     * Session context menus expose Close After Done directly below Delayed
     * Send. The sidebar sends only the session id; native owns the armed flag,
     * the continuous three-minute Done timer, and the final close behavior.
     */
    vscode.postMessage({
      sessionId: session.sessionId,
      type: 'toggleCloseAfterDone',
    });
  };

  const requestGenerateSessionTitle = () => {
    const firstMessage = session.firstUserMessage?.trim();
    if (!firstMessage) {
      return;
    }

    setContextMenuPosition(undefined);
    /**
     * CDXC:SessionTitles 2026-05-08-10:54
     * Generate Title must summarize the captured 1st user message through the
     * normal renameSession flow. That controller path already owns Codex title
     * generation, Agent CLI sync, and the "Generating title..." card loading
     * state, so the sidebar must send the first message as the rename input
     * instead of posting a separate generateSessionName command.
     */
    vscode.postMessage({
      details: {
        agentIcon: session.agentIcon,
        firstUserMessageLength: firstMessage.length,
        isGeneratingFirstPromptTitle: session.isGeneratingFirstPromptTitle === true,
        primaryTitle: session.primaryTitle,
        sessionId: session.sessionId,
        terminalTitle: session.terminalTitle,
      },
      event: 'session.generateTitle.clicked',
      scenarioId: 'native.session.title',
      type: 'sidebarDebugLog',
    });
    vscode.postMessage({
      sessionId: session.sessionId,
      shouldGenerateTitle: true,
      title: firstMessage,
      type: 'renameSession',
    });
  };

  const requestFullReloadSession = () => {
    setContextMenuPosition(undefined);
    vscode.postMessage({
      sessionId: session.sessionId,
      type: 'fullReloadSession',
    });
  };

  const requestSplitSessionRight = () => {
    setContextMenuPosition(undefined);
    setAdvancedSubmenuPosition(undefined);
    vscode.postMessage({
      sessionId: session.sessionId,
      type: 'splitSessionRight',
    });
  };

  /*
  CDXC:AgentProviders 2026-09-03:
  Advanced > Switch Account opens a third-level submenu of the daemon-resolved
  same-family accounts (`session.switchableAgents`); picking one asks the
  runtime to rewrite the row's agent and full-reload it. Positioned like the
  Advanced submenu itself: centred in the sidebar, beside the clicked row.
  */
  const switchableAgents = session.switchableAgents ?? [];
  const canSwitchAccount = canFullReloadSession && switchableAgents.length > 0;
  const openSwitchAccountSubmenu = (event: ReactMouseEvent<HTMLButtonElement>) => {
    if (switchAccountSubmenuPosition) {
      setSwitchAccountSubmenuPosition(undefined);
      return;
    }
    const bounds = event.currentTarget.getBoundingClientRect();
    const submenuWidth = 204;
    const submenuHeight = CONTEXT_MENU_VERTICAL_PADDING_PX + switchableAgents.length * CONTEXT_MENU_ITEM_HEIGHT_PX;
    setSwitchAccountSubmenuPosition({
      x: getCenteredSidebarMenuX(submenuWidth),
      y: Math.max(
        CONTEXT_MENU_MARGIN_PX,
        Math.min(bounds.bottom + 4, window.innerHeight - submenuHeight - CONTEXT_MENU_MARGIN_PX)
      ),
    });
  };
  const requestSwitchSessionAgent = (agentId: string) => {
    setContextMenuPosition(undefined);
    setAdvancedSubmenuPosition(undefined);
    setSwitchAccountSubmenuPosition(undefined);
    vscode.postMessage({
      agentId,
      sessionId: session.sessionId,
      type: 'switchSessionAgent',
    });
  };

  const canCreateSessionGroupFromSession = sessionGroup?.canCreateSessionGroup === true;

  const requestCreateSessionGroupFromSession = () => {
    setContextMenuPosition(undefined);
    vscode.postMessage({
      sessionId: session.sessionId,
      type: 'createGroupFromSession',
    });
  };

  const requestViewFirstUserMessage = () => {
    const message = session.firstUserMessage?.trim();
    if (!message) {
      return;
    }

    setContextMenuPosition(undefined);
    openAppModal({
      message,
      modal: 'firstUserMessage',
      title: getSessionRenameInitialTitle(session),
      type: 'open',
    });
  };

  const requestSetSleeping = (sleeping: boolean) => {
    flushSync(() => {
      setContextMenuPosition(undefined);
      /*
       * CDXC:SessionSleep 2026-06-10-10:01:
       * Sleep state must come from native/gxserver after zmx provider shutdown.
       * Wake can clear the local faded row immediately because it is reopening a
       * sleeping record, but Sleep must not create a fake sleeping row first.
       */
      if (!sleeping) {
        useSidebarStore.getState().setSessionSleepingLocally(session.sessionId, sleeping);
      }
    });
    vscode.postMessage({
      sessionId: session.sessionId,
      sleeping,
      type: 'setSessionSleeping',
    });
  };

  const requestSleepBelow = () => {
    const requestStartedAtMs = performance.now();
    const latestSessionIdsBelow = readLatestSessionIdsBelow();
    const targetSessionIds = getSleepableSessionIds(latestSessionIdsBelow);
    const resolveDurationMs = performance.now() - requestStartedAtMs;
    const baseDebugDetails = {
      clickedSessionKind: session.sessionKind ?? session.kind,
      debugInstanceId: debugInstanceIdRef.current,
      resolveDurationMs,
      skippedCount: Math.max(0, latestSessionIdsBelow.length - targetSessionIds.length),
      sourceIndex: index,
      targetCount: targetSessionIds.length,
      visibleBelowCount: latestSessionIdsBelow.length,
    };
    if (targetSessionIds.length === 0) {
      postSidebarRefreshDebugLog(
        showDebugSessionNumbers,
        vscode,
        `${SLEEP_BELOW_DEBUG_EVENT_PREFIX}.skipped`,
        createSleepBelowDebugDetails({
          ...baseDebugDetails,
          elapsedSinceRequestMs: performance.now() - requestStartedAtMs,
          event: 'skipped',
        })
      );
      return;
    }

    postSidebarRefreshDebugLog(
      showDebugSessionNumbers,
      vscode,
      `${SLEEP_BELOW_DEBUG_EVENT_PREFIX}.requested`,
      createSleepBelowDebugDetails({
        ...baseDebugDetails,
        elapsedSinceRequestMs: performance.now() - requestStartedAtMs,
        event: 'requested',
      })
    );
    const flushStartedAtMs = performance.now();
    flushSync(() => {
      setContextMenuPosition(undefined);
    });
    const flushDurationMs = performance.now() - flushStartedAtMs;
    const postMessageStartedAtMs = performance.now();
    vscode.postMessage({
      sessionIds: targetSessionIds,
      sleeping: true,
      source: 'sleepBelow',
      type: 'setSessionsSleeping',
    });
    const postMessageDurationMs = performance.now() - postMessageStartedAtMs;
    postSidebarRefreshDebugLog(
      showDebugSessionNumbers,
      vscode,
      `${SLEEP_BELOW_DEBUG_EVENT_PREFIX}.posted`,
      createSleepBelowDebugDetails({
        ...baseDebugDetails,
        elapsedSinceRequestMs: performance.now() - requestStartedAtMs,
        event: 'posted',
        flushDurationMs,
        postMessageDurationMs,
      })
    );
    const frameProbeStartedAtMs = performance.now();
    window.requestAnimationFrame(() => {
      postSidebarRefreshDebugLog(
        showDebugSessionNumbers,
        vscode,
        `${SLEEP_BELOW_DEBUG_EVENT_PREFIX}.nextFrame`,
        createSleepBelowDebugDetails({
          ...baseDebugDetails,
          elapsedSinceRequestMs: performance.now() - requestStartedAtMs,
          event: 'nextFrame',
          flushDurationMs,
          frameDelayMs: performance.now() - frameProbeStartedAtMs,
          postMessageDurationMs,
        })
      );
    });
  };

  const requestCloseBelow = () => {
    const targetSessionIds = [...readLatestSessionIdsBelow()];
    if (targetSessionIds.length === 0) {
      return;
    }

    flushSync(() => {
      setContextMenuPosition(undefined);
      suppressCloseDrivenFocusedSessionScroll(targetSessionIds);
      useSidebarStore.getState().hideSessionsLocally(targetSessionIds);
    });
    postSidebarSessionsCloseInBackground(vscode, targetSessionIds);
  };

  const clearSessionSelection = (reason: string) => {
    onSessionSelectionChange?.({ groupId, mode: 'clear', reason, sessionId: session.sessionId });
  };

  const dismissBulkContextMenu = () => {
    setContextMenuPosition(undefined);
    setTagSubmenuPosition(undefined);
    setAdvancedSubmenuPosition(undefined);
    setSwitchAccountSubmenuPosition(undefined);
    setContextMenuSelectedSessionIds(EMPTY_SESSION_IDS);
  };

  const requestSetSelectedSessionsSleeping = (sleeping: boolean) => {
    const targetSessionIds = sleeping
      ? (bulkActionAvailability?.sleepableSessionIds ?? EMPTY_SESSION_IDS)
      : (bulkActionAvailability?.wakeableSessionIds ?? EMPTY_SESSION_IDS);
    if (targetSessionIds.length === 0) {
      return;
    }

    flushSync(() => {
      dismissBulkContextMenu();
      if (!sleeping) {
        for (const targetSessionId of targetSessionIds) {
          useSidebarStore.getState().setSessionSleepingLocally(targetSessionId, false);
        }
      }
    });
    clearSessionSelection('bulkSetSleeping');
    vscode.postMessage({
      sessionIds: [...targetSessionIds],
      sleeping,
      type: 'setSessionsSleeping',
    });
  };

  const requestSetSelectedSessionsPinned = (pinned: boolean) => {
    const targetSessionIds = pinned
      ? (bulkActionAvailability?.pinnableSessionIds ?? EMPTY_SESSION_IDS)
      : (bulkActionAvailability?.unpinnableSessionIds ?? EMPTY_SESSION_IDS);
    if (targetSessionIds.length === 0) {
      return;
    }

    dismissBulkContextMenu();
    clearSessionSelection('bulkSetPinned');
    runSidebarBulkContextMenuActionInBackground(targetSessionIds, (targetSessionId) => {
      vscode.postMessage({
        pinned,
        sessionId: targetSessionId,
        type: 'setSessionPinned',
      });
    });
  };

  const requestSetSelectedSessionTag = (tag: SidebarSessionTag | undefined) => {
    const targetSessionIds = bulkActionAvailability?.taggableSessionIds ?? EMPTY_SESSION_IDS;
    if (targetSessionIds.length === 0) {
      return;
    }

    dismissBulkContextMenu();
    clearSessionSelection('bulkSetTag');
    runSidebarBulkContextMenuActionInBackground(targetSessionIds, (targetSessionId) => {
      vscode.postMessage({
        sessionId: targetSessionId,
        sessionTag: tag ?? null,
        type: 'setSessionTag',
      });
    });
  };

  const requestFullReloadSelectedSessions = () => {
    const targetSessionIds = bulkActionAvailability?.fullReloadableSessionIds ?? EMPTY_SESSION_IDS;
    if (targetSessionIds.length === 0) {
      return;
    }

    dismissBulkContextMenu();
    clearSessionSelection('bulkFullReload');
    runSidebarBulkContextMenuActionInBackground(targetSessionIds, (targetSessionId) => {
      vscode.postMessage({
        sessionId: targetSessionId,
        type: 'fullReloadSession',
      });
    });
  };

  const requestCloseSelectedSessions = () => {
    const targetSessionIds = bulkActionAvailability?.closableSessionIds ?? EMPTY_SESSION_IDS;
    if (targetSessionIds.length === 0) {
      return;
    }

    flushSync(() => {
      dismissBulkContextMenu();
      suppressCloseDrivenFocusedSessionScroll(targetSessionIds);
      useSidebarStore.getState().hideSessionsLocally(targetSessionIds);
    });
    clearSessionSelection('bulkClose');
    postSidebarSessionsCloseInBackground(vscode, targetSessionIds);
  };

  const requestSetSessionTag = (tag: SidebarSessionTag | undefined) => {
    setContextMenuPosition(undefined);
    setTagSubmenuPosition(undefined);
    vscode.postMessage({
      sessionId: session.sessionId,
      sessionTag: tag ?? null,
      type: 'setSessionTag',
    });
  };

  const openSessionTagSubmenu = (event: ReactMouseEvent<HTMLButtonElement>) => {
    if (tagSubmenuPosition) {
      setTagSubmenuPosition(undefined);
      return;
    }
    setAdvancedSubmenuPosition(undefined);
    setSwitchAccountSubmenuPosition(undefined);
    const bounds = event.currentTarget.getBoundingClientRect();
    const submenuWidth = 204;
    const submenuHeight =
      CONTEXT_MENU_VERTICAL_PADDING_PX +
      sessionTagSubmenuItemCount * CONTEXT_MENU_ITEM_HEIGHT_PX +
      Math.max(0, sessionTagSubmenuSections.length - 1) * 10;
    setTagSubmenuPosition({
      x: getCenteredSidebarMenuX(submenuWidth),
      y: Math.max(
        CONTEXT_MENU_MARGIN_PX,
        Math.min(bounds.bottom + 4, window.innerHeight - submenuHeight - CONTEXT_MENU_MARGIN_PX)
      ),
    });
  };

  const requestSetPinned = (pinned: boolean) => {
    setContextMenuPosition(undefined);
    vscode.postMessage({
      pinned,
      sessionId: session.sessionId,
      type: 'setSessionPinned',
    });
  };

  const requestSetParked = (parked: boolean) => {
    setContextMenuPosition(undefined);
    vscode.postMessage({
      parked,
      sessionId: session.sessionId,
      type: 'setSessionParked',
    });
  };

  const bulkPrimaryActions: SessionContextMenuAction[] = [];
  if (bulkActionAvailability && bulkActionAvailability.sleepableSessionIds.length > 0) {
    bulkPrimaryActions.push({
      icon: <IconMoon aria-hidden='true' className='session-context-menu-icon' size={16} stroke={1.8} />,
      key: 'sleep-selected',
      label: 'Sleep selected',
      onClick: () => requestSetSelectedSessionsSleeping(true),
    });
  }
  if (bulkActionAvailability && bulkActionAvailability.wakeableSessionIds.length > 0) {
    bulkPrimaryActions.push({
      icon: <IconPlayerPlay aria-hidden='true' className='session-context-menu-icon' size={16} stroke={1.8} />,
      key: 'wake-selected',
      label: 'Wake selected',
      onClick: () => requestSetSelectedSessionsSleeping(false),
    });
  }
  if (
    bulkActionAvailability &&
    bulkActionAvailability.taggableSessionIds.length > 0 &&
    sessionTagSubmenuItemCount > 0
  ) {
    bulkPrimaryActions.push({
      icon: <IconTag aria-hidden='true' className='session-context-menu-icon' size={16} stroke={1.8} />,
      key: 'tag-selected-as',
      label: 'Tag selected as',
      onClick: openSessionTagSubmenu,
      submenu: 'session-tags',
    });
  }
  if (bulkActionAvailability && bulkActionAvailability.pinnableSessionIds.length > 0) {
    bulkPrimaryActions.push({
      icon: <IconPinned aria-hidden='true' className='session-context-menu-icon' size={16} stroke={1.8} />,
      key: 'pin-selected',
      label: 'Pin selected',
      onClick: () => requestSetSelectedSessionsPinned(true),
    });
  }
  if (bulkActionAvailability && bulkActionAvailability.unpinnableSessionIds.length > 0) {
    bulkPrimaryActions.push({
      icon: <IconPinnedOff aria-hidden='true' className='session-context-menu-icon' size={16} stroke={1.8} />,
      key: 'unpin-selected',
      label: 'Unpin selected',
      onClick: () => requestSetSelectedSessionsPinned(false),
    });
  }
  if (bulkActionAvailability && bulkActionAvailability.fullReloadableSessionIds.length > 0) {
    bulkPrimaryActions.push({
      icon: <IconRefresh aria-hidden='true' className='session-context-menu-icon' size={16} stroke={1.8} />,
      key: 'full-reload-selected',
      label: 'Full reload selected',
      onClick: requestFullReloadSelectedSessions,
    });
  }

  const bulkDestructiveActions: SessionContextMenuAction[] = [];
  if (bulkActionAvailability && bulkActionAvailability.closableSessionIds.length > 0) {
    bulkDestructiveActions.push({
      danger: true,
      icon: <IconX aria-hidden='true' className='session-context-menu-icon' size={16} stroke={1.8} />,
      /*
       * CDXC:Sessions 2026-07-01-18:33:
       * Multi-selection is an explicit advanced selection state, so Close selected
       * belongs in the selected-row bulk menu even when the single-row Close item
       * stays behind the Session Cards setting.
       */
      key: 'close-selected',
      label: 'Close selected',
      onClick: requestCloseSelectedSessions,
    });
  }

  const primaryActions: SessionContextMenuAction[] = [];
  if (canRenameSession) {
    primaryActions.push({
      icon: <IconPencil aria-hidden='true' className='session-context-menu-icon' size={16} stroke={1.8} />,
      key: 'rename',
      label: 'Rename',
      onClick: requestRename,
    });
  }
  if (canSleepSession) {
    primaryActions.push({
      icon:
        lifecycleState === 'sleeping' ? (
          <IconPlayerPlay aria-hidden='true' className='session-context-menu-icon' size={16} stroke={1.8} />
        ) : (
          <IconMoon aria-hidden='true' className='session-context-menu-icon' size={16} stroke={1.8} />
        ),
      key: 'sleep',
      label: lifecycleState === 'sleeping' ? 'Wake' : 'Sleep',
      onClick: () => requestSetSleeping(lifecycleState === 'running'),
    });
  }
  if (canPinSession) {
    primaryActions.push({
      icon: session.isPinned ? (
        <IconPinnedOff aria-hidden='true' className='session-context-menu-icon' size={16} stroke={1.8} />
      ) : (
        <IconPinned aria-hidden='true' className='session-context-menu-icon' size={16} stroke={1.8} />
      ),
      /**
       * CDXC:Sessions 2026-05-28-12:04:
       * Pinning is a live sidebar-order control, not Favorite. Expose it as its
       * own context-menu action so users can pin any project session without
       * changing previous-session favorites or auto-sleep favorite rules.
       */
      key: 'pin',
      label: session.isPinned ? 'Unpin' : 'Pin',
      onClick: () => requestSetPinned(!session.isPinned),
    });
  }
  if (enableSessionParking && canPinSession && !isBrowserSession) {
    primaryActions.push({
      icon: <IconArchive aria-hidden='true' className='session-context-menu-icon' size={16} stroke={1.8} />,
      key: 'park',
      label: session.isParked ? 'Unpark' : 'Park',
      onClick: () => requestSetParked(!session.isParked),
    });
  }
  if (canOpenSessionNote) {
    primaryActions.push({
      icon: <IconNote aria-hidden='true' className='session-context-menu-icon' size={16} stroke={1.8} />,
      key: 'session-note',
      label: 'Note',
      onClick: requestOpenSessionNote,
    });
  }
  if (canTagSession && sessionTagSubmenuItemCount > 0) {
    primaryActions.push({
      icon: <IconTag aria-hidden='true' className='session-context-menu-icon' size={16} stroke={1.8} />,
      key: 'tag-as',
      label: 'Tag as',
      onClick: openSessionTagSubmenu,
      submenu: 'session-tags',
    });
  }

  const advancedSessionActions: SessionContextMenuAction[] = [];
  if (canDelayedSend) {
    advancedSessionActions.push({
      icon: <IconClock aria-hidden='true' className='session-context-menu-icon' size={16} stroke={1.8} />,
      key: 'delayed-send',
      label: 'Delayed Send',
      onClick: requestDelayedSend,
    });
  }
  if (canCloseAfterDone) {
    /*
     * CDXC:Sessions 2026-06-15-21:00:
     * Close After Done stays next to Delayed Send. The menu glyph inherits the
     * normal context-menu icon tint. Only the armed session-card status clock
     * uses pastel red.
     */
    advancedSessionActions.push({
      icon: <IconClock aria-hidden='true' className='session-context-menu-icon' size={16} stroke={1.8} />,
      key: 'close-after-done',
      label: 'Close After Done',
      onClick: requestToggleCloseAfterDone,
    });
  }
  if (canForkSession) {
    advancedSessionActions.push({
      icon: <IconGitFork aria-hidden='true' className='session-context-menu-icon' size={16} stroke={1.8} />,
      key: 'fork',
      label: 'Fork',
      onClick: requestForkSession,
    });
  }
  // Full reload sits directly above Switch Account, which is Full reload under
  // another same-family agent configuration; both sit above Handoff / Export.
  if (canFullReloadSession) {
    advancedSessionActions.push({
      icon: <IconRefresh aria-hidden='true' className='session-context-menu-icon' size={16} stroke={1.8} />,
      key: 'full-reload',
      label: 'Full reload',
      onClick: requestFullReloadSession,
    });
  }
  if (canSwitchAccount) {
    advancedSessionActions.push({
      icon: <IconSwitchHorizontal aria-hidden='true' className='session-context-menu-icon' size={16} stroke={1.8} />,
      key: 'switch-account',
      label: 'Switch Account',
      onClick: openSwitchAccountSubmenu,
      submenu: 'switch-account',
    });
  }
  if (canExportTranscript) {
    advancedSessionActions.push({
      icon: <IconFileExport aria-hidden='true' className='session-context-menu-icon' size={16} stroke={1.8} />,
      key: 'export-transcript',
      label: 'Handoff / Export',
      onClick: requestExportTranscript,
    });
  }
  if (session.firstUserMessage?.trim()) {
    advancedSessionActions.push({
      icon: <IconMessageCircle aria-hidden='true' className='session-context-menu-icon' size={16} stroke={1.8} />,
      key: 'view-first-message',
      label: 'View 1st message',
      onClick: requestViewFirstUserMessage,
    });
  }
  if (canGenerateSessionTitle) {
    /**
     * CDXC:SessionTitles 2026-05-08-10:54
     * Claude and Codex thread cards need a direct "Generate Title" action that
     * retitles the session from the saved 1st user message. The action is only
     * useful once that message exists, because the controller intentionally
     * generates from real user text rather than from title fallbacks.
     */
    advancedSessionActions.push({
      icon: <IconSparkles aria-hidden='true' className='session-context-menu-icon' size={16} stroke={1.8} />,
      key: 'generate-title',
      label: 'Generate Title',
      onClick: requestGenerateSessionTitle,
    });
  }
  if (canCreateSessionGroupFromSession) {
    advancedSessionActions.push({
      icon: (
        <IconLayoutSidebarRightExpand aria-hidden='true' className='session-context-menu-icon' size={16} stroke={1.8} />
      ),
      key: 'move-to-new-group',
      label: 'Move to New Group',
      onClick: requestCreateSessionGroupFromSession,
    });
  }
  if (canSplitSessionRight) {
    advancedSessionActions.push({
      icon: <IconLayoutColumns aria-hidden='true' className='session-context-menu-icon' size={16} stroke={1.8} />,
      key: 'split-right',
      label: 'Split Right',
      onClick: requestSplitSessionRight,
    });
  }
  if (canFocusMode) {
    /**
     * CDXC:FocusMode 2026-05-28-12:52:
     * Sidebar context-menu Focus should only appear when the group has split panes to zoom.
     * A single pane with multiple tabs still uses normal tab selection, so hiding Focus here keeps the menu aligned with double-click behavior.
     */
    advancedSessionActions.push({
      icon: <IconFocus2 aria-hidden='true' className='session-context-menu-icon' size={16} stroke={1.8} />,
      key: 'focus-mode',
      label: 'Focus',
      onClick: requestFocusMode,
    });
  }

  const advancedCopyActions: SessionContextMenuAction[] = [];
  if (canCopySessionDetails) {
    advancedCopyActions.push({
      icon: <IconCopy aria-hidden='true' className='session-context-menu-icon' size={16} stroke={1.8} />,
      key: 'copy-details',
      label: 'Copy details',
      onClick: requestCopySessionDetails,
    });
  }
  if (canCopyResumeCommand) {
    advancedCopyActions.push({
      icon: <IconCopy aria-hidden='true' className='session-context-menu-icon' size={16} stroke={1.8} />,
      key: 'copy-resume',
      label: 'Copy resume',
      onClick: requestCopyResumeCommand,
    });
  }
  if (canCopyAttachCommand) {
    advancedCopyActions.push({
      icon: <IconCopy aria-hidden='true' className='session-context-menu-icon' size={16} stroke={1.8} />,
      key: 'copy-attach',
      label: 'Copy attach command',
      onClick: requestCopyAttachCommand,
    });
  }

  const belowActions: SessionContextMenuAction[] = [];
  if (effectiveSessionIdsBelow.length > 0) {
    /**
     * CDXC:ContextMenus 2026-06-04-23:40:
     * Session row context menus expose below-scoped lifecycle actions only
     * when the clicked row has visible sessions beneath it. Sleep below targets
     * sleepable terminal, agent, and browser rows, while Close below
     * removes every visible row beneath the clicked session in the current
     * sidebar order.
     *
     * CDXC:ContextMenus 2026-06-10-10:01:
     * Sleep below is scoped to the clicked session's current project/group, not
     * every rendered row lower in the sidebar. Do not paint rows as sleeping
     * before native/gxserver confirms the zmx provider was actually stopped.
     */
    if (sleepableSessionIdsBelow.length > 0) {
      belowActions.push({
        icon: <IconMoon aria-hidden='true' className='session-context-menu-icon' size={16} stroke={1.8} />,
        key: 'sleep-below',
        label: 'Sleep below',
        onClick: requestSleepBelow,
      });
    }
    belowActions.push({
      danger: true,
      icon: <IconX aria-hidden='true' className='session-context-menu-icon' size={16} stroke={1.8} />,
      key: 'close-below',
      label: 'Close below',
      onClick: requestCloseBelow,
    });
  }

  const advancedSections = [
    { actions: advancedSessionActions, label: 'Session' },
    { actions: advancedCopyActions, label: 'Copy' },
    { actions: belowActions, label: 'Below' },
  ].filter((section) => section.actions.length > 0);

  const openAdvancedSubmenu = (event: ReactMouseEvent<HTMLButtonElement>) => {
    if (advancedSubmenuPosition) {
      setAdvancedSubmenuPosition(undefined);
      setSwitchAccountSubmenuPosition(undefined);
      return;
    }
    setTagSubmenuPosition(undefined);
    const bounds = event.currentTarget.getBoundingClientRect();
    const submenuWidth = 204;
    const itemCount = advancedSections.reduce((count, section) => count + section.actions.length, 0);
    const dividerCount = Math.max(0, advancedSections.length - 1);
    const labelCount = advancedSections.length;
    const submenuHeight =
      CONTEXT_MENU_VERTICAL_PADDING_PX +
      itemCount * CONTEXT_MENU_ITEM_HEIGHT_PX +
      dividerCount * CONTEXT_MENU_DIVIDER_HEIGHT_PX +
      labelCount * CONTEXT_MENU_GROUP_LABEL_HEIGHT_PX;
    setAdvancedSubmenuPosition({
      x: getCenteredSidebarMenuX(submenuWidth),
      y: Math.max(
        CONTEXT_MENU_MARGIN_PX,
        Math.min(bounds.bottom + 4, window.innerHeight - submenuHeight - CONTEXT_MENU_MARGIN_PX)
      ),
    });
  };

  const advancedRootActions: SessionContextMenuAction[] = [];
  if (advancedSections.length > 0) {
    advancedRootActions.push({
      icon: <IconDots aria-hidden='true' className='session-context-menu-icon' size={16} stroke={1.8} />,
      key: 'advanced',
      label: 'Advanced',
      onClick: openAdvancedSubmenu,
      submenu: 'advanced',
    });
  }

  const destructiveActions: SessionContextMenuAction[] = [];
  if (showSessionCloseContextMenuAction) {
    destructiveActions.push({
      danger: true,
      icon: <IconX aria-hidden='true' className='session-context-menu-icon' size={16} stroke={1.8} />,
      /**
       * CDXC:Sessions 2026-05-11-00:45
       * User-facing session removal language is Close. Keep the
       * destructive action behavior unchanged while making terminal and
       * browser context menus use the same visible verb.
       *
       * CDXC:ContextMenus 2026-06-10-13:58:
       * The Close menu item is hidden by default and appears only when the
       * Session Cards setting opts into destructive close actions in menus.
       */
      key: 'close',
      label: 'Close',
      onClick: () => requestClose('context-menu'),
    });
  }
  /*
   * CDXC:ContextMenus 2026-08-26:
   * The root menu is unlabeled: Rename/Sleep/Pin/Park/Note/Tag as are the
   * everyday rows, Advanced is the only nested parent, and Close names itself.
   * Group headings live inside Advanced (Session / Copy / Below). The bulk menu
   * stays unlabeled because every one of its rows already ends in "selected".
   */
  const contextMenuSections = (
    isBulkContextMenu
      ? [
          { actions: bulkPrimaryActions, label: undefined },
          { actions: bulkDestructiveActions, label: undefined },
        ]
      : [
          { actions: primaryActions, label: undefined },
          { actions: advancedRootActions, label: undefined },
          { actions: destructiveActions, label: undefined },
        ]
  ).filter((section) => section.actions.length > 0);
  const contextMenuItemCount = contextMenuSections.reduce((count, section) => count + section.actions.length, 0);
  const contextMenuDividerCount = Math.max(0, contextMenuSections.length - 1);

  const requestFocusSession = (
    event?: ReactKeyboardEvent<HTMLElement> | ReactMouseEvent<HTMLElement> | ReactPointerEvent<HTMLElement>
  ) => {
    if (isStaleRemoteRow) {
      return;
    }
    const shouldAcknowledgeAttention = session.activity === 'attention';
    if (event?.metaKey !== true && event?.shiftKey !== true) {
      clearSessionSelection('focusRequest');
    }
    /**
     * CDXC:FocusRouting 2026-05-15-20:01:
     * Intermittent sidebar-card clicks can select an existing session through a
     * newly synthesized native split. Persist the DOM click metadata, card
     * focus state, group id, and local-focus decision so a later repro can be
     * matched against native paneLayout resolution instead of guessing which
     * card action fired.
     */
    vscode.postMessage({
      details: {
        activity: session.activity,
        button: event && 'button' in event ? event.button : undefined,
        clientX: event && 'clientX' in event ? event.clientX : undefined,
        clientY: event && 'clientY' in event ? event.clientY : undefined,
        clickDetail: event && 'detail' in event ? event.detail : undefined,
        index,
        groupId,
        isFocused: session.isFocused,
        isSleeping: session.isSleeping,
        isVisible: session.isVisible,
        localFocusWillRun: !session.isFocused,
        metaKey: event?.metaKey ?? false,
        requestedAt: Date.now(),
        sessionId: session.sessionId,
        sessionKind: session.sessionKind,
        shiftKey: event?.shiftKey ?? false,
      },
      event: 'repro.sidebarSessionFocusRequested',
      scenarioId: 'gpui.sidebar.focus',
      type: 'sidebarDebugLog',
    });
    /*
     * CDXC:FocusRouting 2026-06-08-09:31:
     * Terminal switching should not wait behind local React focus rendering. Keep the forced focus breadcrumb first for native trace correlation, then send the authoritative focusSession command before applying the sidebar highlight locally; the following hydrate reconciles the UI after native focus/layout has started.
     */
    vscode.postMessage({ sessionId: session.sessionId, type: 'focusSession' });
    if (!session.isFocused) {
      onFocusRequested?.(groupId, session.sessionId);
    }
  };

  const setNativeSidebarSessionFocusBorderHandoffHitTarget = (isSessionCard: boolean) => {
    if (isProjectSessionListMoreRow || isProjectSessionListOverflowRow) {
      return;
    }
    vscode.postMessage({
      isSessionCard,
      type: 'setSidebarSessionFocusBorderHandoffHitTarget',
    });
  };

  const cancelNativeSidebarSessionFocusBorderHandoff = () => {
    vscode.postMessage({
      type: 'cancelSidebarSessionFocusBorderHandoff',
    });
  };

  const rememberImmediateFocusClickSuppression = () => {
    if (immediateFocusClickSuppressionRef.current) {
      window.clearTimeout(immediateFocusClickSuppressionRef.current.timeoutId);
    }

    const timeoutId = window.setTimeout(() => {
      if (immediateFocusClickSuppressionRef.current?.sessionId === session.sessionId) {
        immediateFocusClickSuppressionRef.current = undefined;
      }
    }, SESSION_CARD_IMMEDIATE_FOCUS_CLICK_SUPPRESSION_MS);

    immediateFocusClickSuppressionRef.current = {
      sessionId: session.sessionId,
      timeoutId,
    };
  };

  const consumeImmediateFocusClickSuppression = () => {
    if (immediateFocusClickSuppressionRef.current?.sessionId !== session.sessionId) {
      return false;
    }

    window.clearTimeout(immediateFocusClickSuppressionRef.current.timeoutId);
    immediateFocusClickSuppressionRef.current = undefined;
    return true;
  };

  const handleKeyDown = (event: ReactKeyboardEvent<HTMLElement>) => {
    if (isProjectSessionListOverflowRow) {
      event.preventDefault();
      event.stopPropagation();
      return;
    }

    if (isProjectSessionListMoreRow) {
      if (event.key !== 'Enter' && event.key !== ' ') {
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      projectSessionListMoreRow.onReveal();
      return;
    }

    if (event.key === 'ContextMenu' || (event.shiftKey && event.key === 'F10')) {
      event.preventDefault();
      event.stopPropagation();
      const bounds = event.currentTarget.getBoundingClientRect();
      openContextMenu(bounds.top + 18);
      return;
    }

    if (event.key !== 'Enter' && event.key !== ' ') {
      return;
    }

    event.preventDefault();
    event.stopPropagation();
    requestFocusSession();
  };

  return (
    <>
      <OverflowTooltipText
        delayMs={sidebarItemTooltipDelayMs}
        text={sessionTitleTooltip.headingText}
        textRef={aliasHeadingRef}
        tooltip={sessionTitleTooltip.tooltip}
        tooltipWhen={sessionTitleTooltip.tooltipWhen}
      >
        <div
          className='session-frame'
          data-activity={session.activity}
          data-dragging={String(Boolean(sortable.isDragging))}
          data-drop-position={visibleDropPosition}
          data-drop-target={String(shouldShowGroupDropTargetChrome)}
          data-focused={String(session.isFocused)}
          data-group-connector={String(showGroupConnector)}
          data-has-agent-icon={String(hasSessionCardIcon)}
          data-agent-icon-hover-only={String(hideSessionAgentIconUntilHover)}
          data-browser-favicon-hover-only={String(
            isBrowserSession && Boolean(session.faviconDataUrl) && hideBrowserFaviconUntilHover
          )}
          data-lifecycle-state={lifecycleState}
          data-multi-selected={String(isMultiSelected)}
          data-project-session-list-more-row={String(isProjectSessionListMoreRow)}
          data-project-session-list-more-toggle={isProjectSessionListMoreRow ? 'true' : undefined}
          data-project-session-list-overflow={String(isProjectSessionListOverflowRow)}
          data-pinned={String(session.isPinned === true)}
          data-tagged={String(Boolean(currentSessionTag))}
          data-remote-session={String(isRemoteSession)}
          data-running={String(lifecycleState === 'running')}
          data-sleeping={String(Boolean(session.isSleeping))}
          data-visible={String(session.isVisible)}
          ref={setSessionFrameElement}
        >
          <div
            aria-hidden
            className='session-drop-target-surface session-drop-target-surface-before'
            ref={beforeDropTarget.ref}
          />
          <div
            aria-hidden
            className='session-drop-target-surface session-drop-target-surface-after'
            ref={afterDropTarget.ref}
          />
          <article
            aria-current={session.isFocused ? 'page' : undefined}
            aria-hidden={isProjectSessionListOverflowRow ? true : undefined}
            aria-label={sessionAccessibleLabel}
            className='session'
            data-activity={session.activity}
            data-completion-flash={
              completionFlashRunId > 0 ? (completionFlashRunId % 2 === 0 ? 'even' : 'odd') : undefined
            }
            data-has-agent-icon={String(hasSessionCardIcon)}
            data-dragging={String(Boolean(sortable.isDragging))}
            /*
             * CDXC:Drafts 2026-08-28:
             * Present-only, exactly like the wire field: the attribute exists on
             * a draft and is absent otherwise, so `[data-draft='true']` can dim
             * the title without a `false` value needing its own rule.
             */
            data-draft={session.isDraft === true ? 'true' : undefined}
            data-drop-position={visibleDropPosition}
            data-drop-target={String(shouldShowGroupDropTargetChrome)}
            data-focused={String(session.isFocused)}
            data-group-connector={String(showGroupConnector)}
            data-lifecycle-state={lifecycleState}
            data-multi-selected={String(isMultiSelected)}
            data-project-session-list-more-row={String(isProjectSessionListMoreRow)}
            data-project-session-list-more-toggle={isProjectSessionListMoreRow ? 'true' : undefined}
            data-project-session-list-overflow={String(isProjectSessionListOverflowRow)}
            data-agent-icon-hover-only={String(hideSessionAgentIconUntilHover)}
            data-browser-favicon-hover-only={String(
              isBrowserSession && Boolean(session.faviconDataUrl) && hideBrowserFaviconUntilHover
            )}
            data-running={String(lifecycleState === 'running')}
            data-search-selected={String(isSearchSelected)}
            data-pinned={String(session.isPinned === true)}
            data-tagged={String(Boolean(currentSessionTag))}
            data-remote-session={String(isRemoteSession)}
            data-stale-remote={String(isStaleRemoteRow)}
            data-sleeping={String(Boolean(session.isSleeping))}
            data-sidebar-session-id={session.sessionId}
            data-visible={String(session.isVisible)}
            onPointerEnter={() => {
              setNativeSidebarSessionFocusBorderHandoffHitTarget(true);
            }}
            onPointerLeave={() => {
              setNativeSidebarSessionFocusBorderHandoffHitTarget(false);
            }}
            onPointerCancel={(event) => {
              postSessionDragDebugLog('session.pointerCancel', {
                button: event.button,
                buttons: event.buttons,
                clientX: event.clientX,
                clientY: event.clientY,
                pointerId: event.pointerId,
                pointerType: event.pointerType,
              });

              setNativeSidebarSessionFocusBorderHandoffHitTarget(false);
            }}
            onPointerDown={(event) => {
              const isInteractiveDescendant = isSessionCardPointerFocusBlockedByDescendant({
                currentTarget: event.currentTarget,
                target: event.target,
              });
              const isUnmodifiedPrimarySessionFocusMouseDown =
                event.button === 0 &&
                event.metaKey === false &&
                event.ctrlKey === false &&
                event.altKey === false &&
                event.shiftKey === false &&
                isInteractiveDescendant !== true &&
                (event.isPrimary ?? true) &&
                !isProjectSessionListMoreRow &&
                !isProjectSessionListOverflowRow;
              postSessionDragDebugLog('session.pointerDown', {
                button: event.button,
                buttons: event.buttons,
                clientX: event.clientX,
                clientY: event.clientY,
                isDragging: sortable.isDragging,
                pointerId: event.pointerId,
                pointerType: event.pointerType,
              });

              if (event.shiftKey || event.metaKey || event.ctrlKey || selectedSessionIds.length > 0) {
                /*
                 * CDXC:Sessions 2026-07-02-07:32:
                 * The pointer-down breadcrumb pairs with the click breadcrumb:
                 * a pointerDown without a matching click means the browser or
                 * the drag sensor consumed the gesture before the row's
                 * selection handler could run.
                 */
                postMultiSelectDebugLog('pointerDown', {
                  altKey: event.altKey,
                  button: event.button,
                  ctrlKey: event.ctrlKey,
                  isInteractiveDescendant,
                  isPrimary: event.isPrimary,
                  isSessionDragActivationEnabled,
                  metaKey: event.metaKey,
                  pointerType: event.pointerType,
                  shiftKey: event.shiftKey,
                });
              }

              /*
               * CDXC:FocusRouting 2026-06-29-02:04:
               * Native must start preserving the existing focused border before WebKit takes first responder, but only session-focus clicks should use that path. Keep the native hit target hot while the pointer is over a real session row, then cancel the handoff when this mouseDown is a child control or modified click instead of a normal row focus action.
               */
              if (!isUnmodifiedPrimarySessionFocusMouseDown) {
                cancelNativeSidebarSessionFocusBorderHandoff();
              }

              if (
                shouldFocusSidebarSessionOnPointerDown({
                  altKey: event.altKey,
                  button: event.button,
                  ctrlKey: event.ctrlKey,
                  isInteractiveDescendant,
                  isPrimary: event.isPrimary,
                  isSessionDragActivationEnabled,
                  isProjectSessionListMoreRow,
                  isProjectSessionListOverflowRow,
                  metaKey: event.metaKey,
                  renameSessionOnDoubleClick,
                  shiftKey: event.shiftKey,
                })
              ) {
                /*
                 * CDXC:FocusRouting 2026-06-27-21:08:
                 * Simple session-row clicks focus on pointer-down and suppress
                 * the follow-up click. Prevent the default pointer focus only
                 * on that same path so WKWebView does not retake first
                 * responder after native has focused the terminal; the event
                 * still propagates to drag sensors, while modified clicks,
                 * context menus, rename double-clicks, and child buttons keep
                 * their existing behavior.
                 */
                event.preventDefault();
                rememberImmediateFocusClickSuppression();
                requestFocusSession(event);
              }
            }}
            onPointerUp={(event) => {
              postSessionDragDebugLog('session.pointerUp', {
                button: event.button,
                buttons: event.buttons,
                clientX: event.clientX,
                clientY: event.clientY,
                isDragging: sortable.isDragging,
                pointerId: event.pointerId,
                pointerType: event.pointerType,
              });
            }}
            onAuxClick={(event) => {
              if (isProjectSessionListOverflowRow || isProjectSessionListMoreRow) {
                event.preventDefault();
                event.stopPropagation();
                return;
              }

              if (event.button !== 1) {
                return;
              }

              event.preventDefault();
              requestClose('middle-click');
            }}
            onClick={(event) => {
              event.stopPropagation();

              if (isProjectSessionListOverflowRow) {
                event.preventDefault();
                return;
              }

              if (isProjectSessionListMoreRow) {
                event.preventDefault();
                projectSessionListMoreRow.onReveal();
                return;
              }

              /*
               * CDXC:Sessions 2026-07-02-07:32:
               * Selection repros must show whether the click event arrived at
               * this row at all and which branch consumed it. Log every
               * modified click, plus plain clicks while a selection exists.
               */
              const shouldLogSelectionClick =
                event.shiftKey || event.metaKey || event.ctrlKey || selectedSessionIds.length > 0;
              const logSelectionClick = (branch: string) => {
                if (!shouldLogSelectionClick) {
                  return;
                }
                postMultiSelectDebugLog('click', {
                  branch,
                  ctrlKey: event.ctrlKey,
                  detail: event.detail,
                  metaKey: event.metaKey,
                  shiftKey: event.shiftKey,
                });
              };

              if (event.shiftKey) {
                event.preventDefault();
                logSelectionClick('range');
                onSessionSelectionChange?.({
                  groupId,
                  mode: 'range',
                  sessionId: session.sessionId,
                });
                return;
              }

              if (event.metaKey) {
                event.preventDefault();
                logSelectionClick('additive');
                onSessionSelectionChange?.({
                  groupId,
                  mode: 'additive',
                  sessionId: session.sessionId,
                });
                return;
              }

              if (consumeImmediateFocusClickSuppression()) {
                logSelectionClick('focusSuppressed');
                return;
              }

              logSelectionClick('focus');
              requestFocusSession(event);
            }}
            onDoubleClick={(event) => {
              event.preventDefault();
              event.stopPropagation();
              if (
                !shouldRenameSidebarSessionOnDoubleClick({
                  isBrowserSession,
                  isProjectSessionListMoreRow,
                  isProjectSessionListOverflowRow,
                  renameSessionOnDoubleClick,
                })
              ) {
                return;
              }
              requestRename();
            }}
            onContextMenu={(event: ReactMouseEvent<HTMLElement>) => {
              event.preventDefault();
              event.stopPropagation();
              if (isProjectSessionListOverflowRow || isProjectSessionListMoreRow) {
                return;
              }
              openContextMenu(event.clientY, event.clientX);
            }}
            onKeyDown={handleKeyDown}
            ref={setSessionCardElement}
            role='button'
            style={sessionAnchorStyle}
            tabIndex={isProjectSessionListOverflowRow ? -1 : 0}
          >
            {isProjectSessionListMoreRow ? null : (
              <SessionFloatingAgentIcon
                agentIcon={session.agentIcon}
                accountColor={session.accountColor}
                closeAfterDone={session.closeAfterDone}
                closeAfterDoneDeadlineAt={session.closeAfterDoneDeadlineAt}
                closeAfterDoneRemainingLabel={session.closeAfterDoneRemainingLabel}
                delayedSendDeadlineAt={session.delayedSendDeadlineAt}
                delayedSendRemainingLabel={session.delayedSendRemainingLabel}
                faviconDataUrl={session.faviconDataUrl}
                hasComposerDraft={session.hasComposerDraft === true}
                hasSessionNote={Boolean(session.sessionNote?.trim())}
                isDraft={session.isDraft === true}
                isFavorite={session.isFavorite}
                isPinned={session.isPinned}
                isReloading={session.isReloading}
                onCloseAfterDoneClick={requestToggleCloseAfterDone}
                onDelayedSendClick={requestDelayedSend}
                onPinnedClick={requestSetPinned}
                queuedPromptCount={session.queuedPromptCount}
                queuedPromptFailedCount={session.queuedPromptFailedCount}
                sessionTag={session.sessionTag}
                sessionPersistenceName={session.sessionPersistenceName}
                sessionPersistenceProvider={session.sessionPersistenceProvider}
                showTerminalIcon={showTerminalSessionIcon}
              />
            )}
            {/**
             * CDXC:Sessions 2026-05-09-16:55
             * Project and chat session cards route the close-on-hover setting
             * through the same shared row across terminal, agent, and
             * browser panes.
             */}
            <SessionCardContent
              aliasHeadingRef={aliasHeadingRef}
              onDelayedSendClick={requestDelayedSend}
              onClose={() => requestClose('programmatic')}
              session={session}
              showDebugSessionNumbers={showDebugSessionNumbers}
              showCloseButton={!isProjectSessionListMoreRow && showCloseButton}
              showLastActiveTime={!isProjectSessionListMoreRow && showLastActiveTime}
              hideHeaderAgentIcon={isProjectSessionListMoreRow}
            />
          </article>
          {isProjectSessionListMoreRow ? null : (
            <div aria-hidden className='session-status-dot session-status-dot-inline' />
          )}
        </div>
      </OverflowTooltipText>
      {contextMenuPosition && !isProjectSessionListMoreRow ? (
        <SidebarContextMenuPortal
          menuClassName='session-context-menu sidebar-session-context-menu'
          menuRef={menuRef}
          menuStyle={{
            left: `${contextMenuPosition.x}px`,
            top: `${contextMenuPosition.y}px`,
          }}
          onDismiss={() => {
            setContextMenuPosition(undefined);
            setTagSubmenuPosition(undefined);
            setAdvancedSubmenuPosition(undefined);
            setSwitchAccountSubmenuPosition(undefined);
            setContextMenuSelectedSessionIds(EMPTY_SESSION_IDS);
          }}
          vscode={vscode}
        >
          {contextMenuSections.map((section, sectionIndex) => (
            <Fragment key={`section-${sectionIndex}`}>
              {sectionIndex > 0 ? <div className='session-context-menu-divider' role='separator' /> : null}
              <div className='session-context-menu-section'>
                {section.label ? <div className='session-context-menu-group-label'>{section.label}</div> : null}
                {section.actions.map((action) => (
                  <button
                    key={action.key}
                    className={`session-context-menu-item${action.danger ? ' session-context-menu-item-danger' : ''}`}
                    onClick={(event) => action.onClick(event)}
                    aria-expanded={
                      action.submenu === 'session-tags'
                        ? Boolean(tagSubmenuPosition)
                        : action.submenu === 'advanced'
                          ? Boolean(advancedSubmenuPosition)
                          : undefined
                    }
                    aria-haspopup={action.submenu ? 'menu' : undefined}
                    role='menuitem'
                    type='button'
                  >
                    {action.icon}
                    {action.label}
                    {action.submenu ? (
                      <IconChevronRight
                        aria-hidden='true'
                        className='session-context-menu-trailing-icon'
                        size={14}
                        stroke={1.8}
                      />
                    ) : null}
                  </button>
                ))}
              </div>
            </Fragment>
          ))}
        </SidebarContextMenuPortal>
      ) : null}
      {contextMenuPosition && tagSubmenuPosition && !isProjectSessionListMoreRow
        ? createPortal(
            <div
              aria-label='Tag as'
              className='session-context-menu session-tag-submenu'
              data-empty-space-blocking='true'
              onClick={(event) => event.stopPropagation()}
              role='menu'
              style={{
                left: `${tagSubmenuPosition.x}px`,
                top: `${tagSubmenuPosition.y}px`,
                /*
                 * CDXC:ContextMenus 2026-06-09-14:22:
                 * The Tag as submenu follows the raised sidebar context-menu
                 * stack so adjacent sidebar rows cannot cover the submenu while
                 * users are choosing a session marker.
                 */
                zIndex: 'var(--sidebar-context-menu-submenu-z-index, 301)',
              }}
            >
              {/*
               * CDXC:Sessions 2026-06-05-12:30:
               * The session context menu exposes `Tag as` as a submenu with the
               * settings-visible tag list. Choosing the current marker clears
               * it so the old Favorite/Unfavorite workflow remains one click deep.
               *
               * CDXC:Sessions 2026-06-16-00:05:
               * Tag context menus should not render Priority, Progress, or Type
               * label rows. Keep the grouped sections and dividers for scan
               * structure without spending vertical space on heading text.
               */}
              {sessionTagSubmenuSections.map((section) => (
                <div className='session-tag-menu-section' key={section.label}>
                  {section.options.map((option) => {
                    const isSelected = contextMenuSessionTag === option.value;
                    const optionLabel = getSidebarSessionTagLabel(option.value);
                    return (
                      <button
                        aria-checked={isSelected}
                        aria-label={
                          isSelected
                            ? isBulkContextMenu
                              ? `Remove ${optionLabel} tag from selected sessions`
                              : `Remove ${optionLabel} tag`
                            : isBulkContextMenu
                              ? `Tag selected sessions as ${optionLabel}`
                              : `Tag as ${optionLabel}`
                        }
                        className='session-context-menu-item session-tag-menu-item'
                        data-selected={String(isSelected)}
                        key={option.value}
                        onClick={() => {
                          const nextTag = isSelected ? undefined : option.value;
                          if (isBulkContextMenu) {
                            requestSetSelectedSessionTag(nextTag);
                            return;
                          }
                          requestSetSessionTag(nextTag);
                        }}
                        role='menuitemradio'
                        type='button'
                      >
                        <SessionTagIcon
                          className='session-context-menu-icon session-tag-colored-icon'
                          fillFavorite
                          size={16}
                          stroke={1.8}
                          tag={option.value}
                        />
                        <span className='session-tag-menu-item-label'>{option.label}</span>
                        <IconCheck
                          aria-hidden='true'
                          className='session-tag-menu-item-check'
                          data-visible={String(isSelected)}
                          size={14}
                          stroke={2}
                        />
                      </button>
                    );
                  })}
                </div>
              ))}
            </div>,
            document.body
          )
        : null}
      {contextMenuPosition && advancedSubmenuPosition && !isProjectSessionListMoreRow
        ? createPortal(
            <div
              aria-label='Advanced'
              className='session-context-menu session-tag-submenu'
              data-empty-space-blocking='true'
              onClick={(event) => event.stopPropagation()}
              role='menu'
              style={{
                left: `${advancedSubmenuPosition.x}px`,
                top: `${advancedSubmenuPosition.y}px`,
                zIndex: 'var(--sidebar-context-menu-submenu-z-index, 301)',
              }}
            >
              {advancedSections.map((section, sectionIndex) => (
                <Fragment key={`advanced-${section.label}`}>
                  {sectionIndex > 0 ? <div className='session-context-menu-divider' role='separator' /> : null}
                  <div className='session-context-menu-section'>
                    <div className='session-context-menu-group-label'>{section.label}</div>
                    {section.actions.map((action) => (
                      <button
                        key={action.key}
                        className={`session-context-menu-item${action.danger ? ' session-context-menu-item-danger' : ''}`}
                        onClick={(event) => action.onClick(event)}
                        aria-expanded={
                          action.submenu === 'switch-account' ? Boolean(switchAccountSubmenuPosition) : undefined
                        }
                        aria-haspopup={action.submenu ? 'menu' : undefined}
                        role='menuitem'
                        type='button'
                      >
                        {action.icon}
                        {action.label}
                        {action.submenu ? (
                          <IconChevronRight
                            aria-hidden='true'
                            className='session-context-menu-trailing-icon'
                            size={14}
                            stroke={1.8}
                          />
                        ) : null}
                      </button>
                    ))}
                  </div>
                </Fragment>
              ))}
            </div>,
            document.body
          )
        : null}
      {contextMenuPosition && advancedSubmenuPosition && switchAccountSubmenuPosition && !isProjectSessionListMoreRow
        ? createPortal(
            <div
              aria-label='Switch Account'
              className='session-context-menu session-tag-submenu session-switch-account-submenu'
              data-empty-space-blocking='true'
              onClick={(event) => event.stopPropagation()}
              role='menu'
              style={{
                left: `${switchAccountSubmenuPosition.x}px`,
                top: `${switchAccountSubmenuPosition.y}px`,
                zIndex: 'var(--sidebar-context-menu-submenu-z-index, 301)',
              }}
            >
              <div className='session-context-menu-section'>
                {switchableAgents.map((row) => (
                  <button
                    aria-label={`Switch to ${row.name}`}
                    className='session-context-menu-item session-switch-account-item'
                    key={row.agentId}
                    onClick={() => requestSwitchSessionAgent(row.agentId)}
                    role='menuitem'
                    type='button'
                  >
                    <SessionChatHostActionAgentIcon icon={row.icon} />
                    <span className='session-tag-menu-item-label'>{row.name}</span>
                  </button>
                ))}
              </div>
            </div>,
            document.body
          )
        : null}
    </>
  );
}

function getSessionRenameInitialTitle(session: SidebarSessionItem): string {
  return session.primaryTitle?.trim() || session.terminalTitle?.trim() || session.alias;
}

export function canSleepSidebarSession(session: SidebarSessionItem | undefined): boolean {
  /*
  CDXC:ContextMenus 2026-06-07-13:34:
  Sleep below targets every running session, including browser panes. Stopped
  history can remain visible when pinned, tagged, or favorited, but sleeping it
  would reactivate that history as a sleeping sidebar row.
  */
  return session !== undefined && getSidebarSessionLifecycleState(session) === 'running';
}

export function canWakeSidebarSession(session: SidebarSessionItem | undefined): boolean {
  /*
   * CDXC:Sessions 2026-07-01-18:33:
   * Wake selected mirrors Sleep selected and targets only rows that are
   * actually parked or sleeping, avoiding no-op wake messages for active
   * terminal, agent, and browser sessions.
   */
  return session !== undefined && getSidebarSessionLifecycleState(session) === 'sleeping';
}

function getSidebarBulkSessionContextMenuAvailability({
  sessionIds,
  sessionsById,
}: {
  sessionIds: readonly string[];
  sessionsById: Record<string, SidebarSessionItem | undefined>;
}): SidebarBulkSessionContextMenuAvailability {
  /*
   * CDXC:Sessions 2026-07-01-18:33:
   * Bulk session context menus should show only actions that can run over the
   * current selected rows without guessing. Filter each action to eligible
   * concrete sessions and let the action handler target exactly that subset.
   */
  const concreteSessionIds: string[] = [];
  const seenSessionIds = new Set<string>();
  for (const sessionId of sessionIds) {
    if (seenSessionIds.has(sessionId) || !sessionsById[sessionId]) {
      continue;
    }
    seenSessionIds.add(sessionId);
    concreteSessionIds.push(sessionId);
  }

  const sessionForId = (sessionId: string) => sessionsById[sessionId];
  return {
    closableSessionIds: concreteSessionIds,
    fullReloadableSessionIds: concreteSessionIds.filter((sessionId) =>
      supportsSelectedSessionFullReload(sessionForId(sessionId), sessionId)
    ),
    pinnableSessionIds: concreteSessionIds.filter((sessionId) => sessionForId(sessionId)?.isPinned !== true),
    sleepableSessionIds: concreteSessionIds.filter((sessionId) => canSleepSidebarSession(sessionForId(sessionId))),
    taggableSessionIds: concreteSessionIds.filter((sessionId) => canTagSelectedSidebarSession(sessionForId(sessionId))),
    unpinnableSessionIds: concreteSessionIds.filter((sessionId) => sessionForId(sessionId)?.isPinned === true),
    wakeableSessionIds: concreteSessionIds.filter((sessionId) => canWakeSidebarSession(sessionForId(sessionId))),
  };
}

function getSidebarBulkSessionContextMenuCounts({
  availability,
  hasSessionTagSubmenu,
}: {
  availability: SidebarBulkSessionContextMenuAvailability;
  hasSessionTagSubmenu: boolean;
}): { dividerCount: number; itemCount: number; labelCount: number } {
  const primaryItemCount =
    Number(availability.sleepableSessionIds.length > 0) +
    Number(availability.wakeableSessionIds.length > 0) +
    Number(hasSessionTagSubmenu && availability.taggableSessionIds.length > 0) +
    Number(availability.pinnableSessionIds.length > 0) +
    Number(availability.unpinnableSessionIds.length > 0) +
    Number(availability.fullReloadableSessionIds.length > 0);
  const destructiveItemCount = Number(availability.closableSessionIds.length > 0);
  const sectionLengths = [primaryItemCount, destructiveItemCount].filter((count) => count > 0);
  return {
    dividerCount: Math.max(0, sectionLengths.length - 1),
    itemCount: sectionLengths.reduce((count, sectionLength) => count + sectionLength, 0),
    /* The bulk menu renders no group headings; see contextMenuSections. */
    labelCount: 0,
  };
}

function canTagSelectedSidebarSession(session: SidebarSessionItem | undefined): boolean {
  return Boolean(session) && !isSidebarBrowserSession(session);
}

function getSharedSelectedSidebarSessionTag({
  sessionIds,
  sessionsById,
}: {
  sessionIds: readonly string[];
  sessionsById: Record<string, SidebarSessionItem | undefined>;
}): SidebarSessionTag | undefined {
  /*
   * CDXC:Sessions 2026-07-01-18:45:
   * Bulk tag menus must only show a checked tag when every taggable selected
   * session shares that tag. Mixed selections should apply the clicked tag
   * instead of implying the right-clicked row represents the whole selection.
   */
  let hasReferenceTag = false;
  let referenceTag: SidebarSessionTag | undefined;
  for (const sessionId of sessionIds) {
    const session = sessionsById[sessionId];
    if (!session) {
      continue;
    }

    const sessionTag = getEffectiveSessionTag(session);
    if (!hasReferenceTag) {
      referenceTag = sessionTag;
      hasReferenceTag = true;
      continue;
    }

    if (sessionTag !== referenceTag) {
      return undefined;
    }
  }

  return referenceTag;
}

function supportsSelectedSessionFullReload(session: SidebarSessionItem | undefined, sessionId: string): boolean {
  if (!session || isSidebarBrowserSession(session)) {
    return false;
  }

  if (isRemotePresentationSidebarSessionId(sessionId)) {
    return session.sessionKind === 'terminal';
  }

  return supportsFullReload(session);
}

function isRemotePresentationSidebarSessionId(sessionId: string): boolean {
  return /^remote:[^:]+:session:[^:]+:.+$/u.test(sessionId);
}

function supportsResumeCommandCopy(session: SidebarSessionItem): boolean {
  /**
   * CDXC:SessionSleep 2026-04-27-08:04
   * Match agent-tiler context-menu visibility: Copy resume is only shown for
   * built-in agents with known resume or resume-selection CLI behavior.
   *
   * CDXC:AgentProviders 2026-05-20-08:20:
   * Cursor resume uses stored chat UUIDs or a local title lookup fallback, so
   * Cursor CLI cards expose the same copy-resume affordance as Codex and Pi.
   */
  return (
    session.agentIcon === 'codex' ||
    session.agentIcon === 'claude' ||
    session.agentIcon === 'copilot' ||
    session.agentIcon === 'gemini' ||
    session.agentIcon === 'opencode' ||
    session.agentIcon === 'pi' ||
    session.agentIcon === 'cursor-cli' ||
    session.agentIcon === 'antigravity-cli'
  );
}

function gpuiWorkspaceTerminalFocusBridgeAvailable(): boolean {
  if (typeof window === 'undefined') {
    return false;
  }
  const bridge = (window as { ghostexGpui?: { postWorkspaceTerminalFocus?: unknown } }).ghostexGpui;
  return typeof bridge?.postWorkspaceTerminalFocus === 'function';
}

function supportsFork(session: SidebarSessionItem): boolean {
  /**
   * CDXC:AgentProviders 2026-05-08-09:42
   * Pi exposes a real `--fork <session>` CLI path once ghostex has captured the
   * Pi session id/path, so Pi cards should show the same one-click Fork action
   * as Codex in the session context menu.
   */
  return session.agentIcon === 'codex' || session.agentIcon === 'claude' || session.agentIcon === 'pi';
}

function supportsTranscriptExport(session: SidebarSessionItem): boolean {
  return resolveSessionChatTranscriptAgent(session.agentName, session.agentIcon) !== null;
}

function supportsGeneratedName(session: SidebarSessionItem): boolean {
  /**
   * CDXC:AgentProviders 2026-05-08-16:18
   * Pi cards should expose the same right-click Generate Title action as Codex
   * once the first user message has been captured. The native rename path
   * already switches Pi to `/name <title>`, so the menu gate should include Pi
   * instead of creating a Pi-only title-generation command.
   *
   * Antigravity takes `/rename <title>`, the default rename command.
   */
  return (
    session.agentIcon === 'codex' ||
    session.agentIcon === 'claude' ||
    session.agentIcon === 'pi' ||
    session.agentIcon === 'antigravity-cli'
  );
}

function supportsDelayedSendMenuAction(session: SidebarSessionItem, isRemoteSession: boolean): boolean {
  if (isRemoteSession) {
    return session.canScheduleDelayedSend === true;
  }

  return true;
}

function supportsCloseAfterDoneMenuAction(session: SidebarSessionItem, isRemoteSession: boolean): boolean {
  if (isRemoteSession) {
    return session.canToggleCloseAfterDone === true;
  }

  return true;
}

function supportsFullReloadMenuAction(session: SidebarSessionItem, isRemoteSession: boolean): boolean {
  if (isRemoteSession) {
    return session.sessionKind === 'terminal';
  }

  return supportsFullReload(session);
}

function supportsPopOutPaneMenuAction(
  session: SidebarSessionItem,
  {
    isBrowserSession,
    isRemoteSession,
  }: {
    isBrowserSession: boolean;
    isRemoteSession: boolean;
  }
): boolean {
  if (isRemoteSession) {
    return session.canPopOutPane === true && getSidebarSessionLifecycleState(session) === 'running';
  }

  return supportsPopOutPane(session, isBrowserSession);
}

function supportsPopOutPane(session: SidebarSessionItem, isBrowserSession: boolean): boolean {
  /**
   * CDXC:Workarea 2026-05-19-10:15:
   * Sidebar context menus expose pop-out for browser panes and agent terminal
   * sessions. Sleeping sessions dispose their native surface and cannot remain
   * in a detached window.
   */
  if (getSidebarSessionLifecycleState(session) !== 'running') {
    return false;
  }

  if (isBrowserSession) {
    return true;
  }

  return session.sessionKind === 'terminal' && Boolean(session.agentIcon);
}

function supportsFullReload(session: SidebarSessionItem): boolean {
  /**
   * CDXC:SessionSleep 2026-04-27-08:04
   * Match agent-tiler context-menu visibility: Full reload is only shown for
   * agent sessions that can be recreated and resumed programmatically.
   *
   * CDXC:AgentProviders 2026-05-08-16:18
   * Pi has a restorable CLI identity through its captured session id/path, so
   * right-click Full reload should be visible on Pi cards like it is for Codex.
   *
   * CDXC:AgentProviders 2026-05-20-08:20:
   * Cursor cards can full-reload through stored chat UUIDs or trusted titles
   * resolved from the local Cursor chat store for the active project.
   *
   * CDXC:AgentProviders 2026-09-03:
   * Antigravity resumes only by conversation id (`agy --conversation <id>`),
   * which its hooks report; without one a reload could only start a fresh
   * conversation, so the card shows Full reload once the id is captured.
   */
  if (session.agentIcon === 'antigravity-cli') {
    return Boolean(session.agentSessionId?.trim());
  }
  return (
    session.agentIcon === 'codex' ||
    session.agentIcon === 'claude' ||
    session.agentIcon === 'opencode' ||
    session.agentIcon === 'pi' ||
    session.agentIcon === 'cursor-cli'
  );
}

function postSidebarAgentIconRenderDebugLog(vscode: WebviewApi, event: string, details: Record<string, unknown>): void {
  vscode.postMessage({
    details,
    event,
    scenarioId: 'native.agent.detection',
    type: 'sidebarDebugLog',
  });
}

function findSessionCardElement(sessionId: string): HTMLElement | undefined {
  return Array.from(document.querySelectorAll<HTMLElement>('[data-sidebar-session-id]')).find(
    (element) => element.dataset.sidebarSessionId === sessionId
  );
}

function summarizeAgentIconElement(element: HTMLElement | null | undefined) {
  if (!element) {
    return undefined;
  }

  const styles = window.getComputedStyle(element);
  const bounds = element.getBoundingClientRect();
  return {
    className: typeof element.className === 'string' ? element.className : String(element.getAttribute('class') ?? ''),
    dataDefaultTrailingDisplay: element.dataset.defaultTrailingDisplay,
    dataHasAgentIcon: element.dataset.hasAgentIcon,
    dataHoverTrailingDisplay: element.dataset.hoverTrailingDisplay,
    display: styles.display,
    height: Math.round(bounds.height * 100) / 100,
    opacity: styles.opacity,
    visibility: styles.visibility,
    width: Math.round(bounds.width * 100) / 100,
  };
}

let sidebarDebugInstanceCounter = 0;

function createSidebarDebugInstanceId(): number {
  sidebarDebugInstanceCounter += 1;
  return sidebarDebugInstanceCounter;
}
