import {
  IconChevronRight,
  IconCheck,
  IconCopy,
  IconCode,
  IconClock,
  IconDeviceMobile,
  IconDownload,
  IconExternalLink,
  IconFocus2,
  IconGitFork,
  IconHandFinger,
  IconLayoutSidebarRightExpand,
  IconMessageCircle,
  IconMoon,
  IconPencil,
  IconPinned,
  IconPinnedOff,
  IconPlayerPlay,
  IconRefresh,
  IconSparkles,
  IconTag,
  IconUserCircle,
  IconX,
} from "@tabler/icons-react";
import { KeyboardSensor, PointerActivationConstraints, PointerSensor } from "@dnd-kit/dom";
import { SortableKeyboardPlugin } from "@dnd-kit/dom/sortable";
import { useDroppable } from "@dnd-kit/react";
import { useSortable } from "@dnd-kit/react/sortable";
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
} from "react";
import { useShallow } from "zustand/react/shallow";
import {
  getSidebarSessionLifecycleState,
  type SidebarSessionItem,
} from "../shared/session-grid-contract";
import { DEFAULT_ghostex_SETTINGS, type BrowserFeedbackTool } from "../shared/ghostex-settings";
import { buildSidebarSessionDetailsClipboardText } from "../shared/session-details-copy";
import {
  getSessionCardTitleTooltip,
  OverflowTooltipText,
  SessionCardContent,
  SessionFloatingAgentIcon,
  shouldShowTerminalSessionIcon,
} from "./session-card-content";
import { getSessionStatusAnchorName } from "./session-status-anchor";
import {
  createSessionDragData,
  createSessionDropTargetData,
  createSessionDropTargetId,
} from "./sidebar-dnd";
import { openAppModal } from "./app-modal-host-bridge";
import { SidebarContextMenuPortal } from "./sidebar-context-menu-portal";
import { useSidebarStore } from "./sidebar-store";
import {
  getEffectiveSessionTag,
  getSidebarSessionTagLabel,
  SessionTagIcon,
  SIDEBAR_SESSION_TAG_OPTIONS,
  SIDEBAR_SESSION_TAG_SECTIONS,
  type SidebarSessionTag,
} from "./session-tag-ui";
import type { WebviewApi } from "./webview-api";
import { createPortal, flushSync } from "react-dom";

const CONTEXT_MENU_MARGIN_PX = 12;
const CONTEXT_MENU_WIDTH_PX = 178;
const CONTEXT_MENU_ITEM_HEIGHT_PX = 34;
const CONTEXT_MENU_DIVIDER_HEIGHT_PX = 13;
const CONTEXT_MENU_VERTICAL_PADDING_PX = 12;
const SESSION_CARD_DRAG_HOLD_DELAY_MS = 130;
const SESSION_CARD_DRAG_HOLD_TOLERANCE_PX = 12;
const TOUCH_SESSION_CARD_DRAG_HOLD_DELAY_MS = 130;
const TOUCH_SESSION_CARD_DRAG_HOLD_TOLERANCE_PX = 12;
const COMPLETION_FLASH_DURATION_MS = 3_000;
const DND_SESSION_CARD_AX_ATTRIBUTES = [
  "aria-describedby",
  "aria-disabled",
  "aria-grabbed",
  "aria-pressed",
  "aria-roledescription",
] as const;
const DND_SESSION_FRAME_AX_ATTRIBUTES = [
  ...DND_SESSION_CARD_AX_ATTRIBUTES,
  "role",
  "tabindex",
] as const;

function getBrowserFeedbackToolLabel(tool: BrowserFeedbackTool): string {
  return tool === "agentation" ? "Agentation" : "React Grab";
}

