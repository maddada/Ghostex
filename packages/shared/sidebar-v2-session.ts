import {
  getSidebarSessionLifecycleState,
  type SidebarSessionGroup,
  type SidebarSessionItem,
} from "./session-grid-contract-sidebar";

/*
CDXC:SidebarV2 2026-07-29-00:00:
Sidebar V2 ("Inbox") keeps its pure sidebar logic on compact Ghostex session types.
Every V2 module works on the small structural types declared here instead of the
full `SidebarSessionItem`/`SidebarSessionGroup` contracts, so the logic stays
testable, host-agnostic, and free of the contract's ~120 unrelated fields.

Concept mapping:
- provider status running/starting -> `activity: "working"`. A Ghostex
  session's `lifecycleState: "running"` only means the terminal/pane is ALIVE,
  not that an agent is doing work, so it must never be read as "working".
- pending approvals or user input -> `activity: "attention"`,
  optionally refined by `attentionKind` once gxserver can tell the two apart.
- provider error -> `lifecycleState: "error"` (the sidebar
  projection already folds gxserver's `missing`/`unknown` domain states there).
- provider turn timestamps and latest user activity -> `lastInteractionAt`,
  gxserver's meaningful-activity clock, plus `workingStartedAt` for the current
  working stint.

The P2 lifecycle fields (`settledAt`, `settledOverride`, `snoozedUntil`,
`snoozedAt`) are OPTIONAL here: the logic runs on derived data today and snaps
to real server state once gxserver publishes it.
*/

export type SidebarV2Activity = "attention" | "idle" | "working";

/** Amber sub-classification of `activity: "attention"`. gxserver does not
    distinguish approval from input yet, so this is optional and resolves to
    "input" when absent. */
export type SidebarV2AttentionKind = "approval" | "input";

export type SidebarV2LifecycleState = "done" | "error" | "running" | "sleeping";

export type SidebarV2SessionKind = "browser" | "terminal";

/** Server-owned settle pin (P2). "settled" forces the settled shelf, "active"
    pins a session into the inbox and suppresses auto-settle. */
export type SidebarV2SettledOverride = "active" | "settled";

/** Pull-request state for a session's worktree branch (P3). */
export type SidebarV2ChangeRequestState = "closed" | "merged" | "open";

export type SidebarV2Session = {
  activity: SidebarV2Activity;
  /** Host-provided activity text; preferred over the generic status label. */
  activityLabel?: string;
  attentionKind?: SidebarV2AttentionKind;
  /**
   * Creation stamp driving the position-stable inbox order. gxserver's
   * `GxserverPresentationSession.createdAt` carries it, but the sidebar
   * contract does not project it yet, so callers may leave it unset and pass a
   * first-seen ranking to the sort instead (see `sidebar-v2-sort`).
   */
  createdAt?: string;
  isPinned?: boolean;
  lastInteractionAt?: string;
  lifecycleState?: SidebarV2LifecycleState;
  projectId?: string;
  sessionId: string;
  sessionKind?: SidebarV2SessionKind;
  settledAt?: string;
  settledOverride?: SidebarV2SettledOverride;
  snoozedAt?: string;
  snoozedUntil?: string;
  workingStartedAt?: string;
  worktreePath?: string;
};

/**
 * `SidebarSessionItem` widened with the fields V2 needs that the contract does
 * not carry yet. Declaring them optional here keeps the adapter honest today
 * and requires no change once they land on the contract itself.
 */
export type SidebarV2SessionSource = SidebarSessionItem &
  Partial<
    Pick<
      SidebarV2Session,
      | "attentionKind"
      | "createdAt"
      | "projectId"
      | "settledAt"
      | "settledOverride"
      | "snoozedAt"
      | "snoozedUntil"
      | "worktreePath"
    >
  >;

/** Side-channel values for fields the sidebar contract cannot supply yet. */
export type SidebarV2SessionOverrides = Partial<
  Pick<
    SidebarV2Session,
    | "attentionKind"
    | "createdAt"
    | "projectId"
    | "settledAt"
    | "settledOverride"
    | "snoozedAt"
    | "snoozedUntil"
    | "worktreePath"
  >
>;

function omitUndefined<T extends Record<string, unknown>>(value: T): T {
  const result: Record<string, unknown> = {};
  for (const [key, entry] of Object.entries(value)) {
    if (entry !== undefined) {
      result[key] = entry;
    }
  }
  return result as T;
}

export function toSidebarV2Session(
  session: SidebarV2SessionSource,
  overrides: SidebarV2SessionOverrides = {},
): SidebarV2Session {
  const merged: SidebarV2Session = {
    activity: session.activity,
    activityLabel: session.activityLabel,
    attentionKind: overrides.attentionKind ?? session.attentionKind,
    createdAt: overrides.createdAt ?? session.createdAt,
    isPinned: session.isPinned,
    lastInteractionAt: session.lastInteractionAt,
    lifecycleState: getSidebarSessionLifecycleState(session),
    projectId: overrides.projectId ?? session.projectId,
    sessionId: session.sessionId,
    sessionKind: session.sessionKind ?? (session.kind === "browser" ? "browser" : undefined),
    settledAt: overrides.settledAt ?? session.settledAt,
    settledOverride: overrides.settledOverride ?? session.settledOverride,
    snoozedAt: overrides.snoozedAt ?? session.snoozedAt,
    snoozedUntil: overrides.snoozedUntil ?? session.snoozedUntil,
    workingStartedAt: session.workingStartedAt,
    worktreePath: overrides.worktreePath ?? session.worktreePath,
  };
  return omitUndefined(merged);
}

export type SidebarV2GroupAdapterOptions = {
  overridesBySessionId?: ReadonlyMap<string, SidebarV2SessionOverrides>;
};

/**
 * Flattens one sidebar group into V2 sessions, stamping the owning project id
 * so flat-inbox rows can still resolve their project, machine badge, and
 * grouped-mode placement.
 */
export function toSidebarV2SessionsFromGroup(
  group: Pick<SidebarSessionGroup, "groupId" | "sessions">,
  options: SidebarV2GroupAdapterOptions = {},
): SidebarV2Session[] {
  return group.sessions.map((session) =>
    toSidebarV2Session(session, {
      projectId: group.groupId,
      ...options.overridesBySessionId?.get(session.sessionId),
    }),
  );
}

export function toSidebarV2SessionsFromGroups(
  groups: readonly Pick<SidebarSessionGroup, "groupId" | "sessions">[],
  options: SidebarV2GroupAdapterOptions = {},
): SidebarV2Session[] {
  return groups.flatMap((group) => toSidebarV2SessionsFromGroup(group, options));
}

export function isSidebarV2BrowserSession(
  session: Pick<SidebarV2Session, "sessionKind">,
): boolean {
  return session.sessionKind === "browser";
}
