import {
  IconAlarmOff,
  IconArrowBackUp,
  IconCheck,
  IconChevronRight,
  IconClock,
  IconCopy,
  IconDeviceMobile,
  IconGitBranch,
  IconGitFork,
  IconMaximize,
  IconMessageCircle,
  IconMoon,
  IconPencil,
  IconPinned,
  IconPinnedOff,
  IconPlayerPlay,
  IconRefresh,
  IconServer,
  IconSparkles,
  IconTag,
  IconX,
} from "@tabler/icons-react";
import {
  Fragment,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import type { SidebarProjectGroupingMode } from "../../shared/ghostex-settings";
import type {
  SidebarSessionItem,
  SidebarSessionTag,
} from "../../shared/session-grid-contract";
import {
  getEnabledVisibleSidebarSessionTagSections,
  type SidebarSessionTagListItem,
} from "../../shared/session-tags";
import {
  resolveSidebarV2SnoozePresets,
  type SidebarV2SnoozePreset,
} from "../../shared/sidebar-v2-snooze";
import {
  SIDEBAR_CONTEXT_MENU_VIEWPORT_MARGIN_PX,
  SidebarContextMenuPortal,
  getClampedSidebarContextMenuCoordinate,
} from "../sidebar-context-menu-portal";
import { SessionTagIcon, getEffectiveSessionTag } from "../session-tag-ui";
import type { SidebarSessionContextMenuEligibility } from "../sortable-session-card";
import type { WebviewApi } from "../webview-api";

/*
 * CDXC:SidebarV2 2026-07-29:
 * V2's session menu is deliberately a small, self-contained menu rather than a
 * reuse of V1's `sortable-session-card` menu builder. That builder is fused to
 * the V1 card: it reads dnd-kit sortable state, multi-select bulk availability,
 * project-session-list overflow rows, and the app-modal host, none of which
 * exist in the V2 tree. Extracting it would mean refactoring the hottest file
 * in the sidebar while other agents work in it.
 *
 * What matters for correctness is that every item here posts the SAME message
 * V1's equivalent item posts, so the host cannot tell which sidebar issued it.
 *
 * CDXC:SidebarV2ContextMenuParity 2026-07-30:
 * The per-item ELIGIBILITY is no longer re-derived here either: the caller hands
 * in V1's own `getSidebarSessionContextMenuEligibility` result. That resolver is
 * exported, pure, and dnd-free, so importing its answer is the only way the two
 * menus can be guaranteed to agree about which agents can fork, which sessions
 * can be resumed from a copied command, and what a remote row is allowed to do.
 * Only the TYPE is imported here, so this module still pulls in no V1 runtime.
 *
 * Deliberately NOT brought over, with reasons:
 * - Move to New Group / Sleep below / Close below: all three name a V1 structure
 *   (session groups, the project's rendered order below the clicked row) that
 *   the V2 inbox does not render, so their target would have to be invented.
 * - Pop Out Pane: `popOutPane` is unhandled in gpui's sidebar runtime, so the
 *   item could only ever be a silent no-op in the app V2 ships in.
 * - Every bulk "… selected" action: V2 has no multi-select to select with.
 */

export type SidebarV2ContextMenuPosition = {
  clientX: number;
  clientY: number;
};

export type SidebarV2ContextMenuAction = {
  danger?: boolean;
  icon: ReactNode;
  key: string;
  label: string;
  onClick: () => void;
  /**
   * CDXC:SidebarV2Lifecycle 2026-07-29:
   * Snooze needs a second choice (which preset), so it carries a submenu.
   *
   * CDXC:SidebarV2ContextMenuLook 2026-07-30:
   * The submenu opens as its own panel BELOW the parent row — V1's exact
   * pattern, and the reason V1 can have flyouts in a ~260px webview at all.
   * Downwards, not sideways, is what keeps the choices on screen; a click, not
   * hover intent, is what keeps them reachable and testable.
   */
  submenu?: readonly SidebarV2ContextMenuSubmenuItem[];
};

export type SidebarV2ContextMenuSubmenuItem = {
  /** Leading glyph. Tag options carry one because a tag's color IS part of its
      identity in every other sidebar surface; the grouping and snooze submenus
      are text choices and leave it unset. */
  icon?: ReactNode;
  /**
   * CDXC:SidebarV2LogicalProjects 2026-07-29:
   * Marks the option that is already in force. Only CHOICE submenus set it
   * (grouping mode); the snooze presets are commands, not a current state, so
   * they leave it unset and no checkmark column appears for them.
   */
  isChecked?: boolean;
  key: string;
  label: string;
  onClick: () => void;
  /**
   * CDXC:SidebarV2ContextMenuLook 2026-07-30:
   * Which group of the submenu this option belongs to. Consecutive options that
   * name the same group are drawn as one block, and the boundary between two
   * groups gets V1's own divider — that is the ONLY structure V1's tag submenu
   * has (it prints no heading text), and the tag list is unreadable as a flat
   * run of eight markers without it. Submenus whose options are one flat choice
   * (snooze presets, project grouping) leave it unset.
   */
  sectionKey?: string;
  /** Right-aligned absolute time column, e.g. "9:00 AM". */
  trailingLabel?: string;
};

export type SidebarV2ContextMenuLifecycleState = {
  /** The row currently classifies as settled, so the item reads "Un-settle". */
  isSettled: boolean;
  /** The row currently classifies as snoozed, so Wake is offered. */
  isSnoozed: boolean;
  /** gxserver for this row's machine supports settle. */
  supportsSettle: boolean;
  /** gxserver for this row's machine supports snooze. */
  supportsSnooze: boolean;
};

export type SidebarV2ContextMenuHandlers = {
  onClose: () => void;
  /** Arm/disarm the host's Done timer. A toggle with no local state: the host
      owns the armed flag, so the item never claims to know which way it went. */
  onCloseAfterDone?: () => void;
  onCopyAttachCommand?: () => void;
  onCopyDetails?: () => void;
  onCopyResumeCommand?: () => void;
  onDelayedSend?: () => void;
  onFocusMode: () => void;
  onFork?: () => void;
  onFullReload?: () => void;
  onGenerateTitle?: () => void;
  /**
   * CDXC:SidebarV2Worktree 2026-07-29:
   * Start another session in the checkout this row already
   * lives in. It is an OPEN-EXISTING create (the worktree is right there), so
   * it never cuts a new branch.
   */
  onNewSessionOnBranch?: () => void;
  onRename: () => void;
  onSetPinned: (pinned: boolean) => void;
  /** `undefined` clears the tag. Re-picking the tag a session already carries is
      what sends it, matching V1's one-click un-tag. */
  onSetSessionTag?: (tag: SidebarSessionTag | undefined) => void;
  onSetSleeping: (sleeping: boolean) => void;
  onSettle?: () => void;
  onSnooze?: (preset: SidebarV2SnoozePreset) => void;
  onUnsettle?: () => void;
  onViewFirstMessage?: () => void;
  onWake?: () => void;
};

export type SidebarV2ContextMenuOptions = {
  /**
   * CDXC:SidebarV2ContextMenuParity 2026-07-30:
   * The group's zoomable-split capability. V1 hides Focus unless the clicked
   * session's group actually has split panes to zoom, because a single pane with
   * tabs uses ordinary tab selection and the item would do nothing visible. V2
   * showed it unconditionally until this batch.
   */
  canFocusMode?: boolean;
  /**
   * V1's own eligibility answer for this row. Omitted only by callers that have
   * no settings/remote context to resolve it with, in which case the parity
   * items are absent rather than guessed at.
   */
  eligibility?: SidebarSessionContextMenuEligibility;
  lifecycle?: SidebarV2ContextMenuLifecycleState;
  /** Clock the snooze presets resolve against; the menu is built on open. */
  nowMs?: number;
  /** The user's configured tag list. Disabled/hidden tags are filtered out by
      the shared resolver, so an empty result removes "Tag as" entirely. */
  sessionTagListItems?: readonly SidebarSessionTagListItem[];
  /**
   * CDXC:SidebarV2Worktree 2026-07-29:
   * The branch this row's cwd is on. Supplied ONLY when the row is a git
   * checkout on a machine whose gxserver serves the worktree flow, so the
   * capability gate lives with the caller that already resolves it per group.
   */
  worktreeBranch?: string;
};

const CONTEXT_MENU_ICON_CLASS = "session-context-menu-icon";

export function createSidebarV2ContextMenuSections(
  session: SidebarSessionItem,
  handlers: SidebarV2ContextMenuHandlers,
  options: SidebarV2ContextMenuOptions = {},
): SidebarV2ContextMenuAction[][] {
  const isBrowser = session.kind === "browser" || session.sessionKind === "browser";
  const isSleeping = session.isSleeping === true;
  const isPinned = session.isPinned === true;
  /*
   * CDXC:SidebarV2Lifecycle 2026-07-29:
   * Guards are the same ones the hover slot applies, and the same ones gxserver
   * enforces: a browser tab has no agent lifecycle, a session blocked on the
   * user can be neither settled nor snoozed (hiding a pending approval defeats
   * the request), and a working session cannot be settled but CAN be snoozed —
   * snooze changes visibility only, never the agent.
   */
  const lifecycle = options.lifecycle;
  const eligibility = options.eligibility;
  const isBlockedOnUser = session.activity === "attention";
  const isWorking = session.activity === "working";
  const canSettle =
    !isBrowser && lifecycle?.supportsSettle === true && !isBlockedOnUser && !isWorking;
  const canSnooze = !isBrowser && lifecycle?.supportsSnooze === true && !isBlockedOnUser;

  const lifecycleActions: SidebarV2ContextMenuAction[] = [];
  if (canSettle && handlers.onSettle && !lifecycle?.isSettled) {
    lifecycleActions.push({
      icon: <IconCheck aria-hidden="true" className={CONTEXT_MENU_ICON_CLASS} size={16} stroke={2} />,
      key: "settle",
      label: "Settle",
      onClick: handlers.onSettle,
    });
  }
  if (!isBrowser && lifecycle?.supportsSettle === true && lifecycle.isSettled && handlers.onUnsettle) {
    lifecycleActions.push({
      icon: (
        <IconArrowBackUp aria-hidden="true" className={CONTEXT_MENU_ICON_CLASS} size={16} stroke={1.8} />
      ),
      key: "unsettle",
      label: "Un-settle",
      onClick: handlers.onUnsettle,
    });
  }
  if (!isBrowser && lifecycle?.supportsSnooze === true && lifecycle.isSnoozed && handlers.onWake) {
    lifecycleActions.push({
      icon: <IconAlarmOff aria-hidden="true" className={CONTEXT_MENU_ICON_CLASS} size={16} stroke={1.8} />,
      key: "wake",
      label: "Wake now",
      onClick: handlers.onWake,
    });
  }
  if (canSnooze && handlers.onSnooze && !lifecycle?.isSnoozed) {
    const onSnooze = handlers.onSnooze;
    lifecycleActions.push({
      icon: <IconClock aria-hidden="true" className={CONTEXT_MENU_ICON_CLASS} size={16} stroke={1.8} />,
      key: "snooze",
      label: "Snooze",
      // Opening the parent only reveals the presets; the snooze itself is
      // always an explicit preset choice, never a default guess.
      onClick: () => undefined,
      submenu: resolveSidebarV2SnoozePresets(options.nowMs ?? Date.now()).map((preset) => ({
        key: preset.id,
        label: preset.label,
        onClick: () => onSnooze(preset),
        trailingLabel: preset.whenLabel,
      })),
    });
  }
  /*
   * CDXC:SidebarV2ContextMenuParity 2026-07-30:
   * Close After Done sits with settle/snooze rather than in V1's copy/act
   * section: in the inbox model all three answer the same question — when should
   * this row stop asking for attention — and Close After Done is simply the
   * answer that ends with the session gone. The host owns the armed flag and the
   * three-minute Done timer, so the label never states which way the toggle went.
   */
  if (eligibility?.canCloseAfterDone === true && handlers.onCloseAfterDone) {
    lifecycleActions.push({
      icon: <IconClock aria-hidden="true" className={CONTEXT_MENU_ICON_CLASS} size={16} stroke={1.8} />,
      key: "closeAfterDone",
      label: "Close After Done",
      onClick: handlers.onCloseAfterDone,
    });
  }

  const primary: SidebarV2ContextMenuAction[] = [];
  if (!isBrowser) {
    primary.push({
      icon: <IconPencil aria-hidden="true" className="session-context-menu-icon" size={16} stroke={1.8} />,
      key: "rename",
      label: "Rename",
      onClick: handlers.onRename,
    });
  }
  /*
   * CDXC:SidebarV2ContextMenuParity 2026-07-30:
   * Focus is offered only when the clicked row's group actually has split panes
   * to zoom — V1's rule. A group with one pane and several tabs uses ordinary tab
   * selection, so zooming it changes nothing the user can see.
   */
  if (options.canFocusMode === true) {
    primary.push({
      icon: <IconMaximize aria-hidden="true" className="session-context-menu-icon" size={16} stroke={1.8} />,
      key: "focus",
      label: "Focus",
      onClick: handlers.onFocusMode,
    });
  }
  if (!isBrowser && options.worktreeBranch && handlers.onNewSessionOnBranch) {
    primary.push({
      icon: (
        <IconGitBranch aria-hidden="true" className={CONTEXT_MENU_ICON_CLASS} size={16} stroke={1.8} />
      ),
      key: "newSessionOnBranch",
      label: `New session on ${options.worktreeBranch}`,
      onClick: handlers.onNewSessionOnBranch,
    });
  }

  /*
   * CDXC:SidebarV2ContextMenuParity 2026-07-30:
   * The per-session act/inspect/copy section, in V1's order. Every gate is
   * V1's own eligibility answer, and every handler is optional, so an item is
   * absent whenever either the row cannot support it or the caller cannot serve
   * it — never present-but-inert.
   */
  const sessionActions: SidebarV2ContextMenuAction[] = [];
  if (session.firstUserMessage?.trim() && handlers.onViewFirstMessage) {
    sessionActions.push({
      icon: (
        <IconMessageCircle aria-hidden="true" className={CONTEXT_MENU_ICON_CLASS} size={16} stroke={1.8} />
      ),
      key: "viewFirstMessage",
      label: "View 1st message",
      onClick: handlers.onViewFirstMessage,
    });
  }
  if (eligibility?.canCopyResumeCommand === true && handlers.onCopyResumeCommand) {
    sessionActions.push({
      icon: <IconCopy aria-hidden="true" className={CONTEXT_MENU_ICON_CLASS} size={16} stroke={1.8} />,
      key: "copyResume",
      label: "Copy resume",
      onClick: handlers.onCopyResumeCommand,
    });
  }
  if (eligibility?.canCopyAttachCommand === true && handlers.onCopyAttachCommand) {
    sessionActions.push({
      icon: <IconCopy aria-hidden="true" className={CONTEXT_MENU_ICON_CLASS} size={16} stroke={1.8} />,
      key: "copyAttach",
      label: "Copy attach command",
      onClick: handlers.onCopyAttachCommand,
    });
  }
  if (eligibility?.canCopySessionDetails === true && handlers.onCopyDetails) {
    sessionActions.push({
      icon: <IconCopy aria-hidden="true" className={CONTEXT_MENU_ICON_CLASS} size={16} stroke={1.8} />,
      key: "copyDetails",
      label: "Copy details",
      onClick: handlers.onCopyDetails,
    });
  }
  if (eligibility?.canDelayedSend === true && handlers.onDelayedSend) {
    sessionActions.push({
      icon: <IconClock aria-hidden="true" className={CONTEXT_MENU_ICON_CLASS} size={16} stroke={1.8} />,
      key: "delayedSend",
      label: "Delayed Send",
      onClick: handlers.onDelayedSend,
    });
  }
  if (eligibility?.canForkSession === true && handlers.onFork) {
    sessionActions.push({
      icon: <IconGitFork aria-hidden="true" className={CONTEXT_MENU_ICON_CLASS} size={16} stroke={1.8} />,
      key: "fork",
      label: "Fork",
      onClick: handlers.onFork,
    });
  }
  if (eligibility?.canGenerateSessionTitle === true && handlers.onGenerateTitle) {
    sessionActions.push({
      icon: <IconSparkles aria-hidden="true" className={CONTEXT_MENU_ICON_CLASS} size={16} stroke={1.8} />,
      key: "generateTitle",
      label: "Generate Title",
      onClick: handlers.onGenerateTitle,
    });
  }
  if (eligibility?.canFullReloadSession === true && handlers.onFullReload) {
    sessionActions.push({
      icon: <IconRefresh aria-hidden="true" className={CONTEXT_MENU_ICON_CLASS} size={16} stroke={1.8} />,
      key: "fullReload",
      label: "Full reload",
      onClick: handlers.onFullReload,
    });
  }

  /*
   * CDXC:SidebarV2ContextMenuParity 2026-07-30:
   * "Tag as" opens the same submenu panel the snooze presets use, which since
   * 2026-07-30 IS V1's flown-out second portal, opened below the parent row.
   * The options come from
   * the shared enabled-and-visible resolver, with the session's current tag
   * force-included so a hidden or retired marker can still be cleared. There is
   * no separate "Clear tag" row for the same reason V1 has none — re-picking the
   * tag a row already carries IS the clear, one click deep.
   */
  const currentSessionTag = getEffectiveSessionTag(session);
  const tagSubmenuItems = getEnabledVisibleSidebarSessionTagSections(
    options.sessionTagListItems,
    { includeTags: currentSessionTag ? [currentSessionTag] : [] },
  ).flatMap((section) =>
    /*
     * CDXC:SidebarV2ContextMenuLook 2026-07-30:
     * The resolver's own sections are carried through as `sectionKey` so the
     * panel can draw V1's dividers between Priority / Progress / Type instead of
     * one flat run of markers. The ORDER and the SET are untouched.
     */
    section.options.map((option) => ({ ...option, sectionKey: section.label })),
  );

  const stateActions: SidebarV2ContextMenuAction[] = [
    {
      icon: isPinned ? (
        <IconPinnedOff aria-hidden="true" className="session-context-menu-icon" size={16} stroke={1.8} />
      ) : (
        <IconPinned aria-hidden="true" className="session-context-menu-icon" size={16} stroke={1.8} />
      ),
      key: "pin",
      label: isPinned ? "Unpin" : "Pin",
      onClick: () => handlers.onSetPinned(!isPinned),
    },
  ];
  const onSetSessionTag = handlers.onSetSessionTag;
  if (eligibility?.canTagSession === true && onSetSessionTag && tagSubmenuItems.length > 0) {
    stateActions.push({
      icon: <IconTag aria-hidden="true" className={CONTEXT_MENU_ICON_CLASS} size={16} stroke={1.8} />,
      key: "tagAs",
      label: "Tag as",
      // The parent only reveals the markers; assigning one is always explicit.
      onClick: () => undefined,
      submenu: tagSubmenuItems.map((option) => ({
        icon: (
          <SessionTagIcon
            className="session-context-menu-icon session-tag-colored-icon"
            fillFavorite
            size={16}
            stroke={1.8}
            tag={option.value}
          />
        ),
        isChecked: currentSessionTag === option.value,
        key: option.value,
        label: option.label,
        onClick: () =>
          onSetSessionTag(currentSessionTag === option.value ? undefined : option.value),
        sectionKey: option.sectionKey,
      })),
    });
  }
  stateActions.push(
    {
      icon: isSleeping ? (
        <IconPlayerPlay aria-hidden="true" className="session-context-menu-icon" size={16} stroke={1.8} />
      ) : (
        <IconMoon aria-hidden="true" className="session-context-menu-icon" size={16} stroke={1.8} />
      ),
      key: "sleep",
      label: isSleeping ? "Wake" : "Sleep",
      onClick: () => handlers.onSetSleeping(!isSleeping),
    },
  );

  const destructive: SidebarV2ContextMenuAction[] = [
    {
      danger: true,
      /*
       * CDXC:SidebarV2ContextMenuParity 2026-07-30:
       * DELIBERATE divergence from V1: V1 hides Close behind the
       * `showSessionCloseContextMenuAction` setting (default off), so its menu
       * usually has no way to end a session at all. V2's inbox is a triage
       * surface — settle, snooze, close are the three verdicts a row can get —
       * and hiding one of the three behind a setting would leave the model
       * incomplete. Close therefore stays unconditional here, and the setting is
       * intentionally NOT consulted.
       */
      icon: <IconX aria-hidden="true" className="session-context-menu-icon" size={16} stroke={1.8} />,
      key: "close",
      label: "Close",
      onClick: handlers.onClose,
    },
  ];

  return [primary, sessionActions, lifecycleActions, stateActions, destructive].filter(
    (section) => section.length > 0,
  );
}

/*
 * CDXC:SidebarV2LogicalProjects 2026-07-29:
 * The project group header's menu. Today it carries exactly one thing —
 * how this repository's checkouts merge across machines — and it is a SUBMENU
 * of three radio options rather than a dialog: the choice has three values, no
 * free text, and no confirmation step, so a dialog would add a modal surface
 * for one click.
 *
 * The chosen mode is written to EVERY member checkout of the group the user is
 * looking at. Acting on the visible merged row and silently changing only one
 * hidden member would produce a group that half-agrees with itself, which is
 * exactly the `groupingMode: undefined` (no checkmark) state this builder
 * renders when it finds one.
 */
export const SIDEBAR_V2_PROJECT_GROUPING_MENU_OPTIONS: readonly {
  label: string;
  mode: SidebarProjectGroupingMode;
}[] = [
  { label: "Repository", mode: "repository" },
  { label: "Repository + path", mode: "repositoryPath" },
  { label: "Keep separate", mode: "separate" },
];

export type SidebarV2ProjectGroupMenuState = {
  /** False for a project with no git origin: merging cannot apply, so the
      submenu is not offered at all rather than offered and inert. */
  canGroupAcrossMachines: boolean;
  /** The mode every member currently resolves to, or undefined when they
      disagree — no option is then marked as active. */
  groupingMode?: SidebarProjectGroupingMode;
};

export function createSidebarV2ProjectGroupMenuSections(
  group: SidebarV2ProjectGroupMenuState,
  handlers: {
    /**
     * CDXC:SidebarV2GroupedProjectUX 2026-07-30:
     * Park this project into Recent Projects. Omitted when no member checkout is
     * closable, so the item is absent rather than present-and-inert.
     */
    onCloseProject?: () => void;
    onSetGroupingMode: (mode: SidebarProjectGroupingMode) => void;
  },
): SidebarV2ContextMenuAction[][] {
  const grouping: SidebarV2ContextMenuAction[] = group.canGroupAcrossMachines
    ? [
        {
          icon: (
            <IconServer aria-hidden="true" className={CONTEXT_MENU_ICON_CLASS} size={16} stroke={1.8} />
          ),
          key: "groupAcrossMachines",
          label: "Group across machines",
          // The parent only reveals the choices; picking one is always explicit.
          onClick: () => undefined,
          submenu: SIDEBAR_V2_PROJECT_GROUPING_MENU_OPTIONS.map((option) => ({
            isChecked: group.groupingMode === option.mode,
            key: option.mode,
            label: option.label,
            onClick: () => handlers.onSetGroupingMode(option.mode),
          })),
        },
      ]
    : [];
  /*
   * CDXC:SidebarV2GroupedProjectUX 2026-07-30:
   * Close Project is the missing half of V1's open/closed project semantics: the
   * grouped inbox only ever showed OPEN projects, but had no way to close one, so
   * the only way out of the list was the classic sidebar. Reopening needs nothing
   * new — the existing Recent Projects flow is version-agnostic.
   *
   * It is `danger`-styled and sits in its own section for the same reason V1 puts
   * it last: it removes a row from the list, and it must not be one slip away
   * from the grouping radio options above it.
   */
  const destructive: SidebarV2ContextMenuAction[] = handlers.onCloseProject
    ? [
        {
          danger: true,
          icon: <IconX aria-hidden="true" className={CONTEXT_MENU_ICON_CLASS} size={16} stroke={1.8} />,
          key: "closeProject",
          label: "Close Project",
          onClick: handlers.onCloseProject,
        },
      ]
    : [];
  return [grouping, destructive].filter((section) => section.length > 0);
}

export type SidebarV2ContextMenuProps = {
  onDismiss: () => void;
  position: SidebarV2ContextMenuPosition;
  sections: readonly (readonly SidebarV2ContextMenuAction[])[];
  /**
   * CDXC:SidebarV2ContextMenuLook 2026-07-30:
   * Which classic menu this one is. The two differ by exactly one thing — their
   * width — and the classic sidebar sets both: a session row's menu is 178px
   * (`.sidebar-session-context-menu`), a project row's is 196px (an inline width
   * on the project portal), because project labels are longer. Defaulting to
   * `session` keeps every existing row-menu mount unchanged.
   */
  variant?: "projectGroup" | "session";
  vscode: WebviewApi;
};

/** V1's project-row context menu width, from `session-group-section`. */
const SIDEBAR_V2_PROJECT_GROUP_MENU_WIDTH_PX = 196;

/**
 * CDXC:SidebarV2ContextMenuLook 2026-07-30:
 * Where an open submenu panel starts: the left edge of its parent ROW and 4px
 * under it, which is V1's own tag-submenu anchor.
 */
type SidebarV2ContextSubmenuAnchor = {
  left: number;
  top: number;
};

type SidebarV2OpenSubmenu = SidebarV2ContextSubmenuAnchor & {
  items: readonly SidebarV2ContextMenuSubmenuItem[];
  key: string;
  label: string;
};

/**
 * Consecutive options that name the same `sectionKey` are one block. Options
 * with no key are one implicit block, which is how the snooze and grouping
 * submenus keep exactly the shape they had before sections existed.
 */
function groupSidebarV2SubmenuItems(
  items: readonly SidebarV2ContextMenuSubmenuItem[],
): { items: SidebarV2ContextMenuSubmenuItem[]; key: string }[] {
  const groups: { items: SidebarV2ContextMenuSubmenuItem[]; key: string }[] = [];
  for (const item of items) {
    const key = item.sectionKey ?? "";
    const currentGroup = groups[groups.length - 1];
    if (currentGroup && currentGroup.key === key) {
      currentGroup.items.push(item);
      continue;
    }
    groups.push({ items: [item], key });
  }
  return groups;
}

function areSubmenuStylesEqual(
  previousStyle: CSSProperties | undefined,
  nextStyle: CSSProperties,
): boolean {
  return (
    previousStyle?.left === nextStyle.left &&
    previousStyle?.top === nextStyle.top &&
    previousStyle?.maxHeight === nextStyle.maxHeight
  );
}

/*
 * CDXC:SidebarV2ContextMenuLook 2026-07-30:
 * The submenu is a SEPARATE portal panel stacked above the parent menu, exactly
 * like V1's `Tag as` flyout: same `.session-context-menu` chrome, the same
 * submenu z-index, opened below its parent row rather than beside it (which is
 * how V1 avoids flying out over the sidebar in a ~260px webview), and dismissed
 * by the parent portal's own backdrop/Escape handling because it is rendered
 * inside the parent menu's lifetime.
 *
 * It clamps itself from its RENDERED rect through the portal's exported
 * coordinate helper instead of an item-count height estimate, so a long tag list
 * near the bottom of the sidebar cannot be cut off by the webview edge.
 */
function SidebarV2ContextSubmenuPanel({
  anchor,
  items,
  label,
  onDismiss,
}: {
  anchor: SidebarV2ContextSubmenuAnchor;
  items: readonly SidebarV2ContextMenuSubmenuItem[];
  label: string;
  onDismiss: () => void;
}) {
  const panelRef = useRef<HTMLDivElement>(null);
  const [panelStyle, setPanelStyle] = useState<CSSProperties>({
    left: `${anchor.left}px`,
    top: `${anchor.top}px`,
  });

  useLayoutEffect(() => {
    const panel = panelRef.current;
    if (!panel) {
      return undefined;
    }

    const clampPanel = () => {
      const bounds = panel.getBoundingClientRect();
      const maxPanelHeight = Math.max(
        0,
        window.innerHeight - SIDEBAR_CONTEXT_MENU_VIEWPORT_MARGIN_PX * 2,
      );
      const nextStyle: CSSProperties = {
        left: `${getClampedSidebarContextMenuCoordinate(
          anchor.left,
          bounds.width,
          window.innerWidth,
        )}px`,
        maxHeight: `calc(100vh - ${SIDEBAR_CONTEXT_MENU_VIEWPORT_MARGIN_PX * 2}px)`,
        top: `${getClampedSidebarContextMenuCoordinate(
          anchor.top,
          Math.min(bounds.height, maxPanelHeight),
          window.innerHeight,
        )}px`,
      };
      setPanelStyle((previousStyle) =>
        areSubmenuStylesEqual(previousStyle, nextStyle) ? previousStyle : nextStyle,
      );
    };

    clampPanel();
    window.addEventListener("resize", clampPanel);
    const resizeObserver =
      typeof ResizeObserver === "undefined" ? undefined : new ResizeObserver(clampPanel);
    resizeObserver?.observe(panel);
    return () => {
      window.removeEventListener("resize", clampPanel);
      resizeObserver?.disconnect();
    };
  }, [anchor.left, anchor.top]);

  return createPortal(
    <div
      aria-label={label}
      className="session-context-menu sidebar-v2-context-submenu"
      data-empty-space-blocking="true"
      onClick={(event) => {
        event.stopPropagation();
      }}
      onContextMenu={(event) => {
        event.preventDefault();
        event.stopPropagation();
      }}
      ref={panelRef}
      role="menu"
      style={panelStyle}
    >
      {groupSidebarV2SubmenuItems(items).map((group) => (
        /* V1's own submenu section class: it owns the 2px item grid and the
           divider that separates one block from the next. */
        <div
          className="session-tag-menu-section sidebar-v2-context-submenu-section"
          key={`sidebar-v2-submenu-section-${group.key}`}
        >
          {group.items.map((item) => (
            <button
              aria-checked={item.isChecked === undefined ? undefined : item.isChecked}
              className="session-context-menu-item sidebar-v2-context-submenu-item"
              data-checked={item.isChecked === undefined ? undefined : String(item.isChecked)}
              key={item.key}
              onClick={() => {
                onDismiss();
                item.onClick();
              }}
              role={item.isChecked === undefined ? "menuitem" : "menuitemradio"}
              type="button"
            >
              {item.icon}
              <span className="sidebar-v2-context-submenu-label">{item.label}</span>
              {item.isChecked ? (
                <IconCheck
                  aria-hidden="true"
                  className="sidebar-v2-context-submenu-check"
                  size={14}
                  stroke={2}
                />
              ) : null}
              {item.trailingLabel ? (
                <span className="sidebar-v2-context-submenu-when">{item.trailingLabel}</span>
              ) : null}
            </button>
          ))}
        </div>
      ))}
    </div>,
    document.body,
  );
}

/*
 * CDXC:SidebarV2ContextMenuLook 2026-07-30:
 * The V2 row and group menus render through V1's own menu machinery, and they
 * now carry V1's own menu classes and widths as well: `SidebarContextMenuPortal`
 * (backdrop, native open/close notification, Escape, window-blur dismissal,
 * rendered-size viewport clamp) plus `session-context-menu` and, for a row menu,
 * `sidebar-session-context-menu` — which is what fixes the width at V1's
 * deterministic 178px instead of letting each row's longest label decide how
 * wide its menu is. Sections are V1's `Fragment` + divider + section structure,
 * so the grid gaps between sections are V1's to the pixel, and a submenu parent
 * carries V1's trailing chevron.
 *
 * Only the item MODEL is V2's: `createSidebarV2ContextMenuSections` stays the
 * single source of truth for which items exist and when, so adding an item never
 * touches this renderer.
 */
export function SidebarV2ContextMenu({
  onDismiss,
  position,
  sections,
  variant = "session",
  vscode,
}: SidebarV2ContextMenuProps) {
  const [openSubmenu, setOpenSubmenu] = useState<SidebarV2OpenSubmenu>();
  const isProjectGroupMenu = variant === "projectGroup";
  return (
    <>
      <SidebarContextMenuPortal
        menuClassName={`session-context-menu${
          isProjectGroupMenu ? "" : " sidebar-session-context-menu"
        } sidebar-v2-session-context-menu`}
        menuStyle={{
          left: `${position.clientX}px`,
          top: `${position.clientY}px`,
          /* V1 sets the project menu's width inline; the session menu takes its
             width from the class above, exactly as V1's does. */
          width: isProjectGroupMenu ? `${SIDEBAR_V2_PROJECT_GROUP_MENU_WIDTH_PX}px` : undefined,
        }}
        onDismiss={onDismiss}
        vscode={vscode}
      >
        {sections.map((section, sectionIndex) => (
          // Sections are positional, not identified: the index IS the identity.
          <Fragment key={`sidebar-v2-menu-section-${sectionIndex}`}>
            {sectionIndex > 0 ? (
              <div className="session-context-menu-divider" role="separator" />
            ) : null}
            <div className="session-context-menu-section">
              {section.map((action) => {
                const isExpanded = openSubmenu?.key === action.key;
                return (
                  <button
                    aria-expanded={action.submenu ? isExpanded : undefined}
                    aria-haspopup={action.submenu ? "menu" : undefined}
                    className={`session-context-menu-item${
                      action.danger ? " session-context-menu-item-danger" : ""
                    }`}
                    key={action.key}
                    onClick={(event) => {
                      /*
                       * A parent with a submenu only opens its panel. It must not
                       * dismiss the menu, or the choices it exists to offer would
                       * never be reachable.
                       */
                      const submenu = action.submenu;
                      if (submenu) {
                        const bounds = event.currentTarget.getBoundingClientRect();
                        setOpenSubmenu(
                          isExpanded
                            ? undefined
                            : {
                                items: submenu,
                                key: action.key,
                                label: action.label,
                                left: bounds.left,
                                top: bounds.bottom + 4,
                              },
                        );
                        return;
                      }
                      onDismiss();
                      action.onClick();
                    }}
                    role="menuitem"
                    type="button"
                  >
                    {action.icon}
                    {/*
                     * CDXC:SidebarV2ContextMenuLook 2026-07-30:
                     * The label is a span so it can yield space inside V1's fixed
                     * menu width. V2 carries labels V1 never had ("Group across
                     * machines", "New session on <branch>"); left as a bare text
                     * node they push the row — and its trailing chevron — past the
                     * menu box, turning the menu into a horizontal scroller and
                     * hiding the one glyph that says an item opens a panel.
                     */}
                    <span className="sidebar-v2-context-menu-label">{action.label}</span>
                    {action.submenu ? (
                      <IconChevronRight
                        aria-hidden="true"
                        className="session-context-menu-trailing-icon"
                        size={14}
                        stroke={1.8}
                      />
                    ) : null}
                  </button>
                );
              })}
            </div>
          </Fragment>
        ))}
      </SidebarContextMenuPortal>
      {openSubmenu ? (
        <SidebarV2ContextSubmenuPanel
          anchor={{ left: openSubmenu.left, top: openSubmenu.top }}
          items={openSubmenu.items}
          label={openSubmenu.label}
          onDismiss={onDismiss}
        />
      ) : null}
    </>
  );
}
