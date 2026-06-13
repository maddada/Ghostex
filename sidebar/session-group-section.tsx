import {
  IconCaretRightFilled,
  IconCheck,
  IconChevronDown,
  IconChevronLeft,
  IconChevronRight,
  IconChevronUp,
  IconCopy,
  IconFolder,
  IconFolderOpen,
  IconGitBranch,
  IconGitPullRequest,
  IconMessageCircle,
  IconMoon,
  IconPencil,
  IconPlayerPlay,
  IconPlus,
  IconRefresh,
  IconSettings,
  IconTerminal2,
  IconTrash,
  IconWorld,
  IconX,
} from "@tabler/icons-react";
import { CollisionPriority } from "@dnd-kit/abstract";
import { KeyboardSensor, PointerActivationConstraints, PointerSensor } from "@dnd-kit/dom";
import { SortableKeyboardPlugin } from "@dnd-kit/dom/sortable";
import { useDroppable } from "@dnd-kit/react";
import { useSortable } from "@dnd-kit/react/sortable";
import { createPortal } from "react-dom";
import {
  Fragment,
  forwardRef,
  startTransition,
  useCallback,
  useLayoutEffect,
  useEffect,
  useEffectEvent,
  useRef,
  useState,
  type ButtonHTMLAttributes,
  type CSSProperties,
  type Ref,
  type KeyboardEvent as ReactKeyboardEvent,
  type MouseEvent as ReactMouseEvent,
} from "react";
import { AppTooltip, SIDEBAR_TOOLTIP_DISMISS_EVENT } from "./app-tooltip";
import {
  getSidebarSessionLifecycleState,
  type SidebarTheme,
} from "../shared/session-grid-contract";
import type { SidebarProjectDiffStats } from "../shared/project-diff-stats";
import type { SidebarAgentButton } from "../shared/sidebar-agents";
import {
  DEFAULT_ghostex_SETTINGS,
  clampProjectSessionListCollapsedCount,
} from "../shared/ghostex-settings";
import { ConfirmationModal } from "./confirmation-modal";
import {
  createGroupDropData,
  createSessionDropTargetData,
  createSessionDropTargetId,
  type SidebarGroupDropTarget,
  type SidebarSessionDropTarget,
} from "./sidebar-dnd";
import { getGroupSessionSummary, type GroupSessionSummary } from "./group-session-summary";
import { shouldShowSessionGroupConnector } from "./session-group-connector";
import { getGroupStatusAnchorName, getSessionStatusAnchorName } from "./session-status-anchor";
import { useSidebarStore } from "./sidebar-store";
import { SortableSessionCard } from "./sortable-session-card";
import { SidebarContextMenuPortal } from "./sidebar-context-menu-portal";
import { useCollapsibleHeight } from "./use-collapsible-height";
import type { WebviewApi } from "./webview-api";
import { openAppModal } from "./app-modal-host-bridge";
import {
  PROJECT_SESSION_LIST_COLLAPSED_CHANGED_EVENT,
  getProjectSessionListCollapsedHeight,
  getVisibleProjectSessionIds,
  readProjectSessionListCollapsedState,
  writeProjectSessionListCollapsedState,
} from "./project-session-list-toggle";
import {
  DEFAULT_WORKSPACE_THEME_COLOR,
  normalizeWorkspaceThemeColor,
  updateWorkspaceThemeColorHistory,
} from "../shared/workspace-project-appearance";
import {
  readWorkspaceThemeColorHistory,
  writeWorkspaceThemeColorHistory,
} from "./workspace-theme-color-history";
import {
  PRIMARY_AGENT_LAUNCHER_CHANGED_EVENT,
  readPrimaryAgentLauncherId,
  writePrimaryAgentLauncherId,
  type PrimaryAgentLauncherChangedEvent,
} from "./primary-agent-launcher";
import { ProjectAgentLauncherIcon } from "./project-agent-launcher-icon";

const CONTEXT_MENU_MARGIN_PX = 12;
const CONTEXT_MENU_WIDTH_PX = 196;
const CONTEXT_MENU_ITEM_HEIGHT_PX = 34;
const CONTEXT_MENU_VERTICAL_PADDING_PX = 12;
const GROUP_CONTROL_MENU_MARGIN_PX = 12;
const GROUP_AGENT_MENU_WIDTH_PX = 220;
const PROJECT_HEADER_TOOLTIP_VIEWPORT_MARGIN_PX = 8;
const PROJECT_HEADER_TOOLTIP_TRIGGER_OFFSET_PX = 8;
/**
 * CDXC:ProjectReorder 2026-06-13-05:19:
 * macOS sidebar project-header reorder should require a slightly longer hold
 * than session-card dragging so normal header clicks and small pointer moves
 * do not activate project drag too soon.
 */
const GROUP_DRAG_HOLD_DELAY_MS = 250;
const GROUP_DRAG_HOLD_TOLERANCE_PX = 12;
const TOUCH_GROUP_DRAG_HOLD_DELAY_MS = 320;
const TOUCH_GROUP_DRAG_HOLD_TOLERANCE_PX = 12;
const PROJECT_EDITOR_DISPLAY_MAX_FILES = 99;
const DISABLED_GROUP_DND_AX_ATTRIBUTES = [
  "aria-describedby",
  "aria-disabled",
  "aria-grabbed",
  "aria-pressed",
  "aria-roledescription",
  "tabindex",
] as const;
const NESTED_CONTEXT_MENU_INTERACTIVE_SELECTOR =
  "button, input, textarea, select, a[href], [role='button'], [role='menuitem'], [contenteditable='true'], .group-header-actions";
const GROUP_DRAG_BLOCKED_ACTIVATION_SELECTOR =
  "input, textarea, select, [contenteditable='true'], .group-header-actions";

function isNestedInteractiveContextMenuTarget(event: ReactMouseEvent<HTMLElement>): boolean {
  const target = event.target;
  if (!(target instanceof Element)) {
    return false;
  }

  const interactiveTarget = target.closest(NESTED_CONTEXT_MENU_INTERACTIVE_SELECTOR);
  return (
    interactiveTarget instanceof HTMLElement &&
    interactiveTarget !== event.currentTarget &&
    event.currentTarget.contains(interactiveTarget)
  );
}

/**
 * CDXC:ProjectReorder 2026-06-09-17:15:
 * Project headers should reorder from any non-control header surface, not only
 * the project-name text. Keep nested action buttons and title-edit inputs out
 * of drag activation so their clicks and editing behavior stay deterministic.
 */
export function shouldPreventGroupDragActivation(
  target: EventTarget | null,
  dragSurface: Element | null | undefined,
): boolean {
  if (!isElementTarget(target)) {
    return false;
  }

  if (dragSurface && !dragSurface.contains(target)) {
    return false;
  }

  const blockedTarget = target.closest(GROUP_DRAG_BLOCKED_ACTIVATION_SELECTOR);
  return blockedTarget instanceof Element && (!dragSurface || dragSurface.contains(blockedTarget));
}

function isElementTarget(target: EventTarget | null): target is Element {
  return typeof Element !== "undefined" && target instanceof Element;
}
/**
 * CDXC:ProjectDiffStats 2026-05-27-10:44:
 * Cap git +/− line counts shown in project headers at four digits so very large
 * diffs stay readable in the sidebar without widening the status label.
 */
const PROJECT_EDITOR_DISPLAY_MAX_LINES = 9999;
const PROJECT_CONTEXT_THEME_OPTIONS: ReadonlyArray<{ label: string; value: SidebarTheme }> = [
  { label: "Dark Gray", value: "plain-dark" },
  { label: "Dark Green", value: "dark-green" },
  { label: "Dark Blue", value: "dark-blue" },
  { label: "Dark Red", value: "dark-red" },
  { label: "Dark Pink", value: "dark-pink" },
  { label: "Dark Orange", value: "dark-orange" },
  { label: "Light Blue", value: "light-blue" },
  { label: "Light Green", value: "light-green" },
  { label: "Light Pink", value: "light-pink" },
  { label: "Light Orange", value: "light-orange" },
];

function getAnchoredSessionStatusStyle(sessionId: string): CSSProperties {
  return {
    left: "anchor(right)",
    positionAnchor: getSessionStatusAnchorName(sessionId),
    top: "anchor(center)",
  } as CSSProperties;
}

function getCollapsedGroupStatusStyle(groupId: string): CSSProperties {
  return {
    left: "anchor(right)",
    positionAnchor: getGroupStatusAnchorName(groupId),
    top: "anchor(center)",
  } as CSSProperties;
}

/**
 * CDXC:WorkspaceTheme 2026-05-05-02:58
 * Combined-mode project headers consume the persisted workspace theme color
 * through one CSS variable so folder icons, titles, and hover surfaces can
 * share the same tint without changing chat or browser group styling.
 */
function getProjectThemeStyle(themeColor: string | undefined): CSSProperties | undefined {
  if (!themeColor) {
    return undefined;
  }

  return {
    "--workspace-project-theme-color": themeColor,
  } as CSSProperties;
}

function getProjectThemeSwatchStyle(themeColor: string | undefined): CSSProperties | undefined {
  if (!themeColor) {
    return undefined;
  }

  return {
    "--workspace-theme-swatch-background": themeColor,
  } as CSSProperties;
}

const groupSensors = [
  PointerSensor.configure({
    activationConstraints(event) {
      if (event.pointerType === "touch") {
        return [
          new PointerActivationConstraints.Delay({
            tolerance: TOUCH_GROUP_DRAG_HOLD_TOLERANCE_PX,
            value: TOUCH_GROUP_DRAG_HOLD_DELAY_MS,
          }),
        ];
      }

      return [
        new PointerActivationConstraints.Delay({
          tolerance: GROUP_DRAG_HOLD_TOLERANCE_PX,
          value: GROUP_DRAG_HOLD_DELAY_MS,
        }),
      ];
    },
    preventActivation(event, source) {
      return shouldPreventGroupDragActivation(event.target, source.handle ?? source.element);
    },
  }),
  KeyboardSensor,
];

type ContextMenuPosition = {
  x: number;
  y: number;
};

type GroupContextMenuPosition = ContextMenuPosition & {
  view: "group" | "project-custom-theme" | "project-themes";
};

type GroupControlMenu = "project-agent";

type ProjectHeaderActionTooltipPosition = {
  left: number;
  maxWidth: number;
  top: number;
};

type ProjectHeaderActionButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  tooltip: string;
};

function assignProjectHeaderActionRef(
  ref: Ref<HTMLButtonElement> | undefined,
  value: HTMLButtonElement | null,
): void {
  if (!ref) {
    return;
  }

  if (typeof ref === "function") {
    ref(value);
    return;
  }

  ref.current = value;
}

const ProjectHeaderActionButton = forwardRef<
  HTMLButtonElement,
  ProjectHeaderActionButtonProps
