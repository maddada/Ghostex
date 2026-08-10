import { IconFolders, IconLayoutList } from "@tabler/icons-react";
import { Fragment, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import type {
  SidebarNewSessionEnvMode,
  SidebarProjectGroupingMode,
  SidebarV2Layout,
  SidebarVersion,
  ghostexSettings,
} from "../../shared/ghostex-settings";
import { createSidebarV2ProjectGroupingSettings } from "../../shared/sidebar-v2-logical-project";
import type { SidebarAgentButton } from "../../shared/sidebar-agents";
import type {
  SidebarSessionItem,
  SidebarSessionLifecycleCapabilities,
  SidebarSessionTag,
} from "../../shared/session-grid-contract";
import {
  normalizeWorktreePathForComparison,
  resolveOrphanedWorktreePathForSession,
  resolveSidebarV2ManagedWorktreePath,
} from "../../shared/sidebar-v2-worktree-cleanup";
import {
  canSettleSidebarV2Session,
  canSnoozeSidebarV2Session,
  sidebarV2SessionWokeAtMs,
  type SidebarV2LifecycleCapabilities,
} from "../../shared/sidebar-v2-lifecycle";
import type { SidebarV2Session } from "../../shared/sidebar-v2-session";
import { formatSidebarV2SnoozeWakeLabel } from "../../shared/sidebar-v2-snooze";
import { reconcileSidebarV2CreationOrder } from "../../shared/sidebar-v2-sort";
import {
  firstValidTimestampMs,
  resolveSidebarV2Status,
  type SidebarV2Status,
} from "../../shared/sidebar-v2-status";
import { buildSidebarSessionDetailsClipboardText } from "../../shared/session-details-copy";
import { openAppModal } from "../app-modal-host-bridge";
import type { SidebarV2GroupOrderRow } from "../../shared/sidebar-v2-group-order";
import type { SidebarGroupDropTarget, SidebarSessionDropTarget } from "../sidebar-dnd";
import type { SidebarGroupRecord } from "../sidebar-store";
/*
 * CDXC:SidebarV2ContextMenuParity 2026-07-30:
 * The ONE thing V2 borrows from the V1 card: its exported, pure eligibility
 * resolver. Importing the answer rather than re-deriving the gates is what keeps
 * the two session menus from drifting apart about which agents can fork, which
 * sessions can be resumed from a copied command, and what a remote row may do.
 */
import { getSidebarSessionContextMenuEligibility } from "../sortable-session-card";
import type { WebviewApi } from "../webview-api";
import { useSidebarV2Clock } from "./sidebar-v2-clock";
import {
  SidebarV2ContextMenu,
  createSidebarV2ContextMenuSections,
  createSidebarV2ProjectGroupMenuSections,
  type SidebarV2ContextMenuPosition,
} from "./sidebar-v2-context-menu";
import { SidebarV2ProjectGroupSection } from "./sidebar-v2-group-header";
import {
  postSidebarV2CloseSession,
  postSidebarV2CloseWorkspaceProjects,
  postSidebarV2CopyAttachCommand,
  postSidebarV2CopyResumeCommand,
  postSidebarV2CopySessionDetails,
  postSidebarV2CreateWorktreeSession,
  postSidebarV2FocusSession,
  postSidebarV2FocusSessionMode,
  postSidebarV2ForkSession,
  postSidebarV2FullReloadSession,
  postSidebarV2GenerateSessionTitle,
  postSidebarV2RemoveSessionWorktree,
  postSidebarV2RenameSession,
  postSidebarV2RequestProjectWorktrees,
  postSidebarV2SetSessionPinned,
  postSidebarV2SetSessionSleeping,
  postSidebarV2SetSessionTag,
  postSidebarV2SettleSession,
  postSidebarV2SnoozeSession,
  postSidebarV2ToggleCloseAfterDone,
  postSidebarV2UnsettleSession,
  postSidebarV2UnsnoozeSession,
} from "./sidebar-v2-messages";
import { SidebarV2ScopeMenu } from "./sidebar-v2-scope-menu";
import { SidebarV2SessionRow, type SidebarV2SessionRowLifecycle } from "./sidebar-v2-session-row";
import { SidebarV2Shelf } from "./sidebar-v2-shelf";
import { SidebarV2SnoozePopover } from "./sidebar-v2-snooze-popover";
import {
  SidebarV2CreateButton,
  type SidebarV2CreateButtonPosition,
} from "./sidebar-v2-create-button";
import { SidebarV2WorktreeCleanupPrompt } from "./sidebar-v2-worktree-cleanup-prompt";
import {
  SidebarV2WorktreePopover,
  type SidebarV2WorktreeDraft,
  type SidebarV2WorktreeEventSource,
} from "./sidebar-v2-worktree-popover";
import {
  SIDEBAR_V2_ALL_SCOPE_ID,
  createSidebarV2ViewModel,
  type SidebarV2GroupModel,
  type SidebarV2ProjectIdentity,
} from "./sidebar-v2-view-model";

/**
 * CDXC:SidebarV2Lifecycle 2026-07-29:
 * How long a clicked settle/snooze control stays disabled without an answer.
 * The UI is not optimistic — the server's presentation delta moves the row —
 * so the ONLY purpose of the disabled window is to swallow a double click. It
 * has to expire on its own: gxserver answers an idempotent no-op with
 * `changed: false` and emits no delta at all, and a control waiting forever on
 * a delta that is never coming would be permanently dead.
 */
const SIDEBAR_V2_LIFECYCLE_PENDING_TIMEOUT_MS = 4_000;

/**
 * CDXC:SidebarV2 2026-07-29:
 * Sidebar V2 ("Inbox") is an opt-in presentation layer over the exact data the
 * classic sidebar already renders. It owns no session lifecycle of its own: the
 * host stays authoritative, so this contract carries only display-ready
 * projections plus the same message poster V1 uses for activation, context
 * menus, and renames.
 *
 * Prop rules for the V2 tree:
 * - `groupIds` is already ordered and filtered exactly like V1's displayed list
 *   (search + tag filters applied upstream), so V2 never re-derives visibility.
 * - Flat mode reads sessions through `sessionIdsByGroup` + `sessionsById`; group
 *   mode additionally reads `groupsById` for project identity and collapse state.
 * - Every mutation must go back through `vscode.postMessage` with the existing
 *   sidebar message types. Do not add a second host channel for V2.
 */
export type SidebarV2RootProps = {
  /**
   * CDXC:SidebarV2Worktree 2026-07-29:
   * The configured agent buttons, exactly as the shared header receives them.
   * V2's creation control needs them for two things: which agent the plain "+"
   * launches (the last-used one) and the worktree popover's agent picker.
   */
  agents?: readonly SidebarAgentButton[];
  /**
   * CDXC:SidebarV2LogicalProjects 2026-07-29:
   * The inactivity auto-settle window the LOCAL daemon states for itself. Absent
   * means that daemon predates the field, and V2 falls back to the user's
   * `sidebarAutoSettleAfterDays` setting — the same file the local daemon reads,
   * so the two ends still agree. `null` means the daemon has it switched off.
   */
  autoSettleAfterDays?: number | null;
  /**
   * The same window per REMOTE machine, keyed by `remoteMachineContext.machineId`.
   * A connected machine missing from this map gets NO client-side inactivity
   * settle at all: the local window is not that machine's window, and a daemon
   * that cannot state its window still publishes its own `settledOverride`,
   * which is then the single source of truth for its rows.
   */
  autoSettleAfterDaysByMachineId?: Readonly<Record<string, number | null>>;
  /** Group ids the user has collapsed; only meaningful in `byProject` layout. */
  collapsedGroupsById: Readonly<Record<string, true>>;
  /*
   * CDXC:SidebarV2GroupedProjectUX 2026-07-30:
   * Grouped V2 reorders projects through the SAME dnd-kit provider, indicator
   * state, and `syncGroupOrder` contract V1 uses; SidebarApp owns all three, so
   * the drag state travels down as props rather than V2 growing a second drag
   * pipeline. Absent props simply mean the host does not offer project reorder.
   */
  /** Representative group id of the row currently being dragged. */
  draggingGroupId?: string;
  /** The resolved insertion boundary, so the target row can paint V1's line. */
  groupDropIndicator?: SidebarGroupDropTarget;
  /** Mirrors V1's `draggingDisabled`: no manual sort mode, no reorder. */
  isGroupReorderDisabled?: boolean;
  /** Search and other transient filtering disable pinned-row reordering. */
  isPinnedSessionReorderDisabled?: boolean;
  /** Display-ordered group ids after V1 search and tag filtering. */
  groupIds: readonly string[];
  /** Group metadata without session payloads, exactly as the store holds it. */
  groupsById: Readonly<Record<string, SidebarGroupRecord>>;
  /**
   * The host's own project-level empty state — the gxserver-unavailable
   * recovery block (message plus "Load Sessions") and the first-project
   * onboarding copy. Both are recovery/onboarding surfaces that only the host
   * can build, and both must appear in BOTH sidebars, so V2 renders the node it
   * is handed instead of shipping a second copy of that decision. It outranks
   * V2's own empty copy: "No sessions yet" is never the useful thing to say
   * when the daemon is down or no project exists.
   */
  hostEmptyState?: ReactNode;
  /** True while the session search field is filtering the list. */
  isSearchFiltering: boolean;
  layout: SidebarV2Layout;
  /**
   * CDXC:SidebarV2Lifecycle 2026-07-29:
   * The LOCAL gxserver's settle/snooze capability, applied to every group that
   * carries no `remoteMachineContext`. Undefined means the daemon predates
   * session lifecycle, and every affordance hides.
   */
  lifecycleCapabilities?: SidebarSessionLifecycleCapabilities;
  /** Per-remote-machine capability, keyed by `remoteMachineContext.machineId`.
      A connected machine missing from this map is treated as incapable. */
  lifecycleCapabilitiesByMachineId?: Readonly<
    Record<string, SidebarSessionLifecycleCapabilities>
  >;
  /**
   * CDXC:SidebarV2Worktree 2026-07-29:
   * Where host answers arrive. SidebarApp listens on this same source, and gpui
   * hands the sidebar its OWN message source rather than `window`, so the
   * worktree popover cannot just subscribe to `window` and hope.
   */
  messageSource?: SidebarV2WorktreeEventSource;
  /**
   * CDXC:AddProject 2026-07-30:
   * Opens the shared add-project dialog. V2 hides the classic Projects section
   * header that used to carry Add Project, so the caller's own add-project path
   * travels here and appears in the single create control's menu.
   */
  onAddProject?: () => void;
  /**
   * CDXC:SidebarV2SingleCreateControl 2026-07-30:
   * The two Quick creators the shared V1 header used to own as its own buttons.
   * V2's header renders one create control, so they moved into its chevron menu
   * as explicitly-labelled items and travel here as the caller's OWN paths —
   * the very functions the classic header calls, posting the very same host
   * messages. Absent callers simply have no Quick items.
   */
  onCreateQuickBrowserTab?: () => void;
  onCreateQuickTerminal?: () => void;
  /**
   * CDXC:SidebarV2GroupedProjectUX 2026-07-30:
   * Reports the grouped rows AS RENDERED (representative id plus every merged
   * member) so the host can resolve a project drag against the rows on screen.
   * The caller could not derive this list itself without a second copy of the
   * cross-machine merge rules, free to drift from what the user is looking at.
   * Rows with no reorderable identity (the Quick collection) are excluded.
   */
  onGroupedRowsChange?: (rows: readonly SidebarV2GroupOrderRow[]) => void;
  onSetGroupCollapsed: (groupId: string, collapsed: boolean) => void;
  /** Persist the V2 sub-mode; writes through the shared settings pipeline. */
  onSetLayout: (layout: SidebarV2Layout) => void;
  /**
   * CDXC:SidebarV2Worktree 2026-07-29:
   * Persist the "+" default (instant local session vs worktree popover). Same
   * settings pipeline as the layout switch; omitted by callers that do not
   * offer the preference.
   */
  onSetNewSessionsDefaultEnvMode?: (mode: SidebarNewSessionEnvMode) => void;
  /**
   * CDXC:SidebarV2LogicalProjects 2026-07-29:
   * Persist the whole `sidebarProjectGroupingOverrides` record. V2 hands over
   * the complete next record rather than one key, so the caller does not have
   * to re-derive the merge semantics of a patch, and so clearing an override
   * (back to the automatic rule) is expressible as an absent key.
   */
  onSetProjectGroupingOverrides?: (
    overrides: Readonly<Record<string, SidebarProjectGroupingMode>>,
  ) => void;
  /** Return to the classic sidebar from inside V2's own chrome. */
  onSetSidebarVersion: (sidebarVersion: SidebarVersion) => void;
  /** Pointer-resolved insertion boundary from SidebarApp's shared DnD pipeline. */
  pinnedSessionDropIndicator?: SidebarSessionDropTarget;
  /**
   * CDXC:SidebarV2Worktree 2026-07-29:
   * Start an instant session with `agent`, in `groupId` when the click came
   * from a project group and in the Quick collection otherwise. This is the
   * caller's OWN launch path (the one the classic header already uses), so the
   * plain "+" posts exactly the message it always did.
   */
  onRunAgent?: (agent: SidebarAgentButton, groupId?: string) => void;
  /** Last-used agent id, as tracked by the shared launcher storage. */
  primaryAgentId?: string;
  /** Raw search text, for empty-state copy only; filtering already happened. */
  searchQuery: string;
  selectedSessionTagFilters: readonly SidebarSessionTag[];
  /** Display-ordered session ids per group, after search and tag filtering. */
  sessionIdsByGroup: Readonly<Record<string, readonly string[]>>;
  sessionsById: Readonly<Record<string, SidebarSessionItem>>;
  settings: ghostexSettings;
  vscode: WebviewApi;
};

type SidebarV2MenuState = {
  position: SidebarV2ContextMenuPosition;
  sessionId: string;
};

/*
 * CDXC:SidebarV2LogicalProjects 2026-07-29:
 * The project group header's own menu. It is keyed by the group's
 * REPRESENTATIVE id (the id the header renders with), so a re-group caused by
 * the very choice made in this menu re-resolves the target instead of leaving
 * the menu pointing at a group that no longer exists.
 */
type SidebarV2GroupMenuState = {
  groupId: string;
  position: SidebarV2ContextMenuPosition;
};

/*
 * CDXC:SidebarV2Worktree 2026-07-29:
 * One open worktree popover. `existingWorktreePath` is set only by the
 * "New session on <branch>" path, which spawns straight into a checkout that
 * already exists and therefore never opens the form at all.
 */
type SidebarV2WorktreePopoverState = {
  groupId: string;
  position: SidebarV2CreateButtonPosition;
};

type SidebarV2WorktreeCleanupState = {
  errorMessage?: string;
  groupId: string;
  /** The close command was already posted; a dirty re-ask must not repeat it. */
  hasClosedSession: boolean;
  isDirty: boolean;
  requestId?: string;
  sessionId: string;
  warnings?: readonly string[];
  worktreePath: string;
};

function createSidebarV2RequestId(prefix: string): string {
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}

/**
 * CDXC:SidebarV2ContextMenuParity 2026-07-30:
 * The heading the app-modal host puts above Delayed Send and the 1st-message
 * viewer. It resolves the same way V1's does, so a modal opened from either
 * sidebar is titled identically for the same session.
 */
function sidebarV2SessionModalTitle(session: SidebarSessionItem): string {
  return session.primaryTitle?.trim() || session.terminalTitle?.trim() || session.alias;
}

export function SidebarV2Root({
  agents,
  autoSettleAfterDays,
  autoSettleAfterDaysByMachineId,
  collapsedGroupsById,
  draggingGroupId,
  groupDropIndicator,
  groupIds,
  groupsById,
  hostEmptyState,
  isGroupReorderDisabled = false,
  isPinnedSessionReorderDisabled = false,
  isSearchFiltering,
  layout,
  lifecycleCapabilities,
  lifecycleCapabilitiesByMachineId,
  messageSource,
  onAddProject,
  onCreateQuickBrowserTab,
  onCreateQuickTerminal,
  onGroupedRowsChange,
  onSetGroupCollapsed,
  onSetLayout,
  onSetNewSessionsDefaultEnvMode,
  onSetProjectGroupingOverrides,
  onSetSidebarVersion,
  pinnedSessionDropIndicator,
  onRunAgent,
  primaryAgentId,
  searchQuery,
  selectedSessionTagFilters,
  sessionIdsByGroup,
  sessionsById,
  settings,
  vscode,
}: SidebarV2RootProps) {
  const clockMs = useSidebarV2Clock();
  /*
   * CDXC:SidebarV2Lifecycle 2026-07-29:
   * `wakeTick` is bumped by a timeout armed exactly on the next snooze
   * boundary. The shared clock is quantized to 30s on purpose (a per-second
   * whole-inbox re-render is the scroll-linked paint work this sidebar spent a
   * lot of effort removing), but wake times are second-precise, so classifying
   * on the quantized clock alone would keep a woken row on the shelf for up to
   * half a minute. `nowMs` therefore reads a FRESH clock whenever either input
   * changes — the quantized tick for ageing labels, the boundary tick for the
   * wake itself.
   */
  const [wakeTick, setWakeTick] = useState(0);
  const nowMs = useMemo(() => Date.now(), [clockMs, wakeTick]);
  const [scopeId, setScopeId] = useState<string>(SIDEBAR_V2_ALL_SCOPE_ID);
  const [renamingSessionId, setRenamingSessionId] = useState<string>();
  const [menuState, setMenuState] = useState<SidebarV2MenuState>();
  const [groupMenuState, setGroupMenuState] = useState<SidebarV2GroupMenuState>();
  const [snoozeMenuState, setSnoozeMenuState] = useState<SidebarV2MenuState>();
  /*
   * Which rows have an unanswered lifecycle write. Keyed by session id, valued
   * by the lifecycle signature at click time, so a landing delta clears the
   * pending flag by itself; the timeout below covers the idempotent no-op case
   * where no delta will ever arrive.
   */
  const [pendingLifecycleBySessionId, setPendingLifecycleBySessionId] = useState<
    Readonly<Record<string, string>>
  >({});
  /*
   * Shelf defaults keep Settled open (it is the recent history you
   * scroll back through), Snoozed stays shut (you asked not to see it).
   */
  /*
   * CDXC:SidebarV2Worktree 2026-07-29:
   * Worktree flow state. The popover is not optimistic either: the created
   * session arrives as a presentation delta, and `pendingWorktreeRequestId`
   * only keeps the form disabled until the host answers the exact request the
   * user submitted. Refs shadow both request ids so the result listener can be
   * subscribed once instead of re-subscribing on every keystroke.
   */
  const [worktreePopoverState, setWorktreePopoverState] =
    useState<SidebarV2WorktreePopoverState>();
  const [pendingWorktreeRequestId, setPendingWorktreeRequestId] = useState<string>();
  const [worktreeErrorMessage, setWorktreeErrorMessage] = useState<string>();
  const [cleanupState, setCleanupState] = useState<SidebarV2WorktreeCleanupState>();
  const pendingWorktreeRequestIdRef = useRef<string | undefined>(undefined);
  const cleanupRequestIdRef = useRef<string | undefined>(undefined);
  const [isSettledExpanded, setIsSettledExpanded] = useState(true);
  const [isSnoozedExpanded, setIsSnoozedExpanded] = useState(false);
  const [isBrowserExpanded, setIsBrowserExpanded] = useState(true);
  /*
   * Grouped mode gets one shelf state per project per tone. Stored as explicit
   * overrides keyed `groupId:tone` so a project the user never touched keeps
   * the shared defaults instead of being seeded with a stale snapshot.
   */
  const [projectShelfOverrides, setProjectShelfOverrides] = useState<
    Readonly<Record<string, boolean>>
  >({});

  /*
   * First-seen registry. `createdAt` from gxserver is authoritative whenever it
   * is present; this carries the fallback order for hosts that do not publish
   * it, and it must survive re-renders without ever reordering known rows.
   */
  const creationOrderRef = useRef<readonly string[]>([]);
  const creationOrder = useMemo(() => {
    const sessionIds = groupIds.flatMap((groupId) => [...(sessionIdsByGroup[groupId] ?? [])]);
    const nextOrder = reconcileSidebarV2CreationOrder({
      knownOrder: creationOrderRef.current,
      sessionIds,
    });
    creationOrderRef.current = nextOrder;
    return nextOrder;
  }, [groupIds, sessionIdsByGroup]);

  /*
   * CDXC:SidebarV2Lifecycle 2026-07-29:
   * Settle/snooze support is a per-DAEMON fact and this one list mixes daemons:
   * local groups answer from `lifecycleCapabilities`, remote groups answer from
   * their own machine's snapshot. A remote machine that is connected but absent
   * from the map resolves to incapable rather than inheriting local support —
   * showing a settle button the remote daemon would reject is worse than
   * showing none.
   */
  const capabilitiesByGroupId = useMemo(() => {
    const resolved: Record<string, SidebarV2LifecycleCapabilities> = {};
    for (const groupId of groupIds) {
      const machineId = groupsById[groupId]?.remoteMachineContext?.machineId;
      const capabilities =
        machineId === undefined
          ? lifecycleCapabilities
          : lifecycleCapabilitiesByMachineId?.[machineId];
      resolved[groupId] = {
        settle: capabilities?.sessionSettlement === true,
        snooze: capabilities?.sessionSnooze === true,
      };
    }
    return resolved;
  }, [groupIds, groupsById, lifecycleCapabilities, lifecycleCapabilitiesByMachineId]);

  /*
   * CDXC:SidebarV2Git 2026-07-29:
   * The git/PR probe is per DAEMON exactly like settle/snooze — only the
   * machine holding the working tree can run git in it — so it resolves
   * through the same machine-scoped rule: local groups answer from the local
   * capability, remote groups from their own machine's, and a machine missing
   * from the map answers "no". It is a separate map rather than a third field
   * on `SidebarV2LifecycleCapabilities` because that type is the input to the
   * settle/snooze predicates, and git support has no say in whether a session
   * can be settled.
   */
  const gitStatusCapabilityByGroupId = useMemo(() => {
    const resolved: Record<string, boolean> = {};
    for (const groupId of groupIds) {
      const machineId = groupsById[groupId]?.remoteMachineContext?.machineId;
      const capabilities =
        machineId === undefined
          ? lifecycleCapabilities
          : lifecycleCapabilitiesByMachineId?.[machineId];
      resolved[groupId] = capabilities?.sessionGitStatus === true;
    }
    return resolved;
  }, [groupIds, groupsById, lifecycleCapabilities, lifecycleCapabilitiesByMachineId]);

  /*
   * CDXC:SidebarV2Worktree 2026-07-29:
   * Worktree support resolves through the SAME machine-scoped rule as
   * settle/snooze and git: only the daemon that holds the repository can cut a
   * checkout in it. A machine missing from the map answers "no", so a remote
   * project on an un-upgraded gxserver shows the plain "+" and nothing else.
   */
  const worktreeCapabilityByGroupId = useMemo(() => {
    const resolved: Record<string, boolean> = {};
    for (const groupId of groupIds) {
      const machineId = groupsById[groupId]?.remoteMachineContext?.machineId;
      const capabilities =
        machineId === undefined
          ? lifecycleCapabilities
          : lifecycleCapabilitiesByMachineId?.[machineId];
      /*
       * A worktree needs a real code project to cut from, so the Quick
       * collection and any group without project context can never offer it,
       * however capable its daemon is.
       */
      resolved[groupId] =
        capabilities?.worktreeSessions === true &&
        groupsById[groupId]?.projectContext !== undefined;
    }
    return resolved;
  }, [groupIds, groupsById, lifecycleCapabilities, lifecycleCapabilitiesByMachineId]);

  /*
   * CDXC:SidebarV2LogicalProjects 2026-07-29:
   * The auto-settle WINDOW is per daemon for exactly the same reason capability
   * is: every daemon runs its own sweep against its own
   * `sidebarAutoSettleAfterDays`. Before this map existed, the local user's
   * window was applied to every remote machine's rows, so a machine configured
   * for 14 days had its sessions parked on the Settled shelf after 3 (the
   * recorded P2 minor).
   *
   * The rule, per group:
   * - local group, daemon states a window -> that window.
   * - local group, daemon says nothing -> the local SETTING, which is the very
   *   file the local daemon reads, so nothing changes for single-machine users.
   * - remote group, machine states a window -> that machine's window.
   * - remote group, machine says nothing -> null. Not the local window: the
   *   remote daemon's own `settledOverride` is then the only truthful source,
   *   and inventing a client-side settle would contradict it.
   */
  const autoSettleAfterDaysByGroupId = useMemo(() => {
    const resolved: Record<string, number | null> = {};
    for (const groupId of groupIds) {
      const machineId = groupsById[groupId]?.remoteMachineContext?.machineId;
      if (machineId === undefined) {
        resolved[groupId] =
          autoSettleAfterDays === undefined
            ? settings.sidebarAutoSettleAfterDays
            : autoSettleAfterDays;
        continue;
      }
      resolved[groupId] = autoSettleAfterDaysByMachineId?.[machineId] ?? null;
    }
    return resolved;
  }, [
    autoSettleAfterDays,
    autoSettleAfterDaysByMachineId,
    groupIds,
    groupsById,
    settings.sidebarAutoSettleAfterDays,
  ]);

  const projectGrouping = useMemo(
    () => createSidebarV2ProjectGroupingSettings(settings.sidebarProjectGroupingOverrides),
    [settings.sidebarProjectGroupingOverrides],
  );

  const viewModel = useMemo(
    () =>
      createSidebarV2ViewModel({
        autoSettleAfterDays: settings.sidebarAutoSettleAfterDays,
        autoSettleAfterDaysByGroupId,
        capabilitiesByGroupId,
        creationOrder,
        groupIds,
        groupsById,
        nowMs,
        projectGrouping,
        scopeId,
        sessionIdsByGroup,
        sessionsById,
      }),
    [
      autoSettleAfterDaysByGroupId,
      capabilitiesByGroupId,
      creationOrder,
      groupIds,
      groupsById,
      nowMs,
      projectGrouping,
      scopeId,
      sessionIdsByGroup,
      settings.sidebarAutoSettleAfterDays,
      sessionsById,
    ],
  );

  /*
   * CDXC:SidebarV2GroupedProjectUX 2026-07-30:
   * The reorderable rows, reported up so the host's drag pipeline resolves drops
   * against the list actually on screen. The Quick collection is excluded: it is
   * not a project, has no persisted order, and letting it act as an insertion
   * boundary would let a project be "reordered" relative to a row that cannot
   * move. Flat mode reports nothing, because it renders no project rows at all.
   */
  const groupOrderRows = useMemo<readonly SidebarV2GroupOrderRow[]>(
    () =>
      layout === "byProject"
        ? viewModel.groups
          .filter((group) => !group.isQuick)
          .map((group) => ({
            groupId: group.groupId,
            memberGroupIds: group.memberGroupIds,
          }))
        : [],
    [layout, viewModel.groups],
  );
  useEffect(() => {
    onGroupedRowsChange?.(groupOrderRows);
  }, [groupOrderRows, onGroupedRowsChange]);

  /*
   * One timeout on the soonest wake, re-armed whenever that boundary moves.
   * The delay is clamped to the signed-32-bit setTimeout ceiling: a larger
   * value overflows and fires immediately, which would turn a far-future wake
   * into a tight re-arm loop. Clamped, it just re-arms every ~24.8 days.
   */
  useEffect(() => {
    const nextWakeAtMs = viewModel.nextWakeAtMs;
    if (nextWakeAtMs === null) {
      return undefined;
    }
    const delayMs = Math.min(Math.max(0, nextWakeAtMs - Date.now()) + 50, 2_147_483_647);
    const timeoutId = window.setTimeout(() => {
      setWakeTick((tick) => tick + 1);
    }, delayMs);
    return () => {
      window.clearTimeout(timeoutId);
    };
  }, [viewModel.nextWakeAtMs]);

  const useColoredAgentIcons = settings.useColoredSessionAgentIcons === true;
  const showProjectIcons = settings.showProjectIcons;

  /*
   * CDXC:SidebarV2 2026-07-29:
   * ONE inbox, one current session. The store carries a focused session per
   * PROJECT — V1 renders a highlighted row inside every project group — so the
   * flat inbox has to intersect that with the active project, which is exactly
   * the rule gxserver's own projection applies (`isActiveProject &&
   * focusedSessionId === …`). Reading `isFocused` alone highlights one row per
   * project and the inbox stops having a "you are here".
   */
  const activeSessionId = useMemo(() => {
    const activeGroupId = groupIds.find((groupId) => groupsById[groupId]?.isActive === true);
    if (activeGroupId === undefined) {
      return undefined;
    }
    return (sessionIdsByGroup[activeGroupId] ?? []).find(
      (sessionId) => sessionsById[sessionId]?.isFocused === true,
    );
  }, [groupIds, groupsById, sessionIdsByGroup, sessionsById]);

  const resolveStatus = (session: SidebarV2Session): SidebarV2Status => {
    return resolveSidebarV2Status(session, {
      /*
       * The session the user is looking at has, by definition, been seen. Feed
       * that through as the visit stamp so the focused row stops shouting
       * "Done" the moment its run finishes.
       */
      lastVisitedAtMs: session.sessionId === activeSessionId ? nowMs : null,
      nowMs,
    });
  };

  const closeMenu = () => setMenuState(undefined);

  const activateSession = (sessionId: string) => {
    postSidebarV2FocusSession(vscode, sessionId);
  };

  /*
   * CDXC:SidebarV2Worktree 2026-07-29:
   * ── creation ──────────────────────────────────────────────────────────────
   * The plain "+" is NOT a new command: it calls the launch path the caller
   * already owns (`onRunAgent`), which is the same one the classic header's
   * agent button uses, with the same last-used agent. V2 adds the worktree
   * branch beside it, never in front of it.
   */
  const configuredAgents = useMemo(
    () => (agents ?? []).filter((agent) => agent.command?.trim()),
    [agents],
  );
  const primaryAgent =
    configuredAgents.find((agent) => agent.agentId === primaryAgentId) ??
    configuredAgents.find((agent) => agent.isDefault) ??
    configuredAgents[0];

  const canCreateWorktreeInGroup = (groupId: string | undefined): boolean =>
    groupId !== undefined && worktreeCapabilityByGroupId[groupId] === true;

  /*
   * Which project the HEADER's worktree entry targets. The header is not inside
   * a project, so it answers in the order the user's attention is: the scoped
   * project, then the active one, then the first capable project. When nothing
   * qualifies the chevron does not render at all — a worktree with no repo to
   * cut from is not a thing the button can offer.
   */
  const headerWorktreeGroupId = useMemo(() => {
    if (scopeId !== SIDEBAR_V2_ALL_SCOPE_ID && worktreeCapabilityByGroupId[scopeId] === true) {
      return scopeId;
    }
    const activeGroupId = groupIds.find(
      (groupId) =>
        groupsById[groupId]?.isActive === true && worktreeCapabilityByGroupId[groupId] === true,
    );
    return (
      activeGroupId ??
      groupIds.find((groupId) => worktreeCapabilityByGroupId[groupId] === true)
    );
  }, [groupIds, groupsById, scopeId, worktreeCapabilityByGroupId]);

  /*
   * CDXC:SidebarV2SingleCreateControl 2026-07-30:
   * Which project the HEADER's plain "+" creates in. Same order of attention as
   * the worktree resolution above — scoped project, active project, first
   * project — but WITHOUT the worktree-capability filter, because creating an
   * ordinary session needs nothing from the daemon beyond being a project.
   *
   * `projectContext !== undefined` is what makes a group a project: the Quick
   * collection has none. So when this resolves to `undefined` there genuinely is
   * no project to create in, and the launch path's Quick substitution is then
   * the only correct answer rather than a silent downgrade.
   */
  const headerCreateGroupId = useMemo(() => {
    const isProjectGroup = (groupId: string): boolean =>
      groupsById[groupId]?.projectContext !== undefined;
    if (scopeId !== SIDEBAR_V2_ALL_SCOPE_ID && isProjectGroup(scopeId)) {
      return scopeId;
    }
    const activeGroupId = groupIds.find(
      (groupId) => groupsById[groupId]?.isActive === true && isProjectGroup(groupId),
    );
    return activeGroupId ?? groupIds.find(isProjectGroup);
  }, [groupIds, groupsById, scopeId]);

  const runInstantSession = (groupId?: string, agent?: SidebarAgentButton) => {
    const launchAgent = agent ?? primaryAgent;
    if (!onRunAgent || !launchAgent) {
      return;
    }
    onRunAgent(launchAgent, groupId);
  };

  const openWorktreePopover = (
    position: SidebarV2CreateButtonPosition,
    groupId: string | undefined,
  ) => {
    if (!groupId) {
      return;
    }
    setWorktreeErrorMessage(undefined);
    setWorktreePopoverState({ groupId, position });
  };

  const submitWorktreeSession = (groupId: string, draft: SidebarV2WorktreeDraft) => {
    const requestId = createSidebarV2RequestId("sidebar-v2-worktree");
    pendingWorktreeRequestIdRef.current = requestId;
    setPendingWorktreeRequestId(requestId);
    setWorktreeErrorMessage(undefined);
    postSidebarV2CreateWorktreeSession(vscode, {
      agentId: draft.agentId,
      baseBranch: draft.baseBranch,
      existingWorktreePath: draft.existingWorktreePath,
      firstPrompt: draft.firstPrompt,
      projectId: groupId,
      requestId,
      startFromOrigin: draft.startFromOrigin,
    });
  };

  /*
   * "New session on <branch>" spawns straight into the checkout
   * the clicked row already lives in. There is nothing left to choose — the
   * worktree exists, the branch is fixed — so it posts directly instead of
   * opening the form with one pre-filled field.
   */
  const startSessionOnExistingWorktree = (groupId: string, worktreePath: string) => {
    submitWorktreeSession(groupId, {
      agentId: primaryAgent?.agentId,
      existingWorktreePath: worktreePath,
    });
  };

  /*
   * CDXC:SidebarV2Worktree 2026-07-29:
   * ── cleanup ───────────────────────────────────────────────────────────────
   * A worktree is "managed" when its branch is in this flow's own `ghostex/`
   * namespace AND the session's cwd names the checkout. Worktrees the user made
   * by hand, and checkouts reached through "open existing", keep their own
   * branch names and are therefore never proposed for deletion — closing those
   * sessions behaves exactly as it did before this phase.
   *
   * Every row also carries its raw `cwd`, which is what decides whether the
   * checkout is SHARED. The managed answer depends on the periodic git probe, so
   * deriving "is anyone else in this folder?" from it would race that probe and
   * offer to delete a directory another, freshly created session is sitting in.
   */
  const managedWorktreeSessions = useMemo(
    () =>
      Object.values(sessionsById).map((item) => {
        const worktreePath = resolveSidebarV2ManagedWorktreePath(item);
        return worktreePath === null
          ? { cwd: item.cwd, sessionId: item.sessionId }
          : { cwd: item.cwd, sessionId: item.sessionId, worktreePath };
      }),
    [sessionsById],
  );

  const groupIdForSession = (sessionId: string): string | undefined =>
    groupIds.find((groupId) => (sessionIdsByGroup[groupId] ?? []).includes(sessionId));

  const closeSession = (sessionId: string) => {
    const groupId = groupIdForSession(sessionId);
    const orphanedWorktreePath = canCreateWorktreeInGroup(groupId)
      ? resolveOrphanedWorktreePathForSession(managedWorktreeSessions, sessionId)
      : null;
    if (orphanedWorktreePath && groupId) {
      cleanupRequestIdRef.current = undefined;
      setCleanupState({
        groupId,
        hasClosedSession: false,
        isDirty: false,
        sessionId,
        worktreePath: orphanedWorktreePath,
      });
      return;
    }
    postSidebarV2CloseSession(vscode, sessionId);
  };

  const finishCleanup = (removeWorktree: boolean) => {
    const state = cleanupState;
    if (!state) {
      return;
    }
    if (!state.hasClosedSession) {
      postSidebarV2CloseSession(vscode, state.sessionId);
    }
    if (!removeWorktree) {
      setCleanupState(undefined);
      return;
    }
    const requestId = createSidebarV2RequestId("sidebar-v2-worktree-remove");
    cleanupRequestIdRef.current = requestId;
    setCleanupState({
      ...state,
      errorMessage: undefined,
      hasClosedSession: true,
      requestId,
    });
    postSidebarV2RemoveSessionWorktree(vscode, {
      /*
       * The second pass forces. gxserver's `dirty` answer is a refusal, and the
       * user has now seen exactly what it refused over.
       */
      force: state.isDirty ? true : undefined,
      projectId: state.groupId,
      requestId,
      worktreePath: state.worktreePath,
    });
  };

  /*
   * One listener for both worktree answers. Subscribing once (and matching on
   * the request id through refs) keeps a re-render from tearing the listener
   * down mid-flight, which is how a slow host answer gets dropped.
   */
  useEffect(() => {
    const source: SidebarV2WorktreeEventSource = messageSource ?? window;
    const handleMessage = (event: Event) => {
      const data = (event as MessageEvent<Record<string, unknown> | undefined>).data;
      if (!data || typeof data !== "object") {
        return;
      }
      if (data.type === "worktreeSessionResult") {
        if (data.requestId !== pendingWorktreeRequestIdRef.current) {
          return;
        }
        pendingWorktreeRequestIdRef.current = undefined;
        setPendingWorktreeRequestId(undefined);
        if (data.ok === true) {
          setWorktreeErrorMessage(undefined);
          setWorktreePopoverState(undefined);
          return;
        }
        setWorktreeErrorMessage(
          typeof data.error === "string" && data.error.trim()
            ? data.error
            : "Could not create the worktree session.",
        );
        return;
      }
      if (data.type !== "sessionWorktreeRemovalResult") {
        return;
      }
      if (data.requestId !== cleanupRequestIdRef.current) {
        return;
      }
      cleanupRequestIdRef.current = undefined;
      const warnings = Array.isArray(data.warnings)
        ? data.warnings.filter((warning): warning is string => typeof warning === "string")
        : undefined;
      setCleanupState((previous) => {
        if (!previous) {
          return previous;
        }
        if (data.ok === true && data.removed === true) {
          return undefined;
        }
        if (data.ok === true && data.dirty === true) {
          return { ...previous, isDirty: true, requestId: undefined, warnings };
        }
        return {
          ...previous,
          errorMessage:
            typeof data.error === "string" && data.error.trim()
              ? data.error
              : "Could not remove the worktree.",
          requestId: undefined,
          warnings,
        };
      });
    };
    source.addEventListener("message", handleMessage);
    return () => {
      source.removeEventListener("message", handleMessage);
    };
  }, [messageSource]);

  /*
   * CDXC:SidebarV2Lifecycle 2026-07-29:
   * The pending signature is the exact server-owned state the write is trying
   * to change. When the presentation delta lands, the signature changes and the
   * control re-enables without any explicit acknowledgement channel.
   */
  const lifecycleSignature = (session: SidebarV2Session): string =>
    [
      session.settledOverride ?? "",
      session.settledAt ?? "",
      session.snoozedUntil ?? "",
      session.snoozedAt ?? "",
    ].join("|");

  const isLifecyclePending = (session: SidebarV2Session): boolean =>
    pendingLifecycleBySessionId[session.sessionId] === lifecycleSignature(session);

  const markLifecyclePending = (session: SidebarV2Session) => {
    const sessionId = session.sessionId;
    const signature = lifecycleSignature(session);
    setPendingLifecycleBySessionId((previous) => ({ ...previous, [sessionId]: signature }));
    window.setTimeout(() => {
      setPendingLifecycleBySessionId((previous) => {
        if (previous[sessionId] !== signature) {
          return previous;
        }
        const { [sessionId]: _released, ...rest } = previous;
        return rest;
      });
    }, SIDEBAR_V2_LIFECYCLE_PENDING_TIMEOUT_MS);
  };

  const runLifecycleCommand = (session: SidebarV2Session, command: () => void) => {
    markLifecyclePending(session);
    command();
  };

  const settleSession = (session: SidebarV2Session) =>
    runLifecycleCommand(session, () => postSidebarV2SettleSession(vscode, session.sessionId));
  const unsettleSession = (session: SidebarV2Session) =>
    runLifecycleCommand(session, () => postSidebarV2UnsettleSession(vscode, session.sessionId));
  const wakeSession = (session: SidebarV2Session) =>
    runLifecycleCommand(session, () => postSidebarV2UnsnoozeSession(vscode, session.sessionId));
  const snoozeSession = (session: SidebarV2Session, snoozedUntil: string) =>
    runLifecycleCommand(session, () =>
      postSidebarV2SnoozeSession(vscode, session.sessionId, snoozedUntil),
    );

  const sessionCapabilities = (session: SidebarV2Session): SidebarV2LifecycleCapabilities =>
    viewModel.capabilitiesByGroupId[session.projectId ?? ""] ?? { settle: false, snooze: false };

  /**
   * The hover-slot lifecycle contract for one row. `shelf` is where the row is
   * being rendered, because the offered action belongs to the shelf, not to the
   * session: the inbox offers settle+snooze, the settled shelf offers
   * un-settle, and the snoozed shelf offers wake.
   */
  const resolveRowLifecycle = (
    session: SidebarV2Session,
    shelf: "inbox" | "settled" | "snoozed",
    options: { isBrowser: boolean },
  ): SidebarV2SessionRowLifecycle => {
    const capabilities = sessionCapabilities(session);
    const wakeAtMs = firstValidTimestampMs(session.snoozedUntil);
    const isWoke =
      /*
       * The active session has, by definition, just been looked at, so its Woke
       * badge is retired the same way the "Done" label is. gxserver retains
       * `snoozedUntil` for ~24h after the wake precisely so this indicator can
       * survive the trip; it is the client that decides when it stops helping.
       */
      session.sessionId !== activeSessionId &&
      sidebarV2SessionWokeAtMs(session, { capabilities, nowMs }) !== null;

    if (options.isBrowser) {
      return {
        action: "none",
        isPending: false,
        isWoke: false,
        onSettle: () => undefined,
        onSnooze: () => undefined,
        onUnsettle: () => undefined,
        onWake: () => undefined,
        showSnooze: false,
      };
    }

    const action =
      shelf === "settled"
        ? capabilities.settle
          ? "unsettle"
          : "none"
        : shelf === "snoozed"
          ? capabilities.snooze
            ? "wake"
            : "none"
          : canSettleSidebarV2Session(session, { capabilities })
            ? "settle"
            : "none";

    return {
      action,
      isPending: isLifecyclePending(session),
      isWoke,
      onSettle: () => settleSession(session),
      onSnooze: (position) =>
        setSnoozeMenuState({ position, sessionId: session.sessionId }),
      onUnsettle: () => unsettleSession(session),
      onWake: () => wakeSession(session),
      showSnooze:
        shelf === "inbox" && canSnoozeSidebarV2Session(session, { capabilities }),
      wakeLabel:
        shelf === "snoozed" && wakeAtMs !== null
          ? formatSidebarV2SnoozeWakeLabel(wakeAtMs, nowMs)
          : undefined,
    };
  };

  const renderRow = (
    session: SidebarV2Session,
    options: {
      project?: SidebarV2ProjectIdentity;
      shelf?: "inbox" | "settled" | "snoozed";
      slimLabel?: string;
      variant: "card" | "slim";
    },
  ) => {
    const item = sessionsById[session.sessionId];
    if (!item) {
      return null;
    }
    const isBrowser = item.kind === "browser" || item.sessionKind === "browser";
    const dragGroupId = session.projectId;
    return (
      <SidebarV2SessionRow
        dragGroupId={dragGroupId}
        dragIndex={
          dragGroupId ? (sessionIdsByGroup[dragGroupId] ?? []).indexOf(session.sessionId) : 0
        }
        dropPosition={
          pinnedSessionDropIndicator?.kind === "session" &&
          pinnedSessionDropIndicator.groupId === dragGroupId &&
          pinnedSessionDropIndicator.sessionId === session.sessionId
            ? pinnedSessionDropIndicator.position
            : undefined
        }
        /*
         * CDXC:SidebarV2Git 2026-07-29:
         * Capability is applied HERE, not in the row: an un-upgraded daemon's
         * rows are handed `undefined` and are therefore byte-identical to rows
         * from a session with no git data at all.
         */
        gitStatus={
          gitStatusCapabilityByGroupId[session.projectId ?? ""] === true
            ? item.gitStatus
            : undefined
        }
        isActive={session.sessionId === activeSessionId}
        isMenuOpen={
          menuState?.sessionId === session.sessionId ||
          snoozeMenuState?.sessionId === session.sessionId
        }
        isRenaming={renamingSessionId === session.sessionId}
        key={session.sessionId}
        lifecycle={resolveRowLifecycle(session, options.shelf ?? "inbox", { isBrowser })}
        /*
         * CDXC:SidebarV2LogicalProjects 2026-07-29:
         * The badge is resolved from the row's OWN host group, never from the
         * group it is rendered under: a merged cross-machine group renders
         * rows from several machines side by side, and the whole point of the
         * badge is to say which one each row came from. Local groups carry no
         * machine name, so local rows get no badge.
         */
        machineName={viewModel.projectsByGroupId[session.projectId ?? ""]?.machineName}
        onActivate={() => activateSession(session.sessionId)}
        onOpenMenu={(position) => setMenuState({ position, sessionId: session.sessionId })}
        onRenameCancel={() => setRenamingSessionId(undefined)}
        onRenameCommit={(title) => {
          setRenamingSessionId(undefined);
          postSidebarV2RenameSession(vscode, session.sessionId, title);
        }}
        onRenameStart={() => setRenamingSessionId(session.sessionId)}
        onTogglePinned={(pinned) => postSidebarV2SetSessionPinned(vscode, session.sessionId, pinned)}
        pinnedReorderEnabled={
          options.shelf === undefined && !isPinnedSessionReorderDisabled
        }
        project={options.project}
        session={item}
        showProjectIcons={showProjectIcons}
        slimLabel={options.slimLabel}
        status={resolveStatus(session)}
        useColoredAgentIcons={useColoredAgentIcons}
        variant={options.variant}
      />
    );
  };

  const menuSession = menuState ? sessionsById[menuState.sessionId] : undefined;
  /*
   * Both portals need the V2 session (lifecycle fields, project id), not just
   * the raw contract row, so they resolve it out of the rendered partitions
   * rather than re-adapting the store item.
   */
  const findV2Session = (sessionId: string): SidebarV2Session | undefined => {
    for (const group of viewModel.groups) {
      const match =
        group.partition.active.find((entry) => entry.sessionId === sessionId) ??
        group.partition.settled.find((entry) => entry.sessionId === sessionId) ??
        group.partition.snoozed.find((entry) => entry.sessionId === sessionId) ??
        group.browserSessions.find((entry) => entry.sessionId === sessionId);
      if (match) {
        return match;
      }
    }
    return undefined;
  };
  const menuV2Session = menuState ? findV2Session(menuState.sessionId) : undefined;
  const snoozeV2Session = snoozeMenuState ? findV2Session(snoozeMenuState.sessionId) : undefined;

  /*
   * CDXC:SidebarV2Worktree 2026-07-29:
   * "New session on <branch>" needs three facts to be offerable: the row's
   * probed branch (the label), its cwd (the checkout to spawn into), and a
   * daemon that serves the flow. Missing any of them, the item is absent —
   * never present-but-disabled, which would promise something that cannot run.
   *
   * A row whose cwd IS the project's own checkout is excluded for the same
   * reason: the main working tree is not adoptable (it never appears in `git
   * worktree list`), so gxserver refuses it and the item could only ever fail.
   * "Another session in this project" is exactly what the plain "+" does.
   */
  const menuWorktreeBranch = (() => {
    if (!menuSession || !menuState) {
      return undefined;
    }
    const branch = menuSession.gitStatus?.branch?.trim();
    const cwd = normalizeWorktreePathForComparison(menuSession.cwd);
    if (!branch || cwd === null) {
      return undefined;
    }
    const groupId = groupIdForSession(menuState.sessionId);
    if (!canCreateWorktreeInGroup(groupId)) {
      return undefined;
    }
    const projectPath = normalizeWorktreePathForComparison(
      groupId === undefined ? undefined : groupsById[groupId]?.projectContext?.path,
    );
    return projectPath !== null && projectPath === cwd ? undefined : branch;
  })();

  /*
   * CDXC:SidebarV2ContextMenuParity 2026-07-30:
   * The clicked row's group record, and V1's OWN eligibility answer derived from
   * it. Re-deriving those gates in V2 would guarantee the two menus drift, so the
   * exported resolver is called with the same three inputs V1 feeds it:
   *
   * - `isProjectSessionListMoreRow` is always false — V2 has no "+N more" row.
   * - `isRemoteSession` is the group's remote-machine context, the same signal
   *   V1's remote rows carry. It matters because Delayed Send, Close After Done
   *   and full reload are local AppKit/host-timer actions that a remote row must
   *   opt into through published capabilities rather than have assumed for it.
   * - the two copy flags are user settings; both default OFF, so an untouched
   *   install sees no copy items in either sidebar.
   */
  const menuGroup =
    menuState === undefined
      ? undefined
      : groupsById[groupIdForSession(menuState.sessionId) ?? ""];
  const menuEligibility = menuSession
    ? getSidebarSessionContextMenuEligibility({
        isProjectSessionListMoreRow: false,
        isRemoteSession: menuGroup?.remoteMachineContext !== undefined,
        session: menuSession,
        showSessionCommandCopyActions: settings.showSessionCommandCopyActions === true,
        showSessionDetailsCopyAction: settings.showSessionDetailsCopyAction === true,
      })
    : undefined;

  const scopedProjectLabel =
    scopeId === SIDEBAR_V2_ALL_SCOPE_ID
      ? undefined
      : viewModel.scopeOptions.find((option) => option.scopeId === scopeId)?.label;

  const flatIsEmpty =
    viewModel.flat.active.length === 0 &&
    viewModel.flat.settled.length === 0 &&
    viewModel.flat.snoozed.length === 0 &&
    viewModel.browserSessions.length === 0;
  const groupedIsEmpty = viewModel.groups.every((group) => group.sessionCount === 0);
  const isEmpty = layout === "flat" ? flatIsEmpty : groupedIsEmpty || viewModel.groups.length === 0;

  const emptyState = (() => {
    /*
     * The host's recovery/onboarding block wins outright. When gxserver is
     * unavailable there are no sessions to show BECAUSE the daemon is down, and
     * "No sessions yet" both hides that and hides the button that fixes it.
     */
    if (hostEmptyState != null) {
      return hostEmptyState;
    }
    if (!isEmpty) {
      return null;
    }
    if (isSearchFiltering) {
      const trimmedQuery = searchQuery.trim();
      return (
        <p className="sidebar-v2-empty-message">
          {trimmedQuery ? `No sessions match “${trimmedQuery}”` : "No sessions match your search"}
        </p>
      );
    }
    if (selectedSessionTagFilters.length > 0) {
      return <p className="sidebar-v2-empty-message">No sessions match the selected tags</p>;
    }
    if (scopedProjectLabel !== undefined && viewModel.hasAnySession) {
      return (
        <p className="sidebar-v2-empty-message">{`No sessions in ${scopedProjectLabel} yet`}</p>
      );
    }
    return (
      <>
        <p className="sidebar-v2-empty-message">No sessions yet</p>
        {/*
         * The escape hatch lives in the empty state on purpose: an empty V2
         * inbox is the one moment where a user can reasonably wonder whether
         * the new sidebar lost their sessions.
         */}
        <button
          className="sidebar-v2-empty-action"
          onClick={() => onSetSidebarVersion("v1")}
          type="button"
        >
          Switch back to the classic sidebar
        </button>
      </>
    );
  })();

  /*
   * CDXC:SidebarV2LogicalProjects 2026-07-29:
   * A grouping choice is written to every member checkout of the group the user
   * acted on, and the whole overrides record is handed back so removing an
   * override later is expressible. `separate` is stored explicitly rather than
   * as "no key": the automatic rule IS repository merging, so "keep separate"
   * has to be a recorded decision or the next snapshot would re-merge it.
   *
   * CDXC:SidebarV2LogicalProjects 2026-07-29 (P5 fix round):
   * Splitting and re-merging have to be SYMMETRIC. Splitting is one click on
   * one row, so re-merging must be too — but a merging choice made on one of
   * several already-split rows can only reach that row's own checkouts, and the
   * others would keep their `separate` override and stay out. So the merging
   * modes write to every VISIBLE row that belongs to the same repository, which
   * is the set the user is looking at and the exact set the previous split
   * produced.
   *
   * `separate` deliberately keeps the narrow scope: "keep this one apart" is a
   * statement about the group the user acted on, and widening it would let one
   * click scatter rows the user never touched.
   */
  const setGroupGroupingMode = (
    group: SidebarV2GroupModel,
    mode: SidebarProjectGroupingMode,
  ) => {
    if (!onSetProjectGroupingOverrides) {
      return;
    }
    const affectedGroups =
      mode === "separate" || !group.repositoryCanonicalKey
        ? [group]
        : viewModel.groups.filter(
            (candidate) => candidate.repositoryCanonicalKey === group.repositoryCanonicalKey,
          );
    const overrideKeys = [
      ...new Set(affectedGroups.flatMap((affected) => affected.groupingOverrideKeys)),
    ];
    if (overrideKeys.length === 0) {
      return;
    }
    const next: Record<string, SidebarProjectGroupingMode> = {
      ...settings.sidebarProjectGroupingOverrides,
    };
    for (const key of overrideKeys) {
      next[key] = mode;
    }
    onSetProjectGroupingOverrides(next);
  };

  const groupMenuGroup = groupMenuState
    ? viewModel.groups.find((group) => group.groupId === groupMenuState.groupId)
    : undefined;

  /*
   * CDXC:SidebarV2GroupedProjectUX 2026-07-30:
   * Which of a logical row's member checkouts can actually be closed, using V1's
   * exact rule (`session-group-section.tsx`): the member must be a project, and
   * either the host says its project is removable or it lives on a remote machine
   * — remote rows are closable into Recent Projects even though remote DELETE
   * stays disabled, so close eligibility is deliberately not `canRemoveProject`
   * alone.
   *
   * The list matters as much as the gate. Closing only the representative is
   * exactly the reported bug: a repository open on this Mac AND on a remote
   * machine merges into ONE row, so parking the local checkout leaves the remote
   * member behind and the row the user just closed is still on screen, now backed
   * only by the machine they were not thinking about.
   */
  const closableMemberGroupIds = (group: SidebarV2GroupModel): string[] =>
    group.memberGroupIds.filter((memberGroupId) => {
      const record = groupsById[memberGroupId];
      return (
        record?.projectContext !== undefined &&
        (record.projectContext.canRemoveProject === true ||
          record.remoteMachineContext !== undefined)
      );
    });

  const groupMenuSections = groupMenuGroup
    ? createSidebarV2ProjectGroupMenuSections(
      {
        /*
         * The grouping submenu needs BOTH a mergeable repository and a caller
         * that can persist the choice; without the writer it would be a radio
         * group that forgets. Close Project stands on its own gate below, so a
         * non-git project still gets a menu.
         */
        canGroupAcrossMachines:
          groupMenuGroup.canGroupAcrossMachines && onSetProjectGroupingOverrides !== undefined,
        ...(groupMenuGroup.groupingMode ? { groupingMode: groupMenuGroup.groupingMode } : {}),
      },
      {
        onCloseProject:
          closableMemberGroupIds(groupMenuGroup).length > 0
            ? () =>
                postSidebarV2CloseWorkspaceProjects(
                  vscode,
                  closableMemberGroupIds(groupMenuGroup),
                )
            : undefined,
        onSetGroupingMode: (mode) => setGroupGroupingMode(groupMenuGroup, mode),
      },
    )
    : [];

  const isProjectShelfExpanded = (groupId: string, tone: "settled" | "snoozed") =>
    projectShelfOverrides[`${groupId}:${tone}`] ?? (tone === "settled" ? true : false);
  const toggleProjectShelf = (groupId: string, tone: "settled" | "snoozed") => {
    const key = `${groupId}:${tone}`;
    setProjectShelfOverrides((previous) => ({
      ...previous,
      [key]: !(previous[key] ?? (tone === "settled" ? true : false)),
    }));
  };

  const renderProjectShelf = (group: SidebarV2GroupModel) => (
    <>
      {/*
       * CDXC:SidebarV2 2026-07-29:
       * The spec only asks grouped mode for a per-project Settled shelf, but a
       * snoozed session still has to live somewhere in grouped mode or the
       * layout toggle would silently hide sessions. It gets the same shelf
       * treatment rather than being dropped.
       */}
      <SidebarV2Shelf
        count={group.partition.snoozed.length}
        isExpanded={isProjectShelfExpanded(group.groupId, "snoozed")}
        label="Snoozed"
        onToggle={() => toggleProjectShelf(group.groupId, "snoozed")}
        tone="snoozed"
      >
        {group.partition.snoozed.map((session) =>
          renderRow(session, { shelf: "snoozed", slimLabel: "Snoozed", variant: "slim" }),
        )}
      </SidebarV2Shelf>
      <SidebarV2Shelf
        count={group.partition.settled.length}
        isExpanded={isProjectShelfExpanded(group.groupId, "settled")}
        label="Settled"
        onToggle={() => toggleProjectShelf(group.groupId, "settled")}
        tone="settled"
      >
        {group.partition.settled.map((session) =>
          renderRow(session, { shelf: "settled", variant: "slim" }),
        )}
      </SidebarV2Shelf>
    </>
  );

  return (
    <div
      className="sidebar-v2-root"
      data-sidebar-v2-layout={layout}
      data-sidebar-version="v2"
    >
      <div className="sidebar-v2-toolbar">
        {/*
         * The scope dropdown belongs to flat mode only: grouped mode already
         * states every project as a header, so scoping it would be two
         * competing answers to the same question.
         */}
        {layout === "flat" ? (
          <SidebarV2ScopeMenu
            onSelectScope={setScopeId}
            options={viewModel.scopeOptions}
            scopeId={scopeId}
            showProjectIcons={showProjectIcons}
            vscode={vscode}
          />
        ) : (
          <span className="sidebar-v2-toolbar-label">Grouped by project</span>
        )}
        <button
          aria-label={layout === "byProject" ? "Show a flat inbox" : "Group by project"}
          aria-pressed={layout === "byProject"}
          className="sidebar-v2-layout-toggle"
          onClick={() => onSetLayout(layout === "byProject" ? "flat" : "byProject")}
          type="button"
        >
          {layout === "byProject" ? (
            <IconLayoutList aria-hidden="true" size={15} stroke={1.8} />
          ) : (
            <IconFolders aria-hidden="true" size={15} stroke={1.8} />
          )}
        </button>
        {/*
         * CDXC:SidebarV2Worktree 2026-07-29:
         * V2's own creation control. It renders only when the caller supplied a
         * launch path and an agent to launch, so a host that cannot create
         * sessions never shows a "+" that would do nothing.
         *
         * CDXC:SidebarV2SingleCreateControl 2026-07-30:
         * This is now the ONLY create control in V2's header — the shared V1
         * header no longer receives its create callbacks while V2 is active —
         * so the chevron carries the agent picker and the two Quick entries as
         * well as the worktree flow. The plain half and the picker both target
         * `headerCreateGroupId`, a real project, so neither can land a session
         * in Quick by accident.
         */}
        {onRunAgent && primaryAgent ? (
          <SidebarV2CreateButton
            agents={configuredAgents}
            canCreateWorktree={headerWorktreeGroupId !== undefined}
            defaultEnvMode={settings.newSessionsDefaultEnvMode}
            label={`New ${primaryAgent.name} session`}
            onAddProject={onAddProject}
            onCreateAgentSession={(agent) => runInstantSession(headerCreateGroupId, agent)}
            onCreateInstantSession={() => runInstantSession(headerCreateGroupId)}
            onCreateQuickBrowserTab={onCreateQuickBrowserTab}
            onCreateQuickTerminal={onCreateQuickTerminal}
            onOpenWorktreePopover={(position) =>
              openWorktreePopover(position, headerWorktreeGroupId)
            }
            onSetDefaultEnvMode={onSetNewSessionsDefaultEnvMode}
            primaryAgentId={primaryAgent.agentId}
            vscode={vscode}
          />
        ) : null}
      </div>

      {layout === "flat" ? (
        <ul className="sidebar-v2-list" role="list">
          {/*
           * CDXC:SidebarV2 2026-07-29:
           * Browser tabs get their own flat-mode section instead of sitting in
           * the inbox: they have no agent lifecycle to settle or snooze, and
           * mixing them in makes the inbox read as a tab bar. Clicking a row
           * posts the same `focusSession` V1's browser rows post, so the host
           * resolves the same machine-scoped project browser tab.
           *
           * CDXC:SidebarV2BrowserShelfFirst 2026-07-30:
           * The shelf sits at the TOP of the flat list, above the active cards.
           * Grouped mode already renders each project's browser rows first, so
           * this is the flat list agreeing with the grouped one, and it puts the
           * collapsible tab strip where a tab strip belongs instead of stranding
           * it under an arbitrarily long inbox.
           */}
          <SidebarV2Shelf
            count={viewModel.browserSessions.length}
            isExpanded={isBrowserExpanded}
            label="Browser"
            onToggle={() => setIsBrowserExpanded((previous) => !previous)}
            tone="browser"
          >
            {viewModel.browserSessions.map((session) =>
              renderRow(session, {
                project: viewModel.projectsByGroupId[session.projectId ?? ""],
                variant: "card",
              }),
            )}
          </SidebarV2Shelf>
          {viewModel.flat.active.map((session, sessionIndex) => (
            <Fragment key={session.sessionId}>
              {renderRow(session, {
                project: viewModel.projectsByGroupId[session.projectId ?? ""],
                variant: "card",
              })}
              {session.isPinned === true &&
              viewModel.flat.active[sessionIndex + 1] !== undefined &&
              viewModel.flat.active[sessionIndex + 1]?.isPinned !== true ? (
                <li aria-hidden className="pinned-sessions-divider" />
              ) : null}
            </Fragment>
          ))}
          <SidebarV2Shelf
            count={viewModel.flat.snoozed.length}
            isExpanded={isSnoozedExpanded}
            label="Snoozed"
            onToggle={() => setIsSnoozedExpanded((previous) => !previous)}
            tone="snoozed"
          >
            {viewModel.flat.snoozed.map((session) =>
              renderRow(session, { shelf: "snoozed", slimLabel: "Snoozed", variant: "slim" }),
            )}
          </SidebarV2Shelf>
          <SidebarV2Shelf
            count={viewModel.flat.settled.length}
            isExpanded={isSettledExpanded}
            label="Settled"
            onToggle={() => setIsSettledExpanded((previous) => !previous)}
            tone="settled"
          >
            {viewModel.flat.settled.map((session) =>
              renderRow(session, { shelf: "settled", variant: "slim" }),
            )}
          </SidebarV2Shelf>
        </ul>
      ) : (
        /*
         * CDXC:SidebarV2GroupedProjectUX 2026-07-30:
         * V1's project-list container classnames, so the reference-layout rules
         * that shape a project list (`row-gap: 0`, the shared scroll-row bleed
         * the headers compensate for) apply to V2's grouped list verbatim
         * instead of being re-derived under a V2-only classname.
         */
        <div className="sidebar-v2-groups group-list workspace-group-list reference-project-group-list">
          {viewModel.groups.map((group, groupIndex) => {
            const isCollapsed = collapsedGroupsById[group.groupId] === true;
            return (
              <SidebarV2ProjectGroupSection
                dropPosition={
                  groupDropIndicator?.groupId === group.groupId
                    ? groupDropIndicator.position
                    : undefined
                }
                group={group}
                headerActions={
                  onRunAgent && primaryAgent ? (
                    <SidebarV2CreateButton
                      canCreateWorktree={canCreateWorktreeInGroup(group.groupId)}
                      defaultEnvMode={settings.newSessionsDefaultEnvMode}
                      label={`New ${primaryAgent.name} session in ${group.title}`}
                      onCreateInstantSession={() => runInstantSession(group.groupId)}
                      onOpenWorktreePopover={(position) =>
                        openWorktreePopover(position, group.groupId)
                      }
                      onSetDefaultEnvMode={onSetNewSessionsDefaultEnvMode}
                      vscode={vscode}
                    />
                  ) : null
                }
                index={groupIndex}
                isActive={group.memberGroupIds.some(
                  (memberGroupId) => groupsById[memberGroupId]?.isActive === true,
                )}
                isCollapsed={isCollapsed}
                containsActiveSession={
                  activeSessionId !== undefined &&
                  [
                    ...group.browserSessions,
                    ...group.partition.active,
                    ...group.partition.settled,
                    ...group.partition.snoozed,
                  ].some((session) => session.sessionId === activeSessionId)
                }
                /*
                 * The Quick collection has no persisted project order to write,
                 * and a sidebar that is not manually sorted must not offer
                 * reorder at all (V1's `draggingDisabled`).
                 */
                isDragDisabled={group.isQuick || isGroupReorderDisabled}
                isDragPreviewSource={draggingGroupId === group.groupId}
                key={group.groupId}
                /*
                 * CDXC:SidebarV2LogicalProjects 2026-07-29:
                 * The group menu lives on the header's own context menu — the
                 * header IS the merged project, so right-clicking it is the one
                 * gesture that unambiguously means "this project".
                 *
                 * CDXC:SidebarV2GroupedProjectUX 2026-07-30:
                 * It is no longer gated on cross-machine merging being possible:
                 * Close Project applies to every open project, git origin or not.
                 * The builder decides which items exist, and the render below
                 * suppresses the menu when it decided none do.
                 */
                onContextMenu={(event) => {
                  event.preventDefault();
                  setGroupMenuState({
                    groupId: group.groupId,
                    position: { clientX: event.clientX, clientY: event.clientY },
                  });
                }}
                onResolveMissingProjectFolder={() => {
                  vscode.postMessage({
                    groupId: group.groupId,
                    type: "createProjectTerminal",
                  });
                }}
                onSetCollapsed={(collapsed) => onSetGroupCollapsed(group.groupId, collapsed)}
                projectPath={
                  groupsById[group.groupId]?.remoteMachineContext === undefined
                    ? groupsById[group.groupId]?.projectContext?.path
                    : undefined
                }
                showProjectIcons={showProjectIcons}
                projectPathState={
                  groupsById[group.groupId]?.remoteMachineContext === undefined
                    ? groupsById[group.groupId]?.projectContext?.pathState
                    : undefined
                }
              >
                <ul className="sidebar-v2-list" role="list">
                    {/*
                     * CDXC:ProjectBrowserTabs 2026-05-16-12:59 (V1 parity):
                     * Browser rows stay above the agent/terminal rows inside a
                     * project group, exactly as the classic sidebar renders
                     * them today.
                     */}
                    {group.browserSessions.map((session) => renderRow(session, { variant: "card" }))}
                    {group.partition.active.map((session, sessionIndex) => (
                      <Fragment key={session.sessionId}>
                        {renderRow(session, { variant: "card" })}
                        {session.isPinned === true &&
                        group.partition.active[sessionIndex + 1] !== undefined &&
                        group.partition.active[sessionIndex + 1]?.isPinned !== true ? (
                          <li aria-hidden className="pinned-sessions-divider" />
                        ) : null}
                      </Fragment>
                    ))}
                    {renderProjectShelf(group)}
                </ul>
              </SidebarV2ProjectGroupSection>
            );
          })}
        </div>
      )}

      {emptyState ? <div className="sidebar-v2-empty">{emptyState}</div> : null}

      {menuState && menuSession ? (
        <SidebarV2ContextMenu
          onDismiss={closeMenu}
          position={menuState.position}
          sections={createSidebarV2ContextMenuSections(
            menuSession,
            {
              /*
               * CDXC:SidebarV2Worktree 2026-07-29:
               * Close goes through the cleanup check, which either posts the
               * SAME close command straight away or asks about the checkout
               * first. Sessions outside a managed worktree take the unchanged
               * path.
               */
              onClose: () => closeSession(menuSession.sessionId),
              onCloseAfterDone: () =>
                postSidebarV2ToggleCloseAfterDone(vscode, menuSession.sessionId),
              onCopyAttachCommand: () =>
                postSidebarV2CopyAttachCommand(vscode, menuSession.sessionId),
              /*
               * The clipboard text is built from the RENDERED row plus its group,
               * exactly as V1 builds it, so the same session copies the same
               * block of details from either sidebar.
               */
              onCopyDetails: () =>
                postSidebarV2CopySessionDetails(
                  vscode,
                  menuSession.sessionId,
                  buildSidebarSessionDetailsClipboardText(menuSession, menuGroup),
                ),
              onCopyResumeCommand: () =>
                postSidebarV2CopyResumeCommand(vscode, menuSession.sessionId),
              /*
               * CDXC:SidebarV2ContextMenuParity 2026-07-30:
               * Delayed Send and the 1st-message viewer are the two items whose
               * effect is a full-window modal rather than a host command. They
               * call the module-level app-modal bridge directly with V1's exact
               * payloads — that bridge is not a sidebar-scoped channel, so V2
               * needs no new prop and no second modal implementation.
               */
              onDelayedSend: () =>
                openAppModal({
                  agentIcon: menuSession.agentIcon,
                  closeAfterDoneActive: menuSession.closeAfterDone === true,
                  delayedSendDeadlineAt: menuSession.delayedSendDeadlineAt,
                  delayedSendRemainingLabel: menuSession.delayedSendRemainingLabel,
                  modal: "delayedSend",
                  sendWhenAllProjectSessionsStopActive:
                    menuSession.sendWhenAllProjectSessionsStopActive === true,
                  sendWhenAgentStopsActive: menuSession.sendWhenAgentStopsActive === true,
                  sessionId: menuSession.sessionId,
                  supportsSendWhenAgentStops: true,
                  supportsSendWhenAllProjectSessionsStop: true,
                  title: sidebarV2SessionModalTitle(menuSession),
                  type: "open",
                }),
              onFocusMode: () => postSidebarV2FocusSessionMode(vscode, menuSession.sessionId),
              onFork: () => postSidebarV2ForkSession(vscode, menuSession.sessionId),
              onFullReload: () => postSidebarV2FullReloadSession(vscode, menuSession.sessionId),
              onGenerateTitle: () => {
                const firstMessage = menuSession.firstUserMessage?.trim();
                if (!firstMessage) {
                  return;
                }
                postSidebarV2GenerateSessionTitle(vscode, menuSession.sessionId, firstMessage);
              },
              onNewSessionOnBranch: menuWorktreeBranch
                ? () => {
                    const groupId = groupIdForSession(menuSession.sessionId);
                    const worktreePath = menuSession.cwd?.trim();
                    if (groupId && worktreePath) {
                      startSessionOnExistingWorktree(groupId, worktreePath);
                    }
                  }
                : undefined,
              onRename: () => setRenamingSessionId(menuSession.sessionId),
              onSetPinned: (pinned) =>
                postSidebarV2SetSessionPinned(vscode, menuSession.sessionId, pinned),
              onSetSessionTag: (tag) =>
                postSidebarV2SetSessionTag(vscode, menuSession.sessionId, tag),
              onSetSleeping: (sleeping) =>
                postSidebarV2SetSessionSleeping(vscode, menuSession.sessionId, sleeping),
              onSettle: menuV2Session ? () => settleSession(menuV2Session) : undefined,
              onSnooze: menuV2Session
                ? (preset) => snoozeSession(menuV2Session, preset.snoozedUntil)
                : undefined,
              onUnsettle: menuV2Session ? () => unsettleSession(menuV2Session) : undefined,
              onViewFirstMessage: () => {
                const message = menuSession.firstUserMessage?.trim();
                if (!message) {
                  return;
                }
                openAppModal({
                  message,
                  modal: "firstUserMessage",
                  title: sidebarV2SessionModalTitle(menuSession),
                  type: "open",
                });
              },
              onWake: menuV2Session ? () => wakeSession(menuV2Session) : undefined,
            },
            {
              canFocusMode: menuGroup?.canFocusMode === true,
              eligibility: menuEligibility,
              lifecycle: menuV2Session
                ? {
                    /*
                     * "Settled" / "snoozed" here mean what the PARTITION
                     * decided, not what a field says: an auto-settled session
                     * has no `settledAt`, and a snoozed session that raised its
                     * hand is back in the inbox. Reading the rendered shelf
                     * membership keeps the menu and the list in agreement.
                     */
                    isSettled: viewModel.groups.some((group) =>
                      group.partition.settled.some(
                        (entry) => entry.sessionId === menuV2Session.sessionId,
                      ),
                    ),
                    isSnoozed: viewModel.groups.some((group) =>
                      group.partition.snoozed.some(
                        (entry) => entry.sessionId === menuV2Session.sessionId,
                      ),
                    ),
                    supportsSettle: sessionCapabilities(menuV2Session).settle,
                    supportsSnooze: sessionCapabilities(menuV2Session).snooze,
                  }
                : undefined,
              nowMs,
              sessionTagListItems: settings.sidebarSessionTagListItems,
              worktreeBranch: menuWorktreeBranch,
            },
          )}
          vscode={vscode}
        />
      ) : null}

      {/*
       * CDXC:SidebarV2GroupedProjectUX 2026-07-30:
       * The builder decides which group items exist, so the guard is on the
       * RESULT: a right-click that would open an empty menu opens nothing at all.
       */}
      {groupMenuState && groupMenuGroup && groupMenuSections.length > 0 ? (
        <SidebarV2ContextMenu
          onDismiss={() => setGroupMenuState(undefined)}
          position={groupMenuState.position}
          sections={groupMenuSections}
          /*
           * CDXC:SidebarV2ContextMenuLook 2026-07-30:
           * A project row's menu is wider than a session row's in the classic
           * sidebar (196px vs 178px), because project commands carry longer
           * labels. Naming the variant is what keeps that difference V1's rather
           * than this mount's.
           */
          variant="projectGroup"
          vscode={vscode}
        />
      ) : null}

      {snoozeMenuState && snoozeV2Session ? (
        <SidebarV2SnoozePopover
          nowMs={nowMs}
          onDismiss={() => setSnoozeMenuState(undefined)}
          onSelectPreset={(preset) => snoozeSession(snoozeV2Session, preset.snoozedUntil)}
          position={snoozeMenuState.position}
          vscode={vscode}
        />
      ) : null}

      {worktreePopoverState ? (
        <SidebarV2WorktreePopover
          agents={agents ?? []}
          defaultAgentId={primaryAgent?.agentId}
          errorMessage={worktreeErrorMessage}
          isPending={pendingWorktreeRequestId !== undefined}
          messageSource={messageSource}
          onDismiss={() => {
            /*
             * Dismissing abandons the answer too: a popover the user closed
             * must not be re-opened by a late host reply.
             */
            pendingWorktreeRequestIdRef.current = undefined;
            setPendingWorktreeRequestId(undefined);
            setWorktreePopoverState(undefined);
          }}
          onRequestWorktrees={(requestId) =>
            postSidebarV2RequestProjectWorktrees(vscode, {
              /*
               * The branch/worktree probe is addressed by gxserver PROJECT id
               * (that is what the host's existing resolver takes), while the
               * create command is addressed by the sidebar group id. Both come
               * from the same group record, so neither side has to guess.
               */
              projectId:
                groupsById[worktreePopoverState.groupId]?.projectContext?.editor.projectId,
              requestId,
            })
          }
          onSubmit={(draft) => submitWorktreeSession(worktreePopoverState.groupId, draft)}
          position={worktreePopoverState.position}
          projectLabel={viewModel.projectsByGroupId[worktreePopoverState.groupId]?.title}
          vscode={vscode}
        />
      ) : null}

      {cleanupState ? (
        <SidebarV2WorktreeCleanupPrompt
          errorMessage={cleanupState.errorMessage}
          isDirty={cleanupState.isDirty}
          isPending={cleanupState.requestId !== undefined}
          onCancel={() => {
            /*
             * Cancel abandons the CLOSE as well when it has not been sent yet:
             * the prompt is part of the close gesture, not a separate step
             * after it.
             */
            cleanupRequestIdRef.current = undefined;
            setCleanupState(undefined);
          }}
          onKeepWorktree={() => finishCleanup(false)}
          onRemoveWorktree={() => finishCleanup(true)}
          warnings={cleanupState.warnings}
          worktreePath={cleanupState.worktreePath}
        />
      ) : null}
    </div>
  );
}
