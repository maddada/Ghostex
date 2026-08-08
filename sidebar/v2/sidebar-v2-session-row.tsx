import {
  IconAlarm,
  IconAlarmOff,
  IconCheck,
  IconCircleCheck,
  IconCircleDashed,
  IconClock,
  IconGitBranch,
  IconPinned,
  IconPinnedOff,
  IconServer,
  IconArrowBackUp,
} from "@tabler/icons-react";
import {
  useEffect,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type MouseEvent as ReactMouseEvent,
} from "react";
import { PointerSensor } from "@dnd-kit/dom";
import { useSortable } from "@dnd-kit/react/sortable";
import type {
  SidebarSessionGitStatus,
  SidebarSessionItem,
} from "../../shared/session-grid-contract";
import type { SidebarV2Status } from "../../shared/sidebar-v2-status";
import { AppTooltip } from "../app-tooltip";
import { createSessionDragData } from "../sidebar-dnd";
import { getSidebarReorderActivationConstraints } from "../sidebar-reorder-activation";
import { formatSessionHeadingText } from "../session-card-content";
import { SidebarV2ProjectIcon, SidebarV2SessionIcon } from "./sidebar-v2-icons";
import type { SidebarV2ProjectIdentity } from "./sidebar-v2-view-model";

/*
 * CDXC:SidebarV2 2026-07-29:
 * One row component renders both V2 surfaces, because they are the same row at
 * two densities — the rich 3-line inbox card and the slim shelf row. Ported
 * from t3code's SidebarV2 with its central discipline intact:
 *
 * - There is ONE surface model. Background is reserved for interaction state
 *   (hover, active). Status never paints a background, a border, or an edge
 *   strip; it lives entirely in the right slot's text and hue.
 * - The right slot reserves the STATUS's width and floats the hover actions
 *   over the row's right edge, so the row never reflows on hover AND the
 *   project name keeps every pixel the status does not need (reserving the
 *   wider hover chrome instead truncated names against a half-empty line).
 *   Both states hide with `visibility`, so the hidden one is never a tabbable,
 *   hit-testable, screen-reader-visible ghost.
 * - In grouped mode the project line is dropped entirely (the group header
 *   already states the project) and the status moves onto the title line, so a
 *   grouped card is a real two-line card instead of a card with a blank line.
 * - Receding is four coordinated softenings (row color, title weight, title
 *   color, project weight), never hidden chrome, so a resting row stays
 *   readable and stays findable.
 */

export type SidebarV2SessionRowVariant = "card" | "slim";

/**
 * CDXC:SidebarV2Lifecycle 2026-07-29:
 * Which lifecycle action this row's hover slot offers. It is a property of the
 * SHELF the row is rendered on, not of the session: a settled row offers
 * un-settle, a snoozed row offers wake, and an inbox row offers settle+snooze.
 * `"none"` is for rows with no agent lifecycle at all (browser tabs).
 */
export type SidebarV2SessionRowLifecycleAction = "none" | "settle" | "unsettle" | "wake";

export type SidebarV2SessionRowLifecycle = {
  action: SidebarV2SessionRowLifecycleAction;
  /** True while a lifecycle write is in flight; the control stays visible but
      refuses a second click until the server delta lands. */
  isPending: boolean;
  /** The session woke from a snooze and has not been visited since. */
  isWoke: boolean;
  onSettle: () => void;
  onSnooze: (position: { clientX: number; clientY: number }) => void;
  onUnsettle: () => void;
  onWake: () => void;
  /** Offer the snooze clock button (inbox rows whose daemon supports snooze
      and whose session is not blocked on the user). */
  showSnooze: boolean;
  /** Compact "wakes in" text for snoozed shelf rows ("2h", "3d"). */
  wakeLabel?: string;
};