>(function ProjectHeaderActionButton(
  {
    children,
    className,
    disabled,
    onBlur,
    onFocus,
    onMouseEnter,
    onMouseLeave,
    tooltip,
    ...buttonProps
  },
  forwardedRef,
) {
  const buttonRef = useRef<HTMLButtonElement>(null);
  const tooltipRef = useRef<HTMLDivElement>(null);
  const [isTooltipOpen, setIsTooltipOpen] = useState(false);
  const [tooltipPosition, setTooltipPosition] =
    useState<ProjectHeaderActionTooltipPosition>();

  const setButtonRef = (button: HTMLButtonElement | null) => {
    buttonRef.current = button;
    assignProjectHeaderActionRef(forwardedRef, button);
  };

  const closeTooltip = () => {
    setIsTooltipOpen(false);
    setTooltipPosition(undefined);
  };

  const openTooltip = () => {
    if (disabled || !tooltip) {
      closeTooltip();
      return;
    }

    setIsTooltipOpen(true);
  };

  useEffect(() => {
    const handleSidebarTooltipDismiss = () => closeTooltip();
    window.addEventListener(SIDEBAR_TOOLTIP_DISMISS_EVENT, handleSidebarTooltipDismiss);
    return () => {
      window.removeEventListener(SIDEBAR_TOOLTIP_DISMISS_EVENT, handleSidebarTooltipDismiss);
    };
  }, []);

  useLayoutEffect(() => {
    if (!isTooltipOpen) {
      return undefined;
    }

    const updateTooltipPosition = () => {
      const button = buttonRef.current;
      const tooltipElement = tooltipRef.current;
      if (!button || !tooltipElement) {
        return;
      }

      const buttonBounds = button.getBoundingClientRect();
      const tooltipBounds = tooltipElement.getBoundingClientRect();
      const maxWidth = Math.max(
        0,
        window.innerWidth - PROJECT_HEADER_TOOLTIP_VIEWPORT_MARGIN_PX * 2,
      );
      const width = Math.min(tooltipBounds.width, maxWidth);
      const halfWidth = width / 2;
      const centeredLeft = buttonBounds.left + buttonBounds.width / 2;
      const left = Math.max(
        PROJECT_HEADER_TOOLTIP_VIEWPORT_MARGIN_PX + halfWidth,
        Math.min(
          centeredLeft,
          window.innerWidth - PROJECT_HEADER_TOOLTIP_VIEWPORT_MARGIN_PX - halfWidth,
        ),
      );
      const belowTop = buttonBounds.bottom + PROJECT_HEADER_TOOLTIP_TRIGGER_OFFSET_PX;
      const preferredTop = belowTop;
      const top = Math.max(
        PROJECT_HEADER_TOOLTIP_VIEWPORT_MARGIN_PX,
        Math.min(
          preferredTop,
          window.innerHeight - PROJECT_HEADER_TOOLTIP_VIEWPORT_MARGIN_PX - tooltipBounds.height,
        ),
      );

      setTooltipPosition((previousPosition) => {
        if (
          previousPosition?.left === left &&
          previousPosition.maxWidth === maxWidth &&
          previousPosition.top === top
        ) {
          return previousPosition;
        }

        return { left, maxWidth, top };
      });
    };

    updateTooltipPosition();
    window.addEventListener("resize", updateTooltipPosition);
    window.addEventListener("scroll", updateTooltipPosition, true);

    const resizeObserver =
      typeof ResizeObserver === "undefined" ? undefined : new ResizeObserver(updateTooltipPosition);
    if (buttonRef.current) {
      resizeObserver?.observe(buttonRef.current);
    }
    if (tooltipRef.current) {
      resizeObserver?.observe(tooltipRef.current);
    }

    return () => {
      window.removeEventListener("resize", updateTooltipPosition);
      window.removeEventListener("scroll", updateTooltipPosition, true);
      resizeObserver?.disconnect();
    };
  }, [isTooltipOpen, tooltip]);

  /*
   * CDXC:ProjectHeaderTooltips 2026-05-29-20:29:
   * Project-header action tooltips must paint above subsequent sticky project
   * headers. Keep the actual header button in its layout slot, but portal the
   * tooltip bubble to document.body and place it from the button rect so group
   * and sticky-header stacking contexts cannot cover the label.
   *
   * CDXC:SidebarTooltips 2026-05-30-06:36:
   * Sidebar action tooltips must share the square bordered surface and open
   * below the hovered action. Keep the portal placement logic for native
   * webview stacking, but stop flipping project-header action labels above the
   * trigger when the sidebar has more room on top.
   */
  return (
    <>
      <button
        {...buttonProps}
        className={className}
        disabled={disabled}
        onBlur={(event) => {
          onBlur?.(event);
          closeTooltip();
        }}
        onFocus={(event) => {
          onFocus?.(event);
          openTooltip();
        }}
        onMouseEnter={(event) => {
          onMouseEnter?.(event);
          openTooltip();
        }}
        onMouseLeave={(event) => {
          onMouseLeave?.(event);
          closeTooltip();
        }}
        ref={setButtonRef}
      >
        {children}
      </button>
      {isTooltipOpen && tooltip
        ? createPortal(
            <div
              className="project-header-action-tooltip-popup"
              ref={tooltipRef}
              role="tooltip"
              style={
                {
                  "--project-header-action-tooltip-left": tooltipPosition
                    ? `${tooltipPosition.left}px`
                    : "50vw",
                  "--project-header-action-tooltip-max-width": tooltipPosition
                    ? `${tooltipPosition.maxWidth}px`
                    : `calc(100vw - ${PROJECT_HEADER_TOOLTIP_VIEWPORT_MARGIN_PX * 2}px)`,
                  "--project-header-action-tooltip-top": tooltipPosition
                    ? `${tooltipPosition.top}px`
                    : "0px",
                } as CSSProperties
              }
            >
              {tooltip}
            </div>,
            document.body,
          )
        : null}
    </>
  );
});

export function shouldTreatProjectAsEmptySessionGroup({
  hasProjectContext,
  sessionCount,
}: {
  hasProjectContext: boolean;
  sessionCount: number;
}): boolean {
  /**
   * CDXC:ProjectHeaders 2026-05-18-14:53:
   * Empty project groups should not show a "No sessions" placeholder. When a
   * user expands a collapsed empty project, create a session there so the
   * expanded project immediately has active content.
   */
  return hasProjectContext && sessionCount === 0;
}

export const PINNED_SESSION_DROP_GAP_AFTER_LAST = "after-last";

function getSessionDropGapKeyBefore(sessionId: string): string {
  return `before:${sessionId}`;
}

/**
 * CDXC:PinnedSessions 2026-06-02-20:35:
 * Pinned project-session reorder feedback should paint one stable insertion
 * line in a fixed gap slot, including the slot before the first pinned row.
 * Map row-based drop targets to visible list gaps so the line does not jitter
 * with per-row hover halves while the drag pointer moves over a session.
 */
export function getPinnedSessionDropGapKey({
  dropTarget,
  groupId,
  visibleSessionIds,
}: {
  dropTarget: SidebarSessionDropTarget | undefined;
  groupId: string;
  visibleSessionIds: readonly string[];
}): string | undefined {
  if (!dropTarget || dropTarget.groupId !== groupId || visibleSessionIds.length === 0) {
    return undefined;
  }

  if (dropTarget.kind === "group") {
    return dropTarget.position === "start"
      ? getSessionDropGapKeyBefore(visibleSessionIds[0])
      : PINNED_SESSION_DROP_GAP_AFTER_LAST;
  }

  const targetIndex = visibleSessionIds.indexOf(dropTarget.sessionId);
  if (targetIndex < 0) {
    return undefined;
  }

  if (dropTarget.position === "before") {
    return getSessionDropGapKeyBefore(dropTarget.sessionId);
  }

  const nextSessionId = visibleSessionIds[targetIndex + 1];
  return nextSessionId
    ? getSessionDropGapKeyBefore(nextSessionId)
    : PINNED_SESSION_DROP_GAP_AFTER_LAST;
}

export function shouldShowOpenProjectFolderIcon({
  isCollapsed,
  sessionCount,
}: {
  isCollapsed: boolean;
  sessionCount: number;
}): boolean {
  /**
   * CDXC:ProjectHeaders 2026-05-17-01:43:
   * A project row with zero sessions should look like a closed folder even
   * when its group body is technically expanded. Reserve the open folder icon
   * for expanded projects that actually have session rows under them.
   */
  return !isCollapsed && sessionCount > 0;
}

export function formatProjectEditorDiffStatsLabel(
  stats: SidebarProjectDiffStats,
  showFileCount = false,
): string {
  /**
   * CDXC:ProjectDiffStats 2026-05-15-13:58:
   * Project git additions/deletions belong beside the project name, not inside
   * the former sidebar Code launcher. Keep the compact stat formatter shared
   * so the header label and tests preserve the existing capped numeric
   * behavior.
   */
  return [
    showFileCount ? formatProjectEditorFilesCount(stats.files) : undefined,
    `+${formatProjectEditorLineCount(stats.additions)}`,
    `-${formatProjectEditorLineCount(stats.deletions)}`,
  ]
    .filter((part): part is string => part !== undefined)
    .join(" ");
}

export function shouldShowProjectEditorDiffStats(stats: SidebarProjectDiffStats): boolean {
  /**
   * CDXC:ProjectDiffStats 2026-05-15-19:36:
   * Project headers should stay quiet when git reports no added or removed
   * lines. Hide the adjacent status text for +0 -0, but keep showing it as
   * soon as either additions or deletions is nonzero.
   */
  return stats.additions > 0 || stats.deletions > 0;
}

function formatProjectEditorFilesCount(files: number): string {
  return String(Math.min(PROJECT_EDITOR_DISPLAY_MAX_FILES, Math.max(0, files)));
}

function formatProjectEditorLineCount(lines: number): string {
  return String(Math.min(PROJECT_EDITOR_DISPLAY_MAX_LINES, Math.max(0, lines)));
}

function ProjectHeaderDiffStats({
  showFileCount,
  stats,
}: {
  showFileCount: boolean;
  stats: SidebarProjectDiffStats;
}) {
  return (
    <div
      aria-label={`Git changes: ${formatProjectEditorDiffStatsLabel(stats, showFileCount)}`}
      className="group-project-diff-stats"
    >
      {showFileCount ? (
        <span className="group-project-diff-files">
          {formatProjectEditorFilesCount(stats.files)}
        </span>
      ) : null}
      <span className="group-project-diff-stat group-project-diff-stat-additions">
        +{formatProjectEditorLineCount(stats.additions)}
      </span>
      <span className="group-project-diff-stat group-project-diff-stat-deletions">
        -{formatProjectEditorLineCount(stats.deletions)}
      </span>
    </div>
  );
}

function formatProjectTooltipGitStats(stats: SidebarProjectDiffStats): string {
  if (stats.isLoading) {
    return "Git: loading changes";
  }

  if (!stats.isRepo) {
    return "Git: not a repository";
  }

  const fileCount = Math.max(0, stats.files);
  return `${fileCount} ${formatCountLabel(fileCount, "file")} changed  +${formatProjectEditorLineCount(
    stats.additions,
  )}  -${formatProjectEditorLineCount(
    stats.deletions,
  )}`;
}

function formatCountLabel(count: number, singular: string): string {
  return Math.abs(count) === 1 ? singular : `${singular}s`;
}

function ProjectTitleTooltip({
  projectKindLabel,
  projectPath,
  sessionCount,
  stats,
  title,
  worktreeCount,
}: {
  projectKindLabel: string;
  projectPath: string;
  sessionCount: number;
  stats: SidebarProjectDiffStats;
  title: string;
  worktreeCount: number;
}) {
  return (
    <div className="project-title-tooltip">
      <div className="project-title-tooltip-heading">{title}</div>
      <div className="project-title-tooltip-body">
        <div>{projectKindLabel}</div>
        <div className="project-title-tooltip-path">{projectPath}</div>
        <div>{formatProjectTooltipGitStats(stats)}</div>
        <div>
          {sessionCount} {formatCountLabel(sessionCount, "session")} · {worktreeCount}{" "}
          {formatCountLabel(worktreeCount, "worktree")}
        </div>
      </div>
    </div>
  );
}

export type SessionGroupSectionProps = {
  autoEdit: boolean;
  canClose: boolean;
  completionFlashNonceBySessionId?: Record<string, number>;
  draggingDisabled?: boolean;
  groupDropIndicator?: SidebarGroupDropTarget;
  groupId: string;
  index: number;
  isGroupDragPreviewSource?: boolean;
  isCollapsed: boolean;
  onAutoEditHandled: () => void;
  onCollapsedChange: (groupId: string, collapsed: boolean) => void;
  onCreateSessionRequested?: (groupId: string) => void;
  onFocusRequested?: (groupId: string, sessionId: string) => void;
  orderedSessionIds?: readonly string[];
  selectedSearchSessionId?: string;
  allowPinnedSessionReorder?: boolean;
  enableProjectSessionListToggle?: boolean;
  pinnedSessionDropIndicator?: SidebarSessionDropTarget;
  sessionDropIndicatorGroupId?: string;
  sessionDraggingDisabled?: boolean;
  projectHeaderActions?: "all" | "terminal-only";
  showHeaderActions?: boolean;
  showSessionDropPositionIndicators?: boolean;
  vscode: WebviewApi;
};

