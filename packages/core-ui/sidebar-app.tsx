import { Cursor, KeyboardSensor, PointerSensor } from "@dnd-kit/dom";
import { move } from "@dnd-kit/helpers";
import { DragDropProvider, type DragDropEventHandlers } from "@dnd-kit/react";
import { useSortable } from "@dnd-kit/react/sortable";
import {
  IconArrowLeft,
  IconArrowRight,
  IconArrowsDiagonal2,
  IconArrowsDiagonalMinimize,
  IconBolt,
  IconCaretRightFilled,
  IconChevronDown,
  IconChevronRight,
  IconCheck,
  IconClock,
  IconCloud,
  IconCoffee,
  IconDeviceMobile,
  IconEdit,
  IconFilter2,
  IconFileSearch,
  IconFolders,
  IconHistoryToggle,
  IconKeyboard,
  IconLayoutSidebar,
  IconLoader2,
  IconMenu2,
  IconMoon,
  IconPlus,
  IconPlusFilled,
  IconPlugConnected,
  IconRobotFace,
  IconSearch,
  IconSettings,
  IconSquareMinus,
  IconTerminal2,
  IconUsersGroup,
  IconWorld,
  IconX,
  type TablerIcon,
} from "@tabler/icons-react";
import {
  useCallback,
  useEffect,
  useEffectEvent,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type MouseEvent as ReactMouseEvent,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import { useShallow } from "zustand/react/shallow";
import { Button } from "@/packages/components/ui/button";
import {
  MAX_GROUP_COUNT,
  type SidebarActiveSessionsSortMode,
  type ExtensionToSidebarMessage,
  type SidebarPreviousSessionItem,
} from "../shared/session-grid-contract";
import {
  getWorkspaceThemeForeground,
  normalizeWorkspaceThemeColor,
} from "../shared/workspace-project-appearance";
import {
  moveProjectsWithWorktrees,
  type ProjectWorktreeOrderItem,
} from "../shared/project-worktree-order";
import { playCompletionSound, prepareCompletionSoundPlayback } from "./completion-sound-player";
import { GitCommitModal } from "./git-commit-modal";
import {
  SidebarPreviousSessionsSearchGroup,
  SidebarSessionSearchField,
} from "./sidebar-session-search-overlay";
import {
  registerSidebarContextMenuDismissHandler,
  SidebarContextMenuPortal,
} from "./sidebar-context-menu-portal";
import { readSidebarHiddenItems, writeSidebarHiddenItems } from "./sidebar-hidden-items";
import { SidebarFixedTooltipButton } from "./sidebar-fixed-tooltip-button";
import {
  SidebarCollapseAnimationProvider,
  useSidebarCollapsiblePresence,
} from "./sidebar-collapse-animation";
import {
  createSidebarSessionSearchResults,
  createSidebarSessionSearchSelection,
  getNextSidebarSessionSearchSelection,
  isSidebarSessionSearchSelectionMatch,
  type SidebarSessionSearchSelection,
} from "./sidebar-session-search";
import { logSidebarDebug } from "./sidebar-debug";
import {
  createSidebarRefreshDebugInstanceId,
  postSidebarRefreshDebugLog,
  summarizeSidebarRefreshMessage,
} from "./sidebar-refresh-debug-log";
import {
  hashSidebarCollapseDebugId,
  SIDEBAR_COLLAPSE_STATE_DEBUG_EVENT_PREFIX,
  summarizeSidebarCollapseDebugGroupIds,
} from "./sidebar-collapse-state-debug";
import { postSidebarOrderReproLog } from "./sidebar-order-repro-log";
import {
  getSidebarReorderActivationConstraints,
  SIDEBAR_REORDER_DISTANCE_PX,
} from "./sidebar-reorder-activation";
import { scrollElementIntoViewIfNeeded } from "./scroll-into-view-if-needed";
import { resetSidebarStore, useSidebarStore } from "./sidebar-store";
import {
  createRemoteMachineDragData,
  getClientPoint,
  getSidebarGroupDropTargetFromEvent,
  getSidebarDropData,
  getSidebarSessionDropTarget,
  moveGroupIdsByDropTarget,
  type SidebarGroupDropTarget,
  type SidebarSessionDropTarget,
  getSidebarSessionDropTargetFromEvent,
  getSidebarSessionDropTargetAtPoint,
  canonicalizeSidebarSessionDropTarget,
  moveSessionIdsByDropTarget,
} from "./sidebar-dnd";
import {
  getAutoCollapseGroupIds,
  getSessionCountsByGroup,
  reconcileCollapsedGroupsById,
} from "./group-collapse";
import {
  getAwakeTerminalAndBrowserCount,
  getGroupSessionSummary,
} from "./group-session-summary";
import { SessionGroupSection } from "./session-group-section";
import { ProjectCollectionSection } from "./project-collection-section";
import {
  areSidebarProjectCollectionsStatesEqual,
  createSidebarProjectCollection,
  moveProjectsToSidebarCollection,
  parseSidebarProjectCollectionsFromGxserver,
  readLegacyCollapsedSidebarProjectCollectionIds,
  readSidebarProjectCollections,
  removeSidebarProjectCollection,
  reorderSidebarProjectCollectionDefinitions,
  reorderSidebarProjectCollections,
  serializeSidebarProjectCollectionsForGxserver,
  updateSidebarProjectCollection,
  writeSidebarProjectCollections,
  type SidebarProjectCollection,
  type SidebarProjectCollectionsState,
} from "./project-collections";
import { isEditableKeyboardTarget } from "./text-input-keyboard";
import { TOOLTIP_DELAY_MS } from "./tooltip-delay";
import {
  AppTooltip,
  dismissSidebarTooltips,
  setSidebarTooltipsSuppressedForDrag,
  TooltipProvider,
  useDismissSidebarTooltipsOnScroll,
} from "./app-tooltip";
import { useScrollGlowState } from "./use-scroll-glow-state";
import { AgentMenuChatIndicator } from "./agent-menu-chat-indicator";
import type { WebviewApi } from "./webview-api";
import { createDisplaySessionLayout } from "../shared/active-sessions-sort";
import {
  moveSidebarV2GroupRows,
  projectSidebarV2GroupOrderByMachine,
  type SidebarV2GroupOrderRow,
} from "../shared/sidebar-v2-group-order";
import {
  filterDefaultNamedSessionSearchItems,
  filterPreviousSessions,
  filterSidebarSessionItems,
} from "./previous-session-search";
import {
  getSidebarSessionTagLabel,
  SessionTagIcon,
  type SidebarSessionTagFilter,
} from "./session-tag-ui";
import {
  getEnabledVisibleSidebarSessionTagFilters,
  getSidebarSessionTagListItemFilter,
  normalizeSidebarSessionTagListItems,
  sessionMatchesSidebarTagFilters,
  type SidebarSessionTagListItem,
} from "../shared/session-tags";
import { isEmptySidebarDoubleClick } from "./empty-sidebar-double-click";
import { closeAppModal, openAppModal, openQuickAccess } from "./app-modal-host-bridge";
import { formatSidebarHotkeyLabel } from "./hotkey-label";
import {
  GHOSTEX_HOTKEY_DEFINITIONS,
  getghostexHotkeyActionById,
  getghostexHotkeyActionIdForKey,
  ghostexHotkeyTextFromKeyboardEvent,
  normalizeHotkeyText,
  normalizeghostexHotkeySettings,
  type ghostexHotkeySettings,
} from "../shared/ghostex-hotkeys";
import {
  DEFAULT_ghostex_SETTINGS,
  KEEP_AWAKE_DURATION_OPTIONS,
  getSidebarTitlebarForegroundForBackground,
  getSidebarTitlebarGradientColors,
  isDiagnosticLoggingScenarioEnabled,
  type DiagnosticLoggingScenarioId,
  type ghostexSettings,
  type KeepAwakeDurationMinutes,
  type RemoteMachineSettings,
  type SidebarNewSessionEnvMode,
  type SidebarProjectGroupingMode,
  type SidebarV2Layout,
  type SidebarVersion,
} from "../shared/ghostex-settings";
import { SidebarV2Root } from "./v2/sidebar-v2-root";
import {
  SIDEBAR_PROJECT_JUMP_EVENT,
  type SidebarProjectJumpEventDetail,
} from "../shared/sidebar-project-jump";
import type { SidebarAgentButton } from "../shared/sidebar-agents";
import { PET_CONTROLS_VISIBLE } from "../shared/pets";
import {
  readRenderedSidebarSessionSlotIds,
  readRenderedSidebarSessionSlots,
  resolveAdjacentRenderedSidebarSessionSlotId,
  resolveRenderedSidebarSessionAdditiveSelection,
  resolveRenderedSidebarSessionRangeSelection,
  resolveVisibleSidebarSessionSlotId,
} from "./sidebar-visible-session-slots";
import {
  PRIMARY_AGENT_LAUNCHER_CHANGED_EVENT,
  readPrimaryAgentLauncherId,
  writePrimaryAgentLauncherId,
  type PrimaryAgentLauncherChangedEvent,
} from "./primary-agent-launcher";
import {
  readProjectSessionListCollapsedState,
  type ProjectSessionListCollapsedState,
} from "./project-session-list-toggle";
import { ProjectAgentLauncherIcon } from "./project-agent-launcher-icon";
import { hasKnownSidebarProjectInventory } from "./sidebar-project-empty-state";

type SidebarEventSource = Pick<Window, "addEventListener" | "removeEventListener">;

export type SidebarAppProps = {
  enableProjectCollections?: boolean;
  messageSource?: SidebarEventSource;
  nativeHostEventSource?: SidebarEventSource | null;
  onStartGxserver?: () => void;
  vscode: WebviewApi;
  windowScopeId?: string;
};

type SessionIdsByGroup = Record<string, string[]>;
type SidebarStoreState = ReturnType<typeof useSidebarStore.getState>;
type SidebarGroupsById = SidebarStoreState[ "groupsById" ];
type SidebarSessionsById = SidebarStoreState[ "sessionsById" ];
type SidebarSectionSessionSummary = ReturnType<typeof getGroupSessionSummary> & {
  awakeCount: number;
};
type RemoteMachineRuntimeStatus = Extract<ExtensionToSidebarMessage, { type: "remoteMachineStatus"; }>;
type RemoteMachineRuntimeStatuses = Record<string, RemoteMachineRuntimeStatus[ "state" ]>;
type RemoteMachineStatusMessages = Record<string, string>;
type HeaderSortMenuPosition = {
  left: number;
  top: number;
};

type RemoteMachineHeaderConnectionControl = {
  kind: "busy" | "connect" | "error";
  label: string;
  onClick?: () => void;
};

function getSidebarSectionSessionSummary(
  groupIds: readonly string[],
  sessionIdsByGroup: Readonly<Record<string, readonly string[] | undefined>>,
  sessionsById: SidebarSessionsById,
): SidebarSectionSessionSummary {
  const sessionIds = new Set(
    groupIds.flatMap((groupId) => sessionIdsByGroup[groupId] ?? []),
  );
  const sessions = [...sessionIds].flatMap((sessionId) => {
    const session = sessionsById[sessionId];
    return session ? [session] : [];
  });

  return {
    ...getGroupSessionSummary(sessions),
    awakeCount: getAwakeTerminalAndBrowserCount(sessions),
  };
}

const REFERENCE_SECTION_AGENT_MENU_WIDTH_PX = 220;

type NativeModifierStateHostEvent = {
  isCommandPressed: boolean;
  type: "nativeModifierState";
};

const SIDEBAR_HOTKEY_OVERLAY_ENABLED = false;
/*
 * CDXC:Hotkeys 2026-06-15-02:33:
 * Temporarily disable the Cmd-hold sidebar hotkey overlay while keeping the
 * hook, renderer, styles, and native modifier bridge in source for near-term
 * re-enable. Holding Cmd must not show the overlay from sidebar DOM focus or
 * native terminal/browser/titlebar focus while this flag is false.
 */

const SIDEBAR_KEEP_AWAKE_RUNTIME_STORAGE_KEY = "ghostex.titlebar.keepAwakeRuntime";
const GHOSTEX_DISCORD_URL = "https://discord.gg/df7b3G92CS";

type SidebarKeepAwakeRuntimeState = {
  durationMinutes: KeepAwakeDurationMinutes;
};

function isSidebarRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

function isKeepAwakeDurationMinutes(value: unknown): value is KeepAwakeDurationMinutes {
  return KEEP_AWAKE_DURATION_OPTIONS.some((option) => option.value === value);
}

function readSidebarKeepAwakeRuntime(): SidebarKeepAwakeRuntimeState | undefined {
  if (typeof window === "undefined") {
    return undefined;
  }

  try {
    const rawRuntime = window.localStorage.getItem(SIDEBAR_KEEP_AWAKE_RUNTIME_STORAGE_KEY);
    if (!rawRuntime) {
      return undefined;
    }
    const parsedRuntime: unknown = JSON.parse(rawRuntime);
    if (!isSidebarRecord(parsedRuntime) || !isKeepAwakeDurationMinutes(parsedRuntime.durationMinutes)) {
      return undefined;
    }
    const fireAtMs = parsedRuntime.fireAtMs;
    if (typeof fireAtMs === "number" && Number.isFinite(fireAtMs) && fireAtMs <= Date.now()) {
      return undefined;
    }
    return {
      durationMinutes: parsedRuntime.durationMinutes,
    };
  } catch {
    return undefined;
  }
}

type SidebarGroupDragPreview = {
  groupId: string;
  isCollapsed: boolean;
  left: number;
  pointerOffsetY: number;
  themeColor?: string;
  title: string;
  top: number;
  width: number;
};

/*
 * CDXC:CollectionDragPreview 2026-07-22:
 * Collection reordering uses feedback "none", and dnd-kit only flips a
 * draggable's status to "dragging" inside its feedback plugin, so with "none"
 * sortable.isDragging never becomes true and the drag starts with zero visual
 * feedback. Like project headers, the app owns the drag visuals: this preview
 * drives a cursor-following collapsed-header ghost plus the faint source
 * placeholder.
 */
type SidebarProjectCollectionDragPreview = {
  collectionId: string;
  color: string;
  left: number;
  pointerOffsetY: number;
  title: string;
  top: number;
  width: number;
};

type SidebarRemoteMachineDragPreview = {
  collapsed: boolean;
  left: number;
  machineId: string;
  pointerOffsetY: number;
  title: string;
  top: number;
  width: number;
};

function useCommandHotkeyOverlay(): boolean {
  const [ isVisible, setIsVisible ] = useState(false);
  const isCommandPressedRef = useRef(false);
  const showTimerRef = useRef<number | undefined>(undefined);

  useEffect(() => {
    if (!SIDEBAR_HOTKEY_OVERLAY_ENABLED) {
      return;
    }

    const clearOverlayTimer = () => {
      if (showTimerRef.current !== undefined) {
        window.clearTimeout(showTimerRef.current);
        showTimerRef.current = undefined;
      }
    };
    const hideOverlay = () => {
      isCommandPressedRef.current = false;
      clearOverlayTimer();
      setIsVisible(false);
    };
    const showOverlayAfterDelay = () => {
      if (isCommandPressedRef.current || showTimerRef.current !== undefined) {
        return;
      }
      isCommandPressedRef.current = true;
      /**
       * CDXC:Hotkeys 2026-05-11-09:26
       * Holding Cmd for one second should reveal an in-sidebar cheat sheet of
       * the current effective hotkeys. Delay the overlay so normal Cmd chords
       * do not flash UI while still making discovery available from the key the
       * simplified keymap now centers on.
       *
       * CDXC:Hotkeys 2026-06-14-19:40:
       * Native terminal, browser, and titlebar focus can hold Cmd without
       * delivering a WebKit keydown to the sidebar. Keep this dormant path wired
       * to native modifier host events so the cheat sheet can be restored by
       * flipping SIDEBAR_HOTKEY_OVERLAY_ENABLED.
       *
       * CDXC:Hotkeys 2026-06-15-02:33:
       * SIDEBAR_HOTKEY_OVERLAY_ENABLED intentionally short-circuits this effect
       * before listeners attach, so holding Cmd must not show this overlay until
       * the temporary disable is removed.
       */
      showTimerRef.current = window.setTimeout(() => {
        showTimerRef.current = undefined;
        if (isCommandPressedRef.current) {
          setIsVisible(true);
        }
      }, 1_000);
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Meta") {
        return;
      }
      showOverlayAfterDelay();
    };
    const handleKeyUp = (event: KeyboardEvent) => {
      if (event.key === "Meta" || !event.metaKey) {
        hideOverlay();
      }
    };
    const handleNativeHostEvent = (event: Event) => {
      if (!(event instanceof CustomEvent) || !isNativeModifierStateHostEvent(event.detail)) {
        return;
      }
      if (event.detail.isCommandPressed) {
        showOverlayAfterDelay();
      } else {
        hideOverlay();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    window.addEventListener("keyup", handleKeyUp);
    window.addEventListener("ghostex-native-host-event", handleNativeHostEvent);
    window.addEventListener("blur", hideOverlay);
    return () => {
      clearOverlayTimer();
      window.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("keyup", handleKeyUp);
      window.removeEventListener("ghostex-native-host-event", handleNativeHostEvent);
      window.removeEventListener("blur", hideOverlay);
    };
  }, []);

  return SIDEBAR_HOTKEY_OVERLAY_ENABLED && isVisible;
}

function isNativeModifierStateHostEvent(value: unknown): value is NativeModifierStateHostEvent {
  return (
    Boolean(value) &&
    typeof value === "object" &&
    (value as NativeModifierStateHostEvent).type === "nativeModifierState" &&
    typeof (value as NativeModifierStateHostEvent).isCommandPressed === "boolean"
  );
}

function SidebarHotkeyOverlay({ hotkeys }: { hotkeys?: ghostexHotkeySettings; }) {
  const normalizedHotkeys = normalizeghostexHotkeySettings(hotkeys);
  const rows = getSidebarHotkeyOverlayRows(normalizedHotkeys);

  return (
    <>
      <div aria-hidden="true" className="sidebar-hotkey-overlay-backdrop" />
      <aside aria-label="Keyboard shortcuts" className="sidebar-hotkey-overlay">
        <div className="sidebar-hotkey-overlay-title">Hotkeys</div>
        <div className="sidebar-hotkey-overlay-grid">
          {rows.map((row) => (
            <div className="sidebar-hotkey-overlay-row" key={`${row.title}-${row.hotkey}`}>
              <span className="sidebar-hotkey-overlay-action">{row.title}</span>
              <kbd className="sidebar-hotkey-overlay-key">{formatSidebarHotkeyLabel(row.hotkey)}</kbd>
            </div>
          ))}
        </div>
      </aside>
    </>
  );
}

function ProjectGroupDragGhost({ preview }: { preview: SidebarGroupDragPreview; }) {
  const style = {
    left: `${preview.left}px`,
    top: `${preview.top}px`,
    width: `${preview.width}px`,
    ...(preview.themeColor ? { "--workspace-project-theme-color": preview.themeColor } : {}),
  } as CSSProperties;

  /*
   * CDXC:ProjectDragPreview 2026-07-02-13:05:
   * The ghost mirrors the real project header DOM (group > group-head >
   * group-title-wrap > group-title-row) so it picks up the exact header
   * padding, font, and theme color instead of a bespoke approximation. It
   * renders the title only — trailing header action buttons are omitted.
   *
   * CDXC:ProjectDragPreview 2026-07-02-21:10:
   * The reference-layout .group-head uses negative scroll-bleed margins to
   * extend the row past the panel clip. The ghost's fixed shell is already
   * anchored to the measured header rect (which reflects those margins), so
   * the nested head keeps the scoped padding but must not re-apply the
   * margins, or the title would shift left of the grabbed header.
   */
  return (
    <div
      aria-hidden="true"
      className="project-drag-ghost group"
      data-project-group="true"
      data-workspace-custom-theme={String(Boolean(preview.themeColor))}
      style={style}
    >
      <div className="group-head" data-collapsible="true" style={{ margin: 0 }}>
        <div className="group-title-wrap">
          <div className="group-title-row" data-project-leading-icon="false">
            <div className="group-title-handle" data-draggable="true">
              <button
                aria-disabled="false"
                aria-expanded={!preview.isCollapsed}
                aria-label={preview.title}
                className="group-title-button"
                data-empty-project="false"
                tabIndex={-1}
                type="button"
              >
                <span className="group-title section-titlebar-label">{preview.title}</span>
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

function ProjectCollectionDragGhost({ preview }: { preview: SidebarProjectCollectionDragPreview; }) {
  const style = {
    left: `${preview.left}px`,
    top: `${preview.top}px`,
    width: `${preview.width}px`,
    "--project-collection-color": preview.color,
  } as CSSProperties;

  /*
   * CDXC:CollectionDragPreview 2026-07-22:
   * The ghost mirrors a collapsed collection panel's DOM
   * (section.project-collection > .project-collection-header > caret + title)
   * so it inherits the exact reference-panel skin and typography. It renders
   * the caret and title only — trailing header actions are omitted, matching
   * the project drag ghost.
   */
  return (
    <section
      aria-hidden="true"
      className="project-collection project-collection-drag-ghost"
      data-collapsed="true"
      style={style}
    >
      <div className="project-collection-header">
        <span className="project-collection-collapse">
          <IconCaretRightFilled aria-hidden="true" size={14} />
        </span>
        <span className="project-collection-title">{preview.title}</span>
      </div>
    </section>
  );
}

function RemoteMachineDragGhost({ preview }: { preview: SidebarRemoteMachineDragPreview; }) {
  const style = {
    left: `${preview.left}px`,
    top: `${preview.top}px`,
    width: `${preview.width}px`,
  } as CSSProperties;

  return (
    <div aria-hidden="true" className="remote-machine-drag-ghost" style={style}>
      <div
        className="reference-sidebar-section-row"
        data-actions-always-visible="false"
        data-has-remote-connection-control="false"
        data-reference-section="remote"
      >
        <button
          aria-expanded={!preview.collapsed}
          className="reference-sidebar-section-heading"
          tabIndex={-1}
          type="button"
        >
          <IconCloud
            aria-hidden="true"
            className="reference-sidebar-section-icon"
            size={15}
            stroke={1.8}
          />
          <span className="reference-sidebar-section-title">{preview.title}</span>
          <IconCaretRightFilled
            aria-hidden="true"
            className="reference-sidebar-section-chevron"
            size={13}
          />
        </button>
      </div>
    </div>
  );
}

function getSidebarHotkeyOverlayRows(hotkeys: ghostexHotkeySettings) {
  const rows: Array<{ hotkey: string; title: string; }> = [];
  for (const definition of GHOSTEX_HOTKEY_DEFINITIONS) {
    if (definition.id === "jumpToProject1") {
      const hotkey = normalizeHotkeyText(hotkeys.jumpToProject1 ?? "");
      if (hotkey) {
        rows.push({
          hotkey: formatNumberedHotkeyExample(hotkey),
          title: "Jump to Project N",
        });
      }
      continue;
    }
    if (definition.id === "focusSessionSlot1") {
      const hotkey = normalizeHotkeyText(hotkeys.focusSessionSlot1 ?? "");
      if (hotkey) {
        rows.push({
          hotkey: formatNumberedHotkeyExample(hotkey),
          title: "Focus Session N",
        });
      }
      continue;
    }
    if (
      /^jumpToProject[2-9]$/u.test(definition.id) ||
      /^focusSessionSlot[2-9]$/u.test(definition.id)
    ) {
      continue;
    }
    const hotkey = normalizeHotkeyText(hotkeys[ definition.id ] ?? "");
    if (hotkey) {
      rows.push({ hotkey, title: definition.title });
    }
  }
  return rows;
}

function formatNumberedHotkeyExample(hotkey: string): string {
  /**
   * CDXC:Hotkeys 2026-05-11-09:36
   * The Cmd-hold overlay should not list every numbered session or group slot.
   * Show one N-based example derived from slot 1 so user rebinds still explain
   * the whole numbered family without crowding the cheat sheet.
   */
  return hotkey.replace(/(^|[+ ])1(?=$| )/u, "$1n");
}

type SidebarPointerDownSessionTarget = {
  groupId: string;
  point: {
    x: number;
    y: number;
  };
  sessionId: string;
};

type SidebarSessionPointerDragState = {
  didMove: boolean;
  startPoint?: {
    x: number;
    y: number;
  };
};

type SidebarUiCollapseState = {
  collapsedGroupsById: Record<string, true>;
  collapsedProjectCollectionsByKey: Record<string, true>;
  collapsedProjectSessionListsById: ProjectSessionListCollapsedState;
  collapsedRemoteMachineSectionsById: Record<string, true>;
  isReferenceChatsCollapsed: boolean;
  isReferenceProjectsCollapsed: boolean;
};

type SidebarUiCollapseStorage = {
  state: SidebarUiCollapseState;
  version: 2;
};

type SidebarUiCollapseStateReadResult = {
  reason?: "invalid-shape" | "missing" | "parse-error" | "storage-unavailable";
  state: SidebarUiCollapseState;
  storedByteLength?: number;
};

type SidebarUiCollapseStateWriteResult = {
  ok: boolean;
  reason?: "storage-error" | "storage-unavailable";
  storedByteLength?: number;
};

type SidebarProjectGroupOrderItem = ProjectWorktreeOrderItem & {
  orderId: string;
};

/**
 * CDXC:SidebarBrowserTabReveal 2026-08-18:
 * `requestId` is what makes a reveal one-shot: two consecutive middle-clicks on
 * the same link name the same session and must each scroll it back into view.
 */
type SidebarSessionRevealRequest = {
  requestId: number;
  sessionId: string;
};

type SidebarProjectCollectionRenderItem =
  | { collection: SidebarProjectCollection; groupIds: string[]; kind: "collection" }
  | { groupId: string; kind: "project" };

type SidebarProjectGroupLookup = Record<
  string,
  | {
    projectContext?: {
      path?: string;
      editor: {
        projectId: string;
      };
      worktree?: {
        parentProjectId: string;
      };
    };
    remoteMachineContext?: {
      machineId: string;
      projectId?: string;
    };
  }
  | undefined
>;

type ReferenceSidebarSectionId = "projects" | "quick" | "remote";

const REFERENCE_SECTION_CHILD_ANIMATION_RESET_MS = 420;

const sensors = [
  PointerSensor.configure({
    activationConstraints: getSidebarReorderActivationConstraints,
  }),
  KeyboardSensor,
];

/*
 * Remote machines reorder only from their visible header. Pointer-only
 * activation keeps Space/Enter owned by the existing collapse button and
 * prevents a keyboard drag from leaving the shared manager in an unseen drag.
 */
const remoteMachineSensors = [
  PointerSensor.configure({
    activationConstraints: getSidebarReorderActivationConstraints,
  }),
];

const SIDEBAR_STARTUP_INTERACTION_BLOCK_MS = 1500;
const SIDEBAR_STARTUP_REPRO_WINDOW_MS = 15_000;
const SIDEBAR_GXSERVER_UNAVAILABLE_GROUP_ID = "gxserver-unavailable";
const SIDEBAR_GXSERVER_UNAVAILABLE_EMPTY_STATE_DELAY_MS = 20_000;
const SIDEBAR_UI_COLLAPSE_STATE_STORAGE_KEY = "ghostex-sidebar-ui-collapse-state";
/*
 * Collapse preferences belong to one app window. The current GPUI host uses
 * "main"; future windows must pass their own stable scope id so their sidebars
 * persist independently without sending presentation state through gxserver.
 */
const DEFAULT_SIDEBAR_WINDOW_SCOPE_ID = "main";
const MIN_SESSION_SEARCH_QUERY_LENGTH = 4;
const COMPLETION_FLASH_DURATION_MS = 3_000;
const DEBUG_BUILD_STAMP_STYLE: CSSProperties = {
  position: "fixed",
  right: "10px",
  bottom: "8px",
  zIndex: 20,
  padding: 0,
  border: "none",
  background: "transparent",
  color: "var(--vscode-foreground)",
  fontFamily: "var(--vscode-font-family)",
  fontSize: "10px",
  lineHeight: 1.2,
  fontVariantNumeric: "tabular-nums",
  opacity: 0.72,
};

function createDefaultSidebarUiCollapseState(): SidebarUiCollapseState {
  return {
    collapsedGroupsById: {},
    collapsedProjectCollectionsByKey: {},
    collapsedProjectSessionListsById: {},
    collapsedRemoteMachineSectionsById: {},
    isReferenceChatsCollapsed: false,
    isReferenceProjectsCollapsed: false,
  };
}

function normalizeSidebarWindowScopeId(value: string): string {
  const normalized = value.trim().slice(0, 120);
  return normalized || DEFAULT_SIDEBAR_WINDOW_SCOPE_ID;
}

function getSidebarUiCollapseStateStorageKey(windowScopeId: string): string {
  return `${SIDEBAR_UI_COLLAPSE_STATE_STORAGE_KEY}:window:${encodeURIComponent(windowScopeId)}`;
}

function createLocalProjectCollectionCollapseKey(collectionId: string): string {
  return `local:${collectionId}`;
}

function createRemoteProjectCollectionCollapseKey(machineId: string, collectionId: string): string {
  return `remote:${machineId}:${collectionId}`;
}

function normalizeSidebarUiCollapseState(candidate: unknown): SidebarUiCollapseState {
  if (!candidate || typeof candidate !== "object" || Array.isArray(candidate)) {
    return createDefaultSidebarUiCollapseState();
  }
  const state = candidate as Partial<SidebarUiCollapseState>;
  return {
    collapsedGroupsById: normalizeStoredCollapsedGroupsById(state.collapsedGroupsById),
    collapsedProjectCollectionsByKey: normalizeStoredCollapsedGroupsById(
      state.collapsedProjectCollectionsByKey,
    ),
    collapsedProjectSessionListsById: normalizeStoredCollapsedGroupsById(
      state.collapsedProjectSessionListsById,
    ),
    collapsedRemoteMachineSectionsById: normalizeStoredCollapsedGroupsById(
      state.collapsedRemoteMachineSectionsById,
    ),
    isReferenceChatsCollapsed: state.isReferenceChatsCollapsed === true,
    isReferenceProjectsCollapsed: state.isReferenceProjectsCollapsed === true,
  };
}

function readSidebarUiCollapseState(windowScopeId: string): SidebarUiCollapseStateReadResult {
  if (typeof window === "undefined") {
    return {
      reason: "storage-unavailable",
      state: createDefaultSidebarUiCollapseState(),
    };
  }

  try {
    const scopedStoredValue = window.localStorage.getItem(
      getSidebarUiCollapseStateStorageKey(windowScopeId),
    );
    if (scopedStoredValue !== null) {
      const scopedCandidate = JSON.parse(scopedStoredValue) as Partial<SidebarUiCollapseStorage>;
      if (
        !scopedCandidate ||
        typeof scopedCandidate !== "object" ||
        scopedCandidate.version !== 2
      ) {
        return {
          reason: "invalid-shape",
          state: createDefaultSidebarUiCollapseState(),
          storedByteLength: scopedStoredValue.length,
        };
      }
      return {
        state: normalizeSidebarUiCollapseState(scopedCandidate.state),
        storedByteLength: scopedStoredValue.length,
      };
    }

    const legacyStoredValue = window.localStorage.getItem(SIDEBAR_UI_COLLAPSE_STATE_STORAGE_KEY);
    const candidate = JSON.parse(legacyStoredValue ?? "null");
    if (!candidate || typeof candidate !== "object" || Array.isArray(candidate)) {
      const state = createDefaultSidebarUiCollapseState();
      state.collapsedProjectCollectionsByKey = Object.fromEntries(
        Object.keys(readLegacyCollapsedSidebarProjectCollectionIds()).map((collectionId) => [
          createLocalProjectCollectionCollapseKey(collectionId),
          true,
        ]),
      );
      state.collapsedProjectSessionListsById = readProjectSessionListCollapsedState();
      return { reason: "missing", state };
    }

    const migrated = normalizeSidebarUiCollapseState(candidate);
    migrated.collapsedProjectCollectionsByKey = Object.fromEntries(
      Object.keys(readLegacyCollapsedSidebarProjectCollectionIds()).map((collectionId) => [
        createLocalProjectCollectionCollapseKey(collectionId),
        true,
      ]),
    );
    migrated.collapsedProjectSessionListsById = readProjectSessionListCollapsedState();
    return { state: migrated, storedByteLength: legacyStoredValue?.length ?? 0 };
  } catch {
    return {
      reason: "parse-error",
      state: createDefaultSidebarUiCollapseState(),
    };
  }
}

function normalizeStoredCollapsedGroupsById(candidate: unknown): Record<string, true> {
  if (!candidate || typeof candidate !== "object" || Array.isArray(candidate)) {
    return {};
  }

  const collapsedGroupsById: Record<string, true> = {};
  for (const [ groupId, collapsed ] of Object.entries(candidate)) {
    if (collapsed === true) {
      collapsedGroupsById[ groupId ] = true;
    }
  }
  return collapsedGroupsById;
}

function summarizeSidebarUiCollapseState(state: SidebarUiCollapseState): Record<string, unknown> {
  return {
    collapsedGroupCount: Object.keys(state.collapsedGroupsById).length,
    collapsedProjectCollectionCount: Object.keys(state.collapsedProjectCollectionsByKey).length,
    collapsedProjectSessionListCount: Object.keys(state.collapsedProjectSessionListsById).length,
    collapsedRemoteMachineSectionCount: Object.keys(state.collapsedRemoteMachineSectionsById)
      .length,
    isReferenceChatsCollapsed: state.isReferenceChatsCollapsed,
    isReferenceProjectsCollapsed: state.isReferenceProjectsCollapsed,
  };
}

function summarizeSidebarUiCollapseRead(
  result: SidebarUiCollapseStateReadResult,
): Record<string, unknown> {
  return {
    ...summarizeSidebarUiCollapseState(result.state),
    readReason: result.reason ?? "stored",
    storedByteLength: result.storedByteLength ?? 0,
  };
}

function writeSidebarUiCollapseState(
  windowScopeId: string,
  state: SidebarUiCollapseState,
): SidebarUiCollapseStateWriteResult {
  if (typeof window === "undefined") {
    return { ok: false, reason: "storage-unavailable" };
  }

  try {
    const serialized = JSON.stringify({
      state,
      version: 2,
    } satisfies SidebarUiCollapseStorage);
    window.localStorage.setItem(getSidebarUiCollapseStateStorageKey(windowScopeId), serialized);
    return { ok: true, storedByteLength: serialized.length };
  } catch {
    // Ignore storage failures; the in-memory collapse state should still update.
    return { ok: false, reason: "storage-error" };
  }
}

function readSidebarProjectJumpEventDetail(event: Event): SidebarProjectJumpEventDetail | undefined {
  const detail = (event as CustomEvent<unknown>).detail;
  if (!detail || typeof detail !== "object") {
    return undefined;
  }
  const candidate = detail as Partial<SidebarProjectJumpEventDetail>;
  if (
    typeof candidate.groupId !== "string" ||
    typeof candidate.projectId !== "string" ||
    typeof candidate.expandCollapsedProject !== "boolean" ||
    typeof candidate.showLessAfterExpand !== "boolean" ||
    (
      candidate.revealFocusedSession !== undefined &&
      typeof candidate.revealFocusedSession !== "boolean"
    )
  ) {
    return undefined;
  }
  return {
    expandCollapsedProject: candidate.expandCollapsedProject,
    groupId: candidate.groupId,
    projectId: candidate.projectId,
    revealFocusedSession: candidate.revealFocusedSession === true ? true : undefined,
    showLessAfterExpand: candidate.showLessAfterExpand,
  };
}

export function SidebarApp({
  enableProjectCollections = false,
  messageSource = window,
  nativeHostEventSource = window,
  onStartGxserver,
  vscode,
  windowScopeId: rawWindowScopeId = DEFAULT_SIDEBAR_WINDOW_SCOPE_ID,
}: SidebarAppProps) {
  useDismissSidebarTooltipsOnScroll();
  const [ windowScopeId ] = useState(() => normalizeSidebarWindowScopeId(rawWindowScopeId));
  const [ initialUiCollapseStateRead ] = useState(() => readSidebarUiCollapseState(windowScopeId));
  const initialUiCollapseState = initialUiCollapseStateRead.state;
  const [ isStartupInteractionBlocked, setIsStartupInteractionBlocked ] = useState(true);
  const [ autoEditingGroupId, setAutoEditingGroupId ] = useState<string>();
  const [ agentCreateRequestId, setAgentCreateRequestId ] = useState(0);
  const [ isDaemonSessionsOpen, setIsDaemonSessionsOpen ] = useState(false);
  const [ isPinnedPromptsOpen, setIsPinnedPromptsOpen ] = useState(false);
  const [ isPreviousSessionsOpen, setIsPreviousSessionsOpen ] = useState(false);
  const [ isReferenceChatsCollapsed, setIsReferenceChatsCollapsed ] = useState(
    initialUiCollapseState.isReferenceChatsCollapsed,
  );
  const [ isReferenceProjectsCollapsed, setIsReferenceProjectsCollapsed ] = useState(
    initialUiCollapseState.isReferenceProjectsCollapsed,
  );
  const [ isScratchPadOpen, setIsScratchPadOpen ] = useState(false);
  const [ isSettingsOpen, setIsSettingsOpen ] = useState(false);
  const [ isSessionSearchOpen, setIsSessionSearchOpen ] = useState(false);
  const [ initialHiddenItems ] = useState(readSidebarHiddenItems);
  const [ hiddenGroupIds, setHiddenGroupIds ] = useState(initialHiddenItems.groupIds);
  const [ hiddenCollectionKeys, setHiddenCollectionKeys ] = useState(
    initialHiddenItems.collectionKeys,
  );
  const [ showHiddenSidebarItems, setShowHiddenSidebarItems ] = useState(false);
  const showCommandHotkeyOverlay = useCommandHotkeyOverlay();
  const [ completionFlashNonceBySessionId, setCompletionFlashNonceBySessionId ] = useState<
    Record<string, number>
  >({});
  const [ collapsedGroupsById, setCollapsedGroupsById ] = useState<Record<string, true>>(
    initialUiCollapseState.collapsedGroupsById,
  );
  const [ collapsedProjectCollectionsByKey, setCollapsedProjectCollectionsByKey ] = useState<
    Record<string, true>
  >(initialUiCollapseState.collapsedProjectCollectionsByKey);
  const [ collapsedProjectSessionListsById, setCollapsedProjectSessionListsById ] =
    useState<ProjectSessionListCollapsedState>(
      initialUiCollapseState.collapsedProjectSessionListsById,
    );
  const [ projectCollections, setProjectCollections ] = useState<SidebarProjectCollectionsState>(
    enableProjectCollections ? readSidebarProjectCollections : { collections: [], nextCollectionNumber: 1 },
  );
  const [ remoteProjectCollectionsByMachineId, setRemoteProjectCollectionsByMachineId ] = useState<
    Record<string, SidebarProjectCollectionsState>
  >({});
  /*
  CDXC:SidebarProjectCollections 2026-07-18-00:00:
  Tracks the last collection state exchanged with gxserver (pushed to it or
  adopted from it) so the write-through effect posts only real local edits.
  Without this baseline, mount and server reconciliation would echo the state
  straight back and a fresh install would clobber the server copy with its
  empty localStorage overlay.
  */
  const lastGxserverSyncedProjectCollectionsRef = useRef(projectCollections);
  const [ autoEditingProjectCollectionId, setAutoEditingProjectCollectionId ] = useState<string>();
  const [ collapsedRemoteMachineSectionsById, setCollapsedRemoteMachineSectionsById ] = useState<
    Record<string, true>
  >(initialUiCollapseState.collapsedRemoteMachineSectionsById);
  const [ referenceSectionChildAnimations, setReferenceSectionChildAnimations ] = useState<
    Record<ReferenceSidebarSectionId, boolean>
  >({
    projects: false,
    quick: false,
    remote: false,
  });
  const previousExpandedReferenceProjectGroupIdsRef = useRef<string[]>([]);
  const previousExpandedRemoteProjectGroupIdsByMachineIdRef = useRef<
    Record<string, string[]>
  >({});
  const previousExpandedProjectGroupIdsByCollectionIdRef = useRef<Record<string, string[]>>({});
  const [ sessionSearchQuery, setSessionSearchQuery ] = useState("");
  const [ selectedSessionTagFilters, setSelectedSessionTagFilters ] = useState<
    SidebarSessionTagFilter[]
  >([]);
  const [ remoteSessionSearchPreviousSessions, setRemoteSessionSearchPreviousSessions ] =
    useState<SidebarPreviousSessionItem[] | undefined>(undefined);
  const [ groupDropIndicator, setGroupDropIndicator ] = useState<SidebarGroupDropTarget>();
  const [ projectCollectionDropIndicator, setProjectCollectionDropIndicator ] =
    useState<SidebarProjectCollectionDropTarget>();
  const [ remoteMachineDropIndicator, setRemoteMachineDropIndicator ] =
    useState<SidebarRemoteMachineDropTarget>();
  const [ projectUngroupDropIndicatorScopeId, setProjectUngroupDropIndicatorScopeId ] =
    useState<string>();
  const [ groupDragPreview, setGroupDragPreview ] = useState<SidebarGroupDragPreview>();
  const [ projectCollectionDragPreview, setProjectCollectionDragPreview ] =
    useState<SidebarProjectCollectionDragPreview>();
  const [ remoteMachineDragPreview, setRemoteMachineDragPreview ] =
    useState<SidebarRemoteMachineDragPreview>();
  /*
   * CDXC:ProjectReorderScrollLock 2026-07-22:
   * While a project or collection header is being dragged, the per-project
   * session scrollers must not auto-scroll under the ghost. dnd-kit's Scroller
   * treats any computed overflow auto/scroll ancestor under the pointer as a
   * scroll target, so this flag flips those inner scrollers to overflow hidden
   * for the duration of the drag (the main sidebar scroller stays scrollable
   * to reach offscreen drop positions).
   */
  const [ isProjectReorderDragActive, setIsProjectReorderDragActive ] = useState(false);
  const [ referenceLayoutElement, setReferenceLayoutElement ] = useState<HTMLDivElement | null>(
    null,
  );
  const [ pinnedSessionDropIndicator, setPinnedSessionDropIndicator ] =
    useState<SidebarSessionDropTarget>();
  const [ sessionDropIndicator, setSessionDropIndicator ] = useState<SidebarSessionDropTarget>();
  const [ isSessionSearchSelectionVisible, setIsSessionSearchSelectionVisible ] = useState(false);
  const [ focusedSessionRevealRequestId, setFocusedSessionRevealRequestId ] = useState(0);
  /**
   * CDXC:SidebarBrowserTabReveal 2026-08-18:
   * The host's pending "make this row visible" request. It is kept in state
   * rather than handled inline because the row it names can arrive after the
   * request: gpui creates a Browser tab and asks for the reveal in the same
   * turn the tab is first published, so the reveal effect re-runs with the
   * displayed session list until the row exists.
   */
  const [ sessionRevealRequest, setSessionRevealRequest ] =
    useState<SidebarSessionRevealRequest>();
  const [ showGxserverUnavailableEmptyState, setShowGxserverUnavailableEmptyState ] =
    useState(false);
  const [ selectedSessionSearchResult, setSelectedSessionSearchResult ] =
    useState<SidebarSessionSearchSelection>();
  const [ selectedSidebarSessionIds, setSelectedSidebarSessionIds ] = useState<string[]>([]);
  const pendingCreateGroupRef = useRef(false);
  const didResetStoreRef = useRef(false);
  const sessionGroupsPanelRef = useRef<HTMLElement>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);
  const groupIdsRef = useRef<string[]>([]);
  /*
   * CDXC:SidebarV2GroupedProjectUX 2026-07-30:
   * The grouped V2 rows AS RENDERED, reported up by SidebarV2Root. A project drag
   * resolves its drop from the pointer against the rows on screen, so the
   * candidate list has to be those exact rows: re-deriving the logical grouping
   * here would be a second copy of `groupSidebarV2ProjectsByLogicalKey`'s merge
   * rules, free to drift from the list the user is actually looking at.
   *
   * It is a SEPARATE ref rather than a V2 mode on `groupIdsRef`: that ref is
   * V1's, several V1 paths read it during a drag, and swapping its contents under
   * them is a bigger change than the reorder feature needs.
   */
  const sidebarV2GroupOrderRowsRef = useRef<readonly SidebarV2GroupOrderRow[]>([]);
  const sessionIdsByGroupRef = useRef<SessionIdsByGroup>({});
  const pinnedSessionDropTargetLogKeyRef = useRef<string | undefined>(undefined);
  const previousSessionCountsByGroupRef = useRef<Record<string, number>>({});
  const latestSessionSearchPreviousRequestIdRef = useRef<string | undefined>(undefined);
  const didApplyStartupEmptyChatsCollapseRef = useRef(false);
  const hasEstablishedStartupGroupCollapseBaselineRef = useRef(false);
  const hasObservedAvailableGxserverStateRef = useRef(false);
  const previousNormalizedSessionSearchQueryRef = useRef("");
  const refreshDebugInstanceIdRef = useRef(createSidebarRefreshDebugInstanceId());
  const pointerDownSessionTargetRef = useRef<SidebarPointerDownSessionTarget | undefined>(
    undefined,
  );
  const sessionPointerDragStateRef = useRef<SidebarSessionPointerDragState | undefined>(undefined);
  const completionFlashTimeoutBySessionIdRef = useRef<Map<string, number>>(new Map());
  const referenceSectionAnimationTimeoutsRef = useRef<
    Partial<Record<ReferenceSidebarSectionId, number>>
  >({});
  const sessionGroupsContentRef = useRef<HTMLDivElement>(null);
  const sidebarStartupStartedAtRef = useRef(getSidebarStartupNow());
  const hasAppliedHydrateRef = useRef(false);
  const firstHydrateRevisionRef = useRef<number | undefined>(undefined);
  const lastSidebarStartupRenderStateKeyRef = useRef<string | undefined>(undefined);
  const didLogRefreshInstanceObservedRef = useRef(false);
  const didLogInitialUiCollapseStateReadRef = useRef(false);
  const collapseStateHydrateLogCountRef = useRef(0);
  const lastCollapseStateHydrateShapeRef = useRef<string | undefined>(undefined);
  const focusedSessionScrollLogSequenceRef = useRef(0);
  const previousFocusedSessionRevealRequestIdRef = useRef(focusedSessionRevealRequestId);
  const handledSessionRevealRequestIdRef = useRef<number | undefined>(undefined);
  const pendingSessionRevealScrollRequestIdRef = useRef<number | undefined>(undefined);

  if (!didResetStoreRef.current) {
    resetSidebarStore();
    didResetStoreRef.current = true;
  }

  useEffect(() => {
    return () => {
      setSidebarTooltipsSuppressedForDrag(false);
    };
  }, []);

  useEffect(() => {
    writeSidebarHiddenItems({ collectionKeys: hiddenCollectionKeys, groupIds: hiddenGroupIds });
  }, [hiddenCollectionKeys, hiddenGroupIds]);

  useEffect(() => {
    if (!enableProjectCollections) {
      return;
    }
    writeSidebarProjectCollections(projectCollections);
    /*
    CDXC:SidebarProjectCollections 2026-07-18-00:00:
    localStorage stays the instant-edit overlay, but every local collection
    edit also write-through-syncs the whole wire state to gxserver via the
    host so React Native Android sees the same colored "Group N" overlay. States that
    just arrived from (or were already pushed to) the server are skipped to
    avoid echo loops.
    */
    if (
      areSidebarProjectCollectionsStatesEqual(
        lastGxserverSyncedProjectCollectionsRef.current,
        projectCollections,
      )
    ) {
      return;
    }
    lastGxserverSyncedProjectCollectionsRef.current = projectCollections;
    vscode.postMessage({
      state: serializeSidebarProjectCollectionsForGxserver(projectCollections),
      type: "updateSidebarProjectCollections",
    });
  }, [enableProjectCollections, projectCollections, vscode]);

  const applyLocalFocus = useSidebarStore((state) => state.applyLocalFocus);
  const consumeFocusedSessionScrollSuppression = useSidebarStore(
    (state) => state.consumeFocusedSessionScrollSuppression,
  );
  const applyCommandRunStateClearedMessage = useSidebarStore(
    (state) => state.applyCommandRunStateClearedMessage,
  );
  const applyCommandRunStateMessage = useSidebarStore((state) => state.applyCommandRunStateMessage);
  const applyGroupsChangedMessage = useSidebarStore((state) => state.applyGroupsChangedMessage);
  const applyHudChangedMessage = useSidebarStore((state) => state.applyHudChangedMessage);
  const applyOrderSyncResultMessage = useSidebarStore((state) => state.applyOrderSyncResultMessage);
  const applySessionPresentationMessage = useSidebarStore(
    (state) => state.applySessionPresentationMessage,
  );
  const applySidebarMessage = useSidebarStore((state) => state.applySidebarMessage);
  const setDaemonSessionsState = useSidebarStore((state) => state.setDaemonSessionsState);
  const setGitCommitDraft = useSidebarStore((state) => state.setGitCommitDraft);
  const setGitFileDiffDraft = useSidebarStore((state) => state.setGitFileDiffDraft);
  const {
    activeSessionsSortMode,
    agentManagerZoomPercent,
    agents,
    createSessionOnSidebarDoubleClick,
    customThemeColor,
    debuggingMode,
    groupOrder,
    groupsById,
    previousSessions,
    projectSettingsProjects,
    recentProjects,
    settings,
    revision,
    sessionsById,
    sidebarAutoSettleAfterDays,
    sidebarAutoSettleAfterDaysByMachineId,
    sidebarLifecycleCapabilities,
    sidebarLifecycleCapabilitiesByMachineId,
    theme,
    workspaceGroupIds,
  } = useSidebarStore(
    useShallow((state) => ({
      activeSessionsSortMode: state.hud.activeSessionsSortMode,
      agentManagerZoomPercent: state.hud.agentManagerZoomPercent,
      agents: state.hud.agents,
      createSessionOnSidebarDoubleClick: state.hud.createSessionOnSidebarDoubleClick,
      customThemeColor: state.hud.customThemeColor,
      debuggingMode: state.hud.debuggingMode,
      groupOrder: state.groupOrder,
      groupsById: state.groupsById,
      /*
       * CDXC:SidebarV2LogicalProjects 2026-07-29:
       * The auto-settle window is machine-scoped for the same reason capability
       * is, so it travels on the HUD next to it rather than being read off the
       * local settings for every row.
       */
      sidebarAutoSettleAfterDays: state.hud.autoSettleAfterDays,
      sidebarAutoSettleAfterDaysByMachineId: state.hud.autoSettleAfterDaysByMachineId,
      sidebarLifecycleCapabilities: state.hud.lifecycleCapabilities,
      sidebarLifecycleCapabilitiesByMachineId: state.hud.lifecycleCapabilitiesByMachineId,
      previousSessions: state.previousSessions,
      projectSettingsProjects: state.hud.projectSettingsProjects,
      recentProjects: state.hud.recentProjects,
      revision: state.revision,
      settings: state.hud.settings,
      sessionsById: state.sessionsById,
      theme: state.hud.theme,
      workspaceGroupIds: state.workspaceGroupIds,
    })),
  );
  const gitCommitDraft = useSidebarStore((state) => state.gitCommitDraft);
  const gitFileDiffDraft = useSidebarStore((state) => state.gitFileDiffDraft);
  const authoritativeSessionIdsByGroup = useSidebarStore((state) => state.sessionIdsByGroup);
  const [ remoteMachineRuntimeStatuses, setRemoteMachineRuntimeStatuses ] =
    useState<RemoteMachineRuntimeStatuses>({});
  const [ remoteMachineStatusMessages, setRemoteMachineStatusMessages ] =
    useState<RemoteMachineStatusMessages>({});
  const [ primaryAgentLauncherId, setPrimaryAgentLauncherId ] = useState(readPrimaryAgentLauncherId);
  const [ sidebarKeepAwakeRuntime, setSidebarKeepAwakeRuntime ] = useState(
    readSidebarKeepAwakeRuntime,
  );
  const buildStamp = useSidebarStore((state) =>
    state.hud.debuggingMode ? state.hud.buildStamp : undefined,
  );
  const hasGxserverUnavailablePlaceholder = Boolean(
    groupsById[ SIDEBAR_GXSERVER_UNAVAILABLE_GROUP_ID ],
  );
  const hasAvailableGxserverState =
    !hasGxserverUnavailablePlaceholder && groupOrder.length > 0;

  useEffect(() => {
    if (hasAvailableGxserverState) {
      hasObservedAvailableGxserverStateRef.current = true;
    }
  }, [ hasAvailableGxserverState ]);

  useEffect(() => {
    if (!hasGxserverUnavailablePlaceholder) {
      setShowGxserverUnavailableEmptyState(false);
      return;
    }

    /*
     * CDXC:GPUIGxserverLiveDisconnect 2026-07-14:
     * The 20-second grace period below is only for cold startup while gxserver
     * may still recover. GPUI supplies the explicit start action, so once this
     * mounted sidebar has already rendered an available daemon state, a later
     * unavailable hydrate is a live disconnect and must expose the recovery
     * message and button immediately instead of leaving Projects blank.
     */
    if (onStartGxserver && hasObservedAvailableGxserverStateRef.current) {
      setShowGxserverUnavailableEmptyState(true);
      return;
    }

    /*
     * CDXC:GxserverPresentation 2026-06-16-09:35:
     * When gxserver is off or missing during startup, the sidebar must not show
     * the raw synthetic status project row. Keep the Projects body blank while
     * startup can still recover, then after 20 seconds show the two-line restart
     * guidance using the exact reference-sidebar empty-state typography shared
     * with "No projects."
     */
    setShowGxserverUnavailableEmptyState(false);
    const timeoutId = window.setTimeout(() => {
      setShowGxserverUnavailableEmptyState(true);
    }, SIDEBAR_GXSERVER_UNAVAILABLE_EMPTY_STATE_DELAY_MS);

    return () => {
      window.clearTimeout(timeoutId);
    };
  }, [ hasGxserverUnavailablePlaceholder, onStartGxserver ]);

  const effectiveSettings = settings ?? DEFAULT_ghostex_SETTINGS;
  /*
   * CDXC:SidebarV2 2026-07-29:
   * The sidebar version rides the shared settings pipeline, so the sidebar
   * reads it from the same hydrated snapshot every other setting comes from.
   * A host that has not sent settings yet keeps the classic sidebar.
   */
  const sidebarVersion: SidebarVersion = effectiveSettings.sidebarVersion;
  const sidebarV2Layout: SidebarV2Layout = effectiveSettings.sidebarV2Layout;
  const isSidebarV2Active = sidebarVersion === "v2";
  /*
   * CDXC:SidebarV2GroupedProjectUX 2026-07-30:
   * Grouped V2 is the ONE V2 layout that renders project rows, so it is the only
   * one that owns project reorder. The flat inbox has no project rows to drag,
   * and letting a group drag resolve against it would resolve against nothing.
   */
  const isSidebarV2GroupedActive = isSidebarV2Active && sidebarV2Layout === "byProject";
  const showSidebarKeepAwakeButton =
    effectiveSettings.showBetaFeatures && !effectiveSettings.hideKeepAwakeTitlebarControl;
  const sidebarRefreshDiagnosticLoggingEnabled = isDiagnosticLoggingScenarioEnabled(
    effectiveSettings.diagnosticLogging,
    "native.sidebar.refresh",
  );
  const sidebarCollapseDiagnosticLoggingEnabled = isDiagnosticLoggingScenarioEnabled(
    effectiveSettings.diagnosticLogging,
    "native.sidebar.collapse",
  );

  useEffect(() => {
    const refreshKeepAwakeRuntime = () => {
      setSidebarKeepAwakeRuntime(readSidebarKeepAwakeRuntime());
    };
    window.addEventListener("focus", refreshKeepAwakeRuntime);
    window.addEventListener("storage", refreshKeepAwakeRuntime);
    document.addEventListener("visibilitychange", refreshKeepAwakeRuntime);
    return () => {
      window.removeEventListener("focus", refreshKeepAwakeRuntime);
      window.removeEventListener("storage", refreshKeepAwakeRuntime);
      document.removeEventListener("visibilitychange", refreshKeepAwakeRuntime);
    };
  }, []);

  const sidebarSessionTagListItems = useMemo(
    () => normalizeSidebarSessionTagListItems(effectiveSettings.sidebarSessionTagListItems),
    [ effectiveSettings.sidebarSessionTagListItems ],
  );
  const enabledVisibleSidebarSessionTagSet = useMemo(
    () => new Set(getEnabledVisibleSidebarSessionTagFilters(sidebarSessionTagListItems)),
    [ sidebarSessionTagListItems ],
  );
  const activeSelectedSessionTagFilters = useMemo(
    () =>
      selectedSessionTagFilters.filter((tag) => enabledVisibleSidebarSessionTagSet.has(tag)),
    [ enabledVisibleSidebarSessionTagSet, selectedSessionTagFilters ],
  );

  useEffect(() => {
    /*
     * CDXC:SessionTagFilters 2026-06-13-17:50:
     * If a selected sidebar tag filter becomes hidden or disabled from
     * Settings, drop it from the active filter state so sessions are not
     * invisibly filtered by a tag the sidebar menu no longer lets users choose.
     */
    setSelectedSessionTagFilters((current) => {
      const next = current.filter((tag) => enabledVisibleSidebarSessionTagSet.has(tag));
      return next.length === current.length ? current : next;
    });
  }, [ enabledVisibleSidebarSessionTagSet ]);

  useEffect(() => {
    const refreshPrimaryAgentLauncher = (event: Event) => {
      const changedEvent = event as PrimaryAgentLauncherChangedEvent;
      setPrimaryAgentLauncherId(
        typeof changedEvent.detail?.agentId === "string"
          ? changedEvent.detail.agentId
          : readPrimaryAgentLauncherId(),
      );
    };

    window.addEventListener(PRIMARY_AGENT_LAUNCHER_CHANGED_EVENT, refreshPrimaryAgentLauncher);
    return () => {
      window.removeEventListener(PRIMARY_AGENT_LAUNCHER_CHANGED_EVENT, refreshPrimaryAgentLauncher);
    };
  }, []);

  const postSidebarDebugLog = useEffectEvent((
    scenarioId: DiagnosticLoggingScenarioId,
    event: string,
    details: unknown,
  ) => {
    if (!debuggingMode) {
      return;
    }

    logSidebarDebug(debuggingMode, event, details);
    vscode.postMessage({
      details,
      event,
      scenarioId,
      type: "sidebarDebugLog",
    });
  });

  const postSidebarCollapseStateLog = useEffectEvent(
    (
      event: string,
      details: Record<string, unknown>,
      options: { enabled?: boolean; } = {},
    ) => {
      /*
       * CDXC:SidebarCollapseDiagnostics 2026-06-02-23:52:
       * Sidebar restart repros need a dedicated low-volume trace for localStorage
       * collapse-state reads, writes, hydrate timing, and user toggles. Keep the
       * payload privacy-safe by recording counts, booleans, revisions, elapsed
       * timings, and hashed group identifiers instead of project names or paths.
       */
      if (!(options.enabled ?? sidebarCollapseDiagnosticLoggingEnabled)) {
        return;
      }

      vscode.postMessage({
        details: {
          ...details,
          elapsedMs: getSidebarStartupElapsedMs(sidebarStartupStartedAtRef.current),
          firstHydrateRevision: firstHydrateRevisionRef.current,
          hasEstablishedStartupGroupCollapseBaseline:
            hasEstablishedStartupGroupCollapseBaselineRef.current,
          hasHydrate: hasAppliedHydrateRef.current,
          instanceId: refreshDebugInstanceIdRef.current,
          revision,
        },
        event: `${SIDEBAR_COLLAPSE_STATE_DEBUG_EVENT_PREFIX}${event}`,
        scenarioId: "native.sidebar.collapse",
        type: "sidebarDebugLog",
      });
    },
  );

  const postPinnedSessionReorderLog = useEffectEvent((event: string, details: unknown) => {
    /*
     * CDXC:PinnedSessions 2026-05-28-15:33:
     * Pinned reorder failures need click-scoped repro breadcrumbs even when
     * broad Debugging Mode is off. Keep these events low-volume and explicit
     * so a user drag can reveal which guard prevented syncSessionOrder.
     */
    vscode.postMessage({
      details,
      event: `repro.pinnedSessionReorder.${event}`,
      scenarioId: "native.pane.reorder",
      type: "sidebarDebugLog",
    });
  });

  const postSidebarStartupReproLog = useEffectEvent((event: string, details: unknown) => {
    if (
      getSidebarStartupElapsedMs(sidebarStartupStartedAtRef.current) >
      SIDEBAR_STARTUP_REPRO_WINDOW_MS
    ) {
      return;
    }

    vscode.postMessage({
      details,
      event: `repro.sidebarStartup.${event}`,
      scenarioId: "native.sidebar.refresh",
      type: "sidebarDebugLog",
    });
  });

  const postSidebarRefreshLifecycleLog = useEffectEvent(
    (event: string, details: Record<string, unknown>) => {
      const currentSettings = useSidebarStore.getState().hud.settings ?? DEFAULT_ghostex_SETTINGS;
      postSidebarRefreshDebugLog(
        isDiagnosticLoggingScenarioEnabled(
          currentSettings.diagnosticLogging,
          "native.sidebar.refresh",
        ),
        vscode,
        event,
        details,
      );
    },
  );

  useLayoutEffect(() => {
    if (!hasAppliedHydrateRef.current) {
      return;
    }

    const autoCollapseGroupIds = getAutoCollapseGroupIds({
      groupsById,
      workspaceGroupIds,
    });
    const nextSessionCountsByGroup = getSessionCountsByGroup({
      groupIds: groupOrder,
      sessionIdsByGroup: authoritativeSessionIdsByGroup,
    });
    const isEstablishingStartupGroupCollapseBaseline =
      !hasEstablishedStartupGroupCollapseBaselineRef.current;
    const hasGxserverUnavailablePlaceholder = groupOrder.includes(
      SIDEBAR_GXSERVER_UNAVAILABLE_GROUP_ID,
    );
    const visibleGroupIds = new Set(groupOrder);
    const unknownCollapsedGroupCount = Object.keys(collapsedGroupsById).filter(
      (groupId) => !visibleGroupIds.has(groupId),
    ).length;
    const preserveUnknownCollapsedGroups =
      isEstablishingStartupGroupCollapseBaseline && hasGxserverUnavailablePlaceholder;
    const sessionCountIncreaseGroupIds = isEstablishingStartupGroupCollapseBaseline
      ? []
      : groupOrder.filter((groupId) => {
        const previousCount = previousSessionCountsByGroupRef.current[ groupId ];
        return (
          previousCount !== undefined &&
          (authoritativeSessionIdsByGroup[ groupId ] ?? []).length > previousCount
        );
      });

    if (preserveUnknownCollapsedGroups && unknownCollapsedGroupCount > 0) {
      postSidebarCollapseStateLog("startupPartialHydratePreserved", {
        groupCount: groupOrder.length,
        placeholderGroupPresent: true,
        unknownCollapsedGroupCount,
      });
    }

    setCollapsedGroupsById((previous) =>
      reconcileCollapsedGroupsById({
        autoCollapseGroupIds,
        expandOnSessionCountIncreaseGroupIds: groupOrder,
        groupIds: groupOrder,
        preserveUnknownCollapsedGroups,
        previousSessionCountsByGroup: previousSessionCountsByGroupRef.current,
        previousCollapsedGroupsById: previous,
        sessionIdsByGroup: authoritativeSessionIdsByGroup,
        skipExpandOnSessionCountIncrease: isEstablishingStartupGroupCollapseBaseline,
      }),
    );

    /**
     * CDXC:SidebarReference 2026-05-08-11:09
     * When creating a chat, terminal, browser pane, or agent session inside a
     * collapsed Combined sidebar area, expand the owning Chats/Projects section
     * as soon as the host hydrates the added session so the user sees the
     * result of the action.
     * CDXC:SidebarReference 2026-05-20-12:00
     * Do not expand Chats/Projects section headers on the first post-hydrate
     * baseline pass after restart. Restored session counts are not new sessions.
     */
    if (sessionCountIncreaseGroupIds.some((groupId) => groupsById[ groupId ]?.isChatCollection)) {
      postSidebarCollapseStateLog("sectionAutoExpanded", {
        reason: "session-count-increase",
        section: "quick",
        sessionCountIncreaseGroupCount: sessionCountIncreaseGroupIds.length,
      });
      setIsReferenceChatsCollapsed(false);
    }

    if (sessionCountIncreaseGroupIds.some((groupId) => !groupsById[ groupId ]?.isChatCollection)) {
      postSidebarCollapseStateLog("sectionAutoExpanded", {
        reason: "session-count-increase",
        section: "projects",
        sessionCountIncreaseGroupCount: sessionCountIncreaseGroupIds.length,
      });
      setIsReferenceProjectsCollapsed(false);
    }

    previousSessionCountsByGroupRef.current = nextSessionCountsByGroup;
    if (isEstablishingStartupGroupCollapseBaseline && !hasGxserverUnavailablePlaceholder) {
      postSidebarCollapseStateLog("startupBaselineEstablished", {
        groupCount: groupOrder.length,
        sessionCount: Object.keys(sessionsById).length,
      });
      hasEstablishedStartupGroupCollapseBaselineRef.current = true;
    }
  }, [
    authoritativeSessionIdsByGroup,
    collapsedGroupsById,
    groupOrder,
    groupsById,
    sessionsById,
    workspaceGroupIds,
  ]);

  const isSidebarInteractionBlocked = isStartupInteractionBlocked;

  const setGroupCollapsed = (groupId: string, collapsed: boolean) => {
    const wasCollapsed = collapsedGroupsById[ groupId ] === true;
    const collapsedGroupCountBefore = Object.keys(collapsedGroupsById).length;
    postSidebarCollapseStateLog("groupToggle", {
      changed: wasCollapsed !== collapsed,
      collapsed,
      collapsedGroupCountBefore,
      collapsedGroupCountExpectedAfter:
        collapsedGroupCountBefore + (wasCollapsed === collapsed ? 0 : collapsed ? 1 : -1),
      groupHash: hashSidebarCollapseDebugId(groupId),
      groupIndex: groupOrder.indexOf(groupId),
      wasCollapsed,
    });
    setCollapsedGroupsById((previous) => {
      if (collapsed) {
        if (previous[ groupId ]) {
          return previous;
        }

        return {
          ...previous,
          [ groupId ]: true,
        };
      }

      if (!previous[ groupId ]) {
        return previous;
      }

      const next = { ...previous };
      delete next[ groupId ];
      return next;
    });
  };

  const setGroupsCollapsed = (groupIds: readonly string[], collapsed: boolean) => {
    const targetGroupSet = new Set(groupIds);
    const collapsedGroupCountBefore = Object.keys(collapsedGroupsById).length;
    const changedGroupCount = groupIds.filter(
      (groupId) => collapsedGroupsById[ groupId ] !== (collapsed ? true : undefined),
    ).length;
    postSidebarCollapseStateLog("groupsBulkToggle", {
      changedGroupCount,
      collapsed,
      collapsedGroupCountBefore,
      collapsedGroupCountExpectedAfter:
        collapsedGroupCountBefore + (collapsed ? changedGroupCount : -changedGroupCount),
      groupHashes: summarizeSidebarCollapseDebugGroupIds(groupIds),
      targetGroupCount: targetGroupSet.size,
    });
    setCollapsedGroupsById((previous) => {
      if (collapsed) {
        const next = { ...previous };
        let changed = false;
        for (const groupId of groupIds) {
          if (!next[ groupId ]) {
            next[ groupId ] = true;
            changed = true;
          }
        }
        return changed ? next : previous;
      }

      let next: Record<string, true> | undefined;
      for (const groupId of groupIds) {
        if (previous[ groupId ]) {
          next ??= { ...previous };
          delete next[ groupId ];
        }
      }
      return next ?? previous;
    });
  };

  const setProjectCollectionCollapsed = (collectionKey: string, collapsed: boolean) => {
    setCollapsedProjectCollectionsByKey((previous) => {
      if (collapsed) {
        return previous[ collectionKey ] ? previous : { ...previous, [ collectionKey ]: true };
      }
      if (!previous[ collectionKey ]) {
        return previous;
      }
      const next = { ...previous };
      delete next[ collectionKey ];
      return next;
    });
  };

  const setProjectSessionListCollapsed = (projectId: string, collapsed: boolean) => {
    setCollapsedProjectSessionListsById((previous) => {
      if (collapsed) {
        return previous[ projectId ] ? previous : { ...previous, [ projectId ]: true };
      }
      if (!previous[ projectId ]) {
        return previous;
      }
      const next = { ...previous };
      delete next[ projectId ];
      return next;
    });
  };

  const setRemoteMachineSectionCollapsed = (machineId: string, collapsed: boolean) => {
    const wasCollapsed = collapsedRemoteMachineSectionsById[ machineId ] === true;
    postSidebarCollapseStateLog("remoteMachineSectionToggle", {
      changed: wasCollapsed !== collapsed,
      collapsed,
      machineHash: hashSidebarCollapseDebugId(machineId),
      wasCollapsed,
    });
    /*
     * CDXC:RemoteMachines 2026-06-09-19:02:
     * Remote machine sections are peers of Quick and Projects in the reference
     * sidebar. Persist their collapsed state by saved machine id so each machine
     * can collapse independently without affecting local project groups.
     */
    setCollapsedRemoteMachineSectionsById((previous) => {
      if (collapsed) {
        if (previous[ machineId ]) {
          return previous;
        }

        return {
          ...previous,
          [ machineId ]: true,
        };
      }

      if (!previous[ machineId ]) {
        return previous;
      }

      const next = { ...previous };
      delete next[ machineId ];
      return next;
    });
  };

  const dismissAppModalForSidebarNavigation = (area: string) => {
    /*
     * CDXC:SettingsDismissal 2026-06-15-14:07:
     * Settings is a workspace-scoped app modal, but sidebar navigation should
     * always return users to the live workspace. Dismiss the native app-modal
     * host before session focus, session creation, sidebar nav buttons,
     * top-level modals, and direct previous-session text search.
     */
    setIsSettingsOpen(false);
    if (!window.webkit?.messageHandlers?.ghostexAppModalHost) {
      return;
    }
    closeAppModal(area);
  };

  const focusSidebarSessionFromNavigation = (groupId: string, sessionId: string) => {
    dismissAppModalForSidebarNavigation("SettingsDismissal:focusSession");
    useSidebarStore.getState().clearFocusedSessionScrollSuppression();
    applyLocalFocus(groupId, sessionId);
  };

  const requestNewSession = () => {
    if (isSidebarInteractionBlocked) {
      return;
    }

    dismissAppModalForSidebarNavigation("SettingsDismissal:createSession");
    vscode.postMessage({ type: "createSession" });
  };

  const handleSidebarDoubleClick = (event: ReactMouseEvent<HTMLElement>) => {
    if (!createSessionOnSidebarDoubleClick) {
      return;
    }

    if (!isEmptySidebarDoubleClick(event)) {
      return;
    }

    event.preventDefault();
    requestNewSession();
  };

  const handleSidebarClickCapture = (event: ReactMouseEvent<HTMLElement>) => {
    const target = event.target;
    if (!(target instanceof Element)) {
      return;
    }
    if (!target.closest(".session")) {
      return;
    }
    dismissAppModalForSidebarNavigation("SettingsDismissal:sessionClick");
  };

  const handleWindowMessage = useEffectEvent((event: MessageEvent<ExtensionToSidebarMessage>) => {
    if (!event.data) {
      return;
    }

    if (event.data.type === "hydrate" && enableProjectCollections) {
      const nextRemoteCollections: Record<string, SidebarProjectCollectionsState> = {};
      for (const [machineId, state] of Object.entries(
        event.data.remoteSidebarProjectCollectionsByMachineId ?? {},
      )) {
        const parsed = parseSidebarProjectCollectionsFromGxserver(state);
        if (parsed) {
          nextRemoteCollections[machineId] = parsed;
        }
      }
      setRemoteProjectCollectionsByMachineId(nextRemoteCollections);
    }

    if (event.data.type === "gpuiProjectSlotHotkey") {
      resolveGpuiProjectSlotHotkey(event.data.slotNumber);
      return;
    }

    if (event.data.type === "nativeHotkey") {
      runGhostexHotkeyAction(event.data.actionId);
      return;
    }

    if (event.data.type === "playCompletionSound") {
      const sessionId = event.data.sessionId;
      postSidebarDebugLog("native.agent.detection", "completionSound.messageReceived", {
        sound: event.data.sound,
        sessionId,
      });
      if (sessionId) {
        const existingTimeout = completionFlashTimeoutBySessionIdRef.current.get(sessionId);
        if (existingTimeout !== undefined) {
          window.clearTimeout(existingTimeout);
        }
        setCompletionFlashNonceBySessionId((previous) => ({
          ...previous,
          [ sessionId ]: (previous[ sessionId ] ?? 0) + 1,
        }));
        const timeout = window.setTimeout(() => {
          completionFlashTimeoutBySessionIdRef.current.delete(sessionId);
          setCompletionFlashNonceBySessionId((previous) => {
            if (!(sessionId in previous)) {
              return previous;
            }

            const next = { ...previous };
            delete next[ sessionId ];
            return next;
          });
        }, COMPLETION_FLASH_DURATION_MS);
        completionFlashTimeoutBySessionIdRef.current.set(sessionId, timeout);
      }
      void playCompletionSound(event.data.sound, (soundEvent, details) => {
        postSidebarDebugLog("native.agent.detection", soundEvent, details);
      });
      return;
    }

    if (event.data.type === "sessionPresentationChanged") {
      applySessionPresentationMessage(event.data);
      return;
    }

    if (event.data.type === "sidebarGroupsChanged") {
      applyGroupsChangedMessage(event.data);
      return;
    }

    if (event.data.type === "sidebarProjectCollectionsChanged") {
      /*
      CDXC:SidebarProjectCollections 2026-07-18-00:00:
      gxserver's normalized copy is authoritative whenever it has collections;
      adopt it into the localStorage-backed state so edits from React Native Android or
      another desktop land here. An empty server copy while local collections
      exist means gxserver has no durable state yet (first run after the
      server-backed cutover), so seed it from the local overlay instead of
      wiping the user's groups. nextCollectionNumber keeps the local maximum
      so "Group N" numbering never goes backwards.
      */
      if (!enableProjectCollections) {
        return;
      }
      const parsed = parseSidebarProjectCollectionsFromGxserver(
        event.data.sidebarProjectCollections,
      );
      if (!parsed) {
        return;
      }
      const remoteMachineId = event.data.remoteMachineId;
      if (remoteMachineId) {
        setRemoteProjectCollectionsByMachineId((previous) => ({
          ...previous,
          [remoteMachineId]: parsed,
        }));
        return;
      }
      if (parsed.collections.length === 0) {
        if (projectCollections.collections.length > 0) {
          lastGxserverSyncedProjectCollectionsRef.current = projectCollections;
          vscode.postMessage({
            state: serializeSidebarProjectCollectionsForGxserver(projectCollections),
            type: "updateSidebarProjectCollections",
          });
        }
        return;
      }
      const adopted: SidebarProjectCollectionsState = {
        collections: parsed.collections,
        nextCollectionNumber: Math.max(
          parsed.nextCollectionNumber,
          projectCollections.nextCollectionNumber,
        ),
      };
      lastGxserverSyncedProjectCollectionsRef.current = adopted;
      if (!areSidebarProjectCollectionsStatesEqual(adopted, projectCollections)) {
        setProjectCollections(adopted);
      }
      return;
    }

    if (event.data.type === "sidebarHudChanged") {
      applyHudChangedMessage(event.data);
      return;
    }

    if (event.data.type === "sidebarCommandRunStateChanged") {
      applyCommandRunStateMessage(event.data);
      return;
    }

    if (event.data.type === "sidebarCommandRunStateCleared") {
      applyCommandRunStateClearedMessage(event.data);
      return;
    }

    if (event.data.type === "revealSidebarSession") {
      setSessionRevealRequest({
        requestId: event.data.requestId,
        sessionId: event.data.sessionId,
      });
      return;
    }

    if (event.data.type === "sidebarOrderSyncResult") {
      postSidebarOrderReproLog(vscode, "repro.sidebarOrder.webview.syncResultReceived", {
        itemIds: event.data.itemIds,
        kind: event.data.kind,
        requestId: event.data.requestId,
        status: event.data.status,
      });
      applyOrderSyncResultMessage(event.data);
      return;
    }

    if (event.data.type === "daemonSessionsState") {
      setDaemonSessionsState(event.data);
      return;
    }

    if (event.data.type === "promptGitCommit") {
      setGitCommitDraft(event.data);
      return;
    }

    if (event.data.type === "sidebarGitFileDiff") {
      /*
      CDXC:GitReview 2026-06-24-15:22:
      Inline commit-review diffs may now arrive from any shared SidebarApp host.
      Apply them only to the matching open request so an async gxserver diff from an older review cannot populate a later modal.
      */
      if (useSidebarStore.getState().gitCommitDraft?.requestId === event.data.requestId) {
        setGitFileDiffDraft(event.data.draft);
      }
      return;
    }

    if (event.data.type === "previousSessionsResult") {
      if (event.data.requestId !== latestSessionSearchPreviousRequestIdRef.current) {
        return;
      }
      setRemoteSessionSearchPreviousSessions(event.data.previousSessions);
      return;
    }

    if (event.data.type === "remoteMachineStatus") {
      const remoteMachineStatus = event.data as RemoteMachineRuntimeStatus;
      setRemoteMachineRuntimeStatuses((current) => ({
        ...current,
        [ remoteMachineStatus.machineId ]: remoteMachineStatus.state,
      }));
      setRemoteMachineStatusMessages((current) => {
        const message = remoteMachineStatus.message?.trim();
        if (message) {
          return { ...current, [ remoteMachineStatus.machineId ]: message };
        }
        if (current[ remoteMachineStatus.machineId ] === undefined) {
          return current;
        }
        const next = { ...current };
        delete next[ remoteMachineStatus.machineId ];
        return next;
      });
      return;
    }

    if (event.data.type === "showSessionRenameModal") {
      dismissAppModalForSidebarNavigation("SettingsDismissal:renameSession");
      openAppModal({
        initialTitle: event.data.initialTitle,
        modal: "renameSession",
        sessionAgentIcon: event.data.sessionAgentIcon,
        sessionId: event.data.sessionId,
        type: "open",
      });
      return;
    }

    if (event.data.type !== "hydrate" && event.data.type !== "sessionState") {
      return;
    }

    postSidebarOrderReproLog(vscode, "repro.sidebarOrder.webview.messageReceived", {
      agentIds: event.data.hud.agents.map((agent) => agent.agentId),
      commandIds: event.data.hud.commands.map((command) => command.commandId),
      groupCount: event.data.groups.length,
      groupIds: event.data.groups.map((group) => group.groupId),
      messageType: event.data.type,
      revision: event.data.revision,
    });
    postSidebarStartupReproLog("messageReceived", {
      elapsedMs: getSidebarStartupElapsedMs(sidebarStartupStartedAtRef.current),
      groupCount: event.data.groups.length,
      hasHydrateBeforeMessage: hasAppliedHydrateRef.current,
      firstHydrateRevision: firstHydrateRevisionRef.current,
      messageType: event.data.type,
      previousRevision: revision,
      revision: event.data.revision,
      sessionCount: countSidebarSessions(event.data.groups),
      stale: event.data.revision < revision,
      startupInteractionBlocked: isStartupInteractionBlocked,
    });
    const messageSettings = event.data.hud.settings ?? effectiveSettings;
    const messageSidebarRefreshDiagnosticLoggingEnabled = isDiagnosticLoggingScenarioEnabled(
      messageSettings.diagnosticLogging,
      "native.sidebar.refresh",
    );
    const messageSidebarCollapseDiagnosticLoggingEnabled = isDiagnosticLoggingScenarioEnabled(
      messageSettings.diagnosticLogging,
      "native.sidebar.collapse",
    );
    postSidebarRefreshDebugLog(messageSidebarRefreshDiagnosticLoggingEnabled, vscode, "messageReceived", {
      ...summarizeSidebarRefreshMessage(event.data, revision),
      hasHydrateBeforeMessage: hasAppliedHydrateRef.current,
      instanceId: refreshDebugInstanceIdRef.current,
    });
    const sidebarCollapseMessageSessionCount = countSidebarSessions(event.data.groups);
    const sidebarCollapseMessageShape = [
      event.data.type,
      event.data.groups.length,
      sidebarCollapseMessageSessionCount,
      event.data.revision < revision ? "stale" : "fresh",
    ].join(":");
    const shouldLogSidebarCollapseHydrateMessage =
      messageSidebarCollapseDiagnosticLoggingEnabled &&
      getSidebarStartupElapsedMs(sidebarStartupStartedAtRef.current) <=
      SIDEBAR_STARTUP_REPRO_WINDOW_MS &&
      (collapseStateHydrateLogCountRef.current < 8 ||
        lastCollapseStateHydrateShapeRef.current !== sidebarCollapseMessageShape);
    if (shouldLogSidebarCollapseHydrateMessage) {
      /**
       * CDXC:SidebarCollapseDiagnostics 2026-06-02-22:18:
       * Collapse-state startup logs need the first hydrate sequence and shape
       * changes, not every repeated gxserver presentation refresh. Limit the
       * high-frequency message logs so support bundles stay readable while
       * still capturing partial 2-group startup hydrates.
       */
      collapseStateHydrateLogCountRef.current += 1;
      lastCollapseStateHydrateShapeRef.current = sidebarCollapseMessageShape;
      postSidebarCollapseStateLog(
        "messageReceived",
        {
          collapsedGroupCount: Object.keys(collapsedGroupsById).length,
          groupCount: event.data.groups.length,
          isReferenceChatsCollapsed,
          isReferenceProjectsCollapsed,
          messageRevision: event.data.revision,
          messageType: event.data.type,
          sessionCount: sidebarCollapseMessageSessionCount,
          stale: event.data.revision < revision,
        },
        { enabled: true },
      );
    }
    if (
      messageSidebarRefreshDiagnosticLoggingEnabled &&
      !didLogRefreshInstanceObservedRef.current
    ) {
      didLogRefreshInstanceObservedRef.current = true;
      postSidebarRefreshDebugLog(messageSidebarRefreshDiagnosticLoggingEnabled, vscode, "appInstanceObserved", {
        elapsedMs: getSidebarStartupElapsedMs(sidebarStartupStartedAtRef.current),
        instanceId: refreshDebugInstanceIdRef.current,
        messageType: event.data.type,
        revision: event.data.revision,
      });
    }
    if (event.data.type === "sessionState" && !hasAppliedHydrateRef.current) {
      postSidebarStartupReproLog("sessionStateBeforeHydrate", {
        elapsedMs: getSidebarStartupElapsedMs(sidebarStartupStartedAtRef.current),
        previousRevision: revision,
        revision: event.data.revision,
        sessionCount: countSidebarSessions(event.data.groups),
      });
    }
    /*
     * CDXC:AgentDetection 2026-04-27-07:29
     * Agent-icon debugging must verify the message boundary, not the CSS layer:
     * log whether native-projected agentIcon values reach the sidebar webview
     * and survive the Zustand store apply step.
     */
    postSidebarAgentIconBoundaryLog(vscode, "sidebar.agentIcon.messageReceived", {
      messageType: event.data.type,
      revision: event.data.revision,
      summary: summarizeSidebarAgentIconsFromGroups(event.data.groups),
    });

    if (pendingCreateGroupRef.current) {
      const nextGroupId = findCreatedGroupId(
        groupOrder,
        event.data.groups.map((group) => group.groupId),
      );
      if (nextGroupId) {
        setAutoEditingGroupId(nextGroupId);
        pendingCreateGroupRef.current = false;
      }
    }

    applySidebarMessage(event.data);
    postSidebarRefreshDebugLog(messageSidebarRefreshDiagnosticLoggingEnabled, vscode, "messageApplied", {
      ...summarizeSidebarRefreshMessage(event.data, revision),
      hasHydrateAfterApply: hasAppliedHydrateRef.current,
      instanceId: refreshDebugInstanceIdRef.current,
      storeRevisionAfterApply: useSidebarStore.getState().revision,
      storeSessionCountAfterApply: Object.keys(useSidebarStore.getState().sessionsById).length,
    });
    postSidebarAgentIconBoundaryLog(vscode, "sidebar.agentIcon.messageApplied", {
      messageType: event.data.type,
      revision: event.data.revision,
      summary: summarizeSidebarAgentIconsFromStore(useSidebarStore.getState().sessionsById),
    });
    if (event.data.type === "hydrate" && !hasAppliedHydrateRef.current) {
      hasAppliedHydrateRef.current = true;
      firstHydrateRevisionRef.current = event.data.revision;
    }
    if (shouldLogSidebarCollapseHydrateMessage) {
      postSidebarCollapseStateLog(
        "messageApplied",
        {
          collapsedGroupCount: Object.keys(collapsedGroupsById).length,
          groupCount: event.data.groups.length,
          isReferenceChatsCollapsed,
          isReferenceProjectsCollapsed,
          messageRevision: event.data.revision,
          messageType: event.data.type,
          sessionCount: sidebarCollapseMessageSessionCount,
          storeCollapsedGroupCount: Object.keys(collapsedGroupsById).length,
          storeRevisionAfterApply: useSidebarStore.getState().revision,
        },
        { enabled: true },
      );
    }
    postSidebarStartupReproLog("messageApplied", {
      elapsedMs: getSidebarStartupElapsedMs(sidebarStartupStartedAtRef.current),
      groupCount: event.data.groups.length,
      hasHydrateAfterApply: hasAppliedHydrateRef.current,
      firstHydrateRevision: firstHydrateRevisionRef.current,
      messageType: event.data.type,
      previousRevision: revision,
      revision: event.data.revision,
      sessionCount: countSidebarSessions(event.data.groups),
      stale: event.data.revision < revision,
      startupInteractionBlocked: isStartupInteractionBlocked,
    });
  });

  useEffect(() => {
    /*
    CDXC:SidebarRefreshDiagnostics 2026-06-06-23:18:
    The mount/unmount diagnostic must describe the React app lifetime only. Including effect-event callbacks in this dependency list made every hydrate render look like an app remount in persistent logs, hiding the real refresh cadence and adding avoidable Debugging Mode noise.
    */
    const instanceId = refreshDebugInstanceIdRef.current;
    postSidebarStartupReproLog("appMounted", {
      elapsedMs: getSidebarStartupElapsedMs(sidebarStartupStartedAtRef.current),
      startupInteractionBlockMs: SIDEBAR_STARTUP_INTERACTION_BLOCK_MS,
    });
    postSidebarRefreshLifecycleLog("appMounted", {
      elapsedMs: getSidebarStartupElapsedMs(sidebarStartupStartedAtRef.current),
      instanceId,
      revision: useSidebarStore.getState().revision,
      sessionCount: Object.keys(useSidebarStore.getState().sessionsById).length,
    });

    return () => {
      postSidebarStartupReproLog("appUnmounted", {
        elapsedMs: getSidebarStartupElapsedMs(sidebarStartupStartedAtRef.current),
        finalRevision: useSidebarStore.getState().revision,
      });
      postSidebarRefreshLifecycleLog("appUnmounted", {
        elapsedMs: getSidebarStartupElapsedMs(sidebarStartupStartedAtRef.current),
        finalRevision: useSidebarStore.getState().revision,
        instanceId,
        sessionCount: Object.keys(useSidebarStore.getState().sessionsById).length,
      });
    };
  }, []);

  useEffect(() => {
    if (
      !sidebarCollapseDiagnosticLoggingEnabled ||
      didLogInitialUiCollapseStateReadRef.current
    ) {
      return;
    }

    didLogInitialUiCollapseStateReadRef.current = true;
    postSidebarCollapseStateLog("initialRead", {
      ...summarizeSidebarUiCollapseRead(initialUiCollapseStateRead),
      currentCollapsedGroupCount: Object.keys(collapsedGroupsById).length,
      groupCount: groupOrder.length,
      sessionCount: Object.keys(sessionsById).length,
      workspaceGroupCount: workspaceGroupIds.length,
    });
  }, [
    collapsedGroupsById,
    groupOrder,
    initialUiCollapseStateRead,
    sidebarCollapseDiagnosticLoggingEnabled,
    sessionsById,
    workspaceGroupIds,
  ]);

  useEffect(() => {
    const renderState = {
      elapsedMs: getSidebarStartupElapsedMs(sidebarStartupStartedAtRef.current),
      firstHydrateRevision: firstHydrateRevisionRef.current,
      groupCount: groupOrder.length,
      hasHydrate: hasAppliedHydrateRef.current,
      revision,
      sessionCount: Object.keys(sessionsById).length,
      startupInteractionBlocked: isStartupInteractionBlocked,
      workspaceGroupCount: workspaceGroupIds.length,
    };
    const renderStateKey = JSON.stringify(renderState);
    if (lastSidebarStartupRenderStateKeyRef.current === renderStateKey) {
      return;
    }

    lastSidebarStartupRenderStateKeyRef.current = renderStateKey;
    postSidebarStartupReproLog("renderState", renderState);
    postSidebarRefreshDebugLog(sidebarRefreshDiagnosticLoggingEnabled, vscode, "renderStateChanged", {
      ...renderState,
      instanceId: refreshDebugInstanceIdRef.current,
    });
    if (hasAppliedHydrateRef.current && renderState.sessionCount === 0) {
      postSidebarStartupReproLog("emptyStateAfterHydrate", renderState);
      postSidebarRefreshDebugLog(sidebarRefreshDiagnosticLoggingEnabled, vscode, "emptyStateAfterHydrate", {
        ...renderState,
        instanceId: refreshDebugInstanceIdRef.current,
      });
    }
  }, [
    groupOrder,
    isStartupInteractionBlocked,
    postSidebarStartupReproLog,
    revision,
    sidebarRefreshDiagnosticLoggingEnabled,
    sessionsById,
    vscode,
    workspaceGroupIds,
  ]);

  useEffect(() => {
    const handleMessage = (event: Event) => {
      if (event instanceof MessageEvent) {
        handleWindowMessage(event);
      }
    };

    messageSource.addEventListener("message", handleMessage);

    return () => {
      messageSource.removeEventListener("message", handleMessage);
    };
  }, [ handleWindowMessage, messageSource ]);

  useEffect(() => {
    if (!nativeHostEventSource) {
      return;
    }

    const handleNativeHostEvent = (event: Event) => {
      if (!(event instanceof CustomEvent)) {
        return;
      }

      handleWindowMessage(
        new MessageEvent<ExtensionToSidebarMessage>("message", {
          data: event.detail,
        }),
      );
    };

    /**
     * CDXC:Hotkeys 2026-06-05-21:17:
     * Native macOS shortcuts arrive through the Ghostex host custom event, while extension-style traffic arrives through postMessage. Route both into the same sidebar action handler so Cmd+number uses the visible-row slot resolver consistently.
     *
     * CDXC:Hotkeys 2026-06-12-12:33:
     * The native sidebar wrapper owns typed nativeHotkey host events. Allow that wrapper to disable this shared listener so Cmd+T creates one terminal tab instead of running both the wrapper action and the shared SidebarApp createSession bridge.
     */
    nativeHostEventSource.addEventListener("ghostex-native-host-event", handleNativeHostEvent);

    return () => {
      nativeHostEventSource.removeEventListener("ghostex-native-host-event", handleNativeHostEvent);
    };
  }, [ handleWindowMessage, nativeHostEventSource ]);

  useEffect(() => {
    return () => {
      for (const timeout of completionFlashTimeoutBySessionIdRef.current.values()) {
        window.clearTimeout(timeout);
      }
      completionFlashTimeoutBySessionIdRef.current.clear();

      for (const timeoutId of Object.values(referenceSectionAnimationTimeoutsRef.current)) {
        if (timeoutId !== undefined) {
          window.clearTimeout(timeoutId);
        }
      }
      referenceSectionAnimationTimeoutsRef.current = {};
    };
  }, []);

  useEffect(() => {
    const timeout = window.setTimeout(() => {
      postSidebarStartupReproLog("interactionBlockReleased", {
        elapsedMs: getSidebarStartupElapsedMs(sidebarStartupStartedAtRef.current),
        revision: useSidebarStore.getState().revision,
      });
      setIsStartupInteractionBlocked(false);
    }, SIDEBAR_STARTUP_INTERACTION_BLOCK_MS);

    return () => {
      window.clearTimeout(timeout);
    };
  }, []);

  useEffect(() => {
    document.body.dataset.sidebarTheme = theme;
    const normalizedThemeColor = normalizeWorkspaceThemeColor(customThemeColor);
    const customSidebarTitlebarColorsEnabled =
      effectiveSettings.customSidebarTitlebarColorsEnabled === true;
    const customSidebarTitlebarForegroundColor = getSidebarTitlebarForegroundForBackground(
      effectiveSettings.customSidebarTitlebarBackgroundColor,
    );
    const customSidebarTitlebarGradientColors = getSidebarTitlebarGradientColors(
      effectiveSettings.customSidebarTitlebarBackgroundColor,
    );
    if (normalizedThemeColor) {
      /**
       * CDXC:WorkspaceTheme 2026-05-05-02:58
       * Custom workspace colors are active-project sidebar theme overrides:
       * keep the preset data-sidebar-theme as fallback, but publish validated
       * CSS variables so the app-level theme surfaces derive from the color.
       */
      document.body.dataset.sidebarCustomTheme = "true";
      document.body.style.setProperty("--workspace-sidebar-theme-color", normalizedThemeColor);
      document.body.style.setProperty(
        "--workspace-sidebar-theme-foreground",
        getWorkspaceThemeForeground(normalizedThemeColor),
      );
    } else {
      delete document.body.dataset.sidebarCustomTheme;
      document.body.style.removeProperty("--workspace-sidebar-theme-color");
      document.body.style.removeProperty("--workspace-sidebar-theme-foreground");
    }

    if (customSidebarTitlebarColorsEnabled) {
      /**
       * CDXC:SidebarTitlebarColors 2026-06-15-11:24:
       * Custom sidebar/titlebar colors are an experimental chrome override.
       * Publish dedicated CSS variables instead of mutating app theme tokens so
       * Settings modals, sidebar dropdowns, and other overlay surfaces continue
       * to resolve their normal Dark Gray/Dark 2 colors.
       *
       * CDXC:SidebarTitlebarColors 2026-06-15-13:22:
       * The foreground is derived from the selected background at apply time.
       * Do not preserve older stored foreground choices in the sidebar DOM.
       *
       * CDXC:SidebarTitlebarColors 2026-06-19-12:33:
       * The sidebar custom chrome background is a fixed-strength vertical
       * gradient derived from the selected tint-adjusted background. Publish
       * explicit gradient stop variables while keeping the solid background
       * token for row/card contrast calculations.
       */
      document.body.dataset.customSidebarTitlebarColors = "true";
      document.body.style.setProperty(
        "--custom-sidebar-titlebar-foreground-color",
        customSidebarTitlebarForegroundColor,
      );
      document.body.style.setProperty(
        "--custom-sidebar-titlebar-background-color",
        effectiveSettings.customSidebarTitlebarBackgroundColor,
      );
      document.body.style.setProperty(
        "--custom-sidebar-titlebar-gradient-top-color",
        customSidebarTitlebarGradientColors.sidebarTop,
      );
      document.body.style.setProperty(
        "--custom-sidebar-titlebar-gradient-bottom-color",
        customSidebarTitlebarGradientColors.sidebarBottom,
      );
    } else {
      delete document.body.dataset.customSidebarTitlebarColors;
      document.body.style.removeProperty("--custom-sidebar-titlebar-foreground-color");
      document.body.style.removeProperty("--custom-sidebar-titlebar-background-color");
      document.body.style.removeProperty("--custom-sidebar-titlebar-gradient-top-color");
      document.body.style.removeProperty("--custom-sidebar-titlebar-gradient-bottom-color");
    }

    return () => {
      delete document.body.dataset.sidebarTheme;
      delete document.body.dataset.sidebarCustomTheme;
      delete document.body.dataset.customSidebarTitlebarColors;
      document.body.style.removeProperty("--workspace-sidebar-theme-color");
      document.body.style.removeProperty("--workspace-sidebar-theme-foreground");
      document.body.style.removeProperty("--custom-sidebar-titlebar-foreground-color");
      document.body.style.removeProperty("--custom-sidebar-titlebar-background-color");
      document.body.style.removeProperty("--custom-sidebar-titlebar-gradient-top-color");
      document.body.style.removeProperty("--custom-sidebar-titlebar-gradient-bottom-color");
    };
  }, [
    customThemeColor,
    effectiveSettings.customSidebarTitlebarBackgroundColor,
    effectiveSettings.customSidebarTitlebarColorsEnabled,
    theme,
  ]);

  useEffect(() => {
    document.body.style.setProperty("--ghostex-agent-manager-zoom", `${agentManagerZoomPercent}%`);

    return () => {
      document.body.style.removeProperty("--ghostex-agent-manager-zoom");
    };
  }, [ agentManagerZoomPercent ]);

  const closeGitCommitModal = useEffectEvent((requestId: string) => {
    setGitCommitDraft(undefined);
    setGitFileDiffDraft(undefined);
    vscode.postMessage({
      requestId,
      type: "cancelSidebarGitCommit",
    });
  });

  useEffect(() => {
    if (!sessionGroupsPanelRef.current) {
      return;
    }

    sessionGroupsPanelRef.current.inert = isSidebarInteractionBlocked;
  }, [ isSidebarInteractionBlocked ]);

  const triggerReferenceSectionChildAnimation = (section: ReferenceSidebarSectionId) => {
    /**
     * CDXC:SidebarSessions 2026-05-17-00:11:
     * Reference-sidebar child entrance motion is only for explicit section
     * expansion. Session open/close hydration must not leave a durable CSS
     * state that replays the project/session "loading in" animation.
     */
    setReferenceSectionChildAnimations((previous) =>
      previous[ section ] ? previous : { ...previous, [ section ]: true },
    );

    const existingTimeoutId = referenceSectionAnimationTimeoutsRef.current[ section ];
    if (existingTimeoutId !== undefined) {
      window.clearTimeout(existingTimeoutId);
    }

    referenceSectionAnimationTimeoutsRef.current[ section ] = window.setTimeout(() => {
      setReferenceSectionChildAnimations((previous) =>
        previous[ section ] ? { ...previous, [ section ]: false } : previous,
      );
      delete referenceSectionAnimationTimeoutsRef.current[ section ];
    }, REFERENCE_SECTION_CHILD_ANIMATION_RESET_MS);
  };

  const isManualActiveSessionsSort = activeSessionsSortMode === "manual";
  /**
   * CDXC:SidebarLayout 2026-05-13-08:11
   * The reference sidebar replaces the old visible Actions/Agents grids with
   * app-modal entries, titlebar modes, and project header controls. Do not
   * mount the obsolete hidden panels in the sidebar tree.
   */
  const { groupIds: effectiveGroupIds, sessionIdsByGroup: effectiveSessionIdsByGroup } = useMemo(
    () =>
      createDisplaySessionLayout({
        sessionIdsByGroup: createWorkspaceSessionIdsByGroup(
          workspaceGroupIds,
          authoritativeSessionIdsByGroup,
        ),
        sessionsById,
        sortMode: activeSessionsSortMode,
        workspaceGroupIds,
      }),
    [ activeSessionsSortMode, authoritativeSessionIdsByGroup, sessionsById, workspaceGroupIds ],
  );
  const normalizedSessionSearchQuery = sessionSearchQuery.trim();
  const isSessionSearchFiltering =
    isSessionSearchOpen && normalizedSessionSearchQuery.length >= MIN_SESSION_SEARCH_QUERY_LENGTH;
  const isReferenceProjectsRenderedCollapsed =
    isReferenceProjectsCollapsed && !isSessionSearchFiltering;
  const projectsSectionPresence = useSidebarCollapsiblePresence(
    isReferenceProjectsRenderedCollapsed,
    effectiveSettings.sidebarCollapseAnimationDurationMs,
  );
  const isSidebarSearchProjectGroupRenderedCollapsed = (groupId: string) =>
    !isSessionSearchFiltering && collapsedGroupsById[ groupId ] === true;
  useEffect(() => {
    if (!isSessionSearchFiltering) {
      latestSessionSearchPreviousRequestIdRef.current = undefined;
      setRemoteSessionSearchPreviousSessions(undefined);
      return;
    }
    const timeoutId = window.setTimeout(() => {
      const requestId = `sidebar-search-previous-${Date.now()}-${Math.random().toString(36).slice(2)}`;
      latestSessionSearchPreviousRequestIdRef.current = requestId;
      /*
      CDXC:GxserverPresentationSearch 2026-06-01-15:08:
      Main sidebar search must show active-session matches immediately from the hydrated presentation snapshot, then query gxserver for previous/history metadata with a 200ms debounce. Do not depend on startup-hydrated previousSessions after the hard cutover.
      */
      vscode.postMessage({
        limit: 20,
        query: normalizedSessionSearchQuery,
        requestId,
        type: "requestPreviousSessions",
      });
    }, 200);
    return () => {
      window.clearTimeout(timeoutId);
    };
  }, [ isSessionSearchFiltering, normalizedSessionSearchQuery, vscode ]);
  /**
   * CDXC:ProjectBrowserTabs 2026-05-16-12:59:
   * Do not render a standalone Browsers group in the sidebar. Browser pane
   * sessions belong in their project group, and the shared workspace display
   * layout orders those project browser sessions before terminals/agents.
   */
  const displayedWorkspaceSessionIdsByGroup = useMemo(
    () =>
      createDisplayedSessionIdsByGroup({
        groupIds: effectiveGroupIds,
        query: normalizedSessionSearchQuery,
        selectedSessionTags: activeSelectedSessionTagFilters,
        sessionIdsByGroup: effectiveSessionIdsByGroup,
        sessionsById,
        shouldFilter: isSessionSearchFiltering,
      }),
    [
      effectiveGroupIds,
      effectiveSessionIdsByGroup,
      activeSelectedSessionTagFilters,
      isSessionSearchFiltering,
      normalizedSessionSearchQuery,
      sessionsById,
    ],
  );
  const hiddenCollectionMemberGroupIds = useMemo(() => {
    const hiddenKeys = new Set(hiddenCollectionKeys);
    const hiddenGroupIds = new Set<string>();
    for (const groupId of effectiveGroupIds) {
      const group = groupsById[groupId];
      const remoteMachineId = group?.remoteMachineContext?.machineId;
      const projectId = remoteMachineId
        ? group?.remoteMachineContext?.projectId
        : group?.projectContext?.editor.projectId;
      if (!projectId) {
        continue;
      }
      const collectionState = remoteMachineId
        ? remoteProjectCollectionsByMachineId[remoteMachineId]
        : projectCollections;
      const collection = collectionState?.collections.find((candidate) =>
        candidate.projectIds.includes(projectId),
      );
      if (!collection) {
        continue;
      }
      const collectionKey = remoteMachineId
        ? `remote:${remoteMachineId}:${collection.collectionId}`
        : `local:${collection.collectionId}`;
      if (hiddenKeys.has(collectionKey)) {
        hiddenGroupIds.add(groupId);
      }
    }
    return hiddenGroupIds;
  }, [
    effectiveGroupIds,
    groupsById,
    hiddenCollectionKeys,
    projectCollections,
    remoteProjectCollectionsByMachineId,
  ]);
  const displayedWorkspaceGroupIds = useMemo(
    () =>
      createDisplayedGroupIds(
        effectiveGroupIds,
        displayedWorkspaceSessionIdsByGroup,
        isSessionSearchFiltering || activeSelectedSessionTagFilters.length > 0,
      ).filter(
        (groupId) =>
          showHiddenSidebarItems ||
          (!hiddenGroupIds.includes(groupId) && !hiddenCollectionMemberGroupIds.has(groupId)),
      ),
    [
      activeSelectedSessionTagFilters.length,
      displayedWorkspaceSessionIdsByGroup,
      effectiveGroupIds,
      isSessionSearchFiltering,
      hiddenGroupIds,
      hiddenCollectionMemberGroupIds,
      showHiddenSidebarItems,
    ],
  );
  const displayedReferenceChatGroupIds = useMemo(
    () =>
      displayedWorkspaceGroupIds.filter((groupId) => groupsById[ groupId ]?.isChatCollection),
    [ displayedWorkspaceGroupIds, groupsById ],
  );
  /*
   * CDXC:SidebarSearch 2026-06-28-06:29:
   * Search results must reveal matching live project sessions even when the
   * user's normal section or project collapse state would hide them. Treat
   * collapse as render-only while filtering so clearing search restores the
   * user's previous sidebar shape without persisting temporary expansion.
   *
   */
  const displayedReferenceProjectGroupIds = useMemo(
    () =>
      displayedWorkspaceGroupIds.filter(
        (groupId) =>
          groupId !== SIDEBAR_GXSERVER_UNAVAILABLE_GROUP_ID &&
          !groupsById[ groupId ]?.isChatCollection &&
          !groupsById[ groupId ]?.remoteMachineContext,
      ),
    [ displayedWorkspaceGroupIds, groupsById ],
  );
  const groupIdsContainingActiveSession = useMemo(
    () =>
      new Set(
        effectiveGroupIds.filter(
          (groupId) =>
            groupsById[groupId]?.isActive === true &&
            (effectiveSessionIdsByGroup[groupId] ?? []).some(
              (sessionId) => sessionsById[sessionId]?.isFocused === true,
            ),
        ),
      ),
    [effectiveGroupIds, effectiveSessionIdsByGroup, groupsById, sessionsById],
  );
  const referenceProjectsSectionSessionSummary = useMemo(
    () =>
      getSidebarSectionSessionSummary(
        displayedReferenceProjectGroupIds,
        displayedWorkspaceSessionIdsByGroup,
        sessionsById,
      ),
    [displayedReferenceProjectGroupIds, displayedWorkspaceSessionIdsByGroup, sessionsById],
  );
  /*
   * CDXC:SidebarV2 2026-07-29:
   * The gxserver-unavailable row is a synthetic placeholder, not a project, so
   * the Inbox sidebar must not render it as one — exactly as V1 excludes it
   * from `displayedReferenceProjectGroupIds` above. The recovery message and
   * its Load Sessions action take its place (see `sidebarV2HostEmptyState`).
   */
  const sidebarV2DisplayedGroupIds = useMemo(
    () =>
      displayedWorkspaceGroupIds.filter(
        (groupId) =>
          groupId !== SIDEBAR_GXSERVER_UNAVAILABLE_GROUP_ID &&
          groupsById[groupId]?.isChatCollection !== true,
      ),
    [ displayedWorkspaceGroupIds, groupsById ],
  );
  const sidebarV2SectionSessionSummary = useMemo(
    () =>
      getSidebarSectionSessionSummary(
        sidebarV2DisplayedGroupIds,
        displayedWorkspaceSessionIdsByGroup,
        sessionsById,
      ),
    [sidebarV2DisplayedGroupIds, displayedWorkspaceSessionIdsByGroup, sessionsById],
  );
  const projectCollectionIdByProjectId = useMemo(() => {
    const next = new Map<string, string>();
    for (const collection of projectCollections.collections) {
      for (const projectId of collection.projectIds) {
        next.set(projectId, collection.collectionId);
      }
    }
    /*
     * CDXC:RemoteProjectCollections 2026-07-21:
     * Worktree children inherit their parent's collection for remote machine
     * projects too, so iterate every displayed workspace group instead of only
     * the local Projects section.
     */
    for (const groupId of displayedWorkspaceGroupIds) {
      const projectContext = groupsById[groupId]?.projectContext;
      const projectId = projectContext?.editor.projectId;
      const parentProjectId = projectContext?.worktree?.parentProjectId;
      const parentCollectionId = parentProjectId
        ? next.get(parentProjectId)
        : undefined;
      if (projectId && parentCollectionId) {
        next.set(projectId, parentCollectionId);
      }
    }
    return next;
  }, [ displayedWorkspaceGroupIds, groupsById, projectCollections ]);
  /*
   * CDXC:RemoteProjectCollections 2026-07-21:
   * The collection/project interleaving is shared between the local Projects
   * section and each remote machine section, so the builder takes the section's
   * group ids instead of closing over the local list. Remote machines only
   * render collections that have displayed member projects on that machine.
   */
  const buildProjectCollectionRenderItems = (
    sectionGroupIds: readonly string[],
    collectionState: SidebarProjectCollectionsState = projectCollections,
    resolveProjectId: (groupId: string) => string | undefined = (groupId) =>
      groupsById[groupId]?.projectContext?.editor.projectId,
  ): SidebarProjectCollectionRenderItem[] => {
    if (!enableProjectCollections) {
      return sectionGroupIds.map((groupId) => ({ groupId, kind: "project" }));
    }
    const groupIdByProjectId = new Map<string, string>();
    const projectIdByGroupId = new Map<string, string>();
    for (const groupId of sectionGroupIds) {
      const projectId = resolveProjectId(groupId);
      if (projectId) {
        groupIdByProjectId.set(projectId, groupId);
        projectIdByGroupId.set(groupId, projectId);
      }
    }
    const collectionIdByProjectId = new Map<string, string>();
    for (const collection of collectionState.collections) {
      for (const projectId of collection.projectIds) {
        collectionIdByProjectId.set(projectId, collection.collectionId);
      }
    }
    for (const groupId of sectionGroupIds) {
      const projectId = projectIdByGroupId.get(groupId);
      const parentProjectId = groupsById[groupId]?.projectContext?.worktree?.parentProjectId;
      const inheritedCollectionId = parentProjectId
        ? collectionIdByProjectId.get(parentProjectId)
        : undefined;
      if (projectId && inheritedCollectionId) {
        collectionIdByProjectId.set(projectId, inheritedCollectionId);
      }
    }
    /*
     * CDXC:GroupedProjectsFirst 2026-07-21:
     * Collections render first, in their definition order (which collection
     * drags reorder), and ungrouped projects always stack below the last
     * group while keeping their own drag order among themselves.
     */
    const emittedCollectionIds = new Set<string>();
    const items: SidebarProjectCollectionRenderItem[] = sectionGroupIds.flatMap((groupId) =>
      projectIdByGroupId.has(groupId) ? [] : [{ groupId, kind: "project" as const }],
    );
    for (const collection of collectionState.collections) {
      const visibleProjectIds = sectionGroupIds.flatMap((candidateGroupId) => {
        const candidateProjectId = projectIdByGroupId.get(candidateGroupId);
        return candidateProjectId &&
          collectionIdByProjectId.get(candidateProjectId) === collection.collectionId
          ? [candidateProjectId]
          : [];
      });
      if (visibleProjectIds.length === 0) {
        continue;
      }
      emittedCollectionIds.add(collection.collectionId);
      items.push({
        collection: { ...collection, projectIds: visibleProjectIds },
        groupIds: visibleProjectIds
          .map((candidate) => groupIdByProjectId.get(candidate))
          .filter((candidate): candidate is string => Boolean(candidate)),
        kind: "collection",
      });
    }
    for (const groupId of sectionGroupIds) {
      const projectId = projectIdByGroupId.get(groupId);
      if (!projectId) {
        continue;
      }
      const collectionId = projectId ? collectionIdByProjectId.get(projectId) : undefined;
      if (collectionId && emittedCollectionIds.has(collectionId)) {
        continue;
      }
      items.push({ groupId, kind: "project" });
    }
    return items;
  };
  const displayedProjectCollectionItems = useMemo<SidebarProjectCollectionRenderItem[]>(
    () => buildProjectCollectionRenderItems(displayedReferenceProjectGroupIds),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [
      displayedReferenceProjectGroupIds,
      enableProjectCollections,
      groupsById,
      projectCollectionIdByProjectId,
      projectCollections.collections,
    ],
  );
  const createProjectCollectionForProject = useEffectEvent((projectId: string) => {
    const created = createSidebarProjectCollection(projectCollections, projectId);
    setProjectCollections(
      moveProjectsToSidebarCollection(
        created.state,
        getProjectCollectionFamilyProjectIds(
          projectId,
          displayedReferenceProjectGroupIds,
          groupsById,
        ),
        created.collectionId,
      ),
    );
    setAutoEditingProjectCollectionId(created.collectionId);
  });
  const moveProjectToCollection = useEffectEvent(
    (projectId: string, collectionId: string | undefined) => {
      setProjectCollections((previous) =>
        moveProjectsToSidebarCollection(
          previous,
          getProjectCollectionFamilyProjectIds(
            projectId,
            displayedReferenceProjectGroupIds,
            groupsById,
          ),
          collectionId,
        ),
      );
    },
  );
  const updateRemoteProjectCollections = useEffectEvent(
    (
      machineId: string,
      update: (state: SidebarProjectCollectionsState) => SidebarProjectCollectionsState,
    ) => {
      const current = remoteProjectCollectionsByMachineId[machineId] ?? {
        collections: [],
        nextCollectionNumber: 1,
      };
      const updated = update(current);
      setRemoteProjectCollectionsByMachineId((previous) => ({
        ...previous,
        [machineId]: updated,
      }));
      vscode.postMessage({
        remoteMachineId: machineId,
        state: serializeSidebarProjectCollectionsForGxserver(updated),
        type: "updateSidebarProjectCollections",
      });
    },
  );
  const createRemoteProjectCollectionForProject = useEffectEvent(
    (machineId: string, projectId: string, machineGroupIds: readonly string[]) => {
      const rawProjectIds = getRemoteProjectCollectionFamilyProjectIds(
        projectId,
        machineGroupIds,
        groupsById,
      );
      const rawProjectId = rawProjectIds[0];
      if (!rawProjectId) {
        return;
      }
      let createdCollectionId: string | undefined;
      updateRemoteProjectCollections(machineId, (previous) => {
        const created = createSidebarProjectCollection(previous, rawProjectId);
        createdCollectionId = created.collectionId;
        return moveProjectsToSidebarCollection(
          created.state,
          rawProjectIds,
          created.collectionId,
        );
      });
      if (createdCollectionId) {
        setAutoEditingProjectCollectionId(`${machineId}:${createdCollectionId}`);
      }
    },
  );
  const moveRemoteProjectToCollection = useEffectEvent(
    (
      machineId: string,
      projectId: string,
      collectionId: string | undefined,
      machineGroupIds: readonly string[],
    ) => {
      const rawProjectIds = getRemoteProjectCollectionFamilyProjectIds(
        projectId,
        machineGroupIds,
        groupsById,
      );
      updateRemoteProjectCollections(machineId, (previous) =>
        moveProjectsToSidebarCollection(previous, rawProjectIds, collectionId),
      );
    },
  );
  const displayedProjectCollectionGroupIds = useMemo(
    () =>
      displayedProjectCollectionItems.flatMap((item) =>
        item.kind === "project" ? [item.groupId] : item.groupIds,
      ),
    [displayedProjectCollectionItems],
  );
  const remoteProjectGroupIdsByMachineId = useMemo(() => {
    const next: Record<string, string[]> = {};
    for (const groupId of displayedWorkspaceGroupIds) {
      const remoteMachineContext = groupsById[ groupId ]?.remoteMachineContext;
      if (!remoteMachineContext) {
        continue;
      }
      next[ remoteMachineContext.machineId ] ??= [];
      next[ remoteMachineContext.machineId ].push(groupId);
    }
    return next;
  }, [ displayedWorkspaceGroupIds, groupsById ]);
  const remoteSectionSessionSummariesByMachineId = useMemo(() => {
    const next: Record<string, SidebarSectionSessionSummary> = {};
    for (const [machineId, groupIds] of Object.entries(remoteProjectGroupIdsByMachineId)) {
      next[machineId] = getSidebarSectionSessionSummary(
        groupIds,
        displayedWorkspaceSessionIdsByGroup,
        sessionsById,
      );
    }
    return next;
  }, [displayedWorkspaceSessionIdsByGroup, remoteProjectGroupIdsByMachineId, sessionsById]);
  /*
   * CDXC:SidebarV2GroupedProjectUX 2026-07-30:
   * Every machine's OWN project order, keyed by machine, which is the unit
   * `syncGroupOrder` accepts: gxserver rejects a list that mixes local and remote
   * ids or spans two remote machines outright. Grouped V2 projects a logical-row
   * reorder onto these lists and posts one message per machine that changed.
   *
   * The local list is the raw store order (`displayedReferenceProjectGroupIds`),
   * not V1's collection-interleaved render order: V2 renders no collections, so
   * the collection order is not the order the user just rearranged.
   */
  const sidebarV2GroupIdsByMachineId = useMemo(
    () => ({
      [SIDEBAR_V2_LOCAL_GROUP_ORDER_KEY]: displayedReferenceProjectGroupIds,
      ...remoteProjectGroupIdsByMachineId,
    }),
    [ displayedReferenceProjectGroupIds, remoteProjectGroupIdsByMachineId ],
  );
  const remoteMachines = settings?.remoteMachines ?? [];
  /*
   * CDXC:RemoteMachines 2026-08-08:
   * Forgetting a collapsed machine id is only meaningful against the
   * authoritative saved-machine list, and host settings arrive asynchronously.
   * Before they do, `settings?.remoteMachines ?? []` means "not known yet", not
   * "no machines" — pruning against that empty list deleted every restored
   * collapsed machine within the first frames of launch, and the persistence
   * effect below then wrote the emptied map back, so remote sections always
   * reopened expanded and the saved state was destroyed for good. This is the
   * same hazard `preserveUnknownCollapsedGroups` documents for project groups
   * (see group-collapse.ts), so gate on the same hydrate signal. The dependency
   * is a primitive id key because `remoteMachines` is a fresh array identity on
   * every render and re-ran this pass constantly.
   */
  const savedRemoteMachineIdsKey = settings
    ? JSON.stringify(settings.remoteMachines.map((machine) => machine.id))
    : undefined;
  useEffect(() => {
    if (savedRemoteMachineIdsKey === undefined || !hasAppliedHydrateRef.current) {
      return;
    }
    const remoteMachineIds = new Set<string>(JSON.parse(savedRemoteMachineIdsKey) as string[]);
    setCollapsedRemoteMachineSectionsById((previous) => {
      let next: Record<string, true> | undefined;
      for (const machineId of Object.keys(previous)) {
        if (!remoteMachineIds.has(machineId)) {
          next ??= { ...previous };
          delete next[ machineId ];
        }
      }
      return next ?? previous;
    });
  }, [ savedRemoteMachineIdsKey ]);
  const moveRemoteMachineSection = useEffectEvent(
    (sourceRemoteMachineId: string, target: SidebarRemoteMachineDropTarget) => {
      if (!settings) {
        return;
      }
      const nextRemoteMachineIds = moveRemoteMachineIdToDropTarget(
        settings.remoteMachines.map((machine) => machine.id),
        sourceRemoteMachineId,
        target,
      );
      if (!nextRemoteMachineIds) {
        return;
      }
      const machineById = new Map(
        settings.remoteMachines.map((machine) => [machine.id, machine]),
      );
      const nextRemoteMachines = nextRemoteMachineIds.flatMap((machineId) => {
        const machine = machineById.get(machineId);
        return machine ? [machine] : [];
      });
      /*
       * CDXC:RemoteMachines 2026-06-03-00:18:
       * Remote machine sidebar sections are user-orderable peers of Projects.
       * Persist the order in Settings.remoteMachines so app restart and the
       * Remote settings tab show the same section order.
       */
      vscode.postMessage({
        baseRevision: revision,
        patch: {
          remoteMachines: nextRemoteMachines,
        },
        source: "sidebar:remoteMachineOrder",
        type: "updateSettingsPatch",
      });
    },
  );
  const filteredPreviousSessions = useMemo(
    () => {
      if (!isSessionSearchFiltering) {
        return [];
      }
      const searchResults =
        remoteSessionSearchPreviousSessions ??
        filterPreviousSessions(previousSessions, normalizedSessionSearchQuery);
      return filterDefaultNamedSessionSearchItems(searchResults);
    },
    [
      isSessionSearchFiltering,
      normalizedSessionSearchQuery,
      previousSessions,
      remoteSessionSearchPreviousSessions,
    ],
  );
  const hasExpandedReferenceProjects = useMemo(
    () =>
      displayedReferenceProjectGroupIds.some((groupId) => collapsedGroupsById[ groupId ] !== true),
    [ collapsedGroupsById, displayedReferenceProjectGroupIds ],
  );
  const handleSidebarProjectJump = useEffectEvent((detail: SidebarProjectJumpEventDetail) => {
    const shouldRevealFocusedSession = detail.revealFocusedSession === true;
    const requestFocusedSessionReveal = () => {
      if (!shouldRevealFocusedSession) {
        return;
      }
      setFocusedSessionRevealRequestId((requestId) => requestId + 1);
    };

    if (
      !detail.expandCollapsedProject ||
      !displayedReferenceProjectGroupIds.includes(detail.groupId)
    ) {
      requestFocusedSessionReveal();
      return;
    }

    const wasProjectCollapsed = collapsedGroupsById[ detail.groupId ] === true;
    const wasSectionCollapsed = isReferenceProjectsCollapsed;
    if (!wasProjectCollapsed && !wasSectionCollapsed) {
      requestFocusedSessionReveal();
      return;
    }

    /**
     * CDXC:ProjectHotkeys 2026-06-15-11:12:
     * Jump to Project shortcuts are navigation in the visible Projects sidebar area. When configured, a keyboard jump must reveal a collapsed target row immediately through React state, and the optional Show less write is only applied when that project row was actually expanded by the jump.
     *
     * CDXC:SidebarSessionReveal 2026-06-16-07:55:
     * Project/worktree creation can ask this same event to retry focused-row
     * scrolling after the target project has been expanded, because a new
     * gxserver row may arrive after the first focus hydrate.
     */
    postSidebarCollapseStateLog("projectJumpAutoExpand", {
      projectGroupCount: displayedReferenceProjectGroupIds.length,
      groupHash: hashSidebarCollapseDebugId(detail.groupId),
      revealFocusedSession: shouldRevealFocusedSession,
      showLessAfterExpand: detail.showLessAfterExpand,
      wasProjectCollapsed,
      wasSectionCollapsed,
    });
    if (wasSectionCollapsed) {
      triggerReferenceSectionChildAnimation("projects");
      setIsReferenceProjectsCollapsed(false);
    }
    if (wasProjectCollapsed) {
      setGroupCollapsed(detail.groupId, false);
      if (detail.showLessAfterExpand) {
        setProjectSessionListCollapsed(detail.projectId, true);
      }
    }
    requestFocusedSessionReveal();
  });
  const resolveGpuiProjectSlotHotkey = useEffectEvent((slotNumber: number) => {
    if (!Number.isInteger(slotNumber) || slotNumber < 1 || slotNumber > 9) {
      return;
    }

    const groupId = displayedReferenceProjectGroupIds[ slotNumber - 1 ];
    const projectId = groupId ? groupsById[ groupId ]?.projectContext?.editor.projectId : undefined;
    if (!groupId || !projectId) {
      return;
    }

    /*
     * CDXC:GPUIProjectHotkeys 2026-06-26-23:42:
     * GPUI project slot messages resolve locally in SidebarApp because SidebarApp owns rendered Projects row order. Use displayedReferenceProjectGroupIds so slots match visible Projects rows while excluding Quick chats and remote machine projects, then focus the group's currently focused or first displayed session through the existing WorkspaceTerminalFocus bridge; GPUI has no focusGroup host bridge to materialize command panes.
    */
    handleSidebarProjectJump({
      expandCollapsedProject: effectiveSettings.expandCollapsedProjectsOnJump,
      groupId,
      projectId,
      revealFocusedSession: true,
      showLessAfterExpand: effectiveSettings.showLessForExpandedProjectJumps,
    });
    const groupSessionIds = displayedWorkspaceSessionIdsByGroup[ groupId ] ?? [];
    const targetSessionId =
      groupSessionIds.find((sessionId) => sessionsById[ sessionId ]?.isFocused === true) ??
      groupSessionIds[ 0 ];
    if (!targetSessionId) {
      return;
    }
    focusSidebarSessionFromNavigation(groupId, targetSessionId);
    vscode.postMessage({
      sessionId: targetSessionId,
      type: "focusSession",
    });
  });
  useEffect(() => {
    const handleProjectJumpEvent = (event: Event) => {
      const detail = readSidebarProjectJumpEventDetail(event);
      if (detail) {
        handleSidebarProjectJump(detail);
      }
    };
    window.addEventListener(SIDEBAR_PROJECT_JUMP_EVENT, handleProjectJumpEvent);
    return () => {
      window.removeEventListener(SIDEBAR_PROJECT_JUMP_EVENT, handleProjectJumpEvent);
    };
  }, [ handleSidebarProjectJump ]);
  const focusedSessionId = useMemo(
    () => Object.values(sessionsById).find((session) => session.isFocused)?.sessionId,
    [ sessionsById ],
  );
  const postMultiSelectSelectionDebugLog = useEffectEvent(
    (event: string, details: Record<string, unknown>) => {
      /*
       * CDXC:SidebarMultiSelect 2026-07-02-07:32:
       * Selection-change repros need the resolver inputs and outputs even when
       * the sidebar Debugging Mode toggle is off, so post directly instead of
       * going through postSidebarDebugLog. Persistence is gated natively by the
       * native.sidebar.refresh diagnostic scenario (Settings > Diagnostic
       * logging > "Sidebar refresh and hydration"), and payloads stay limited
       * to ids, indexes, counts, and booleans.
       */
      vscode.postMessage({
        details,
        event: `repro.sidebarMultiSelect.${event}`,
        scenarioId: "native.sidebar.refresh",
        type: "sidebarDebugLog",
      });
    },
  );
  const handleSidebarSessionSelectionChange = useEffectEvent(
    (request: {
      groupId: string;
      mode: "additive" | "clear" | "range";
      reason?: string;
      sessionId: string;
    }) => {
      if (request.mode === "clear") {
        if (selectedSidebarSessionIds.length > 0) {
          postMultiSelectSelectionDebugLog("selectionCleared", {
            previousCount: selectedSidebarSessionIds.length,
            reason: request.reason ?? "unknown",
            sessionId: request.sessionId,
          });
        }
        setSelectedSidebarSessionIds([]);
        return;
      }

      /*
       * CDXC:SidebarMultiSelect 2026-07-02-08:12:
       * A user repro log showed every shift/cmd selection resolving against
       * visibleCount:2 with clickedIndex:-1 while the sidebar rendered a full
       * project list. data-visible tracks surfaced workspace panes, so the
       * default slot filter reduced the selectable rows to the current split.
       * Selection must consider every rendered row the user can see and click.
       */
      const visibleSessionIds = readRenderedSidebarSessionSlotIds(
        sessionGroupsContentRef.current ?? document,
        { skipPaneHiddenRows: false },
      );
      if (request.mode === "range") {
        const nextSelection = resolveRenderedSidebarSessionRangeSelection({
          activeSessionId: focusedSessionId,
          clickedSessionId: request.sessionId,
          visibleSessionIds,
        });
        postMultiSelectSelectionDebugLog("selectionResolved", {
          activeIndex: focusedSessionId ? visibleSessionIds.indexOf(focusedSessionId) : -1,
          activeSessionId: focusedSessionId ?? null,
          clickedIndex: visibleSessionIds.indexOf(request.sessionId),
          clickedSessionId: request.sessionId,
          mode: request.mode,
          resultCount: nextSelection.length,
          resultSessionIds: nextSelection.slice(0, 30),
          visibleCount: visibleSessionIds.length,
        });
        setSelectedSidebarSessionIds(nextSelection);
        return;
      }

      const nextSelection = resolveRenderedSidebarSessionAdditiveSelection({
        clickedSessionId: request.sessionId,
        currentSelection: selectedSidebarSessionIds,
        visibleSessionIds,
      });
      postMultiSelectSelectionDebugLog("selectionResolved", {
        activeIndex: focusedSessionId ? visibleSessionIds.indexOf(focusedSessionId) : -1,
        activeSessionId: focusedSessionId ?? null,
        clickedIndex: visibleSessionIds.indexOf(request.sessionId),
        clickedSessionId: request.sessionId,
        currentCount: selectedSidebarSessionIds.length,
        mode: request.mode,
        resultCount: nextSelection.length,
        resultSessionIds: nextSelection.slice(0, 30),
        visibleCount: visibleSessionIds.length,
      });
      setSelectedSidebarSessionIds(nextSelection);
    },
  );
  useEffect(() => {
    /*
     * CDXC:SidebarMultiSelect 2026-07-01-18:33:
     * Multi-selected session ids are transient UI state. Hydration, close, and
     * remote updates can remove rows, so prune stale ids instead of letting a
     * later selected-row context menu target invisible or missing sessions.
     *
     * CDXC:SidebarMultiSelect 2026-07-02-07:32:
     * Pruning can also silently shrink a selection the user just made when a
     * hydrate briefly drops session records, so log every actual prune.
     */
    const nextSelection = selectedSidebarSessionIds.filter(
      (sessionId) => sessionsById[ sessionId ] !== undefined,
    );
    if (nextSelection.length === selectedSidebarSessionIds.length) {
      return;
    }
    postMultiSelectSelectionDebugLog("selectionPruned", {
      previousCount: selectedSidebarSessionIds.length,
      prunedSessionIds: selectedSidebarSessionIds
        .filter((sessionId) => sessionsById[ sessionId ] === undefined)
        .slice(0, 30),
      remainingCount: nextSelection.length,
    });
    setSelectedSidebarSessionIds(nextSelection);
  }, [ selectedSidebarSessionIds, sessionsById ]);
  const postSidebarWakeScrollLog = useEffectEvent(
    (event: string, targetSessionId: string, details: Record<string, unknown>) => {
      postSidebarDebugLog("native.sidebar.refresh", `repro.sidebarWakeScroll.${event}`, {
        ...details,
        ...summarizeSidebarWakeScrollOrderState({
          activeSessionsSortMode,
          displayedWorkspaceGroupIds,
          displayedWorkspaceSessionIdsByGroup,
          focusedSessionId: targetSessionId,
          groupsById,
          revision,
          sessionsById,
        }),
        ...summarizeSidebarWakeScrollRenderedSlots(
          sessionGroupsContentRef.current ?? document,
          targetSessionId,
        ),
      });
    },
  );
  const focusSidebarSessionSlot = useEffectEvent((slotNumber: number) => {
    /*
     * CDXC:Hotkeys 2026-06-05-20:53:
     * Cmd+1..9 must target sessions by the order of rows currently shown in the sidebar. Flatten the rendered Quick, Projects, and Remote project rows after group collapse and project Show less state so collapsed-project sessions are ignored instead of being selected from hidden inventory order.
     *
     * CDXC:Hotkeys 2026-06-05-21:17:
     * A user repro showed the state-derived slot list could reserve a number for a hidden row, so Cmd+5 selected the sixth visible session and Cmd+6 jumped much lower. Resolve the slot list from the rendered session-card DOM rows at key time so numbering follows the sidebar exactly as shown.
     */
    const root = sessionGroupsContentRef.current ?? document;
    const sessionId =
      slotNumber === 0 || slotNumber === -1
        ? resolveAdjacentRenderedSidebarSessionSlotId({
          direction: slotNumber === 0 ? 1 : -1,
          focusedSessionId,
          slots: readRenderedSidebarSessionSlots(root),
        })
        : resolveVisibleSidebarSessionSlotId({
          focusedSessionId,
          slotNumber,
          /*
           * data-visible describes whether a session is already surfaced in a
           * workspace pane, not whether its sidebar row is visible. Numbered
           * shortcuts must include every rendered row so Cmd+N can surface the
           * Nth session the user can actually see in the sidebar.
           */
          visibleSessionIds: readRenderedSidebarSessionSlotIds(root, {
            skipPaneHiddenRows: false,
          }),
        });
    if (!sessionId) {
      return;
    }

    const groupId = findSessionGroupId(displayedWorkspaceSessionIdsByGroup, sessionId);
    if (groupId) {
      applyLocalFocus(groupId, sessionId);
    }
    vscode.postMessage({
      sessionId,
      type: "focusSession",
    });
  });
  const runGhostexHotkeyAction = useEffectEvent((actionId: string) => {
    const action = getghostexHotkeyActionById(actionId);
    if (!action) {
      return;
    }

    if (action.kind === "focusSessionSlot") {
      dismissAppModalForSidebarNavigation("SettingsDismissal:focusSessionHotkey");
      focusSidebarSessionSlot(action.slotNumber);
      return;
    }

    if (action.kind === "createSession") {
      requestNewSession();
      return;
    }

    if (action.kind === "openCommandPalette") {
      openCommandPalette();
      return;
    }

    if (action.kind === "openSessionSearchPalette") {
      openPreviousSessions();
      return;
    }

    if (action.kind === "openSettings") {
      openSidebarSettings();
      return;
    }

    if (action.kind === "openHotkeys") {
      openHotkeys();
      return;
    }

    if (action.kind === "moveSidebar") {
      moveSidebar();
      return;
    }

    if (action.kind === "toggleSidebarCollapsed") {
      toggleSidebarCollapsed();
      return;
    }

    /*
     * CDXC:HotkeyRouting 2026-06-26-23:04:
     * Rename Active Session, Open Commands Panel, Start Action slots, and
     * Focus Previous/Next Group, Directional Focus, and Split Sideways/Downwards
     * are native-owned hotkey actions when dispatched through the shared
     * SidebarApp bridge. Forward only the action id and runGhostexHotkeyAction
     * type so native runNativeHotkeyAction resolves authority state without
     * renderer-owned private data payloads such as session ids, titles, paths,
     * command text, or URLs.
     *
     * CDXC:HotkeyRouting 2026-06-26-23:58:
     * View Mode switching is native-owned; SidebarApp forwards setViewMode
     * through the same action-id-only bridge so renderer state stays private.
     */
    if (
      action.kind === "focusAdjacentGroup" ||
      action.kind === "focusDirection" ||
      action.kind === "focusedPaneAction" ||
      action.kind === "jumpToProject" ||
      /*
       * CDXC:NavigationHistory 2026-08-19:
       * Back/Forward is host-owned: gpui walks the trail through its native
       * titlebar route and the web shell through its sidebar runtime, so the
       * palette row forwards the action id and nothing else, exactly like the
       * other host-owned navigation rows here.
       */
      action.kind === "navigateHistory" ||
      action.kind === "openCommandsPanel" ||
      action.kind === "renameActiveSession" ||
      action.kind === "runActionSlot" ||
      action.kind === "setViewMode" ||
      action.kind === "splitFocusedPane" ||
      action.kind === "switchWorkareaView" ||
      action.kind === "terminalToolbarAction" ||
      action.kind === "toggleCompanionPane"
    ) {
      vscode.postMessage({ actionId: action.id, type: "runGhostexHotkeyAction" });
    }
  });
  useLayoutEffect(() => {
    if (
      didApplyStartupEmptyChatsCollapseRef.current ||
      !hasAppliedHydrateRef.current
    ) {
      return;
    }

    didApplyStartupEmptyChatsCollapseRef.current = true;
    const hasChatSessions = displayedReferenceChatGroupIds.some(
      (groupId) => (authoritativeSessionIdsByGroup[ groupId ] ?? []).length > 0,
    );
    if (!hasChatSessions) {
      postSidebarCollapseStateLog("sectionAutoCollapsed", {
        reason: "startup-empty-quick",
        section: "quick",
      });
      /**
       * CDXC:SidebarReference 2026-05-10-15:51
       * Startup restores the user's section/group collapse state, except an empty
       * Combined Chats section must always begin collapsed so a project-only
       * workspace does not waste vertical space on an empty chat container.
       */
      setIsReferenceChatsCollapsed(true);
    }
  }, [ authoritativeSessionIdsByGroup, displayedReferenceChatGroupIds ]);

  useEffect(() => {
    /**
     * CDXC:SidebarReference 2026-05-10-15:51
     * Combined section headers and per-group collapse state are
     * UI navigation state. Persist them in the sidebar webview so restarting
     * ghostex keeps collapsed items collapsed and expanded items expanded.
     * CDXC:SidebarReference 2026-05-20-12:00
     * The first post-hydrate group-collapse reconcile seeds session-count baseline
     * without expand-on-count-increase so restored projects do not reopen on launch.
     *
     * CDXC:RemoteMachines 2026-06-09-19:02:
     * Remote machine section collapse belongs to the same UI navigation state as
     * Quick and Projects. Persist each machine independently by saved machine id.
     */
    const nextCollapseState = {
      collapsedGroupsById,
      collapsedProjectCollectionsByKey,
      collapsedProjectSessionListsById,
      collapsedRemoteMachineSectionsById,
      isReferenceChatsCollapsed,
      isReferenceProjectsCollapsed,
    };
    const writeResult = writeSidebarUiCollapseState(windowScopeId, nextCollapseState);
    postSidebarCollapseStateLog("write", {
      ...summarizeSidebarUiCollapseState(nextCollapseState),
      groupCount: groupOrder.length,
      storedByteLength: writeResult.storedByteLength ?? 0,
      writeOk: writeResult.ok,
      writeReason: writeResult.reason ?? "stored",
    });
  }, [
    collapsedGroupsById,
    collapsedProjectCollectionsByKey,
    collapsedProjectSessionListsById,
    collapsedRemoteMachineSectionsById,
    isReferenceChatsCollapsed,
    isReferenceProjectsCollapsed,
    windowScopeId,
  ]);

  const shouldShowSessionSearchEmptyState =
    isSessionSearchFiltering &&
    displayedWorkspaceGroupIds.length === 0 &&
    filteredPreviousSessions.length === 0;
  /**
   * CDXC:SidebarSearch 2026-05-08-11:26
   * A no-match search is its own result state. Hide the normal Chats and
   * Projects sections while it is visible so the empty placeholder has the
   * same visual role as the existing "No Quick Sessions" group placeholder.
   */
  const shouldHideReferenceSectionsForSearchEmptyState = shouldShowSessionSearchEmptyState;
  /**
   * CDXC:SidebarProjectsEmptyState 2026-06-18-06:01:
   * A sidebar with zero rendered project groups should guide first-time setup from the same left-aligned Projects empty-state block as the previous "No projects" placeholder. Tie the copy to the visible Projects label and its hover plus action instead of adding a separate card or fallback surface.
   */
  const hasKnownProjectInventoryForEmptyState = hasKnownSidebarProjectInventory({
    groupsById,
    projectSettingsProjectCount: projectSettingsProjects?.length ?? 0,
    recentProjectCount: recentProjects.length,
    unavailableProjectGroupId: SIDEBAR_GXSERVER_UNAVAILABLE_GROUP_ID,
    workspaceGroupIds,
  });
  const shouldShowFirstProjectEmptyState =
    !isSessionSearchOpen && !hasKnownProjectInventoryForEmptyState;
  /*
   * CDXC:SidebarProjectsEmptyState 2026-06-30-03:25:
   * Sidebar search must not flash first-project onboarding after any project is
   * known. Search filtering and transient group display updates can temporarily
   * remove all visible Projects rows, so decide the first-run copy from
   * authoritative project inventory and parked Recent Projects instead of the
   * current displayed group arrays.
   */
  const referenceProjectsEmptyState = showGxserverUnavailableEmptyState ? (
    <div className="reference-sidebar-empty-state">
      Unable to load sessions.
      <br />
      {onStartGxserver ? (
        <button
          className="reference-sidebar-empty-state-action"
          onClick={onStartGxserver}
          type="button"
        >
          Load Sessions
        </button>
      ) : (
        "Restart Ghostex to try again."
      )}
    </div>
  ) : hasGxserverUnavailablePlaceholder ? null : (
    <div className="reference-sidebar-empty-state">
      {shouldShowFirstProjectEmptyState ? (
        <>
          No Projects Added.
          <br />
          <br />
          {"Hover over the Projects label and click on the plus button to add your first project and get started!"}
        </>
      ) : (
        "No projects"
      )}
    </div>
  );
  /*
   * CDXC:SidebarV2 2026-07-29:
   * Two of the project-level empty states are host-owned recovery/onboarding
   * surfaces rather than list chrome — gxserver being unavailable (with its
   * Load Sessions action) and having no project at all — so BOTH sidebars have
   * to render them. Everything else an empty list can mean (search, tag
   * filters, project scope) stays each sidebar's own copy.
   */
  const shouldRenderHostProjectsEmptyState =
    hasGxserverUnavailablePlaceholder ||
    (shouldShowFirstProjectEmptyState &&
      !sidebarV2DisplayedGroupIds.some(
        (groupId) => (displayedWorkspaceSessionIdsByGroup[ groupId ] ?? []).length > 0,
      ));
  const sidebarV2HostEmptyState = !shouldRenderHostProjectsEmptyState
    ? undefined
    : /*
       * `null` here is V1's deliberate blank Projects body during the gxserver
       * startup grace period. Hand V2 an empty node instead of `undefined` so it
       * stays blank too rather than falling through to "No sessions yet".
       */
      (referenceProjectsEmptyState ?? <></>);
  const {
    hasOverflow: sessionGroupsHaveScrollableOverflow,
  } = useScrollGlowState(sessionGroupsContentRef);
  const sidebarSessionSearchResults = useMemo(
    () =>
      createSidebarSessionSearchResults({
        displayedWorkspaceGroupIds,
        displayedWorkspaceSessionIdsByGroup,
        filteredPreviousSessions,
      }),
    [
      displayedWorkspaceGroupIds,
      displayedWorkspaceSessionIdsByGroup,
      filteredPreviousSessions,
    ],
  );
  useEffect(() => {
    groupIdsRef.current = displayedProjectCollectionGroupIds;
  }, [ displayedProjectCollectionGroupIds ]);

  useEffect(() => {
    sessionIdsByGroupRef.current = displayedWorkspaceSessionIdsByGroup;
  }, [ displayedWorkspaceSessionIdsByGroup ]);

  useEffect(() => {
    const queryChanged =
      previousNormalizedSessionSearchQueryRef.current !== normalizedSessionSearchQuery;
    previousNormalizedSessionSearchQueryRef.current = normalizedSessionSearchQuery;

    if (
      !isSessionSearchOpen ||
      normalizedSessionSearchQuery.length === 0 ||
      sidebarSessionSearchResults.length === 0 ||
      queryChanged
    ) {
      setIsSessionSearchSelectionVisible(false);
    }

    setSelectedSessionSearchResult((previous) => {
      if (!isSessionSearchOpen || normalizedSessionSearchQuery.length === 0) {
        return previous;
      }

      if (sidebarSessionSearchResults.length === 0) {
        return undefined;
      }

      if (queryChanged) {
        return createSidebarSessionSearchSelection(sidebarSessionSearchResults[ 0 ]);
      }

      if (!previous) {
        return undefined;
      }

      return sidebarSessionSearchResults.some((result) =>
        isSidebarSessionSearchSelectionMatch(result, previous),
      )
        ? previous
        : createSidebarSessionSearchSelection(sidebarSessionSearchResults[ 0 ]);
    });
  }, [ isSessionSearchOpen, normalizedSessionSearchQuery, sidebarSessionSearchResults ]);

  useEffect(() => {
    if (!isSessionSearchSelectionVisible || !selectedSessionSearchResult) {
      return;
    }

    const selectedElement =
      selectedSessionSearchResult.kind === "session"
        ? document.querySelector<HTMLElement>(
          `[data-sidebar-session-id="${selectedSessionSearchResult.sessionId}"]`,
        )
        : document.querySelector<HTMLElement>(
          `[data-sidebar-history-id="${selectedSessionSearchResult.historyId}"]`,
        );
    selectedElement?.scrollIntoView({
      block: "nearest",
    });
  }, [ isSessionSearchSelectionVisible, selectedSessionSearchResult ]);

  useEffect(() => {
    const isExplicitFocusedSessionRevealRequest =
      focusedSessionRevealRequestId !== previousFocusedSessionRevealRequestIdRef.current;
    previousFocusedSessionRevealRequestIdRef.current = focusedSessionRevealRequestId;
    if (isExplicitFocusedSessionRevealRequest) {
      useSidebarStore.getState().clearFocusedSessionScrollSuppression();
    }

    if (!focusedSessionId || !sessionGroupsContentRef.current) {
      return;
    }

    /*
     * CDXC:SidebarWakeScrollDiagnostics 2026-06-16-02:20:
     * Wake-scroll repros need to prove whether the sidebar jumped because focus-following issued scrollIntoView or because the focused row moved in the displayed order. Log only session IDs, row indexes, sort mode, and geometry metrics while the native.sidebar.refresh scenario is enabled.
     *
     * CDXC:SidebarSessionClose 2026-06-21-18:02:
     * Closing the focused terminal session should retarget native focus without reveal-scrolling the sidebar. Consume the one-shot close marker before scrollIntoViewIfNeeded so the user's list position stays stable after close.
     */
    let afterAnimationFrameId: number | undefined;
    let afterSettledTimeoutId: number | undefined;
    const sequence = ++focusedSessionScrollLogSequenceRef.current;
    const animationFrameId = window.requestAnimationFrame(() => {
      const scrollViewport = sessionGroupsContentRef.current;
      if (!scrollViewport) {
        postSidebarWakeScrollLog("focusedRowScrollSkipped", focusedSessionId, {
          reason: "missing-scroll-viewport",
          sequence,
        });
        return;
      }

      if (!isExplicitFocusedSessionRevealRequest) {
        const suppression = consumeFocusedSessionScrollSuppression();
        if (suppression) {
          postSidebarWakeScrollLog("focusedRowScrollSkipped", focusedSessionId, {
            reason: "close-driven-focus-scroll-suppressed",
            sequence,
            suppressionReason: suppression.reason,
          });
          return;
        }
      }

      const focusedSessionElement = document.querySelector<HTMLElement>(
        `[data-sidebar-session-id="${focusedSessionId}"]`,
      );
      if (!focusedSessionElement) {
        postSidebarWakeScrollLog("focusedRowScrollSkipped", focusedSessionId, {
          reason: "missing-focused-row",
          sequence,
        });
        return;
      }

      const beforeScrollTop = scrollViewport.scrollTop;
      const beforeGeometry = summarizeSidebarWakeScrollGeometry(
        focusedSessionElement,
        scrollViewport,
      );
      const scrollIssued = scrollElementIntoViewIfNeeded(focusedSessionElement, scrollViewport);
      postSidebarWakeScrollLog("focusedRowScrollDecision", focusedSessionId, {
        beforeGeometry,
        scrollIssued,
        sequence,
      });

      if (!scrollIssued) {
        return;
      }

      afterAnimationFrameId = window.requestAnimationFrame(() => {
        const nextScrollViewport = sessionGroupsContentRef.current;
        const nextFocusedSessionElement = document.querySelector<HTMLElement>(
          `[data-sidebar-session-id="${focusedSessionId}"]`,
        );
        postSidebarWakeScrollLog("focusedRowScrollAfterFrame", focusedSessionId, {
          afterGeometry: nextScrollViewport && nextFocusedSessionElement
            ? summarizeSidebarWakeScrollGeometry(nextFocusedSessionElement, nextScrollViewport)
            : undefined,
          scrollDeltaTop: nextScrollViewport ? nextScrollViewport.scrollTop - beforeScrollTop : undefined,
          sequence,
        });
      });
      afterSettledTimeoutId = window.setTimeout(() => {
        const settledScrollViewport = sessionGroupsContentRef.current;
        const settledFocusedSessionElement = document.querySelector<HTMLElement>(
          `[data-sidebar-session-id="${focusedSessionId}"]`,
        );
        postSidebarWakeScrollLog("focusedRowScrollAfterSettled", focusedSessionId, {
          afterGeometry: settledScrollViewport && settledFocusedSessionElement
            ? summarizeSidebarWakeScrollGeometry(settledFocusedSessionElement, settledScrollViewport)
            : undefined,
          scrollDeltaTop: settledScrollViewport
            ? settledScrollViewport.scrollTop - beforeScrollTop
            : undefined,
          sequence,
        });
      }, 350);
    });

    return () => {
      window.cancelAnimationFrame(animationFrameId);
      if (afterAnimationFrameId !== undefined) {
        window.cancelAnimationFrame(afterAnimationFrameId);
      }
      if (afterSettledTimeoutId !== undefined) {
        window.clearTimeout(afterSettledTimeoutId);
      }
    };
  }, [ consumeFocusedSessionScrollSuppression, focusedSessionId, focusedSessionRevealRequestId ]);

  /*
   * CDXC:SidebarBrowserTabReveal 2026-08-18:
   * Opening a Browser tab must leave the user able to SEE it in the sidebar.
   * Every collapsed container between the sidebar scroller and the row is
   * expanded for real (the same persisted collapse state the chevrons write, so
   * the expansion sticks), the group section is told to open the row's own
   * kind section, and the row is scrolled into view only if it is off screen.
   *
   * The scroll waits for the expand transitions the browser actually created
   * instead of a matching JS timer, exactly like `useSidebarCollapsiblePresence`
   * waits to unmount: measuring mid-animation reads a collapsed body as
   * zero-height and decides the row is already visible when it is not.
   */
  useEffect(() => {
    if (!sessionRevealRequest) {
      return;
    }
    const { requestId, sessionId } = sessionRevealRequest;
    if (handledSessionRevealRequestIdRef.current !== requestId) {
      const groupId = Object.keys(displayedWorkspaceSessionIdsByGroup).find((candidateGroupId) =>
        displayedWorkspaceSessionIdsByGroup[ candidateGroupId ]?.includes(sessionId),
      );
      if (!groupId) {
        // The row has not been published yet; this effect re-runs when it is.
        return;
      }
      handledSessionRevealRequestIdRef.current = requestId;
      pendingSessionRevealScrollRequestIdRef.current = requestId;

      if (isReferenceProjectsCollapsed) {
        triggerReferenceSectionChildAnimation("projects");
        setIsReferenceProjectsCollapsed(false);
      }
      const collectionItem = displayedProjectCollectionItems.find(
        (item) => item.kind === "collection" && item.groupIds.includes(groupId),
      );
      if (collectionItem?.kind === "collection") {
        setProjectCollectionCollapsed(
          createLocalProjectCollectionCollapseKey(collectionItem.collection.collectionId),
          false,
        );
      }
      setGroupCollapsed(groupId, false);
      const projectId = groupsById[ groupId ]?.projectContext?.editor.projectId;
      if (projectId) {
        setProjectSessionListCollapsed(projectId, false);
      }
    }

    /*
     * The expansions above happen once, but the scroll they enable must survive
     * this effect being torn down and re-run: a sidebar refresh landing in the
     * same frame changes the deps, which cancels the pending frame. Keeping the
     * scroll pending until it actually runs makes the re-run reschedule it
     * instead of dropping it.
     */
    if (pendingSessionRevealScrollRequestIdRef.current !== requestId) {
      return;
    }

    let cancelled = false;
    const scrollRevealedRowIntoView = () => {
      const scrollViewport = sessionGroupsContentRef.current;
      const revealedRow = document.querySelector<HTMLElement>(
        `[data-sidebar-session-id="${sessionId}"]`,
      );
      if (cancelled || !scrollViewport || !revealedRow) {
        return;
      }
      pendingSessionRevealScrollRequestIdRef.current = undefined;
      scrollElementIntoViewIfNeeded(revealedRow, scrollViewport);
    };
    const animationFrameId = window.requestAnimationFrame(() => {
      if (cancelled) {
        return;
      }
      const scrollViewport = sessionGroupsContentRef.current;
      const revealedRow = document.querySelector<HTMLElement>(
        `[data-sidebar-session-id="${sessionId}"]`,
      );
      if (!scrollViewport || !revealedRow) {
        return;
      }
      const expandAnimations: Animation[] = [];
      for (
        let ancestor = revealedRow.parentElement;
        ancestor && ancestor !== scrollViewport;
        ancestor = ancestor.parentElement
      ) {
        expandAnimations.push(...ancestor.getAnimations());
      }
      if (expandAnimations.length === 0) {
        scrollRevealedRowIntoView();
        return;
      }
      void Promise.allSettled(
        expandAnimations.map((expandAnimation) => expandAnimation.finished),
      ).then(scrollRevealedRowIntoView);
    });

    return () => {
      cancelled = true;
      window.cancelAnimationFrame(animationFrameId);
    };
  }, [
    /*
     * The collapse state is a real dependency, not incidental: expanding a
     * collapsed project mounts its body, so the row this effect scrolls to only
     * exists in the DOM on the run that follows its own expansion.
     */
    collapsedGroupsById,
    collapsedProjectCollectionsByKey,
    collapsedProjectSessionListsById,
    displayedProjectCollectionItems,
    displayedWorkspaceSessionIdsByGroup,
    groupsById,
    isReferenceProjectsCollapsed,
    sessionRevealRequest,
  ]);

  const unlockCompletionSoundPlayback = useEffectEvent(() => {
    void prepareCompletionSoundPlayback((soundEvent, details) => {
      postSidebarDebugLog("native.agent.detection", soundEvent, details);
    });
  });

  const recordPointerDownSessionTarget = useEffectEvent((event: PointerEvent) => {
    const target = event.target;
    if (!(target instanceof Element)) {
      pointerDownSessionTargetRef.current = undefined;
      return;
    }

    const sessionElement = target.closest<HTMLElement>("[data-sidebar-session-id]");
    const groupElement = target.closest<HTMLElement>(
      "[data-sidebar-session-group-id], [data-sidebar-group-id]",
    );
    const sessionId = sessionElement?.dataset.sidebarSessionId;
    const groupId =
      groupElement?.dataset.sidebarSessionGroupId ?? groupElement?.dataset.sidebarGroupId;
    if (!sessionId || !groupId) {
      pointerDownSessionTargetRef.current = undefined;
      return;
    }

    pointerDownSessionTargetRef.current = {
      groupId,
      point: {
        x: event.clientX,
        y: event.clientY,
      },
      sessionId,
    };

    if (sessionsById[ sessionId ]?.isPinned === true) {
      /*
       * CDXC:PinnedSessions 2026-06-02-19:53:
       * Pinned project-session reorder regressions can fail before dnd-kit
       * emits a session drag. Persist one pointer-down breadcrumb for pinned
       * rows so support can distinguish "drag never started" from "drop guard
       * skipped sync" without logging titles, paths, commands, or user text.
       */
      postPinnedSessionReorderLog("pointerDown", {
        groupCollapsed: collapsedGroupsById[ groupId ] === true,
        pointer: summarizePointerEventForPinnedReorder(event),
        state: createPinnedSessionReorderDebugState(
          { groupId, kind: "session", sessionId },
          sessionIdsByGroupRef.current,
          effectiveSessionIdsByGroup,
          authoritativeSessionIdsByGroup,
          sessionsById,
        ),
        targetDom: createPinnedSessionDomDebugState(groupId, sessionId),
      });
    }
  });

  useEffect(() => {
    const handlePointerDown = (event: PointerEvent) => {
      recordPointerDownSessionTarget(event);
      unlockCompletionSoundPlayback();
    };
    const handleKeyDown = () => {
      pointerDownSessionTargetRef.current = undefined;
      unlockCompletionSoundPlayback();
    };

    window.addEventListener("pointerdown", handlePointerDown, true);
    window.addEventListener("keydown", handleKeyDown, true);

    return () => {
      window.removeEventListener("pointerdown", handlePointerDown, true);
      window.removeEventListener("keydown", handleKeyDown, true);
    };
  }, [ recordPointerDownSessionTarget, unlockCompletionSoundPlayback ]);

  /*
   * CDXC:RemoteGroupReorder 2026-07-12:
   * Remote machine project groups reorder among their own machine's rows only.
   * Resolve drag candidates from the source group's scope so a remote drag
   * cannot target local Projects rows (and vice versa), while local project
   * drags keep using the collection-ordered id list.
   */
  const groupDragCandidateIdsForSource = (sourceGroupId: string): readonly string[] => {
    /*
     * CDXC:SidebarV2GroupedProjectUX 2026-07-30:
     * Grouped V2 does not have per-machine sections to scope a drag to. Its rows
     * ARE logical projects that may span machines, so every rendered row is a
     * candidate and the machine split moves to the other end of the operation:
     * the drop is projected back onto each machine's own list on release.
     */
    if (isSidebarV2GroupedActive) {
      return sidebarV2GroupOrderRowsRef.current.map((row) => row.groupId);
    }
    const machineId = groupsById[ sourceGroupId ]?.remoteMachineContext?.machineId;
    if (machineId) {
      return remoteProjectGroupIdsByMachineId[ machineId ] ?? [];
    }
    return groupIdsRef.current;
  };

  /*
   * CDXC:SidebarV2GroupedProjectUX 2026-07-30:
   * Written from V2's own render, read only inside a drag. It is a ref rather
   * than state on purpose: the rendered row list changes on every session
   * update, and mirroring it into state would re-render the whole sidebar for
   * information nothing paints. The identity is stable so V2's reporting effect
   * fires on row changes, not on every SidebarApp render.
   */
  const setSidebarV2GroupOrderRows = useCallback((rows: readonly SidebarV2GroupOrderRow[]) => {
    sidebarV2GroupOrderRowsRef.current = rows;
  }, []);

  /*
   * CDXC:SidebarV2GroupedProjectUX 2026-07-30:
   * Grouped V2's no-op answer, supplied to the shared pointer resolver so the
   * drop line appears exactly where a release would actually reorder something.
   */
  const sidebarV2GroupNoOpTargetForSource = (sourceGroupId: string) =>
    isSidebarV2GroupedActive
      ? (target: SidebarGroupDropTarget) =>
        moveSidebarV2GroupRows(sidebarV2GroupOrderRowsRef.current, sourceGroupId, target) ===
        undefined
      : undefined;

  const updateSessionDropIndicator = useEffectEvent(
    (event: Parameters<NonNullable<DragDropEventHandlers[ "onDragOver" ]>>[ 0 ]) => {
      const sourceData = getSidebarDropData(event.operation.source);
      if (sourceData?.kind === "remote-machine") {
        setGroupDropIndicator(undefined);
        setPinnedSessionDropIndicator(undefined);
        setProjectCollectionDropIndicator(undefined);
        setProjectUngroupDropIndicatorScopeId(undefined);
        setSessionDropIndicator(undefined);
        const resolvedRemoteMachineDropTarget = resolveRemoteMachineDropTargetFromPoint(
          getDragNativeEvent(event),
          remoteMachines.map((machine) => machine.id),
          sourceData.remoteMachineId,
          getSidebarDropData(event.operation.target),
        );
        setRemoteMachineDropIndicator((previous) =>
          areSameRemoteMachineDropTarget(previous, resolvedRemoteMachineDropTarget)
            ? previous
            : resolvedRemoteMachineDropTarget,
        );
        return;
      }

      setRemoteMachineDropIndicator(undefined);
      if (sourceData?.kind === "group") {
        setPinnedSessionDropIndicator(undefined);
        setSessionDropIndicator(undefined);
        const nativeEvent = getDragNativeEvent(event);
        const sourceProjectId =
          groupsById[sourceData.groupId]?.projectContext?.editor.projectId;
        const resolvedUngroupDropScopeId =
          sourceProjectId && projectCollectionIdByProjectId.has(sourceProjectId)
            ? resolveProjectUngroupDropScopeFromPoint(
              nativeEvent,
              sourceData.groupId,
              groupsById,
            )
            : undefined;
        const resolvedGroupDropTarget = resolvedUngroupDropScopeId
          ? undefined
          : resolveGroupDropTargetFromPoint(
            nativeEvent,
            groupDragCandidateIdsForSource(sourceData.groupId),
            groupsById,
            getSidebarDropData(event.operation.target),
            sourceData,
            sidebarV2GroupNoOpTargetForSource(sourceData.groupId),
          );
        setProjectUngroupDropIndicatorScopeId((previous) =>
          previous === resolvedUngroupDropScopeId ? previous : resolvedUngroupDropScopeId,
        );
        setGroupDropIndicator((previous) =>
          areSameGroupDropTarget(previous, resolvedGroupDropTarget)
            ? previous
            : resolvedGroupDropTarget,
        );
        return;
      }

      setGroupDropIndicator(undefined);
      setProjectUngroupDropIndicatorScopeId(undefined);
      if (sourceData?.kind === "project-collection") {
        setPinnedSessionDropIndicator(undefined);
        setSessionDropIndicator(undefined);
        const resolvedCollectionDropTarget = resolveProjectCollectionDropTargetFromPoint(
          getDragNativeEvent(event),
          displayedProjectCollectionItems.flatMap((item) =>
            item.kind === "collection" ? [item.collection.collectionId] : [],
          ),
          sourceData.collectionId,
          getSidebarDropData(event.operation.target),
        );
        setProjectCollectionDropIndicator((previous) =>
          previous?.collectionId === resolvedCollectionDropTarget?.collectionId &&
          previous?.position === resolvedCollectionDropTarget?.position
            ? previous
            : resolvedCollectionDropTarget,
        );
        return;
      }

      setProjectCollectionDropIndicator(undefined);
      if (sourceData?.kind !== "session") {
        setPinnedSessionDropIndicator(undefined);
        setSessionDropIndicator(undefined);
        return;
      }

      if (sessionsById[ sourceData.sessionId ]?.isPinned === true) {
        setSessionDropIndicator(undefined);
        const resolvedPinnedSessionDropTarget = resolvePinnedSessionDropTargetFromPoint(
          getDragNativeEvent(event),
          sourceData,
          sessionIdsByGroupRef.current,
          sessionsById,
        );
        const pinnedTargetLogKey = createPinnedSessionDropTargetLogKey(
          sourceData,
          resolvedPinnedSessionDropTarget,
        );
        if (pinnedSessionDropTargetLogKeyRef.current !== pinnedTargetLogKey) {
          pinnedSessionDropTargetLogKeyRef.current = pinnedTargetLogKey;
          postPinnedSessionReorderLog("targetChanged", {
            point: getClientPoint(getDragNativeEvent(event)),
            resolvedPinnedSessionDropTarget,
            sourceData,
            state: createPinnedSessionReorderDebugState(
              sourceData,
              sessionIdsByGroupRef.current,
              effectiveSessionIdsByGroup,
              authoritativeSessionIdsByGroup,
              sessionsById,
            ),
          });
        }
        setPinnedSessionDropIndicator((previous) =>
          areSameSessionDropTarget(previous, resolvedPinnedSessionDropTarget)
            ? previous
            : resolvedPinnedSessionDropTarget,
        );
        return;
      }

      setPinnedSessionDropIndicator(undefined);
      const resolvedSessionDropTarget = resolveSessionDropTargetFromPoint(
        getDragNativeEvent(event),
        sessionIdsByGroupRef.current,
        getSidebarDropData(event.operation.target),
        sourceData,
      );

      /*
       * CDXC:SidebarDragDrop 2026-06-19-11:12:
       * Manual session sorting should always show an insertion line while the
       * pointer is over another session row: above the row midpoint means
       * before, below the midpoint means after. Store the resolved drop target
       * directly instead of only highlighting a target project so the visual
       * indicator does not disappear when dnd-kit reports the broader group.
       */
      setSessionDropIndicator((previous) =>
        areSameSessionDropTarget(previous, resolvedSessionDropTarget ?? undefined)
          ? previous
          : resolvedSessionDropTarget ?? undefined,
      );
    },
  );

  const handleDragStart = ((event) => {
    setSidebarTooltipsSuppressedForDrag(true);
    const nativeEvent = getDragNativeEvent(event);
    const sourceData = getSidebarDropData(event.operation.source);
    const pointerDownSessionTarget = pointerDownSessionTargetRef.current;
    setIsProjectReorderDragActive(
      sourceData?.kind === "group" ||
        sourceData?.kind === "project-collection" ||
        sourceData?.kind === "remote-machine",
    );
    if (sourceData?.kind === "group") {
      const point = getClientPoint(nativeEvent);
      const group = groupsById[ sourceData.groupId ];
      const headerMetrics = point
        ? getProjectGroupDragHeaderMetrics(sourceData.groupId, point)
        : undefined;
      /**
       * CDXC:ProjectDragPreview 2026-05-21-11:45:
       * Project drag ghosts should be anchored to the live cursor and should
       * render only the project header, even when the source project is expanded.
       * Keep the source row in the list as the faint placeholder instead of
       * cloning the whole expanded project into the moving preview.
       *
       * CDXC:ProjectDragPreview 2026-05-28-12:35:
       * The project drag ghost should preserve the grabbed header button's
       * exact left edge and width, then move only on the vertical axis. Capture
       * the header row bounds at drag start and keep the pointer's initial
       * vertical offset so horizontal pointer drift never shifts the ghost.
       */
      setGroupDragPreview(
        point && headerMetrics && group?.projectContext
          ? {
            groupId: sourceData.groupId,
            isCollapsed: collapsedGroupsById[ sourceData.groupId ] === true,
            left: headerMetrics.left,
            pointerOffsetY: headerMetrics.pointerOffsetY,
            themeColor: group.projectContext.themeColor,
            title: group.title,
            top: headerMetrics.top,
            width: headerMetrics.width,
          }
          : undefined,
      );
    } else {
      setGroupDragPreview(undefined);
    }
    if (sourceData?.kind === "project-collection") {
      const point = getClientPoint(nativeEvent);
      const collection = projectCollections.collections.find(
        (candidate) => candidate.collectionId === sourceData.collectionId,
      );
      const metrics = point
        ? getProjectCollectionDragMetrics(event.operation.source, sourceData.collectionId)
        : undefined;
      setProjectCollectionDragPreview(
        point && metrics && collection
          ? {
            collectionId: sourceData.collectionId,
            color: collection.color,
            left: metrics.left,
            pointerOffsetY: point.y - metrics.top,
            title: collection.title,
            top: metrics.top,
            width: metrics.width,
          }
          : undefined,
      );
    } else {
      setProjectCollectionDragPreview(undefined);
    }
    if (sourceData?.kind === "remote-machine") {
      const point = getClientPoint(nativeEvent);
      const machine = remoteMachines.find(
        (candidate) => candidate.id === sourceData.remoteMachineId,
      );
      const metrics = point
        ? getRemoteMachineDragHeaderMetrics(sourceData.remoteMachineId, point)
        : undefined;
      setRemoteMachineDragPreview(
        point && metrics && machine
          ? {
            collapsed:
              collapsedRemoteMachineSectionsById[sourceData.remoteMachineId] === true,
            left: metrics.left,
            machineId: sourceData.remoteMachineId,
            pointerOffsetY: metrics.pointerOffsetY,
            title: machine.name,
            top: metrics.top,
            width: metrics.width,
          }
          : undefined,
      );
    } else {
      setRemoteMachineDragPreview(undefined);
    }
    sessionPointerDragStateRef.current =
      sourceData?.kind === "session"
        ? createSessionPointerDragState(sourceData, pointerDownSessionTarget, nativeEvent)
        : undefined;
    pinnedSessionDropTargetLogKeyRef.current = undefined;
    setGroupDropIndicator(undefined);
    setPinnedSessionDropIndicator(undefined);
    setProjectCollectionDropIndicator(undefined);
    setProjectUngroupDropIndicatorScopeId(undefined);
    setRemoteMachineDropIndicator(undefined);
    setSessionDropIndicator(undefined);
    if (
      pointerDownSessionTarget &&
      sessionsById[ pointerDownSessionTarget.sessionId ]?.isPinned === true &&
      !(
        sourceData?.kind === "session" &&
        sourceData.groupId === pointerDownSessionTarget.groupId &&
        sourceData.sessionId === pointerDownSessionTarget.sessionId
      )
    ) {
      postPinnedSessionReorderLog("dragStartSourceMismatch", {
        point: getClientPoint(nativeEvent),
        pointerDownSessionTarget,
        sourceData,
        sourceKind: sourceData?.kind,
        state: createPinnedSessionReorderDebugState(
          {
            groupId: pointerDownSessionTarget.groupId,
            kind: "session",
            sessionId: pointerDownSessionTarget.sessionId,
          },
          sessionIdsByGroupRef.current,
          effectiveSessionIdsByGroup,
          authoritativeSessionIdsByGroup,
          sessionsById,
        ),
        targetData: getSidebarDropData(event.operation.target),
      });
    }
    if (sourceData?.kind === "session" && sessionsById[ sourceData.sessionId ]?.isPinned === true) {
      postPinnedSessionReorderLog("dragStart", {
        point: getClientPoint(nativeEvent),
        pointerDownSessionTarget,
        sourceData,
        state: createPinnedSessionReorderDebugState(
          sourceData,
          sessionIdsByGroupRef.current,
          effectiveSessionIdsByGroup,
          authoritativeSessionIdsByGroup,
          sessionsById,
        ),
        targetData: getSidebarDropData(event.operation.target),
      });
    }
    postSidebarDebugLog("native.pane.reorder", "session.dragStart", {
      nativeEventType: nativeEvent?.type,
      pointerDragState: sessionPointerDragStateRef.current,
      point: getClientPoint(nativeEvent),
      sourceData,
      targetData: getSidebarDropData(event.operation.target),
    });
  }) satisfies DragDropEventHandlers[ "onDragStart" ];

  const handleDragMove = ((event) => {
    const nativeEvent = getDragNativeEvent(event);
    updateGroupDragPreviewFromEvent(setGroupDragPreview, nativeEvent);
    updateGroupDragPreviewFromEvent(setProjectCollectionDragPreview, nativeEvent);
    updateGroupDragPreviewFromEvent(setRemoteMachineDragPreview, nativeEvent);
    updateSessionPointerDragState(sessionPointerDragStateRef.current, nativeEvent);
    updateSessionDropIndicator(event);
  }) satisfies DragDropEventHandlers[ "onDragMove" ];

  const handleDragOver = ((event) => {
    const nativeEvent = getDragNativeEvent(event);
    updateGroupDragPreviewFromEvent(setGroupDragPreview, nativeEvent);
    updateGroupDragPreviewFromEvent(setProjectCollectionDragPreview, nativeEvent);
    updateGroupDragPreviewFromEvent(setRemoteMachineDragPreview, nativeEvent);
    updateSessionPointerDragState(sessionPointerDragStateRef.current, nativeEvent);
    updateSessionDropIndicator(event);
  }) satisfies DragDropEventHandlers[ "onDragOver" ];

  const handleDragEnd = ((event) => {
    setSidebarTooltipsSuppressedForDrag(false);
    setGroupDropIndicator(undefined);
    setGroupDragPreview(undefined);
    setProjectCollectionDragPreview(undefined);
    setRemoteMachineDragPreview(undefined);
    setIsProjectReorderDragActive(false);
    setPinnedSessionDropIndicator(undefined);
    setProjectCollectionDropIndicator(undefined);
    setProjectUngroupDropIndicatorScopeId(undefined);
    setRemoteMachineDropIndicator(undefined);
    setSessionDropIndicator(undefined);
    const currentGroupIds = groupIdsRef.current;
    const currentSessionIdsByGroup = sessionIdsByGroupRef.current;
    const previousSessionIdsByGroup = effectiveSessionIdsByGroup;

    const nativeEvent = getDragNativeEvent(event);
    const sourceData = getSidebarDropData(event.operation.source);
    const targetData = getSidebarDropData(event.operation.target);
    const sessionPointerDragState = sessionPointerDragStateRef.current;
    updateSessionPointerDragState(sessionPointerDragState, nativeEvent);
    sessionPointerDragStateRef.current = undefined;
    const resolvedSessionDropTarget =
      sourceData?.kind === "session"
        ? resolveSessionDropTargetFromPoint(
          nativeEvent,
          currentSessionIdsByGroup,
          targetData,
          sourceData,
        )
        : undefined;
    postSidebarDebugLog("native.pane.reorder", "session.dragEnd", {
      canceled: event.canceled,
      nativeEventType: nativeEvent?.type,
      pointerDragState: sessionPointerDragState,
      point: getClientPoint(nativeEvent),
      resolvedSessionDropTarget,
      sourceData,
      targetData,
    });
    if (!sourceData) {
      return;
    }

    if (sourceData.kind === "project-collection") {
      setProjectCollectionDropIndicator(undefined);
      if (event.canceled) {
        return;
      }

      /*
       * A collection drag moves its complete visible project block between the
       * existing collection slots. Ungrouped projects keep their slots, child
       * project order stays intact, and the resulting flat project order is
       * persisted through the same sync contract as ordinary project drags.
       *
       * CDXC:CollectionReorder 2026-07-21:
       * Collections drag with feedback "none" (like project cards), so dnd-kit
       * never reports a rect-overlap target for them: the source shape stays at
       * its resting position for the whole drag. Resolve the insertion boundary
       * from the pointer position against the visible collection panels — the
       * same pattern project rows use via resolveGroupDropTargetFromPoint.
       */
      const collectionItems = displayedProjectCollectionItems.filter(
        (item): item is Extract<SidebarProjectCollectionRenderItem, { kind: "collection" }> =>
          item.kind === "collection",
      );
      const collectionIds = collectionItems.map((item) => item.collection.collectionId);
      const resolvedCollectionDropTarget = resolveProjectCollectionDropTargetFromPoint(
        nativeEvent,
        collectionIds,
        sourceData.collectionId,
        targetData,
      );
      if (!resolvedCollectionDropTarget) {
        return;
      }
      const nextCollectionIds = moveCollectionIdToDropTarget(
        collectionIds,
        sourceData.collectionId,
        resolvedCollectionDropTarget,
      );
      if (!nextCollectionIds) {
        return;
      }

      const collectionItemById = new Map(
        collectionItems.map((item) => [item.collection.collectionId, item]),
      );
      let nextCollectionIndex = 0;
      const nextRenderItems = displayedProjectCollectionItems.map((item) => {
        if (item.kind !== "collection") {
          return item;
        }
        const collectionId = nextCollectionIds[nextCollectionIndex];
        nextCollectionIndex += 1;
        return collectionId ? collectionItemById.get(collectionId) ?? item : item;
      });
      const nextGroupIds = nextRenderItems.flatMap((item) =>
        item.kind === "collection" ? item.groupIds : [item.groupId],
      );
      if (haveSameSessionOrder(currentGroupIds, nextGroupIds)) {
        return;
      }

      const nextProjectIds = nextGroupIds.flatMap((groupId) => {
        const projectId = groupsById[groupId]?.projectContext?.editor.projectId;
        return projectId ? [projectId] : [];
      });
      setProjectCollections((previous) =>
        reorderSidebarProjectCollections(
          reorderSidebarProjectCollectionDefinitions(previous, nextCollectionIds),
          nextProjectIds,
        ),
      );
      vscode.postMessage({
        groupIds: nextGroupIds,
        type: "syncGroupOrder",
      });
      return;
    }

    if (sourceData.kind === "remote-machine") {
      if (event.canceled) {
        return;
      }
      const resolvedRemoteMachineDropTarget = resolveRemoteMachineDropTargetFromPoint(
        nativeEvent,
        remoteMachines.map((machine) => machine.id),
        sourceData.remoteMachineId,
        targetData,
      );
      if (!resolvedRemoteMachineDropTarget) {
        return;
      }
      moveRemoteMachineSection(sourceData.remoteMachineId, resolvedRemoteMachineDropTarget);
      return;
    }

    if (sourceData.kind === "group") {
      if (event.canceled) {
        return;
      }

      /*
       * CDXC:SidebarV2GroupedProjectUX 2026-07-30:
       * A grouped V2 row is a LOGICAL project: one header can stand for several
       * physical checkouts, on several machines. So the drop cannot be one
       * reordered id list — `syncGroupOrder` rejects a mixed local/remote or
       * cross-machine list, because each machine owns its own project order.
       *
       * Instead the row's new index among the LOGICAL rows is projected onto each
       * participating machine's own list, and one `syncGroupOrder` goes out per
       * machine that actually changed. The projection itself is pure and unit
       * tested (`shared/sidebar-v2-group-order.ts`); everything DOM-dependent —
       * which boundary the pointer is over — stays in the shared resolver above,
       * so the committed reorder is the same boundary the drop line drew.
       *
       * This branch also bypasses V1's collection/ungroup handling below on
       * purpose: grouped V2 renders neither collections nor per-machine sections,
       * so there is no collection to drop out of and no machine list to leave.
       */
      if (isSidebarV2GroupedActive) {
        const rows = sidebarV2GroupOrderRowsRef.current;
        const resolvedTarget = resolveGroupDropTargetFromPoint(
          nativeEvent,
          rows.map((row) => row.groupId),
          groupsById,
          targetData,
          sourceData,
          sidebarV2GroupNoOpTargetForSource(sourceData.groupId),
        );
        if (!resolvedTarget) {
          return;
        }
        const projectedOrders = projectSidebarV2GroupOrderByMachine({
          groupIdsByMachineId: sidebarV2GroupIdsByMachineId,
          rows,
          sourceGroupId: sourceData.groupId,
          target: resolvedTarget,
        });
        for (const machineGroupIds of Object.values(projectedOrders)) {
          vscode.postMessage({
            groupIds: machineGroupIds,
            type: "syncGroupOrder",
          });
        }
        return;
      }

      /*
       * CDXC:RemoteGroupReorder 2026-07-12:
       * Remote machine groups reorder within their machine section and post the
       * machine-scoped id order through the same syncGroupOrder contract; the
       * host persists the per-machine order. Collections apply to local
       * projects only.
       */
      const remoteMachineId = groupsById[ sourceData.groupId ]?.remoteMachineContext?.machineId;
      if (remoteMachineId) {
        const machineGroupIds = remoteProjectGroupIdsByMachineId[ remoteMachineId ] ?? [];
        const sourceProjectId =
          groupsById[sourceData.groupId]?.projectContext?.editor.projectId;
        const resolvedUngroupDropScopeId = resolveProjectUngroupDropScopeFromPoint(
          nativeEvent,
          sourceData.groupId,
          groupsById,
        );
        if (
          sourceProjectId &&
          projectCollectionIdByProjectId.has(sourceProjectId) &&
          resolvedUngroupDropScopeId === createRemoteProjectListScopeId(remoteMachineId)
        ) {
          const nextMachineGroupIds = moveProjectGroupFamilyToEnd(
            machineGroupIds,
            sourceData.groupId,
            groupsById,
          );
          setProjectCollections((previous) =>
            moveProjectsToSidebarCollection(
              previous,
              getProjectCollectionFamilyProjectIds(
                sourceProjectId,
                machineGroupIds,
                groupsById,
              ),
              undefined,
            ),
          );
          if (!haveSameSessionOrder(machineGroupIds, nextMachineGroupIds)) {
            vscode.postMessage({
              groupIds: nextMachineGroupIds,
              type: "syncGroupOrder",
            });
          }
          return;
        }
        const resolvedRemoteDropTarget = resolveGroupDropTargetFromPoint(
          nativeEvent,
          machineGroupIds,
          groupsById,
          targetData,
          sourceData,
        );
        if (!resolvedRemoteDropTarget) {
          return;
        }
        const nextMachineGroupIds = moveGroupIdsByProjectDropTarget(
          machineGroupIds,
          sourceData.groupId,
          resolvedRemoteDropTarget,
          groupsById,
        );
        if (haveSameSessionOrder(machineGroupIds, nextMachineGroupIds)) {
          return;
        }
        vscode.postMessage({
          groupIds: nextMachineGroupIds,
          type: "syncGroupOrder",
        });
        return;
      }

      const sourceProjectId =
        groupsById[sourceData.groupId]?.projectContext?.editor.projectId;
      const resolvedUngroupDropScopeId = resolveProjectUngroupDropScopeFromPoint(
        nativeEvent,
        sourceData.groupId,
        groupsById,
      );
      if (
        sourceProjectId &&
        projectCollectionIdByProjectId.has(sourceProjectId) &&
        resolvedUngroupDropScopeId === LOCAL_PROJECT_LIST_SCOPE_ID
      ) {
        const nextGroupIds = moveProjectGroupFamilyToEnd(
          currentGroupIds,
          sourceData.groupId,
          groupsById,
        );
        setProjectCollections((previous) =>
          moveProjectsToSidebarCollection(
            previous,
            getProjectCollectionFamilyProjectIds(
              sourceProjectId,
              currentGroupIds,
              groupsById,
            ),
            undefined,
          ),
        );
        if (!haveSameSessionOrder(currentGroupIds, nextGroupIds)) {
          vscode.postMessage({
            groupIds: nextGroupIds,
            type: "syncGroupOrder",
          });
        }
        return;
      }
      const resolvedGroupDropTarget = resolveGroupDropTargetFromPoint(
        nativeEvent,
        currentGroupIds,
        groupsById,
        targetData,
        sourceData,
      );
      const isProjectGroupOrder =
        createProjectGroupOrderItems(currentGroupIds, groupsById).length === currentGroupIds.length;
      const nextGroupIds = resolvedGroupDropTarget
        ? moveGroupIdsByProjectDropTarget(
          currentGroupIds,
          sourceData.groupId,
          resolvedGroupDropTarget,
          groupsById,
        )
        : targetData?.kind === "group" && !isProjectGroupOrder
          ? move(currentGroupIds, event)
          : currentGroupIds;
      if (haveSameSessionOrder(currentGroupIds, nextGroupIds)) {
        return;
      }

      if (enableProjectCollections && resolvedGroupDropTarget) {
        const sourceProjectId = groupsById[sourceData.groupId]?.projectContext?.editor.projectId;
        const targetProjectId =
          groupsById[resolvedGroupDropTarget.groupId]?.projectContext?.editor.projectId;
        if (sourceProjectId && targetProjectId) {
          const targetCollectionId = projectCollectionIdByProjectId.get(targetProjectId);
          const sourceFamilyProjectIds = getProjectCollectionFamilyProjectIds(
            sourceProjectId,
            currentGroupIds,
            groupsById,
          );
          const nextProjectIds = nextGroupIds.flatMap((groupId) => {
            const projectId = groupsById[groupId]?.projectContext?.editor.projectId;
            return projectId ? [projectId] : [];
          });
          setProjectCollections((previous) =>
            reorderSidebarProjectCollections(
              moveProjectsToSidebarCollection(
                previous,
                sourceFamilyProjectIds,
                targetCollectionId,
              ),
              nextProjectIds,
            ),
          );
        }
      }

      vscode.postMessage({
        groupIds: nextGroupIds,
        type: "syncGroupOrder",
      });
      return;
    }

    if (sourceData.kind !== "session") {
      return;
    }

    if (sessionPointerDragState?.startPoint && !sessionPointerDragState.didMove) {
      if (sessionsById[ sourceData.sessionId ]?.isPinned === true) {
        postPinnedSessionReorderLog("dragEndIgnoredWithoutPointerMovement", {
          point: getClientPoint(nativeEvent),
          pointerDragState: sessionPointerDragState,
          sourceData,
        });
      }
      postSidebarDebugLog("native.pane.reorder", "session.dragEndIgnoredWithoutPointerMovement", {
        point: getClientPoint(nativeEvent),
        sourceData,
      });
      return;
    }

    if (event.canceled) {
      if (sessionsById[ sourceData.sessionId ]?.isPinned === true) {
        postPinnedSessionReorderLog("dragEndCanceled", {
          point: getClientPoint(nativeEvent),
          sourceData,
          targetData,
        });
      }
      return;
    }

    if (sessionsById[ sourceData.sessionId ]?.isPinned === true) {
      const resolvedPinnedSessionDropTarget = resolvePinnedSessionDropTargetFromPoint(
        nativeEvent,
        sourceData,
        currentSessionIdsByGroup,
        sessionsById,
      );
      postPinnedSessionReorderLog("dragEndResolved", {
        point: getClientPoint(nativeEvent),
        resolution: createPinnedSessionDropResolutionDebugState(
          nativeEvent,
          sourceData,
          currentSessionIdsByGroup,
          sessionsById,
        ),
        resolvedPinnedSessionDropTarget,
        resolvedSessionDropTarget,
        sourceData,
        state: createPinnedSessionReorderDebugState(
          sourceData,
          currentSessionIdsByGroup,
          previousSessionIdsByGroup,
          authoritativeSessionIdsByGroup,
          sessionsById,
        ),
        targetData,
      });
      if (!resolvedPinnedSessionDropTarget) {
        postPinnedSessionReorderLog("dragEndSkipped", {
          reason: "noPinnedDropTarget",
          sourceData,
          targetData,
        });
        return;
      }

      const previousPinnedSessionIds = (previousSessionIdsByGroup[ sourceData.groupId ] ?? []).filter(
        (sessionId) => sessionsById[ sessionId ]?.isPinned === true,
      );
      const nextPinnedSessionIds = movePinnedSessionIdsByDropTarget(
        previousPinnedSessionIds,
        sourceData.sessionId,
        resolvedPinnedSessionDropTarget,
      );
      if (
        haveSameSessionOrder(previousPinnedSessionIds, nextPinnedSessionIds) ||
        !haveSameSessionSet(previousPinnedSessionIds, nextPinnedSessionIds)
      ) {
        postPinnedSessionReorderLog("dragEndSkipped", {
          nextPinnedSessionIds,
          previousPinnedSessionIds,
          reason: haveSameSessionOrder(previousPinnedSessionIds, nextPinnedSessionIds)
            ? "samePinnedOrder"
            : "pinnedSetMismatch",
          resolvedPinnedSessionDropTarget,
          sourceData,
        });
        return;
      }

      /**
       * CDXC:PinnedSessions 2026-05-28-14:29:
       * Dropping a pinned project session must persist exactly the row slot
       * indicated during drag. Resolve pinned drops from pointer position
       * against the pinned partition, then save pinned rows first while leaving
       * non-pinned project sessions in their authoritative order.
       */
      const nextSessionIds = createPinnedFirstSessionOrder(
        (authoritativeSessionIdsByGroup[ sourceData.groupId ] ?? []).length > 0
          ? (authoritativeSessionIdsByGroup[ sourceData.groupId ] ?? [])
          : (previousSessionIdsByGroup[ sourceData.groupId ] ?? []),
        nextPinnedSessionIds,
        sessionsById,
      );
      vscode.postMessage({
        groupId: sourceData.groupId,
        sessionIds: nextSessionIds,
        type: "syncSessionOrder",
      });
      postPinnedSessionReorderLog("syncSessionOrderPosted", {
        nextPinnedSessionIds,
        nextSessionIds,
        previousPinnedSessionIds,
        resolvedPinnedSessionDropTarget,
        sourceData,
      });
      return;
    }

    if (resolvedSessionDropTarget === null) {
      return;
    }

    if (!targetData && resolvedSessionDropTarget === undefined) {
      return;
    }

    const nextSessionIdsByGroup =
      resolvedSessionDropTarget !== undefined
        ? moveSessionIdsByDropTarget(
          currentSessionIdsByGroup,
          sourceData.sessionId,
          resolvedSessionDropTarget,
        )
        : move(currentSessionIdsByGroup, event);
    const nextListedSessionIds = new Set(Object.values(nextSessionIdsByGroup).flat());
    const omittedSessionIds = Object.values(currentSessionIdsByGroup)
      .flat()
      .filter((sessionId) => !nextListedSessionIds.has(sessionId));
    postSidebarDebugLog("native.pane.reorder", "session.dragComputedOrder", {
      currentSessionIdsByGroup,
      nextSessionIdsByGroup,
      omittedSessionIds,
      resolvedSessionDropTarget,
      sourceData,
      targetData,
    });
    const previousGroupId = findSessionGroupId(previousSessionIdsByGroup, sourceData.sessionId);
    const nextGroupId = findSessionGroupId(nextSessionIdsByGroup, sourceData.sessionId);
    if (!previousGroupId || !nextGroupId) {
      return;
    }

    if (previousGroupId !== nextGroupId) {
      if (sessionsById[ sourceData.sessionId ]?.isPinned === true) {
        /**
         * CDXC:PinnedSessions 2026-05-28-12:04:
         * Project pinned sessions are only reorderable inside their owning
         * project. A pinned drag that lands over another project must not turn
         * into a cross-project move just because pinned cards are draggable in
         * the reference sidebar.
         */
        return;
      }

      const targetIndex = nextSessionIdsByGroup[ nextGroupId ]?.indexOf(sourceData.sessionId);
      if (targetIndex == null || targetIndex < 0) {
        return;
      }

      vscode.postMessage({
        groupId: nextGroupId,
        sessionId: sourceData.sessionId,
        targetIndex,
        type: "moveSessionToGroup",
      });
      return;
    }

    if (!isManualActiveSessionsSort) {
      if (sessionsById[ sourceData.sessionId ]?.isPinned === true) {
        const authoritativeSessionIds = authoritativeSessionIdsByGroup[ nextGroupId ] ?? [];
        const previousSessionIds = previousSessionIdsByGroup[ nextGroupId ] ?? [];
        const nextDisplaySessionIds = nextSessionIdsByGroup[ nextGroupId ] ?? [];
        const nextPinnedSessionIds = nextDisplaySessionIds.filter(
          (sessionId) => sessionsById[ sessionId ]?.isPinned === true,
        );
        const previousPinnedSessionIds = previousSessionIds.filter(
          (sessionId) => sessionsById[ sessionId ]?.isPinned === true,
        );
        if (
          !haveSameSessionOrder(previousPinnedSessionIds, nextPinnedSessionIds) &&
          haveSameSessionSet(previousPinnedSessionIds, nextPinnedSessionIds)
        ) {
          /**
           * CDXC:PinnedSessions 2026-05-28-12:04:
           * Last-activity mode still needs pinned rows to be manually
           * rearrangeable within a project. Persist only the pinned partition
           * order, then keep non-pinned sessions in their authoritative order
           * so activity sorting remains display-only for the rest of the group.
           */
          vscode.postMessage({
            groupId: nextGroupId,
            sessionIds: createPinnedFirstSessionOrder(
              authoritativeSessionIds.length > 0 ? authoritativeSessionIds : previousSessionIds,
              nextPinnedSessionIds,
              sessionsById,
            ),
            type: "syncSessionOrder",
          });
        }
      }
      return;
    }

    const previousSessionIds = previousSessionIdsByGroup[ nextGroupId ] ?? [];
    const nextSessionIds = nextSessionIdsByGroup[ nextGroupId ] ?? [];
    if (haveSameSessionOrder(previousSessionIds, nextSessionIds)) {
      return;
    }

    vscode.postMessage({
      groupId: nextGroupId,
      sessionIds: nextSessionIds,
      type: "syncSessionOrder",
    });
  }) satisfies DragDropEventHandlers[ "onDragEnd" ];

  const openSidebarSettings = () => {
    setIsPinnedPromptsOpen(false);
    if (!settings) {
      vscode.postMessage({ type: "openSettings" });
      return;
    }
    setIsPreviousSessionsOpen(false);
    setIsDaemonSessionsOpen(false);
    setIsScratchPadOpen(false);
    setIsSessionSearchSelectionVisible(false);
    setIsSessionSearchOpen(false);
    setSessionSearchQuery("");
    openAppModal({ modal: "settings", type: "open" });
  };

  const openHotkeys = () => {
    /*
     * CDXC:SidebarTopChrome 2026-06-29-01:43:
     * Cmd+. is advertised in the sidebar Settings dropdown after the menu moved out of the titlebar. Route it to the same full-window app-modal host as Settings and Command Palette, closing transient sidebar drawers first so the shortcut opens one focused Hotkeys surface.
     */
    setIsPinnedPromptsOpen(false);
    setIsPreviousSessionsOpen(false);
    setIsDaemonSessionsOpen(false);
    setIsScratchPadOpen(false);
    setIsSessionSearchSelectionVisible(false);
    setIsSessionSearchOpen(false);
    setSessionSearchQuery("");
    openAppModal({ modal: "hotkeys", type: "open" });
  };

  const openCommandPalette = () => {
    /**
     * CDXC:CommandPalette 2026-06-13-10:26:
     * Cmd+Shift+P should open the full-window app-modal command palette,
     * matching Settings instead of rendering a dialog inside the narrow
     * sidebar. Close transient sidebar drawers first so the centered palette is
     * the only active command surface.
     *
     * CDXC:CommandPalette 2026-06-13-22:18:
     * Ghostex Quick Access gives Commands and Sessions separate tabs.
     * This launcher opens Commands; Cmd+P routes to the Sessions modal
     * id instead of encoding a mode in this input query.
     *
   */
    setIsPinnedPromptsOpen(false);
    setIsPreviousSessionsOpen(false);
    setIsDaemonSessionsOpen(false);
    setIsScratchPadOpen(false);
    setIsSessionSearchSelectionVisible(false);
    setIsSessionSearchOpen(false);
    setSessionSearchQuery("");
    openQuickAccess("commands");
  };

  const openKeepAwakePowerSettings = () => {
    setIsPinnedPromptsOpen(false);
    setIsPreviousSessionsOpen(false);
    setIsDaemonSessionsOpen(false);
    setIsScratchPadOpen(false);
    setIsSessionSearchSelectionVisible(false);
    setIsSessionSearchOpen(false);
    setSessionSearchQuery("");
    openAppModal({
      initialSearchQuery: "Keep awake",
      modal: "settings",
      type: "open",
    });
  };

  const startSidebarKeepAwake = (durationMinutes: KeepAwakeDurationMinutes) => {
    /*
     * CDXC:SidebarTopChrome 2026-06-29-01:43:
     * Keep Awake moved from the macOS titlebar into the sidebar shortcut row, but the titlebar host remains the caffeinate runtime owner. Optimistically reflect the chosen duration in sidebar UI while native forwards the command to the existing titlebar start path.
     */
    setSidebarKeepAwakeRuntime({ durationMinutes });
    vscode.postMessage({
      action: "start",
      durationMinutes,
      type: "runTitlebarKeepAwakeCommand",
    });
    window.setTimeout(() => {
      setSidebarKeepAwakeRuntime(readSidebarKeepAwakeRuntime() ?? { durationMinutes });
    }, 250);
  };

  const stopSidebarKeepAwake = () => {
    setSidebarKeepAwakeRuntime(undefined);
    vscode.postMessage({
      action: "stop",
      type: "runTitlebarKeepAwakeCommand",
    });
    window.setTimeout(() => {
      setSidebarKeepAwakeRuntime(readSidebarKeepAwakeRuntime());
    }, 250);
  };

  const closeSessionSearch = () => {
    setIsSessionSearchSelectionVisible(false);
    setIsSessionSearchOpen(false);
    setSessionSearchQuery("");
  };

  const closeTopmostSidebarOverlay = useEffectEvent(() => {
    if (gitCommitDraft) {
      closeGitCommitModal(gitCommitDraft.requestId);
      return true;
    }

    if (isDaemonSessionsOpen) {
      setIsDaemonSessionsOpen(false);
      return true;
    }

    if (isSettingsOpen) {
      setIsSettingsOpen(false);
      return true;
    }

    if (isPreviousSessionsOpen) {
      setIsPreviousSessionsOpen(false);
      return true;
    }

    if (isPinnedPromptsOpen) {
      setIsPinnedPromptsOpen(false);
      return true;
    }

    if (isScratchPadOpen) {
      setIsScratchPadOpen(false);
      return true;
    }

    if (isSessionSearchOpen) {
      closeSessionSearch();
      return true;
    }

    return false;
  });

  const restoreSearchedPreviousSession = (historyId: string) => {
    vscode.postMessage({
      historyId,
      type: "restorePreviousSession",
    });
    closeSessionSearch();
  };

  const deleteSearchedPreviousSession = (historyId: string) => {
    vscode.postMessage({
      historyId,
      type: "deletePreviousSession",
    });
  };

  /*
   * CDXC:SidebarV2 2026-07-29:
   * Search results are one feature, not a V1 feature: the Inbox sidebar filters
   * the live list exactly as V1 does, so it must also offer the closed sessions
   * that match. This group is self-contained (it posts its own restore/delete
   * messages), so both sidebars render the same element rather than V2 shipping
   * a second previous-sessions implementation.
   */
  const previousSessionsSearchGroup = isSessionSearchFiltering ? (
    <SidebarPreviousSessionsSearchGroup
      onDeletePreviousSession={deleteSearchedPreviousSession}
      onRestorePreviousSession={restoreSearchedPreviousSession}
      previousSessions={filteredPreviousSessions}
      selectedHistoryId={
        isSessionSearchSelectionVisible && selectedSessionSearchResult?.kind === "previous"
          ? selectedSessionSearchResult.historyId
          : undefined
      }
      showDebugSessionNumbers={debuggingMode}
    />
  ) : null;

  const activateSelectedSessionSearchResult = useEffectEvent(() => {
    if (!selectedSessionSearchResult) {
      return false;
    }

    if (selectedSessionSearchResult.kind === "previous") {
      restoreSearchedPreviousSession(selectedSessionSearchResult.historyId);
      return true;
    }

    const selectedResult = sidebarSessionSearchResults.find((result) =>
      isSidebarSessionSearchSelectionMatch(result, selectedSessionSearchResult),
    );
    if (!selectedResult || selectedResult.kind !== "session") {
      return false;
    }

    dismissAppModalForSidebarNavigation("SettingsDismissal:sessionSearchActivate");
    useSidebarStore.getState().clearFocusedSessionScrollSuppression();
    applyLocalFocus(selectedResult.groupId, selectedResult.sessionId);
    vscode.postMessage({
      sessionId: selectedResult.sessionId,
      type: "focusSession",
    });
    return true;
  });

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      const target = event.target;
      if (!(target instanceof Node)) {
        return;
      }
      const searchInput = searchInputRef.current;
      const isSearchInputTarget = searchInput !== null && target === searchInput;

      if (event.key === "Escape") {
        if (isSearchInputTarget && sessionSearchQuery.length > 0) {
          event.preventDefault();
          event.stopPropagation();
          setSessionSearchQuery("");
          searchInput.focus();
          return;
        }
        if (!closeTopmostSidebarOverlay()) {
          return;
        }

        event.preventDefault();
        event.stopPropagation();
        return;
      }

      const commandPaletteHotkeyActionId = getCommandPaletteHotkeyActionId(
        event,
        settings?.hotkeys,
      );
      if (commandPaletteHotkeyActionId && !hasActiveSidebarHotkeyRecorder()) {
        event.preventDefault();
        event.stopPropagation();
        if (commandPaletteHotkeyActionId === "openCommandPalette") {
          openCommandPalette();
        } else {
          openPreviousSessions();
        }
        return;
      }

      if (
        event.defaultPrevented ||
        gitCommitDraft !== undefined ||
        isDaemonSessionsOpen ||
        isPreviousSessionsOpen ||
        isScratchPadOpen ||
        (isEditableSidebarKeyboardTarget(target) && !isSearchInputTarget)
      ) {
        return;
      }

      if (
        isSessionSearchOpen &&
        isSidebarSessionSearchNavigationKey(event) &&
        (isSearchInputTarget || !isEditableSidebarKeyboardTarget(target))
      ) {
        const nextSelection = getNextSidebarSessionSearchSelection({
          currentSelection: selectedSessionSearchResult,
          direction: getSidebarSessionSearchNavigationDirection(event),
          results: sidebarSessionSearchResults,
        });
        if (!nextSelection) {
          return;
        }

        event.preventDefault();
        event.stopPropagation();
        setSelectedSessionSearchResult(nextSelection);
        setIsSessionSearchSelectionVisible(true);
        return;
      }

      if (
        isSessionSearchOpen &&
        event.key === "Enter" &&
        (isSearchInputTarget || !isEditableSidebarKeyboardTarget(target))
      ) {
        if (!activateSelectedSessionSearchResult()) {
          return;
        }

        event.preventDefault();
        event.stopPropagation();
        setIsSessionSearchSelectionVisible(false);
        return;
      }

      if (isSearchInputTarget) {
        return;
      }

      /*
       * CDXC:SidebarKeyboard 2026-05-26-15:29:
       * Ordinary typing while focus is on sidebar chrome should not open or edit session search.
       * Leave non-editable sidebar keypresses unhandled so the host can provide its default invalid-key feedback instead of capturing the user's text in the sidebar.
       */
    };

    document.addEventListener("keydown", handleKeyDown, true);
    return () => {
      document.removeEventListener("keydown", handleKeyDown, true);
    };
  }, [
    activateSelectedSessionSearchResult,
    closeTopmostSidebarOverlay,
    gitCommitDraft,
    isDaemonSessionsOpen,
    isPreviousSessionsOpen,
    isScratchPadOpen,
    isSessionSearchOpen,
    selectedSessionSearchResult,
    sessionSearchQuery,
    sidebarSessionSearchResults,
  ]);

  const setActiveSessionsSortMode = (sortMode: SidebarActiveSessionsSortMode) => {
    vscode.postMessage({
      manualSessionIdsByGroup:
        sortMode === "manual" && activeSessionsSortMode !== "manual"
          ? Object.fromEntries(
            workspaceGroupIds.map((groupId) => [
              groupId,
              [ ...(effectiveSessionIdsByGroup[ groupId ] ?? []) ],
            ]),
          )
          : undefined,
      sortMode,
      type: "setActiveSessionsSortMode",
    });
  };

  /*
   * CDXC:SidebarV2 2026-07-29:
   * Sidebar version and its Group by Project sub-mode are persisted settings,
   * not sidebar-local view state. Write them through the same settings patch
   * channel the Settings modal uses so gpui persists them and hydrates every
   * surface back, instead of the sort-mode channel that gpui does not handle.
   */
  const updateSidebarVersionSettings = (patch: {
    sidebarV2Layout?: SidebarV2Layout;
    sidebarVersion?: SidebarVersion;
  }) => {
    vscode.postMessage({
      baseRevision: revision,
      patch,
      source: "sidebar:sidebarVersion",
      type: "updateSettingsPatch",
    });
  };

  const setSidebarVersion = (nextSidebarVersion: SidebarVersion) => {
    if (nextSidebarVersion === sidebarVersion) {
      return;
    }
    updateSidebarVersionSettings({ sidebarVersion: nextSidebarVersion });
  };

  const setSidebarV2Layout = (nextLayout: SidebarV2Layout) => {
    if (nextLayout === sidebarV2Layout) {
      return;
    }
    updateSidebarVersionSettings({ sidebarV2Layout: nextLayout });
  };

  const toggleActiveSessionsSortMode = () => {
    setActiveSessionsSortMode(
      activeSessionsSortMode === "manual" ? "lastActivity" : "manual",
    );
  };

  const toggleSessionTagFilter = (sessionTag: SidebarSessionTagFilter) => {
    if (!enabledVisibleSidebarSessionTagSet.has(sessionTag)) {
      return;
    }
    setSelectedSessionTagFilters((current) =>
      current.includes(sessionTag)
        ? current.filter((tag) => tag !== sessionTag)
        : [ ...current, sessionTag ],
    );
  };

  const moveSidebar = () => {
    dismissAppModalForSidebarNavigation("SettingsDismissal:moveSidebar");
    vscode.postMessage({ type: "moveSidebarToOtherSide" });
  };

  const toggleSidebarCollapsed = () => {
    dismissAppModalForSidebarNavigation("SettingsDismissal:toggleSidebar");
    /**
     * CDXC:SidebarCollapse 2026-06-12-02:23:
     * Sidebar collapse is native chrome state. React requests the toggle, while
     * AppKit owns hiding the sidebar WebView, divider, and workspace border.
     */
    vscode.postMessage({ type: "toggleSidebarCollapsed" });
  };

  /*
   * CDXC:AddProject 2026-07-30:
   * Add Project opens the shared add-project dialog in the app-modal host for
   * every entry point. The local header sends no machine (the dialog resolves
   * the machine list itself and skips its machine step when there is only one),
   * while a remote machine header preselects its own machine so the flow can
   * never silently browse this computer's filesystem instead of that machine's.
   */
  const openAddProjectModal = (machineId?: string) => {
    dismissAppModalForSidebarNavigation("SettingsDismissal:addProject");
    openAppModal({ ...(machineId ? { machineId } : {}), modal: "addProject", type: "open" });
  };

  const createReferenceAgentChat = (agent: SidebarAgentButton) => {
    const quickGroupId = displayedReferenceChatGroupIds[ 0 ];
    if (!quickGroupId) {
      return;
    }

    dismissAppModalForSidebarNavigation("SettingsDismissal:createQuickAgent");
    /**
     * CDXC:QuickAgents 2026-06-08-18:25:
     * The Quick section header should expose the same selected-agent split picker as project headers. Launch through runSidebarAgent with the synthetic Quick group id so native creates a new projectless agent chat instead of targeting the active code project.
     */
    setPrimaryAgentLauncherId(agent.agentId);
    writePrimaryAgentLauncherId(agent.agentId);
    vscode.postMessage({
      agentId: agent.agentId,
      groupId: quickGroupId,
      type: "runSidebarAgent",
    });
  };

  /*
   * CDXC:SidebarV2Worktree 2026-07-29:
   * Sidebar V2's "+" launches through THIS function so the instant path stays
   * byte-identical to the classic sidebar's: a project click posts the same
   * `runSidebarAgent` the project header posts.
   *
   * CDXC:SidebarV2SingleCreateControl 2026-07-30:
   * V2 no longer reaches the `!groupId` branch from any ordinary create path.
   * Its header "+" and its agent picker both resolve a REAL project first (V2's
   * own `headerCreateGroupId`: scoped project, then active project, then the
   * first project), so a session can no longer silently land in Quick just
   * because the click came from the header rather than from a project row.
   *
   * The branch stays because `groupId` is genuinely optional in one case: a
   * workspace with ZERO project groups, where V2's resolution has nothing to
   * return. Quick is then the only place a session can go, so falling through to
   * the Quick launcher is the correct answer rather than a downgrade. Quick
   * creation ON PURPOSE happens through the chevron's explicitly-labelled
   * "Quick Terminal" / "Quick Browser Tab" items, which never come here.
   */
  const runSidebarV2Agent = (agent: SidebarAgentButton, groupId?: string) => {
    if (!groupId) {
      createReferenceAgentChat(agent);
      return;
    }
    dismissAppModalForSidebarNavigation("SettingsDismissal:createSidebarV2Agent");
    setPrimaryAgentLauncherId(agent.agentId);
    writePrimaryAgentLauncherId(agent.agentId);
    vscode.postMessage({
      agentId: agent.agentId,
      groupId,
      type: "runSidebarAgent",
    });
  };

  /*
   * CDXC:SidebarV2Worktree 2026-07-29:
   * The "default new sessions to worktree" preference is GLOBAL and rides the
   * same settings patch channel as the sidebar version switch, so gpui persists
   * it and every surface hydrates it back.
   */
  const setNewSessionsDefaultEnvMode = (mode: SidebarNewSessionEnvMode) => {
    if (mode === effectiveSettings.newSessionsDefaultEnvMode) {
      return;
    }
    vscode.postMessage({
      baseRevision: revision,
      patch: { newSessionsDefaultEnvMode: mode },
      source: "sidebar:newSessionsDefaultEnvMode",
      type: "updateSettingsPatch",
    });
  };

  /*
   * CDXC:SidebarV2LogicalProjects 2026-07-29:
   * Cross-machine grouping overrides ride the SAME settings patch channel as
   * the sidebar version switch, under their own source so a grouping change can
   * never be mistaken for a remote-machine-capable save. V2 hands over the
   * whole record, so the patch is a straight replacement rather than a merge
   * the settings pipeline would have to interpret.
   */
  const setSidebarProjectGroupingOverrides = (
    overrides: Readonly<Record<string, SidebarProjectGroupingMode>>,
  ) => {
    vscode.postMessage({
      baseRevision: revision,
      patch: { sidebarProjectGroupingOverrides: overrides },
      source: "sidebar:projectGrouping",
      type: "updateSettingsPatch",
    });
  };

  const openConfigureAgentsModal = () => {
    dismissAppModalForSidebarNavigation("SettingsDismissal:configureAgents");
    openAppModal({ modal: "configureAgents", type: "open" });
  };

  const openReferenceAutomations = () => {
    dismissAppModalForSidebarNavigation("SettingsDismissal:automations");
    vscode.postMessage({ type: "openAutomationsPage" });
  };

  const openReferenceMobile = () => {
    dismissAppModalForSidebarNavigation("SettingsDismissal:mobile");
    vscode.postMessage({ type: "openMobileBrowserChat" });
  };

  const openReferenceAgentsHub = () => {
    dismissAppModalForSidebarNavigation("SettingsDismissal:agentsHub");
    openAppModal({ modal: "agentsHub", type: "open" });
  };

  const togglePinnedPrompts = () => {
    dismissAppModalForSidebarNavigation("SettingsDismissal:pinnedPrompts");
    setIsDaemonSessionsOpen(false);
    setIsPreviousSessionsOpen(false);
    setIsScratchPadOpen(false);
    setIsSessionSearchSelectionVisible(false);
    setIsSessionSearchOpen(false);
    setSessionSearchQuery("");
    openAppModal({ modal: "pinnedPrompts", type: "open" });
  };

  const openPreviousSessions = () => {
    dismissAppModalForSidebarNavigation("SettingsDismissal:previousSessions");
    setIsPinnedPromptsOpen(false);
    setIsDaemonSessionsOpen(false);
    setIsScratchPadOpen(false);
    setIsSessionSearchSelectionVisible(false);
    setIsSessionSearchOpen(false);
    setSessionSearchQuery("");
    openQuickAccess("recentSessions");
  };

  const searchPreviousSessionsByPrompt = () => {
    dismissAppModalForSidebarNavigation("SettingsDismissal:previousSessionsPromptSearch");
    setIsPinnedPromptsOpen(false);
    setIsDaemonSessionsOpen(false);
    setIsScratchPadOpen(false);
    setIsSessionSearchSelectionVisible(false);
    setIsSessionSearchOpen(false);
    setSessionSearchQuery("");
    vscode.postMessage({ type: "searchPreviousSessionsByText" });
  };

  const renderReferenceProjectGroup = (groupId: string) => {
    const projectId = groupsById[ groupId ]?.projectContext?.editor.projectId;
    return (
      <SessionGroupSection
        autoEdit={autoEditingGroupId === groupId}
        allowPinnedSessionReorder={!isManualActiveSessionsSort}
        canClose={effectiveGroupIds.length > 1}
        completionFlashNonceBySessionId={completionFlashNonceBySessionId}
        draggingDisabled={isSessionSearchOpen}
        enableProjectSessionListToggle={!isSessionSearchFiltering}
        groupDropIndicator={groupDropIndicator}
        groupId={groupId}
        index={displayedReferenceProjectGroupIds.indexOf(groupId)}
        isCollapsed={isSidebarSearchProjectGroupRenderedCollapsed(groupId)}
        isHidden={hiddenGroupIds.includes(groupId)}
        isGroupDragPreviewSource={groupDragPreview?.groupId === groupId}
        key={groupId}
        onAutoEditHandled={() => setAutoEditingGroupId(undefined)}
        onCollapsedChange={setGroupCollapsed}
        onCreateProjectCollection={enableProjectCollections ? createProjectCollectionForProject : undefined}
        onFocusRequested={focusSidebarSessionFromNavigation}
        onMoveProjectToCollection={enableProjectCollections ? moveProjectToCollection : undefined}
        onProjectSessionListCollapsedChange={setProjectSessionListCollapsed}
        onHideGroup={() => setHiddenGroupIds((current) => current.includes(groupId) ? current.filter((id) => id !== groupId) : [...current, groupId])}
        onSessionSelectionChange={handleSidebarSessionSelectionChange}
        orderedSessionIds={displayedWorkspaceSessionIdsByGroup[ groupId ] ?? []}
        pinnedSessionDropIndicator={pinnedSessionDropIndicator}
        projectCollectionId={projectId ? projectCollectionIdByProjectId.get(projectId) : undefined}
        projectCollectionOptions={enableProjectCollections ? projectCollections.collections : undefined}
        projectSessionListCollapsedState={collapsedProjectSessionListsById}
        revealSessionRequest={sessionRevealRequest}
        selectedSearchSessionId={
          isSessionSearchSelectionVisible && selectedSessionSearchResult?.kind === "session"
            ? selectedSessionSearchResult.sessionId
            : undefined
        }
        selectedSessionIds={selectedSidebarSessionIds}
        sessionDraggingDisabled={!isManualActiveSessionsSort}
        sessionDropIndicator={sessionDropIndicator}
        sessionTagListItems={sidebarSessionTagListItems}
        showHeaderActions={true}
        showSessionDropPositionIndicators={true}
        useColoredAgentIcons={effectiveSettings.useColoredSessionAgentIcons}
        vscode={vscode}
      />
    );
  };

  return (
    <TooltipProvider delayDuration={TOOLTIP_DELAY_MS}>
      <SidebarCollapseAnimationProvider
        durationMs={effectiveSettings.sidebarCollapseAnimationDurationMs}
      >
      <div
        className="sidebar-reference-layout"
        data-project-reorder-drag={String(isProjectReorderDragActive)}
        data-project-group-style={effectiveSettings.sidebarProjectGroupStyle}
        data-reference-sidebar="true"
        data-session-agent-icon-color-mode={
          effectiveSettings.useColoredSessionAgentIcons ? "colored" : "monochrome"
        }
        ref={setReferenceLayoutElement}
        style={{
          "--sidebar-collapse-duration": `${effectiveSettings.sidebarCollapseAnimationDurationMs}ms`,
        } as CSSProperties}
      >
        {showCommandHotkeyOverlay ? <SidebarHotkeyOverlay hotkeys={settings?.hotkeys} /> : null}
        <SidebarReferenceTopChrome
          keepAwakeRuntime={sidebarKeepAwakeRuntime}
          onOpenAgentsHub={openReferenceAgentsHub}
          onOpenAutomations={openReferenceAutomations}
          onOpenDiscord={() => {
            vscode.postMessage({ type: "openExternalUrl", url: GHOSTEX_DISCORD_URL });
          }}
          onOpenHotkeys={openHotkeys}
          onOpenMobile={openReferenceMobile}
          onOpenPowerSettings={openKeepAwakePowerSettings}
          onOpenPreviousSessions={openPreviousSessions}
          onRunKeepAwake={startSidebarKeepAwake}
          onSearchPreviousSessionsByPrompt={searchPreviousSessionsByPrompt}
          onSearch={openPreviousSessions}
          onStopKeepAwake={stopSidebarKeepAwake}
          onTogglePetOverlay={() => {
            vscode.postMessage({ type: "togglePetOverlay" });
          }}
          settings={effectiveSettings}
          showKeepAwakeButton={showSidebarKeepAwakeButton}
        />
        <div
          className="stack"
          data-dimmed={String(isStartupInteractionBlocked)}
          data-sidebar-custom-theme={String(Boolean(normalizeWorkspaceThemeColor(customThemeColor)))}
          data-sidebar-theme={theme}
          onClickCapture={handleSidebarClickCapture}
          onDoubleClick={handleSidebarDoubleClick}
        >
          <section className="session-groups-panel" ref={sessionGroupsPanelRef}>
            <div className="session-groups-top">
              {null}
            </div>
            {/*
            CDXC:SidebarScroll 2026-06-30-01:59:
            The sidebar's project list must scroll as fast as the browser can move it.
            Do not apply the vertical scroll mask or sticky-header gradient geometry here; the user explicitly accepts losing those visual fades to remove scroll-linked paint work.
          */}
            <div
              className="session-groups-scroll-shell"
              data-scrollable-y={String(sessionGroupsHaveScrollableOverflow)}
            >
              <div
                className="session-groups-content"
                data-scrollable-y={String(sessionGroupsHaveScrollableOverflow)}
                ref={sessionGroupsContentRef}
              >
                {/*
                CDXC:SidebarSessions 2026-05-17-00:11:
                Opening or closing one session must not remount every sidebar
                project. Keep DragDropProvider stable so sortable/droppable hooks
                update the dnd registry without forcing all project rows to
                replay their entrance animation.

                CDXC:SidebarV2GroupedProjectUX 2026-07-30:
                ONE provider now wraps BOTH sidebar bodies. Grouped V2 reorders
                projects through the same dnd-kit sortables, the same pointer drop
                resolution, and the same `syncGroupOrder` contract as V1, so a
                second provider would mean a second dnd manager, a second sensor
                set, and two registries that disagree about what is being dragged.
                It is deliberately mounted OUTSIDE the version switch so switching
                sidebars does not unmount and remount the manager mid-session.
              */}
                <DragDropProvider
                  onDragEnd={handleDragEnd}
                  onDragMove={handleDragMove}
                  onDragOver={handleDragOver}
                  onDragStart={handleDragStart}
                  plugins={(plugins) => plugins.filter((plugin) => plugin !== Cursor)}
                  sensors={sensors}
                >
                {/*
                 * CDXC:SidebarV2 2026-07-29:
                 * Sidebar V2 replaces only the session list body. Top chrome,
                 * search, and every host message path stay shared, and the V1
                 * tree below is untouched so the default sidebar renders
                 * exactly as before.
                 */}
                {isSidebarV2Active ? (
                  /*
                   * CDXC:SidebarV2 2026-07-29:
                   * The Inbox header IS this shared section header, not a
                   * parallel V2 copy. It already owns the pieces V2 needs —
                   * Sort & Filter (tag filters, Group by Project, the way back
                   * to the classic sidebar) and search — and every one of them
                   * posts the same host messages in both sidebars. V2 adds only
                   * the project scope dropdown, which lives in its own toolbar
                   * because it filters the inbox rather than acting on it.
                   *
                   * CDXC:SidebarV2SingleCreateControl 2026-07-30:
                   * The creation cluster is the one piece V2 does NOT take from
                   * here. V2 owns a single split "+" in its own toolbar, so this
                   * mount deliberately withholds `onCreateBrowserChat`,
                   * `onCreateChat`, `onRunAgent`, and `onConfigureAgents` (plus
                   * the agent inputs that only feed that split button): every one
                   * of those props is optional, so their buttons simply do not
                   * render, and the header is left with Sort & Filter. The V1
                   * mount below still passes all of them, unchanged. Both Quick
                   * creators moved into V2's chevron menu as explicitly-labelled
                   * items, and "Configure agents" stays reachable through the
                   * command palette.
                   */
                  <SidebarReferenceSectionHeader
                    activeSessionsSortMode={activeSessionsSortMode}
                    actionsAlwaysVisible={true}
                    collapsed={isReferenceProjectsRenderedCollapsed}
                    containsActiveSession={[...groupIdsContainingActiveSession].some(
                      (groupId) => groupsById[groupId]?.isChatCollection !== true,
                    )}
                    onFilterChats={openPreviousSessions}
                    onSetActiveSessionsSortMode={setActiveSessionsSortMode}
                    onSetSidebarV2Layout={setSidebarV2Layout}
                    onSetSidebarVersion={setSidebarVersion}
                    onToggleSessionTagFilter={toggleSessionTagFilter}
                    onToggleCollapsed={() => {
                      setIsReferenceProjectsCollapsed((previous) => !previous);
                    }}
                    sectionKey="projects"
                    selectedSessionTagFilters={activeSelectedSessionTagFilters}
                    sessionSummary={sidebarV2SectionSessionSummary}
                    sessionTagListItems={sidebarSessionTagListItems}
                    sidebarV2Layout={sidebarV2Layout}
                    sidebarVersion={sidebarVersion}
                    title="Sessions"
                  />
                ) : null}
                {isSidebarV2Active && projectsSectionPresence.isPresent ? (
                  <div
                    aria-hidden={projectsSectionPresence.isVisuallyCollapsed}
                    className="sidebar-v2-section-body sidebar-animated-collapse-body"
                    data-collapsed={String(projectsSectionPresence.isVisuallyCollapsed)}
                    inert={projectsSectionPresence.isVisuallyCollapsed ? true : undefined}
                    ref={projectsSectionPresence.setCollapsibleElement}
                  >
                    <SidebarV2Root
                      /*
                       * CDXC:SidebarV2Worktree 2026-07-29:
                       * V2's creation control launches through SidebarApp's own
                       * agent path, so it needs the same two inputs the shared
                       * header uses: the configured agents and the last-used
                       * agent id. No new launch logic lives in V2.
                       */
                      agents={agents}
                      /*
                       * CDXC:SidebarV2LogicalProjects 2026-07-29:
                       * Each daemon's OWN auto-settle window, so a remote
                       * machine's sessions are partitioned with that machine's
                       * setting instead of this Mac's.
                       */
                      autoSettleAfterDays={sidebarAutoSettleAfterDays}
                      autoSettleAfterDaysByMachineId={sidebarAutoSettleAfterDaysByMachineId}
                      collapsedGroupsById={collapsedGroupsById}
                      /*
                       * CDXC:SidebarV2GroupedProjectUX 2026-07-30:
                       * Project reorder state, from the SAME dnd pipeline V1
                       * uses: the resolved insertion boundary, which row is
                       * currently being dragged, and V1's manual-sort gate. The
                       * ghost that follows the cursor is rendered by SidebarApp,
                       * so V2 only needs to paint the source placeholder and the
                       * drop line.
                       */
                      draggingGroupId={groupDragPreview?.groupId}
                      groupDropIndicator={groupDropIndicator}
                      groupIds={sidebarV2DisplayedGroupIds}
                      isGroupReorderDisabled={!isManualActiveSessionsSort}
                      isPinnedSessionReorderDisabled={isSessionSearchOpen}
                      groupsById={groupsById}
                      hostEmptyState={sidebarV2HostEmptyState}
                      isSearchFiltering={isSessionSearchFiltering}
                      layout={sidebarV2Layout}
                      /*
                       * CDXC:SidebarV2Lifecycle 2026-07-29:
                       * Settle/snooze support travels on the HUD because it is
                       * a property of the daemons behind the rows, not of the
                       * rows themselves — the local gxserver plus one entry per
                       * connected remote machine. Hosts that publish neither
                       * leave both undefined and V2 hides every lifecycle
                       * affordance.
                       */
                      lifecycleCapabilities={sidebarLifecycleCapabilities}
                      lifecycleCapabilitiesByMachineId={sidebarLifecycleCapabilitiesByMachineId}
                      /*
                       * CDXC:SidebarV2Worktree 2026-07-29:
                       * The worktree popover reads host answers off the SAME
                       * source SidebarApp listens on. gpui hands the sidebar a
                       * private message source, so passing `window` here would
                       * work in Storybook and silently never fire in the app.
                       */
                      messageSource={messageSource}
                      /*
                       * CDXC:AddProject 2026-07-30:
                       * V2 hides the classic Projects section header, so its
                       * create menu carries the same Add Project entry point
                       * the shared header exposes.
                       */
                      onAddProject={() => openAddProjectModal()}
                      onGroupedRowsChange={setSidebarV2GroupOrderRows}
                      onSetGroupCollapsed={setGroupCollapsed}
                      onSetNewSessionsDefaultEnvMode={setNewSessionsDefaultEnvMode}
                      onSetProjectGroupingOverrides={setSidebarProjectGroupingOverrides}
                      onSetSidebarVersion={setSidebarVersion}
                      pinnedSessionDropIndicator={pinnedSessionDropIndicator}
                      onSetLayout={setSidebarV2Layout}
                      onRunAgent={runSidebarV2Agent}
                      primaryAgentId={primaryAgentLauncherId}
                      searchQuery={sessionSearchQuery}
                      selectedSessionTagFilters={activeSelectedSessionTagFilters}
                      sessionIdsByGroup={displayedWorkspaceSessionIdsByGroup}
                      sessionsById={sessionsById}
                      settings={effectiveSettings}
                      vscode={vscode}
                    />
                    {previousSessionsSearchGroup}
                  </div>
                ) : null}
                {isSidebarV2Active ? null : (
                <>
                  {!shouldHideReferenceSectionsForSearchEmptyState ? (
                    <SidebarReferenceSectionHeader
                      activeSessionsSortMode={activeSessionsSortMode}
                      actionsAlwaysVisible={displayedReferenceProjectGroupIds.length === 0}
                      bulkActionLabel={
                        displayedReferenceProjectGroupIds.length > 0
                          ? hasExpandedReferenceProjects
                            ? "Collapse All"
                            : "Expand Previous"
                          : undefined
                      }
                      collapsed={isReferenceProjectsRenderedCollapsed}
                      containsActiveSession={[...groupIdsContainingActiveSession].some(
                        (groupId) =>
                          groupsById[groupId]?.isChatCollection !== true &&
                          groupsById[groupId]?.remoteMachineContext === undefined,
                      )}
                      onAddProject={() => openAddProjectModal()}
                      onToggleShowHidden={() => setShowHiddenSidebarItems((current) => !current)}
                      showHidden={showHiddenSidebarItems}
                      onBulkProjectToggle={
                        displayedReferenceProjectGroupIds.length > 0
                          ? () => {
                            postSidebarCollapseStateLog("projectBulkCommand", {
                              expandedProjectGroupCount:
                                displayedReferenceProjectGroupIds.length -
                                Object.keys(collapsedGroupsById).filter((groupId) =>
                                  displayedReferenceProjectGroupIds.includes(groupId),
                                ).length,
                              mode: hasExpandedReferenceProjects
                                ? "collapse-all"
                                : "expand-previous",
                              previousExpandedGroupCount:
                                previousExpandedReferenceProjectGroupIdsRef.current.length,
                              projectGroupCount: displayedReferenceProjectGroupIds.length,
                            });
                            if (isReferenceProjectsCollapsed && !hasExpandedReferenceProjects) {
                              triggerReferenceSectionChildAnimation("projects");
                            }
                            setIsReferenceProjectsCollapsed(false);
                            if (hasExpandedReferenceProjects) {
                              previousExpandedReferenceProjectGroupIdsRef.current =
                                displayedReferenceProjectGroupIds.filter(
                                  (groupId) => collapsedGroupsById[ groupId ] !== true,
                                );
                              setGroupsCollapsed(displayedReferenceProjectGroupIds, true);
                              return;
                            }

                            const previousExpandedProjectGroupIds =
                              previousExpandedReferenceProjectGroupIdsRef.current.filter(
                                (groupId) => displayedReferenceProjectGroupIds.includes(groupId),
                              );
                            setGroupsCollapsed(
                              previousExpandedProjectGroupIds.length > 0
                                ? previousExpandedProjectGroupIds
                                : displayedReferenceProjectGroupIds,
                              false,
                            );
                          }
                          : undefined
                      }
                      onSetActiveSessionsSortMode={setActiveSessionsSortMode}
                      onSetSidebarV2Layout={setSidebarV2Layout}
                      onSetSidebarVersion={setSidebarVersion}
                      onToggleSessionTagFilter={toggleSessionTagFilter}
                      onToggleCollapsed={() => {
                        const nextCollapsed = !isReferenceProjectsCollapsed;
                        postSidebarCollapseStateLog("sectionToggle", {
                          childGroupCount: displayedReferenceProjectGroupIds.length,
                          collapsed: nextCollapsed,
                          section: "projects",
                        });
                        if (isReferenceProjectsCollapsed) {
                          triggerReferenceSectionChildAnimation("projects");
                        }
                        setIsReferenceProjectsCollapsed((previous) => !previous);
                      }}
                      sectionKey="projects"
                      selectedSessionTagFilters={activeSelectedSessionTagFilters}
                      sessionSummary={referenceProjectsSectionSessionSummary}
                      sessionTagListItems={sidebarSessionTagListItems}
                      sidebarV2Layout={sidebarV2Layout}
                      sidebarVersion={sidebarVersion}
                      title="Projects"
                    />
                  ) : null}
                  {!shouldHideReferenceSectionsForSearchEmptyState &&
                  projectsSectionPresence.isPresent ? (
                    <div
                      aria-hidden={projectsSectionPresence.isVisuallyCollapsed}
                      className="group-list workspace-group-list reference-project-group-list reference-sidebar-collapsible-body"
                      data-animate-children={String(referenceSectionChildAnimations.projects)}
                      data-collapsed={String(projectsSectionPresence.isVisuallyCollapsed)}
                      inert={projectsSectionPresence.isVisuallyCollapsed ? true : undefined}
                      ref={projectsSectionPresence.setCollapsibleElement}
                      data-sidebar-project-list-scope={LOCAL_PROJECT_LIST_SCOPE_ID}
                    >
                      {displayedReferenceProjectGroupIds.length > 0 ? (
                        <>
                          {displayedProjectCollectionItems.map((item, itemIndex) =>
                            item.kind === "project" ? (
                              renderReferenceProjectGroup(item.groupId)
                            ) : (
                              <ProjectCollectionSection
                              autoEdit={autoEditingProjectCollectionId === item.collection.collectionId}
                              bulkProjectActionLabel={
                                item.groupIds.some(
                                  (groupId) => collapsedGroupsById[groupId] !== true,
                                )
                                  ? "Collapse All"
                                  : "Expand Previous"
                              }
                              collapsed={
                                !isSessionSearchFiltering &&
                                collapsedProjectCollectionsByKey[
                                  createLocalProjectCollectionCollapseKey(
                                    item.collection.collectionId,
                                  )
                                ] === true
                              }
                              collection={item.collection}
                              containsActiveSession={item.groupIds.some((groupId) =>
                                groupIdsContainingActiveSession.has(groupId),
                              )}
                              draggingDisabled={isSessionSearchOpen}
                              dropIndicatorPosition={
                                projectCollectionDropIndicator?.collectionId ===
                                  item.collection.collectionId
                                  ? projectCollectionDropIndicator.position
                                  : undefined
                              }
                              index={itemIndex}
                              isDragPreviewSource={
                                projectCollectionDragPreview?.collectionId ===
                                item.collection.collectionId
                              }
                              isHidden={hiddenCollectionKeys.includes(
                                `local:${item.collection.collectionId}`,
                              )}
                              key={item.collection.collectionId}
                              onAutoEditHandled={() => setAutoEditingProjectCollectionId(undefined)}
                              onBulkProjectToggle={() => {
                                const hasExpandedProjects = item.groupIds.some(
                                  (groupId) => collapsedGroupsById[groupId] !== true,
                                );
                                if (hasExpandedProjects) {
                                  previousExpandedProjectGroupIdsByCollectionIdRef.current[
                                    item.collection.collectionId
                                  ] = item.groupIds.filter(
                                    (groupId) => collapsedGroupsById[groupId] !== true,
                                  );
                                  setGroupsCollapsed(item.groupIds, true);
                                  return;
                                }

                                const previousExpandedProjectGroupIds =
                                  previousExpandedProjectGroupIdsByCollectionIdRef.current[
                                    item.collection.collectionId
                                  ]?.filter((groupId) => item.groupIds.includes(groupId)) ?? [];
                                setGroupsCollapsed(
                                  previousExpandedProjectGroupIds.length > 0
                                    ? previousExpandedProjectGroupIds
                                    : item.groupIds,
                                  false,
                                );
                              }}
                              onChange={(updated) => {
                                setProjectCollections((previous) =>
                                  updateSidebarProjectCollection(
                                    previous,
                                    updated.collectionId,
                                    (existing) => ({
                                      ...existing,
                                      color: updated.color,
                                      title: updated.title,
                                    }),
                                  ),
                                );
                              }}
                              onCollapsedChange={(collapsed) =>
                                setProjectCollectionCollapsed(
                                  createLocalProjectCollectionCollapseKey(
                                    item.collection.collectionId,
                                  ),
                                  collapsed,
                                )
                              }
                              onDelete={() => {
                                setProjectCollections((previous) =>
                                  removeSidebarProjectCollection(
                                    previous,
                                    item.collection.collectionId,
                                  ),
                                );
                              }}
                              onHide={() => {
                                const collectionKey = `local:${item.collection.collectionId}`;
                                setHiddenCollectionKeys((current) =>
                                  current.includes(collectionKey)
                                    ? current.filter((key) => key !== collectionKey)
                                    : [...current, collectionKey],
                                );
                              }}
                              onSelectSessions={setSelectedSidebarSessionIds}
                              sessionIds={item.groupIds.flatMap(
                                (groupId) => effectiveSessionIdsByGroup[ groupId ] ?? [],
                              )}
                              sessionTagListItems={sidebarSessionTagListItems}
                              sessionsById={sessionsById}
                              vscode={vscode}
                            >
                              {item.groupIds.map(renderReferenceProjectGroup)}
                              </ProjectCollectionSection>
                            ),
                          )}
                          <ProjectListEndUngroupDropZone
                            active={
                              projectUngroupDropIndicatorScopeId ===
                              LOCAL_PROJECT_LIST_SCOPE_ID
                            }
                            scopeId={LOCAL_PROJECT_LIST_SCOPE_ID}
                          />
                        </>
                      ) : (
                        referenceProjectsEmptyState
                      )}
                    </div>
                  ) : null}
                  {!shouldHideReferenceSectionsForSearchEmptyState && remoteMachines.length > 0 ? (
                    <div className="reference-remote-section-list">
                      {/*
	                     * CDXC:RemoteMachines 2026-06-02-23:47:
	                     * Saved Remote machines render as peer sidebar sections beside local Projects. Until the SSH/gxserver connection is active, each machine remains visible and exposes Reload instead of Add Project.
	                     *
	                     * CDXC:RemoteMachines 2026-06-09-19:02:
	                     * Remote machine section rows must collapse like Quick and Projects and use the same section-header styling, including the visible chevron and hover actions.
	                     */}
                      {remoteMachines.map((machine, index) => {
                        /*
                         * CDXC:RemoteProjectCollections 2026-07-21:
                         * Remote machine sections render the same collection
                         * panels as local Projects. Assigning a remote project
                         * to a group previously updated state with no visible
                         * result because remote lists were always flat.
                         */
                        const machineProjectGroupIds =
                          remoteProjectGroupIdsByMachineId[ machine.id ] ?? [];
                        const hasExpandedMachineProjects = machineProjectGroupIds.some(
                          (groupId) => collapsedGroupsById[groupId] !== true,
                        );
                        const machineProjectCollections =
                          remoteProjectCollectionsByMachineId[machine.id] ?? {
                            collections: [],
                            nextCollectionNumber: 1,
                          };
                        const machineCollectionIdByProjectId =
                          createProjectCollectionIdByProjectId(
                            machineProjectCollections,
                            machineProjectGroupIds,
                            groupsById,
                            (groupId) => groupsById[groupId]?.remoteMachineContext?.projectId,
                          );
                        const machineCollectionItems = enableProjectCollections
                          ? buildProjectCollectionRenderItems(
                              machineProjectGroupIds,
                              machineProjectCollections,
                              (groupId) =>
                                groupsById[groupId]?.remoteMachineContext?.projectId,
                            )
                          : undefined;
                        const renderRemoteProjectGroup = (groupId: string, groupIndex: number) => (
                          <SessionGroupSection
                            autoEdit={false}
                            canClose={!groupsById[ groupId ]?.projectContext}
                            completionFlashNonceBySessionId={completionFlashNonceBySessionId}
                            draggingDisabled={isSessionSearchOpen}
                            groupDropIndicator={groupDropIndicator}
                            groupId={groupId}
                            index={groupIndex}
                            isCollapsed={isSidebarSearchProjectGroupRenderedCollapsed(groupId)}
                            isHidden={hiddenGroupIds.includes(groupId)}
                            isGroupDragPreviewSource={groupDragPreview?.groupId === groupId}
                            key={groupId}
                            onAutoEditHandled={() => undefined}
                            onCollapsedChange={setGroupCollapsed}
                            onCreateProjectCollection={
                              enableProjectCollections
                                ? (projectId) =>
                                    createRemoteProjectCollectionForProject(
                                      machine.id,
                                      projectId,
                                      machineProjectGroupIds,
                                    )
                                : undefined
                            }
                            onFocusRequested={() => undefined}
                            onMoveProjectToCollection={
                              enableProjectCollections
                                ? (projectId, collectionId) =>
                                    moveRemoteProjectToCollection(
                                      machine.id,
                                      projectId,
                                      collectionId,
                                      machineProjectGroupIds,
                                    )
                                : undefined
                            }
                            onProjectSessionListCollapsedChange={setProjectSessionListCollapsed}
                            onHideGroup={() =>
                              setHiddenGroupIds((current) =>
                                current.includes(groupId)
                                  ? current.filter((id) => id !== groupId)
                                  : [...current, groupId],
                              )
                            }
                            onSessionSelectionChange={handleSidebarSessionSelectionChange}
                            orderedSessionIds={displayedWorkspaceSessionIdsByGroup[ groupId ] ?? []}
                            enableProjectSessionListToggle={!isSessionSearchFiltering}
                            projectHeaderActions="all"
                            projectSessionListCollapsedState={collapsedProjectSessionListsById}
                            projectCollectionId={
                              groupsById[groupId]?.remoteMachineContext?.projectId
                                ? machineCollectionIdByProjectId.get(
                                    groupsById[groupId]!.remoteMachineContext!.projectId!,
                                  )
                                : undefined
                            }
                            projectCollectionOptions={
                              enableProjectCollections
                                ? machineProjectCollections.collections
                                : undefined
                            }
                            sessionDraggingDisabled={true}
                            sessionTagListItems={sidebarSessionTagListItems}
                            selectedSessionIds={selectedSidebarSessionIds}
                            showHeaderActions={true}
                            showSessionDropPositionIndicators={false}
                            useColoredAgentIcons={effectiveSettings.useColoredSessionAgentIcons}
                            vscode={vscode}
                          />
                        );
                        return (
                        <RemoteMachineSidebarSection
                          activeSessionsSortMode={activeSessionsSortMode}
                          bulkActionLabel={
                            machineProjectGroupIds.length > 0
                              ? hasExpandedMachineProjects
                                ? "Collapse All"
                                : "Expand Previous"
                              : undefined
                          }
                          collapsed={
                            !isSessionSearchFiltering &&
                            collapsedRemoteMachineSectionsById[ machine.id ] === true
                          }
                          containsActiveSession={[...groupIdsContainingActiveSession].some(
                            (groupId) =>
                              groupsById[groupId]?.remoteMachineContext?.machineId === machine.id,
                          )}
                          index={index}
                          key={machine.id}
                          machine={machine}
                          isDragPreviewSource={
                            remoteMachineDragPreview?.machineId === machine.id
                          }
                          remoteMachineDropIndicatorPosition={
                            remoteMachineDropIndicator?.remoteMachineId === machine.id
                              ? remoteMachineDropIndicator.position
                              : undefined
                          }
                          onAddProject={() => openAddProjectModal(machine.id)}
                          onBulkProjectToggle={
                            machineProjectGroupIds.length > 0
                              ? () => {
                                  setRemoteMachineSectionCollapsed(machine.id, false);
                                  if (hasExpandedMachineProjects) {
                                    previousExpandedRemoteProjectGroupIdsByMachineIdRef.current[
                                      machine.id
                                    ] = machineProjectGroupIds.filter(
                                      (groupId) => collapsedGroupsById[groupId] !== true,
                                    );
                                    setGroupsCollapsed(machineProjectGroupIds, true);
                                    return;
                                  }
                                  const previousExpandedProjectGroupIds =
                                    previousExpandedRemoteProjectGroupIdsByMachineIdRef.current[
                                      machine.id
                                    ]?.filter((groupId) =>
                                      machineProjectGroupIds.includes(groupId),
                                    ) ?? [];
                                  setGroupsCollapsed(
                                    previousExpandedProjectGroupIds.length > 0
                                      ? previousExpandedProjectGroupIds
                                      : machineProjectGroupIds,
                                    false,
                                  );
                                }
                              : undefined
                          }
                          onEdit={() => {
                            dismissAppModalForSidebarNavigation("SettingsDismissal:remoteEditSettings");
                            openAppModal({
                              initialRemoteMachineId: machine.id,
                              initialTab: "remote",
                              modal: "settings",
                              type: "open",
                            });
                          }}
                          onReconnect={() => {
                            dismissAppModalForSidebarNavigation("SettingsDismissal:remoteReconnect");
                            vscode.postMessage({
                              remoteMachineId: machine.id,
                              type: "reconnectRemoteMachine",
                            });
                          }}
                          onSetActiveSessionsSortMode={setActiveSessionsSortMode}
                          onSetSidebarV2Layout={setSidebarV2Layout}
                          onSetSidebarVersion={setSidebarVersion}
                          onToggleSessionTagFilter={toggleSessionTagFilter}
                          projectCollectionItems={machineCollectionItems}
                          projectUngroupDropIndicatorScopeId={
                            projectUngroupDropIndicatorScopeId
                          }
                          projectGroupIds={machineProjectGroupIds}
                          renderProjectCollection={(item, itemIndex) => (
                            <ProjectCollectionSection
                              autoEdit={
                                autoEditingProjectCollectionId ===
                                `${machine.id}:${item.collection.collectionId}`
                              }
                              bulkProjectActionLabel={
                                item.groupIds.some(
                                  (groupId) => collapsedGroupsById[groupId] !== true,
                                )
                                  ? "Collapse All"
                                  : "Expand Previous"
                              }
                              collapsed={
                                !isSessionSearchFiltering &&
                                collapsedProjectCollectionsByKey[
                                  createRemoteProjectCollectionCollapseKey(
                                    machine.id,
                                    item.collection.collectionId,
                                  )
                                ] === true
                              }
                              collection={item.collection}
                              containsActiveSession={item.groupIds.some((groupId) =>
                                groupIdsContainingActiveSession.has(groupId),
                              )}
                              draggingDisabled={true}
                              index={itemIndex}
                              isHidden={hiddenCollectionKeys.includes(
                                `remote:${machine.id}:${item.collection.collectionId}`,
                              )}
                              key={`${machine.id}:${item.collection.collectionId}`}
                              onAutoEditHandled={() => setAutoEditingProjectCollectionId(undefined)}
                              onBulkProjectToggle={() => {
                                const bulkToggleKey = `${machine.id}:${item.collection.collectionId}`;
                                const hasExpandedProjects = item.groupIds.some(
                                  (groupId) => collapsedGroupsById[groupId] !== true,
                                );
                                if (hasExpandedProjects) {
                                  previousExpandedProjectGroupIdsByCollectionIdRef.current[
                                    bulkToggleKey
                                  ] = item.groupIds.filter(
                                    (groupId) => collapsedGroupsById[groupId] !== true,
                                  );
                                  setGroupsCollapsed(item.groupIds, true);
                                  return;
                                }

                                const previousExpandedProjectGroupIds =
                                  previousExpandedProjectGroupIdsByCollectionIdRef.current[
                                    bulkToggleKey
                                  ]?.filter((groupId) => item.groupIds.includes(groupId)) ?? [];
                                setGroupsCollapsed(
                                  previousExpandedProjectGroupIds.length > 0
                                    ? previousExpandedProjectGroupIds
                                    : item.groupIds,
                                  false,
                                );
                              }}
                              onChange={(updated) => {
                                updateRemoteProjectCollections(machine.id, (previous) =>
                                  updateSidebarProjectCollection(
                                    previous,
                                    updated.collectionId,
                                    (existing) => ({
                                      ...existing,
                                      color: updated.color,
                                      title: updated.title,
                                    }),
                                  ),
                                );
                              }}
                              onCollapsedChange={(collapsed) =>
                                setProjectCollectionCollapsed(
                                  createRemoteProjectCollectionCollapseKey(
                                    machine.id,
                                    item.collection.collectionId,
                                  ),
                                  collapsed,
                                )
                              }
                              onDelete={() => {
                                updateRemoteProjectCollections(machine.id, (previous) =>
                                  removeSidebarProjectCollection(
                                    previous,
                                    item.collection.collectionId,
                                  ),
                                );
                              }}
                              onHide={() => {
                                const collectionKey = `remote:${machine.id}:${item.collection.collectionId}`;
                                setHiddenCollectionKeys((current) =>
                                  current.includes(collectionKey)
                                    ? current.filter((key) => key !== collectionKey)
                                    : [...current, collectionKey],
                                );
                              }}
                              onSelectSessions={setSelectedSidebarSessionIds}
                              sessionIds={item.groupIds.flatMap(
                                (groupId) => effectiveSessionIdsByGroup[ groupId ] ?? [],
                              )}
                              sessionTagListItems={sidebarSessionTagListItems}
                              sessionsById={sessionsById}
                              sortableId={`remote-project-collection:${machine.id}:${item.collection.collectionId}`}
                              vscode={vscode}
                            >
                              {item.groupIds.map((groupId) =>
                                renderRemoteProjectGroup(
                                  groupId,
                                  machineProjectGroupIds.indexOf(groupId),
                                ),
                              )}
                            </ProjectCollectionSection>
                          )}
                          renderProjectGroup={renderRemoteProjectGroup}
                          selectedSessionTagFilters={activeSelectedSessionTagFilters}
                          sessionSummary={remoteSectionSessionSummariesByMachineId[machine.id]}
                          sessionTagListItems={sidebarSessionTagListItems}
                          sidebarV2Layout={sidebarV2Layout}
                          sidebarVersion={sidebarVersion}
                          onToggleCollapsed={() => {
                            const nextCollapsed =
                              collapsedRemoteMachineSectionsById[ machine.id ] !== true;
                            if (!nextCollapsed) {
                              triggerReferenceSectionChildAnimation("remote");
                            }
                            setRemoteMachineSectionCollapsed(machine.id, nextCollapsed);
                          }}
                          status={remoteMachineRuntimeStatuses[ machine.id ] ?? "disconnected"}
                          statusMessage={remoteMachineStatusMessages[ machine.id ]}
                        />
                        );
                      })}
                    </div>
                  ) : null}
                {previousSessionsSearchGroup}
                {shouldShowSessionSearchEmptyState ? (
                  <div
                    className="group-empty-drop-target session-search-empty-drop-target"
                    data-empty-space-blocking="true"
                  >
                    <div className="group-empty-state session-search-empty-state">
                      No current sessions or sessions to reopen match that search.
                    </div>
                  </div>
                ) : displayedWorkspaceGroupIds.every(
                  (groupId) => (displayedWorkspaceSessionIdsByGroup[ groupId ] ?? []).length === 0,
                ) &&
                  !isSessionSearchOpen ? (
                  <div className="empty" data-empty-space-blocking="true"></div>
                ) : null}
                </>
                )}
                {/*
                  * CDXC:ProjectDragPreview 2026-07-02-21:10:
                  * The ghost must live inside the .sidebar-reference-layout
                  * scope, or the reference project-header title rules do not
                  * match and the ghost renders with the base uppercase
                  * section-label styling. The layout root is display:contents,
                  * so the fixed-position ghost still anchors to the viewport.
                  *
                  * CDXC:SidebarV2GroupedProjectUX 2026-07-30:
                  * Hoisted out of the V1 branch with the provider. Grouped V2
                  * project rows drag with `feedback: "none"` exactly as V1's do,
                  * so this cursor ghost is the ONLY thing that follows the
                  * pointer during a V2 project reorder — leaving it behind in the
                  * V1 branch would have made a V2 drag invisible.
                  */}
                {groupDragPreview && referenceLayoutElement
                  ? createPortal(
                    <ProjectGroupDragGhost preview={groupDragPreview} />,
                    referenceLayoutElement,
                  )
                  : null}
                {projectCollectionDragPreview && referenceLayoutElement
                  ? createPortal(
                    <ProjectCollectionDragGhost preview={projectCollectionDragPreview} />,
                    referenceLayoutElement,
                  )
                  : null}
                {remoteMachineDragPreview && referenceLayoutElement
                  ? createPortal(
                    <RemoteMachineDragGhost preview={remoteMachineDragPreview} />,
                    referenceLayoutElement,
                  )
                  : null}
                </DragDropProvider>
              </div>
            </div>
          </section>
          <GitCommitModal
            agents={agents}
            draft={
              gitCommitDraft ?? {
                confirmLabel: "Commit",
                description: "",
                changedFiles: [],
                requestId: "",
                showCommitMessage: true,
                suggestedBody: undefined,
                suggestedSubject: "",
              }
            }
            isOpen={gitCommitDraft !== undefined}
            fileDiffDraft={gitFileDiffDraft}
            onCancel={(requestId) => {
              closeGitCommitModal(requestId);
            }}
            onConfirm={(requestId, message, options) => {
              setGitCommitDraft(undefined);
              setGitFileDiffDraft(undefined);
              vscode.postMessage({
                agentId: options.agentId,
                commitOnNewRef: options.commitOnNewRef,
                deleteWorktreeAfter: options.deleteWorktreeAfter,
                filePaths: options.filePaths,
                message,
                requestId,
                type: "confirmSidebarGitCommit",
              });
            }}
            onDirectMerge={(requestId, message, options) => {
              setGitCommitDraft(undefined);
              setGitFileDiffDraft(undefined);
              vscode.postMessage({
                agentId: options.agentId,
                deleteWorktreeAfter: options.deleteWorktreeAfter,
                filePaths: options.filePaths,
                message,
                requestId,
                type: "confirmSidebarGitDirectMerge",
              });
            }}
            onMultipleCommits={(requestId, agentId) => {
              setGitCommitDraft(undefined);
              setGitFileDiffDraft(undefined);
              vscode.postMessage({ agentId, requestId, type: "runSidebarGitMultipleCommits" });
            }}
            onOpenFileDiff={(filePath, requestId) => {
              vscode.postMessage({ filePath, requestId, type: "openSidebarGitChangedFileDiff" });
            }}
            theme={theme}
          />
          {buildStamp ? (
            <AppTooltip content="Copy build stamp">
              <button
                aria-label={`Copy build stamp ${buildStamp}`}
                className="copy-cursor"
                onClick={() => {
                  void navigator.clipboard.writeText(buildStamp).catch(() => { });
                }}
                style={DEBUG_BUILD_STAMP_STYLE}
                type="button"
              >
                {buildStamp}
              </button>
            </AppTooltip>
          ) : null}
        </div>
        <SidebarReferenceFooter
          commandPaletteHotkey={formatSidebarMenuHotkeyLabel(
            normalizeghostexHotkeySettings(effectiveSettings.hotkeys).openCommandPalette,
          )}
          onOpenQuickAccess={openCommandPalette}
          onOpenSettings={openSidebarSettings}
        />
      </div>
      </SidebarCollapseAnimationProvider>
    </TooltipProvider>
  );
}

type SidebarReferencePrimaryMenuKind = "keepAwake" | "settings";

function SidebarReferenceTopChrome({
  keepAwakeRuntime,
  onOpenAgentsHub,
  onOpenAutomations,
  onOpenDiscord,
  onOpenHotkeys,
  onOpenMobile,
  onOpenPowerSettings,
  onOpenPreviousSessions,
  onRunKeepAwake,
  onSearchPreviousSessionsByPrompt,
  onSearch,
  onStopKeepAwake,
  onTogglePetOverlay,
  settings,
  showKeepAwakeButton,
}: {
  keepAwakeRuntime?: SidebarKeepAwakeRuntimeState;
  onOpenAgentsHub: () => void;
  onOpenAutomations: () => void;
  onOpenDiscord: () => void;
  onOpenHotkeys: () => void;
  onOpenMobile: () => void;
  onOpenPowerSettings: () => void;
  onOpenPreviousSessions: () => void;
  onRunKeepAwake: (durationMinutes: KeepAwakeDurationMinutes) => void;
  onSearchPreviousSessionsByPrompt: () => void;
  onSearch: () => void;
  onStopKeepAwake: () => void;
  onTogglePetOverlay: () => void;
  settings: ghostexSettings;
  showKeepAwakeButton: boolean;
}) {
  const topControlRowRef = useRef<HTMLDivElement>(null);
  const [ openMenu, setOpenMenu ] = useState<SidebarReferencePrimaryMenuKind>();
  const settingsMenuHotkeys = normalizeghostexHotkeySettings(settings.hotkeys);

  useEffect(() => {
    if (!openMenu) {
      return undefined;
    }

    const handleOutsidePointerDown = (event: PointerEvent) => {
      if (isNode(event.target) && topControlRowRef.current?.contains(event.target)) {
        return;
      }
      setOpenMenu(undefined);
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setOpenMenu(undefined);
      }
    };
    const handleWindowBlur = () => {
      setOpenMenu(undefined);
    };
    const unregisterNativeDismiss = registerSidebarContextMenuDismissHandler(() => {
      setOpenMenu(undefined);
    });

    document.addEventListener("pointerdown", handleOutsidePointerDown, true);
    document.addEventListener("keydown", handleKeyDown);
    window.addEventListener("blur", handleWindowBlur);
    return () => {
      unregisterNativeDismiss();
      document.removeEventListener("pointerdown", handleOutsidePointerDown, true);
      document.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("blur", handleWindowBlur);
    };
  }, [openMenu]);

  const toggleMoreMenu = () => {
    dismissSidebarTooltips();
    setOpenMenu((current) => (current ? undefined : "settings"));
  };

  const closeMenuAndRun = (action: () => void) => {
    dismissSidebarTooltips();
    setOpenMenu(undefined);
    action();
  };

  /**
   * CDXC:SidebarReference 2026-05-08-09:11
   * Combined mode should visually match the provided app sidebar: native-style
   * window dots, disabled back/forward chrome, and primary sidebar navigation.
   *
   * CDXC:TitlebarActions 2026-05-11-02:46
   * Actions moved out of the sidebar header into the native titlebar beside
   * Open In. Keep this top chrome focused on navigation/search so the action
   * menu has one home and one split-button UX.
   *
   * CDXC:AgentsHub 2026-05-12-09:59
   * Agents Hub should remain the first primary sidebar destination so agent
   * configuration content is reached before secondary reference surfaces.
   *
   * CDXC:Mobile 2026-06-16-00:45:
   * The primary sidebar needs a Mobile entry near other reference/setup
   * navigation. It should launch through the same fixed browser-chat path as
   * Plugins so mobile setup docs open outside the active code project.
   *
   * CDXC:Mobile 2026-06-16-01:23:
   * Mobile should open the Ghostex download page, not the GitHub README anchor,
   * because the product site now owns mobile download routing.
   *
   * CDXC:Automations 2026-06-29-15:55:
   * Automations should sit above Mobile in the primary sidebar and open the
   * gxserver-backed Automation page instead of the old coming-soon toast.
   *
   * CDXC:Automations 2026-06-30-11:05:
   * Sidebar Automations opens the Quick-level all-project page. Project-specific automation access moved to the titlebar Automate view so the sidebar shortcut does not hijack the active project's Kanban/Project surface.
   *
   * CDXC:Automations 2026-06-30-12:51:
   * The sidebar shortcut tooltip should use the full page name, Automations Overview, so users can distinguish it from the per-project Automate titlebar view.
   *
   * CDXC:SidebarReference 2026-06-16-01:23:
   * Plugins should no longer consume a primary sidebar row.
   *
   * CDXC:Plugins 2026-06-16-01:29:
   * Hide the Plugins sidebar affordance for now instead of keeping it as an
   * Agents Hub secondary action.
   *
   * CDXC:ExperimentalFeatures 2026-06-28-07:41:
   * Agents Hub is no longer gated by Enable Experimental Features. Keep it
   * visible as the first primary sidebar destination even when experimental
   * features are disabled.
   *
   * CDXC:SidebarReference 2026-06-28-15:04:
   * Agents Hub, Automations, and Mobile should be icon-only shortcuts sharing one full-width row at the top of the sidebar, with hover tooltips providing their labels. Search remains a separate full-width row below them.
   *
   * CDXC:SidebarTopChrome 2026-06-29-01:43:
   * Settings and Keep Awake moved out of the macOS titlebar into the same full-width sidebar shortcut row. They remain icon-only with hover tooltips, and normal clicks open local sidebar dropdowns instead of native titlebar child-window menus.
   *
   * CDXC:SidebarTopChrome 2026-06-29-03:39:
   * The overflow menu trigger should present itself as "More" in the sidebar
   * tooltip while the dropdown still contains the Settings destination.
   *
   * CDXC:SidebarTopChrome 2026-07-04-17:26:
   * The visible top chrome is now Search plus More. Agents Hub, Automations,
   * Mobile, Keep Awake, Search by Prompt, and Previous Sessions all live under
   * More so the sidebar only spends one row on primary navigation.
   *
   * CDXC:SidebarFooter 2026-08-07:
   * Settings now has one icon-only home in the fixed sidebar footer. Keep it
   * out of More so the same destination is not repeated in two places.
   */
  return (
    <header className="reference-sidebar-top">
      <div aria-hidden="true" className="reference-sidebar-window-row">
        <span className="reference-sidebar-window-dot" data-window-dot="close" />
        <span className="reference-sidebar-window-dot" data-window-dot="minimize" />
        <span className="reference-sidebar-window-dot" data-window-dot="zoom" />
        <IconLayoutSidebar className="reference-sidebar-window-icon" size={16} stroke={1.9} />
        <IconArrowLeft className="reference-sidebar-window-icon" size={17} stroke={1.9} />
        <IconArrowRight className="reference-sidebar-window-icon" size={17} stroke={1.9} />
      </div>
      <nav aria-label="Sidebar primary navigation" className="reference-sidebar-primary-nav">
        <div
          aria-label="Sidebar search and menu"
          className="reference-sidebar-search-more-row"
          ref={topControlRowRef}
          role="group"
        >
          <SidebarReferenceSearchNavItem
            onSearch={onSearch}
            shortcut={formatSidebarMenuHotkeyLabel(settingsMenuHotkeys.openSessionSearchPalette)}
          />
          <div className="reference-sidebar-primary-menu-cell">
            <SidebarReferenceShortcutButton
              ariaExpanded={Boolean(openMenu)}
              ariaHaspopup="menu"
              icon={IconMenu2}
              label="More"
              menuOpen={Boolean(openMenu)}
              onClick={toggleMoreMenu}
            />
            {openMenu === "settings" ? (
              <SidebarReferenceSettingsDropdown
                keepAwakeRuntime={keepAwakeRuntime}
                hotkeys={settingsMenuHotkeys}
                onOpenAgentsHub={() => closeMenuAndRun(onOpenAgentsHub)}
                onOpenAutomations={() => closeMenuAndRun(onOpenAutomations)}
                onOpenDiscord={() => closeMenuAndRun(onOpenDiscord)}
                onOpenHotkeys={() => closeMenuAndRun(onOpenHotkeys)}
                onOpenKeepAwakeMenu={() => {
                  dismissSidebarTooltips();
                  setOpenMenu("keepAwake");
                }}
                onOpenMobile={() => closeMenuAndRun(onOpenMobile)}
                onOpenPreviousSessions={() => closeMenuAndRun(onOpenPreviousSessions)}
                onSearchPreviousSessionsByPrompt={() =>
                  closeMenuAndRun(onSearchPreviousSessionsByPrompt)
                }
                onTogglePetOverlay={() => closeMenuAndRun(onTogglePetOverlay)}
                showKeepAwakeButton={showKeepAwakeButton}
              />
            ) : null}
            {openMenu === "keepAwake" ? (
              <SidebarReferenceKeepAwakeDropdown
                activeDuration={keepAwakeRuntime?.durationMinutes}
                isRunning={Boolean(keepAwakeRuntime)}
                onBack={() => {
                  dismissSidebarTooltips();
                  setOpenMenu("settings");
                }}
                onOpenPowerSettings={() => closeMenuAndRun(onOpenPowerSettings)}
                onStartKeepAwake={(durationMinutes) =>
                  closeMenuAndRun(() => onRunKeepAwake(durationMinutes))
                }
                onStopKeepAwake={() => closeMenuAndRun(onStopKeepAwake)}
              />
            ) : null}
          </div>
        </div>
      </nav>
    </header>
  );
}

function SidebarReferenceSearchNavItem({
  onSearch,
  shortcut,
}: {
  onSearch: () => void;
  shortcut?: string;
}) {
  return (
    <div className="reference-sidebar-search-slot" data-active="false">
      <div className="reference-sidebar-nav-item">
        <Button
          className="reference-sidebar-nav-button"
          onClick={onSearch}
          size="sm"
          type="button"
          variant="ghost"
        >
          <IconSearch
            aria-hidden="true"
            className="reference-sidebar-nav-icon reference-sidebar-search-icon"
            size={15}
            stroke={1.8}
          />
          <span className="reference-sidebar-nav-label">Search</span>
          {shortcut ? <kbd className="reference-sidebar-nav-shortcut">{shortcut}</kbd> : null}
        </Button>
      </div>
    </div>
  );
}

function SidebarReferenceNavButton({
  icon: Icon,
  iconOnly = false,
  label,
  onClick,
}: {
  icon: TablerIcon;
  iconOnly?: boolean;
  label: string;
  onClick: () => void;
}) {
  const className = iconOnly
    ? "reference-sidebar-nav-button reference-sidebar-nav-icon-button reference-sidebar-hover-action-tooltip"
    : "reference-sidebar-nav-button";

  if (iconOnly) {
    return (
      <SidebarFixedTooltipButton
        aria-label={label}
        className={className}
        onClick={onClick}
        tooltip={label}
        type="button"
      >
        <Icon
          aria-hidden="true"
          className="reference-sidebar-nav-icon"
          data-icon="inline-start"
          size={15}
          stroke={1.9}
        />
      </SidebarFixedTooltipButton>
    );
  }

  return (
    <Button
      className={className}
      onClick={onClick}
      size="sm"
      type="button"
      variant="ghost"
    >
      <Icon
        aria-hidden="true"
        className="reference-sidebar-nav-icon"
        data-icon="inline-start"
        size={15}
        stroke={1.9}
      />
      <span className="reference-sidebar-nav-label">{label}</span>
    </Button>
  );
}

function SidebarReferenceShortcutButton({
  active = false,
  ariaExpanded,
  ariaHaspopup,
  icon: Icon,
  label,
  menuOpen = false,
  onClick,
  stableBackground = false,
}: {
  active?: boolean;
  ariaExpanded?: boolean;
  ariaHaspopup?: "menu";
  icon: TablerIcon;
  label: string;
  menuOpen?: boolean;
  onClick: () => void;
  stableBackground?: boolean;
}) {
  return (
    <SidebarFixedTooltipButton
      aria-expanded={ariaExpanded}
      aria-haspopup={ariaHaspopup}
      aria-label={label}
      className="reference-sidebar-nav-button reference-sidebar-nav-icon-button reference-sidebar-hover-action-tooltip"
      data-active={String(active)}
      data-state={menuOpen ? "open" : undefined}
      data-stable-background={stableBackground ? "true" : undefined}
      onClick={onClick}
      tooltip={label}
      type="button"
    >
      <Icon
        aria-hidden="true"
        className="reference-sidebar-nav-icon"
        data-icon="inline-start"
        size={15}
        stroke={1.9}
      />
    </SidebarFixedTooltipButton>
  );
}

function SidebarReferenceSettingsDropdown({
  keepAwakeRuntime,
  hotkeys,
  onOpenAgentsHub,
  onOpenAutomations,
  onOpenDiscord,
  onOpenHotkeys,
  onOpenMobile,
  onOpenKeepAwakeMenu,
  onOpenPreviousSessions,
  onSearchPreviousSessionsByPrompt,
  onTogglePetOverlay,
  showKeepAwakeButton,
}: {
  keepAwakeRuntime?: SidebarKeepAwakeRuntimeState;
  hotkeys: ghostexHotkeySettings;
  onOpenAgentsHub: () => void;
  onOpenAutomations: () => void;
  onOpenDiscord: () => void;
  onOpenHotkeys: () => void;
  onOpenMobile: () => void;
  onOpenKeepAwakeMenu: () => void;
  onOpenPreviousSessions: () => void;
  onSearchPreviousSessionsByPrompt: () => void;
  onTogglePetOverlay: () => void;
  showKeepAwakeButton: boolean;
}) {
  return (
    <div className="reference-sidebar-primary-dropdown" role="menu">
      <SidebarReferencePrimaryMenuItem
        icon={IconHistoryToggle}
        label="Sessions"
        onSelect={onOpenPreviousSessions}
        shortcut={formatSidebarMenuHotkeyLabel(hotkeys.openSessionSearchPalette)}
      />
      <SidebarReferencePrimaryMenuItem
        icon={IconFileSearch}
        label="Search by Prompt"
        onSelect={onSearchPreviousSessionsByPrompt}
      />
      <SidebarReferencePrimaryMenuSeparator />
      <SidebarReferencePrimaryMenuItem
        icon={IconUsersGroup}
        label="Agents Hub"
        onSelect={onOpenAgentsHub}
      />
      <SidebarReferencePrimaryMenuItem
        icon={IconClock}
        label="Automations Overview"
        onSelect={onOpenAutomations}
      />
      <SidebarReferencePrimaryMenuSeparator />
      <SidebarReferencePrimaryMenuItem
        icon={IconDeviceMobile}
        label="Mobile"
        onSelect={onOpenMobile}
      />
      {PET_CONTROLS_VISIBLE ? (
        <SidebarReferencePrimaryMenuItem
          icon={IconRobotFace}
          label="Wake Pet"
          onSelect={onTogglePetOverlay}
        />
      ) : null}
      {showKeepAwakeButton ? (
        <>
          <SidebarReferencePrimaryMenuItem
            icon={keepAwakeRuntime ? IconCoffee : IconMoon}
            label="Keep awake"
            onSelect={onOpenKeepAwakeMenu}
            trailingIcon={IconChevronRight}
          />
        </>
      ) : null}
      <SidebarReferencePrimaryMenuItem
        icon={IconUsersGroup}
        label="Join Discord"
        onSelect={onOpenDiscord}
      />
      <SidebarReferencePrimaryMenuSeparator />
      <SidebarReferencePrimaryMenuItem
        icon={IconKeyboard}
        label="Hotkeys"
        onSelect={onOpenHotkeys}
        shortcut={formatSidebarMenuHotkeyLabel(hotkeys.openHotkeys)}
      />
    </div>
  );
}

function SidebarReferenceFooter({
  commandPaletteHotkey,
  onOpenQuickAccess,
  onOpenSettings,
}: {
  commandPaletteHotkey?: string;
  onOpenQuickAccess: () => void;
  onOpenSettings: () => void;
}) {
  return (
    <footer className="reference-sidebar-footer">
      <div className="reference-sidebar-search-slot reference-sidebar-footer-quick-access-slot">
        <div className="reference-sidebar-nav-item">
          <Button
            aria-label="Commands"
            className="reference-sidebar-nav-button reference-sidebar-footer-quick-access-button"
            onClick={onOpenQuickAccess}
            size="sm"
            type="button"
            variant="ghost"
          >
            <IconBolt
              aria-hidden="true"
              className="reference-sidebar-nav-icon reference-sidebar-search-icon"
              size={15}
              stroke={1.8}
            />
            <span className="reference-sidebar-nav-label">Commands</span>
            {commandPaletteHotkey ? (
              <kbd className="reference-sidebar-nav-shortcut">{commandPaletteHotkey}</kbd>
            ) : null}
          </Button>
        </div>
      </div>
      <div className="reference-sidebar-primary-menu-cell">
        <SidebarReferenceShortcutButton
          icon={IconSettings}
          label="Settings"
          onClick={onOpenSettings}
        />
      </div>
    </footer>
  );
}

function SidebarReferenceKeepAwakeDropdown({
  activeDuration,
  isRunning,
  onBack,
  onOpenPowerSettings,
  onStartKeepAwake,
  onStopKeepAwake,
}: {
  activeDuration?: KeepAwakeDurationMinutes;
  isRunning: boolean;
  onBack: () => void;
  onOpenPowerSettings: () => void;
  onStartKeepAwake: (durationMinutes: KeepAwakeDurationMinutes) => void;
  onStopKeepAwake: () => void;
}) {
  return (
    <div className="reference-sidebar-primary-dropdown" role="menu">
      <SidebarReferencePrimaryMenuItem
        icon={IconArrowLeft}
        label="More"
        onSelect={onBack}
      />
      <SidebarReferencePrimaryMenuSeparator />
      <div className="reference-sidebar-primary-menu-label">Keep awake period</div>
      {KEEP_AWAKE_DURATION_OPTIONS.map((option) => (
        <SidebarReferencePrimaryMenuItem
          active={activeDuration === option.value}
          icon={IconCoffee}
          key={option.value}
          label={getSidebarKeepAwakeMenuLabel(option.label)}
          onSelect={() => onStartKeepAwake(option.value)}
        />
      ))}
      {isRunning ? (
        <SidebarReferencePrimaryMenuItem
          icon={IconSquareMinus}
          label="Don't keep awake"
          onSelect={onStopKeepAwake}
        />
      ) : null}
      <SidebarReferencePrimaryMenuSeparator />
      <SidebarReferencePrimaryMenuItem
        icon={IconSettings}
        label="Power Settings"
        onSelect={onOpenPowerSettings}
      />
    </div>
  );
}

function SidebarReferencePrimaryMenuItem({
  active = false,
  icon: Icon,
  label,
  onSelect,
  shortcut,
  trailingIcon: TrailingIcon,
}: {
  active?: boolean;
  icon: TablerIcon;
  label: string;
  onSelect: () => void;
  shortcut?: string;
  trailingIcon?: TablerIcon;
}) {
  return (
    <button
      className="reference-sidebar-primary-menu-item"
      onClick={onSelect}
      role="menuitem"
      type="button"
    >
      <Icon aria-hidden="true" className="reference-sidebar-primary-menu-icon" size={16} stroke={1.8} />
      <span className="reference-sidebar-primary-menu-label-text">{label}</span>
      {shortcut ? (
        <span className="reference-sidebar-primary-menu-shortcut">{shortcut}</span>
      ) : null}
      {TrailingIcon ? (
        <TrailingIcon
          aria-hidden="true"
          className="reference-sidebar-primary-menu-trailing-icon"
          size={15}
          stroke={1.8}
        />
      ) : null}
      {active ? (
        <IconCheck aria-hidden="true" className="reference-sidebar-primary-menu-check" size={15} stroke={1.8} />
      ) : null}
    </button>
  );
}

function SidebarReferencePrimaryMenuSeparator() {
  return <div className="reference-sidebar-primary-menu-separator" role="separator" />;
}

function getSidebarKeepAwakeMenuLabel(label: string): string {
  return label === "Until turned off" ? label : `For ${label.toLowerCase()}`;
}

function formatSidebarMenuHotkeyLabel(hotkey: string | undefined): string | undefined {
  return hotkey ? formatSidebarHotkeyLabel(hotkey) : undefined;
}

function isNode(value: EventTarget | null): value is Node {
  return value instanceof Node;
}

function SidebarReferenceSectionHeader({
  activeSessionsSortMode,
  actionsAlwaysVisible,
  agents = [],
  bulkActionLabel,
  collapsed,
  containsActiveSession = false,
  dragHandleRef,
  onAddProject,
  onBulkProjectToggle,
  onConfigureAgents,
  onCreateBrowserChat,
  onCreateChat,
  onEdit,
  onFilterChats,
  onRunAgent,
  onSetActiveSessionsSortMode,
  onSetSidebarV2Layout,
  onSetSidebarVersion,
  onToggleShowHidden,
  onToggleSessionTagFilter,
  onToggleCollapsed,
  primaryAgentId,
  remoteConnectionControl,
  sectionKey,
  selectedSessionTagFilters = [],
  sessionSummary,
  sessionTagListItems,
  sidebarV2Layout = "flat",
  sidebarVersion = "v1",
  title,
  showHidden = false,
  useColoredAgentIcons = false,
}: {
  activeSessionsSortMode?: SidebarActiveSessionsSortMode;
  actionsAlwaysVisible?: boolean;
  agents?: readonly SidebarAgentButton[];
  bulkActionLabel?: string;
  collapsed: boolean;
  containsActiveSession?: boolean;
  dragHandleRef?: (element: Element | null) => void;
  onAddProject?: () => void;
  onBulkProjectToggle?: () => void;
  onConfigureAgents?: () => void;
  onCreateBrowserChat?: () => void;
  onCreateChat?: () => void;
  onEdit?: () => void;
  onFilterChats?: () => void;
  onRunAgent?: (agent: SidebarAgentButton) => void;
  onSetActiveSessionsSortMode?: (sortMode: SidebarActiveSessionsSortMode) => void;
  /*
   * CDXC:SidebarV2 2026-07-29:
   * The Sort & Filter menu is the in-sidebar entry point for the Inbox
   * sidebar. Both writers are optional so section headers rendered without a
   * settings pipeline (remote machine headers) simply omit the group.
   */
  onSetSidebarV2Layout?: (layout: SidebarV2Layout) => void;
  onSetSidebarVersion?: (sidebarVersion: SidebarVersion) => void;
  onToggleShowHidden?: () => void;
  onToggleSessionTagFilter?: (tag: SidebarSessionTagFilter) => void;
  onToggleCollapsed: () => void;
  primaryAgentId?: string;
  remoteConnectionControl?: RemoteMachineHeaderConnectionControl;
  sectionKey: ReferenceSidebarSectionId;
  selectedSessionTagFilters?: readonly SidebarSessionTagFilter[];
  sessionSummary?: SidebarSectionSessionSummary;
  sessionTagListItems?: readonly SidebarSessionTagListItem[];
  sidebarV2Layout?: SidebarV2Layout;
  sidebarVersion?: SidebarVersion;
  title: string;
  showHidden?: boolean;
  useColoredAgentIcons?: boolean;
}) {
  /**
   * CDXC:SidebarReference 2026-05-08-01:41
   * Reference-mode Chats and Projects are collapsible section headers. Chats
   * exposes browser-chat and new-chat controls on hover, while Projects expose
   * add-project and expand/collapse-all controls on hover so the compact
   * Codex.app-style list keeps management actions nearby. Add Project owns both
   * folder selection and repository cloning through its source picker.
   *
   * CDXC:SidebarReference 2026-05-08-02:21
   * The project bulk control is one stateful text button: "Collapse All" while
   * any project is expanded, then "Expand Previous" after it collapses the
   * previously expanded projects.
   *
   * CDXC:SidebarReference 2026-05-08-02:56
   * The bulk project button stays icon-only in the visible UI: use
   * IconArrowsDiagonal2 for Collapse All and IconArrowsDiagonalMinimize for
   * Expand Previous, while preserving the text labels for tooltips and
   * accessibility.
   *
   * CDXC:Tooltips 2026-05-20-10:05:
   * Quick and Projects section-header actions use the same local left-side
   * tooltip treatment as the reference-sidebar hover icons because portaled
   * Radix tooltips mis-anchor in the native sidebar webview. Quick exposes
   * filter, browser, terminal, and agent-picker actions beside the section label.
   *
   * CDXC:SidebarStickyHeaders 2026-05-20-09:55:
   * Section headers need a stable section key in the DOM so spacing can be
   * tuned for Projects and Quick independently without depending on visible
   * label text or adjacent markup shape.
   *
   * CDXC:ManualSessionSorting 2026-06-05-12:30:
   * Quick and Projects expose the same filter-shaped sort control in their
   * section headers. Last Active Sorting remains the default, while Manual
   * Sorting preserves the first visible last-active snapshot and later
   * user-defined row order.
   *
   * CDXC:QuickAgents 2026-06-08-18:25:
   * Quick exposes the same selected-agent split picker as project headers, with
   * Browser and Terminal as separate section-header actions to its left. Keep
   * the agent picker at the far right of the Quick header cluster so it aligns
   * with project-header agent placement. The main agent half launches the
   * selected provider and the chevron opens the shared agent list plus Configure.
   *
   * CDXC:RemoteMachines 2026-06-10-09:54:
   * Remote machine headers keep Edit in the hover action cluster so users can
   * jump to that machine's Settings -> Remote fields, while the always-visible
   * connection-state control remains beside the machine title.
   *
   * CDXC:SidebarSortFilter 2026-06-15-21:24:
   * The section-header filter icon should use the stable hover label "Sort & Filter" even when the accessible label continues to expose the current sort mode and selected tag-filter count.
   */
  const [ sortMenuPosition, setSortMenuPosition ] = useState<HeaderSortMenuPosition>();
  const [ agentMenuPosition, setAgentMenuPosition ] = useState<HeaderSortMenuPosition>();
  const BulkProjectIcon =
    bulkActionLabel === "Collapse All" ? IconArrowsDiagonalMinimize : IconArrowsDiagonal2;
  const SectionIcon =
    sectionKey === "remote"
      ? IconCloud
      : sectionKey === "projects" && title === "Projects"
        ? IconFolders
        : undefined;
  const remoteConnectionError =
    remoteConnectionControl?.kind === "error" ? remoteConnectionControl : undefined;
  const remoteConnectionBusy =
    remoteConnectionControl?.kind === "busy" ? remoteConnectionControl : undefined;
  const leadingRemoteConnectionControl = remoteConnectionError ?? remoteConnectionBusy;
  const trailingRemoteConnectionControl = leadingRemoteConnectionControl
    ? undefined
    : remoteConnectionControl;
  const primaryAgent = agents.find((agent) => agent.agentId === primaryAgentId) ?? agents[ 0 ];
  const primaryAgentLabel = primaryAgent?.name ?? "Agent";
  const primaryAgentIconColorMode = useColoredAgentIcons ? "brand" : "monochrome";
  const normalizedSessionTagListItems = useMemo(
    () => normalizeSidebarSessionTagListItems(sessionTagListItems),
    [ sessionTagListItems ],
  );
  const hasTagFilters = selectedSessionTagFilters.length > 0;
  const hasActions =
    onAddProject ||
    onBulkProjectToggle ||
    onConfigureAgents ||
    onCreateBrowserChat ||
    onCreateChat ||
    onEdit ||
    onFilterChats ||
    onRunAgent ||
    onSetActiveSessionsSortMode ||
    onToggleShowHidden ||
    onToggleSessionTagFilter;
  /*
   * CDXC:SidebarV2 2026-07-29:
   * Session sorting is a V1-only concept: the Inbox is position-stable by
   * construction and ignores the sort mode entirely. So while V2 is active the
   * whole sort radio group disappears from this menu, and the trigger's
   * accessible name states the active sidebar instead of advertising a sort
   * order that does nothing.
   */
  const isSidebarV2Active = sidebarVersion === "v2";
  const sortModeLabel =
    activeSessionsSortMode === "manual" ? "Manual Sorting" : "Last Active Sorting";
  const showSortModeOptions = onSetActiveSessionsSortMode !== undefined && !isSidebarV2Active;
  const filterModeLabel = isSidebarV2Active ? "Inbox sidebar" : sortModeLabel;
  const filterLabel = hasTagFilters
    ? `${filterModeLabel}, ${selectedSessionTagFilters.length} tag filter${selectedSessionTagFilters.length === 1 ? "" : "s"
    }`
    : filterModeLabel;
  const hasActionStatus =
    (sessionSummary?.workingCount ?? 0) > 0 ||
    (sessionSummary?.attentionCount ?? 0) > 0;
  const shouldShowCollapsedStatus =
    collapsed &&
    sessionSummary !== undefined &&
    (hasActionStatus || sessionSummary.awakeCount > 0);

  const openSortMenu = (event: ReactMouseEvent<HTMLButtonElement>) => {
    const bounds = event.currentTarget.getBoundingClientRect();
    setAgentMenuPosition(undefined);
    setSortMenuPosition({
      left: bounds.left,
      top: bounds.bottom + 4,
    });
  };

  const openAgentMenu = (event: ReactMouseEvent<HTMLButtonElement>) => {
    const bounds = event.currentTarget.getBoundingClientRect();
    setSortMenuPosition(undefined);
    setAgentMenuPosition({
      left: bounds.right - REFERENCE_SECTION_AGENT_MENU_WIDTH_PX,
      top: bounds.bottom + 6,
    });
  };

  const selectSortMode = (sortMode: SidebarActiveSessionsSortMode) => {
    setSortMenuPosition(undefined);
    onSetActiveSessionsSortMode?.(sortMode);
  };

  const selectSidebarVersion = (nextSidebarVersion: SidebarVersion) => {
    setSortMenuPosition(undefined);
    onSetSidebarVersion?.(nextSidebarVersion);
  };

  const selectSidebarV2Layout = (nextLayout: SidebarV2Layout) => {
    setSortMenuPosition(undefined);
    onSetSidebarV2Layout?.(nextLayout);
  };

  const runAgent = (agent: SidebarAgentButton | undefined) => {
    setAgentMenuPosition(undefined);
    if (!agent) {
      onConfigureAgents?.();
      return;
    }
    onRunAgent?.(agent);
  };

  return (
    <div
      className="reference-sidebar-section-row"
      data-actions-always-visible={String(actionsAlwaysVisible === true)}
      data-collapsed={String(collapsed)}
      data-contains-active-session={String(containsActiveSession)}
      data-has-remote-connection-control={String(
        trailingRemoteConnectionControl !== undefined,
      )}
      data-reference-section={sectionKey}
    >
      {remoteConnectionError ? (
        <SidebarFixedTooltipButton
          aria-label={remoteConnectionError.label}
          className="reference-remote-machine-error-cloud"
          onClick={remoteConnectionError.onClick}
          tooltip={remoteConnectionError.label}
          tooltipSide="top"
          type="button"
        >
          <IconCloud aria-hidden="true" size={15} stroke={1.8} />
        </SidebarFixedTooltipButton>
      ) : remoteConnectionBusy ? (
        <SidebarFixedTooltipButton
          aria-busy="true"
          aria-disabled="true"
          aria-label={remoteConnectionBusy.label}
          className="reference-remote-machine-busy-indicator"
          tooltip={remoteConnectionBusy.label}
          tooltipSide="top"
          type="button"
        >
          <IconLoader2 aria-hidden="true" size={13} stroke={1.8} />
        </SidebarFixedTooltipButton>
      ) : null}
      <button
        aria-expanded={!collapsed}
        className="reference-sidebar-section-heading"
        onClick={onToggleCollapsed}
        ref={dragHandleRef}
        type="button"
      >
        {SectionIcon && !leadingRemoteConnectionControl ? (
          <SectionIcon
            aria-hidden="true"
            className="reference-sidebar-section-icon"
            size={15}
            stroke={1.8}
          />
        ) : null}
        <span className="reference-sidebar-section-title">{title}</span>
        {remoteConnectionControl ? null : (
          <IconCaretRightFilled
            aria-hidden="true"
            className="reference-sidebar-section-chevron"
            size={13}
          />
        )}
      </button>
      {trailingRemoteConnectionControl ? (
        <SidebarFixedTooltipButton
          aria-label={trailingRemoteConnectionControl.label}
          className="reference-remote-machine-connection-control"
          data-kind={trailingRemoteConnectionControl.kind}
          onClick={trailingRemoteConnectionControl.onClick}
          tooltip={trailingRemoteConnectionControl.label}
          tooltipSide="top"
          type="button"
        >
          <IconPlugConnected aria-hidden="true" size={14} stroke={1.9} />
        </SidebarFixedTooltipButton>
      ) : null}
      {shouldShowCollapsedStatus && sessionSummary ? (
        <div
          aria-label={[
            sessionSummary.workingCount > 0
              ? `${sessionSummary.workingCount} working`
              : "",
            sessionSummary.attentionCount > 0
              ? `${sessionSummary.attentionCount} done`
              : "",
            !hasActionStatus && sessionSummary.awakeCount > 0
              ? `${sessionSummary.awakeCount} awake terminals and browsers`
              : "",
          ]
            .filter(Boolean)
            .join(", ")}
          className="group-collapsed-status-counts reference-sidebar-section-status-counts"
        >
          {sessionSummary.workingCount > 0 ? (
            <span className="group-collapsed-status-count" data-activity="working">
              {sessionSummary.workingCount}
            </span>
          ) : null}
          {sessionSummary.attentionCount > 0 ? (
            <span className="group-collapsed-status-count" data-activity="attention">
              {sessionSummary.attentionCount}
            </span>
          ) : null}
          {!hasActionStatus && sessionSummary.awakeCount > 0 ? (
            <span className="group-collapsed-status-count" data-activity="awake">
              {sessionSummary.awakeCount}
            </span>
          ) : null}
        </div>
      ) : null}
      {hasActions ? (
        <div className="reference-sidebar-section-actions">
          {onSetActiveSessionsSortMode || onToggleSessionTagFilter ? (
            <SidebarFixedTooltipButton
              aria-expanded={sortMenuPosition !== undefined}
              aria-haspopup="menu"
              aria-label={`Filter sessions: ${filterLabel}`}
              className="reference-sidebar-section-action reference-sidebar-section-sort-action reference-sidebar-hover-action-tooltip"
              data-selected={String(
                (activeSessionsSortMode === "manual" && !isSidebarV2Active) ||
                  hasTagFilters ||
                  showHidden,
              )}
              onClick={openSortMenu}
              tooltip="Sort & Filter"
              tooltipAlign="end"
              type="button"
            >
              <IconFilter2 aria-hidden="true" size={14} stroke={1.9} />
            </SidebarFixedTooltipButton>
          ) : null}
          {onCreateBrowserChat ? (
            <SidebarFixedTooltipButton
              aria-label="Quick Browser Tab"
              className="reference-sidebar-section-action reference-sidebar-hover-action-tooltip"
              onClick={onCreateBrowserChat}
              tooltip="Quick Browser Tab"
              tooltipAlign="end"
              type="button"
            >
              <IconWorld aria-hidden="true" size={15} stroke={1.9} />
            </SidebarFixedTooltipButton>
          ) : null}
          {onCreateChat ? (
            <SidebarFixedTooltipButton
              aria-label="Quick Terminal"
              className="reference-sidebar-section-action reference-sidebar-hover-action-tooltip"
              onClick={onCreateChat}
              tooltip="Quick Terminal"
              tooltipAlign="end"
              type="button"
            >
              <IconTerminal2 aria-hidden="true" size={14} stroke={2} />
            </SidebarFixedTooltipButton>
          ) : null}
          {onRunAgent || onConfigureAgents ? (
            <div
              className="group-agent-split-button reference-sidebar-section-agent-picker"
              data-open={String(agentMenuPosition !== undefined)}
            >
              <SidebarFixedTooltipButton
                aria-label={`Create ${primaryAgentLabel}`}
                className="group-agent-main-button reference-sidebar-hover-action-tooltip"
                onClick={() => runAgent(primaryAgent)}
                tooltip={`Create ${primaryAgentLabel}`}
                tooltipAlign="end"
                type="button"
              >
                <ProjectAgentLauncherIcon
                  agent={primaryAgent}
                  colorMode={primaryAgentIconColorMode}
                />
              </SidebarFixedTooltipButton>
              <SidebarFixedTooltipButton
                aria-expanded={agentMenuPosition !== undefined}
                aria-haspopup="menu"
                aria-label="Select agent"
                className="group-agent-toggle-button reference-sidebar-hover-action-tooltip"
                data-open={String(agentMenuPosition !== undefined)}
                onClick={openAgentMenu}
                tooltip="Select Agent"
                tooltipAlign="end"
                type="button"
              >
                <IconChevronDown aria-hidden="true" size={13} stroke={2} />
              </SidebarFixedTooltipButton>
            </div>
          ) : null}
          {onBulkProjectToggle && bulkActionLabel ? (
            <SidebarFixedTooltipButton
              aria-label={bulkActionLabel}
              className="reference-sidebar-section-action reference-sidebar-section-bulk-project-action reference-sidebar-hover-action-tooltip"
              onClick={onBulkProjectToggle}
              tooltip={bulkActionLabel}
              tooltipAlign="end"
              type="button"
            >
              <BulkProjectIcon aria-hidden="true" size={14} stroke={1.9} />
            </SidebarFixedTooltipButton>
          ) : null}
          {onEdit ? (
            <SidebarFixedTooltipButton
              aria-label={`Edit ${title}`}
              className="reference-sidebar-section-action reference-sidebar-hover-action-tooltip"
              onClick={onEdit}
              tooltip="Edit"
              tooltipAlign="end"
              type="button"
            >
              <IconEdit aria-hidden="true" size={14} stroke={1.9} />
            </SidebarFixedTooltipButton>
          ) : null}
          {onAddProject ? (
            <SidebarFixedTooltipButton
              aria-label="Add project"
              className="reference-sidebar-section-action reference-sidebar-hover-action-tooltip"
              onClick={onAddProject}
              tooltip="Add project"
              tooltipAlign="end"
              type="button"
            >
              <IconPlus aria-hidden="true" size={14} stroke={2} />
            </SidebarFixedTooltipButton>
          ) : null}
        </div>
      ) : null}
      {sortMenuPosition ? (
        <SidebarContextMenuPortal
          menuClassName="session-context-menu reference-sidebar-sort-menu"
          menuStyle={{
            left: sortMenuPosition.left,
            top: sortMenuPosition.top,
          }}
          onDismiss={() => setSortMenuPosition(undefined)}
        >
          {onToggleShowHidden ? (
            <>
              <button
                aria-checked={showHidden}
                className="session-context-menu-item"
                onClick={onToggleShowHidden}
                role="menuitemcheckbox"
                type="button"
              >
                <IconCheck
                  aria-hidden="true"
                  className="session-context-menu-icon"
                  data-visible={String(showHidden)}
                  size={14}
                  stroke={2}
                />
                Show hidden
              </button>
              <div className="session-context-menu-divider" role="separator" />
            </>
          ) : null}
          {/*
            * CDXC:SidebarV2 2026-07-29:
            * The sidebar picker sits above the sort radios because it chooses
            * which sidebar renders at all. Manual Sorting is a V1-only concept,
            * so it disappears while the Inbox sidebar is active instead of
            * offering an order the inbox intentionally ignores.
            */}
          {onSetSidebarVersion ? (
            <>
              <button
                aria-checked={sidebarVersion !== "v2"}
                className="session-context-menu-item"
                onClick={() => selectSidebarVersion("v1")}
                role="menuitemradio"
                type="button"
              >
                <IconCheck
                  aria-hidden="true"
                  className="session-context-menu-icon"
                  data-visible={String(sidebarVersion !== "v2")}
                  size={14}
                  stroke={2}
                />
                Classic sidebar
              </button>
              <button
                aria-checked={sidebarVersion === "v2"}
                className="session-context-menu-item"
                onClick={() => selectSidebarVersion("v2")}
                role="menuitemradio"
                type="button"
              >
                <IconCheck
                  aria-hidden="true"
                  className="session-context-menu-icon"
                  data-visible={String(sidebarVersion === "v2")}
                  size={14}
                  stroke={2}
                />
                Inbox sidebar (New)
              </button>
              {sidebarVersion === "v2" && onSetSidebarV2Layout ? (
                <button
                  aria-checked={sidebarV2Layout === "byProject"}
                  className="session-context-menu-item"
                  onClick={() =>
                    selectSidebarV2Layout(sidebarV2Layout === "byProject" ? "flat" : "byProject")
                  }
                  role="menuitemcheckbox"
                  type="button"
                >
                  <IconCheck
                    aria-hidden="true"
                    className="session-context-menu-icon"
                    data-visible={String(sidebarV2Layout === "byProject")}
                    size={14}
                    stroke={2}
                  />
                  Group by Project
                </button>
              ) : null}
              {showSortModeOptions || onToggleSessionTagFilter ? (
                <div className="session-context-menu-divider" role="separator" />
              ) : null}
            </>
          ) : null}
          {showSortModeOptions ? (
            <>
              <button
                aria-checked={activeSessionsSortMode !== "manual"}
                className="session-context-menu-item"
                onClick={() => selectSortMode("lastActivity")}
                role="menuitemradio"
                type="button"
              >
                <IconCheck
                  aria-hidden="true"
                  className="session-context-menu-icon"
                  data-visible={String(activeSessionsSortMode !== "manual")}
                  size={14}
                  stroke={2}
                />
                Last Active Sorting
              </button>
              <button
                aria-checked={activeSessionsSortMode === "manual"}
                className="session-context-menu-item"
                onClick={() => selectSortMode("manual")}
                role="menuitemradio"
                type="button"
              >
                <IconCheck
                  aria-hidden="true"
                  className="session-context-menu-icon"
                  data-visible={String(activeSessionsSortMode === "manual")}
                  size={14}
                  stroke={2}
                />
                Manual Sorting
              </button>
            </>
          ) : null}
          {showSortModeOptions && onToggleSessionTagFilter ? (
            <div className="session-context-menu-divider" role="separator" />
          ) : null}
          {onToggleSessionTagFilter
            ? normalizedSessionTagListItems.map((item) => {
              if (!item.visible) {
                return null;
              }
              if (item.type === "separator") {
                return item.enabled ? (
                  <div className="session-context-menu-divider" key={item.id} role="separator" />
                ) : null;
              }

              const filter = getSidebarSessionTagListItemFilter(item);
              if (!filter) {
                return null;
              }
              const isSelected = selectedSessionTagFilters.includes(filter);
              return (
                <button
                  aria-checked={isSelected}
                  className="session-context-menu-item reference-sidebar-tag-filter-item"
                  data-selected={String(isSelected)}
                  disabled={!item.enabled}
                  key={item.id}
                  onClick={() => onToggleSessionTagFilter(filter)}
                  role="menuitemcheckbox"
                  type="button"
                >
                  <SessionTagIcon
                    className="session-context-menu-icon session-tag-colored-icon"
                    fillFavorite
                    size={14}
                    stroke={1.8}
                    tag={filter}
                  />
                  {getSidebarSessionTagLabel(filter)}
                  <IconCheck
                    aria-hidden="true"
                    className="session-context-menu-trailing-icon reference-sidebar-tag-filter-check"
                    data-visible={String(isSelected)}
                    size={14}
                    stroke={2}
                  />
                </button>
              );
            })
            : null}
        </SidebarContextMenuPortal>
      ) : null}
      {agentMenuPosition ? (
        <SidebarContextMenuPortal
          menuClassName="session-context-menu group-agent-menu reference-sidebar-agent-menu"
          menuStyle={{
            left: `${agentMenuPosition.left}px`,
            top: `${agentMenuPosition.top}px`,
            width: `${REFERENCE_SECTION_AGENT_MENU_WIDTH_PX}px`,
          }}
          onDismiss={() => setAgentMenuPosition(undefined)}
        >
          {agents.map((agent) => (
            <button
              aria-label={agent.name}
              aria-pressed={primaryAgent?.agentId === agent.agentId}
              className="session-context-menu-item group-control-menu-item group-agent-menu-item"
              data-selected={String(primaryAgent?.agentId === agent.agentId)}
              key={agent.agentId}
              onClick={() => runAgent(agent)}
              role="menuitem"
              type="button"
            >
              <ProjectAgentLauncherIcon agent={agent} colorMode="brand" />
              <span className="group-agent-menu-label">{agent.name}</span>
              <AgentMenuChatIndicator agent={agent} />
            </button>
          ))}
          {agents.length > 0 ? (
            <div className="session-context-menu-divider" role="separator" />
          ) : null}
          <button
            className="session-context-menu-item group-control-menu-item group-agent-menu-item"
            onClick={() => {
              setAgentMenuPosition(undefined);
              onConfigureAgents?.();
            }}
            role="menuitem"
            type="button"
          >
            <IconSettings aria-hidden="true" className="session-context-menu-icon" size={14} />
            <span className="group-agent-menu-label">Configure</span>
          </button>
        </SidebarContextMenuPortal>
      ) : null}
    </div>
  );
}

function remoteMachineBusyLabel(
  status: RemoteMachineRuntimeStatus[ "state" ],
): string | undefined {
  switch (status) {
    case "connecting":
      return "Connecting…";
    case "installing":
      return "Installing gxserver…";
    case "downloadingRemoteServerPackage":
      return "Downloading server package…";
    default:
      return undefined;
  }
}

function remoteMachineFailureLabel(status: RemoteMachineRuntimeStatus[ "state" ]): string {
  switch (status) {
    case "installApprovalRequired":
      return "Install approval required.";
    case "installFailed":
      return "gxserver install failed.";
    case "invalid":
      return "Saved remote machine is incomplete.";
    case "keychainFailed":
      return "Could not save the auth token to Keychain.";
    case "presentationStreamFailed":
    case "presentationSubscribeFailed":
      return "Remote session stream failed.";
    case "sshFailed":
      return "SSH connection failed.";
    case "tokenUnavailable":
      return "Remote auth token unavailable.";
    case "tunnelFailed":
      return "Secure tunnel failed.";
    case "unsupported":
    case "unsupportedRemotePlatform":
      return "Remote platform not supported.";
    default:
      return "Remote connect failed.";
  }
}

function RemoteMachineSidebarSection({
  activeSessionsSortMode,
  bulkActionLabel,
  collapsed,
  containsActiveSession,
  index,
  isDragPreviewSource,
  machine,
  onAddProject,
  onBulkProjectToggle,
  onEdit,
  onReconnect,
  onSetActiveSessionsSortMode,
  onSetSidebarV2Layout,
  onSetSidebarVersion,
  onToggleSessionTagFilter,
  onToggleCollapsed,
  projectCollectionItems,
  projectUngroupDropIndicatorScopeId,
  projectGroupIds,
  remoteMachineDropIndicatorPosition,
  renderProjectCollection,
  renderProjectGroup,
  selectedSessionTagFilters,
  sessionSummary,
  sessionTagListItems,
  sidebarV2Layout,
  sidebarVersion,
  status,
  statusMessage,
}: {
  activeSessionsSortMode: SidebarActiveSessionsSortMode;
  bulkActionLabel?: string;
  collapsed: boolean;
  containsActiveSession: boolean;
  index: number;
  isDragPreviewSource: boolean;
  machine: RemoteMachineSettings;
  onAddProject: () => void;
  onBulkProjectToggle?: () => void;
  onEdit: () => void;
  onReconnect: () => void;
  onSetActiveSessionsSortMode: (sortMode: SidebarActiveSessionsSortMode) => void;
  onSetSidebarV2Layout: (layout: SidebarV2Layout) => void;
  onSetSidebarVersion: (sidebarVersion: SidebarVersion) => void;
  onToggleSessionTagFilter: (tag: SidebarSessionTagFilter) => void;
  onToggleCollapsed: () => void;
  projectCollectionItems?: readonly SidebarProjectCollectionRenderItem[];
  projectUngroupDropIndicatorScopeId?: string;
  projectGroupIds: readonly string[];
  remoteMachineDropIndicatorPosition?: "before" | "after";
  renderProjectCollection?: (
    item: Extract<SidebarProjectCollectionRenderItem, { kind: "collection" }>,
    itemIndex: number,
  ) => ReactNode;
  renderProjectGroup: (groupId: string, groupIndex: number) => ReactNode;
  selectedSessionTagFilters: readonly SidebarSessionTagFilter[];
  sessionSummary?: SidebarSectionSessionSummary;
  sessionTagListItems: readonly SidebarSessionTagListItem[];
  sidebarV2Layout: SidebarV2Layout;
  sidebarVersion: SidebarVersion;
  status: RemoteMachineRuntimeStatus[ "state" ];
  statusMessage?: string;
}) {
  const isConnected = status === "connected";
  const busyLabel = remoteMachineBusyLabel(status);
  const isBusy = busyLabel !== undefined;
  /*
   * CDXC:GPUIRemoteConnectFeedback 2026-07-21:
   * Only connected remote machines keep the collapsible chevron. Every other
   * state replaces it with an always-visible header control: Connect while
   * disconnected, a spinner during connect/install/download, or an error
   * button whose tooltip carries the native host's sanitized failure reason.
   * Native owns the matching viewport-level toast for progress and failures.
   */
  const isFailure = !isConnected && !isBusy && status !== "disconnected";
  const remoteConnectionControl: RemoteMachineHeaderConnectionControl | undefined = isConnected
    ? undefined
    : isBusy
      ? {
          kind: "busy",
          label: busyLabel,
        }
      : isFailure
        ? {
            kind: "error",
            label: `Error: ${statusMessage ?? remoteMachineFailureLabel(status)}`,
            onClick: onReconnect,
          }
        : {
            kind: "connect",
            label: "Connect",
            onClick: onReconnect,
          };
  /*
   * CDXC:GPUIRemoteLastSeen 2026-07-12:
   * Disconnected machines keep listing their last-seen project groups faded
   * (the runtime marks those groups stale) instead of hiding the body, so
   * "No projects" is a connected-only empty state.
   */
  const showProjectList = isConnected || projectGroupIds.length > 0;
  const projectListScopeId = createRemoteProjectListScopeId(machine.id);
  const projectListPresence = useSidebarCollapsiblePresence(collapsed);
  const sortable = useSortable({
    accept: "remote-machine",
    data: createRemoteMachineDragData(machine.id),
    feedback: "none",
    id: `remote-machine:${machine.id}`,
    index,
    sensors: remoteMachineSensors,
    type: "remote-machine",
  });

  return (
    <div
      className="reference-remote-machine-section"
      data-disconnected={String(!isConnected)}
      data-dragging={String(Boolean(sortable.isDragging || isDragPreviewSource))}
      data-remote-machine-drop-position={remoteMachineDropIndicatorPosition}
      data-sidebar-remote-machine-id={machine.id}
      ref={sortable.ref}
    >
      <SidebarReferenceSectionHeader
        activeSessionsSortMode={activeSessionsSortMode}
        actionsAlwaysVisible={false}
        bulkActionLabel={bulkActionLabel}
        collapsed={collapsed}
        containsActiveSession={containsActiveSession}
        dragHandleRef={sortable.handleRef}
        onAddProject={isConnected ? onAddProject : undefined}
        onBulkProjectToggle={onBulkProjectToggle}
        onEdit={onEdit}
        onSetActiveSessionsSortMode={onSetActiveSessionsSortMode}
        onSetSidebarV2Layout={onSetSidebarV2Layout}
        onSetSidebarVersion={onSetSidebarVersion}
        onToggleSessionTagFilter={onToggleSessionTagFilter}
        onToggleCollapsed={onToggleCollapsed}
        remoteConnectionControl={remoteConnectionControl}
        sectionKey="remote"
        selectedSessionTagFilters={selectedSessionTagFilters}
        sessionSummary={sessionSummary}
        sessionTagListItems={sessionTagListItems}
        sidebarV2Layout={sidebarV2Layout}
        sidebarVersion={sidebarVersion}
        title={machine.name}
      />
      {showProjectList && projectListPresence.isPresent ? (
        <div
          aria-hidden={projectListPresence.isVisuallyCollapsed}
          className="group-list workspace-group-list reference-project-group-list reference-sidebar-collapsible-body"
          data-animate-children="false"
          data-collapsed={String(projectListPresence.isVisuallyCollapsed)}
          inert={projectListPresence.isVisuallyCollapsed ? true : undefined}
          ref={projectListPresence.setCollapsibleElement}
          data-sidebar-project-list-scope={projectListScopeId}
          data-sidebar-remote-project-list="true"
          data-stale={String(!isConnected)}
        >
          {projectGroupIds.length > 0 ? (
            <>
              {projectCollectionItems && renderProjectCollection
                ? projectCollectionItems.map((item, itemIndex) =>
                  item.kind === "project"
                    ? renderProjectGroup(item.groupId, projectGroupIds.indexOf(item.groupId))
                    : renderProjectCollection(item, itemIndex),
                )
                : projectGroupIds.map((groupId, groupIndex) =>
                  renderProjectGroup(groupId, groupIndex),
                )}
              <ProjectListEndUngroupDropZone
                active={projectUngroupDropIndicatorScopeId === projectListScopeId}
                scopeId={projectListScopeId}
              />
            </>
          ) : (
            <div className="reference-sidebar-empty-state">No projects</div>
          )}
        </div>
      ) : null}
    </div>
  );
}

function createWorkspaceSessionIdsByGroup(
  workspaceGroupIds: readonly string[],
  sessionIdsByGroup: SessionIdsByGroup,
): SessionIdsByGroup {
  return Object.fromEntries(
    workspaceGroupIds.map((groupId) => [ groupId, sessionIdsByGroup[ groupId ] ?? [] ]),
  );
}

function findSessionGroupId(
  sessionIdsByGroup: SessionIdsByGroup,
  sessionId: string,
): string | undefined {
  return Object.entries(sessionIdsByGroup).find(([ , sessionIds ]) =>
    sessionIds.includes(sessionId),
  )?.[ 0 ];
}

function summarizeSidebarWakeScrollOrderState({
  activeSessionsSortMode,
  displayedWorkspaceGroupIds,
  displayedWorkspaceSessionIdsByGroup,
  focusedSessionId,
  groupsById,
  revision,
  sessionsById,
}: {
  activeSessionsSortMode: SidebarActiveSessionsSortMode;
  displayedWorkspaceGroupIds: readonly string[];
  displayedWorkspaceSessionIdsByGroup: SessionIdsByGroup;
  focusedSessionId: string;
  groupsById: SidebarGroupsById;
  revision: number;
  sessionsById: SidebarSessionsById;
}): Record<string, unknown> {
  const groupId = findSessionGroupId(displayedWorkspaceSessionIdsByGroup, focusedSessionId);
  const groupSessionIds = groupId ? displayedWorkspaceSessionIdsByGroup[ groupId ] ?? [] : [];
  const groupIndex = groupId ? displayedWorkspaceGroupIds.indexOf(groupId) : -1;
  const targetIndexInGroup = groupSessionIds.indexOf(focusedSessionId);
  const group = groupId ? groupsById[ groupId ] : undefined;
  const session = sessionsById[ focusedSessionId ];
  return {
    activeSessionsSortMode,
    displayedGroupCount: displayedWorkspaceGroupIds.length,
    firstSessionIdInGroup: groupSessionIds[ 0 ],
    focusedSessionId,
    groupId,
    groupIndex,
    groupIsChatCollection: group?.isChatCollection === true,
    groupIsProject: Boolean(group?.projectContext),
    groupIsRemote: Boolean(group?.remoteMachineContext),
    groupSessionCount: groupSessionIds.length,
    lastSessionIdInGroup: groupSessionIds.at(-1),
    revision,
    sessionActivity: session?.activity,
    sessionIsFocused: session?.isFocused,
    sessionIsLive: session?.isLive,
    sessionIsPinned: session?.isPinned,
    sessionIsSleeping: session?.isSleeping,
    sessionIsVisible: session?.isVisible,
    sessionKind: session?.sessionKind ?? session?.kind,
    sessionLastInteractionAt: session?.lastInteractionAt,
    sessionLifecycleState: session?.lifecycleState,
    sessionNativePaneState: session?.nativePaneState,
    sessionProviderSessionState: session?.providerSessionState,
    targetIndexInGroup,
    targetWindowSessionIds: createSidebarWakeScrollSessionIdWindow(
      groupSessionIds,
      targetIndexInGroup,
    ),
  };
}

function summarizeSidebarWakeScrollRenderedSlots(
  root: ParentNode,
  focusedSessionId: string,
): Record<string, unknown> {
  const slots = readRenderedSidebarSessionSlots(root);
  const renderedSessionIds = slots.map((slot) => slot.sessionId);
  const renderedIndex = renderedSessionIds.indexOf(focusedSessionId);
  return {
    renderedAwakeSlotCount: slots.filter((slot) => !slot.isSleeping).length,
    renderedFirstSessionId: renderedSessionIds[ 0 ],
    renderedIndex,
    renderedLastSessionId: renderedSessionIds.at(-1),
    renderedSleepingSlotCount: slots.filter((slot) => slot.isSleeping).length,
    renderedSlotCount: slots.length,
    renderedWindowSessionIds: createSidebarWakeScrollSessionIdWindow(
      renderedSessionIds,
      renderedIndex,
    ),
  };
}

function summarizeSidebarWakeScrollGeometry(
  focusedSessionElement: HTMLElement,
  scrollViewport: HTMLElement,
): Record<string, unknown> {
  const rowBounds = focusedSessionElement.getBoundingClientRect();
  const viewportBounds = scrollViewport.getBoundingClientRect();
  return {
    clientHeight: roundSidebarWakeScrollMetric(scrollViewport.clientHeight),
    isAboveViewport: rowBounds.top < viewportBounds.top,
    isBelowViewport: rowBounds.bottom > viewportBounds.bottom,
    isOutsideViewport: rowBounds.top < viewportBounds.top || rowBounds.bottom > viewportBounds.bottom,
    rowBottomRelativeToViewport: roundSidebarWakeScrollMetric(rowBounds.bottom - viewportBounds.top),
    rowHeight: roundSidebarWakeScrollMetric(rowBounds.height),
    rowTopRelativeToViewport: roundSidebarWakeScrollMetric(rowBounds.top - viewportBounds.top),
    scrollHeight: roundSidebarWakeScrollMetric(scrollViewport.scrollHeight),
    scrollTop: roundSidebarWakeScrollMetric(scrollViewport.scrollTop),
    viewportHeight: roundSidebarWakeScrollMetric(viewportBounds.height),
  };
}

function createSidebarWakeScrollSessionIdWindow(
  sessionIds: readonly string[],
  targetIndex: number,
  radius = 3,
): string[] {
  if (targetIndex < 0) {
    return [];
  }
  return sessionIds.slice(
    Math.max(0, targetIndex - radius),
    Math.min(sessionIds.length, targetIndex + radius + 1),
  );
}

function roundSidebarWakeScrollMetric(value: number): number {
  return Math.round(value * 100) / 100;
}

function haveSameSessionOrder(left: readonly string[], right: readonly string[]): boolean {
  if (left.length !== right.length) {
    return false;
  }

  return left.every((sessionId, index) => sessionId === right[ index ]);
}

function haveSameSessionSet(left: readonly string[], right: readonly string[]): boolean {
  if (left.length !== right.length) {
    return false;
  }

  const rightIds = new Set(right);
  return left.every((sessionId) => rightIds.has(sessionId));
}

function createPinnedFirstSessionOrder(
  previousSessionIds: readonly string[],
  pinnedSessionIds: readonly string[],
  sessionsById: Record<string, { isPinned?: boolean; } | undefined>,
): string[] {
  const pinnedSessionIdSet = new Set(pinnedSessionIds);
  const unpinnedSessionIds = previousSessionIds.filter(
    (sessionId) => sessionsById[ sessionId ]?.isPinned !== true,
  );

  return [
    ...pinnedSessionIds.filter((sessionId) => pinnedSessionIdSet.has(sessionId)),
    ...unpinnedSessionIds,
  ];
}

function movePinnedSessionIdsByDropTarget(
  previousPinnedSessionIds: readonly string[],
  sourceSessionId: string,
  target: SidebarSessionDropTarget,
): string[] {
  if (target.kind !== "session") {
    return [ ...previousPinnedSessionIds ];
  }

  return (
    moveSessionIdsByDropTarget(
      {
        [ target.groupId ]: [ ...previousPinnedSessionIds ],
      },
      sourceSessionId,
      target,
    )[ target.groupId ] ?? [ ...previousPinnedSessionIds ]
  );
}

function createPinnedSessionDropTargetLogKey(
  sourceData: Extract<ReturnType<typeof getSidebarDropData>, { kind: "session"; }>,
  target: SidebarSessionDropTarget | undefined,
): string {
  if (!target) {
    return `${sourceData.groupId}:${sourceData.sessionId}:none`;
  }

  if (target.kind === "group") {
    return `${sourceData.groupId}:${sourceData.sessionId}:${target.groupId}:group:${target.position}`;
  }

  return `${sourceData.groupId}:${sourceData.sessionId}:${target.groupId}:${target.sessionId}:${target.position}`;
}

function createPinnedSessionReorderDebugState(
  sourceData: Extract<ReturnType<typeof getSidebarDropData>, { kind: "session"; }>,
  currentSessionIdsByGroup: SessionIdsByGroup,
  effectiveSessionIdsByGroup: SessionIdsByGroup,
  authoritativeSessionIdsByGroup: SessionIdsByGroup,
  sessionsById: Record<
    string,
    { isPinned?: boolean; sessionId?: string; } | undefined
  >,
): Record<string, unknown> {
  const currentSessionIds = currentSessionIdsByGroup[ sourceData.groupId ] ?? [];
  const effectiveSessionIds = effectiveSessionIdsByGroup[ sourceData.groupId ] ?? [];
  const authoritativeSessionIds = authoritativeSessionIdsByGroup[ sourceData.groupId ] ?? [];
  const currentPinnedSessionIds = currentSessionIds.filter(
    (sessionId) => sessionsById[ sessionId ]?.isPinned === true,
  );
  const effectivePinnedSessionIds = effectiveSessionIds.filter(
    (sessionId) => sessionsById[ sessionId ]?.isPinned === true,
  );

  return {
    authoritativeSessionIds,
    currentPinnedSessionIds,
    currentSessionIds,
    effectivePinnedSessionIds,
    effectiveSessionIds,
    pinnedCount: currentPinnedSessionIds.length,
    sourceCurrentIndex: currentSessionIds.indexOf(sourceData.sessionId),
    sourceCurrentPinnedIndex: currentPinnedSessionIds.indexOf(sourceData.sessionId),
    sourceEffectiveIndex: effectiveSessionIds.indexOf(sourceData.sessionId),
    sourceEffectivePinnedIndex: effectivePinnedSessionIds.indexOf(sourceData.sessionId),
    sourceIsPinned: sessionsById[ sourceData.sessionId ]?.isPinned === true,
  };
}

function summarizePointerEventForPinnedReorder(event: PointerEvent): Record<string, unknown> {
  return {
    button: event.button,
    buttons: event.buttons,
    clientX: event.clientX,
    clientY: event.clientY,
    isPrimary: event.isPrimary,
    pointerType: event.pointerType,
  };
}

function createPinnedSessionDomDebugState(
  groupId: string,
  sessionId: string,
): Record<string, unknown> {
  const groupElement = getSidebarGroupElementById(groupId);
  const sessionElement = getTargetSessionElement(sessionId, undefined);
  const frameElement = sessionElement?.closest<HTMLElement>(".session-frame");

  return {
    group: {
      collapsed: groupElement?.dataset.collapsed,
      dragging: groupElement?.dataset.dragging,
      found: Boolean(groupElement),
      rect: summarizeElementRectForPinnedReorder(groupElement),
    },
    session: {
      dragging: sessionElement?.dataset.dragging,
      found: Boolean(sessionElement),
      frameFound: Boolean(frameElement),
      pinned: sessionElement?.dataset.pinned,
      rect: summarizeElementRectForPinnedReorder(sessionElement),
      visible: sessionElement?.dataset.visible,
    },
  };
}

function createPinnedSessionDropResolutionDebugState(
  nativeEvent: Event | undefined,
  sourceData: Extract<ReturnType<typeof getSidebarDropData>, { kind: "session"; }>,
  sessionIdsByGroup: SessionIdsByGroup,
  sessionsById: Record<string, { isPinned?: boolean; } | undefined>,
): Record<string, unknown> {
  const point = getClientPoint(nativeEvent);
  const groupElement = getSidebarGroupElementById(sourceData.groupId);
  const groupBounds = groupElement?.getBoundingClientRect();
  const groupSessionIds = sessionIdsByGroup[ sourceData.groupId ] ?? [];
  const pinnedSessionIds = groupSessionIds.filter(
    (sessionId) => sessionsById[ sessionId ]?.isPinned === true,
  );
  const targetMetrics = pinnedSessionIds
    .filter((sessionId) => sessionId !== sourceData.sessionId)
    .map((sessionId) => {
      const element = getTargetSessionElement(sessionId, point);
      const bounds = element?.getBoundingClientRect();
      return {
        elementFound: Boolean(element),
        height: bounds?.height,
        midpointY: bounds ? bounds.top + bounds.height / 2 : undefined,
        pinnedIndex: pinnedSessionIds.indexOf(sessionId),
        pointBeforeMidpoint:
          bounds && point ? point.y <= bounds.top + bounds.height / 2 : undefined,
        top: bounds?.top,
      };
    });
  const renderedPinnedBounds = targetMetrics
    .filter((metric) => metric.elementFound && metric.top !== undefined && metric.height !== undefined)
    .reduce<{ bottom: number; top: number; } | undefined>((bounds, metric) => {
      const top = metric.top!;
      const bottom = top + metric.height!;
      return bounds
        ? { bottom: Math.max(bounds.bottom, bottom), top: Math.min(bounds.top, top) }
        : { bottom, top };
    }, undefined);
  const pointInsideGroup =
    point !== undefined &&
    (groupBounds !== undefined
      ? point.y >= groupBounds.top && point.y <= groupBounds.bottom
      : renderedPinnedBounds !== undefined &&
        point.y >= renderedPinnedBounds.top &&
        point.y <= renderedPinnedBounds.bottom);

  return {
    groupElementFound: Boolean(groupElement),
    groupRect: summarizeElementRectForPinnedReorder(groupElement),
    groupSessionCount: groupSessionIds.length,
    hasPoint: Boolean(point),
    pinnedCount: pinnedSessionIds.length,
    point,
    pointInsideGroup,
    sourceInPinnedSet: pinnedSessionIds.includes(sourceData.sessionId),
    sourcePinnedIndex: pinnedSessionIds.indexOf(sourceData.sessionId),
    targetMetricCount: targetMetrics.filter((metric) => metric.elementFound).length,
    targetMetrics,
  };
}

function summarizeElementRectForPinnedReorder(
  element: Element | null | undefined,
): Record<string, number> | undefined {
  if (!element) {
    return undefined;
  }

  const bounds = element.getBoundingClientRect();
  return {
    bottom: bounds.bottom,
    height: bounds.height,
    top: bounds.top,
  };
}

function findCreatedGroupId(
  previousGroups: readonly string[],
  nextGroups: readonly string[],
): string | undefined {
  const previousGroupIds = new Set(previousGroups);
  return nextGroups.find((groupId) => !previousGroupIds.has(groupId));
}

function resolveSessionDropTargetFromPoint(
  nativeEvent: Event | undefined,
  sessionIdsByGroup: SessionIdsByGroup,
  targetData: ReturnType<typeof getSidebarDropData>,
  sourceData: Extract<ReturnType<typeof getSidebarDropData>, { kind: "session"; }> | undefined,
) {
  const point = getClientPoint(nativeEvent);
  /*
   * CDXC:SidebarDragDrop 2026-06-19-11:12:
   * Prefer current pointer hit testing over dnd-kit's reported target so the
   * insertion line follows the hovered row midpoint continuously, including
   * the exact center of a session row.
   */
  const candidates = [
    point ? getSidebarSessionDropTargetAtPoint(document, point.x, point.y) : undefined,
    getSidebarSessionDropTargetFromEvent(nativeEvent),
    getSidebarSessionDropTargetFromDropData(targetData, point),
    getSidebarSessionDropTarget(targetData),
  ];

  for (const rawCandidate of candidates) {
    if (!rawCandidate) {
      continue;
    }

    const candidate = canonicalizeSidebarSessionDropTarget(rawCandidate);
    const groupSessionIds = sessionIdsByGroup[ candidate.groupId ];
    if (!groupSessionIds) {
      continue;
    }

    if (candidate.kind === "session" && !groupSessionIds.includes(candidate.sessionId)) {
      continue;
    }

    /*
     * CDXC:SidebarDragDrop 2026-07-02-13:05:
     * When releasing here would keep the session exactly where it started,
     * suppress the insertion line entirely instead of falling through to a
     * different candidate, so no line is shown for a no-op drop.
     */
    if (
      isSourceSessionDropTarget(candidate, sourceData) ||
      (sourceData && isNoOpSessionDropTarget(sessionIdsByGroup, sourceData.sessionId, candidate))
    ) {
      return null;
    }

    return candidate;
  }

  return null;
}

function isNoOpSessionDropTarget(
  sessionIdsByGroup: SessionIdsByGroup,
  sessionId: string,
  target: SidebarSessionDropTarget,
): boolean {
  const nextSessionIdsByGroup = moveSessionIdsByDropTarget(sessionIdsByGroup, sessionId, target);
  if (nextSessionIdsByGroup === sessionIdsByGroup) {
    return true;
  }

  return Object.entries(nextSessionIdsByGroup).every(([ groupId, nextSessionIds ]) =>
    haveSameSessionOrder(sessionIdsByGroup[ groupId ] ?? [], nextSessionIds),
  );
}

function resolvePinnedSessionDropTargetFromPoint(
  nativeEvent: Event | undefined,
  sourceData: Extract<ReturnType<typeof getSidebarDropData>, { kind: "session"; }>,
  sessionIdsByGroup: SessionIdsByGroup,
  sessionsById: Record<string, { isPinned?: boolean; } | undefined>,
): SidebarSessionDropTarget | undefined {
  const point = getClientPoint(nativeEvent);
  if (!point) {
    return undefined;
  }

  const groupElement = getSidebarGroupElementById(sourceData.groupId);
  const groupBounds = groupElement?.getBoundingClientRect();
  const groupSessionIds = sessionIdsByGroup[ sourceData.groupId ] ?? [];
  const pinnedSessionIds = groupSessionIds.filter(
    (sessionId) => sessionsById[ sessionId ]?.isPinned === true,
  );
  if (pinnedSessionIds.length < 2 || !pinnedSessionIds.includes(sourceData.sessionId)) {
    return undefined;
  }

  const targetSessionMetrics = pinnedSessionIds
    .filter((sessionId) => sessionId !== sourceData.sessionId)
    .flatMap((sessionId) => {
      const element = getTargetSessionElement(sessionId, point);
      return element
        ? [
          {
            bounds: element.getBoundingClientRect(),
            sessionId,
          },
        ]
        : [];
    });
  if (targetSessionMetrics.length === 0) {
    return undefined;
  }
  const renderedPinnedTop = Math.min(...targetSessionMetrics.map((target) => target.bounds.top));
  const renderedPinnedBottom = Math.max(
    ...targetSessionMetrics.map((target) => target.bounds.bottom),
  );
  if (
    groupBounds
      ? point.y < groupBounds.top || point.y > groupBounds.bottom
      : point.y < renderedPinnedTop || point.y > renderedPinnedBottom
  ) {
    return undefined;
  }

  /*
   * CDXC:PinnedSessions 2026-05-28-14:29:
   * Pinned session drag feedback should be a stable insertion line within the
   * pinned partition. Base the active slot on pinned row midpoints only, not on
   * whichever full-project or unpinned-row droppable dnd-kit reports while the
   * pointer crosses row gaps.
   *
   * CDXC:SidebarDragDrop 2026-06-19-11:12:
   * The exact midpoint belongs to the lower half so a session row always shows
   * an insertion line: center/down is after, center/up is before.
   */
  const resolvedTarget = ((): SidebarSessionDropTarget => {
    for (const target of targetSessionMetrics) {
      if (point.y < target.bounds.top + target.bounds.height / 2) {
        return {
          groupId: sourceData.groupId,
          kind: "session",
          position: "before",
          sessionId: target.sessionId,
        };
      }
    }

    const lastTarget = targetSessionMetrics[ targetSessionMetrics.length - 1 ];
    return {
      groupId: sourceData.groupId,
      kind: "session",
      position: "after",
      sessionId: lastTarget.sessionId,
    };
  })();

  /*
   * CDXC:SidebarDragDrop 2026-07-02-13:05:
   * Pinned reorder also hides its insertion feedback when releasing would keep
   * the pinned row in its current slot.
   */
  return isNoOpSessionDropTarget(sessionIdsByGroup, sourceData.sessionId, resolvedTarget)
    ? undefined
    : resolvedTarget;
}

type SidebarRemoteMachineDropTarget = {
  position: "before" | "after";
  remoteMachineId: string;
};

type SidebarProjectCollectionDropTarget = {
  collectionId: string;
  position: "before" | "after";
};

function resolveRemoteMachineDropTargetFromPoint(
  nativeEvent: Event | undefined,
  remoteMachineIds: readonly string[],
  sourceRemoteMachineId: string,
  targetData: ReturnType<typeof getSidebarDropData>,
): SidebarRemoteMachineDropTarget | undefined {
  const point = getClientPoint(nativeEvent);
  const candidate = point
    ? getRemoteMachineBoundaryTargetAtY(remoteMachineIds, point.y)
    : targetData?.kind === "remote-machine" &&
      remoteMachineIds.includes(targetData.remoteMachineId)
      ? { remoteMachineId: targetData.remoteMachineId, position: "before" as const }
      : undefined;
  if (!candidate) {
    return undefined;
  }
  return moveRemoteMachineIdToDropTarget(
    remoteMachineIds,
    sourceRemoteMachineId,
    candidate,
  )
    ? candidate
    : undefined;
}

function getRemoteMachineBoundaryTargetAtY(
  remoteMachineIds: readonly string[],
  y: number,
): SidebarRemoteMachineDropTarget | undefined {
  const headerMidpoints = remoteMachineIds.flatMap((remoteMachineId) => {
    const section = document.querySelector<HTMLElement>(
      `[data-sidebar-remote-machine-id="${CSS.escape(remoteMachineId)}"]`,
    );
    const header = section?.querySelector<HTMLElement>(".reference-sidebar-section-row");
    if (!header) {
      return [];
    }
    const bounds = header.getBoundingClientRect();
    return bounds.height > 0
      ? [{ midpoint: bounds.top + bounds.height / 2, remoteMachineId }]
      : [];
  });
  if (headerMidpoints.length === 0) {
    return undefined;
  }
  for (const header of headerMidpoints) {
    if (y < header.midpoint) {
      return { remoteMachineId: header.remoteMachineId, position: "before" };
    }
  }
  return {
    remoteMachineId: headerMidpoints[headerMidpoints.length - 1].remoteMachineId,
    position: "after",
  };
}

function moveRemoteMachineIdToDropTarget(
  remoteMachineIds: readonly string[],
  sourceRemoteMachineId: string,
  target: SidebarRemoteMachineDropTarget,
): string[] | undefined {
  const withoutSource = remoteMachineIds.filter(
    (remoteMachineId) => remoteMachineId !== sourceRemoteMachineId,
  );
  if (withoutSource.length === remoteMachineIds.length) {
    return undefined;
  }
  const anchorIndex = withoutSource.indexOf(target.remoteMachineId);
  if (target.remoteMachineId === sourceRemoteMachineId || anchorIndex < 0) {
    return undefined;
  }
  const insertionIndex = target.position === "before" ? anchorIndex : anchorIndex + 1;
  const next = [...withoutSource];
  next.splice(insertionIndex, 0, sourceRemoteMachineId);
  return next.every((remoteMachineId, index) => remoteMachineId === remoteMachineIds[index])
    ? undefined
    : next;
}

function areSameRemoteMachineDropTarget(
  left: SidebarRemoteMachineDropTarget | undefined,
  right: SidebarRemoteMachineDropTarget | undefined,
): boolean {
  return left?.remoteMachineId === right?.remoteMachineId && left?.position === right?.position;
}

const LOCAL_PROJECT_LIST_SCOPE_ID = "local";

/*
 * CDXC:SidebarV2GroupedProjectUX 2026-07-30:
 * The key the LOCAL daemon's project order rides under in grouped V2's
 * per-machine order map. It is deliberately not a bare "local": remote machine
 * ids come from user settings, and a machine the user happened to id "local"
 * would otherwise silently overwrite this Mac's list and swallow its reorder.
 */
const SIDEBAR_V2_LOCAL_GROUP_ORDER_KEY = "sidebar-v2:local-project-order";

function createRemoteProjectListScopeId(remoteMachineId: string): string {
  return `remote:${remoteMachineId}`;
}

function ProjectListEndUngroupDropZone({
  active,
  scopeId,
}: {
  active: boolean;
  scopeId: string;
}) {
  return (
    <div
      aria-hidden="true"
      className="project-list-end-ungroup-drop-zone"
      data-active={String(active)}
      data-sidebar-project-ungroup-drop-zone={scopeId}
    />
  );
}

/*
 * CDXC:ProjectUngroupDrop 2026-07-23:
 * A collection-only Projects or Remote Machine section has no ungrouped row
 * whose collection id can resolve to undefined. Give each section a real
 * normal-flow end zone, then resolve it only for a grouped project from that
 * same local/remote scope. The zone owns the line below the final collection;
 * project rows and collection panels keep their existing independent drag
 * boundaries.
 */
function resolveProjectUngroupDropScopeFromPoint(
  nativeEvent: Event | undefined,
  sourceGroupId: string,
  groupsById: SidebarProjectGroupLookup,
): string | undefined {
  const point = getClientPoint(nativeEvent);
  if (!point) {
    return undefined;
  }
  const remoteMachineId =
    groupsById[sourceGroupId]?.remoteMachineContext?.machineId;
  const scopeId = remoteMachineId
    ? createRemoteProjectListScopeId(remoteMachineId)
    : LOCAL_PROJECT_LIST_SCOPE_ID;
  const element = document.querySelector<HTMLElement>(
    `[data-sidebar-project-ungroup-drop-zone="${CSS.escape(scopeId)}"]`,
  );
  if (!element) {
    return undefined;
  }
  const bounds = element.getBoundingClientRect();
  return bounds.height > 0 &&
    point.x >= bounds.left &&
    point.x <= bounds.right &&
    point.y >= bounds.top &&
    point.y <= bounds.bottom
    ? scopeId
    : undefined;
}

function moveProjectGroupFamilyToEnd(
  groupIds: readonly string[],
  sourceGroupId: string,
  groupsById: SidebarProjectGroupLookup,
): string[] {
  const sourceProjectId =
    groupsById[sourceGroupId]?.projectContext?.editor.projectId;
  if (!sourceProjectId) {
    return [...groupIds];
  }
  const familyProjectIds = new Set(
    getProjectCollectionFamilyProjectIds(sourceProjectId, groupIds, groupsById),
  );
  const isFamilyGroup = (groupId: string) => {
    const projectId = groupsById[groupId]?.projectContext?.editor.projectId;
    return Boolean(projectId && familyProjectIds.has(projectId));
  };
  return [
    ...groupIds.filter((groupId) => !isFamilyGroup(groupId)),
    ...groupIds.filter(isFamilyGroup),
  ];
}

/*
 * CDXC:CollectionReorder 2026-07-21:
 * Collection drags use feedback "none", so dnd-kit's rect-overlap collision
 * never reports a target (the source shape never leaves its slot). Resolve the
 * insertion boundary from the pointer against the local collection panels'
 * midpoints, exactly like resolveGroupDropTargetFromPoint does for project
 * rows. Remote sections render the same collections with the same ids, so the
 * lookup skips any panel inside a remote machine section.
 */
function getLocalProjectCollectionElement(collectionId: string): HTMLElement | undefined {
  const elements = document.querySelectorAll<HTMLElement>(
    `section.project-collection[data-sidebar-project-collection-id="${CSS.escape(collectionId)}"]`,
  );
  for (const element of elements) {
    if (!element.closest(".reference-remote-machine-section")) {
      return element;
    }
  }
  return undefined;
}

function resolveProjectCollectionDropTargetFromPoint(
  nativeEvent: Event | undefined,
  collectionIds: readonly string[],
  sourceCollectionId: string,
  targetData: ReturnType<typeof getSidebarDropData>,
): SidebarProjectCollectionDropTarget | undefined {
  const point = getClientPoint(nativeEvent);
  const candidate = point
    ? getProjectCollectionBoundaryTargetAtY(collectionIds, point.y)
    : targetData?.kind === "project-collection" &&
      collectionIds.includes(targetData.collectionId)
      ? { collectionId: targetData.collectionId, position: "before" as const }
      : undefined;
  if (!candidate) {
    return undefined;
  }
  return moveCollectionIdToDropTarget(collectionIds, sourceCollectionId, candidate)
    ? candidate
    : undefined;
}

function getProjectCollectionBoundaryTargetAtY(
  collectionIds: readonly string[],
  y: number,
): SidebarProjectCollectionDropTarget | undefined {
  const midpoints = collectionIds.flatMap((collectionId) => {
    const element = getLocalProjectCollectionElement(collectionId);
    if (!element) {
      return [];
    }
    const bounds = element.getBoundingClientRect();
    return bounds.height > 0
      ? [ { collectionId, midpoint: bounds.top + bounds.height / 2 } ]
      : [];
  });
  if (midpoints.length === 0) {
    return undefined;
  }
  for (const entry of midpoints) {
    if (y < entry.midpoint) {
      return { collectionId: entry.collectionId, position: "before" };
    }
  }
  return {
    collectionId: midpoints[ midpoints.length - 1 ].collectionId,
    position: "after",
  };
}

/*
 * Returns the reordered id list, or undefined when the drop is a no-op (the
 * boundary sits directly around the dragged collection's own slot).
 */
function moveCollectionIdToDropTarget(
  collectionIds: readonly string[],
  sourceCollectionId: string,
  target: SidebarProjectCollectionDropTarget,
): string[] | undefined {
  const withoutSource = collectionIds.filter(
    (collectionId) => collectionId !== sourceCollectionId,
  );
  if (withoutSource.length === collectionIds.length) {
    return undefined;
  }
  const anchorIndex = withoutSource.indexOf(target.collectionId);
  const insertionIndex =
    target.collectionId === sourceCollectionId
      ? undefined
      : anchorIndex < 0
        ? undefined
        : target.position === "before"
          ? anchorIndex
          : anchorIndex + 1;
  if (insertionIndex === undefined) {
    return undefined;
  }
  const next = [...withoutSource];
  next.splice(insertionIndex, 0, sourceCollectionId);
  return next.every((collectionId, index) => collectionId === collectionIds[ index ])
    ? undefined
    : next;
}

function resolveGroupDropTargetFromPoint(
  nativeEvent: Event | undefined,
  groupIds: readonly string[],
  groupsById: SidebarProjectGroupLookup,
  targetData: ReturnType<typeof getSidebarDropData>,
  sourceData: Extract<ReturnType<typeof getSidebarDropData>, { kind: "group"; }> | undefined,
  /*
   * CDXC:SidebarV2GroupedProjectUX 2026-07-30:
   * How "this drop would change nothing" is decided. V1's default answer runs the
   * physical project-with-worktrees move; grouped V2 passes its own, because its
   * ids are LOGICAL rows and the two moves can disagree about which boundaries
   * are no-ops. Letting the caller supply the predicate keeps the drop line and
   * the committed reorder answering the same question.
   */
  isNoOpTarget?: (target: SidebarGroupDropTarget) => boolean,
): SidebarGroupDropTarget | undefined {
  const point = getClientPoint(nativeEvent);
  /*
   * CDXC:ProjectReorder 2026-07-02-13:05:
   * The insertion line was dancing because dnd-kit's rect-overlap target could
   * disagree with the pointer position, and because the same boundary could be
   * reported as "after A" or "before B", which draw in different spots. While
   * the pointer is known, resolve one canonical boundary from the visible
   * header midpoints ("before" the first group whose header midpoint is below
   * the pointer, "after" only past the last group) and suppress the line for
   * no-op drops instead of falling through to another candidate.
   */
  const candidates = point
    ? [ getSidebarGroupBoundaryTargetAtY(groupIds, point.y) ]
    : [
      getSidebarGroupDropTargetFromDropData(targetData, point),
      getSidebarGroupDropTargetFromEvent(nativeEvent),
    ];

  for (const candidate of candidates) {
    if (!candidate) {
      continue;
    }

    if (!groupIds.includes(candidate.groupId)) {
      continue;
    }

    if (candidate.groupId === sourceData?.groupId) {
      return undefined;
    }

    if (
      sourceData &&
      (isNoOpTarget
        ? isNoOpTarget(candidate)
        : isNoOpGroupDropTarget(groupIds, sourceData.groupId, candidate, groupsById))
    ) {
      return undefined;
    }

    return candidate;
  }

  return undefined;
}

function getSidebarGroupBoundaryTargetAtY(
  groupIds: readonly string[],
  y: number,
): SidebarGroupDropTarget | undefined {
  const groupHeaderMidpoints = groupIds.flatMap((groupId) => {
    const groupElement = getSidebarGroupElementById(groupId);
    if (!groupElement) {
      return [];
    }

    const bounds = getSidebarGroupDropBoundsElement(groupElement).getBoundingClientRect();
    return bounds.height > 0 ? [ { groupId, midpoint: bounds.top + bounds.height / 2 } ] : [];
  });
  if (groupHeaderMidpoints.length === 0) {
    return undefined;
  }

  for (const header of groupHeaderMidpoints) {
    if (y < header.midpoint) {
      return { groupId: header.groupId, position: "before" };
    }
  }

  return {
    groupId: groupHeaderMidpoints[ groupHeaderMidpoints.length - 1 ].groupId,
    position: "after",
  };
}

function areSameGroupDropTarget(
  left: SidebarGroupDropTarget | undefined,
  right: SidebarGroupDropTarget | undefined,
): boolean {
  return left?.groupId === right?.groupId && left?.position === right?.position;
}

function areSameSessionDropTarget(
  left: SidebarSessionDropTarget | undefined,
  right: SidebarSessionDropTarget | undefined,
): boolean {
  if (!left || !right || left.kind !== right.kind || left.groupId !== right.groupId) {
    return left === right;
  }

  if (left.kind === "session" && right.kind === "session") {
    return left.sessionId === right.sessionId && left.position === right.position;
  }

  return left.position === right.position;
}

function isSourceSessionDropTarget(
  candidate: SidebarSessionDropTarget,
  sourceData: Extract<ReturnType<typeof getSidebarDropData>, { kind: "session"; }> | undefined,
): boolean {
  return Boolean(
    sourceData &&
    candidate.kind === "session" &&
    candidate.groupId === sourceData.groupId &&
    candidate.sessionId === sourceData.sessionId,
  );
}

function getSidebarSessionDropTargetFromDropData(
  targetData: ReturnType<typeof getSidebarDropData>,
  point: ReturnType<typeof getClientPoint>,
): SidebarSessionDropTarget | undefined {
  if (targetData?.kind === "session") {
    const sessionElement = getTargetSessionElement(targetData.sessionId, point);
    if (!sessionElement) {
      return undefined;
    }

    const bounds = sessionElement.getBoundingClientRect();
    const relativeY = point?.y ?? bounds.top + bounds.height / 2;
    /*
     * CDXC:SidebarDragDrop 2026-06-19-11:12:
     * Dnd-kit may report a broad target while the pointer is around a row
     * midpoint. Resolve the explicit target with the same center/down-after
     * rule as point-based row hit testing so the line stays visible.
     */
    const position: "after" | "before" =
      relativeY >= bounds.top + bounds.height / 2 ? "after" : "before";
    return {
      groupId: targetData.groupId,
      kind: "session",
      position,
      sessionId: targetData.sessionId,
    };
  }

  if (targetData?.kind === "group") {
    const groupElement = document.querySelector<HTMLElement>(
      `[data-sidebar-group-id="${targetData.groupId}"]`,
    );
    if (!groupElement) {
      return undefined;
    }

    const bounds = groupElement.getBoundingClientRect();
    const relativeY = point?.y ?? bounds.top;
    const position: "end" | "start" = relativeY > bounds.top + bounds.height / 2 ? "end" : "start";
    return {
      groupId: targetData.groupId,
      kind: "group",
      position,
    };
  }

  return undefined;
}

function getSidebarGroupDropTargetFromDropData(
  targetData: ReturnType<typeof getSidebarDropData>,
  point: ReturnType<typeof getClientPoint>,
): SidebarGroupDropTarget | undefined {
  if (targetData?.kind !== "group") {
    return undefined;
  }

  const groupElement = getTargetGroupElement(targetData.groupId, point);
  if (!groupElement) {
    return undefined;
  }

  /*
   * CDXC:ProjectReorder 2026-05-22-22:18:
   * Dnd-kit target data can point at an expanded project container. Use the
   * same header-row bounds as point-based hit testing so the drop line does not
   * jump between above and below while the pointer moves through session rows.
   */
  const boundsElement = getSidebarGroupDropBoundsElement(groupElement);
  const bounds = boundsElement.getBoundingClientRect();
  const relativeY = point?.y ?? bounds.top + bounds.height / 2;
  return {
    groupId: targetData.groupId,
    position: relativeY > bounds.top + bounds.height / 2 ? "after" : "before",
  };
}

function isNoOpGroupDropTarget(
  groupIds: readonly string[],
  sourceGroupId: string,
  target: SidebarGroupDropTarget,
  groupsById: SidebarProjectGroupLookup,
): boolean {
  /*
   * CDXC:ProjectReorder 2026-05-22-22:18:
   * Do not show an insertion line for adjacent before/after targets that would
   * leave the project order unchanged on drop. The preview should only mark
   * committed position changes.
   *
   * CDXC:WorktreeProjectOrder 2026-05-25-12:38:
   * Worktree projects cannot be dropped outside their main-project family, and
   * a main-project drag is computed as a family move so its worktrees stay
   * directly underneath it in the same order.
   */
  return haveSameSessionOrder(
    groupIds,
    moveGroupIdsByProjectDropTarget(groupIds, sourceGroupId, target, groupsById),
  );
}

function moveGroupIdsByProjectDropTarget(
  groupIds: readonly string[],
  sourceGroupId: string,
  target: SidebarGroupDropTarget,
  groupsById: SidebarProjectGroupLookup,
): string[] {
  const projectGroupItems = createProjectGroupOrderItems(groupIds, groupsById);
  if (projectGroupItems.length !== groupIds.length) {
    return moveGroupIdsByDropTarget(groupIds, sourceGroupId, target);
  }

  return moveProjectsWithWorktrees(projectGroupItems, sourceGroupId, {
    orderId: target.groupId,
    position: target.position,
  }).map((project) => project.orderId);
}

function createProjectGroupOrderItems(
  groupIds: readonly string[],
  groupsById: SidebarProjectGroupLookup,
): SidebarProjectGroupOrderItem[] {
  return groupIds.flatMap((groupId) => {
    const projectContext = groupsById[ groupId ]?.projectContext;
    if (!projectContext) {
      return [];
    }

    return [
      {
        orderId: groupId,
        projectId: projectContext.editor.projectId,
        worktree: projectContext.worktree
          ? { parentProjectId: projectContext.worktree.parentProjectId }
          : undefined,
      },
    ];
  });
}

function getProjectCollectionFamilyProjectIds(
  projectId: string,
  groupIds: readonly string[],
  groupsById: SidebarProjectGroupLookup,
): string[] {
  const requestedProjectContext = groupIds
    .map((groupId) => groupsById[groupId]?.projectContext)
    .find((projectContext) => projectContext?.editor.projectId === projectId);
  const familyParentProjectId =
    requestedProjectContext?.worktree?.parentProjectId ?? projectId;
  const projectIds = groupIds.flatMap((groupId) => {
    const projectContext = groupsById[groupId]?.projectContext;
    const candidateProjectId = projectContext?.editor.projectId;
    if (
      candidateProjectId === familyParentProjectId ||
      projectContext?.worktree?.parentProjectId === familyParentProjectId
    ) {
      return candidateProjectId ? [candidateProjectId] : [];
    }
    return [];
  });
  return projectIds.length > 0 ? [...new Set(projectIds)] : [projectId];
}

function createProjectCollectionIdByProjectId(
  state: SidebarProjectCollectionsState,
  groupIds: readonly string[],
  groupsById: SidebarProjectGroupLookup,
  resolveProjectId: (groupId: string) => string | undefined,
): Map<string, string> {
  const result = new Map<string, string>();
  for (const collection of state.collections) {
    for (const projectId of collection.projectIds) {
      result.set(projectId, collection.collectionId);
    }
  }
  for (const groupId of groupIds) {
    const projectId = resolveProjectId(groupId);
    const parentProjectId = groupsById[groupId]?.projectContext?.worktree?.parentProjectId;
    const inheritedCollectionId = parentProjectId ? result.get(parentProjectId) : undefined;
    if (projectId && inheritedCollectionId) {
      result.set(projectId, inheritedCollectionId);
    }
  }
  return result;
}

function getRemoteProjectCollectionFamilyProjectIds(
  scopedProjectId: string,
  groupIds: readonly string[],
  groupsById: SidebarProjectGroupLookup,
): string[] {
  const requestedGroup = groupIds
    .map((groupId) => groupsById[groupId])
    .find((group) => group?.projectContext?.editor.projectId === scopedProjectId);
  const rawProjectId = requestedGroup?.remoteMachineContext?.projectId;
  if (!rawProjectId) {
    return [];
  }
  const familyParentProjectId =
    requestedGroup?.projectContext?.worktree?.parentProjectId ?? rawProjectId;
  const projectIds = groupIds.flatMap((groupId) => {
    const group = groupsById[groupId];
    const candidateProjectId = group?.remoteMachineContext?.projectId;
    if (
      candidateProjectId === familyParentProjectId ||
      group?.projectContext?.worktree?.parentProjectId === familyParentProjectId
    ) {
      return candidateProjectId ? [candidateProjectId] : [];
    }
    return [];
  });
  return projectIds.length > 0 ? [...new Set(projectIds)] : [rawProjectId];
}

function getSidebarGroupDropBoundsElement(groupElement: HTMLElement): HTMLElement {
  return groupElement.querySelector<HTMLElement>(".group-head") ?? groupElement;
}

function getTargetSessionElement(
  sessionId: string,
  point: ReturnType<typeof getClientPoint>,
): HTMLElement | undefined {
  const selector = `[data-sidebar-session-id="${sessionId}"]`;
  if (point) {
    for (const element of document.elementsFromPoint(point.x, point.y)) {
      const sessionElement = element.closest<HTMLElement>(selector);
      if (sessionElement && sessionElement.dataset.dragging !== "true") {
        return sessionElement;
      }
    }
  }

  return Array.from(document.querySelectorAll<HTMLElement>(selector)).find(
    (sessionElement) => sessionElement.dataset.dragging !== "true",
  );
}

function getSidebarGroupElementById(groupId: string): HTMLElement | undefined {
  return Array.from(document.querySelectorAll<HTMLElement>("[data-sidebar-group-id]")).find(
    (groupElement) => groupElement.dataset.sidebarGroupId === groupId,
  );
}

function getTargetGroupElement(
  groupId: string,
  point: ReturnType<typeof getClientPoint>,
): HTMLElement | undefined {
  const selector = `[data-sidebar-group-id="${groupId}"]`;
  if (point) {
    for (const element of document.elementsFromPoint(point.x, point.y)) {
      const groupElement = element.closest<HTMLElement>(selector);
      if (groupElement && groupElement.dataset.dragging !== "true") {
        return groupElement;
      }
    }
  }

  return Array.from(document.querySelectorAll<HTMLElement>(selector)).find(
    (groupElement) => groupElement.dataset.dragging !== "true",
  );
}

function getDragNativeEvent(value: unknown): Event | undefined {
  return isObjectRecord(value) && value.nativeEvent instanceof Event
    ? value.nativeEvent
    : undefined;
}

function updateGroupDragPreviewFromEvent<
  Preview extends { pointerOffsetY: number; top: number; },
>(
  setGroupDragPreview: (
    updater: (previous: Preview | undefined) => Preview | undefined,
  ) => void,
  nativeEvent: Event | undefined,
): void {
  const point = getClientPoint(nativeEvent);
  if (!point) {
    return;
  }

  setGroupDragPreview((previous) =>
    previous
      ? {
        ...previous,
        top: point.y - previous.pointerOffsetY,
      }
      : previous,
  );
}

function getProjectGroupDragHeaderMetrics(
  groupId: string,
  point: { x: number; y: number; },
): { left: number; pointerOffsetY: number; top: number; width: number; } | undefined {
  const groupElement = Array.from(
    document.querySelectorAll<HTMLElement>("[data-sidebar-group-id]"),
  ).find(
    (candidate) =>
      candidate.dataset.sidebarGroupId === groupId && candidate.dataset.dragging !== "true",
  );
  const headerElement = groupElement?.querySelector<HTMLElement>(".group-head");
  const headerRect = headerElement?.getBoundingClientRect();
  if (!headerRect) {
    return undefined;
  }

  return {
    left: headerRect.left,
    pointerOffsetY: point.y - headerRect.top,
    top: headerRect.top,
    width: headerRect.width,
  };
}

function getProjectCollectionDragMetrics(
  source: unknown,
  collectionId: string,
): { left: number; top: number; width: number; } | undefined {
  /*
   * CDXC:CollectionDragPreview 2026-07-22:
   * The same collection can render once locally and once per remote machine
   * section, so prefer the dnd-kit source element (the grabbed section) over a
   * document query that could match another instance. The section rect is used
   * instead of the header rect so the ghost's own 1px panel border lands
   * exactly on the grabbed panel's border.
   */
  const sourceElement =
    isObjectRecord(source) && source.element instanceof HTMLElement ? source.element : undefined;
  const sectionElement =
    sourceElement?.dataset.sidebarProjectCollectionId === collectionId
      ? sourceElement
      : Array.from(
        document.querySelectorAll<HTMLElement>("[data-sidebar-project-collection-id]"),
      ).find((candidate) => candidate.dataset.sidebarProjectCollectionId === collectionId);
  const sectionRect = sectionElement?.getBoundingClientRect();
  if (!sectionRect) {
    return undefined;
  }

  return {
    left: sectionRect.left,
    top: sectionRect.top,
    width: sectionRect.width,
  };
}

function getRemoteMachineDragHeaderMetrics(
  remoteMachineId: string,
  point: { x: number; y: number; },
): { left: number; pointerOffsetY: number; top: number; width: number; } | undefined {
  const section = document.querySelector<HTMLElement>(
    `[data-sidebar-remote-machine-id="${CSS.escape(remoteMachineId)}"]`,
  );
  const header = section?.querySelector<HTMLElement>(".reference-sidebar-section-row");
  const bounds = header?.getBoundingClientRect();
  if (!bounds) {
    return undefined;
  }
  return {
    left: bounds.left,
    pointerOffsetY: point.y - bounds.top,
    top: bounds.top,
    width: bounds.width,
  };
}

function createSessionPointerDragState(
  sourceData: Extract<ReturnType<typeof getSidebarDropData>, { kind: "session"; }>,
  pointerDownSessionTarget: SidebarPointerDownSessionTarget | undefined,
  nativeEvent: Event | undefined,
): SidebarSessionPointerDragState {
  const startPoint =
    pointerDownSessionTarget &&
      pointerDownSessionTarget.groupId === sourceData.groupId &&
      pointerDownSessionTarget.sessionId === sourceData.sessionId
      ? pointerDownSessionTarget.point
      : undefined;

  return {
    didMove: hasPointerDragMovedPastThreshold(startPoint, getClientPoint(nativeEvent)),
    startPoint,
  };
}

function updateSessionPointerDragState(
  pointerDragState: SidebarSessionPointerDragState | undefined,
  nativeEvent: Event | undefined,
): void {
  if (!pointerDragState || pointerDragState.didMove) {
    return;
  }

  pointerDragState.didMove = hasPointerDragMovedPastThreshold(
    pointerDragState.startPoint,
    getClientPoint(nativeEvent),
  );
}

function hasPointerDragMovedPastThreshold(
  startPoint: { x: number; y: number; } | undefined,
  currentPoint: { x: number; y: number; } | undefined,
): boolean {
  if (!startPoint || !currentPoint) {
    return false;
  }

  return (
    Math.hypot(currentPoint.x - startPoint.x, currentPoint.y - startPoint.y) >=
    SIDEBAR_REORDER_DISTANCE_PX
  );
}

function isObjectRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function getSidebarStartupNow(): number {
  if (typeof performance !== "undefined") {
    return performance.now();
  }

  return Date.now();
}

function getSidebarStartupElapsedMs(startedAt: number): number {
  return Math.round(getSidebarStartupNow() - startedAt);
}

function countSidebarSessions(groups: readonly { sessions: readonly unknown[]; }[]): number {
  return groups.reduce((total, group) => total + group.sessions.length, 0);
}

function postSidebarAgentIconBoundaryLog(
  vscode: WebviewApi,
  event: string,
  details: Record<string, unknown>,
): void {
  vscode.postMessage({
    details,
    event,
    scenarioId: "native.agent.detection",
    type: "sidebarDebugLog",
  });
}

function summarizeSidebarAgentIconsFromGroups(
  groups: readonly {
    groupId: string;
    sessions: readonly {
      agentIcon?: string;
      sessionId: string;
      sessionKind?: string;
    }[];
  }[],
) {
  const sessions = groups.flatMap((group) =>
    group.sessions.map((session) => ({
      agentIcon: session.agentIcon,
      groupId: group.groupId,
      sessionId: session.sessionId,
      sessionKind: session.sessionKind,
    })),
  );

  return summarizeSidebarAgentIconSessions(sessions);
}

function summarizeSidebarAgentIconsFromStore(
  sessionsById: ReturnType<typeof useSidebarStore.getState>[ "sessionsById" ],
) {
  return summarizeSidebarAgentIconSessions(
    Object.values(sessionsById).map((session) => ({
      agentIcon: session.agentIcon,
      sessionId: session.sessionId,
      sessionKind: session.sessionKind,
    })),
  );
}

function summarizeSidebarAgentIconSessions(
  sessions: readonly {
    agentIcon?: string;
    groupId?: string;
    sessionId: string;
    sessionKind?: string;
  }[],
) {
  const agentSessions = sessions.filter((session) => Boolean(session.agentIcon));
  return {
    agentIconSessionCount: agentSessions.length,
    agentSessions: agentSessions.slice(0, 10),
    sessionCount: sessions.length,
  };
}

function createDisplayedSessionIdsByGroup({
  groupIds,
  query,
  selectedSessionTags,
  sessionIdsByGroup,
  sessionsById,
  shouldFilter,
}: {
  groupIds: readonly string[];
  query: string;
  selectedSessionTags: readonly SidebarSessionTagFilter[];
  sessionIdsByGroup: SessionIdsByGroup;
    sessionsById: ReturnType<typeof useSidebarStore.getState>[ "sessionsById" ];
  shouldFilter: boolean;
}): SessionIdsByGroup {
  const displayedSessionIdsByGroup: SessionIdsByGroup = {};

  for (const groupId of groupIds) {
    const sessionIds = sessionIdsByGroup[ groupId ] ?? [];
    const queryFilteredSessionIds = !shouldFilter
      ? [ ...sessionIds ]
      : filterSessionIdsByQuery(sessionIds, sessionsById, query);
    displayedSessionIdsByGroup[ groupId ] = filterSessionIdsByTags(
      queryFilteredSessionIds,
      sessionsById,
      selectedSessionTags,
    );
  }

  return displayedSessionIdsByGroup;
}

function filterSessionIdsByTags(
  sessionIds: readonly string[],
  sessionsById: ReturnType<typeof useSidebarStore.getState>[ "sessionsById" ],
  selectedSessionTags: readonly SidebarSessionTagFilter[],
): string[] {
  if (selectedSessionTags.length === 0) {
    return [ ...sessionIds ];
  }

  return sessionIds.filter((sessionId) => {
    const session = sessionsById[ sessionId ];
    return session ? sessionMatchesSidebarTagFilters(session, selectedSessionTags) : false;
  });
}

function filterSessionIdsByQuery(
  sessionIds: readonly string[],
  sessionsById: ReturnType<typeof useSidebarStore.getState>[ "sessionsById" ],
  query: string,
): string[] {
  const sessions = sessionIds.flatMap((sessionId) => {
    const session = sessionsById[ sessionId ];
    return session ? [ session ] : [];
  });
  const matchedSessionIds = new Set(
    filterSidebarSessionItems(sessions, query).map((session) => session.sessionId),
  );

  return sessionIds.filter((sessionId) => matchedSessionIds.has(sessionId));
}

function createDisplayedGroupIds(
  groupIds: readonly string[],
  sessionIdsByGroup: SessionIdsByGroup,
  shouldFilter: boolean,
): string[] {
  if (!shouldFilter) {
    return [ ...groupIds ];
  }

  return groupIds.filter((groupId) => (sessionIdsByGroup[ groupId ] ?? []).length > 0);
}

function getCommandPaletteHotkeyActionId(
  event: KeyboardEvent,
  hotkeys: ghostexHotkeySettings | undefined,
): "openCommandPalette" | "openSessionSearchPalette" | undefined {
  const hotkeyText = ghostexHotkeyTextFromKeyboardEvent(event);
  if (!hotkeyText) {
    return undefined;
  }
  const actionId = getghostexHotkeyActionIdForKey(
    normalizeghostexHotkeySettings(hotkeys),
    hotkeyText,
  );
  return actionId === "openCommandPalette" || actionId === "openSessionSearchPalette"
    ? actionId
    : undefined;
}

function hasActiveSidebarHotkeyRecorder(): boolean {
  return Boolean(document.querySelector("[data-hotkey-recorder='true'][data-recording='true']"));
}

function isSidebarSessionSearchNavigationKey(event: KeyboardEvent): boolean {
  return (
    !event.altKey &&
    !event.ctrlKey &&
    !event.metaKey &&
    (event.key === "ArrowDown" || event.key === "ArrowUp" || event.key === "Tab")
  );
}

function getSidebarSessionSearchNavigationDirection(event: KeyboardEvent): -1 | 1 {
  return event.key === "ArrowUp" || (event.key === "Tab" && event.shiftKey) ? -1 : 1;
}

function isEditableSidebarKeyboardTarget(target: Node): boolean {
  if (!(target instanceof HTMLElement)) {
    return false;
  }

  if (target.isContentEditable) {
    return true;
  }

  return Boolean(target.closest("input, textarea, select, [contenteditable]"));
}