export type SidebarV2SessionRowProps = {
  dragGroupId?: string;
  dragIndex?: number;
  dropPosition?: "after" | "before";
  /** Extra label rendered instead of the status label on slim shelf rows. */
  slimLabel?: string;
  /**
   * CDXC:SidebarV2Git 2026-07-29:
   * gxserver's git/PR probe for this session's cwd, ALREADY capability-gated by
   * the caller: a machine whose daemon cannot probe git passes `undefined`, so
   * the row never has to know which gxserver it came from and "unsupported"
   * renders identically to "nothing to report".
   */
  gitStatus?: SidebarSessionGitStatus;
  isActive: boolean;
  isRenaming: boolean;
  /** True while this row owns an open context menu or snooze popover; the
      hover actions stay revealed because the pointer has left for the portal. */
  isMenuOpen?: boolean;
  lifecycle?: SidebarV2SessionRowLifecycle;
  /**
   * CDXC:SidebarV2LogicalProjects 2026-07-29:
   * The machine this session actually runs on, shown as a small badge. It is a
   * prop of its own rather than being read off `project` because grouped mode
   * drops the project line entirely, and a merged cross-machine group is
   * exactly where the badge matters most: without it, two rows from two
   * machines under one repository header are indistinguishable. Local sessions
   * pass nothing and get no badge.
   */
  machineName?: string;
  onActivate: (event: ReactMouseEvent<HTMLElement> | ReactKeyboardEvent<HTMLElement>) => void;
  onOpenMenu: (position: { clientX: number; clientY: number }) => void;
  onRenameCancel: () => void;
  onRenameCommit: (title: string) => void;
  onRenameStart: () => void;
  onTogglePinned: (pinned: boolean) => void;
  pinnedReorderEnabled?: boolean;
  /** Omitted in grouped mode: the project header already states the project. */
  project?: SidebarV2ProjectIdentity;
  session: SidebarSessionItem;
  showProjectIcons: boolean;
  status: SidebarV2Status;
  useColoredAgentIcons: boolean;
  variant: SidebarV2SessionRowVariant;
};

const SIDEBAR_V2_NO_LIFECYCLE: SidebarV2SessionRowLifecycle = {
  action: "none",
  isPending: false,
  isWoke: false,
  onSettle: () => undefined,
  onSnooze: () => undefined,
  onUnsettle: () => undefined,
  onWake: () => undefined,
  showSnooze: false,
};

function isInteractiveTarget(target: EventTarget | null): boolean {
  return target instanceof Element && target.closest("button, a, input") !== null;
}

const sidebarV2PinnedSessionSensors = [
  PointerSensor.configure({
    activationConstraints: getSidebarReorderActivationConstraints,
    preventActivation(event, source) {
      const target = event.target instanceof Element ? event.target : undefined;
      return Boolean(
        target && target !== source.element && target.closest("button, a, input, textarea, select"),
      );
    },
  }),
];

/*
 * CDXC:SidebarV2Git 2026-07-29:
 * The card's third line is the WORK line: which branch, which review, how big.
 * Everything below is about deciding whether that line has anything to say,
 * because V2 never reserves a blank row — a session with no git data keeps the
 * exact card it had before P3 existed.
 */
type SidebarV2GitDisplay = {
  additions: number;
  branch: string;
  deletions: number;
  hasDiff: boolean;
  prNumber?: number;
  /** `"unknown"` when gxserver reported a number without a state. */
  prState: "closed" | "draft" | "merged" | "open" | "unknown";
  /** Composed hover text: branch, PR state, diff — whatever exists. */
  tooltip: string;
};

const SIDEBAR_V2_PR_STATE_LABELS: Record<SidebarV2GitDisplay["prState"], string> = {
  closed: "Closed",
  draft: "Draft",
  merged: "Merged",
  open: "Open",
  unknown: "Pull request",
};

function nonNegativeCount(value: number | undefined): number {
  return typeof value === "number" && Number.isFinite(value) && value > 0 ? Math.trunc(value) : 0;
}

/**
 * Resolves the display shape of one session's git state, or `undefined` when
 * there is nothing to render. An object with a null branch, no PR, and a 0/0
 * diff is a legitimate answer from gxserver (a clean detached checkout) and
 * must produce NO line rather than an empty one.
 */