function clampContextMenuPosition(
  clientX: number,
  clientY: number,
  itemCount: number,
): GroupContextMenuPosition {
  const menuHeight = CONTEXT_MENU_VERTICAL_PADDING_PX + itemCount * CONTEXT_MENU_ITEM_HEIGHT_PX;
  return {
    view: "group",
    x: Math.max(
      CONTEXT_MENU_MARGIN_PX,
      Math.min(clientX, window.innerWidth - CONTEXT_MENU_WIDTH_PX - CONTEXT_MENU_MARGIN_PX),
    ),
    y: Math.max(
      CONTEXT_MENU_MARGIN_PX,
      Math.min(clientY, window.innerHeight - menuHeight - CONTEXT_MENU_MARGIN_PX),
    ),
  };
}

export function getGroupContextMenuItemCount({
  canFullReloadGroup,
  hasProjectContext,
  isWorktreeProject,
}: {
  canFullReloadGroup: boolean;
  hasProjectContext: boolean;
  isWorktreeProject: boolean;
}): number {
  /*
   * CDXC:ProjectGroups 2026-06-08-09:19:
   * Worktree project headings should expose Copy Path but omit the IDE Open action in their compact context menu. Keep the root context-menu item count explicit by project kind so viewport clamping stays aligned with the visible worktree and repository menu actions.
   */
  if (hasProjectContext) {
    return isWorktreeProject ? 5 : 5 + Number(canFullReloadGroup);
  }

  return 3 + Number(canFullReloadGroup);
}

function getControlMenuPosition(button: HTMLButtonElement | null): ContextMenuPosition | undefined {
  if (!button) {
    return undefined;
  }

  const bounds = button.getBoundingClientRect();
  return {
    x: Math.max(
      GROUP_CONTROL_MENU_MARGIN_PX,
      Math.min(bounds.left + bounds.width / 2, window.innerWidth - GROUP_CONTROL_MENU_MARGIN_PX),
    ),
    y: Math.max(
      GROUP_CONTROL_MENU_MARGIN_PX,
      Math.min(bounds.bottom + 6, window.innerHeight - GROUP_CONTROL_MENU_MARGIN_PX),
    ),
  };
}

