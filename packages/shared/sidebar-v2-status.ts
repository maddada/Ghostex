import type { SidebarV2Session } from './sidebar-v2-session';

/*
CDXC:SidebarV2 2026-07-29-00:00:
Sidebar inbox status resolution (`resolveSidebarV2Status`,
`formatWorkingDurationLabel`, `resolveWorkingStartedAt`, `hasUnseenCompletion`,
`firstValidTimestampMs`).

Three-hue discipline is preserved: color is reserved for "act now" (amber),
"in motion" (sky), and "broken" (red). Everything else recedes to neutral —
a settled-looking row must never compete for attention. The resolver returns
structured data only (kind/label/hue/recede/pulse); CSS classes and JSX stay in
the V2 render tree.
*/

export type SidebarV2StatusHue = 'amber' | 'indigo' | 'neutral' | 'red' | 'sky';

export type SidebarV2StatusKind = 'approval' | 'done' | 'failed' | 'idle' | 'input' | 'working';

export type SidebarV2Status = {
  hue: SidebarV2StatusHue;
  kind: SidebarV2StatusKind;
  label: string;
  /** True for the resting state: the row should draw no status chrome. */
  recede: boolean;
  pulse: boolean;
};

/** How long an idle session keeps reading "Done" after its last activity. */
export const SIDEBAR_V2_DONE_WINDOW_MS = 30 * 60 * 1_000;

const MINUTE_MS = 60 * 1_000;
const HOUR_MS = 60 * MINUTE_MS;
const DAY_MS = 24 * HOUR_MS;
const WEEK_MS = 7 * DAY_MS;

/** NaN-safe Date.parse for comparators: a malformed timestamp must not poison
    the whole ordering, so it sinks to the epoch instead. */
export function parseTimestampMs(isoDate: string | null | undefined): number {
  if (isoDate == null) {
    return 0;
  }
  const parsed = Date.parse(isoDate);
  return Number.isNaN(parsed) ? 0 : parsed;
}

/** First VALID timestamp wins: `a ?? b` falls through on null, but a present-
    yet-malformed string must also fall through to the next candidate rather
    than sink the row to the epoch. */
export function firstValidTimestampMs(...candidates: readonly (string | null | undefined)[]): number | null {
  for (const candidate of candidates) {
    if (candidate == null) {
      continue;
    }
    const parsed = Date.parse(candidate);
    if (!Number.isNaN(parsed)) {
      return parsed;
    }
  }
  return null;
}

/** String twin of `firstValidTimestampMs` for callers that need the ISO string
    (display labels, tick anchors) rather than epoch ms. */
export function firstValidTimestamp(...candidates: readonly (string | null | undefined)[]): string | null {
  for (const candidate of candidates) {
    if (candidate == null) {
      continue;
    }
    if (!Number.isNaN(Date.parse(candidate))) {
      return candidate;
    }
  }
  return null;
}

export type SidebarV2StatusSession = Pick<
  SidebarV2Session,
  'activity' | 'activityLabel' | 'attentionKind' | 'lastInteractionAt' | 'lifecycleState' | 'workingStartedAt'
>;

export type ResolveSidebarV2StatusOptions = {
  /** Overrides `SIDEBAR_V2_DONE_WINDOW_MS`. */
  doneWindowMs?: number;
  /**
   * When the user last opened/focused this session. A visit after the last
   * activity retires the "Done" label back to the receding relative-time
   * state, preserving unread-completion behavior.
   */
  lastVisitedAtMs?: number | null;
  nowMs: number;
};

/**
 * The timestamp a working session's elapsed label counts from: the current
 * working stint's start, falling back to the meaningful-activity clock when
 * gxserver has not published a stint (older daemons, native-host sessions).
 */
export function resolveSidebarV2WorkingStartedAtMs(
  session: Pick<SidebarV2Session, 'lastInteractionAt' | 'workingStartedAt'>
): number | null {
  return firstValidTimestampMs(session.workingStartedAt, session.lastInteractionAt);
}

export function formatWorkingDurationLabel(elapsedMs: number): string {
  const seconds = Number.isFinite(elapsedMs) ? Math.max(0, Math.floor(elapsedMs / 1_000)) : 0;
  if (seconds < 60) {
    return `${seconds}s`;
  }
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) {
    return `${minutes}m`;
  }
  return `${Math.floor(minutes / 60)}h ${minutes % 60}m`;
}

/** Compact "how long ago" label for resting rows: "now", "5m", "2h", "3d",
    "2w". Deliberately unit-only so it never widens the row. */
export function formatSidebarV2RelativeTime(elapsedMs: number): string {
  if (!Number.isFinite(elapsedMs) || elapsedMs < MINUTE_MS) {
    return 'now';
  }
  if (elapsedMs < HOUR_MS) {
    return `${Math.floor(elapsedMs / MINUTE_MS)}m`;
  }
  if (elapsedMs < DAY_MS) {
    return `${Math.floor(elapsedMs / HOUR_MS)}h`;
  }
  if (elapsedMs < WEEK_MS) {
    return `${Math.floor(elapsedMs / DAY_MS)}d`;
  }
  return `${Math.floor(elapsedMs / WEEK_MS)}w`;
}