export function resolveSidebarV2GitDisplay(
  gitStatus: SidebarSessionGitStatus | undefined,
): SidebarV2GitDisplay | undefined {
  if (!gitStatus) {
    return undefined;
  }
  const branch = gitStatus.branch?.trim() ?? "";
  const prNumber =
    typeof gitStatus.prNumber === "number" && Number.isFinite(gitStatus.prNumber)
      ? Math.trunc(gitStatus.prNumber)
      : undefined;
  const additions = nonNegativeCount(gitStatus.additions);
  const deletions = nonNegativeCount(gitStatus.deletions);
  const hasDiff = additions > 0 || deletions > 0;
  if (branch === "" && prNumber === undefined && !hasDiff) {
    return undefined;
  }
  const prState = prNumber === undefined ? "unknown" : (gitStatus.prState ?? "unknown");
  const tooltip = [
    branch === "" ? undefined : branch,
    prNumber === undefined
      ? undefined
      : `#${prNumber} · ${SIDEBAR_V2_PR_STATE_LABELS[prState]}`,
    hasDiff ? `+${additions} −${deletions}` : undefined,
  ]
    .filter((part): part is string => part !== undefined)
    .join(" · ");
  return { additions, branch, deletions, hasDiff, prNumber, prState, tooltip };
}

/**
 * The PR badge. It is a subtle PILL rather than colored text so the number
 * reads as a chip at 11px, but the color still lives entirely inside the badge:
 * V2's one surface model forbids status from painting the row's background or
 * edges, and a review state is a status.
 */
function PrBadge({ git }: { git: SidebarV2GitDisplay }) {
  if (git.prNumber === undefined) {
    return null;
  }
  return (
    <span
      aria-label={`#${git.prNumber} · ${SIDEBAR_V2_PR_STATE_LABELS[git.prState]}`}
      className="sidebar-v2-row-pr"
      data-pr-state={git.prState}
      data-sidebar-v2-pr="true"
    >
      {`#${git.prNumber}`}
    </span>
  );
}

function StatusLabel({ status }: { status: SidebarV2Status }) {
  return (
    <span
      className="sidebar-v2-status"
      data-hue={status.hue}
      data-kind={status.kind}
      data-pulse={String(status.pulse)}
    >
      {status.kind === "working" ? (
        <IconCircleDashed aria-hidden="true" size={16} stroke={1.8} />
      ) : null}
      {status.kind === "done" ? <IconCircleCheck aria-hidden="true" size={16} stroke={1.8} /> : null}
      <span role="status">{status.label}</span>
    </span>
  );
}