const sessionCardSensors = [
  PointerSensor.configure({
    activationConstraints(event) {
      if (event.pointerType === "touch") {
        return [
          new PointerActivationConstraints.Delay({
            tolerance: TOUCH_SESSION_CARD_DRAG_HOLD_TOLERANCE_PX,
            value: TOUCH_SESSION_CARD_DRAG_HOLD_DELAY_MS,
          }),
        ];
      }

      return [
        new PointerActivationConstraints.Delay({
          tolerance: SESSION_CARD_DRAG_HOLD_TOLERANCE_PX,
          value: SESSION_CARD_DRAG_HOLD_DELAY_MS,
        }),
      ];
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
  submenu?: "session-tags";
};

export type SortableSessionCardProps = {
  completionFlashNonce?: number;
  dragDisabled?: boolean;
  dropDisabled?: boolean;
  forcedDropPosition?: "before" | "after";
  groupId: string;
  index: number;
  isProjectSessionListOverflowRow?: boolean;
  isSearchSelected?: boolean;
  onFocusRequested?: (groupId: string, sessionId: string) => void;
  sessionIdsBelow?: readonly string[];
  sessionId: string;
  showGroupDropTargetChrome?: boolean;
  showGroupConnector?: boolean;
  showDropPositionIndicator?: boolean;
  vscode: WebviewApi;
};

export function resolveSessionCardSessionIdsBelow({
  contextMenuSessionIdsBelow,
  isContextMenuOpen,
  sessionIdsBelow,
}: {
  contextMenuSessionIdsBelow: readonly string[];
  isContextMenuOpen: boolean;
  sessionIdsBelow: readonly string[];
}): readonly string[] {
  /*
   * CDXC:SidebarContextMenu 2026-06-10-10:01:
   * Session-card below actions receive the current group/project slice from
   * SessionGroupSection. Keep the menu target list scoped to that slice instead
   * of deriving cross-project targets from global rendered sidebar rows.
   */
  return isContextMenuOpen ? contextMenuSessionIdsBelow : sessionIdsBelow;
}

export function getSessionCardAccessibleLabel({
  isFocused,
  title,
}: {
  isFocused: boolean;
  title: string;
}): string {
  const fallbackTitle = title.trim() || "Session";
  return isFocused ? `${fallbackTitle}, current session` : fallbackTitle;
}

export type SidebarBulkContextMenuScheduler = (operation: () => void) => void;

/**
 * CDXC:SidebarContextMenu 2026-06-07-13:00:
 * Sleep below and Close below can fan out to many native lifecycle messages.
 * Run those targets from scheduled background tasks so the menu click returns
 * immediately, the context menu can dismiss before lifecycle work starts, and
 * the sidebar remains responsive between target operations.
 *
 * CDXC:SidebarContextMenu 2026-06-07-13:09:
 * Bulk lifecycle menu items should still close like normal menu actions. Only
 * the fan-out work runs in the background; menu visibility should not be used
 * as an operation progress indicator.
 */
export function runSidebarBulkContextMenuActionInBackground(
  sessionIds: readonly string[],
  runForSessionId: (sessionId: string) => void,
  scheduler: SidebarBulkContextMenuScheduler = (operation) => {
    globalThis.setTimeout(operation, 0);
  },
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

function postSidebarSessionCloseInBackground(
  vscode: WebviewApi,
  sessionId: string,
): void {
  /*
  CDXC:LocalFirstSidebar 2026-06-12-06:22:
  Native sidebar message delivery is synchronous in the macOS host. Close clicks must flush the local card removal before asking the host to tear down the terminal/browser/T3 runtime, otherwise closeTerminal work can block the same user gesture and make the sidebar feel delayed.
  */
  globalThis.setTimeout(() => {
    vscode.postMessage({
      sessionId,
      type: "closeSession",
    });
  }, 0);
}

function postSidebarSessionsCloseInBackground(
  vscode: WebviewApi,
  sessionIds: readonly string[],
): void {
  globalThis.setTimeout(() => {
    vscode.postMessage({
      sessionIds: [...sessionIds],
      type: "closeSessions",
    });
  }, 0);
}

function clampContextMenuPosition(
  clientY: number,
  itemCount: number,
  dividerCount: number,
): ContextMenuPosition {
  const menuHeight =
    CONTEXT_MENU_VERTICAL_PADDING_PX +
    itemCount * CONTEXT_MENU_ITEM_HEIGHT_PX +
    dividerCount * CONTEXT_MENU_DIVIDER_HEIGHT_PX;
  return {
    x: getCenteredSidebarMenuX(CONTEXT_MENU_WIDTH_PX),
    y: Math.max(
      CONTEXT_MENU_MARGIN_PX,
      Math.min(clientY, window.innerHeight - menuHeight - CONTEXT_MENU_MARGIN_PX),
    ),
  };
}

function getCenteredSidebarMenuX(menuWidth: number): number {
  /*
   * CDXC:SidebarContextMenu 2026-06-05-21:23:
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
  sessionIdsBelow = [],
  sessionId,
  showGroupDropTargetChrome = true,
  showGroupConnector = false,
  showDropPositionIndicator = true,
  vscode,
}: SortableSessionCardProps) {
  const [contextMenuPosition, setContextMenuPosition] = useState<ContextMenuPosition>();
  const [contextMenuSessionIdsBelow, setContextMenuSessionIdsBelow] = useState<readonly string[]>([]);
  const effectiveSessionIdsBelow = resolveSessionCardSessionIdsBelow({
    contextMenuSessionIdsBelow,
    isContextMenuOpen: Boolean(contextMenuPosition),
    sessionIdsBelow,
  });
  const session = useSidebarStore((state) => state.sessionsById[sessionId]);
  const sleepableSessionIdsBelow = useSidebarStore(
    useShallow((state) =>
      effectiveSessionIdsBelow.filter((candidateSessionId) =>
        canSleepSidebarSession(state.sessionsById[candidateSessionId]),
      ),
    ),
  );
  const canFocusMode = useSidebarStore((state) => state.groupsById[groupId]?.canFocusMode === true);
  const shouldKeepLastProjectSessionVisibleOnClose = useSidebarStore(
    useShallow((state) => {
      const group = state.groupsById[groupId];
      const groupSessionIds = state.sessionIdsByGroup[groupId] ?? [];
      return (
        group?.projectContext !== undefined &&
        group.isChatCollection !== true &&
        groupSessionIds.length === 1 &&
        groupSessionIds[0] === sessionId
      );
    }),
  );
  const {
    hideSessionAgentIconUntilHover,
    hideBrowserFaviconUntilHover,
    browserFeedbackTool,
    showCloseButton,
    showDebugSessionNumbers,
    showLastActiveTime,
    showSessionCloseContextMenuAction,
    showSessionCommandCopyActions,
    showSessionDetailsCopyAction,
  } = useSidebarStore(
    useShallow((state) => ({
      /*
       * CDXC:SidebarSessions 2026-05-16-08:46:
       * The hover-only agent icon setting is visual chrome only; keep icons in
       * the DOM so the same row can reveal them on hover/focus without
       * changing session identity or drag hit targets.
       */
      hideSessionAgentIconUntilHover:
        state.hud.settings?.hideSessionAgentIconUntilHover ??
        DEFAULT_ghostex_SETTINGS.hideSessionAgentIconUntilHover,
      /*
       * CDXC:BrowserPanes 2026-05-28-07:38:
       * Browser favicons identify pages and need their own hover-only setting
       * instead of being suppressed by the agent-logo hover preference.
       */
      hideBrowserFaviconUntilHover:
        state.hud.settings?.hideBrowserFaviconUntilHover ??
        DEFAULT_ghostex_SETTINGS.hideBrowserFaviconUntilHover,
      browserFeedbackTool:
        state.hud.settings?.browserFeedbackTool ?? DEFAULT_ghostex_SETTINGS.browserFeedbackTool,
      showCloseButton: state.hud.showCloseButtonOnSessionCards,
      showDebugSessionNumbers: state.hud.debuggingMode,
      showLastActiveTime:
        !(state.hud.settings?.hideLastActiveTimeOnSessionCards ??
          DEFAULT_ghostex_SETTINGS.hideLastActiveTimeOnSessionCards),
      /*
       * CDXC:SidebarContextMenu 2026-06-10-13:58:
       * The destructive single-session Close item is hidden unless Settings
       * explicitly enables close actions in session context menus.
       */
      showSessionCloseContextMenuAction:
        state.hud.settings?.showSessionCloseContextMenuAction ??
        DEFAULT_ghostex_SETTINGS.showSessionCloseContextMenuAction,
      /*
       * CDXC:SidebarContextMenu 2026-06-09-23:17:
       * Copy resume and Copy attach command are opt-in context-menu utilities.
       * Hide both by default and reveal them only when Settings explicitly
       * enables command-copy actions for session buttons.
       */
      showSessionCommandCopyActions:
        state.hud.settings?.showSessionCommandCopyActions ??
        DEFAULT_ghostex_SETTINGS.showSessionCommandCopyActions,
      /*
       * CDXC:SidebarContextMenu 2026-06-11-23:08:
       * Copy details is an opt-in metadata clipboard action. Gate the menu item
       * with its own Settings flag instead of tying it to shell command copying.
       */
      showSessionDetailsCopyAction:
        state.hud.settings?.showSessionDetailsCopyAction ??
        DEFAULT_ghostex_SETTINGS.showSessionDetailsCopyAction,
    })),
  );
  const sessionGroup = useSidebarStore((state) => state.groupsById[groupId]);
  const [tagSubmenuPosition, setTagSubmenuPosition] = useState<ContextMenuPosition>();
  const [completionFlashRunId, setCompletionFlashRunId] = useState(0);
  const menuRef = useRef<HTMLDivElement>(null);
  const aliasHeadingRef = useRef<HTMLDivElement>(null);
  const sessionFrameRef = useRef<HTMLDivElement | null>(null);
  const sessionCardRef = useRef<HTMLElement | null>(null);
  const debugInstanceIdRef = useRef(createSidebarDebugInstanceId());
  const lastAgentIconRenderDebugKeyRef = useRef<string | undefined>(undefined);
  const isBrowserSession = session?.sessionKind === "browser" || session?.kind === "browser";
  const isT3Session = session?.sessionKind === "t3";
  const canTagSession = !isBrowserSession;
  const canForkSession = session ? !isBrowserSession && supportsFork(session) : false;
  const canDelayedSend = session ? !isBrowserSession && !isT3Session : false;
  const canCopyResumeCommand = session
    ? showSessionCommandCopyActions && !isBrowserSession && supportsResumeCommandCopy(session)
    : false;
  const canCopyAttachCommand =
    showSessionCommandCopyActions &&
    !isBrowserSession &&
    Boolean(session?.sessionPersistenceProvider && session.sessionPersistenceName);
  const canCopySessionDetails = showSessionDetailsCopyAction;
  const canFullReloadSession = session ? !isBrowserSession && supportsFullReload(session) : false;
  const canPopOutPane = session ? supportsPopOutPane(session, isBrowserSession, isT3Session) : false;
  const canGenerateSessionTitle = session
    ? !isBrowserSession &&
      supportsGeneratedName(session) &&
      Boolean(session.firstUserMessage?.trim())
    : false;
  const canSleepSession = session ? !isBrowserSession : false;
  const postSessionDragDebugLog = useEffectEvent(
    (event: string, details: Record<string, unknown>) => {
      if (!showDebugSessionNumbers) {
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
        type: "sidebarDebugLog",
      });
    },
  );
  const sortable = useSortable({
    accept: "session",
    data: createSessionDragData(groupId, session.sessionId),
    disabled:
      isProjectSessionListOverflowRow ||
      dragDisabled ||
      isBrowserSession ||
      contextMenuPosition !== undefined,
    feedback: "clone",
    group: groupId,
    id: sessionId,
    index,
    plugins: [SortableKeyboardPlugin],
    sensors: sessionCardSensors,
    type: "session",
  });
  const isSessionReorderDisabled =
    isProjectSessionListOverflowRow || !session || dropDisabled || contextMenuPosition !== undefined;
  const beforeDropTarget = useDroppable({
    accept: "session",
    data: createSessionDropTargetData({
      groupId,
      kind: "session",
      position: "before",
      sessionId,
    }),
    disabled: isSessionReorderDisabled,
    id: createSessionDropTargetId({
      groupId,
      kind: "session",
      position: "before",
      sessionId,
    }),
  });
  const afterDropTarget = useDroppable({
    accept: "session",
    data: createSessionDropTargetData({
      groupId,
      kind: "session",
      position: "after",
      sessionId,
    }),
    disabled: isSessionReorderDisabled,
    id: createSessionDropTargetId({
      groupId,
      kind: "session",
      position: "after",
      sessionId,
    }),
  });
  const dropPosition = sortable.isDragging
    ? undefined
    : forcedDropPosition ??
      (beforeDropTarget.isDropTarget
        ? "before"
        : afterDropTarget.isDropTarget
          ? "after"
          : undefined);
  const visibleDropPosition = showDropPositionIndicator ? dropPosition : undefined;
  const isVisibleDropTarget = showDropPositionIndicator && Boolean(visibleDropPosition);
  const shouldShowGroupDropTargetChrome = showGroupDropTargetChrome && isVisibleDropTarget;

  if (!session) {
    return null;
  }

  const currentSessionTag = getEffectiveSessionTag(session);
  const sessionTitleTooltip = getSessionCardTitleTooltip({
    session,
    showDebugSessionNumbers,
  });
  const sessionAccessibleLabel = getSessionCardAccessibleLabel({
    isFocused: session.isFocused,
    title: sessionTitleTooltip.headingText,
  });
  const lifecycleState = getSidebarSessionLifecycleState(session);
  const showTerminalSessionIcon = shouldShowTerminalSessionIcon(session);
  const hasSessionCardIcon =
    session.isPinned === true ||
    Boolean(currentSessionTag) ||
    Boolean(session.delayedSendRemainingLabel) ||
    Boolean(session.agentIcon) ||
    showTerminalSessionIcon ||
    session.isReloading === true;
  const sessionAnchorStyle = {
    anchorName: getSessionStatusAnchorName(sessionId),
  } as CSSProperties;
  const setSessionFrameElement = useCallback(
    (element: HTMLDivElement | null) => {
      sessionFrameRef.current = element;
      sortable.ref(element);
    },
    [sortable],
  );
  const setSessionCardElement = useCallback(
    (element: HTMLElement | null) => {
      sessionCardRef.current = element;
      sortable.sourceRef(element);
    },
    [sortable],
  );

  useEffect(() => {
    setContextMenuPosition(undefined);
    setTagSubmenuPosition(undefined);
  }, [session.alias, session.sessionId]);

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

    const scrubDndAccessibilityAttributes = (target: {
      attributes: readonly string[];
      element: HTMLElement;
    }) => {
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
              mutation.type === "attributes" &&
              mutation.attributeName !== null &&
              target.attributes.includes(mutation.attributeName),
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
    postSessionDragDebugLog("session.cardMounted", {
      dropPosition,
      isBrowserSession,
    });

    return () => {
      postSessionDragDebugLog("session.cardUnmounted", {
        dropPosition,
        isBrowserSession,
      });
    };
  }, [isBrowserSession, postSessionDragDebugLog]);

  useEffect(() => {
    postSessionDragDebugLog("session.dropPositionChanged", {
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
    const showHeaderLoadingSpinner =
      session.isReloading === true || session.isGeneratingFirstPromptTitle === true;
    const hasHeaderAgentIcon =
      Boolean(session.agentIcon) || showTerminalSessionIcon || showHeaderLoadingSpinner;
    const defaultTrailingDisplay = hasHeaderAgentIcon
      ? "icon"
      : hasLastInteractionLabel
        ? "time"
        : "icon";
    const shouldKeepLoadingIconVisible = showHeaderLoadingSpinner && hasHeaderAgentIcon;
    const hoverTrailingDisplay = shouldKeepLoadingIconVisible
      ? "icon"
      : defaultTrailingDisplay === "icon"
        ? hasLastInteractionLabel
          ? "time"
          : "icon"
        : hasHeaderAgentIcon
          ? "icon"
          : "time";
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
     * CDXC:AgentDetection 2026-04-27-07:43
     * Agent identity is confirmed at the native/webview/store boundary. Log
     * the card render decision and actual DOM state so missing sidebar icons
     * can be traced without guessing at CSS or projection state.
     */
    postSidebarAgentIconRenderDebugLog(vscode, "sidebar.agentIcon.cardRenderState", {
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
      const frame = card?.closest<HTMLElement>(".session-frame");
      const trailing = card?.querySelector<HTMLElement>(".session-head-trailing");
      const headerIcon = card?.querySelector<HTMLElement>(
        ".session-header-agent-icon, .session-header-agent-tabler-icon, .session-header-reloading-icon",
      );
      const floatingIcon = frame?.querySelector<HTMLElement>(
        ".session-floating-agent-icon, .session-floating-agent-tabler-icon, .session-floating-reloading-icon",
      );

      postSidebarAgentIconRenderDebugLog(vscode, "sidebar.agentIcon.cardDomState", {
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
    resolveSessionCardSessionIdsBelow({
      contextMenuSessionIdsBelow,
      isContextMenuOpen: Boolean(contextMenuPosition),
      sessionIdsBelow,
    });

  const getContextMenuCountsForSessionIdsBelow = (nextSessionIdsBelow: readonly string[]) => {
    const nextSleepableSessionIdsBelow = nextSessionIdsBelow.filter((candidateSessionId) =>
      canSleepSidebarSession(useSidebarStore.getState().sessionsById[candidateSessionId]),
    );
    const nextBelowActionCount =
      nextSessionIdsBelow.length > 0 ? 1 + Number(nextSleepableSessionIdsBelow.length > 0) : 0;
    const nextSectionLengths = [
      primaryActions.length,
      sessionActions.length,
      nextBelowActionCount,
      destructiveActions.length,
    ].filter((count) => count > 0);
    return {
      dividerCount: Math.max(0, nextSectionLengths.length - 1),
      itemCount: nextSectionLengths.reduce((count, sectionLength) => count + sectionLength, 0),
    };
  };

  const openContextMenu = (clientY: number) => {
    const nextSessionIdsBelow = readLatestSessionIdsBelow();
    const nextMenuCounts = getContextMenuCountsForSessionIdsBelow(nextSessionIdsBelow);
    setTagSubmenuPosition(undefined);
    setContextMenuSessionIdsBelow(nextSessionIdsBelow);
    setContextMenuPosition(
      clampContextMenuPosition(clientY, nextMenuCounts.itemCount, nextMenuCounts.dividerCount),
    );
  };

  const requestRename = () => {
    if (isBrowserSession) {
      return;
    }

    setContextMenuPosition(undefined);
    /**
     * CDXC:AppModals 2026-04-27-14:25
     * Rename must always use the full-window modal host. Missing host is an
     * error, not a reason to show the old squeezed sidebar dialog.
     */
    openAppModal({
      initialTitle: getSessionRenameInitialTitle(session),
      modal: "renameSession",
      sessionId: session.sessionId,
      type: "open",
    });
  };

  const requestClose = (
    source: "context-menu" | "middle-click" | "meta-click" | "programmatic",
  ) => {
    if (isT3Session && showDebugSessionNumbers) {
      vscode.postMessage({
        details: {
          activity: session.activity,
          groupId,
          isFocused: session.isFocused,
          isRunning: session.isRunning,
          isVisible: session.isVisible,
          requestedAt: Date.now(),
          sessionId: session.sessionId,
          source,
          title: session.primaryTitle,
        },
        event: "repro.t3CloseSession.requested",
        type: "sidebarDebugLog",
      });
    }

    flushSync(() => {
      setContextMenuPosition(undefined);
      if (shouldKeepLastProjectSessionVisibleOnClose) {
        /*
        CDXC:LocalFirstSidebar 2026-06-01-20:52:
        Closing a project's final sidebar session parks it instead of removing it. Keep the card visible immediately and mark it sleeping locally so the project does not blink out before gxserver publishes the parked-session presentation.
        */
        useSidebarStore.getState().setSessionSleepingLocally(session.sessionId, true);
      } else {
        useSidebarStore.getState().hideSessionLocally(session.sessionId);
      }
    });
    postSidebarSessionCloseInBackground(vscode, session.sessionId);
  };

  const requestCopyResumeCommand = () => {
    setContextMenuPosition(undefined);
    vscode.postMessage({
      sessionId: session.sessionId,
      type: "copyResumeCommand",
    });
  };

  const requestCopyAttachCommand = () => {
    setContextMenuPosition(undefined);
    /**
     * CDXC:SessionPersistence 2026-05-07-20:32
     * Provider-backed tmux/zmx/zellij session cards expose the native attach
     * command alongside resume copying, using the stored provider/name pair
     * rather than the current global Settings provider.
     */
    vscode.postMessage({
      sessionId: session.sessionId,
      type: "copyAttachCommand",
    });
  };

  const requestCopySessionDetails = () => {
    setContextMenuPosition(undefined);
    vscode.postMessage({
      detailsText: buildSidebarSessionDetailsClipboardText(session, sessionGroup),
      sessionId: session.sessionId,
      type: "copySessionDetails",
    });
  };

  const requestForkSession = () => {
    setContextMenuPosition(undefined);
    vscode.postMessage({
      sessionId: session.sessionId,
      type: "forkSession",
    });
  };

  const requestFocusMode = () => {
    setContextMenuPosition(undefined);
    /**
     * CDXC:SessionFocusMode 2026-05-23-09:28:
     * Double-click and context-menu Focus should zoom the clicked session's
     * pane tab group rather than rename the session. Route through the
     * controller so it can switch to Agents mode and later restore the prior
     * Code/Git/Project surface on unfocus.
     */
    vscode.postMessage({
      sessionId: session.sessionId,
      type: "focusSessionMode",
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
      delayedSendDeadlineAt: session.delayedSendDeadlineAt,
      delayedSendRemainingLabel: session.delayedSendRemainingLabel,
      modal: "delayedSend",
      sessionId: session.sessionId,
      title: getSessionRenameInitialTitle(session),
      type: "open",
    });
  };

  const requestT3BrowserAccess = () => {
    if (!isT3Session) {
      return;
    }

    setContextMenuPosition(undefined);
    /**
     * CDXC:T3RemoteAccess 2026-05-02-00:57
     * T3 session cards expose Remote Access directly; the controller resolves
     * the share URL and the app modal host owns the centered QR dialog.
     */
    vscode.postMessage({
      sessionId: session.sessionId,
      type: "requestT3SessionBrowserAccess",
    });
  };

  const requestBrowserPaneAction = (
    action: "devtools" | "feedback-tool" | "profile-picker" | "import-settings",
  ) => {
    if (!isBrowserSession) {
      return;
    }

    setContextMenuPosition(undefined);
    /**
     * CDXC:BrowserPanes 2026-05-02-06:35
     * Browser-pane cards surface the browser-specific controls in their
     * context menu so the sidebar can reach native WebKit features while the
     * browser itself renders as a regular workspace pane.
     */
    vscode.postMessage({
      action,
      sessionId: session.sessionId,
      type: "runBrowserPaneAction",
    });
  };

  const requestGenerateSessionTitle = () => {
    const firstMessage = session.firstUserMessage?.trim();
    if (!firstMessage) {
      return;
    }

    setContextMenuPosition(undefined);
    /**
     * CDXC:SessionNaming 2026-05-08-10:54
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
      event: "session.generateTitle.clicked",
      type: "sidebarDebugLog",
    });
    vscode.postMessage({
      sessionId: session.sessionId,
      shouldGenerateTitle: true,
      title: firstMessage,
      type: "renameSession",
    });
  };

  const requestFullReloadSession = () => {
    setContextMenuPosition(undefined);
    vscode.postMessage({
      sessionId: session.sessionId,
      type: "fullReloadSession",
    });
  };

  const requestPopOutPane = () => {
    if (!canPopOutPane) {
      return;
    }

    setContextMenuPosition(undefined);
    /**
     * CDXC:PanePopOut 2026-05-19-10:15:
     * Browser and agent session cards expose Pop Out Pane in the sidebar context
     * menu. The native controller toggles presentation from the current session
     * record, matching the focused-pane hotkey and tab-bar overflow behavior.
     */
    vscode.postMessage({
      sessionId: session.sessionId,
      type: "popOutPane",
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
      modal: "firstUserMessage",
      title: getSessionRenameInitialTitle(session),
      type: "open",
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
      type: "setSessionSleeping",
    });
  };

  const requestSleepBelow = () => {
    const targetSessionIds = readLatestSessionIdsBelow().filter((candidateSessionId) =>
      canSleepSidebarSession(useSidebarStore.getState().sessionsById[candidateSessionId]),
    );
    if (targetSessionIds.length === 0) {
      return;
    }

    flushSync(() => {
      setContextMenuPosition(undefined);
    });
    vscode.postMessage({
      sessionIds: targetSessionIds,
      sleeping: true,
      type: "setSessionsSleeping",
    });
  };

  const requestCloseBelow = () => {
    const targetSessionIds = [...readLatestSessionIdsBelow()];
    if (targetSessionIds.length === 0) {
      return;
    }

    flushSync(() => {
      setContextMenuPosition(undefined);
      useSidebarStore.getState().hideSessionsLocally(targetSessionIds);
    });
    postSidebarSessionsCloseInBackground(vscode, targetSessionIds);
  };

  const requestSetSessionTag = (tag: SidebarSessionTag | undefined) => {
    setContextMenuPosition(undefined);
    setTagSubmenuPosition(undefined);
    vscode.postMessage({
      sessionId: session.sessionId,
      sessionTag: tag ?? null,
      type: "setSessionTag",
    });
  };

  const openSessionTagSubmenu = (event: ReactMouseEvent<HTMLButtonElement>) => {
    const bounds = event.currentTarget.getBoundingClientRect();
    const submenuWidth = 204;
    const submenuHeight =
      CONTEXT_MENU_VERTICAL_PADDING_PX +
      SIDEBAR_SESSION_TAG_OPTIONS.length * CONTEXT_MENU_ITEM_HEIGHT_PX +
      SIDEBAR_SESSION_TAG_SECTIONS.length * 18 +
      Math.max(0, SIDEBAR_SESSION_TAG_SECTIONS.length - 1) * 10;
    setTagSubmenuPosition({
      x: getCenteredSidebarMenuX(submenuWidth),
      y: Math.max(
        CONTEXT_MENU_MARGIN_PX,
        Math.min(bounds.bottom + 4, window.innerHeight - submenuHeight - CONTEXT_MENU_MARGIN_PX),
      ),
    });
  };

  const requestSetPinned = (pinned: boolean) => {
    setContextMenuPosition(undefined);
    vscode.postMessage({
      pinned,
      sessionId: session.sessionId,
      type: "setSessionPinned",
    });
  };

  const primaryActions: SessionContextMenuAction[] = [];
  if (!isBrowserSession) {
    primaryActions.push({
      icon: (
        <IconPencil
          aria-hidden="true"
          className="session-context-menu-icon"
          size={16}
          stroke={1.8}
        />
      ),
      key: "rename",
      label: "Rename",
      onClick: requestRename,
    });
  }
  primaryActions.push({
    icon: session.isPinned ? (
      <IconPinnedOff
        aria-hidden="true"
        className="session-context-menu-icon"
        size={16}
        stroke={1.8}
      />
    ) : (
      <IconPinned
        aria-hidden="true"
        className="session-context-menu-icon"
        size={16}
        stroke={1.8}
      />
    ),
    /**
     * CDXC:PinnedSessions 2026-05-28-12:04:
     * Pinning is a live sidebar-order control, not Favorite. Expose it as its
     * own context-menu action so users can pin any project session without
     * changing previous-session favorites or auto-sleep favorite rules.
     */
    key: "pin",
    label: session.isPinned ? "Unpin" : "Pin",
    onClick: () => requestSetPinned(!session.isPinned),
  });
  if (canTagSession) {
    primaryActions.push({
      icon: (
        <IconTag aria-hidden="true" className="session-context-menu-icon" size={16} stroke={1.8} />
      ),
      key: "tag-as",
      label: "Tag as",
      onClick: openSessionTagSubmenu,
      submenu: "session-tags",
    });
  }
  if (canSleepSession) {
    primaryActions.push({
      icon: session.isSleeping ? (
        <IconPlayerPlay
          aria-hidden="true"
          className="session-context-menu-icon"
          size={16}
          stroke={1.8}
        />
      ) : (
        <IconMoon aria-hidden="true" className="session-context-menu-icon" size={16} stroke={1.8} />
      ),
      key: "sleep",
      label: session.isSleeping ? "Wake" : "Sleep",
      onClick: () => requestSetSleeping(!session.isSleeping),
    });
  }

  const belowActions: SessionContextMenuAction[] = [];
  if (effectiveSessionIdsBelow.length > 0) {
    /**
     * CDXC:SidebarContextMenu 2026-06-04-23:40:
     * Session row context menus expose below-scoped lifecycle actions only
     * when the clicked row has visible sessions beneath it. Sleep below targets
     * sleepable terminal/agent rows, while Close below removes every visible
     * row beneath the clicked session in the current sidebar order.
     *
     * CDXC:SidebarContextMenu 2026-06-10-10:01:
     * Sleep below is scoped to the clicked session's current project/group, not
     * every rendered row lower in the sidebar. Do not paint rows as sleeping
     * before native/gxserver confirms the zmx provider was actually stopped.
     */
    if (sleepableSessionIdsBelow.length > 0) {
      belowActions.push({
        icon: (
          <IconMoon
            aria-hidden="true"
            className="session-context-menu-icon"
            size={16}
            stroke={1.8}
          />
        ),
        key: "sleep-below",
        label: "Sleep below",
        onClick: requestSleepBelow,
      });
    }
    belowActions.push({
      danger: true,
      icon: (
        <IconX aria-hidden="true" className="session-context-menu-icon" size={16} stroke={1.8} />
      ),
      key: "close-below",
      label: "Close below",
      onClick: requestCloseBelow,
    });
  }

  const feedbackToolLabel = getBrowserFeedbackToolLabel(browserFeedbackTool);
  const sessionActions: SessionContextMenuAction[] = [];
  if (isBrowserSession) {
    sessionActions.push(
      {
        icon: (
          <IconCode
            aria-hidden="true"
            className="session-context-menu-icon"
            size={16}
            stroke={1.8}
          />
        ),
        key: "browser-devtools",
        label: "DevTools",
        onClick: () => requestBrowserPaneAction("devtools"),
      },
      {
        icon: (
          <IconHandFinger
            aria-hidden="true"
            className="session-context-menu-icon"
            size={16}
            stroke={1.8}
          />
        ),
        key: "browser-feedback-tool",
        label: feedbackToolLabel,
        onClick: () => requestBrowserPaneAction("feedback-tool"),
      },
      {
        icon: (
          <IconUserCircle
            aria-hidden="true"
            className="session-context-menu-icon"
            size={16}
            stroke={1.8}
          />
        ),
        key: "browser-profile",
        label: "Profile",
        onClick: () => requestBrowserPaneAction("profile-picker"),
      },
      {
        icon: (
          <IconDownload
            aria-hidden="true"
            className="session-context-menu-icon"
            size={16}
            stroke={1.8}
          />
        ),
        key: "browser-import",
        label: "Import Settings",
        onClick: () => requestBrowserPaneAction("import-settings"),
      },
    );
  }
  if (session.firstUserMessage?.trim()) {
    sessionActions.push({
      icon: (
        <IconMessageCircle
          aria-hidden="true"
          className="session-context-menu-icon"
          size={16}
          stroke={1.8}
        />
      ),
      key: "view-first-message",
      label: "View 1st message",
      onClick: requestViewFirstUserMessage,
    });
  }
  if (isT3Session) {
    sessionActions.push({
      icon: (
        <IconDeviceMobile
          aria-hidden="true"
          className="session-context-menu-icon"
          size={16}
          stroke={1.8}
        />
      ),
      key: "browser-access",
      label: "Remote Access",
      onClick: requestT3BrowserAccess,
    });
  }
  if (canCopyResumeCommand) {
    sessionActions.push({
      icon: (
        <IconCopy aria-hidden="true" className="session-context-menu-icon" size={16} stroke={1.8} />
      ),
      key: "copy-resume",
      label: "Copy resume",
      onClick: requestCopyResumeCommand,
    });
  }
  if (canCopyAttachCommand) {
    sessionActions.push({
      icon: (
        <IconCopy aria-hidden="true" className="session-context-menu-icon" size={16} stroke={1.8} />
      ),
      key: "copy-attach",
      label: "Copy attach command",
      onClick: requestCopyAttachCommand,
    });
  }
  if (canCopySessionDetails) {
    sessionActions.push({
      icon: (
        <IconCopy aria-hidden="true" className="session-context-menu-icon" size={16} stroke={1.8} />
      ),
      key: "copy-details",
      label: "Copy details",
      onClick: requestCopySessionDetails,
    });
  }
  if (canDelayedSend) {
    sessionActions.push({
      icon: (
        <IconClock
          aria-hidden="true"
          className="session-context-menu-icon"
          size={16}
          stroke={1.8}
        />
      ),
      key: "delayed-send",
      label: "Delayed Send",
      onClick: requestDelayedSend,
    });
  }
  if (canForkSession) {
    sessionActions.push({
      icon: (
        <IconGitFork
          aria-hidden="true"
          className="session-context-menu-icon"
          size={16}
          stroke={1.8}
        />
      ),
      key: "fork",
      label: "Fork",
      onClick: requestForkSession,
    });
  }
  if (canGenerateSessionTitle) {
    /**
     * CDXC:SessionNaming 2026-05-08-10:54
     * Claude and Codex thread cards need a direct "Generate Title" action that
     * retitles the session from the saved 1st user message. The action is only
     * useful once that message exists, because the controller intentionally
     * generates from real user text rather than from title fallbacks.
     */
    sessionActions.push({
      icon: (
        <IconSparkles
          aria-hidden="true"
          className="session-context-menu-icon"
          size={16}
          stroke={1.8}
        />
      ),
      key: "generate-title",
      label: "Generate Title",
      onClick: requestGenerateSessionTitle,
    });
  }
  if (canFullReloadSession) {
    sessionActions.push({
      icon: (
        <IconRefresh
          aria-hidden="true"
          className="session-context-menu-icon"
          size={16}
          stroke={1.8}
        />
      ),
      key: "full-reload",
      label: "Full reload",
      onClick: requestFullReloadSession,
    });
  }
  if (canFocusMode) {
    /**
     * CDXC:SessionFocusMode 2026-05-28-12:52:
     * Sidebar context-menu Focus should only appear when the group has split panes to zoom.
     * A single pane with multiple tabs still uses normal tab selection, so hiding Focus here keeps the menu aligned with double-click behavior.
     */
    sessionActions.push({
      icon: (
        <IconFocus2
          aria-hidden="true"
          className="session-context-menu-icon"
          size={16}
          stroke={1.8}
        />
      ),
      key: "focus-mode",
      label: "Focus",
      onClick: requestFocusMode,
    });
  }
  if (canPopOutPane) {
    sessionActions.push({
      icon: session.isPoppedOut ? (
        <IconLayoutSidebarRightExpand
          aria-hidden="true"
          className="session-context-menu-icon"
          size={16}
          stroke={1.8}
        />
      ) : (
        <IconExternalLink
          aria-hidden="true"
          className="session-context-menu-icon"
          size={16}
          stroke={1.8}
        />
      ),
      key: "pop-out-pane",
      label: session.isPoppedOut ? "Restore Pane" : "Pop Out Pane",
      onClick: requestPopOutPane,
    });
  }

  const destructiveActions: SessionContextMenuAction[] = [];
  if (showSessionCloseContextMenuAction) {
    destructiveActions.push({
      danger: true,
      icon: (
        <IconX aria-hidden="true" className="session-context-menu-icon" size={16} stroke={1.8} />
      ),
      /**
       * CDXC:SessionClose 2026-05-11-00:45
       * User-facing session removal language is Close. Keep the
       * destructive action behavior unchanged while making terminal, T3, and
       * browser context menus use the same visible verb.
       *
       * CDXC:SidebarContextMenu 2026-06-10-13:58:
       * The Close menu item is hidden by default and appears only when the
       * Session Cards setting opts into destructive close actions in menus.
       */
      key: "close",
      label: "Close",
      onClick: () => requestClose("context-menu"),
    });
  }
  const contextMenuSections = [primaryActions, sessionActions, belowActions, destructiveActions].filter(
    (section) => section.length > 0,
  );
  const contextMenuItemCount = contextMenuSections.reduce(
    (count, section) => count + section.length,
    0,
  );
  const contextMenuDividerCount = Math.max(0, contextMenuSections.length - 1);

  const requestFocusSession = (
    event?: ReactKeyboardEvent<HTMLElement> | ReactMouseEvent<HTMLElement>,
  ) => {
    const shouldAcknowledgeAttention = session.activity === "attention";
    /**
     * CDXC:SidebarSessionFocus 2026-05-15-20:01:
     * Intermittent sidebar-card clicks can select an existing session through a
     * newly synthesized native split. Persist the DOM click metadata, card
     * focus state, group id, and local-focus decision so a later repro can be
     * matched against native paneLayout resolution instead of guessing which
     * card action fired.
     */
    vscode.postMessage({
      details: {
        activity: session.activity,
        button: event && "button" in event ? event.button : undefined,
        clientX: event && "clientX" in event ? event.clientX : undefined,
        clientY: event && "clientY" in event ? event.clientY : undefined,
        clickDetail: event && "detail" in event ? event.detail : undefined,
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
      event: "repro.sidebarSessionFocusRequested",
      type: "sidebarDebugLog",
    });
    /*
     * CDXC:SidebarSessionFocus 2026-06-08-09:31:
     * Terminal switching should not wait behind local React focus rendering. Keep the forced focus breadcrumb first for native trace correlation, then send the authoritative focusSession command before applying the sidebar highlight locally; the following hydrate reconciles the UI after native focus/layout has started.
     */
    vscode.postMessage({ sessionId: session.sessionId, type: "focusSession" });
    if (!session.isFocused) {
      onFocusRequested?.(groupId, session.sessionId);
    }
  };

  const handleKeyDown = (event: ReactKeyboardEvent<HTMLElement>) => {
    if (isProjectSessionListOverflowRow) {
      event.preventDefault();
      event.stopPropagation();
      return;
    }

    if (event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")) {
      event.preventDefault();
      event.stopPropagation();
      const bounds = event.currentTarget.getBoundingClientRect();
      openContextMenu(bounds.top + 18);
      return;
    }

    if (event.key !== "Enter" && event.key !== " ") {
      return;
    }

    event.preventDefault();
    event.stopPropagation();
    requestFocusSession();
  };

  return (
    <>
      <OverflowTooltipText
        text={sessionTitleTooltip.headingText}
        textRef={aliasHeadingRef}
        tooltip={sessionTitleTooltip.tooltip}
        tooltipWhen={sessionTitleTooltip.tooltipWhen}
      >
        <div
          className="session-frame"
          data-activity={session.activity}
          data-dragging={String(Boolean(sortable.isDragging))}
          data-drop-position={visibleDropPosition}
          data-drop-target={String(shouldShowGroupDropTargetChrome)}
          data-focused={String(session.isFocused)}
          data-group-connector={String(showGroupConnector)}
          data-has-agent-icon={String(hasSessionCardIcon)}
          data-agent-icon-hover-only={String(hideSessionAgentIconUntilHover)}
          data-browser-favicon-hover-only={String(
            isBrowserSession && Boolean(session.faviconDataUrl) && hideBrowserFaviconUntilHover,
          )}
          data-lifecycle-state={lifecycleState}
          data-project-session-list-overflow={String(isProjectSessionListOverflowRow)}
          data-pinned={String(session.isPinned === true)}
          data-tagged={String(Boolean(currentSessionTag))}
          data-running={String(lifecycleState === "running")}
          data-sleeping={String(Boolean(session.isSleeping))}
          data-visible={String(session.isVisible)}
          ref={setSessionFrameElement}
        >
          <div
            aria-hidden
            className="session-drop-target-surface session-drop-target-surface-before"
            ref={beforeDropTarget.ref}
          />
          <div
            aria-hidden
            className="session-drop-target-surface session-drop-target-surface-after"
            ref={afterDropTarget.ref}
          />
          <article
            aria-current={session.isFocused ? "page" : undefined}
            aria-hidden={isProjectSessionListOverflowRow ? true : undefined}
            aria-label={sessionAccessibleLabel}
            className="session"
            data-activity={session.activity}
            data-completion-flash={
              completionFlashRunId > 0
                ? completionFlashRunId % 2 === 0
                  ? "even"
                  : "odd"
                : undefined
            }
            data-has-agent-icon={String(hasSessionCardIcon)}
            data-dragging={String(Boolean(sortable.isDragging))}
            data-drop-position={visibleDropPosition}
            data-drop-target={String(shouldShowGroupDropTargetChrome)}
            data-focused={String(session.isFocused)}
            data-group-connector={String(showGroupConnector)}
            data-lifecycle-state={lifecycleState}
            data-project-session-list-overflow={String(isProjectSessionListOverflowRow)}
            data-agent-icon-hover-only={String(hideSessionAgentIconUntilHover)}
            data-browser-favicon-hover-only={String(
              isBrowserSession && Boolean(session.faviconDataUrl) && hideBrowserFaviconUntilHover,
            )}
            data-running={String(lifecycleState === "running")}
            data-search-selected={String(isSearchSelected)}
            data-pinned={String(session.isPinned === true)}
            data-tagged={String(Boolean(currentSessionTag))}
            data-sleeping={String(Boolean(session.isSleeping))}
            data-sidebar-session-id={session.sessionId}
            data-visible={String(session.isVisible)}
            onPointerCancel={(event) => {
              postSessionDragDebugLog("session.pointerCancel", {
                button: event.button,
                buttons: event.buttons,
                clientX: event.clientX,
                clientY: event.clientY,
                pointerId: event.pointerId,
                pointerType: event.pointerType,
              });
            }}
            onPointerDown={(event) => {
              postSessionDragDebugLog("session.pointerDown", {
                button: event.button,
                buttons: event.buttons,
                clientX: event.clientX,
                clientY: event.clientY,
                isDragging: sortable.isDragging,
                pointerId: event.pointerId,
                pointerType: event.pointerType,
              });
            }}
            onPointerUp={(event) => {
              postSessionDragDebugLog("session.pointerUp", {
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
              if (isProjectSessionListOverflowRow) {
                event.preventDefault();
                event.stopPropagation();
                return;
              }

              if (event.button !== 1) {
                return;
              }

              event.preventDefault();
              requestClose("middle-click");
            }}
            onClick={(event) => {
              event.stopPropagation();

              if (isProjectSessionListOverflowRow) {
                event.preventDefault();
                return;
              }

              if (event.metaKey) {
                event.preventDefault();
                requestClose("meta-click");
                return;
              }

              requestFocusSession(event);
            }}
            onDoubleClick={(event) => {
              event.preventDefault();
              event.stopPropagation();
              if (isProjectSessionListOverflowRow) {
                return;
              }
              requestFocusMode();
            }}
            onContextMenu={(event: ReactMouseEvent<HTMLElement>) => {
              event.preventDefault();
              event.stopPropagation();
              if (isProjectSessionListOverflowRow) {
                return;
              }
              openContextMenu(event.clientY);
            }}
            onKeyDown={handleKeyDown}
            ref={setSessionCardElement}
            role="button"
            style={sessionAnchorStyle}
            tabIndex={isProjectSessionListOverflowRow ? -1 : 0}
          >
            <SessionFloatingAgentIcon
              agentIcon={session.agentIcon}
              delayedSendDeadlineAt={session.delayedSendDeadlineAt}
              delayedSendRemainingLabel={session.delayedSendRemainingLabel}
              faviconDataUrl={session.faviconDataUrl}
              isFavorite={session.isFavorite}
              isPinned={session.isPinned}
              isReloading={session.isReloading}
              onDelayedSendClick={requestDelayedSend}
              sessionTag={session.sessionTag}
              sessionPersistenceName={session.sessionPersistenceName}
              sessionPersistenceProvider={session.sessionPersistenceProvider}
              showTerminalIcon={showTerminalSessionIcon}
            />
            {/**
             * CDXC:SidebarSessions 2026-05-09-16:55
             * Project and chat session cards route the close-on-hover setting
             * through the same shared row across terminal, agent, T3 Code, and
             * browser panes.
             */}
            <SessionCardContent
              aliasHeadingRef={aliasHeadingRef}
              onDelayedSendClick={requestDelayedSend}
              onClose={() => requestClose("programmatic")}
              session={session}
              showDebugSessionNumbers={showDebugSessionNumbers}
              showCloseButton={showCloseButton}
              showLastActiveTime={showLastActiveTime}
            />
          </article>
          <div aria-hidden className="session-status-dot session-status-dot-inline" />
        </div>
      </OverflowTooltipText>
      {contextMenuPosition ? (
        <SidebarContextMenuPortal
          menuClassName="session-context-menu sidebar-session-context-menu"
          menuRef={menuRef}
          menuStyle={{
            left: `${contextMenuPosition.x}px`,
            top: `${contextMenuPosition.y}px`,
          }}
          onDismiss={() => {
            setContextMenuPosition(undefined);
            setTagSubmenuPosition(undefined);
          }}
          vscode={vscode}
        >
          {contextMenuSections.map((section, sectionIndex) => (
            <Fragment key={`section-${sectionIndex}`}>
              {sectionIndex > 0 ? (
                <div className="session-context-menu-divider" role="separator" />
              ) : null}
              <div className="session-context-menu-section">
                {section.map((action) => (
                  <button
                    key={action.key}
                    className={`session-context-menu-item${action.danger ? " session-context-menu-item-danger" : ""}`}
                    onClick={(event) => action.onClick(event)}
                    aria-expanded={
                      action.submenu === "session-tags"
                        ? Boolean(tagSubmenuPosition)
                        : undefined
                    }
                    aria-haspopup={action.submenu === "session-tags" ? "menu" : undefined}
                    role="menuitem"
                    type="button"
                  >
                    {action.icon}
                    {action.label}
                    {action.submenu === "session-tags" ? (
                      <IconChevronRight
                        aria-hidden="true"
                        className="session-context-menu-trailing-icon"
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
      {contextMenuPosition && tagSubmenuPosition
        ? createPortal(
            <div
              aria-label="Tag as"
              className="session-context-menu session-tag-submenu"
              data-empty-space-blocking="true"
              onClick={(event) => event.stopPropagation()}
              role="menu"
              style={{
                left: `${tagSubmenuPosition.x}px`,
                top: `${tagSubmenuPosition.y}px`,
                /*
                 * CDXC:SidebarContextMenu 2026-06-09-14:22:
                 * The Tag as submenu follows the raised sidebar context-menu
                 * stack so sticky project headers cannot cover the submenu
                 * while users are choosing a session marker.
                 */
                zIndex: "var(--sidebar-context-menu-submenu-z-index, 301)",
              }}
            >
              {/*
               * CDXC:SessionTags 2026-06-05-12:30:
               * The session context menu exposes `Tag as` as a submenu with the
               * canonical session tag list. Choosing the current marker clears
               * it so the old Favorite/Unfavorite workflow remains one click deep.
               */}
              {SIDEBAR_SESSION_TAG_SECTIONS.map((section) => (
                <div className="session-tag-menu-section" key={section.label}>
                  <div className="session-tag-menu-section-label">{section.label}</div>
                  {section.options.map((option) => {
                    const isSelected = currentSessionTag === option.value;
                    return (
                      <button
                        aria-checked={isSelected}
                        aria-label={
                          isSelected
                            ? `Remove ${getSidebarSessionTagLabel(option.value)} tag`
                            : `Tag as ${getSidebarSessionTagLabel(option.value)}`
                        }
                        className="session-context-menu-item session-tag-menu-item"
                        data-selected={String(isSelected)}
                        key={option.value}
                        onClick={() =>
                          requestSetSessionTag(isSelected ? undefined : option.value)
                        }
                        role="menuitemradio"
                        type="button"
                      >
                        <SessionTagIcon
                          className="session-context-menu-icon session-tag-colored-icon"
                          fillFavorite
                          size={16}
                          stroke={1.8}
                          tag={option.value}
                        />
                        <span className="session-tag-menu-item-label">{option.label}</span>
                        <IconCheck
                          aria-hidden="true"
                          className="session-tag-menu-item-check"
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
            document.body,
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
  CDXC:SidebarContextMenu 2026-06-07-13:34:
  Sleep below must only target awake non-browser sessions. Some snapshots mark
  an already parked row through lifecycleState before isSleeping is reconciled,
  so check both fields to avoid sending duplicate sleep work to native.
  */
  return Boolean(session) &&
    session?.sessionKind !== "browser" &&
    session?.kind !== "browser" &&
    session?.isSleeping !== true &&
    session?.lifecycleState !== "sleeping";
}

function supportsResumeCommandCopy(session: SidebarSessionItem): boolean {
  /**
   * CDXC:SessionRestore 2026-04-27-08:04
   * Match agent-tiler context-menu visibility: Copy resume is only shown for
   * built-in agents with known resume or resume-selection CLI behavior.
   *
   * CDXC:CursorCLI 2026-05-20-08:20:
   * Cursor resume uses stored chat UUIDs or a local title lookup fallback, so
   * Cursor CLI cards expose the same copy-resume affordance as Codex and Pi.
   */
  return (
    session.agentIcon === "codex" ||
    session.agentIcon === "claude" ||
    session.agentIcon === "copilot" ||
    session.agentIcon === "gemini" ||
    session.agentIcon === "opencode" ||
    session.agentIcon === "pi" ||
    session.agentIcon === "cursor-cli"
  );
}

function supportsFork(session: SidebarSessionItem): boolean {
  /**
   * CDXC:PiAgent 2026-05-08-09:42
   * Pi exposes a real `--fork <session>` CLI path once ghostex has captured the
   * Pi session id/path, so Pi cards should show the same one-click Fork action
   * as Codex in the session context menu.
   */
  return (
    session.agentIcon === "codex" ||
    session.agentIcon === "claude" ||
    session.agentIcon === "pi"
  );
}

function supportsGeneratedName(session: SidebarSessionItem): boolean {
  /**
   * CDXC:PiAgent 2026-05-08-16:18
   * Pi cards should expose the same right-click Generate Title action as Codex
   * once the first user message has been captured. The native rename path
   * already switches Pi to `/name <title>`, so the menu gate should include Pi
   * instead of creating a Pi-only title-generation command.
   */
  return (
    session.agentIcon === "codex" ||
    session.agentIcon === "claude" ||
    session.agentIcon === "pi"
  );
}

function supportsPopOutPane(
  session: SidebarSessionItem,
  isBrowserSession: boolean,
  isT3Session: boolean,
): boolean {
  /**
   * CDXC:PanePopOut 2026-05-19-10:15:
   * Sidebar context menus expose pop-out for browser panes and agent terminal
   * sessions. Sleeping sessions dispose their native surface and cannot remain
   * in a detached window; T3 panes keep the native title-bar model unchanged.
   */
  if (session.isSleeping === true || isT3Session) {
    return false;
  }

  if (isBrowserSession) {
    return true;
  }

  return session.sessionKind === "terminal" && Boolean(session.agentIcon);
}

function supportsFullReload(session: SidebarSessionItem): boolean {
  /**
   * CDXC:SessionRestore 2026-04-27-08:04
   * Match agent-tiler context-menu visibility: Full reload is only shown for
   * agent sessions that can be recreated and resumed programmatically.
   *
   * CDXC:PiAgent 2026-05-08-16:18
   * Pi has a restorable CLI identity through its captured session id/path, so
   * right-click Full reload should be visible on Pi cards like it is for Codex.
   *
   * CDXC:CursorCLI 2026-05-20-08:20:
   * Cursor cards can full-reload through stored chat UUIDs or trusted titles
   * resolved from the local Cursor chat store for the active project.
   */
  return (
    session.agentIcon === "codex" ||
    session.agentIcon === "claude" ||
    session.agentIcon === "opencode" ||
    session.agentIcon === "pi" ||
    session.agentIcon === "cursor-cli"
  );
}

function postSidebarAgentIconRenderDebugLog(
  vscode: WebviewApi,
  event: string,
  details: Record<string, unknown>,
): void {
  vscode.postMessage({
    details,
    event,
    type: "sidebarDebugLog",
  });
}

function findSessionCardElement(sessionId: string): HTMLElement | undefined {
  return Array.from(document.querySelectorAll<HTMLElement>("[data-sidebar-session-id]")).find(
    (element) => element.dataset.sidebarSessionId === sessionId,
  );
}

function summarizeAgentIconElement(element: HTMLElement | null | undefined) {
  if (!element) {
    return undefined;
  }

  const styles = window.getComputedStyle(element);
  const bounds = element.getBoundingClientRect();
  return {
    className:
      typeof element.className === "string"
        ? element.className
        : String(element.getAttribute("class") ?? ""),
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