export function SessionGroupSection({
  autoEdit,
  canClose,
  completionFlashNonceBySessionId,
  draggingDisabled = false,
  groupDropIndicator,
  groupId,
  index,
  isGroupDragPreviewSource = false,
  isCollapsed,
  onAutoEditHandled,
  onCollapsedChange,
  onCreateSessionRequested,
  onFocusRequested,
  orderedSessionIds: orderedSessionIdsProp,
  selectedSearchSessionId,
  allowPinnedSessionReorder = false,
  enableProjectSessionListToggle = true,
  pinnedSessionDropIndicator,
  projectHeaderActions = "all",
  sessionDropIndicatorGroupId,
  sessionDraggingDisabled = false,
  showHeaderActions = true,
  showSessionDropPositionIndicators = true,
  vscode,
}: SessionGroupSectionProps) {
  const group = useSidebarStore((state) => state.groupsById[groupId]);
  const storedSessionIds = useSidebarStore((state) => state.sessionIdsByGroup[groupId] ?? []);
  const sessionsById = useSidebarStore((state) => state.sessionsById);
  const projectWorktreeCount = useSidebarStore((state) => {
    const projectId = state.groupsById[groupId]?.projectContext?.editor.projectId;
    if (!projectId) {
      return 0;
    }

    return Object.values(state.groupsById).filter(
      (candidate) => candidate?.projectContext?.worktree?.parentProjectId === projectId,
    ).length;
  });
  const orderedSessionIds = orderedSessionIdsProp ?? storedSessionIds;
  const [contextMenuPosition, setContextMenuPosition] = useState<GroupContextMenuPosition>();
  const [customThemeColor, setCustomThemeColor] = useState(DEFAULT_WORKSPACE_THEME_COLOR);
  const [recentThemeColors, setRecentThemeColors] = useState(readWorkspaceThemeColorHistory);
  const [draftTitle, setDraftTitle] = useState(group?.title ?? "");
  const [isConfirmOpen, setIsConfirmOpen] = useState(false);
  const [isEditing, setIsEditing] = useState(false);
  const [openControlMenu, setOpenControlMenu] = useState<GroupControlMenu>();
  const [primaryProjectAgentLauncherId, setPrimaryProjectAgentLauncherId] = useState(
    readPrimaryAgentLauncherId,
  );
  const [projectSessionListCollapsedState, setProjectSessionListCollapsedState] = useState(
    readProjectSessionListCollapsedState,
  );
  const [projectSessionListCollapsedHeight, setProjectSessionListCollapsedHeight] =
    useState<number>();
  const { collapsibleStyle, contentRef } = useCollapsibleHeight<HTMLDivElement>();
  const menuRef = useRef<HTMLDivElement>(null);
  const controlMenuRef = useRef<HTMLDivElement>(null);
  const projectAgentButtonRef = useRef<HTMLButtonElement>(null);
  const groupSectionRef = useRef<HTMLElement | null>(null);
  const debugInstanceIdRef = useRef(createSessionGroupDebugInstanceId());

  useEffect(() => {
    const refreshCollapsedState = () => {
      setProjectSessionListCollapsedState(readProjectSessionListCollapsedState());
    };

    window.addEventListener(PROJECT_SESSION_LIST_COLLAPSED_CHANGED_EVENT, refreshCollapsedState);
    return () => {
      window.removeEventListener(
        PROJECT_SESSION_LIST_COLLAPSED_CHANGED_EVENT,
        refreshCollapsedState,
      );
    };
  }, []);

  useEffect(() => {
    const refreshPrimaryAgentLauncher = (event: Event) => {
      const changedEvent = event as PrimaryAgentLauncherChangedEvent;
      setPrimaryProjectAgentLauncherId(
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

  /**
   * CDXC:Projects 2026-05-04-14:49
   * Project group headers use folder metaphors: closed folder when collapsed
   * and open folder when expanded. The synthetic Chats group keeps the chat
   * glyph so users can distinguish projectless conversations from projects.
   *
   * CDXC:Chats 2026-05-04-09:41
   * The Combined-mode Chats header is a synthetic collection, not one mutable
   * project group. It can create new chat folders, but it must not accept
   * session drops, group dragging, or project/group context-menu mutations.
   */
  const isChatCollection = group?.isChatCollection === true;
  const projectContext = group?.projectContext;
  const projectSessionListStorageId = projectContext?.editor.projectId ?? group?.groupId;
  const isProjectSessionListCollapsed =
    Boolean(projectContext) &&
    projectSessionListStorageId !== undefined &&
    projectSessionListCollapsedState[projectSessionListStorageId] === true;
  /**
   * CDXC:SidebarLayout 2026-05-13-08:11
   * Project groups stay draggable while session drag targets are disabled in
   * the reference sidebar. That prevents session moves across project
   * boundaries without taking away project-group reordering.
   */
  const areSessionDropTargetsDisabled = draggingDisabled || sessionDraggingDisabled;
  /**
   * CDXC:PinnedSessions 2026-05-28-12:04:
   * Reference project rows still disable general session dragging, but
   * pinned-session reorder needs active drop targets across the same project
   * list so a dragged pinned row can be released over any row in that project.
   */
  const debuggingMode = useSidebarStore((state) => state.hud.debuggingMode);
  const agents = useSidebarStore((state) => state.hud.agents);
  const hideProjectHeaderDiffStats = useSidebarStore(
    (state) =>
      state.hud.settings?.hideProjectHeaderDiffStats ??
      DEFAULT_ghostex_SETTINGS.hideProjectHeaderDiffStats,
  );
  const showProjectEditorDiffFileCount = useSidebarStore(
    (state) =>
      state.hud.settings?.showProjectEditorDiffFileCount ??
      DEFAULT_ghostex_SETTINGS.showProjectEditorDiffFileCount,
  );
  const projectSessionListCollapsedCount = useSidebarStore(
    (state) =>
      clampProjectSessionListCollapsedCount(
        state.hud.settings?.projectSessionListCollapsedCount ??
          DEFAULT_ghostex_SETTINGS.projectSessionListCollapsedCount,
      ),
  );
  const visibleSessionIds = getVisibleProjectSessionIds({
    collapsedCount: projectSessionListCollapsedCount,
    isCollapsed: isProjectSessionListCollapsed,
    isProjectGroup: Boolean(projectContext),
    isToggleEnabled: enableProjectSessionListToggle,
    sessionIds: orderedSessionIds,
  });
  const shouldShowProjectSessionListToggle =
    Boolean(projectContext) &&
    !isCollapsed &&
    enableProjectSessionListToggle &&
    orderedSessionIds.length > projectSessionListCollapsedCount;
  const renderedSessionIds = shouldShowProjectSessionListToggle ? orderedSessionIds : visibleSessionIds;
  const projectSessionListLastVisibleSessionId =
    visibleSessionIds.length > 0 ? visibleSessionIds[visibleSessionIds.length - 1] : undefined;
  const projectSessionListRenderedSessionIdsKey = renderedSessionIds.join("\u0000");
  const shouldClipProjectSessionList =
    shouldShowProjectSessionListToggle && isProjectSessionListCollapsed;

  useLayoutEffect(() => {
    if (!shouldClipProjectSessionList) {
      return;
    }

    const element = contentRef.current;
    if (!element) {
      return;
    }

    let animationFrameId = 0;

    const updateCollapsedHeight = () => {
      setProjectSessionListCollapsedHeight(
        getProjectSessionListCollapsedHeight({
          lastVisibleSessionId: projectSessionListLastVisibleSessionId,
          sessionListElement: element,
        }),
      );
    };

    const scheduleUpdate = () => {
      window.cancelAnimationFrame(animationFrameId);
      animationFrameId = window.requestAnimationFrame(updateCollapsedHeight);
    };

    updateCollapsedHeight();
    const observer = new ResizeObserver(() => {
      scheduleUpdate();
    });
    observer.observe(element);

    return () => {
      observer.disconnect();
      window.cancelAnimationFrame(animationFrameId);
    };
  }, [
    contentRef,
    projectSessionListLastVisibleSessionId,
    projectSessionListRenderedSessionIdsKey,
    shouldClipProjectSessionList,
  ]);
  const postGroupDebugLog = useEffectEvent((event: string, details: Record<string, unknown>) => {
    if (!debuggingMode) {
      return;
    }

    vscode.postMessage({
      details: {
        debugInstanceId: debugInstanceIdRef.current,
        groupId,
        ...details,
      },
      event,
      type: "sidebarDebugLog",
    });
  });
  /**
   * CDXC:PinnedSessions 2026-05-28-14:29:
   * Reference-sidebar pinned session dragging is a row-to-row reorder inside
   * one project. Do not let the project section itself accept session drags,
   * because the group drop surface competes with pinned row insertion lines
   * and creates flickering project-wide background feedback.
   */
  const sortable = useSortable({
    accept: allowPinnedSessionReorder ? "group" : ["group", "session"],
    collisionPriority: CollisionPriority.Low,
    data: createGroupDropData(groupId),
    disabled: isChatCollection || draggingDisabled,
    /**
     * CDXC:ProjectDragPreview 2026-05-21-11:45:
     * Project reordering uses an app-rendered cursor ghost instead of dnd-kit's
     * source-sized feedback. Expanded projects can contain many session rows,
     * so the default feedback makes the preview appear far from the cursor and
     * includes content that should stay out of the drag ghost.
     */
    feedback: projectContext ? "none" : "default",
    id: groupId,
    index,
    plugins: [SortableKeyboardPlugin],
    sensors: groupSensors,
    type: "group",
  });
  const setGroupSectionElement = useCallback(
    (element: HTMLElement | null) => {
      groupSectionRef.current = element;
      sortable.ref(element);
    },
    [sortable],
  );
  const emptyGroupDropTarget = useDroppable({
    accept: "session",
    data: createSessionDropTargetData({
      groupId,
      kind: "group",
      position: "start",
    }),
    disabled: isChatCollection || areSessionDropTargetsDisabled,
    id: createSessionDropTargetId({
      groupId,
      kind: "group",
      position: "start",
    }),
  });

  if (!group) {
    return null;
  }

  const groupSessions = orderedSessionIds
    .map((sessionId) => sessionsById[sessionId])
    .filter((session): session is NonNullable<typeof session> => session !== undefined);
  const shouldRenderPinnedSessionDropGaps =
    allowPinnedSessionReorder && showSessionDropPositionIndicators && orderedSessionIds.length > 0;
  const pinnedSessionDropGapKey = shouldRenderPinnedSessionDropGaps
    ? getPinnedSessionDropGapKey({
        dropTarget: pinnedSessionDropIndicator,
        groupId: group.groupId,
        visibleSessionIds,
      })
    : undefined;
  const visibleGroupSessions = visibleSessionIds
    .map((sessionId) => sessionsById[sessionId])
    .filter((session): session is NonNullable<typeof session> => session !== undefined);
  const visibleSessionIdSet = new Set(visibleSessionIds);
  const projectSessionListToggleLabel = isProjectSessionListCollapsed ? "Show more" : "Show less";
  const shouldScrubDisabledGroupDndAccessibility = isChatCollection || draggingDisabled;
  const sessionSummary = getGroupSessionSummary(groupSessions);
  const actualSessionCount = storedSessionIds.length;
  const allSessionsSleeping =
    groupSessions.length > 0 && groupSessions.every((session) => session.isSleeping);
  /**
   * CDXC:ProjectSleep 2026-05-27-06:28:
   * Sleep Inactive means awake plus idle/unknown activity, not "no live zmx
   * runtime." Live zmx-backed terminals should still be sleepable when they are
   * not working and not waiting for attention.
   */
  const hasInactiveProjectSessionsToSleep =
    Boolean(projectContext) &&
    groupSessions.some(
      (session) =>
        session.sessionKind === "terminal" &&
        session.isSleeping !== true &&
        session.activity !== "working" &&
        session.activity !== "attention",
    );
  const canFullReloadGroup = groupSessions.length > 0;
  const collapsedIndicatorActivity = sessionSummary.indicatorActivity;
  const hasCollapsedSummary = collapsedIndicatorActivity !== undefined;
  /**
   * CDXC:ProjectStatusIndicators 2026-05-08-09:33
   * Collapsed project headers must expose the hidden session status counts
   * inline with the project title: attention/done sessions stay #95d7f6 and
   * working sessions stay amber. Header actions replace this slot on hover.
   * CDXC:ProjectStatusIndicators 2026-05-08-10:48
   * Project-header status counts render in the visual order users scan for
   * active work: working count first, then attention count.
   */
  const shouldShowCollapsedProjectCounts =
    Boolean(projectContext) &&
    isCollapsed &&
    (sessionSummary.attentionCount > 0 || sessionSummary.workingCount > 0);
  const collapsedSummaryLabel = getCollapsedSummaryLabel(collapsedIndicatorActivity);
  const sessionsRegionId = `${group.groupId}-sessions`;
  const groupHeaderAnchorStyle = {
    anchorName: getGroupStatusAnchorName(group.groupId),
  } as CSSProperties;
  const projectThemeStyle = getProjectThemeStyle(projectContext?.themeColor);
  const groupHeaderStyle = projectThemeStyle
    ? ({ ...groupHeaderAnchorStyle, ...projectThemeStyle } as CSSProperties)
    : groupHeaderAnchorStyle;
  const sessionsShellStyle =
    shouldClipProjectSessionList && projectSessionListCollapsedHeight !== undefined
      ? ({
          ...(collapsibleStyle ?? {}),
          "--sidebar-collapse-content-height": `${projectSessionListCollapsedHeight}px`,
        } as CSSProperties)
      : collapsibleStyle;

  const isGroupDropTarget =
    sortable.isDropTarget ||
    emptyGroupDropTarget.isDropTarget ||
    (pinnedSessionDropIndicator === undefined && sessionDropIndicatorGroupId === groupId);
  /**
   * CDXC:ProjectReorder 2026-05-18-20:39:
   * Dragging a project in the reference sidebar must show a dim insertion line
   * where the project will land on pointer release. Keep the indicator on the
   * target project row instead of coloring the whole row so scanning remains
   * quiet during reorder.
   */
  const groupDropPosition =
    groupDropIndicator?.groupId === groupId ? groupDropIndicator.position : undefined;
  const isSessionDropTargetVisible = groupDropPosition === undefined && isGroupDropTarget;
  const showSessionGroupConnector = shouldShowSessionGroupConnector({
    sessions: groupSessions,
  });
  /*
   * CDXC:QuickSessions 2026-05-16-12:55:
   * The projectless chat collection remains modeled as Chats internally, but the empty reference-sidebar copy should read as Quick Sessions for users.
   */
  const emptyStateLabel = isChatCollection ? "No Quick Sessions" : "No sessions";
  const isEmptyProjectGroup =
    shouldTreatProjectAsEmptySessionGroup({
      hasProjectContext: Boolean(projectContext),
      sessionCount: actualSessionCount,
    });
  const shouldRenderOpenProjectFolderIcon = shouldShowOpenProjectFolderIcon({
    isCollapsed,
    sessionCount: actualSessionCount,
  });
  /**
   * CDXC:ProjectGroups 2026-05-15-14:33:
   * Project groups remain expandable even with no sessions because the body can
   * later receive project sessions. The sidebar no longer exposes an embedded
   * Code editor row or a project-header Code reveal button.
   * Non-project empty groups keep the old static header behavior.
   */
  const canToggleCollapsed = actualSessionCount > 0 || Boolean(projectContext);
  const groupTitleActionLabel =
    canToggleCollapsed ? `${isCollapsed ? "Expand" : "Collapse"} ${group.title}` : group.title;
  /**
   * CDXC:ProjectHeaders 2026-05-18-14:53:
   * Project row collapse/expand keeps an accessible label but no hover tooltip.
   * Project header clicks toggle the project session list rather than activating
   * the project; only the right-side action buttons keep their own click
   * behavior, and the folder icon is visual-only.
   *
   * CDXC:ProjectHeaderTooltips 2026-05-25-09:43:
   * Project header action buttons need compact hover labels without relying on
   * native title attributes.
   *
   * CDXC:ProjectHeaderTooltips 2026-05-29-18:19:
   * Project header action labels must open below their button, not to the left,
   * because left-side labels clip against the sidebar edge when the compact
   * action cluster is near the left side of the project header.
   *
   * CDXC:ProjectHeaderTooltips 2026-05-29-20:29:
   * Project header action labels must render through a fixed tooltip portal so
   * the next sticky project header cannot cover labels from the previous row.
   */
  const shouldSuppressProjectCollapseTooltip =
    Boolean(projectContext) && canToggleCollapsed;
  /*
   * CDXC:ProjectTitleTooltips 2026-05-30-07:33:
   * Hovering a project name should show a richer sidebar tooltip, not the
   * collapse/expand hint. The tooltip title uses brighter medium-weight text,
   * then shows factual project metadata: project kind, path, git file/+/- stats,
   * and the current session/worktree counts.
   */
  const projectTitleTooltip =
    projectContext && !isEditing ? (
      <ProjectTitleTooltip
        projectKindLabel={projectContext.worktree ? "Worktree project" : "Repository project"}
        projectPath={projectContext.path}
        sessionCount={actualSessionCount}
        stats={projectContext.editor.diffStats}
        title={group.title}
        worktreeCount={projectWorktreeCount}
      />
    ) : undefined;
  const createSessionTooltip = isChatCollection ? "Create a Chat" : "Create a Terminal";
  const primaryProjectAgent =
    agents.find((agent) => agent.agentId === primaryProjectAgentLauncherId) ?? agents[0];
  const primaryProjectAgentLabel = primaryProjectAgent?.name ?? "Agent";
  useEffect(() => {
    postGroupDebugLog("group.sectionMounted", {
      orderedSessionIds,
    });

    return () => {
      postGroupDebugLog("group.sectionUnmounted", {});
    };
  }, [postGroupDebugLog, orderedSessionIds]);

  useEffect(() => {
    postGroupDebugLog("group.dropStateChanged", {
      isGroupDropTarget,
      orderedSessionIds,
      sessionEmptyDropTarget: emptyGroupDropTarget.isDropTarget,
      sortableIsDropTarget: sortable.isDropTarget,
    });
  }, [
    emptyGroupDropTarget.isDropTarget,
    isGroupDropTarget,
    orderedSessionIds,
    postGroupDebugLog,
    sortable.isDropTarget,
  ]);

  useEffect(() => {
    if (!shouldScrubDisabledGroupDndAccessibility) {
      return;
    }

    const element = groupSectionRef.current;
    if (!element) {
      return;
    }

    const scrubDndAccessibilityAttributes = () => {
      for (const attribute of DISABLED_GROUP_DND_AX_ATTRIBUTES) {
        element.removeAttribute(attribute);
      }
    };

    scrubDndAccessibilityAttributes();

    const observer = new MutationObserver((mutations) => {
      if (
        mutations.some(
          (mutation) =>
            mutation.type === "attributes" &&
            mutation.attributeName !== null &&
            DISABLED_GROUP_DND_AX_ATTRIBUTES.includes(
              mutation.attributeName as (typeof DISABLED_GROUP_DND_AX_ATTRIBUTES)[number],
            ),
        )
      ) {
        window.queueMicrotask(scrubDndAccessibilityAttributes);
      }
    });

    observer.observe(element, {
      attributeFilter: [...DISABLED_GROUP_DND_AX_ATTRIBUTES],
      attributes: true,
    });

    return () => {
      observer.disconnect();
    };
  }, [shouldScrubDisabledGroupDndAccessibility]);

  useEffect(() => {
    if (isEditing) {
      return;
    }

    setDraftTitle(group.title);
  }, [group.title, isEditing]);

  useEffect(() => {
    if (!autoEdit) {
      return;
    }

    startTransition(() => {
      setDraftTitle(group.title);
      setIsEditing(true);
      onAutoEditHandled();
    });
  }, [autoEdit, group.title, onAutoEditHandled]);

  useEffect(() => {
    setContextMenuPosition(undefined);
    setOpenControlMenu(undefined);
  }, [group.groupId, group.title]);

  useEffect(() => {
    if (group.isActive) {
      return;
    }

    setOpenControlMenu(undefined);
  }, [group.isActive]);

  useEffect(() => {
    if (!openControlMenu) {
      return;
    }

    const handlePointerDown = (event: PointerEvent) => {
      const target = event.target;
      if (!(target instanceof Node)) {
        return;
      }

      if (
        controlMenuRef.current?.contains(target) ||
        projectAgentButtonRef.current?.contains(target)
      ) {
        return;
      }

      setOpenControlMenu(undefined);
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setOpenControlMenu(undefined);
      }
    };
    const handleBlur = () => {
      setOpenControlMenu(undefined);
    };
    const handleVisibilityChange = () => {
      if (document.visibilityState !== "visible") {
        setOpenControlMenu(undefined);
      }
    };

    document.addEventListener("pointerdown", handlePointerDown);
    document.addEventListener("keydown", handleKeyDown);
    document.addEventListener("visibilitychange", handleVisibilityChange);
    window.addEventListener("blur", handleBlur);

    return () => {
      document.removeEventListener("pointerdown", handlePointerDown);
      document.removeEventListener("keydown", handleKeyDown);
      document.removeEventListener("visibilitychange", handleVisibilityChange);
      window.removeEventListener("blur", handleBlur);
    };
  }, [openControlMenu]);

  const submitRename = () => {
    const nextTitle = draftTitle.trim();
    setIsEditing(false);
    setDraftTitle(nextTitle || group.title);

    if (!nextTitle || nextTitle === group.title) {
      return;
    }

    if (projectContext) {
      vscode.postMessage({
        groupId: group.groupId,
        title: nextTitle,
        type: "renameWorkspaceProjectForGroup",
      });
      return;
    }

    vscode.postMessage({ groupId: group.groupId, title: nextTitle, type: "renameGroup" });
  };

  const requestFocusGroup = () => {
    vscode.postMessage({
      groupId: group.groupId,
      type: "focusGroup",
    });
  };

  const requestCreateSession = () => {
    onCreateSessionRequested?.(group.groupId);

    vscode.postMessage({
      groupId: group.groupId,
      type: "createSessionInGroup",
    });
  };

  const persistPrimaryProjectAgentLauncher = (agentId: string) => {
    setPrimaryProjectAgentLauncherId(agentId);
    writePrimaryAgentLauncherId(agentId);
  };

  const toggleProjectSessionListCollapsed = () => {
    if (!projectSessionListStorageId) {
      return;
    }

    setProjectSessionListCollapsedState(() => {
      const latestState = readProjectSessionListCollapsedState();
      const nextState = { ...latestState };
      if (latestState[projectSessionListStorageId]) {
        delete nextState[projectSessionListStorageId];
      } else {
        nextState[projectSessionListStorageId] = true;
      }
      writeProjectSessionListCollapsedState(nextState);
      return nextState;
    });
  };

  const requestCreateProjectTerminal = () => {
    setOpenControlMenu(undefined);
    requestCreateSession();
  };

  const openWorktreeModal = () => {
    if (!projectContext) {
      return;
    }
    openAppModal({
      modal: "worktree",
      projectId: projectContext.editor.projectId,
      projectName: group.title,
      projectPath: projectContext.path,
      remoteMachineId: group.remoteMachineContext?.machineId,
      remoteMachineName: group.remoteMachineContext?.machineName,
      type: "open",
    });
  };

  const requestCreateWorktreePullRequest = () => {
    if (!projectContext?.worktree) {
      return;
    }
    vscode.postMessage({
      action: "pr",
      groupId: group.groupId,
      type: "runSidebarGitAction",
    });
  };

  const requestRunProjectAgent = (agent: SidebarAgentButton | undefined) => {
    setOpenControlMenu(undefined);
    if (!projectContext || !agent) {
      return;
    }
    persistPrimaryProjectAgentLauncher(agent.agentId);
    vscode.postMessage({
      agentId: agent.agentId,
      groupId: group.groupId,
      type: "runSidebarAgent",
    });
  };

  const openConfigureAgentsModal = () => {
    setOpenControlMenu(undefined);
    openAppModal({ modal: "configureAgents", type: "open" });
  };

  const requestCreateBrowserPane = () => {
    if (!projectContext) {
      return;
    }

    vscode.postMessage({
      groupId: group.groupId,
      type: "openBrowserPaneInGroup",
    });
  };

  const requestCloseGroup = () => {
    if (!canClose) {
      return;
    }

    setContextMenuPosition(undefined);
    if (orderedSessionIds.length <= 1) {
      vscode.postMessage({
        groupId: group.groupId,
        type: "closeGroup",
      });
      return;
    }

    setIsConfirmOpen(true);
  };

  const requestSetGroupSleeping = (sleeping: boolean) => {
    setContextMenuPosition(undefined);
    vscode.postMessage({
      groupId: group.groupId,
      sleeping,
      type: "setGroupSleeping",
    });
  };

  const requestSleepInactiveProjectSessions = () => {
    setContextMenuPosition(undefined);
    vscode.postMessage({
      groupId: group.groupId,
      type: "sleepInactiveProjectSessions",
    });
  };

  const requestCloseInactiveProjectSessions = () => {
    setContextMenuPosition(undefined);
    vscode.postMessage({
      groupId: group.groupId,
      type: "closeInactiveProjectSessions",
    });
  };

  const requestWakeProjectSleepingSessions = () => {
    setContextMenuPosition(undefined);
    vscode.postMessage({
      groupId: group.groupId,
      type: "wakeProjectSleepingSessions",
    });
  };

  const requestFullReloadGroup = () => {
    if (!canFullReloadGroup) {
      return;
    }

    setContextMenuPosition(undefined);
    vscode.postMessage({
      groupId: group.groupId,
      type: projectContext ? "fullReloadProjectZmxSessions" : "fullReloadGroup",
    });
  };

  const openProjectThemeMenu = () => {
    setContextMenuPosition((currentPosition) =>
      currentPosition ? { ...currentPosition, view: "project-themes" } : currentPosition,
    );
  };

  const openProjectCustomThemeMenu = () => {
    setCustomThemeColor(
      projectContext?.themeColor ?? recentThemeColors[0] ?? DEFAULT_WORKSPACE_THEME_COLOR,
    );
    setContextMenuPosition((currentPosition) =>
      currentPosition ? { ...currentPosition, view: "project-custom-theme" } : currentPosition,
    );
  };

  const openProjectRootMenu = () => {
    setContextMenuPosition((currentPosition) =>
      currentPosition ? { ...currentPosition, view: "group" } : currentPosition,
    );
  };

  const copyProjectPath = () => {
    setContextMenuPosition(undefined);
    vscode.postMessage({
      groupId: group.groupId,
      type: "copyWorkspaceProjectPathForGroup",
    });
  };

  const openProjectInFinder = () => {
    setContextMenuPosition(undefined);
    vscode.postMessage({
      groupId: group.groupId,
      type: "openWorkspaceProjectInFinderForGroup",
    });
  };

  const refreshProjectDiffStats = () => {
    if (!projectContext) {
      return;
    }
    vscode.postMessage({
      groupId: group.groupId,
      type: "refreshWorkspaceProjectDiffForGroup",
    });
  };

  const chooseProjectTheme = (theme: SidebarTheme) => {
    setContextMenuPosition(undefined);
    vscode.postMessage({
      groupId: group.groupId,
      theme,
      themeColor: null,
      type: "setWorkspaceProjectThemeForGroup",
    });
  };

  const chooseProjectThemeColor = (themeColor: string) => {
    const normalizedColor = normalizeWorkspaceThemeColor(themeColor);
    if (!normalizedColor) {
      return;
    }

    setContextMenuPosition(undefined);
    const nextRecentThemeColors = updateWorkspaceThemeColorHistory(
      recentThemeColors,
      normalizedColor,
    );
    setRecentThemeColors(nextRecentThemeColors);
    writeWorkspaceThemeColorHistory(nextRecentThemeColors);
    vscode.postMessage({
      groupId: group.groupId,
      themeColor: normalizedColor,
      type: "setWorkspaceProjectThemeForGroup",
    });
  };

  const closeProject = () => {
    if (!projectContext?.canRemoveProject) {
      return;
    }

    setContextMenuPosition(undefined);
    vscode.postMessage({
      groupId: group.groupId,
      type: "closeWorkspaceProjectForGroup",
    });
  };

  const removeWorktreeProject = () => {
    if (!projectContext?.worktree || !projectContext.canRemoveProject) {
      return;
    }

    setContextMenuPosition(undefined);
    vscode.postMessage({
      groupId: group.groupId,
      type: "removeWorkspaceProjectForGroup",
    });
  };

  const promptDeleteWorktree = () => {
    if (!projectContext?.worktree) {
      return;
    }

    setContextMenuPosition(undefined);
    vscode.postMessage({
      groupId: group.groupId,
      type: "promptDeleteWorktreeForGroup",
    });
  };

  const handleTitleKeyDown = (event: ReactKeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter") {
      event.preventDefault();
      submitRename();
      return;
    }

    if (event.key !== "Escape") {
      return;
    }

    event.preventDefault();
    setDraftTitle(group.title);
    setIsEditing(false);
  };

  const toggleCollapsed = () => {
    if (!canToggleCollapsed) {
      return;
    }

    onCollapsedChange(group.groupId, !isCollapsed);
  };

  const toggleCollapsedOrSelectEmptyProject = () => {
    if (!projectContext) {
      toggleCollapsed();
      return;
    }

    if (!canToggleCollapsed) {
      return;
    }

    const isExpandingEmptyProject = isCollapsed && isEmptyProjectGroup;
    onCollapsedChange(group.groupId, !isCollapsed);
    if (isExpandingEmptyProject) {
      requestCreateProjectTerminal();
    }
  };

  const handleGroupHeaderClick = (event: ReactMouseEvent<HTMLDivElement>) => {
    if (isEditing) {
      return;
    }

    if (event.target instanceof Element && event.target.closest(".group-header-actions")) {
      return;
    }

    if (projectContext) {
      event.preventDefault();
      event.stopPropagation();
      toggleCollapsedOrSelectEmptyProject();
      return;
    }

    event.preventDefault();
    event.stopPropagation();
    toggleCollapsedOrSelectEmptyProject();
  };

  return (
    <>
      <section
        className="group"
        data-active={String(group.isActive)}
        data-collapsed={String(isCollapsed)}
        data-dragging={String(Boolean(sortable.isDragging || isGroupDragPreviewSource))}
        data-group-drop-position={groupDropPosition}
        data-drop-target={String(isGroupDropTarget)}
        data-empty-space-blocking="true"
        data-empty-project={String(isEmptyProjectGroup)}
        data-project-group={String(Boolean(projectContext))}
        data-chat-collection={String(isChatCollection)}
        data-session-connector={String(showSessionGroupConnector)}
        data-sidebar-group-id={group.groupId}
        data-workspace-custom-theme={String(Boolean(projectContext?.themeColor))}
        aria-label={shouldScrubDisabledGroupDndAccessibility ? `${group.title} sessions` : undefined}
        onClick={() => {
          if (isCollapsed) {
            return;
          }

          requestFocusGroup();
        }}
        onContextMenu={(event: ReactMouseEvent<HTMLElement>) => {
          if (!projectContext && isNestedInteractiveContextMenuTarget(event)) {
            /**
             * CDXC:SidebarContextMenu 2026-05-15-17:53:
             * Header buttons without their own context menu should not open the
             * surrounding project/group context menu on right-click. Suppress
             * nested interactive targets while preserving right-click menus on
             * the row surface itself.
             *
             * CDXC:SidebarContextMenu 2026-05-16-13:39:
             * Project headers own a custom project context menu across their
             * whole header, including icon/title and action-button children.
             * Do not apply the nested-control suppression to project groups.
             */
            event.preventDefault();
            event.stopPropagation();
            return;
          }

          if (isChatCollection || (!showHeaderActions && !projectContext)) {
            return;
          }

          event.preventDefault();
          event.stopPropagation();
          setContextMenuPosition(
            clampContextMenuPosition(
              event.clientX,
              event.clientY,
              getGroupContextMenuItemCount({
                canFullReloadGroup,
                hasProjectContext: Boolean(projectContext),
                isWorktreeProject: Boolean(projectContext?.worktree),
              }),
            ),
          );
        }}
        ref={setGroupSectionElement}
        role={shouldScrubDisabledGroupDndAccessibility ? "group" : undefined}
      >
        <div
          className="group-head"
          data-collapsible="true"
          onClick={handleGroupHeaderClick}
          onMouseEnter={refreshProjectDiffStats}
          ref={projectContext && !isChatCollection ? sortable.handleRef : undefined}
          style={groupHeaderStyle}
        >
          <div className="group-title-wrap">
            {isEditing ? (
              <input
                autoFocus
                className="group-title-input"
                onBlur={submitRename}
                onChange={(event) => setDraftTitle(event.currentTarget.value)}
                onClick={(event) => event.stopPropagation()}
                onKeyDown={handleTitleKeyDown}
                value={draftTitle}
              />
            ) : (
              <div className="group-title-row">
                {projectContext ? (
                  <span
                    aria-hidden="true"
                    className="group-collapse-button section-titlebar-toggle"
                    data-collapsed={String(isCollapsed)}
                    data-empty-project={String(isEmptyProjectGroup)}
                    data-has-idle-icon={String(canToggleCollapsed)}
                    data-static-icon={String(!canToggleCollapsed)}
                  >
                    <span
                      aria-hidden="true"
                      className="group-collapse-icon group-collapse-idle-icon section-titlebar-toggle-icon section-titlebar-toggle-idle-icon"
                    >
                      {isChatCollection ? (
                        <IconMessageCircle size={16} stroke={1.8} />
                      ) : projectContext.worktree ? (
                        <IconGitBranch size={16} stroke={1.8} />
                      ) : shouldRenderOpenProjectFolderIcon ? (
                        <IconFolderOpen size={16} stroke={1.8} />
                      ) : (
                        <IconFolder size={16} stroke={1.8} />
                      )}
                    </span>
                    {canToggleCollapsed ? (
                      <IconCaretRightFilled
                        aria-hidden="true"
                        className="group-collapse-icon group-collapse-chevron-icon section-titlebar-toggle-icon section-titlebar-toggle-chevron-icon"
                        size={16}
                      />
                    ) : null}
                  </span>
                ) : (
                  <button
                    aria-controls={canToggleCollapsed && !isCollapsed ? sessionsRegionId : undefined}
                    aria-disabled={!canToggleCollapsed && !isEmptyProjectGroup}
                    aria-expanded={canToggleCollapsed ? !isCollapsed : undefined}
                    aria-label={groupTitleActionLabel}
                    className="group-collapse-button section-titlebar-toggle"
                    data-collapsed={String(isCollapsed)}
                    data-empty-project={String(isEmptyProjectGroup)}
                    data-has-idle-icon={String(canToggleCollapsed)}
                    data-static-icon={String(!canToggleCollapsed)}
                    onClick={(event) => {
                      event.preventDefault();
                      event.stopPropagation();
                      toggleCollapsedOrSelectEmptyProject();
                    }}
                    type="button"
                  >
                    <span
                      aria-hidden="true"
                      className="group-collapse-icon group-collapse-idle-icon section-titlebar-toggle-icon section-titlebar-toggle-idle-icon"
                    >
                      {isChatCollection ? (
                        <IconMessageCircle size={16} stroke={1.8} />
                      ) : shouldRenderOpenProjectFolderIcon ? (
                        <IconFolderOpen size={16} stroke={1.8} />
                      ) : (
                        <IconFolder size={16} stroke={1.8} />
                      )}
                    </span>
                    {canToggleCollapsed ? (
                      <IconCaretRightFilled
                        aria-hidden="true"
                        className="group-collapse-icon group-collapse-chevron-icon section-titlebar-toggle-icon section-titlebar-toggle-chevron-icon"
                        size={16}
                      />
                    ) : null}
                  </button>
                )}
                <div
                  className="group-title-handle"
                  data-draggable={String(!isChatCollection)}
                  ref={!projectContext && !isChatCollection ? sortable.handleRef : undefined}
                >
                  {shouldSuppressProjectCollapseTooltip ? (
                    <AppTooltip
                      content={projectTitleTooltip}
                      contentClassName="project-title-tooltip-content"
                    >
                      <button
                        aria-controls={
                          canToggleCollapsed && !isCollapsed ? sessionsRegionId : undefined
                        }
                        aria-disabled={!canToggleCollapsed && !isEmptyProjectGroup}
                        aria-expanded={canToggleCollapsed ? !isCollapsed : undefined}
                        aria-label={groupTitleActionLabel}
                        className="group-title-button"
                        data-empty-project={String(isEmptyProjectGroup)}
                        onClick={(event) => {
                          event.preventDefault();
                          event.stopPropagation();
                          toggleCollapsedOrSelectEmptyProject();
                        }}
                        type="button"
                      >
                        <span className="group-title section-titlebar-label">{group.title}</span>
                      </button>
                    </AppTooltip>
                  ) : (
                    <AppTooltip content={groupTitleActionLabel}>
                      <button
                        aria-controls={canToggleCollapsed && !isCollapsed ? sessionsRegionId : undefined}
                        aria-disabled={!canToggleCollapsed && !isEmptyProjectGroup}
                        aria-expanded={canToggleCollapsed ? !isCollapsed : undefined}
                        aria-label={groupTitleActionLabel}
                        className="group-title-button"
                        data-empty-project={String(isEmptyProjectGroup)}
                        onClick={(event) => {
                          event.preventDefault();
                          event.stopPropagation();
                          toggleCollapsedOrSelectEmptyProject();
                        }}
                        type="button"
                      >
                        <span className="group-title section-titlebar-label">{group.title}</span>
                      </button>
                    </AppTooltip>
                  )}
                </div>
                <div className="group-title-spacer" />
                {shouldShowCollapsedProjectCounts ? (
                  <div
                    aria-label={getCollapsedProjectCountsLabel(sessionSummary)}
                    className="group-collapsed-status-counts"
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
                  </div>
                ) : null}
                {/* CDXC:ProjectDiffStats 2026-05-16-08:46: Users can hide the project-header +added/-removed line summary entirely while keeping diff collection and action refresh behavior unchanged. */}
                {projectContext &&
                !hideProjectHeaderDiffStats &&
                !shouldShowCollapsedProjectCounts &&
                shouldShowProjectEditorDiffStats(projectContext.editor.diffStats) ? (
                  <ProjectHeaderDiffStats
                    showFileCount={showProjectEditorDiffFileCount}
                    stats={projectContext.editor.diffStats}
                  />
                ) : null}
                {showHeaderActions ? (
                  <div
                    className="group-header-actions"
                    data-open={String(openControlMenu !== undefined)}
                    onClick={(event) => {
                      event.preventDefault();
                      event.stopPropagation();
                    }}
                  >
                    {projectContext ? (
                      /**
                       * CDXC:ProjectGroups 2026-05-10-14:18
                       * Project headers expose a compact control family on
                       * every project row: browser pane creation, a separate
                       * terminal button, and an agent-only split launcher.
                       * Terminal creation is not an
                       * agent dropdown option so terminal and agent launches
                       * stay visually and behaviorally distinct.
                       *
                       * CDXC:ProjectGroups 2026-05-15-14:33:
                       * The sidebar no longer shows the Code editor row or a
                       * project-header Show Code Editor button. Embedded Code
                       * remains reachable through the native titlebar.
                       *
                       * CDXC:ProjectGroups 2026-05-18-14:53:
                       * Project header icon actions originally relied on
                       * accessible labels only so project rows stayed visually
                       * quiet while scanning and hovering.
                       *
                       * CDXC:ProjectHeaderTooltips 2026-05-25-09:43:
                       * Project header icon actions now show compact local
                       * tooltips without native title attributes, matching the
                       * settings/header hover-action surface.
                       *
                       * CDXC:ProjectHeaderTooltips 2026-05-29-18:19:
                       * Keep project header action tooltips below each hovered
                       * button when space allows and clamp the fixed tooltip
                       * portal so short labels remain visible inside narrow
                       * sidebar webviews.
                       *
                       * CDXC:Worktrees 2026-05-18-23:07:
                       * Main project rows expose Create Worktree. Worktree rows
                       * originally showed disabled PR and merge affordances until
                       * those follow-up actions were wired to real commands.
                       *
                       * CDXC:WorktreeMerge 2026-05-27-06:25:
                       * Worktree rows keep one Git affordance: Create PR opens the
                       * T3-style review flow for commit/push/PR, and that modal now
                       * owns the optional direct merge-to-main path so the header does
                       * not imply two competing worktree completion flows.
                       *
                       * CDXC:RemoteMachines 2026-06-09-19:02:
                       * Remote project headers must use the exact same project-header
                       * chrome as local project headers, but only expose the wired
                       * remote terminal creation action until remote browser/agent and
                       * worktree actions are implemented.
                       *
                       * CDXC:ProjectSessionLists 2026-06-10-13:39:
                       * Show more / Show less moved from the bottom of long project session lists into the project header action cluster. Keep it as an icon button with the same per-project collapsed-state storage, and only show it when the expanded project has more rows than the Settings-owned collapsed count.
                       */
                      <>
                        {shouldShowProjectSessionListToggle ? (
                          <ProjectHeaderActionButton
                            aria-label={
                              isProjectSessionListCollapsed
                                ? `Show more sessions in ${group.title}`
                                : `Show less sessions in ${group.title}`
                            }
                            className="group-add-button group-project-session-list-toggle-button"
                            onClick={(event) => {
                              event.preventDefault();
                              event.stopPropagation();
                              toggleProjectSessionListCollapsed();
                            }}
                            tooltip={projectSessionListToggleLabel}
                            type="button"
                          >
                            {isProjectSessionListCollapsed ? (
                              <IconChevronDown
                                aria-hidden="true"
                                className="group-add-icon"
                                size={14}
                                stroke={2}
                              />
                            ) : (
                              <IconChevronUp
                                aria-hidden="true"
                                className="group-add-icon"
                                size={14}
                                stroke={2}
                              />
                            )}
                          </ProjectHeaderActionButton>
                        ) : null}
                        {projectHeaderActions === "all" && projectContext.worktree ? (
                          <ProjectHeaderActionButton
                            aria-label={`Create PR for ${group.title}`}
                            className="group-add-button group-worktree-pr-button"
                            onClick={(event) => {
                              event.preventDefault();
                              event.stopPropagation();
                              requestCreateWorktreePullRequest();
                            }}
                            tooltip="Create PR"
                            type="button"
                          >
                            <IconGitPullRequest
                              aria-hidden="true"
                              className="group-add-icon"
                              size={14}
                              stroke={2}
                            />
                          </ProjectHeaderActionButton>
                        ) : projectHeaderActions === "all" ? (
                          <ProjectHeaderActionButton
                            aria-label={`Create a worktree from ${group.title}`}
                            className="group-add-button group-worktree-button"
                            onClick={(event) => {
                              event.preventDefault();
                              event.stopPropagation();
                              openWorktreeModal();
                            }}
                            tooltip="Add Worktree"
                            type="button"
                          >
                            <IconGitBranch
                              aria-hidden="true"
                              className="group-add-icon"
                              size={14}
                              stroke={2}
                            />
                          </ProjectHeaderActionButton>
                        ) : null}
                        {projectHeaderActions === "all" ? (
                          <ProjectHeaderActionButton
                            aria-label={`Create a browser tab in ${group.title}`}
                            className="group-add-button group-browser-button"
                            onClick={(event) => {
                              event.preventDefault();
                              event.stopPropagation();
                              requestCreateBrowserPane();
                            }}
                            tooltip="New Browser Tab"
                            type="button"
                          >
                            <IconWorld
                              aria-hidden="true"
                              className="group-add-icon"
                              size={14}
                              stroke={2}
                            />
                          </ProjectHeaderActionButton>
                        ) : null}
                        <ProjectHeaderActionButton
                          aria-label={`Create a terminal in ${group.title}`}
                          className="group-add-button group-project-terminal-button"
                          onClick={(event) => {
                            event.preventDefault();
                            event.stopPropagation();
                            requestCreateProjectTerminal();
                          }}
                          tooltip="Create Terminal"
                          type="button"
                        >
                          <IconTerminal2
                            aria-hidden="true"
                            className="group-add-icon"
                            size={14}
                            stroke={2}
                          />
                        </ProjectHeaderActionButton>
                        {projectHeaderActions === "all" ? (
                          <div className="group-control-anchor">
                            <div
                              className="group-agent-split-button"
                              data-open={String(openControlMenu === "project-agent")}
                            >
                              <ProjectHeaderActionButton
                                aria-label={`Create ${primaryProjectAgentLabel} in ${group.title}`}
                                className="group-agent-main-button"
                                onClick={(event) => {
                                  event.preventDefault();
                                  event.stopPropagation();
                                  if (!primaryProjectAgent) {
                                    openConfigureAgentsModal();
                                    return;
                                  }
                                  requestRunProjectAgent(primaryProjectAgent);
                                }}
                                tooltip={`Create ${primaryProjectAgentLabel}`}
                                type="button"
                              >
                                <ProjectAgentLauncherIcon agent={primaryProjectAgent} />
                              </ProjectHeaderActionButton>
                              <ProjectHeaderActionButton
                                aria-expanded={openControlMenu === "project-agent"}
                                aria-haspopup="menu"
                                aria-label={`Select agent for ${group.title}`}
                                className="group-agent-toggle-button"
                                data-open={String(openControlMenu === "project-agent")}
                                onClick={() => {
                                  setOpenControlMenu((previous) =>
                                    previous === "project-agent" ? undefined : "project-agent",
                                  );
                                }}
                                ref={projectAgentButtonRef}
                                tooltip="Select Agent"
                                type="button"
                              >
                                <IconChevronDown aria-hidden="true" size={13} stroke={2} />
                              </ProjectHeaderActionButton>
                            </div>
                          </div>
                        ) : null}
                      </>
                    ) : (
                      <>
                        <AppTooltip content={createSessionTooltip}>
                          <button
                            aria-label={
                              isChatCollection
                                  ? `Create a chat in ${group.title}`
                                  : `Create a session in ${group.title}`
                            }
                            className="group-add-button"
                            onClick={(event) => {
                              event.preventDefault();
                              event.stopPropagation();
                              requestCreateSession();
                            }}
                            type="button"
                          >
                            <IconPlus
                              aria-hidden="true"
                              className="group-add-icon"
                              size={14}
                              stroke={2}
                            />
                          </button>
                        </AppTooltip>
                      </>
                    )}
                  </div>
                ) : null}
              </div>
            )}
          </div>
        </div>
        {isCollapsed && !projectContext && hasCollapsedSummary ? (
          <div
            aria-label={collapsedSummaryLabel}
            className="group-collapsed-summary"
            data-activity={collapsedIndicatorActivity}
            style={getCollapsedGroupStatusStyle(group.groupId)}
          >
            <div aria-hidden className="session-status-dot" />
          </div>
        ) : null}
        <div
          aria-hidden={isCollapsed}
          className="group-sessions-shell sidebar-collapse-shell"
          data-collapsed={String(isCollapsed)}
          data-project-session-list-clipped={String(shouldClipProjectSessionList)}
          style={sessionsShellStyle}
        >
          <div
            className="group-sessions sidebar-collapse-content"
            data-pinned-drop-gaps={String(shouldRenderPinnedSessionDropGaps)}
            data-drop-target={String(isSessionDropTargetVisible)}
            id={sessionsRegionId}
            ref={contentRef}
          >
            {showSessionGroupConnector ? (
              <>
                <div aria-hidden className="group-session-connector-rail" />
                <button
                  aria-label={`Collapse ${group.title}`}
                  className="group-session-connector-button"
                  onClick={(event) => {
                    event.preventDefault();
                    event.stopPropagation();
                    toggleCollapsed();
                  }}
                  type="button"
                />
              </>
            ) : null}
            {orderedSessionIds.length > 0 ? (
              <>
                {renderedSessionIds.map((sessionId, sessionIndex) => {
                  const isProjectSessionListOverflowRow =
                    shouldClipProjectSessionList && !visibleSessionIdSet.has(sessionId);
                  const visibleSessionIndex = isProjectSessionListOverflowRow
                    ? -1
                    : visibleSessionIds.indexOf(sessionId);
                  const sessionIdsBelow =
                    visibleSessionIndex >= 0 ? visibleSessionIds.slice(visibleSessionIndex + 1) : [];

                  return (
                    <Fragment key={sessionId}>
                      {shouldRenderPinnedSessionDropGaps ? (
                        <div
                          aria-hidden
                          className="pinned-session-drop-gap"
                          data-active={String(
                            pinnedSessionDropGapKey === getSessionDropGapKeyBefore(sessionId),
                          )}
                          data-edge={sessionIndex === 0 ? "start" : undefined}
                        />
                      ) : null}
                      <SortableSessionCard
                        completionFlashNonce={completionFlashNonceBySessionId?.[sessionId] ?? 0}
                        dragDisabled={
                          isProjectSessionListOverflowRow ||
                          draggingDisabled ||
                          (sessionDraggingDisabled &&
                            !(
                              allowPinnedSessionReorder &&
                              sessionsById[sessionId]?.isPinned === true
                            ))
                        }
                        dropDisabled={
                          isProjectSessionListOverflowRow ||
                          draggingDisabled ||
                          (sessionDraggingDisabled && !allowPinnedSessionReorder)
                        }
                        groupId={group.groupId}
                        forcedDropPosition={
                          allowPinnedSessionReorder || isProjectSessionListOverflowRow
                            ? undefined
                            : pinnedSessionDropIndicator?.kind === "session" &&
                                pinnedSessionDropIndicator.groupId === group.groupId &&
                                pinnedSessionDropIndicator.sessionId === sessionId
                              ? pinnedSessionDropIndicator.position
                              : undefined
                        }
                        index={sessionIndex}
                        isProjectSessionListOverflowRow={isProjectSessionListOverflowRow}
                        isSearchSelected={
                          !isProjectSessionListOverflowRow &&
                          selectedSearchSessionId === sessionId
                        }
                        onFocusRequested={onFocusRequested}
                        sessionIdsBelow={sessionIdsBelow}
                        sessionId={sessionId}
                        showGroupDropTargetChrome={
                          !allowPinnedSessionReorder && !isProjectSessionListOverflowRow
                        }
                        showGroupConnector={
                          showSessionGroupConnector && !isProjectSessionListOverflowRow
                        }
                        showDropPositionIndicator={
                          showSessionDropPositionIndicators &&
                          !allowPinnedSessionReorder &&
                          !isProjectSessionListOverflowRow
                        }
                        vscode={vscode}
                      />
                    </Fragment>
                  );
                })}
                {shouldRenderPinnedSessionDropGaps ? (
                  <div
                    aria-hidden
                    className="pinned-session-drop-gap"
                    data-active={String(
                      pinnedSessionDropGapKey === PINNED_SESSION_DROP_GAP_AFTER_LAST,
                    )}
                  />
                ) : null}
              </>
            ) : isEmptyProjectGroup ? null : (
              <div
                className="group-empty-drop-target"
                data-drop-position={emptyGroupDropTarget.isDropTarget ? "start" : undefined}
                data-drop-target={String(isSessionDropTargetVisible)}
                ref={emptyGroupDropTarget.ref}
              >
                <div className="group-empty-state">{emptyStateLabel}</div>
              </div>
            )}
          </div>
          {showSessionGroupConnector
            ? visibleGroupSessions.map((session) => (
                <div
                  aria-hidden
                  className="session-status-dot session-status-dot-anchored"
                  data-activity={session.activity}
                  data-lifecycle-state={getSidebarSessionLifecycleState(session)}
                  key={`status-${session.sessionId}`}
                  style={getAnchoredSessionStatusStyle(session.sessionId)}
                />
              ))
            : null}
        </div>
      </section>
      {contextMenuPosition ? (
        <SidebarContextMenuPortal
          menuRef={menuRef}
          menuStyle={{
            left: `${contextMenuPosition.x}px`,
            top: `${contextMenuPosition.y}px`,
            width: `${CONTEXT_MENU_WIDTH_PX}px`,
          }}
          onDismiss={() => {
            setContextMenuPosition(undefined);
          }}
          vscode={vscode}
        >
              {projectContext ? (
                contextMenuPosition.view === "project-themes" ? (
                  <>
                    <button
                      className="session-context-menu-item"
                      onClick={openProjectRootMenu}
                      role="menuitem"
                      type="button"
                    >
                      <IconChevronLeft
                        aria-hidden="true"
                        className="session-context-menu-icon"
                        size={14}
                      />
                      Back
                    </button>
                    <div className="session-context-menu-divider" role="separator" />
                    <button
                      className="session-context-menu-item workspace-theme-menu-item"
                      data-selected={String(Boolean(projectContext.themeColor))}
                      onClick={openProjectCustomThemeMenu}
                      role="menuitemradio"
                      type="button"
                    >
                      <span
                        className="workspace-theme-swatch"
                        style={getProjectThemeSwatchStyle(
                          projectContext.themeColor ??
                            recentThemeColors[0] ??
                            DEFAULT_WORKSPACE_THEME_COLOR,
                        )}
                      />
                      Custom
                      <IconChevronRight
                        aria-hidden="true"
                        className="session-context-menu-trailing-icon"
                        size={14}
                      />
                    </button>
                    {PROJECT_CONTEXT_THEME_OPTIONS.map((theme) => (
                      <button
                        className="session-context-menu-item workspace-theme-menu-item"
                        data-selected={String(
                          !projectContext.themeColor && projectContext.theme === theme.value,
                        )}
                        key={theme.value}
                        onClick={() => chooseProjectTheme(theme.value)}
                        role="menuitemradio"
                        type="button"
                      >
                        <span
                          className="workspace-theme-swatch"
                          data-workspace-theme={theme.value}
                        />
                        {theme.label}
                      </button>
                    ))}
                  </>
                ) : contextMenuPosition.view === "project-custom-theme" ? (
                  <>
                    <button
                      className="session-context-menu-item"
                      onClick={openProjectThemeMenu}
                      role="menuitem"
                      type="button"
                    >
                      <IconChevronLeft
                        aria-hidden="true"
                        className="session-context-menu-icon"
                        size={14}
                      />
                      Back
                    </button>
                    <div className="session-context-menu-divider" role="separator" />
                    <div className="workspace-theme-custom-picker">
                      {/*
                       * CDXC:WorkspaceTheme 2026-05-05-02:58
                       * Combined-mode project headers own the Theme menu custom color picker after the far-left project list was removed. Applying a color posts a validated project theme color and records it in the local recent-color palette.
                       */}
                      <input
                        aria-label="Custom workspace theme color"
                        className="workspace-theme-color-input"
                        onChange={(event) => {
                          const normalizedColor = normalizeWorkspaceThemeColor(
                            event.currentTarget.value,
                          );
                          if (normalizedColor) {
                            setCustomThemeColor(normalizedColor);
                          }
                        }}
                        type="color"
                        value={customThemeColor}
                      />
                      <input
                        aria-label="Custom workspace theme color hex"
                        className="workspace-theme-color-text"
                        onChange={(event) => {
                          const normalizedColor = normalizeWorkspaceThemeColor(
                            event.currentTarget.value,
                          );
                          if (normalizedColor) {
                            setCustomThemeColor(normalizedColor);
                          }
                        }}
                        value={customThemeColor}
                      />
                      <button
                        aria-label="Apply custom workspace theme color"
                        className="workspace-theme-color-apply"
                        onClick={() => chooseProjectThemeColor(customThemeColor)}
                        type="button"
                      >
                        <IconCheck aria-hidden="true" size={14} stroke={2.2} />
                      </button>
                    </div>
                    {recentThemeColors.length > 0 ? (
                      <div className="workspace-theme-color-palette">
                        {recentThemeColors.map((themeColor) => (
                          <button
                            aria-label={`Use ${themeColor}`}
                            className="workspace-theme-color-palette-button"
                            key={themeColor}
                            onClick={() => chooseProjectThemeColor(themeColor)}
                            style={getProjectThemeSwatchStyle(themeColor)}
                            type="button"
                          />
                        ))}
                      </div>
                    ) : null}
                  </>
                ) : projectContext.worktree ? (
                  <>
                    {/*
                     * CDXC:WorktreeDelete 2026-05-28-07:46:
                     * Worktree project rows have their own compact context menu: open/reveal/rename first, then destructive worktree-specific actions. Delete removes the Git worktree checkout after confirmation; Remove only drops the Ghostex project row.
                     *
                     * CDXC:ProjectGroups 2026-06-04-13:39:
                     * Project and worktree filesystem menu items should say Open Folder instead of Finder-specific copy so the macOS app presents OS-agnostic action names.
                     *
                     * CDXC:ProjectGroups 2026-06-08-09:19:
                     * Worktree project headings should keep Copy Path but omit Open so the compact menu prioritizes filesystem copy/reveal and worktree-specific rename/delete/remove actions.
                     */}
                    <button
                      className="session-context-menu-item"
                      onClick={copyProjectPath}
                      role="menuitem"
                      type="button"
                    >
                      <IconCopy
                        aria-hidden="true"
                        className="session-context-menu-icon"
                        size={14}
                      />
                      Copy Path
                    </button>
                    <button
                      className="session-context-menu-item"
                      onClick={openProjectInFinder}
                      role="menuitem"
                      type="button"
                    >
                      <IconFolderOpen
                        aria-hidden="true"
                        className="session-context-menu-icon"
                        size={14}
                      />
                      Open Folder
                    </button>
                    <button
                      className="session-context-menu-item"
                      onClick={() => {
                        setContextMenuPosition(undefined);
                        setIsEditing(true);
                      }}
                      role="menuitem"
                      type="button"
                    >
                      <IconPencil
                        aria-hidden="true"
                        className="session-context-menu-icon"
                        size={14}
                      />
                      Rename
                    </button>
                    <div className="session-context-menu-divider" role="separator" />
                    <div aria-hidden="true" className="session-context-menu-spacer" />
                    <button
                      className="session-context-menu-item session-context-menu-item-danger"
                      onClick={promptDeleteWorktree}
                      role="menuitem"
                      type="button"
                    >
                      <IconTrash
                        aria-hidden="true"
                        className="session-context-menu-icon"
                        size={14}
                      />
                      Delete Worktree
                    </button>
                    <button
                      className="session-context-menu-item session-context-menu-item-danger"
                      disabled={!projectContext.canRemoveProject}
                      onClick={removeWorktreeProject}
                      role="menuitem"
                      type="button"
                    >
                      <IconX aria-hidden="true" className="session-context-menu-icon" size={14} />
                      Remove Worktree
                    </button>
                  </>
                ) : (
                  <>
                    {/*
                     * CDXC:ProjectGroups 2026-05-11-01:05
                     * Project group context menus expose filesystem actions
                     * first, then group lifecycle actions, and end with Close
                     * Project. Close Project parks the project in Recent
                     * Projects without deleting saved sessions.
                     * CDXC:ProjectSleep 2026-05-27-01:50:
                     * Project rows expose Sleep Inactive instead of a generic
                     * Sleep label because the action must preserve running,
                     * working, and attention sessions while sleeping inactive
                     * sessions across every workspace group in the project.
                     * CDXC:ProjectReload 2026-05-27-02:18:
                     * Project-row Wake and Full reload use project-scoped
                     * messages because the rendered row owns a synthetic group
                     * id. Full reload is intentionally narrower than group
                     * reload: native only reloads idle attached zmx terminals.
                     * CDXC:ProjectClose 2026-06-04-23:40:
                     * Project rows expose Close inactive directly above Close
                     * Project so users can remove idle project terminal
                     * sessions without parking the whole project in Recent
                     * Projects or interrupting working/attention sessions.
                     * CDXC:WorkspaceTheme 2026-05-09-17:18
                     * The Theme submenu is unused in the UI for now because
                     * theming has been disabled in this app for now. Keep the
                     * theme implementation available for a later re-enable, but
                     * hide its project right-click menu entry point.
                     */}
                    <button
                      className="session-context-menu-item"
                      onClick={copyProjectPath}
                      role="menuitem"
                      type="button"
                    >
                      <IconCopy
                        aria-hidden="true"
                        className="session-context-menu-icon"
                        size={14}
                      />
                      Copy Path
                    </button>
                    <button
                      className="session-context-menu-item"
                      onClick={openProjectInFinder}
                      role="menuitem"
                      type="button"
                    >
                      <IconFolderOpen
                        aria-hidden="true"
                        className="session-context-menu-icon"
                        size={14}
                      />
                      Open Folder
                    </button>
                    <div className="session-context-menu-divider" role="separator" />
                    <button
                      className="session-context-menu-item"
                      disabled={!allSessionsSleeping && !hasInactiveProjectSessionsToSleep}
                      onClick={() => {
                        if (allSessionsSleeping) {
                          requestWakeProjectSleepingSessions();
                          return;
                        }
                        requestSleepInactiveProjectSessions();
                      }}
                      role="menuitem"
                      type="button"
                    >
                      {allSessionsSleeping ? (
                        <IconPlayerPlay
                          aria-hidden="true"
                          className="session-context-menu-icon"
                          size={14}
                        />
                      ) : (
                        <IconMoon
                          aria-hidden="true"
                          className="session-context-menu-icon"
                          size={14}
                        />
                      )}
                      {allSessionsSleeping ? "Wake" : "Sleep Inactive"}
                    </button>
                    {canFullReloadGroup ? (
                      <button
                        className="session-context-menu-item"
                        onClick={requestFullReloadGroup}
                        role="menuitem"
                        type="button"
                      >
                        <IconRefresh
                          aria-hidden="true"
                          className="session-context-menu-icon"
                          size={14}
                        />
                        Full reload
                      </button>
                    ) : null}
                    <div className="session-context-menu-divider" role="separator" />
                    <button
                      className="session-context-menu-item session-context-menu-item-danger"
                      disabled={!hasInactiveProjectSessionsToSleep}
                      onClick={requestCloseInactiveProjectSessions}
                      role="menuitem"
                      type="button"
                    >
                      <IconX aria-hidden="true" className="session-context-menu-icon" size={14} />
                      Close inactive
                    </button>
                    <button
                      className="session-context-menu-item session-context-menu-item-danger"
                      disabled={!projectContext.canRemoveProject}
                      onClick={closeProject}
                      role="menuitem"
                      type="button"
                    >
                      <IconX aria-hidden="true" className="session-context-menu-icon" size={14} />
                      Close Project
                    </button>
                  </>
                )
              ) : (
                <>
                  <button
                    className="session-context-menu-item"
                    onClick={() => {
                      setContextMenuPosition(undefined);
                      setIsEditing(true);
                    }}
                    role="menuitem"
                    type="button"
                  >
                    <IconPencil
                      aria-hidden="true"
                      className="session-context-menu-icon"
                      size={14}
                    />
                    Rename
                  </button>
                  {canFullReloadGroup ? (
                    <button
                      className="session-context-menu-item"
                      onClick={requestFullReloadGroup}
                      role="menuitem"
                      type="button"
                    >
                      <IconRefresh
                        aria-hidden="true"
                        className="session-context-menu-icon"
                        size={14}
                      />
                      Full reload
                    </button>
                  ) : null}
                  <button
                    className="session-context-menu-item"
                    onClick={() => requestSetGroupSleeping(!allSessionsSleeping)}
                    role="menuitem"
                    type="button"
                  >
                    {allSessionsSleeping ? (
                      <IconPlayerPlay
                        aria-hidden="true"
                        className="session-context-menu-icon"
                        size={14}
                      />
                    ) : (
                      <IconMoon
                        aria-hidden="true"
                        className="session-context-menu-icon"
                        size={14}
                      />
                    )}
                    {allSessionsSleeping ? "Wake" : "Sleep"}
                  </button>
                  <div className="session-context-menu-divider" role="separator" />
                  <button
                    className="session-context-menu-item session-context-menu-item-danger"
                    disabled={!canClose}
                    onClick={requestCloseGroup}
                    role="menuitem"
                    type="button"
                  >
                    <IconX aria-hidden="true" className="session-context-menu-icon" size={14} />
                    Close
                  </button>
                </>
              )}
        </SidebarContextMenuPortal>
      ) : null}
      {projectContext && openControlMenu === "project-agent"
        ? createPortal(
            <div
              className="group-control-menu session-context-menu group-agent-menu"
              onClick={(event) => event.stopPropagation()}
              ref={controlMenuRef}
              role="menu"
              style={getPortalMenuStyle(projectAgentButtonRef.current, GROUP_AGENT_MENU_WIDTH_PX)}
            >
              {agents.map((agent) => (
                <button
                  aria-pressed={primaryProjectAgent?.agentId === agent.agentId}
                  className="session-context-menu-item group-control-menu-item group-agent-menu-item"
                  data-selected={String(primaryProjectAgent?.agentId === agent.agentId)}
                  key={agent.agentId}
                  onClick={() => requestRunProjectAgent(agent)}
                  role="menuitem"
                  type="button"
                >
                  <ProjectAgentLauncherIcon agent={agent} colorMode="brand" />
                  <span className="group-agent-menu-label">{agent.name}</span>
                  {primaryProjectAgent?.agentId === agent.agentId ? (
                    <IconCheck aria-hidden="true" className="session-context-menu-icon" size={14} />
                  ) : null}
                </button>
              ))}
              {agents.length > 0 ? (
                <div className="session-context-menu-divider" role="separator" />
              ) : null}
              <button
                className="session-context-menu-item group-control-menu-item group-agent-menu-item"
                onClick={openConfigureAgentsModal}
                role="menuitem"
                type="button"
              >
                <IconSettings aria-hidden="true" className="session-context-menu-icon" size={14} />
                <span className="group-agent-menu-label">Configure</span>
              </button>
            </div>,
            document.body,
          )
        : null}
      {/**
       * CDXC:SessionClose 2026-05-11-00:45
       * Group close confirmation copy must use Close so bulk session removal
       * matches the session context menu and does not expose
       * process-lifecycle wording to users.
       */}
      <ConfirmationModal
        confirmLabel="Close Group"
        description={`This will close all ${orderedSessionIds.length} session${orderedSessionIds.length === 1 ? "" : "s"} in ${group.title}.`}
        isOpen={isConfirmOpen}
        onCancel={() => setIsConfirmOpen(false)}
        onConfirm={() => {
          setIsConfirmOpen(false);
          vscode.postMessage({
            groupId: group.groupId,
            type: "closeGroup",
          });
        }}
        title="Close group?"
      />
    </>
  );
}

function getPortalMenuStyle(button: HTMLButtonElement | null, menuWidth: number) {
  const position = getControlMenuPosition(button);
  const bounds = button?.getBoundingClientRect();
  if (!position) {
    return undefined;
  }

  const left = Math.max(
    GROUP_CONTROL_MENU_MARGIN_PX,
    Math.min(
      (bounds?.right ?? position.x) - menuWidth,
      window.innerWidth - menuWidth - GROUP_CONTROL_MENU_MARGIN_PX,
    ),
  );

  return {
    left: `${left}px`,
    position: "fixed" as const,
    top: `${position.y}px`,
    width: `${menuWidth}px`,
  };
}

let sessionGroupDebugInstanceCounter = 0;

function createSessionGroupDebugInstanceId(): number {
  sessionGroupDebugInstanceCounter += 1;
  return sessionGroupDebugInstanceCounter;
}

function getCollapsedSummaryLabel(
  indicatorActivity: "attention" | "working" | undefined,
): string | undefined {
  if (indicatorActivity === "attention") {
    return "Group has completed sessions";
  }

  if (indicatorActivity === "working") {
    return "Group has working sessions";
  }

  return undefined;
}

function getCollapsedProjectCountsLabel(
  summary: Pick<GroupSessionSummary, "attentionCount" | "workingCount">,
): string {
  return [
    summary.workingCount > 0 ? `${summary.workingCount} working` : "",
    summary.attentionCount > 0 ? `${summary.attentionCount} attention` : "",
  ]
    .filter(Boolean)
    .join(", ");
}