export function SidebarV2SessionRow({
  dragGroupId,
  dragIndex = 0,
  dropPosition,
  slimLabel,
  gitStatus,
  isActive,
  isMenuOpen = false,
  isRenaming,
  lifecycle = SIDEBAR_V2_NO_LIFECYCLE,
  machineName,
  onActivate,
  onOpenMenu,
  onRenameCancel,
  onRenameCommit,
  onRenameStart,
  onTogglePinned,
  pinnedReorderEnabled = false,
  project,
  session,
  showProjectIcons,
  status,
  useColoredAgentIcons,
  variant,
}: SidebarV2SessionRowProps) {
  const headingText = formatSessionHeadingText(session);
  const git = resolveSidebarV2GitDisplay(gitStatus);
  const [renameDraft, setRenameDraft] = useState(headingText);
  const renameInputRef = useRef<HTMLInputElement>(null);
  const renameCommittedRef = useRef(false);
  const isBrowser = session.kind === "browser" || session.sessionKind === "browser";
  const isPinned = session.isPinned === true;
  const isPinnedDragEnabled =
    pinnedReorderEnabled &&
    isPinned &&
    !isBrowser &&
    !isMenuOpen &&
    !isRenaming &&
    Boolean(dragGroupId);
  const sortable = useSortable({
    accept: "session",
    data: createSessionDragData(dragGroupId ?? "", session.sessionId),
    disabled: !isPinnedDragEnabled,
    feedback: "clone",
    group: dragGroupId ?? "",
    id: session.sessionId,
    index: dragIndex,
    sensors: sidebarV2PinnedSessionSensors,
    type: "session",
  });
  /*
   * In-flight rows (working, or blocked on the user) are not the user's
   * problem yet, so they soften to 70% until hovered. `recede` from the shared
   * status resolver covers the resting rows; the two treatments stack.
   */
  const isInFlight = status.kind === "working" || status.kind === "approval" || status.kind === "input";
  const shouldRecede = status.recede && !isActive;

  useEffect(() => {
    if (!isRenaming) {
      return;
    }
    renameCommittedRef.current = false;
    setRenameDraft(headingText);
    const input = renameInputRef.current;
    if (input) {
      input.focus();
      input.select();
    }
    // Re-seeding the draft on every heading change would fight the user's
    // typing, so this deliberately depends on rename mode alone and reads the
    // heading only at the moment the editor opens.
  }, [isRenaming]);

  const commitRename = () => {
    if (renameCommittedRef.current) {
      return;
    }
    renameCommittedRef.current = true;
    const nextTitle = renameDraft.trim();
    if (nextTitle.length === 0 || nextTitle === headingText.trim()) {
      onRenameCancel();
      return;
    }
    onRenameCommit(nextTitle);
  };

  const cancelRename = () => {
    renameCommittedRef.current = true;
    onRenameCancel();
  };

  const title = isRenaming ? (
    <input
      className="sidebar-v2-row-rename-input"
      onBlur={commitRename}
      onChange={(event) => setRenameDraft(event.target.value)}
      onClick={(event) => event.stopPropagation()}
      onDoubleClick={(event) => event.stopPropagation()}
      onKeyDown={(event) => {
        event.stopPropagation();
        if (event.key === "Enter") {
          event.preventDefault();
          commitRename();
          return;
        }
        if (event.key === "Escape") {
          event.preventDefault();
          cancelRename();
        }
      }}
      ref={renameInputRef}
      value={renameDraft}
    />
  ) : (
    <span className="sidebar-v2-row-title">{headingText}</span>
  );

  /*
   * CDXC:SidebarV2Lifecycle 2026-07-29:
   * Lifecycle buttons live in the floating action bar that keeps the row from
   * reflowing on hover. They occupy the bar's right end — closest to the status
   * they replace — because settle is the action a triaging user reaches for
   * repeatedly; only unpin sits to their left (2026-07-30), and only on the rows
   * that have something to unpin.
   *
   * Every one of them is capability-gated upstream: an unsupported daemon means
   * `action: "none"` / `showSnooze: false`, so the button is not rendered at
   * all instead of rendering a control that would 404.
   */
  const lifecycleActions = (
    <>
      {lifecycle.showSnooze ? (
        <button
          aria-label="Snooze session"
          className="sidebar-v2-row-action"
          data-lifecycle-action="snooze"
          disabled={lifecycle.isPending}
          onClick={(event) => {
            event.preventDefault();
            event.stopPropagation();
            const bounds = event.currentTarget.getBoundingClientRect();
            lifecycle.onSnooze({ clientX: bounds.left, clientY: bounds.bottom + 4 });
          }}
          type="button"
        >
          <IconClock aria-hidden="true" size={14} stroke={1.8} />
        </button>
      ) : null}
      {lifecycle.action === "settle" ? (
        <button
          aria-label="Settle session"
          className="sidebar-v2-row-action sidebar-v2-row-action-labelled"
          data-lifecycle-action="settle"
          disabled={lifecycle.isPending}
          onClick={(event) => {
            event.preventDefault();
            event.stopPropagation();
            lifecycle.onSettle();
          }}
          type="button"
        >
          <IconCheck aria-hidden="true" size={14} stroke={2} />
          {variant === "card" ? <span>Settle</span> : null}
        </button>
      ) : null}
      {lifecycle.action === "unsettle" ? (
        <button
          aria-label="Un-settle session"
          className="sidebar-v2-row-action"
          data-lifecycle-action="unsettle"
          disabled={lifecycle.isPending}
          onClick={(event) => {
            event.preventDefault();
            event.stopPropagation();
            lifecycle.onUnsettle();
          }}
          type="button"
        >
          <IconArrowBackUp aria-hidden="true" size={14} stroke={1.8} />
        </button>
      ) : null}
      {lifecycle.action === "wake" ? (
        <button
          aria-label="Wake session now"
          className="sidebar-v2-row-action"
          data-lifecycle-action="wake"
          disabled={lifecycle.isPending}
          onClick={(event) => {
            event.preventDefault();
            event.stopPropagation();
            lifecycle.onWake();
          }}
          type="button"
        >
          <IconAlarmOff aria-hidden="true" size={14} stroke={1.8} />
        </button>
      ) : null}
    </>
  );

  /*
   * 2026-07-30 (UX batch):
   * The bar carries UNPIN and the lifecycle verbs, and nothing else.
   *
   * - Unpin comes FIRST and exists only on a pinned row. A pin control on every
   *   row spent the scarcest chrome in the sidebar on the rarest action, and the
   *   bar's job is triage. Pinning an unpinned session is a right-click away.
   * - There is no ⋯ trigger: right-clicking the row IS the menu, everywhere in
   *   Ghostex, so a button that duplicates it only competed with the verbs.
   */
  const actions = (
    <span className="sidebar-v2-row-actions">
      {isPinned ? (
        <button
          aria-label="Unpin session"
          className="sidebar-v2-row-action"
          data-row-action="unpin"
          onClick={(event) => {
            event.preventDefault();
            event.stopPropagation();
            onTogglePinned(false);
          }}
          type="button"
        >
          <IconPinnedOff aria-hidden="true" size={14} stroke={1.8} />
        </button>
      ) : null}
      {lifecycleActions}
    </span>
  );

  /*
   * CDXC:SidebarV2Lifecycle 2026-07-29:
   * What the resting slot says, in the order the design ranks it:
   *
   * 1. A snoozed row states when it comes BACK ("2h"). The return ticket is the
   *    whole story of that row; how long ago it was last touched is not.
   * 2. A woken row states that it woke, and keeps saying so until visited (the
   *    inbox sort is static, so the row reappears in its old position and the
   *    signal has to carry the weight on its own). It only replaces the QUIET
   *    states — a woken row that is now working or blocked shows that instead,
   *    because a live status outranks a historical one.
   * 3. Otherwise the shelf label or the normal status label.
   */
  const restingSlot = (() => {
    if (lifecycle.wakeLabel !== undefined) {
      return (
        <span className="sidebar-v2-wake-label" data-lifecycle-label="wake">
          {lifecycle.wakeLabel}
        </span>
      );
    }
    if (lifecycle.isWoke && (status.kind === "idle" || status.kind === "done")) {
      return (
        <span
          aria-label="Woke from snooze"
          className="sidebar-v2-woke"
          data-lifecycle-label="woke"
          role="status"
        >
          <IconAlarm aria-hidden="true" size={14} stroke={1.8} />
          Woke
        </span>
      );
    }
    if (slimLabel !== undefined) {
      return <span>{slimLabel}</span>;
    }
    /*
     * A woken row whose live status is loud (blocked on the user, failed, still
     * working) shows that status, prefixed by the wake glyph. Two competing
     * labels would make the row read as two rows; the glyph is the smallest
     * thing that still says "this one interrupted its own snooze".
     */
    return (
      <>
        {lifecycle.isWoke ? (
          <span
            aria-label="Woke from snooze"
            className="sidebar-v2-woke-mark"
            data-lifecycle-mark="woke"
            role="img"
          >
            <IconAlarm aria-hidden="true" size={13} stroke={1.8} />
          </span>
        ) : null}
        <StatusLabel status={status} />
      </>
    );
  })();

  /*
   * CDXC:SidebarV2Git 2026-07-29 (row-width fix):
   * A slim shelf row's PR badge is RESTING content, so it lives inside the
   * resting slot and swaps with it. It used to sit outside the slot, back when
   * the slot reserved the action bar's width and therefore pushed the badge
   * clear of it; now that the actions float over the row's right edge, an
   * outside badge would sit half under the chips on every hover. Cards
   * keep their badge on the git line, which the bar never reaches.
   */
  const slimPrBadge = variant === "slim" && git ? <PrBadge git={git} /> : null;

  const rightSlot = (
    <span className="sidebar-v2-row-slot">
      <span className="sidebar-v2-row-slot-status">
        {/*
         * 2026-07-30 (UX batch):
         * A pinned row's resting mark. The pin CONTROL is hover-only and only
         * exists on pinned rows, so without this a pinned row said nothing about
         * itself at rest except its position in the list. It is resting content
         * like the status it precedes — it swaps out for the unpin chip on
         * hover, so line 1 still does not reflow across the reveal.
         */}
        {isPinned ? (
          <span
            aria-label="Pinned"
            className="sidebar-v2-row-pin-mark"
            data-sidebar-v2-pinned="true"
            role="img"
          >
            <IconPinned aria-hidden="true" size={12} stroke={1.8} />
          </span>
        ) : null}
        {slimPrBadge}
        {restingSlot}
      </span>
      {actions}
    </span>
  );

  /*
   * CDXC:SidebarV2Git 2026-07-29, revised 2026-07-30:
   * The meta line is THE WORK LINE: branch, review, diff — plus the machine
   * badge when the work is happening somewhere else.
   *
   * - A card is at most 3 lines (project / title / meta), and t3code's card is
   *   exactly that. Giving git its own fourth line would make every flat card
   *   ~25% taller and break the `data-card-lines` intrinsic-size ladder that
   *   keeps the scrollbar still while rows realize.
   * - `session.detail` is NOT the agent name P3 assumed it was. gxserver defines
   *   it as the session's cwd, falling back to the project's path, so on real
   *   snapshots this line rendered a folder path — a truncated repeat of the
   *   project line that also crowded out the branch. It is gone: the line
   *   renders git or nothing.
   * - The machine badge is enough on its own to keep the line. "Which machine is
   *   this running on" is the one fact a row cannot express anywhere else, and a
   *   remote daemon that cannot probe git is exactly the case where the badge
   *   would otherwise vanish with the branch.
   *
   * Every line stays conditional on having something to say: 3 lines only when
   * there is a project line AND work to report, 2 or 1 otherwise, and never a
   * blank reserved row.
   */
  const machineBadgeName = machineName?.trim() || project?.machineName?.trim();
  const hasMetaRow =
    variant === "card" && (git !== undefined || Boolean(machineBadgeName));
  const hasProjectLine = variant === "card" && project !== undefined;
  const cardLineCount = 1 + (hasProjectLine ? 1 : 0) + (hasMetaRow ? 1 : 0);

  return (
    <li
      className="sidebar-v2-row-item"
      data-card-lines={variant === "card" ? String(cardLineCount) : undefined}
      data-dragging={String(Boolean(sortable.isDragging))}
      data-drop-position={sortable.isDragging ? undefined : dropPosition}
      data-pinned-reorderable={String(isPinnedDragEnabled)}
      data-sidebar-session-group-id={dragGroupId}
      data-variant={variant}
      ref={sortable.ref}
    >
      {/*
       * `data-woke` is a state hook on the ROW (tests, future affordances); the
       * visible signal stays inside the right slot, because V2 has one surface
       * model and row backgrounds/edges belong to interaction state alone.
       */}
      <div
        className="sidebar-v2-row"
        data-active={String(isActive)}
        data-in-flight={String(isInFlight)}
        data-lifecycle-action={lifecycle.action}
        data-lifecycle-state={session.isSleeping === true ? "sleeping" : "running"}
        data-menu-open={String(isMenuOpen)}
        data-pinned={String(isPinned)}
        data-recede={String(shouldRecede)}
        data-session-id={session.sessionId}
        data-sidebar-session-id={session.sessionId}
        data-sidebar-v2-row="true"
        data-status-kind={status.kind}
        data-variant={variant}
        data-woke={String(lifecycle.isWoke)}
        ref={isPinnedDragEnabled ? sortable.handleRef : undefined}
        onClick={(event) => {
          if (isRenaming || isInteractiveTarget(event.target)) {
            return;
          }
          event.stopPropagation();
          /*
           * The second click of a double-click must not also activate, or
           * double-click-to-rename would race a focus switch.
           */
          if (event.detail > 1) {
            return;
          }
          onActivate(event);
        }}
        onContextMenu={(event) => {
          event.preventDefault();
          event.stopPropagation();
          onOpenMenu({ clientX: event.clientX, clientY: event.clientY });
        }}
        onDoubleClick={(event) => {
          if (isBrowser || isRenaming || isInteractiveTarget(event.target)) {
            return;
          }
          event.preventDefault();
          event.stopPropagation();
          onRenameStart();
        }}
        onKeyDown={(event) => {
          if (event.target !== event.currentTarget) {
            return;
          }
          if (event.key !== "Enter" && event.key !== " ") {
            return;
          }
          event.preventDefault();
          onActivate(event);
        }}
        role="button"
        tabIndex={0}
      >
        {variant === "card" ? (
          <>
            {project ? (
              <div className="sidebar-v2-row-line" data-line="project">
                {showProjectIcons ? (
                  <SidebarV2ProjectIcon
                    discoveredIconDataUrl={project.discoveredIconDataUrl}
                    fallback={project.isWorktree ? "worktree" : "folder"}
                    icon={project.icon}
                    iconDataUrl={project.iconDataUrl}
                    title={project.title}
                  />
                ) : null}
                <span className="sidebar-v2-row-project">{project.title}</span>
                {rightSlot}
              </div>
            ) : null}
            <div className="sidebar-v2-row-line" data-line="title">
              <SidebarV2SessionIcon
                agentIcon={session.agentIcon}
                faviconDataUrl={session.faviconDataUrl}
                isBrowser={isBrowser}
                useColoredAgentIcons={useColoredAgentIcons}
              />
              {title}
              {/*
               * Without a project line there is no line 1 to carry the status,
               * so it rides the title line instead of leaving a blank row.
               */}
              {project ? null : rightSlot}
            </div>
            {hasMetaRow ? (
              <div
                className="sidebar-v2-row-line"
                data-line="meta"
                data-meta={git ? "git" : "machine"}
              >
                {git ? (
                  <AppTooltip content={git.tooltip}>
                    <span className="sidebar-v2-row-git" data-sidebar-v2-git="true">
                    {git.branch === "" ? (
                      <span className="sidebar-v2-row-git-spacer" />
                    ) : (
                      <span className="sidebar-v2-row-branch">
                        <IconGitBranch aria-hidden="true" size={12} stroke={1.8} />
                        <span className="sidebar-v2-row-branch-name">{git.branch}</span>
                      </span>
                    )}
                    <PrBadge git={git} />
                    {/*
                     * A 0/0 diff is silence, not "no changes yet": the pair is
                     * dropped entirely rather than rendered as +0 −0.
                     */}
                    {git.hasDiff ? (
                      <span className="sidebar-v2-row-diff">
                        <span className="sidebar-v2-row-diff-added">{`+${git.additions}`}</span>
                        <span className="sidebar-v2-row-diff-removed">{`−${git.deletions}`}</span>
                      </span>
                    ) : null}
                    </span>
                  </AppTooltip>
                ) : null}
                {machineBadgeName ? (
                  <span className="sidebar-v2-row-machine" data-sidebar-v2-machine="true">
                    <IconServer aria-hidden="true" size={14} stroke={1.8} />
                    {machineBadgeName}
                  </span>
                ) : null}
              </div>
            ) : null}
          </>
        ) : (
          <>
            <SidebarV2SessionIcon
              agentIcon={session.agentIcon}
              faviconDataUrl={session.faviconDataUrl}
              isBrowser={isBrowser}
              useColoredAgentIcons={useColoredAgentIcons}
            />
            {title}
            {/*
             * CDXC:SidebarV2Git 2026-07-29:
             * A slim shelf row keeps the PR badge and nothing else (t3code
             * parity). Parked work is scanned for "did that ship?", which is
             * the one question the badge answers; branch and diff belong to
             * work you are still doing. The badge is rendered INSIDE the
             * resting slot (see `slimPrBadge`), so it swaps with the time
             * label instead of sitting under the floating action bar.
             */}
            {rightSlot}
          </>
        )}
      </div>
    </li>
  );
}
