import {
  IconAlertTriangle,
  IconCaretRightFilled,
  IconCheck,
  IconChevronDown,
  IconChevronLeft,
  IconChevronRight,
  IconChevronUp,
  IconCopy,
  IconFolder,
  IconFolderOpen,
  IconEyeOff,
  IconGitBranch,
  IconGitPullRequest,
  IconLink,
  IconMessageCircle,
  IconMoon,
  IconPencil,
  IconPlayerPlay,
  IconPlus,
  IconRefresh,
  IconSettings,
  IconStack,
  IconTerminal2,
  IconTrash,
  IconWorld,
  IconX,
} from '@tabler/icons-react';
import { CollisionPriority } from '@dnd-kit/abstract';
import { PointerSensor } from '@dnd-kit/dom';
import { useDroppable } from '@dnd-kit/react';
import { useSortable } from '@dnd-kit/react/sortable';
import {
  Fragment,
  startTransition,
  useCallback,
  useLayoutEffect,
  useEffect,
  useEffectEvent,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent as ReactKeyboardEvent,
  type MouseEvent as ReactMouseEvent,
  type WheelEvent as ReactWheelEvent,
} from 'react';
import { useShallow } from 'zustand/react/shallow';
import { AppTooltip } from './app-tooltip';
import { AgentMenuChatIndicator } from './agent-menu-chat-indicator';
import { SidebarProjectIcon } from './sidebar-project-icon';
import {
  getSidebarSessionLifecycleState,
  type SidebarSessionItem,
  type SidebarTheme,
} from '../shared/session-grid-contract';
import type { SidebarProjectDiffStats } from '../shared/project-diff-stats';
import type { SidebarAgentButton } from '../shared/sidebar-agents';
import type { SidebarCommandButton, SidebarCommandScope } from '../shared/sidebar-commands';
import { DEFAULT_SIDEBAR_COMMAND_ICON } from '../shared/sidebar-command-icons';
import { SidebarCommandIconGlyph } from './sidebar-command-icon';
import { DEFAULT_ghostex_SETTINGS, clampProjectSessionListCollapsedCount } from '../shared/ghostex-settings';
import type { SidebarSessionTagListItem } from '../shared/session-tags';
import { ConfirmationModal } from './confirmation-modal';
import {
  createGroupDropData,
  createSessionDropTargetData,
  createSessionDropTargetId,
  type SidebarGroupDropTarget,
  type SidebarSessionDropTarget,
} from './sidebar-dnd';
import {
  getAwakeTerminalAndBrowserCount,
  getGroupSessionSummary,
  type GroupSessionSummary,
} from './group-session-summary';
import { shouldShowSessionGroupConnector } from './session-group-connector';
import { getGroupStatusAnchorName, getSessionStatusAnchorName } from './session-status-anchor';
import { useSidebarStore } from './sidebar-store';
import {
  type SidebarSessionSelectionChangeRequest,
  SortableSessionCard,
  type SortableSessionCardSharedSettings,
} from './sortable-session-card';
import { SidebarContextMenuPortal } from './sidebar-context-menu-portal';
import { resolveSidebarSpaceIcon } from './space-filter-row';
import { createRemoteSidebarSpaceSectionKey, LOCAL_SIDEBAR_SPACE_SECTION_KEY } from './sidebar-app/space-filtering';
import { getSidebarSpaceIdsContainingProject, type SidebarSpacesState } from './spaces';
import { useCollapsibleHeight } from './use-collapsible-height';
import { useSidebarCollapsiblePresence } from './sidebar-collapse-animation';
import type { WebviewApi } from './webview-api';
import { openAppModal } from './app-modal-host-bridge';
import {
  getExpandedProjectSessionListScrollHeight,
  getProjectSessionListCollapsedHeight,
  getVisibleProjectSessionIds,
  type ProjectSessionListCollapsedState,
} from './project-session-list-toggle';
import {
  DEFAULT_WORKSPACE_THEME_COLOR,
  normalizeWorkspaceThemeColor,
  updateWorkspaceThemeColorHistory,
} from '../shared/workspace-project-appearance';
import { readWorkspaceThemeColorHistory, writeWorkspaceThemeColorHistory } from './workspace-theme-color-history';
import { SidebarFixedTooltipButton } from './sidebar-fixed-tooltip-button';
import {
  PRIMARY_AGENT_LAUNCHER_CHANGED_EVENT,
  readPrimaryAgentLauncherId,
  writePrimaryAgentLauncherId,
  type PrimaryAgentLauncherChangedEvent,
} from './primary-agent-launcher';
import { ProjectAgentLauncherIcon } from './project-agent-launcher-icon';
import { getSidebarReorderActivationConstraints } from './sidebar-reorder-activation';
import { useSidebarItemTooltipDelayMs } from './tooltip-delay';

const CONTEXT_MENU_MARGIN_PX = 12;
const CONTEXT_MENU_WIDTH_PX = 196;
const CONTEXT_MENU_ITEM_HEIGHT_PX = 34;
const CONTEXT_MENU_VERTICAL_PADDING_PX = 12;
const GROUP_CONTROL_MENU_MARGIN_PX = 12;
const GROUP_AGENT_MENU_WIDTH_PX = 220;
/**
 * CDXC:SidebarReorderActivation 2026-08-08:
 * Project headers define the shared sidebar reorder gesture. Its timing and
 * distance constraints now live in `sidebar-reorder-activation.ts` so session,
 * collection, and machine rows cannot silently drift to a slower gesture.
 */
const PROJECT_EDITOR_DISPLAY_MAX_FILES = 99;
const EMPTY_PROJECT_NEW_SESSION_LABEL = 'New Session';
const DISABLED_GROUP_DND_AX_ATTRIBUTES = [
  'aria-describedby',
  'aria-disabled',
  'aria-grabbed',
  'aria-pressed',
  'aria-roledescription',
  'tabindex',
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
  dragSurface: Element | null | undefined
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
  return typeof Element !== 'undefined' && target instanceof Element;
}
/**
 * CDXC:ProjectDiffStats 2026-05-27-10:44:
 * Cap git +/− line counts shown in project headers at four digits so very large
 * diffs stay readable in the sidebar without widening the status label.
 */
const PROJECT_EDITOR_DISPLAY_MAX_LINES = 9999;
const PROJECT_CONTEXT_THEME_OPTIONS: ReadonlyArray<{ label: string; value: SidebarTheme }> = [
  /**
   * CDXC:SidebarTheme 2026-06-15-01:43:
   * Workspace/project theme menus expose the same app-level Dark 1, Dark 2,
   * and Light choices as Settings so project chrome can persist the new
   * default or the previous dark snapshot explicitly.
   */
  { label: 'Dark 1', value: 'dark-1' },
  { label: 'Dark 2', value: 'dark-2' },
  { label: 'Light', value: 'plain-light' },
  { label: 'Dark Green', value: 'dark-green' },
  { label: 'Dark Blue', value: 'dark-blue' },
  { label: 'Dark Red', value: 'dark-red' },
  { label: 'Dark Pink', value: 'dark-pink' },
  { label: 'Dark Orange', value: 'dark-orange' },
  { label: 'Light Blue', value: 'light-blue' },
  { label: 'Light Green', value: 'light-green' },
  { label: 'Light Pink', value: 'light-pink' },
  { label: 'Light Orange', value: 'light-orange' },
];

function getAnchoredSessionStatusStyle(sessionId: string): CSSProperties {
  return {
    left: 'anchor(right)',
    positionAnchor: getSessionStatusAnchorName(sessionId),
    top: 'anchor(center)',
  } as CSSProperties;
}

function getCollapsedGroupStatusStyle(groupId: string): CSSProperties {
  return {
    left: 'anchor(right)',
    positionAnchor: getGroupStatusAnchorName(groupId),
    top: 'anchor(center)',
  } as CSSProperties;
}

/**
 * CDXC:WorkspaceTheme 2026-05-05-02:58
 * Combined-mode project headers consume the persisted workspace theme color
 * through one CSS variable so titles and hover surfaces can share the same tint
 * without changing chat or browser group styling.
 */
function getProjectThemeStyle(themeColor: string | undefined): CSSProperties | undefined {
  if (!themeColor) {
    return undefined;
  }

  return {
    '--workspace-project-theme-color': themeColor,
  } as CSSProperties;
}

function getProjectThemeSwatchStyle(themeColor: string | undefined): CSSProperties | undefined {
  if (!themeColor) {
    return undefined;
  }

  return {
    '--workspace-theme-swatch-background': themeColor,
  } as CSSProperties;
}

/**
 * CDXC:SidebarV2GroupedProjectUX 2026-07-30:
 * Exported so Sidebar V2's grouped project headers also inherit V1's control
 * blocking and deliberate absence of a KeyboardSensor. The pointer timing and
 * distance rules themselves live in `sidebar-reorder-activation.ts`, shared by
 * every sidebar reorder surface.
 */
export const groupSensors = [
  PointerSensor.configure({
    activationConstraints: getSidebarReorderActivationConstraints,
    preventActivation(event, source) {
      return shouldPreventGroupDragActivation(event.target, source.handle ?? source.element);
    },
  }),
  /*
   * CDXC:CollectionReorder 2026-07-21:
   * No KeyboardSensor: Space/Enter on the focusable group head started an
   * invisible keyboard drag (project groups use feedback "none"), and an
   * uncommitted keyboard drag leaves the shared dnd manager stuck non-idle,
   * silently disabling every pointer drag in the sidebar.
   */
];

type ContextMenuPosition = {
  x: number;
  y: number;
};

type GroupContextMenuPosition = ContextMenuPosition & {
  view: 'group' | 'project-collections' | 'project-custom-theme' | 'project-spaces' | 'project-themes';
};

type GroupControlMenu = 'project-agent';

/**
 * CDXC:ProjectHeaderTooltips 2026-06-25-15:48:
 * Project-header action labels share SidebarFixedTooltipButton so project, section, and footer/sidebar hover actions all use one fixed popup that avoids section overflow and Recent Projects clipping.
 */
const ProjectHeaderActionButton = SidebarFixedTooltipButton;

export function shouldTreatProjectAsEmptySessionGroup({
  hasProjectContext,
  sessionCount,
}: {
  hasProjectContext: boolean;
  sessionCount: number;
}): boolean {
  /**
   * CDXC:ProjectGroups 2026-06-15-20:14:
   * Empty project groups should stay visible after their last terminal is closed
   * and render an explicit New Session row in the body. The project header
   * remains only an expand/collapse target so first-terminal creation always
   * comes from the session-shaped button.
   */
  return hasProjectContext && sessionCount === 0;
}

export function getEmptyProjectNewSessionButtonLabel(): string {
  return EMPTY_PROJECT_NEW_SESSION_LABEL;
}

export const PINNED_SESSION_DROP_GAP_AFTER_LAST = 'after-last';

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

  if (dropTarget.kind === 'group') {
    return dropTarget.position === 'start'
      ? getSessionDropGapKeyBefore(visibleSessionIds[0])
      : PINNED_SESSION_DROP_GAP_AFTER_LAST;
  }

  const targetIndex = visibleSessionIds.indexOf(dropTarget.sessionId);
  if (targetIndex < 0) {
    return undefined;
  }

  if (dropTarget.position === 'before') {
    return getSessionDropGapKeyBefore(dropTarget.sessionId);
  }

  const nextSessionId = visibleSessionIds[targetIndex + 1];
  return nextSessionId ? getSessionDropGapKeyBefore(nextSessionId) : PINNED_SESSION_DROP_GAP_AFTER_LAST;
}