/**
 * A completion the user has not looked at yet.
 * `hasUnseenCompletion`: a session that was NEVER visited does not count as
 * unseen, because the user was presumably watching it happen. Feeds the unread
 * marker, not the status label.
 */
export function hasUnseenCompletion(
  session: Pick<SidebarV2Session, 'activity' | 'lastInteractionAt'>,
  options: { lastVisitedAtMs?: number | null }
): boolean {
  if (session.activity !== 'idle') {
    return false;
  }
  const completedAtMs = firstValidTimestampMs(session.lastInteractionAt);
  if (completedAtMs === null) {
    return false;
  }
  if (options.lastVisitedAtMs == null) {
    return false;
  }
  if (!Number.isFinite(options.lastVisitedAtMs)) {
    return true;
  }
  return completedAtMs > options.lastVisitedAtMs;
}

/**
 * Resolves the Ghostex sidebar inbox status.
 *
 * Precedence is blocked-on-user first, then in-motion,
 * then broken, then the resting states. "Done" is Ghostex-specific — an idle
 * session whose last activity is still recent and unvisited — and everything
 * older recedes to a relative-time label with no hue.
 */
export function resolveSidebarV2Status(
  session: SidebarV2StatusSession,
  options: ResolveSidebarV2StatusOptions
): SidebarV2Status {
  const activityLabel = session.activityLabel?.trim();

  if (session.activity === 'attention') {
    /*
     * Ghostex cannot tell an approval prompt from a plain input prompt: gxserver
     * publishes one `attention` activity and no `attentionKind`, and attention
     * always means "act now". So the DEFAULT attention hue is amber, and the
     * quieter indigo half of the split only applies once a host actually
     * publishes `attentionKind: "input"` (P2+ can start doing that without any
     * further change here).
     */
    const kind: SidebarV2StatusKind = session.attentionKind === 'approval' ? 'approval' : 'input';
    return {
      hue: session.attentionKind === 'input' ? 'indigo' : 'amber',
      kind,
      label: activityLabel || (kind === 'approval' ? 'Approval' : 'Input'),
      pulse: false,
      recede: false,
    };
  }

  if (session.activity === 'working') {
    const startedAtMs = resolveSidebarV2WorkingStartedAtMs(session);
    const label =
      startedAtMs === null ? 'Working' : `Working ${formatWorkingDurationLabel(options.nowMs - startedAtMs)}`;
    return { hue: 'sky', kind: 'working', label, pulse: true, recede: false };
  }

  if (session.lifecycleState === 'error') {
    return {
      hue: 'red',
      kind: 'failed',
      label: activityLabel || 'Failed',
      pulse: false,
      recede: false,
    };
  }

  const lastActivityMs = firstValidTimestampMs(session.lastInteractionAt);
  if (lastActivityMs === null) {
    return { hue: 'neutral', kind: 'idle', label: '', pulse: false, recede: true };
  }

  const elapsedMs = options.nowMs - lastActivityMs;
  const doneWindowMs = options.doneWindowMs ?? SIDEBAR_V2_DONE_WINDOW_MS;
  const isVisitedSinceActivity =
    options.lastVisitedAtMs != null &&
    Number.isFinite(options.lastVisitedAtMs) &&
    options.lastVisitedAtMs >= lastActivityMs;
  if (elapsedMs >= 0 && elapsedMs < doneWindowMs && !isVisitedSinceActivity) {
    return { hue: 'neutral', kind: 'done', label: 'Done', pulse: false, recede: false };
  }

  return {
    hue: 'neutral',
    kind: 'idle',
    label: formatSidebarV2RelativeTime(Math.max(0, elapsedMs)),
    pulse: false,
    recede: true,
  };
}

const SIDEBAR_V2_STATUS_PRIORITY: Record<SidebarV2StatusKind, number> = {
  approval: 5,
  input: 4,
  working: 3,
  failed: 2,
  done: 1,
  idle: 0,
};

export function sidebarV2StatusPriority(kind: SidebarV2StatusKind): number {
  return SIDEBAR_V2_STATUS_PRIORITY[kind];
}

/**
 * Roll-up indicator for a collapsed project group: the loudest status among its
 * sessions, or null when everything is resting.
 */
export function resolveSidebarV2GroupStatus(statuses: readonly SidebarV2Status[]): SidebarV2Status | null {
  let loudest: SidebarV2Status | null = null;
  for (const status of statuses) {
    if (status.recede) {
      continue;
    }
    if (loudest === null || sidebarV2StatusPriority(status.kind) > sidebarV2StatusPriority(loudest.kind)) {
      loudest = status;
    }
  }
  return loudest;
}