export function formatProjectEditorDiffStatsLabel(stats: SidebarProjectDiffStats, showFileCount = false): string {
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
    .join(' ');
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

function ProjectHeaderDiffStats({ showFileCount, stats }: { showFileCount: boolean; stats: SidebarProjectDiffStats }) {
  return (
    <div
      aria-label={`Git changes: ${formatProjectEditorDiffStatsLabel(stats, showFileCount)}`}
      className='group-project-diff-stats'
    >
      {showFileCount ? (
        <span className='group-project-diff-files'>{formatProjectEditorFilesCount(stats.files)}</span>
      ) : null}
      <span className='group-project-diff-stat group-project-diff-stat-additions'>
        +{formatProjectEditorLineCount(stats.additions)}
      </span>
      <span className='group-project-diff-stat group-project-diff-stat-deletions'>
        -{formatProjectEditorLineCount(stats.deletions)}
      </span>
    </div>
  );
}

export function formatProjectTooltipGitStats(stats: SidebarProjectDiffStats): string {
  if (stats.isLoading) {
    return 'Git: loading changes';
  }

  if (!stats.isRepo) {
    return 'Git: not a repository';
  }

  const fileCount = Math.max(0, stats.files);
  const changedLineCount = Math.max(0, stats.additions) + Math.max(0, stats.deletions);
  /**
   * CDXC:ProjectDiffStats 2026-06-14-16:33:
   * Project and worktree title tooltips should spell out the file and line
   * nouns so one changed file or one changed line reads as singular while the
   * compact inline diff badge can remain numeric-only.
   */
  return `${fileCount} ${formatCountLabel(fileCount, 'file')} changed  +${formatProjectEditorLineCount(
    stats.additions
  )}  -${formatProjectEditorLineCount(stats.deletions)} ${formatCountLabel(changedLineCount, 'line')}`;
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
    <div className='project-title-tooltip'>
      <div className='project-title-tooltip-heading'>{title}</div>
      <div className='project-title-tooltip-body'>
        <div>{projectKindLabel}</div>
        <div className='project-title-tooltip-path'>{projectPath}</div>
        <div>{formatProjectTooltipGitStats(stats)}</div>
        <div>
          {sessionCount} {formatCountLabel(sessionCount, 'session')} · {worktreeCount}{' '}
          {formatCountLabel(worktreeCount, 'worktree')}
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
  isHidden?: boolean;
  onAutoEditHandled: () => void;
  onCollapsedChange: (groupId: string, collapsed: boolean) => void;
  onCreateSessionRequested?: (groupId: string) => void;
  onFocusRequested?: (groupId: string, sessionId: string) => void;
  onCreateProjectCollection?: (projectId: string) => void;
  onMoveProjectToCollection?: (projectId: string, collectionId: string | undefined) => void;
  onProjectSessionListCollapsedChange?: (projectId: string, collapsed: boolean) => void;
  /*
   * CDXC:SidebarSpaces 2026-08-27:
   * Space membership for an UNGROUPED project row. Per the Spaces decision, a
   * project inside a group cannot be assigned directly — it inherits its group's
   * Spaces — and a worktree can never be assigned at all, so this entry renders
   * only on ordinary ungrouped project rows.
   *
   * `spaces` is the owning gxserver's Space set (`undefined` = that daemon is
   * Space-incapable, so no entry at all), and `spaceMemberProjectId` is the
   * project id in THAT daemon's own id space: the local editor project id, or a
   * remote machine's raw project id.
   */
  onToggleSpaceMembership?: (spaceId: string) => void;
  spaceMemberProjectId?: string;
  spaces?: SidebarSpacesState;
  onHideGroup?: () => void;
  onSessionSelectionChange?: (request: SidebarSessionSelectionChangeRequest) => void;
  orderedSessionIds?: readonly string[];
  selectedSearchSessionId?: string;
  selectedSessionIds?: readonly string[];
  allowPinnedSessionReorder?: boolean;
  enableProjectSessionListToggle?: boolean;
  pinnedSessionDropIndicator?: SidebarSessionDropTarget;
  sessionDropIndicator?: SidebarSessionDropTarget;
  sessionDraggingDisabled?: boolean;
  projectHeaderActions?: 'all' | 'terminal-only';
  projectCollectionId?: string;
  projectCollectionOptions?: readonly { collectionId: string; color: string; title: string }[];
  projectSessionListCollapsedState?: Readonly<ProjectSessionListCollapsedState>;
  /**
   * CDXC:SidebarBrowserTabReveal 2026-08-18:
   * A host reveal request for one session row. The kind sections inside a
   * project group (Browser / Pinned / Sessions) are this component's own state,
   * so revealing a row it owns has to reach it as a request instead of being
   * decided by SidebarApp.
   */
  revealSessionRequest?: { requestId: number; sessionId: string };
  sessionTagListItems?: readonly SidebarSessionTagListItem[];
  showHeaderActions?: boolean;
  showSessionDropPositionIndicators?: boolean;
  useColoredAgentIcons?: boolean;
  vscode: WebviewApi;
};

type ProjectSessionSection = 'browser' | 'pinned' | 'sessions' | 'parked';

const EXPANDED_PROJECT_SESSION_SECTIONS: Readonly<Record<ProjectSessionSection, boolean>> = {
  browser: false,
  pinned: false,
  sessions: false,
  parked: true,
};

function getProjectSessionSection(
  session: SidebarSessionItem | undefined,
  enableSessionParking: boolean
): ProjectSessionSection {
  if (session?.kind === 'browser' || session?.sessionKind === 'browser') {
    return 'browser';
  }
  if (enableSessionParking && session?.isParked === true) {
    return 'parked';
  }
  return session?.isPinned === true ? 'pinned' : 'sessions';
}

function ProjectSessionSectionToggle({
  count,
  isCollapsed,
  label,
  onToggle,
}: {
  count: number;
  isCollapsed: boolean;
  label: string;
  onToggle: () => void;
}) {
  return (
    <button
      aria-expanded={!isCollapsed}
      aria-label={`${isCollapsed ? 'Expand' : 'Collapse'} ${label}`}
      className='session-kind-toggle'
      onClick={(event) => {
        event.preventDefault();
        event.stopPropagation();
        onToggle();
      }}
      type='button'
    >
      <span>
        {label}
        {isCollapsed ? ` ⋅ ${count}` : null}
      </span>
      <IconChevronRight
        aria-hidden='true'
        className='session-kind-toggle-chevron'
        data-expanded={String(!isCollapsed)}
        size={12}
        stroke={2}
      />
    </button>
  );
}

function clampContextMenuPosition(clientX: number, clientY: number, itemCount: number): GroupContextMenuPosition {
  const menuHeight = CONTEXT_MENU_VERTICAL_PADDING_PX + itemCount * CONTEXT_MENU_ITEM_HEIGHT_PX;
  return {
    view: 'group',
    x: Math.max(
      CONTEXT_MENU_MARGIN_PX,
      Math.min(clientX, window.innerWidth - CONTEXT_MENU_WIDTH_PX - CONTEXT_MENU_MARGIN_PX)
    ),
    y: Math.max(CONTEXT_MENU_MARGIN_PX, Math.min(clientY, window.innerHeight - menuHeight - CONTEXT_MENU_MARGIN_PX)),
  };
}

export function getGroupContextMenuItemCount({
  canCreateSessionGroup = false,
  canFullReloadGroup,
  canHideGroup = false,
  canCopyProjectRemoteUrl = false,
  hasProjectContext,
  isWorktreeProject,
  projectCollectionsEnabled = false,
  spacesEnabled = false,
}: {
  canCreateSessionGroup?: boolean;
  canFullReloadGroup: boolean;
  canHideGroup?: boolean;
  canCopyProjectRemoteUrl?: boolean;
  hasProjectContext: boolean;
  isWorktreeProject: boolean;
  projectCollectionsEnabled?: boolean;
  spacesEnabled?: boolean;
}): number {
  /*
   * CDXC:ProjectGroups 2026-06-08-09:19:
   * Worktree project headings should expose Copy Path but omit the IDE Open action in their compact context menu. Keep the root context-menu item count explicit by project kind so viewport clamping stays aligned with the visible worktree and repository menu actions.
   *
   * CDXC:WorktreeRename 2026-08-10:
   * Rename Worktree replaced the dead label-only Rename on worktree rows rather
   * than joining it, so the count is unchanged. It drives viewport clamping, so
   * it has to move with the menu or the last item opens off-screen.
   *
   * CDXC:SidebarContextMenu 2026-08-10:
   * Hide/Unhide renders in both project menus whenever `onHideGroup` is supplied
   * — which the sidebar always does — and was never counted, so every project
   * menu was measured one row short and the last item could open off-screen. It
   * is not part of the group menu, so only the project branches take it.
   *
   * CDXC:SidebarSpaces 2026-08-27:
   * The Spaces submenu entry renders only on ORDINARY ungrouped project rows —
   * grouped projects inherit their group's Spaces and worktrees can never be
   * assigned — so it is counted only in the non-worktree project branch, and the
   * caller passes the same condition the menu itself renders on. Miscounting
   * here does not hide the row, it opens the menu at the wrong y and pushes the
   * last item off-screen, which is why it moves with the menu.
   */
  if (hasProjectContext) {
    return (
      Number(canHideGroup) +
      Number(canCopyProjectRemoteUrl) +
      (isWorktreeProject
        ? 5 + Number(projectCollectionsEnabled)
        : 5 +
          Number(canFullReloadGroup) +
          Number(canCreateSessionGroup) +
          Number(projectCollectionsEnabled) +
          Number(spacesEnabled))
    );
  }

  return 3 + Number(canFullReloadGroup);
}

export type SidebarSessionGapContextMenuCandidate<T> = {
  bottom: number;
  element: T;
  top: number;
};

export function getSidebarSessionGapContextMenuTarget<T>({
  clientY,
  sessionRows,
}: {
  clientY: number;
  sessionRows: readonly SidebarSessionGapContextMenuCandidate<T>[];
}): T | undefined {
  /*
   * CDXC:SidebarContextMenu 2026-06-19-10:46:
   * Project context menus are owned by the project header only. A right-click in
   * the narrow visual gap between two sidebar session rows belongs to the
   * session directly above the gap, preserving row-level actions without adding
   * overlapping hit targets.
   */
  if (!Number.isFinite(clientY)) {
    return undefined;
  }

  for (let index = 0; index < sessionRows.length - 1; index += 1) {
    const currentRow = sessionRows[index];
    const nextRow = sessionRows[index + 1];
    if (!Number.isFinite(currentRow.bottom) || !Number.isFinite(nextRow.top) || nextRow.top < currentRow.bottom) {
      continue;
    }
    if (clientY >= currentRow.bottom && clientY <= nextRow.top) {
      return currentRow.element;
    }
  }

  return undefined;
}

function getControlMenuPosition(button: HTMLButtonElement | null): ContextMenuPosition | undefined {
  if (!button) {
    return undefined;
  }

  const bounds = button.getBoundingClientRect();
  return {
    x: Math.max(
      GROUP_CONTROL_MENU_MARGIN_PX,
      Math.min(bounds.left + bounds.width / 2, window.innerWidth - GROUP_CONTROL_MENU_MARGIN_PX)
    ),
    y: Math.max(
      GROUP_CONTROL_MENU_MARGIN_PX,
      Math.min(bounds.bottom + 6, window.innerHeight - GROUP_CONTROL_MENU_MARGIN_PX)
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
  isHidden = false,
  onAutoEditHandled,
  onCollapsedChange,
  onCreateSessionRequested,
  onFocusRequested,
  onCreateProjectCollection,
  onMoveProjectToCollection,
  onProjectSessionListCollapsedChange,
  onToggleSpaceMembership,
  spaceMemberProjectId,
  spaces,
  onHideGroup,
  onSessionSelectionChange,
  orderedSessionIds: orderedSessionIdsProp,
  selectedSearchSessionId,
  selectedSessionIds,
  allowPinnedSessionReorder = false,
  enableProjectSessionListToggle = true,
  pinnedSessionDropIndicator,
  projectHeaderActions = 'all',
  projectCollectionId,
  projectCollectionOptions = [],
  projectSessionListCollapsedState = {},
  revealSessionRequest,
  sessionDropIndicator,
  sessionDraggingDisabled = false,
  sessionTagListItems,
  showHeaderActions = true,
  showSessionDropPositionIndicators = true,
  useColoredAgentIcons = false,
  vscode,
}: SessionGroupSectionProps) {
  const sidebarItemTooltipDelayMs = useSidebarItemTooltipDelayMs();
  const group = useSidebarStore((state) => state.groupsById[groupId]);
  const storedSessionIds = useSidebarStore((state) => state.sessionIdsByGroup[groupId] ?? []);
  const sessionsById = useSidebarStore((state) => state.sessionsById);
  const containsActiveSession =
    group?.isActive === true && storedSessionIds.some((sessionId) => sessionsById[sessionId]?.isFocused === true);
  const projectWorktreeCount = useSidebarStore((state) => {
    const projectId = state.groupsById[groupId]?.projectContext?.editor.projectId;
    if (!projectId) {
      return 0;
    }

    return Object.values(state.groupsById).filter(
      (candidate) => candidate?.projectContext?.worktree?.parentProjectId === projectId
    ).length;
  });
  /*
   * CDXC:ProjectActions 2026-08-01:
   * Project rows render only the Actions the user flagged showOnProjectRow,
   * read from the HUD's per-project block so every row shows its own project's
   * Actions instead of the active project's. Hosts that do not serve
   * commandsByProject (legacy macOS) leave the map empty and rows render no
   * extra buttons.
   */
  const projectCommands = useSidebarStore((state) => {
    const projectId = state.groupsById[groupId]?.projectContext?.editor.projectId;
    if (!projectId) {
      return undefined;
    }
    return state.hud.commandsByProject?.[projectId];
  });
  /*
   * CDXC:GlobalActions 2026-08-07:
   * Global Actions live in their own daemon-owned list, never in
   * commandsByProject, so reading only the per-project block made the row
   * toggle dead for them. Merge the two lists here and keep each button's
   * scope beside it: the scopes are separate id spaces, so the click needs to
   * say which list its id came from, and React needs a key that cannot
   * collide when both lists happen to hold the same id.
   *
   * Globals render first, matching Settings > Actions, which also lists Global
   * Actions above the project's own. That order also keeps a global button at
   * the same spot on every row, since only the project part varies per row.
   *
   * CDXC:GlobalActions 2026-08-29:
   * The list is owned by the daemon the row's project lives on, so a row on a
   * remote machine reads that machine's entry instead of the local daemon's
   * `globalCommands`. Hosts that serve one daemon never set the remote map and
   * every row keeps reading the single list.
   */
  const globalCommands = useSidebarStore((state) => {
    const remoteMachineId = state.groupsById[groupId]?.remoteMachineContext?.machineId;
    return remoteMachineId ? state.hud.remoteGlobalCommandsByMachineId?.[remoteMachineId] : state.hud.globalCommands;
  });
  const projectRowCommands = useMemo(
    () =>
      [
        ...(globalCommands ?? []).map((command) => ({ command, scope: 'global' }) as const),
        ...(projectCommands ?? []).map((command) => ({ command, scope: 'project' }) as const),
      ].filter((entry) => entry.command.showOnProjectRow),
    [globalCommands, projectCommands]
  );
  const orderedSessionIds = orderedSessionIdsProp ?? storedSessionIds;
  const [contextMenuPosition, setContextMenuPosition] = useState<GroupContextMenuPosition>();
  const [customThemeColor, setCustomThemeColor] = useState(DEFAULT_WORKSPACE_THEME_COLOR);
  const [recentThemeColors, setRecentThemeColors] = useState(readWorkspaceThemeColorHistory);
  const [draftTitle, setDraftTitle] = useState(group?.title ?? '');
  const [isConfirmOpen, setIsConfirmOpen] = useState(false);
  const [isEditing, setIsEditing] = useState(false);
  const [openControlMenu, setOpenControlMenu] = useState<GroupControlMenu>();
  const [primaryProjectAgentLauncherId, setPrimaryProjectAgentLauncherId] = useState(readPrimaryAgentLauncherId);
  const [collapsedProjectSessionSections, setCollapsedProjectSessionSections] = useState(
    EXPANDED_PROJECT_SESSION_SECTIONS
  );
  const [projectSessionListCollapsedHeight, setProjectSessionListCollapsedHeight] = useState<number>();
  const { collapsibleStyle, contentRef, setContentElement } = useCollapsibleHeight<HTMLDivElement>();
  const menuRef = useRef<HTMLDivElement>(null);
  const controlMenuRef = useRef<HTMLDivElement>(null);
  const projectAgentButtonRef = useRef<HTMLButtonElement>(null);
  const projectTitleButtonRef = useRef<HTMLButtonElement>(null);
  const groupTitleInputRef = useRef<HTMLInputElement>(null);
  const groupSectionRef = useRef<HTMLElement | null>(null);
  const handledRevealSessionRequestIdRef = useRef<number | undefined>(undefined);
  const sessionsShellRef = useRef<HTMLDivElement | null>(null);
  const debugInstanceIdRef = useRef(createSessionGroupDebugInstanceId());

  /*
   * CDXC:SidebarScroll 2026-07-23:
   * Boundary scroll handoff for the inner project session scroller. Both this
   * shell and the main sidebar scroller use `overscroll-behavior: none` (no
   * rubber-banding per explicit user request), but `none` also kills native
   * scroll chaining, and CSS cannot express "chain but don't bounce". So when
   * a wheel gesture hits this scroller's top or bottom edge, hand the delta to
   * the enclosing `.session-groups-content` sidebar scroller manually. No
   * preventDefault needed: at the boundary the browser's default action is
   * already nothing.
   */
  const handleSessionsShellWheel = useCallback((event: ReactWheelEvent<HTMLDivElement>) => {
    const shell = event.currentTarget;
    if (shell.dataset.projectSessionListScrollable !== 'true') {
      return;
    }
    const deltaY = event.deltaY;
    // Ignore horizontal-dominant trackpad gestures so they never nudge the sidebar.
    if (deltaY === 0 || Math.abs(deltaY) < Math.abs(event.deltaX)) {
      return;
    }
    const atTop = shell.scrollTop <= 0;
    const atBottom = shell.scrollTop + shell.clientHeight >= shell.scrollHeight - 1;
    if (deltaY < 0 ? !atTop : !atBottom) {
      return;
    }
    const sidebarScroller = shell.closest<HTMLElement>('.session-groups-content');
    if (sidebarScroller) {
      sidebarScroller.scrollTop += deltaY;
    }
  }, []);

  useEffect(() => {
    const refreshPrimaryAgentLauncher = (event: Event) => {
      const changedEvent = event as PrimaryAgentLauncherChangedEvent;
      setPrimaryProjectAgentLauncherId(
        typeof changedEvent.detail?.agentId === 'string' ? changedEvent.detail.agentId : readPrimaryAgentLauncherId()
      );
    };

    window.addEventListener(PRIMARY_AGENT_LAUNCHER_CHANGED_EVENT, refreshPrimaryAgentLauncher);
    return () => {
      window.removeEventListener(PRIMARY_AGENT_LAUNCHER_CHANGED_EVENT, refreshPrimaryAgentLauncher);
    };
  }, []);

  /**
   * CDXC:ProjectHeaders 2026-06-29-21:04:
   * Project and worktree group headers should not render the old leading folder
   * or branch glyph. The full header still owns activation/collapse, while the
   * synthetic Chats collection keeps its chat glyph so it stays visually distinct.
   *
   * CDXC:Chats 2026-05-04-09:41
   * The Combined-mode Chats header is a synthetic collection, not one mutable
   * project group. It can create new chat folders, but it must not accept
   * session drops, group dragging, or project/group context-menu mutations.
   */
  const isChatCollection = group?.isChatCollection === true;
  const projectContext = group?.projectContext;
  const projectGitRemoteOriginUrl = projectContext?.gitRemoteOriginUrl?.trim();
  const rawProjectSessionListStorageId = projectContext?.editor.projectId ?? group?.groupId;
  const projectSessionListStorageId =
    rawProjectSessionListStorageId && group?.remoteMachineContext?.machineId
      ? `remote:${group.remoteMachineContext.machineId}:${rawProjectSessionListStorageId}`
      : rawProjectSessionListStorageId;
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
  const projectAgentLauncherIconColorMode = useColoredAgentIcons ? 'brand' : 'monochrome';
  const hideProjectHeaderDiffStats = useSidebarStore(
    (state) => state.hud.settings?.hideProjectHeaderDiffStats ?? DEFAULT_ghostex_SETTINGS.hideProjectHeaderDiffStats
  );
  const showProjectEditorDiffFileCount = useSidebarStore(
    (state) =>
      state.hud.settings?.showProjectEditorDiffFileCount ?? DEFAULT_ghostex_SETTINGS.showProjectEditorDiffFileCount
  );
  const showProjectIcons = useSidebarStore(
    (state) => state.hud.settings?.showProjectIcons ?? DEFAULT_ghostex_SETTINGS.showProjectIcons
  );
  const enableSessionParking = useSidebarStore(
    (state) => state.hud.settings?.enableSessionParking ?? DEFAULT_ghostex_SETTINGS.enableSessionParking
  );
  /*
   * CDXC:DisabledPluginRouting 2026-08-23:
   * Turning Browser off in Settings → Extensions removes the Browser workarea,
   * so the project header's New Browser Tab button has nowhere left to put a
   * tab. Hide it rather than leaving a control whose only outcome is a refusal.
   */
  const browserViewEnabled = useSidebarStore(
    (state) => (state.hud.settings?.browserViewTabHidden ?? DEFAULT_ghostex_SETTINGS.browserViewTabHidden) !== true
  );
  const projectSessionListCollapsedCount = useSidebarStore((state) =>
    clampProjectSessionListCollapsedCount(
      state.hud.settings?.projectSessionListCollapsedCount ?? DEFAULT_ghostex_SETTINGS.projectSessionListCollapsedCount
    )
  );
  /*
   * CDXC:SidebarScroll 2026-06-30-02:45:
   * Large project lists can render many fixed-height session rows while manual
   * drag ordering stays mounted. Read global card settings once per group and
   * pass them into rows so each card keeps only session-specific store work.
   */
  const sessionCardSettings = useSidebarStore(
    useShallow((state): SortableSessionCardSharedSettings => ({
      /*
       * CDXC:BrowserPanes 2026-05-28-07:38:
       * Browser favicons identify pages and need their own hover-only setting
       * instead of being suppressed by the agent-logo hover preference.
       */
      hideBrowserFaviconUntilHover:
        state.hud.settings?.hideBrowserFaviconUntilHover ?? DEFAULT_ghostex_SETTINGS.hideBrowserFaviconUntilHover,
      /*
       * CDXC:SidebarSessions 2026-05-16-08:46:
       * The hover-only agent icon setting is visual chrome only; keep icons in
       * the DOM so the same row can reveal them on hover/focus without
       * changing session identity or drag hit targets.
       */
      hideSessionAgentIconUntilHover:
        state.hud.settings?.hideSessionAgentIconUntilHover ?? DEFAULT_ghostex_SETTINGS.hideSessionAgentIconUntilHover,
      renameSessionOnDoubleClick:
        state.hud.settings?.renameSessionOnDoubleClick ?? state.hud.renameSessionOnDoubleClick,
      showCloseButton: state.hud.showCloseButtonOnSessionCards,
      showDebugSessionNumbers: state.hud.debuggingMode,
      enableSessionParking: state.hud.settings?.enableSessionParking ?? DEFAULT_ghostex_SETTINGS.enableSessionParking,
      showLastActiveTime: !(
        state.hud.settings?.hideLastActiveTimeOnSessionCards ??
        DEFAULT_ghostex_SETTINGS.hideLastActiveTimeOnSessionCards
      ),
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
        state.hud.settings?.showSessionCommandCopyActions ?? DEFAULT_ghostex_SETTINGS.showSessionCommandCopyActions,
      /*
       * CDXC:SidebarContextMenu 2026-06-11-23:08:
       * Copy details is an opt-in metadata clipboard action. Gate the menu item
       * with its own Settings flag instead of tying it to shell command copying.
       */
      showSessionDetailsCopyAction:
        state.hud.settings?.showSessionDetailsCopyAction ?? DEFAULT_ghostex_SETTINGS.showSessionDetailsCopyAction,
    }))
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
  const renderedSessionIds =
    shouldShowProjectSessionListToggle && !isProjectSessionListCollapsed ? orderedSessionIds : visibleSessionIds;
  const renderedBrowserSessionIds = renderedSessionIds.filter((sessionId) => {
    return getProjectSessionSection(sessionsById[sessionId], enableSessionParking) === 'browser';
  });
  const renderedPinnedSessionIds = renderedSessionIds.filter((sessionId) => {
    return getProjectSessionSection(sessionsById[sessionId], enableSessionParking) === 'pinned';
  });
  const renderedUnpinnedSessionIds = renderedSessionIds.filter((sessionId) => {
    return getProjectSessionSection(sessionsById[sessionId], enableSessionParking) === 'sessions';
  });
  const renderedParkedSessionIds = renderedSessionIds.filter((sessionId) => {
    return getProjectSessionSection(sessionsById[sessionId], enableSessionParking) === 'parked';
  });
  const projectSessionSectionCounts = orderedSessionIds.reduce<Record<ProjectSessionSection, number>>(
    (counts, sessionId) => {
      counts[getProjectSessionSection(sessionsById[sessionId], enableSessionParking)] += 1;
      return counts;
    },
    { browser: 0, parked: 0, pinned: 0, sessions: 0 }
  );
  const shouldRenderSessionKindLabels =
    renderedBrowserSessionIds.length > 0 && renderedBrowserSessionIds.length < renderedSessionIds.length;
  const firstBrowserSessionId = renderedBrowserSessionIds[0];
  const firstPinnedSessionId = renderedPinnedSessionIds[0];
  const firstUnpinnedSessionId = renderedUnpinnedSessionIds[0];
  const firstParkedSessionId = renderedParkedSessionIds[0];
  const firstTerminalSessionId = renderedSessionIds.find((sessionId) => {
    const session = sessionsById[sessionId];
    return session?.kind !== 'browser' && session?.sessionKind !== 'browser';
  });
  const toggleProjectSessionSection = (section: ProjectSessionSection) => {
    setCollapsedProjectSessionSections((previous) => ({
      ...previous,
      [section]: !previous[section],
    }));
  };
  /*
   * CDXC:SidebarBrowserTabReveal 2026-08-18:
   * A revealed row is useless inside a collapsed kind section, so open the one
   * that owns it (Browser for a new Browser tab) and leave it open — the user
   * can close it again, and nothing re-collapses it behind their back.
   */
  useEffect(() => {
    if (!revealSessionRequest || handledRevealSessionRequestIdRef.current === revealSessionRequest.requestId) {
      return;
    }
    const revealedSession = sessionsById[revealSessionRequest.sessionId];
    if (!revealedSession || !orderedSessionIds.includes(revealSessionRequest.sessionId)) {
      return;
    }
    handledRevealSessionRequestIdRef.current = revealSessionRequest.requestId;
    const revealedSection = getProjectSessionSection(revealedSession, enableSessionParking);
    setCollapsedProjectSessionSections((previous) =>
      previous[revealedSection] ? { ...previous, [revealedSection]: false } : previous
    );
  }, [enableSessionParking, orderedSessionIds, revealSessionRequest, sessionsById]);
  const expandedVisibleSessionIds = projectContext
    ? visibleSessionIds.filter(
        (sessionId) =>
          !collapsedProjectSessionSections[getProjectSessionSection(sessionsById[sessionId], enableSessionParking)]
      )
    : visibleSessionIds;
  const projectSessionListLastVisibleSessionId =
    expandedVisibleSessionIds.length > 0 ? expandedVisibleSessionIds[expandedVisibleSessionIds.length - 1] : undefined;
  const shouldClipProjectSessionList = shouldShowProjectSessionListToggle && isProjectSessionListCollapsed;
  const shouldScrollExpandedProjectSessionList = shouldShowProjectSessionListToggle && !isProjectSessionListCollapsed;
  /*
   * CDXC:SidebarScroll 2026-06-30-02:45:
   * Expanded projects may contain hundreds of rows. Only build the DOM-measure
   * dependency key for the collapsed clipped state, where the visible row set is
   * capped by the Show less count.
   */
  const projectSessionListRenderedSessionIdsKey = shouldClipProjectSessionList
    ? expandedVisibleSessionIds.join('\u0000')
    : '';
  const expandedProjectSessionListScrollHeight = shouldScrollExpandedProjectSessionList
    ? getExpandedProjectSessionListScrollHeight({
        rowCount: Math.min(projectSessionListCollapsedCount, orderedSessionIds.length),
      })
    : undefined;

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
        })
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
      scenarioId: 'native.pane.reorder',
      type: 'sidebarDebugLog',
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
    accept: allowPinnedSessionReorder ? 'group' : ['group', 'session'],
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
    feedback: projectContext ? 'none' : 'default',
    id: groupId,
    index,
    sensors: groupSensors,
    type: 'group',
  });
  const setGroupSectionElement = useCallback(
    (element: HTMLElement | null) => {
      groupSectionRef.current = element;
      sortable.ref(element);
    },
    [sortable]
  );
  const emptyGroupDropTarget = useDroppable({
    accept: 'session',
    data: createSessionDropTargetData({
      groupId,
      kind: 'group',
      position: 'start',
    }),
    disabled: isChatCollection || areSessionDropTargetsDisabled,
    id: createSessionDropTargetId({
      groupId,
      kind: 'group',
      position: 'start',
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
  const projectSessionListHiddenCount = shouldClipProjectSessionList
    ? Math.max(0, orderedSessionIds.length - visibleSessionIds.length)
    : 0;
  const projectSessionListToggleLabel = isProjectSessionListCollapsed ? 'Show more' : 'Show less';
  const shouldScrubDisabledGroupDndAccessibility = isChatCollection || draggingDisabled;
  const sessionSummary = getGroupSessionSummary(groupSessions);
  const awakeCount = getAwakeTerminalAndBrowserCount(groupSessions);
  const actualSessionCount = storedSessionIds.length;
  const allSessionsSleeping = groupSessions.length > 0 && groupSessions.every((session) => session.isSleeping);
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
        session.sessionKind === 'terminal' &&
        session.isSleeping !== true &&
        session.activity !== 'working' &&
        session.activity !== 'attention'
    );
  const canFullReloadGroup = groupSessions.length > 0;
  /*
   * CDXC:SidebarSpaces 2026-08-27:
   * The exact condition the Spaces entry renders on, so the menu-height budget
   * and the menu itself can never disagree: an ordinary (non-worktree) project
   * row that is NOT inside a group, on a Space-capable gxserver.
   */
  const isProjectSpacesMenuEnabled = Boolean(
    projectContext &&
    !projectContext.worktree &&
    projectCollectionId === undefined &&
    spaceMemberProjectId &&
    spaces &&
    onToggleSpaceMembership
  );
  const projectMemberSpaceIds =
    spaces && spaceMemberProjectId ? getSidebarSpaceIdsContainingProject(spaces, spaceMemberProjectId) : [];
  const collapsedIndicatorActivity = sessionSummary.indicatorActivity;
  const hasCollapsedSummary = collapsedIndicatorActivity !== undefined;
  /**
   * CDXC:ProjectStatusIndicators 2026-05-08-09:33
   * Collapsed project headers must expose the hidden session status counts
   * inline with the project title: attention/done sessions stay #95d7f6 and
   * working sessions stay amber. Header actions replace this slot on hover.
   * CDXC:ProjectStatusIndicators 2026-05-08-10:48
   * Project-header status counts render in the visual order users scan for
   * active work: working count first, then attention count. When neither action
   * state exists, the same collapsed-only slot shows awake terminals/browsers.
   */
  const shouldShowCollapsedProjectCounts =
    Boolean(projectContext) &&
    isCollapsed &&
    (sessionSummary.attentionCount > 0 || sessionSummary.workingCount > 0 || awakeCount > 0);
  const hasProjectActionStatus = sessionSummary.attentionCount > 0 || sessionSummary.workingCount > 0;
  const collapsedSummaryLabel = getCollapsedSummaryLabel(collapsedIndicatorActivity);
  const sessionsRegionId = `${group.groupId}-sessions`;
  const groupHeaderAnchorStyle = {
    anchorName: getGroupStatusAnchorName(group.groupId),
  } as CSSProperties;
  const projectThemeStyle = getProjectThemeStyle(projectContext?.themeColor);
  const groupHeaderStyle = projectThemeStyle
    ? ({ ...groupHeaderAnchorStyle, ...projectThemeStyle } as CSSProperties)
    : groupHeaderAnchorStyle;
  const hasExpandedProjectSessionListScrollHeight =
    shouldScrollExpandedProjectSessionList && expandedProjectSessionListScrollHeight !== undefined;
  const sessionsShellStyle =
    (shouldClipProjectSessionList && projectSessionListCollapsedHeight !== undefined) ||
    hasExpandedProjectSessionListScrollHeight
      ? ({
          ...(collapsibleStyle ?? {}),
          ...(shouldClipProjectSessionList && projectSessionListCollapsedHeight !== undefined
            ? { '--sidebar-collapse-content-height': `${projectSessionListCollapsedHeight}px` }
            : {}),
          ...(hasExpandedProjectSessionListScrollHeight
            ? {
                '--project-session-list-scroll-height': `${expandedProjectSessionListScrollHeight}px`,
              }
            : {}),
        } as CSSProperties)
      : collapsibleStyle;

  const sessionGroupDropPosition =
    sessionDropIndicator?.kind === 'group' && sessionDropIndicator.groupId === groupId
      ? sessionDropIndicator.position
      : undefined;
  const isGroupDropTarget =
    sortable.isDropTarget || emptyGroupDropTarget.isDropTarget || sessionGroupDropPosition !== undefined;
  /**
   * CDXC:ProjectReorder 2026-05-18-20:39:
   * Dragging a project in the reference sidebar must show a dim insertion line
   * where the project will land on pointer release. Keep the indicator on the
   * target project row instead of coloring the whole row so scanning remains
   * quiet during reorder.
   */
  const groupDropPosition = groupDropIndicator?.groupId === groupId ? groupDropIndicator.position : undefined;
  const isSessionDropTargetVisible = groupDropPosition === undefined && isGroupDropTarget;
  const showSessionGroupConnector = shouldShowSessionGroupConnector({
    sessions: groupSessions,
  });
  /*
   * CDXC:QuickSessions 2026-05-16-12:55:
   * The projectless chat collection remains modeled as Chats internally, but the empty reference-sidebar copy should read as Quick Sessions for users.
   */
  const emptyStateLabel = isChatCollection ? 'No Quick Sessions' : 'No sessions';
  const isEmptyProjectGroup = shouldTreatProjectAsEmptySessionGroup({
    hasProjectContext: Boolean(projectContext),
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
  /*
   * CDXC:SidebarPerformance 2026-06-28-05:39:
   * Collapsed project groups must be header-only work. Do not mount hidden
   * session rows, row dnd hooks, row observers, or sticky-body scroll
   * measurement while the user has collapsed a project.
   */
  const {
    isPresent: shouldRenderGroupSessionsBody,
    isVisuallyCollapsed: isGroupSessionsBodyVisuallyCollapsed,
    setCollapsibleElement: setGroupSessionsBodyElement,
  } = useSidebarCollapsiblePresence(isCollapsed);
  const setSessionsShellElement = useCallback(
    (element: HTMLDivElement | null) => {
      sessionsShellRef.current = element;
      setGroupSessionsBodyElement(element);
    },
    [setGroupSessionsBodyElement]
  );
  const groupTitleActionLabel = canToggleCollapsed
    ? `${isCollapsed ? 'Expand' : 'Collapse'} ${group.title}`
    : group.title;
  /**
   * CDXC:ProjectHeaders 2026-05-18-14:53:
   * Project row collapse/expand keeps an accessible label but no hover tooltip.
   * Project header clicks toggle the project session list rather than activating
   * the project; only the right-side action buttons keep their own click
   * behavior.
   *
   * CDXC:ProjectHeaders 2026-06-29-21:04:
   * Regular project rows no longer rely on a visual-only leading glyph; keep
   * collapse semantics on the header/title instead of preserving the old icon slot.
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
   * section overflow and following rows cannot cover labels from the previous row.
   */
  const shouldSuppressProjectCollapseTooltip = Boolean(projectContext) && canToggleCollapsed;
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
        projectKindLabel={projectContext.worktree ? 'Worktree project' : 'Repository project'}
        projectPath={projectContext.path}
        sessionCount={actualSessionCount}
        stats={projectContext.editor.diffStats}
        title={group.title}
        worktreeCount={projectWorktreeCount}
      />
    ) : undefined;
  const createSessionTooltip = isChatCollection ? 'Create a Chat' : 'Create a Terminal';
  const hasUnavailableProjectPath =
    group.remoteMachineContext === undefined &&
    projectContext?.pathState !== undefined &&
    projectContext.pathState !== 'available';
  const unavailableProjectPathTooltip = hasUnavailableProjectPath
    ? `Folder not found: ${projectContext.path}`
    : undefined;
  const primaryProjectAgent = agents.find((agent) => agent.agentId === primaryProjectAgentLauncherId) ?? agents[0];
  const primaryProjectAgentLabel = primaryProjectAgent?.name ?? 'Agent';
  /*
   * CDXC:RemoteRecentProjects 2026-06-24-10:36:
   * Remote project rows can be closed into Recent Projects even though remote remove/delete remains disabled from the normal project context. Keep close eligibility separate from canRemoveProject so the menu does not expose remote deletion.
   */
  const canCloseProject =
    Boolean(projectContext) && (projectContext?.canRemoveProject === true || Boolean(group.remoteMachineContext));
  useEffect(() => {
    postGroupDebugLog('group.sectionMounted', {
      orderedSessionIds,
    });

    return () => {
      postGroupDebugLog('group.sectionUnmounted', {});
    };
  }, [postGroupDebugLog, orderedSessionIds]);

  useEffect(() => {
    postGroupDebugLog('group.dropStateChanged', {
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
            mutation.type === 'attributes' &&
            mutation.attributeName !== null &&
            DISABLED_GROUP_DND_AX_ATTRIBUTES.includes(
              mutation.attributeName as (typeof DISABLED_GROUP_DND_AX_ATTRIBUTES)[number]
            )
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

  useLayoutEffect(() => {
    if (!isEditing) {
      return;
    }

    const input = groupTitleInputRef.current;
    input?.focus({ preventScroll: true });
    input?.select();
  }, [isEditing]);

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

      if (controlMenuRef.current?.contains(target) || projectAgentButtonRef.current?.contains(target)) {
        return;
      }

      setOpenControlMenu(undefined);
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setOpenControlMenu(undefined);
      }
    };
    const handleBlur = () => {
      setOpenControlMenu(undefined);
    };
    const handleVisibilityChange = () => {
      if (document.visibilityState !== 'visible') {
        setOpenControlMenu(undefined);
      }
    };

    document.addEventListener('pointerdown', handlePointerDown);
    document.addEventListener('keydown', handleKeyDown);
    document.addEventListener('visibilitychange', handleVisibilityChange);
    window.addEventListener('blur', handleBlur);

    return () => {
      document.removeEventListener('pointerdown', handlePointerDown);
      document.removeEventListener('keydown', handleKeyDown);
      document.removeEventListener('visibilitychange', handleVisibilityChange);
      window.removeEventListener('blur', handleBlur);
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
        type: 'renameWorkspaceProjectForGroup',
      });
      return;
    }

    vscode.postMessage({ groupId: group.groupId, title: nextTitle, type: 'renameGroup' });
  };

  const requestFocusGroup = () => {
    vscode.postMessage({
      groupId: group.groupId,
      type: 'focusGroup',
    });
  };

  const requestCreateSession = () => {
    onCreateSessionRequested?.(group.groupId);

    vscode.postMessage({
      groupId: group.groupId,
      type: 'createSessionInGroup',
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
    onProjectSessionListCollapsedChange?.(projectSessionListStorageId, !isProjectSessionListCollapsed);
  };

  const requestCreateProjectTerminal = () => {
    setOpenControlMenu(undefined);
    vscode.postMessage({
      groupId: group.groupId,
      type: 'createProjectTerminal',
    });
  };

  const openWorktreeModal = () => {
    if (!projectContext) {
      return;
    }
    openAppModal({
      modal: 'worktree',
      projectId: projectContext.editor.projectId,
      projectName: group.title,
      projectPath: projectContext.path,
      remoteMachineId: group.remoteMachineContext?.machineId,
      remoteMachineName: group.remoteMachineContext?.machineName,
      type: 'open',
    });
  };

  const requestCreateWorktreePullRequest = () => {
    if (!projectContext?.worktree) {
      return;
    }
    vscode.postMessage({
      action: 'pr',
      groupId: group.groupId,
      type: 'runSidebarGitAction',
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
      type: 'runSidebarAgent',
    });
  };

  const openConfigureAgentsModal = () => {
    setOpenControlMenu(undefined);
    openAppModal({ modal: 'configureAgents', type: 'open' });
  };

  const requestCreateBrowserPane = () => {
    if (!projectContext) {
      return;
    }

    vscode.postMessage({
      groupId: group.groupId,
      type: 'openBrowserPaneInGroup',
    });
  };

  const requestRunProjectRowCommand = (command: SidebarCommandButton, scope: SidebarCommandScope) => {
    if (!projectContext) {
      return;
    }
    /*
     * CDXC:ProjectActions 2026-08-01:
     * Row Action clicks stay selector-shaped like the Command Palette: command
     * id plus the row's group id. The host resolves launch metadata from its
     * trusted per-project HUD state and activates the project before running.
     *
     * CDXC:GlobalActions 2026-08-07:
     * The scope names the list to resolve the id against; the group id keeps
     * meaning the project to run in, so a Global Action clicked on a row runs
     * in that row's project exactly like a project one.
     */
    vscode.postMessage({
      commandId: command.commandId,
      groupId: group.groupId,
      scope,
      type: 'runSidebarCommand',
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
        type: 'closeGroup',
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
      type: 'setGroupSleeping',
    });
  };

  const requestCreateSessionGroup = () => {
    setContextMenuPosition(undefined);
    vscode.postMessage({
      groupId: group.groupId,
      type: 'createGroup',
    });
  };

  const requestSleepInactiveProjectSessions = () => {
    setContextMenuPosition(undefined);
    vscode.postMessage({
      groupId: group.groupId,
      type: 'sleepInactiveProjectSessions',
    });
  };

  const requestCloseInactiveProjectSessions = () => {
    setContextMenuPosition(undefined);
    vscode.postMessage({
      groupId: group.groupId,
      type: 'closeInactiveProjectSessions',
    });
  };

  const requestWakeProjectSleepingSessions = () => {
    setContextMenuPosition(undefined);
    vscode.postMessage({
      groupId: group.groupId,
      type: 'wakeProjectSleepingSessions',
    });
  };

  const requestFullReloadGroup = () => {
    if (!canFullReloadGroup) {
      return;
    }

    setContextMenuPosition(undefined);
    vscode.postMessage({
      groupId: group.groupId,
      type: projectContext ? 'fullReloadProjectZmxSessions' : 'fullReloadGroup',
    });
  };

  const openProjectThemeMenu = () => {
    setContextMenuPosition((currentPosition) =>
      currentPosition ? { ...currentPosition, view: 'project-themes' } : currentPosition
    );
  };

  const openProjectCustomThemeMenu = () => {
    setCustomThemeColor(projectContext?.themeColor ?? recentThemeColors[0] ?? DEFAULT_WORKSPACE_THEME_COLOR);
    setContextMenuPosition((currentPosition) =>
      currentPosition ? { ...currentPosition, view: 'project-custom-theme' } : currentPosition
    );
  };

  const openProjectRootMenu = () => {
    setContextMenuPosition((currentPosition) =>
      currentPosition ? { ...currentPosition, view: 'group' } : currentPosition
    );
  };

  const copyProjectPath = () => {
    setContextMenuPosition(undefined);
    vscode.postMessage({
      groupId: group.groupId,
      type: 'copyWorkspaceProjectPathForGroup',
    });
  };

  const copyProjectRemoteUrl = () => {
    if (!projectGitRemoteOriginUrl) {
      return;
    }
    setContextMenuPosition(undefined);
    vscode.postMessage({
      remoteUrl: projectGitRemoteOriginUrl,
      type: 'copyWorkspaceProjectRemoteUrl',
    });
  };

  const openProjectInFinder = () => {
    setContextMenuPosition(undefined);
    vscode.postMessage({
      groupId: group.groupId,
      type: 'openWorkspaceProjectInFinderForGroup',
    });
  };

  const chooseProjectTheme = (theme: SidebarTheme) => {
    setContextMenuPosition(undefined);
    vscode.postMessage({
      groupId: group.groupId,
      theme,
      themeColor: null,
      type: 'setWorkspaceProjectThemeForGroup',
    });
  };

  const chooseProjectThemeColor = (themeColor: string) => {
    const normalizedColor = normalizeWorkspaceThemeColor(themeColor);
    if (!normalizedColor) {
      return;
    }

    setContextMenuPosition(undefined);
    const nextRecentThemeColors = updateWorkspaceThemeColorHistory(recentThemeColors, normalizedColor);
    setRecentThemeColors(nextRecentThemeColors);
    writeWorkspaceThemeColorHistory(nextRecentThemeColors);
    vscode.postMessage({
      groupId: group.groupId,
      themeColor: normalizedColor,
      type: 'setWorkspaceProjectThemeForGroup',
    });
  };

  const closeProject = () => {
    if (!canCloseProject) {
      return;
    }

    setContextMenuPosition(undefined);
    vscode.postMessage({
      groupId: group.groupId,
      type: 'closeWorkspaceProjectForGroup',
    });
  };

  const openProjectCollectionMenu = () => {
    if (!projectContext || !onCreateProjectCollection || !onMoveProjectToCollection) {
      return;
    }
    setContextMenuPosition((previous) => (previous ? { ...previous, view: 'project-collections' } : previous));
  };

  const createProjectCollection = () => {
    if (!projectContext || !onCreateProjectCollection) {
      return;
    }
    setContextMenuPosition(undefined);
    onCreateProjectCollection(projectContext.editor.projectId);
  };

  const moveProjectToCollection = (collectionId: string | undefined) => {
    if (!projectContext || !onMoveProjectToCollection) {
      return;
    }
    setContextMenuPosition(undefined);
    onMoveProjectToCollection(projectContext.editor.projectId, collectionId);
  };

  const openProjectSpacesMenu = () => {
    setContextMenuPosition((previous) => (previous ? { ...previous, view: 'project-spaces' } : previous));
  };

  const toggleProjectSpaceMembership = (spaceId: string) => {
    setContextMenuPosition(undefined);
    onToggleSpaceMembership?.(spaceId);
  };

  const createSpaceForProject = () => {
    if (!spaceMemberProjectId) {
      return;
    }
    const remoteMachineId = group.remoteMachineContext?.machineId;
    setContextMenuPosition(undefined);
    openAppModal({
      memberProjectId: spaceMemberProjectId,
      mode: 'create',
      modal: 'sidebarSpaceEditor',
      ...(remoteMachineId ? { remoteMachineId } : {}),
      sectionKey: remoteMachineId
        ? createRemoteSidebarSpaceSectionKey(remoteMachineId)
        : LOCAL_SIDEBAR_SPACE_SECTION_KEY,
      type: 'open',
    });
  };

  const removeWorktreeProject = () => {
    if (!projectContext?.worktree || !projectContext.canRemoveProject) {
      return;
    }

    setContextMenuPosition(undefined);
    vscode.postMessage({
      groupId: group.groupId,
      type: 'removeWorkspaceProjectForGroup',
    });
  };

  const promptDeleteWorktree = () => {
    if (!projectContext?.worktree) {
      return;
    }

    setContextMenuPosition(undefined);
    vscode.postMessage({
      groupId: group.groupId,
      type: 'promptDeleteWorktreeForGroup',
    });
  };

  const promptRenameWorktree = () => {
    if (!projectContext?.worktree) {
      return;
    }

    setContextMenuPosition(undefined);
    vscode.postMessage({
      groupId: group.groupId,
      type: 'promptRenameWorktreeForGroup',
    });
  };

  const handleTitleKeyDown = (event: ReactKeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Enter') {
      event.preventDefault();
      submitRename();
      return;
    }

    if (event.key !== 'Escape') {
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

    onCollapsedChange(group.groupId, !isCollapsed);
  };

  const handleGroupHeaderClickCapture = (event: ReactMouseEvent<HTMLDivElement>) => {
    if (isEditing) {
      return;
    }

    if (event.target instanceof Element && event.target.closest('.group-header-actions')) {
      return;
    }

    /**
     * Project reordering owns the full header as its drag handle. In CEF that
     * handle can consume the bubbled click before the header sees it, leaving
     * only the title button able to collapse the group. Toggle during capture
     * so every non-control part of the real header remains clickable while the
     * action cluster keeps its own bounded interactions.
     */
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

  const handleGroupHeaderContextMenu = (event: ReactMouseEvent<HTMLElement>) => {
    if (!projectContext && isNestedInteractiveContextMenuTarget(event)) {
      /**
       * CDXC:SidebarContextMenu 2026-05-15-17:53:
       * Header buttons without their own context menu should not open the
       * surrounding project/group context menu on right-click. Suppress nested
       * interactive targets while preserving right-click menus on the row
       * surface itself.
       *
       * CDXC:SidebarContextMenu 2026-05-16-13:39:
       * Project headers own a custom project context menu across their whole
       * header, including icon/title and action-button children. Do not apply
       * the nested-control suppression to project groups.
       *
       * CDXC:SidebarContextMenu 2026-06-19-10:46:
       * The project context menu must open from the project header only, not
       * from the project body or the spacing between session rows.
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
          canCreateSessionGroup: group.canCreateSessionGroup === true,
          canCopyProjectRemoteUrl: Boolean(projectGitRemoteOriginUrl),
          canFullReloadGroup,
          canHideGroup: Boolean(onHideGroup),
          hasProjectContext: Boolean(projectContext),
          isWorktreeProject: Boolean(projectContext?.worktree),
          projectCollectionsEnabled: Boolean(onCreateProjectCollection && onMoveProjectToCollection),
          spacesEnabled: isProjectSpacesMenuEnabled,
        })
      )
    );
  };

  const handleGroupSessionsContextMenu = (event: ReactMouseEvent<HTMLDivElement>) => {
    if (event.target instanceof Element && event.target.closest('.session')) {
      return;
    }

    const contextMenuTarget = getSidebarSessionGapContextMenuTarget({
      clientY: event.clientY,
      sessionRows: Array.from(
        event.currentTarget.querySelectorAll<HTMLElement>(
          '.session[data-sidebar-session-id][data-project-session-list-more-row="false"][data-project-session-list-overflow="false"]'
        )
      ).map((element) => {
        const bounds = element.getBoundingClientRect();
        return {
          bottom: bounds.bottom,
          element,
          top: bounds.top,
        };
      }),
    });

    if (!contextMenuTarget) {
      return;
    }

    event.preventDefault();
    event.stopPropagation();
    contextMenuTarget.dispatchEvent(
      new MouseEvent('contextmenu', {
        altKey: event.altKey,
        bubbles: true,
        button: event.button,
        buttons: event.buttons,
        cancelable: true,
        clientX: event.clientX,
        clientY: event.clientY,
        ctrlKey: event.ctrlKey,
        metaKey: event.metaKey,
        screenX: event.screenX,
        screenY: event.screenY,
        shiftKey: event.shiftKey,
        view: window,
      })
    );
  };

  return (
    <>
      <section
        className='group'
        data-active={String(group.isActive)}
        data-collapsed={String(isCollapsed)}
        data-contains-active-session={String(containsActiveSession)}
        data-dragging={String(Boolean(sortable.isDragging || isGroupDragPreviewSource))}
        data-group-drop-position={groupDropPosition}
        data-drop-target={String(isGroupDropTarget)}
        data-empty-space-blocking='true'
        data-empty-project={String(isEmptyProjectGroup)}
        data-project-group={String(Boolean(projectContext))}
        data-project-path-state={group.remoteMachineContext === undefined ? projectContext?.pathState : undefined}
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
        ref={setGroupSectionElement}
        role={shouldScrubDisabledGroupDndAccessibility ? 'group' : undefined}
      >
        <div
          className='group-head'
          data-collapsible='true'
          onClickCapture={handleGroupHeaderClickCapture}
          onContextMenu={handleGroupHeaderContextMenu}
          ref={projectContext && !isChatCollection ? sortable.handleRef : undefined}
          style={groupHeaderStyle}
        >
          <div className='group-title-wrap'>
            {isEditing ? (
              <input
                className='group-title-input'
                onBlur={submitRename}
                onChange={(event) => setDraftTitle(event.currentTarget.value)}
                onClick={(event) => event.stopPropagation()}
                onKeyDown={handleTitleKeyDown}
                ref={groupTitleInputRef}
                value={draftTitle}
              />
            ) : (
              <div className='group-title-row' data-project-leading-icon={String(showProjectIcons)}>
                {projectContext ? (
                  isChatCollection ? (
                    <span
                      aria-hidden='true'
                      className='group-collapse-button section-titlebar-toggle'
                      data-collapsed={String(isCollapsed)}
                      data-empty-project={String(isEmptyProjectGroup)}
                      data-has-idle-icon={String(canToggleCollapsed)}
                      data-static-icon={String(!canToggleCollapsed)}
                    >
                      <span
                        aria-hidden='true'
                        className='group-collapse-icon group-collapse-idle-icon section-titlebar-toggle-icon section-titlebar-toggle-idle-icon'
                      >
                        <IconMessageCircle size={16} stroke={1.8} />
                      </span>
                      {canToggleCollapsed ? (
                        <IconCaretRightFilled
                          aria-hidden='true'
                          className='group-collapse-icon group-collapse-chevron-icon section-titlebar-toggle-icon section-titlebar-toggle-chevron-icon'
                          size={16}
                        />
                      ) : null}
                    </span>
                  ) : null
                ) : (
                  <button
                    aria-controls={canToggleCollapsed && !isCollapsed ? sessionsRegionId : undefined}
                    aria-disabled={!canToggleCollapsed && !isEmptyProjectGroup}
                    aria-expanded={canToggleCollapsed ? !isCollapsed : undefined}
                    aria-label={groupTitleActionLabel}
                    className='group-collapse-button section-titlebar-toggle'
                    data-collapsed={String(isCollapsed)}
                    data-empty-project={String(isEmptyProjectGroup)}
                    data-has-idle-icon={String(canToggleCollapsed)}
                    data-static-icon={String(!canToggleCollapsed)}
                    onClick={(event) => {
                      event.preventDefault();
                      event.stopPropagation();
                      toggleCollapsedOrSelectEmptyProject();
                    }}
                    type='button'
                  >
                    <span
                      aria-hidden='true'
                      className='group-collapse-icon group-collapse-idle-icon section-titlebar-toggle-icon section-titlebar-toggle-idle-icon'
                    >
                      {isChatCollection ? (
                        <IconMessageCircle size={16} stroke={1.8} />
                      ) : !isCollapsed ? (
                        <IconFolderOpen size={16} stroke={1.8} />
                      ) : (
                        <IconFolder size={16} stroke={1.8} />
                      )}
                    </span>
                    {canToggleCollapsed ? (
                      <IconCaretRightFilled
                        aria-hidden='true'
                        className='group-collapse-icon group-collapse-chevron-icon section-titlebar-toggle-icon section-titlebar-toggle-chevron-icon'
                        size={16}
                      />
                    ) : null}
                  </button>
                )}
                {showProjectIcons && projectContext && !isChatCollection ? (
                  <SidebarProjectIcon
                    discoveredIconDataUrl={projectContext.discoveredIconDataUrl}
                    fallback={projectContext.worktree ? 'worktree' : isCollapsed ? 'folder' : 'folder-open'}
                    icon={projectContext.icon}
                    iconDataUrl={projectContext.iconDataUrl}
                    title={group.title}
                    tooltipDelay={sidebarItemTooltipDelayMs}
                  />
                ) : null}
                <div
                  className='group-title-handle'
                  data-draggable={String(!isChatCollection)}
                  ref={!projectContext && !isChatCollection ? sortable.handleRef : undefined}
                >
                  {shouldSuppressProjectCollapseTooltip ? (
                    <AppTooltip
                      align='start'
                      anchor={() => projectTitleButtonRef.current?.closest<HTMLElement>('.group-head') ?? null}
                      content={projectTitleTooltip}
                      contentClassName='project-title-tooltip-content'
                      delay={sidebarItemTooltipDelayMs}
                    >
                      <button
                        aria-controls={canToggleCollapsed && !isCollapsed ? sessionsRegionId : undefined}
                        aria-disabled={!canToggleCollapsed && !isEmptyProjectGroup}
                        aria-expanded={canToggleCollapsed ? !isCollapsed : undefined}
                        aria-label={groupTitleActionLabel}
                        className='group-title-button'
                        data-empty-project={String(isEmptyProjectGroup)}
                        onClick={(event) => {
                          event.preventDefault();
                          event.stopPropagation();
                          toggleCollapsedOrSelectEmptyProject();
                        }}
                        ref={projectTitleButtonRef}
                        type='button'
                      >
                        <span className='group-title section-titlebar-label'>{group.title}</span>
                        {isHidden ? (
                          <IconEyeOff aria-label='Hidden' className='sidebar-hidden-item-icon' size={13} />
                        ) : null}
                      </button>
                    </AppTooltip>
                  ) : (
                    <AppTooltip
                      content={groupTitleActionLabel}
                      delay={projectContext ? sidebarItemTooltipDelayMs : undefined}
                    >
                      <button
                        aria-controls={canToggleCollapsed && !isCollapsed ? sessionsRegionId : undefined}
                        aria-disabled={!canToggleCollapsed && !isEmptyProjectGroup}
                        aria-expanded={canToggleCollapsed ? !isCollapsed : undefined}
                        aria-label={groupTitleActionLabel}
                        className='group-title-button'
                        data-empty-project={String(isEmptyProjectGroup)}
                        onClick={(event) => {
                          event.preventDefault();
                          event.stopPropagation();
                          toggleCollapsedOrSelectEmptyProject();
                        }}
                        type='button'
                      >
                        <span className='group-title section-titlebar-label'>{group.title}</span>
                        {isHidden ? (
                          <IconEyeOff aria-label='Hidden' className='sidebar-hidden-item-icon' size={13} />
                        ) : null}
                      </button>
                    </AppTooltip>
                  )}
                </div>
                {unavailableProjectPathTooltip ? (
                  <AppTooltip content={unavailableProjectPathTooltip}>
                    <button
                      aria-label={`Resolve missing folder for ${group.title}`}
                      className='group-project-path-warning'
                      onClick={(event) => {
                        event.preventDefault();
                        event.stopPropagation();
                        requestCreateProjectTerminal();
                      }}
                      type='button'
                    >
                      <IconAlertTriangle aria-hidden='true' size={14} stroke={1.9} />
                    </button>
                  </AppTooltip>
                ) : null}
                <div className='group-title-spacer' />
                {shouldShowCollapsedProjectCounts ? (
                  <div
                    aria-label={getCollapsedProjectCountsLabel(sessionSummary, awakeCount)}
                    className='group-collapsed-status-counts'
                  >
                    {sessionSummary.workingCount > 0 ? (
                      <span className='group-collapsed-status-count' data-activity='working'>
                        {sessionSummary.workingCount}
                      </span>
                    ) : null}
                    {sessionSummary.attentionCount > 0 ? (
                      <span className='group-collapsed-status-count' data-activity='attention'>
                        {sessionSummary.attentionCount}
                      </span>
                    ) : null}
                    {!hasProjectActionStatus && awakeCount > 0 ? (
                      <span className='group-collapsed-status-count' data-activity='awake'>
                        {awakeCount}
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
                    className='group-header-actions'
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
                       * review flow for commit/push/PR, and that modal now
                       * owns the optional direct merge-to-main path so the header does
                       * not imply two competing worktree completion flows.
                       *
                       * CDXC:RemoteMachines 2026-06-09-19:02:
                       * Remote project headers must use the exact same project-header
                       * chrome and actions as local project headers. The GPUI host
                       * routes browser, terminal, agent, worktree, and pull-request
                       * actions through their machine-scoped project identities.
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
                            className='group-add-button group-project-session-list-toggle-button'
                            onClick={(event) => {
                              event.preventDefault();
                              event.stopPropagation();
                              toggleProjectSessionListCollapsed();
                            }}
                            tooltip={projectSessionListToggleLabel}
                            type='button'
                          >
                            {isProjectSessionListCollapsed ? (
                              <IconChevronDown aria-hidden='true' className='group-add-icon' size={14} stroke={2} />
                            ) : (
                              <IconChevronUp aria-hidden='true' className='group-add-icon' size={14} stroke={2} />
                            )}
                          </ProjectHeaderActionButton>
                        ) : null}
                        {projectHeaderActions === 'all' && projectContext.worktree ? (
                          <ProjectHeaderActionButton
                            aria-label={`Create PR for ${group.title}`}
                            className='group-add-button group-worktree-pr-button'
                            onClick={(event) => {
                              event.preventDefault();
                              event.stopPropagation();
                              requestCreateWorktreePullRequest();
                            }}
                            tooltip='Create PR'
                            type='button'
                          >
                            <IconGitPullRequest aria-hidden='true' className='group-add-icon' size={14} stroke={2} />
                          </ProjectHeaderActionButton>
                        ) : projectHeaderActions === 'all' ? (
                          <ProjectHeaderActionButton
                            aria-label={`Create a worktree from ${group.title}`}
                            className='group-add-button group-worktree-button'
                            onClick={(event) => {
                              event.preventDefault();
                              event.stopPropagation();
                              openWorktreeModal();
                            }}
                            tooltip='Add Worktree'
                            type='button'
                          >
                            <IconGitBranch aria-hidden='true' className='group-add-icon' size={14} stroke={2} />
                          </ProjectHeaderActionButton>
                        ) : null}
                        {projectHeaderActions === 'all' && browserViewEnabled ? (
                          <ProjectHeaderActionButton
                            aria-label={`Create a browser tab in ${group.title}`}
                            className='group-add-button group-browser-button'
                            onClick={(event) => {
                              event.preventDefault();
                              event.stopPropagation();
                              requestCreateBrowserPane();
                            }}
                            tooltip='New Browser Tab'
                            type='button'
                          >
                            <IconWorld aria-hidden='true' className='group-add-icon' size={14} stroke={2} />
                          </ProjectHeaderActionButton>
                        ) : null}
                        <ProjectHeaderActionButton
                          aria-label={`Create a terminal in ${group.title}`}
                          className='group-add-button group-project-terminal-button'
                          onClick={(event) => {
                            event.preventDefault();
                            event.stopPropagation();
                            requestCreateProjectTerminal();
                          }}
                          tooltip='Create Terminal'
                          type='button'
                        >
                          <IconTerminal2 aria-hidden='true' className='group-add-icon' size={14} stroke={2} />
                        </ProjectHeaderActionButton>
                        {projectHeaderActions === 'all'
                          ? projectRowCommands.map(({ command, scope }) => {
                              const commandLabel = command.name.trim() || 'Run Action';
                              return (
                                <ProjectHeaderActionButton
                                  aria-label={`Run ${commandLabel} in ${group.title}`}
                                  className='group-add-button group-project-row-command-button'
                                  key={`${scope}:${command.commandId}`}
                                  onClick={(event) => {
                                    event.preventDefault();
                                    event.stopPropagation();
                                    requestRunProjectRowCommand(command, scope);
                                  }}
                                  tooltip={commandLabel}
                                  type='button'
                                >
                                  <SidebarCommandIconGlyph
                                    className='group-add-icon'
                                    icon={command.icon ?? DEFAULT_SIDEBAR_COMMAND_ICON}
                                    size={14}
                                    stroke={2}
                                  />
                                </ProjectHeaderActionButton>
                              );
                            })
                          : null}
                        {projectHeaderActions === 'all' ? (
                          <div className='group-control-anchor'>
                            <div
                              className='group-agent-split-button'
                              data-open={String(openControlMenu === 'project-agent')}
                            >
                              <ProjectHeaderActionButton
                                aria-label={`Create ${primaryProjectAgentLabel} in ${group.title}`}
                                className='group-agent-main-button'
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
                                type='button'
                              >
                                <ProjectAgentLauncherIcon
                                  agent={primaryProjectAgent}
                                  colorMode={projectAgentLauncherIconColorMode}
                                />
                              </ProjectHeaderActionButton>
                              <ProjectHeaderActionButton
                                aria-expanded={openControlMenu === 'project-agent'}
                                aria-haspopup='menu'
                                aria-label={`Select agent for ${group.title}`}
                                className='group-agent-toggle-button'
                                data-open={String(openControlMenu === 'project-agent')}
                                onClick={() => {
                                  setOpenControlMenu((previous) =>
                                    previous === 'project-agent' ? undefined : 'project-agent'
                                  );
                                }}
                                ref={projectAgentButtonRef}
                                tooltip='Select Agent'
                                type='button'
                              >
                                <IconChevronDown aria-hidden='true' size={13} stroke={2} />
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
                            className='group-add-button'
                            onClick={(event) => {
                              event.preventDefault();
                              event.stopPropagation();
                              requestCreateSession();
                            }}
                            type='button'
                          >
                            <IconPlus aria-hidden='true' className='group-add-icon' size={14} stroke={2} />
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
            className='group-collapsed-summary'
            data-activity={collapsedIndicatorActivity}
            style={getCollapsedGroupStatusStyle(group.groupId)}
          >
            <div aria-hidden className='session-status-dot' />
          </div>
        ) : null}
        {shouldRenderGroupSessionsBody ? (
          <div
            aria-hidden={isGroupSessionsBodyVisuallyCollapsed}
            className='group-sessions-shell sidebar-collapse-shell'
            data-collapsed={String(isGroupSessionsBodyVisuallyCollapsed)}
            inert={isGroupSessionsBodyVisuallyCollapsed ? true : undefined}
            data-project-session-list-clipped={String(shouldClipProjectSessionList)}
            data-project-session-list-scrollable={String(shouldScrollExpandedProjectSessionList)}
            onWheel={handleSessionsShellWheel}
            ref={setSessionsShellElement}
            style={sessionsShellStyle}
          >
            <div
              className='group-sessions sidebar-collapse-content'
              data-drop-position={sessionGroupDropPosition}
              data-pinned-drop-gaps={String(shouldRenderPinnedSessionDropGaps)}
              data-drop-target={String(isSessionDropTargetVisible)}
              id={sessionsRegionId}
              onContextMenu={handleGroupSessionsContextMenu}
              ref={setContentElement}
            >
              {showSessionGroupConnector ? (
                <>
                  <div aria-hidden className='group-session-connector-rail' />
                  <button
                    aria-label={`Collapse ${group.title}`}
                    className='group-session-connector-button'
                    onClick={(event) => {
                      event.preventDefault();
                      event.stopPropagation();
                      toggleCollapsed();
                    }}
                    type='button'
                  />
                </>
              ) : null}
              {orderedSessionIds.length > 0 ? (
                <>
                  {renderedSessionIds.map((sessionId, sessionIndex) => {
                    const session = sessionsById[sessionId];
                    const projectSessionSection = getProjectSessionSection(session, enableSessionParking);
                    const isProjectSessionSectionCollapsed =
                      (Boolean(projectContext) || (isChatCollection && projectSessionSection === 'parked')) &&
                      collapsedProjectSessionSections[projectSessionSection];
                    const isProjectSessionListOverflowRow =
                      shouldClipProjectSessionList && !visibleSessionIdSet.has(sessionId);
                    const sessionIdsBelowStartIndex = isProjectSessionListOverflowRow ? undefined : sessionIndex + 1;
                    const sessionDropPosition =
                      sessionDropIndicator?.kind === 'session' &&
                      sessionDropIndicator.groupId === group.groupId &&
                      sessionDropIndicator.sessionId === sessionId
                        ? sessionDropIndicator.position
                        : undefined;
                    const pinnedSessionDropPosition =
                      pinnedSessionDropIndicator?.kind === 'session' &&
                      pinnedSessionDropIndicator.groupId === group.groupId &&
                      pinnedSessionDropIndicator.sessionId === sessionId
                        ? pinnedSessionDropIndicator.position
                        : undefined;

                    return (
                      <Fragment key={sessionId}>
                        {projectContext && sessionId === firstBrowserSessionId ? (
                          <ProjectSessionSectionToggle
                            count={projectSessionSectionCounts.browser}
                            isCollapsed={collapsedProjectSessionSections.browser}
                            label='Browser'
                            onToggle={() => toggleProjectSessionSection('browser')}
                          />
                        ) : null}
                        {projectContext && sessionId === firstPinnedSessionId ? (
                          <ProjectSessionSectionToggle
                            count={projectSessionSectionCounts.pinned}
                            isCollapsed={collapsedProjectSessionSections.pinned}
                            label='Pinned'
                            onToggle={() => toggleProjectSessionSection('pinned')}
                          />
                        ) : null}
                        {projectContext && sessionId === firstUnpinnedSessionId ? (
                          <ProjectSessionSectionToggle
                            count={projectSessionSectionCounts.sessions}
                            isCollapsed={collapsedProjectSessionSections.sessions}
                            label='Sessions'
                            onToggle={() => toggleProjectSessionSection('sessions')}
                          />
                        ) : null}
                        {(projectContext || isChatCollection) && sessionId === firstParkedSessionId ? (
                          <ProjectSessionSectionToggle
                            count={projectSessionSectionCounts.parked}
                            isCollapsed={collapsedProjectSessionSections.parked}
                            label='Parked'
                            onToggle={() => toggleProjectSessionSection('parked')}
                          />
                        ) : null}
                        {!projectContext && shouldRenderSessionKindLabels && sessionId === firstBrowserSessionId ? (
                          <div className='session-kind-label'>Browser</div>
                        ) : null}
                        {!projectContext && shouldRenderSessionKindLabels && sessionId === firstTerminalSessionId ? (
                          <div className='session-kind-label'>Sessions</div>
                        ) : null}
                        {!isProjectSessionSectionCollapsed && shouldRenderPinnedSessionDropGaps ? (
                          <div
                            aria-hidden
                            className='pinned-session-drop-gap'
                            data-active={String(pinnedSessionDropGapKey === getSessionDropGapKeyBefore(sessionId))}
                            data-edge={sessionIndex === 0 ? 'start' : undefined}
                          />
                        ) : null}
                        {!isProjectSessionSectionCollapsed ? (
                          <SortableSessionCard
                            completionFlashNonce={completionFlashNonceBySessionId?.[sessionId] ?? 0}
                            dragDisabled={
                              isProjectSessionListOverflowRow ||
                              draggingDisabled ||
                              (sessionDraggingDisabled &&
                                !(allowPinnedSessionReorder && sessionsById[sessionId]?.isPinned === true))
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
                                : (sessionDropPosition ?? pinnedSessionDropPosition)
                            }
                            index={sessionIndex}
                            isProjectSessionListOverflowRow={isProjectSessionListOverflowRow}
                            isSearchSelected={!isProjectSessionListOverflowRow && selectedSearchSessionId === sessionId}
                            onFocusRequested={onFocusRequested}
                            onSessionSelectionChange={onSessionSelectionChange}
                            sessionCardSettings={sessionCardSettings}
                            sessionGroup={group}
                            sessionTagListItems={sessionTagListItems}
                            sessionIdsBelowSource={visibleSessionIds}
                            sessionIdsBelowStartIndex={sessionIdsBelowStartIndex}
                            sessionId={sessionId}
                            selectedSessionIds={selectedSessionIds}
                            shouldKeepLastProjectSessionVisibleOnClose={
                              Boolean(projectContext) &&
                              !isChatCollection &&
                              storedSessionIds.length === 1 &&
                              storedSessionIds[0] === sessionId
                            }
                            showGroupDropTargetChrome={!allowPinnedSessionReorder && !isProjectSessionListOverflowRow}
                            showGroupConnector={showSessionGroupConnector && !isProjectSessionListOverflowRow}
                            showDropPositionIndicator={
                              showSessionDropPositionIndicators &&
                              !allowPinnedSessionReorder &&
                              !isProjectSessionListOverflowRow
                            }
                            vscode={vscode}
                          />
                        ) : null}
                        {!projectContext &&
                        !isProjectSessionSectionCollapsed &&
                        sessionsById[sessionId]?.isPinned === true &&
                        orderedSessionIds[sessionIndex + 1] !== undefined &&
                        sessionsById[orderedSessionIds[sessionIndex + 1]]?.isPinned !== true ? (
                          <div aria-hidden className='pinned-sessions-divider' />
                        ) : null}
                      </Fragment>
                    );
                  })}
                  {projectSessionListHiddenCount > 0 ? (
                    /*
                     * CDXC:ProjectSessionLists 2026-06-13-22:23:
                     * The hidden-session count belongs below the last rendered project row. Render it through the regular session-card component so it has identical row geometry, but disable session-only chrome and actions; clicking the row restores the normal expanded project list, removes this row, and brings the header Show less action back.
                     */
                    <SortableSessionCard
                      dragDisabled
                      dropDisabled
                      groupId={group.groupId}
                      index={renderedSessionIds.length}
                      projectSessionListMoreRow={{
                        count: projectSessionListHiddenCount,
                        onReveal: toggleProjectSessionListCollapsed,
                      }}
                      sessionCardSettings={sessionCardSettings}
                      sessionGroup={group}
                      sessionId={`${group.groupId}-project-session-list-more`}
                      showDropPositionIndicator={false}
                      showGroupConnector={false}
                      showGroupDropTargetChrome={false}
                      vscode={vscode}
                    />
                  ) : null}
                  {shouldRenderPinnedSessionDropGaps ? (
                    <div
                      aria-hidden
                      className='pinned-session-drop-gap'
                      data-active={String(pinnedSessionDropGapKey === PINNED_SESSION_DROP_GAP_AFTER_LAST)}
                    />
                  ) : null}
                </>
              ) : isEmptyProjectGroup ? (
                /*
                 * CDXC:ProjectGroups 2026-06-15-20:14:
                 * After the last terminal in a project closes, the project body
                 * should expose a session-shaped New Session button with no Last
                 * Active timestamp. Mark it sleeping so it matches dormant rows,
                 * but create a fresh terminal instead of waking a removed one.
                 */
                <div
                  className='session-frame group-empty-project-session-frame'
                  data-empty-project-new-session-row='true'
                  data-focused='false'
                  data-lifecycle-state='sleeping'
                  data-running='false'
                  data-sleeping='true'
                  data-visible='false'
                >
                  <button
                    aria-label={getEmptyProjectNewSessionButtonLabel()}
                    className='session group-empty-project-session-button'
                    data-focused='false'
                    data-search-selected='false'
                    data-sleeping='true'
                    data-visible='false'
                    onClick={(event) => {
                      event.preventDefault();
                      event.stopPropagation();
                      requestCreateProjectTerminal();
                    }}
                    type='button'
                  >
                    <div className='session-head' data-title-full-width='true'>
                      <div className='session-alias-heading'>{getEmptyProjectNewSessionButtonLabel()}</div>
                    </div>
                  </button>
                </div>
              ) : (
                <div
                  className='group-empty-drop-target'
                  data-drop-position={
                    sessionGroupDropPosition ?? (emptyGroupDropTarget.isDropTarget ? 'start' : undefined)
                  }
                  data-drop-target={String(isSessionDropTargetVisible)}
                  ref={emptyGroupDropTarget.ref}
                >
                  <div className='group-empty-state'>{emptyStateLabel}</div>
                </div>
              )}
            </div>
            {showSessionGroupConnector
              ? visibleGroupSessions.map((session) => (
                  <div
                    aria-hidden
                    className='session-status-dot session-status-dot-anchored'
                    data-activity={session.activity}
                    data-lifecycle-state={getSidebarSessionLifecycleState(session)}
                    data-remote-session={String(Boolean(group.remoteMachineContext))}
                    key={`status-${session.sessionId}`}
                    style={getAnchoredSessionStatusStyle(session.sessionId)}
                  />
                ))
              : null}
          </div>
        ) : null}
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
            contextMenuPosition.view === 'project-collections' ? (
              <>
                <button
                  className='session-context-menu-item'
                  onClick={() => {
                    setContextMenuPosition((previous) => (previous ? { ...previous, view: 'group' } : previous));
                  }}
                  role='menuitem'
                  type='button'
                >
                  <IconChevronLeft aria-hidden='true' className='session-context-menu-icon' size={14} />
                  Back
                </button>
                <div className='session-context-menu-divider' role='separator' />
                <button
                  className='session-context-menu-item'
                  onClick={createProjectCollection}
                  role='menuitem'
                  type='button'
                >
                  <IconPlus aria-hidden='true' className='session-context-menu-icon' size={14} />
                  New project group
                </button>
                {projectCollectionOptions.map((collection) => (
                  <button
                    className='session-context-menu-item'
                    key={collection.collectionId}
                    onClick={() => moveProjectToCollection(collection.collectionId)}
                    role='menuitemradio'
                    type='button'
                  >
                    <span className='project-collection-menu-swatch' style={{ background: collection.color }} />
                    <span className='group-agent-menu-label'>{collection.title}</span>
                    {projectCollectionId === collection.collectionId ? (
                      <IconCheck aria-hidden='true' size={14} />
                    ) : null}
                  </button>
                ))}
                {projectCollectionId ? (
                  <>
                    <div className='session-context-menu-divider' role='separator' />
                    <button
                      className='session-context-menu-item'
                      onClick={() => moveProjectToCollection(undefined)}
                      role='menuitem'
                      type='button'
                    >
                      <IconX aria-hidden='true' className='session-context-menu-icon' size={14} />
                      Remove from group
                    </button>
                  </>
                ) : null}
              </>
            ) : contextMenuPosition.view === 'project-spaces' ? (
              <>
                {/*
                 * CDXC:SidebarSpaces 2026-08-27:
                 * Membership rows for one ungrouped project, in the owning
                 * gxserver's Space order. Multi-membership is allowed, so these
                 * are checkboxes rather than radios; toggling closes the menu,
                 * which is what the Tags submenu this pattern is modeled on does.
                 */}
                <button
                  className='session-context-menu-item'
                  onClick={openProjectRootMenu}
                  role='menuitem'
                  type='button'
                >
                  <IconChevronLeft aria-hidden='true' className='session-context-menu-icon' size={14} />
                  Back
                </button>
                <div className='session-context-menu-divider' role='separator' />
                <button
                  className='session-context-menu-item'
                  onClick={createSpaceForProject}
                  role='menuitem'
                  type='button'
                >
                  <IconPlus aria-hidden='true' className='session-context-menu-icon' size={14} />
                  New Space
                </button>
                {spaces && spaces.order.length > 0 ? (
                  <div className='session-context-menu-divider' role='separator' />
                ) : null}
                {spaces && spaces.order.length > 0
                  ? spaces.order.flatMap((spaceId) => {
                      const space = spaces.spaces[spaceId];
                      if (!space) {
                        return [];
                      }
                      const isMember = projectMemberSpaceIds.includes(spaceId);
                      return [
                        <button
                          aria-checked={isMember}
                          className='session-context-menu-item'
                          key={spaceId}
                          onClick={() => toggleProjectSpaceMembership(spaceId)}
                          role='menuitemcheckbox'
                          type='button'
                        >
                          <SidebarCommandIconGlyph
                            className='session-context-menu-icon'
                            color={space.color}
                            icon={resolveSidebarSpaceIcon(space.icon)}
                            size={14}
                          />
                          <span className='sidebar-space-filter-menu-name'>{space.name}</span>
                          {isMember ? (
                            <IconCheck aria-hidden='true' className='session-context-menu-trailing-icon' size={14} />
                          ) : null}
                        </button>,
                      ];
                    })
                  : null}
              </>
            ) : contextMenuPosition.view === 'project-themes' ? (
              <>
                <button
                  className='session-context-menu-item'
                  onClick={openProjectRootMenu}
                  role='menuitem'
                  type='button'
                >
                  <IconChevronLeft aria-hidden='true' className='session-context-menu-icon' size={14} />
                  Back
                </button>
                <div className='session-context-menu-divider' role='separator' />
                <button
                  className='session-context-menu-item workspace-theme-menu-item'
                  data-selected={String(Boolean(projectContext.themeColor))}
                  onClick={openProjectCustomThemeMenu}
                  role='menuitemradio'
                  type='button'
                >
                  <span
                    className='workspace-theme-swatch'
                    style={getProjectThemeSwatchStyle(
                      projectContext.themeColor ?? recentThemeColors[0] ?? DEFAULT_WORKSPACE_THEME_COLOR
                    )}
                  />
                  Custom
                  <IconChevronRight aria-hidden='true' className='session-context-menu-trailing-icon' size={14} />
                </button>
                {PROJECT_CONTEXT_THEME_OPTIONS.map((theme) => (
                  <button
                    className='session-context-menu-item workspace-theme-menu-item'
                    data-selected={String(!projectContext.themeColor && projectContext.theme === theme.value)}
                    key={theme.value}
                    onClick={() => chooseProjectTheme(theme.value)}
                    role='menuitemradio'
                    type='button'
                  >
                    <span className='workspace-theme-swatch' data-workspace-theme={theme.value} />
                    {theme.label}
                  </button>
                ))}
              </>
            ) : contextMenuPosition.view === 'project-custom-theme' ? (
              <>
                <button
                  className='session-context-menu-item'
                  onClick={openProjectThemeMenu}
                  role='menuitem'
                  type='button'
                >
                  <IconChevronLeft aria-hidden='true' className='session-context-menu-icon' size={14} />
                  Back
                </button>
                <div className='session-context-menu-divider' role='separator' />
                <div className='workspace-theme-custom-picker'>
                  {/*
                   * CDXC:WorkspaceTheme 2026-05-05-02:58
                   * Combined-mode project headers own the Theme menu custom color picker after the far-left project list was removed. Applying a color posts a validated project theme color and records it in the local recent-color palette.
                   */}
                  <input
                    aria-label='Custom workspace theme color'
                    className='workspace-theme-color-input'
                    onChange={(event) => {
                      const normalizedColor = normalizeWorkspaceThemeColor(event.currentTarget.value);
                      if (normalizedColor) {
                        setCustomThemeColor(normalizedColor);
                      }
                    }}
                    type='color'
                    value={customThemeColor}
                  />
                  <input
                    aria-label='Custom workspace theme color hex'
                    className='workspace-theme-color-text'
                    onChange={(event) => {
                      const normalizedColor = normalizeWorkspaceThemeColor(event.currentTarget.value);
                      if (normalizedColor) {
                        setCustomThemeColor(normalizedColor);
                      }
                    }}
                    value={customThemeColor}
                  />
                  <button
                    aria-label='Apply custom workspace theme color'
                    className='workspace-theme-color-apply'
                    onClick={() => chooseProjectThemeColor(customThemeColor)}
                    type='button'
                  >
                    <IconCheck aria-hidden='true' size={14} stroke={2.2} />
                  </button>
                </div>
                {recentThemeColors.length > 0 ? (
                  <div className='workspace-theme-color-palette'>
                    {recentThemeColors.map((themeColor) => (
                      <button
                        aria-label={`Use ${themeColor}`}
                        className='workspace-theme-color-palette-button'
                        key={themeColor}
                        onClick={() => chooseProjectThemeColor(themeColor)}
                        style={getProjectThemeSwatchStyle(themeColor)}
                        type='button'
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
                <button className='session-context-menu-item' onClick={copyProjectPath} role='menuitem' type='button'>
                  <IconCopy aria-hidden='true' className='session-context-menu-icon' size={14} />
                  Copy Path
                </button>
                {projectGitRemoteOriginUrl ? (
                  <button
                    className='session-context-menu-item'
                    onClick={copyProjectRemoteUrl}
                    role='menuitem'
                    type='button'
                  >
                    <IconLink aria-hidden='true' className='session-context-menu-icon' size={14} />
                    Copy Remote URL
                  </button>
                ) : null}
                <button
                  className='session-context-menu-item'
                  onClick={openProjectInFinder}
                  role='menuitem'
                  type='button'
                >
                  <IconFolderOpen aria-hidden='true' className='session-context-menu-icon' size={14} />
                  Open Folder
                </button>
                {/*
                 * CDXC:WorktreeRename 2026-08-10:
                 * Worktree rows deliberately do NOT offer the label-only
                 * Rename. It posts `renameWorkspaceProjectForGroup`, which
                 * the GPUI runtime has no case for, so on the desktop app it
                 * was a menu item that did nothing at all — and sat directly
                 * above a Rename Worktree that does, which is worse than
                 * absent. Renaming a worktree means moving the checkout, so
                 * the action below is the whole story for these rows.
                 * Ordinary project rows keep their label rename unchanged.
                 */}
                <button
                  className='session-context-menu-item'
                  onClick={promptRenameWorktree}
                  role='menuitem'
                  type='button'
                >
                  <IconGitBranch aria-hidden='true' className='session-context-menu-icon' size={14} />
                  Rename Worktree…
                </button>
                {onCreateProjectCollection && onMoveProjectToCollection ? (
                  <button
                    className='session-context-menu-item'
                    onClick={openProjectCollectionMenu}
                    role='menuitem'
                    type='button'
                  >
                    <IconPlus aria-hidden='true' className='session-context-menu-icon' size={14} />
                    Add to project group
                    <IconChevronRight aria-hidden='true' className='session-context-menu-trailing-icon' size={14} />
                  </button>
                ) : null}
                {onHideGroup ? (
                  <button
                    className='session-context-menu-item'
                    onClick={() => {
                      setContextMenuPosition(undefined);
                      onHideGroup();
                    }}
                    role='menuitem'
                    type='button'
                  >
                    <IconEyeOff aria-hidden='true' className='session-context-menu-icon' size={14} />
                    {isHidden ? 'Unhide' : 'Hide'}
                  </button>
                ) : null}
                <div className='session-context-menu-divider' role='separator' />
                <div aria-hidden='true' className='session-context-menu-spacer' />
                <button
                  className='session-context-menu-item session-context-menu-item-danger'
                  onClick={promptDeleteWorktree}
                  role='menuitem'
                  type='button'
                >
                  <IconTrash aria-hidden='true' className='session-context-menu-icon' size={14} />
                  Delete Worktree
                </button>
                <button
                  className='session-context-menu-item session-context-menu-item-danger'
                  disabled={!projectContext.canRemoveProject}
                  onClick={removeWorktreeProject}
                  role='menuitem'
                  type='button'
                >
                  <IconX aria-hidden='true' className='session-context-menu-icon' size={14} />
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
                <button className='session-context-menu-item' onClick={copyProjectPath} role='menuitem' type='button'>
                  <IconCopy aria-hidden='true' className='session-context-menu-icon' size={14} />
                  Copy Path
                </button>
                {projectGitRemoteOriginUrl ? (
                  <button
                    className='session-context-menu-item'
                    onClick={copyProjectRemoteUrl}
                    role='menuitem'
                    type='button'
                  >
                    <IconLink aria-hidden='true' className='session-context-menu-icon' size={14} />
                    Copy Remote URL
                  </button>
                ) : null}
                <button
                  className='session-context-menu-item'
                  onClick={openProjectInFinder}
                  role='menuitem'
                  type='button'
                >
                  <IconFolderOpen aria-hidden='true' className='session-context-menu-icon' size={14} />
                  Open Folder
                </button>
                {onCreateProjectCollection && onMoveProjectToCollection ? (
                  <button
                    className='session-context-menu-item'
                    onClick={openProjectCollectionMenu}
                    role='menuitem'
                    type='button'
                  >
                    <IconPlus aria-hidden='true' className='session-context-menu-icon' size={14} />
                    Add to project group
                    <IconChevronRight aria-hidden='true' className='session-context-menu-trailing-icon' size={14} />
                  </button>
                ) : null}
                {/*
                 * CDXC:SidebarSpaces 2026-08-27:
                 * Ungrouped projects only. A project inside a group takes its
                 * Spaces from that group and cannot be assigned on its own, so
                 * offering the entry there would promise an override the model
                 * does not have.
                 */}
                {isProjectSpacesMenuEnabled ? (
                  <button
                    className='session-context-menu-item'
                    onClick={openProjectSpacesMenu}
                    role='menuitem'
                    type='button'
                  >
                    <IconStack aria-hidden='true' className='session-context-menu-icon' size={14} />
                    Spaces
                    <IconChevronRight aria-hidden='true' className='session-context-menu-trailing-icon' size={14} />
                  </button>
                ) : null}
                <div className='session-context-menu-divider' role='separator' />
                {group.canCreateSessionGroup ? (
                  <button
                    className='session-context-menu-item'
                    onClick={requestCreateSessionGroup}
                    role='menuitem'
                    type='button'
                  >
                    <IconPlus aria-hidden='true' className='session-context-menu-icon' size={14} />
                    New Group
                  </button>
                ) : null}
                {onHideGroup ? (
                  <button
                    className='session-context-menu-item'
                    onClick={() => {
                      setContextMenuPosition(undefined);
                      onHideGroup();
                    }}
                    role='menuitem'
                    type='button'
                  >
                    <IconEyeOff aria-hidden='true' className='session-context-menu-icon' size={14} />
                    {isHidden ? 'Unhide' : 'Hide'}
                  </button>
                ) : null}
                <button
                  className='session-context-menu-item'
                  disabled={!allSessionsSleeping && !hasInactiveProjectSessionsToSleep}
                  onClick={() => {
                    if (allSessionsSleeping) {
                      requestWakeProjectSleepingSessions();
                      return;
                    }
                    requestSleepInactiveProjectSessions();
                  }}
                  role='menuitem'
                  type='button'
                >
                  {allSessionsSleeping ? (
                    <IconPlayerPlay aria-hidden='true' className='session-context-menu-icon' size={14} />
                  ) : (
                    <IconMoon aria-hidden='true' className='session-context-menu-icon' size={14} />
                  )}
                  {allSessionsSleeping ? 'Wake' : 'Sleep Inactive'}
                </button>
                {canFullReloadGroup ? (
                  <button
                    className='session-context-menu-item'
                    onClick={requestFullReloadGroup}
                    role='menuitem'
                    type='button'
                  >
                    <IconRefresh aria-hidden='true' className='session-context-menu-icon' size={14} />
                    Full reload
                  </button>
                ) : null}
                <div className='session-context-menu-divider' role='separator' />
                <button
                  className='session-context-menu-item session-context-menu-item-danger'
                  disabled={!hasInactiveProjectSessionsToSleep}
                  onClick={requestCloseInactiveProjectSessions}
                  role='menuitem'
                  type='button'
                >
                  <IconX aria-hidden='true' className='session-context-menu-icon' size={14} />
                  Close inactive
                </button>
                <button
                  className='session-context-menu-item session-context-menu-item-danger'
                  disabled={!canCloseProject}
                  onClick={closeProject}
                  role='menuitem'
                  type='button'
                >
                  <IconX aria-hidden='true' className='session-context-menu-icon' size={14} />
                  Close Project
                </button>
              </>
            )
          ) : (
            <>
              <button
                className='session-context-menu-item'
                onClick={() => {
                  setContextMenuPosition(undefined);
                  setIsEditing(true);
                }}
                role='menuitem'
                type='button'
              >
                <IconPencil aria-hidden='true' className='session-context-menu-icon' size={14} />
                Rename
              </button>
              {canFullReloadGroup ? (
                <button
                  className='session-context-menu-item'
                  onClick={requestFullReloadGroup}
                  role='menuitem'
                  type='button'
                >
                  <IconRefresh aria-hidden='true' className='session-context-menu-icon' size={14} />
                  Full reload
                </button>
              ) : null}
              <button
                className='session-context-menu-item'
                onClick={() => requestSetGroupSleeping(!allSessionsSleeping)}
                role='menuitem'
                type='button'
              >
                {allSessionsSleeping ? (
                  <IconPlayerPlay aria-hidden='true' className='session-context-menu-icon' size={14} />
                ) : (
                  <IconMoon aria-hidden='true' className='session-context-menu-icon' size={14} />
                )}
                {allSessionsSleeping ? 'Wake' : 'Sleep'}
              </button>
              <div className='session-context-menu-divider' role='separator' />
              <button
                className='session-context-menu-item session-context-menu-item-danger'
                disabled={!canClose}
                onClick={requestCloseGroup}
                role='menuitem'
                type='button'
              >
                <IconX aria-hidden='true' className='session-context-menu-icon' size={14} />
                Close
              </button>
            </>
          )}
        </SidebarContextMenuPortal>
      ) : null}
      {projectContext && openControlMenu === 'project-agent' ? (
        <SidebarContextMenuPortal
          menuClassName='session-context-menu group-agent-menu'
          menuRef={controlMenuRef}
          menuStyle={getPortalMenuStyle(projectAgentButtonRef.current, GROUP_AGENT_MENU_WIDTH_PX)}
          onDismiss={() => setOpenControlMenu(undefined)}
          vscode={vscode}
        >
          {/*
           * CDXC:ProjectAgents 2026-06-22-13:11:
           * Project-header agent menus can open near the bottom of the native sidebar.
           * Use the measured sidebar menu portal so long agent lists clamp to the visible webview and scroll instead of overflowing past the sidebar edge.
           */}
          {agents.map((agent) => (
            <button
              aria-label={agent.name}
              aria-pressed={primaryProjectAgent?.agentId === agent.agentId}
              className='session-context-menu-item group-control-menu-item group-agent-menu-item'
              data-selected={String(primaryProjectAgent?.agentId === agent.agentId)}
              key={agent.agentId}
              onClick={() => requestRunProjectAgent(agent)}
              role='menuitem'
              type='button'
            >
              <ProjectAgentLauncherIcon agent={agent} colorMode='brand' />
              <span className='group-agent-menu-label'>{agent.name}</span>
              <AgentMenuChatIndicator agent={agent} />
            </button>
          ))}
          {agents.length > 0 ? <div className='session-context-menu-divider' role='separator' /> : null}
          <button
            className='session-context-menu-item group-control-menu-item group-agent-menu-item'
            onClick={openConfigureAgentsModal}
            role='menuitem'
            type='button'
          >
            <IconSettings aria-hidden='true' className='session-context-menu-icon' size={14} />
            <span className='group-agent-menu-label'>Configure</span>
          </button>
        </SidebarContextMenuPortal>
      ) : null}
      {/**
       * CDXC:SessionClose 2026-05-11-00:45
       * Group close confirmation copy must use Close so bulk session removal
       * matches the session context menu and does not expose
       * process-lifecycle wording to users.
       */}
      <ConfirmationModal
        confirmLabel='Close Group'
        description={`This will close all ${orderedSessionIds.length} session${orderedSessionIds.length === 1 ? '' : 's'} in ${group.title}.`}
        isOpen={isConfirmOpen}
        onCancel={() => setIsConfirmOpen(false)}
        onConfirm={() => {
          setIsConfirmOpen(false);
          vscode.postMessage({
            groupId: group.groupId,
            type: 'closeGroup',
          });
        }}
        title='Close group?'
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
    Math.min((bounds?.right ?? position.x) - menuWidth, window.innerWidth - menuWidth - GROUP_CONTROL_MENU_MARGIN_PX)
  );

  return {
    left: `${left}px`,
    position: 'fixed' as const,
    top: `${position.y}px`,
    width: `${menuWidth}px`,
  };
}

let sessionGroupDebugInstanceCounter = 0;

function createSessionGroupDebugInstanceId(): number {
  sessionGroupDebugInstanceCounter += 1;
  return sessionGroupDebugInstanceCounter;
}

function getCollapsedSummaryLabel(indicatorActivity: 'attention' | 'working' | undefined): string | undefined {
  if (indicatorActivity === 'attention') {
    return 'Group has completed sessions';
  }

  if (indicatorActivity === 'working') {
    return 'Group has working sessions';
  }

  return undefined;
}

function getCollapsedProjectCountsLabel(
  summary: Pick<GroupSessionSummary, 'attentionCount' | 'workingCount'>,
  awakeCount: number
): string {
  const hasActionStatus = summary.workingCount > 0 || summary.attentionCount > 0;
  return [
    summary.workingCount > 0 ? `${summary.workingCount} working` : '',
    summary.attentionCount > 0 ? `${summary.attentionCount} attention` : '',
    !hasActionStatus && awakeCount > 0 ? `${awakeCount} awake terminals and browsers` : '',
  ]
    .filter(Boolean)
    .join(', ');
}
