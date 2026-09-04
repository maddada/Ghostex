import type {
  GxserverDomainLifecycleState,
  GxserverPresentationAttentionState,
  GxserverPresentationProviderSessionState,
  GxserverPresentationSessionActions,
  GxserverPresentationSessionActivity,
  GxserverPresentationSessionGitStatus,
  GxserverPresentationSettledOverride,
  GxserverProjectId,
  GxserverSessionId,
  GxserverSessionKind,
  GxserverSessionSurface,
  GxserverSessionTag,
  GxserverSessionTitleSource,
  GxserverSwitchableSessionAgent,
  GxserverTitleObservationState,
  GxserverZmxSessionName,
} from '../gxserver-protocol';

export interface GxserverPresentationSession {
  actions: GxserverPresentationSessionActions;
  activity: GxserverPresentationSessionActivity;
  agentIcon?: string;
  agentId?: string;
  agentName?: string;
  agentSessionId?: string;
  agentSessionPath?: string;
  /**
   * CDXC:SessionFork 2026-08-28:
   * Fork lineage derived by the daemon from its own registry, never from the
   * transcript files: the parent session this conversation branched off, how
   * many VISIBLE branches share its earlier history (present only at two or
   * more), and who those branches are. A daemon that predates fork awareness
   * publishes none of the three.
   */
  forkedFromSessionId?: GxserverSessionId;
  forkBranchCount?: number;
  forkFamilySessionIds?: GxserverSessionId[];
  /** Stable Action identity used to reuse an existing command-surface session. */
  commandId?: string;
  attention?: GxserverPresentationAttentionState;
  createdAt: string;
  cwd?: string;
  /** Daemon-owned Delayed Send state; absent when no send is armed. */
  delayedSendDeadlineAt?: string;
  delayedSendRemainingLabel?: string;
  delayedSendRemainingMs?: number;
  /**
   * CDXC:Git 2026-07-29:
   * Branch, diff stats, and change-request state for this session's cwd.
   * Absent whenever gxserver has nothing to publish: no probe yet, not a git
   * work tree, or a daemon that predates the probe entirely.
   */
  gitStatus?: GxserverPresentationSessionGitStatus;
  groupId: string;
  /**
   * CDXC:SessionSleep 2026-08-22:
   * Whether this session has EVER entered working or attention, i.e. whether
   * anybody has prompted it. `lastActiveAt` below cannot answer that: the
   * projection falls back to `createdAt` so labels and sorting always have a
   * timestamp, which makes a never-prompted session read as "idle since it was
   * created". Auto Sleep needs the difference, because an agent terminal with
   * no conversation yet cannot be resumed after its provider is killed.
   * Absent from daemons that predate this field; treat that as not-yet-active.
   */
  hasEverBeenActive?: boolean;
  /**
   * CDXC:Drafts 2026-08-28:
   * This session was created from the sidebar and has not received its first
   * user prompt yet. Present ONLY while the session is a draft (never `false`),
   * which is also what a daemon that predates drafts publishes. Sidebars render
   * a draft inline in its normal position with a pencil glyph instead of the
   * agent logo and a dimmed title; `displayTitle` carries the first line of the
   * user's unsent composer text once any exists (`titleSource: 'draft'`).
   */
  isDraft?: true;
  isFavorite: boolean;
  isGeneratingFirstPromptTitle: boolean;
  isParked?: boolean;
  isPinned: boolean;
  kind: GxserverSessionKind;
  lastActiveAt?: string;
  lifecycleState: GxserverDomainLifecycleState;
  /**
   * CDXC:AgentScreenDetection 2026-07-29-12:00:
   * `meaningfulActivityAt` is the recency clients sort by: working blips
   * shorter than gxserver's meaningful threshold never advance it, while a
   * meaningfully working session's value advances live with each snapshot.
   * `workingStartedAt` is published while the session is effectively working
   * so sorters can tell whether the current stint has qualified yet.
   * `lastActiveAt` stays raw (any working/attention entry) for auto-sleep and
   * Last Active labels. Both fields are optional for older remote daemons.
   */
  meaningfulActivityAt?: string;
  workingStartedAt?: string;
  providerSessionState: GxserverPresentationProviderSessionState;
  projectId: GxserverProjectId;
  /**
   * CDXC:SessionChat 2026-08-21-b:
   * How many Ghostex-owned chat prompts are held for this session, so the
   * sidebar can badge the agent icon without subscribing to every session's
   * chat. EVERY row counts, `failed` included: a queue stalled behind a failed
   * row is precisely the state that needs the user, and leaving those rows out
   * made a dead queue look identical to no queue. The key is ABSENT at zero —
   * never `0` — which is also what a daemon that predates the queue publishes,
   * so both mean the same thing to a client: no badge.
   */
  queuedPromptCount?: number;
  /**
   * CDXC:SessionChat 2026-08-21-b:
   * How many of those rows are `failed` (delivery attempted, held for the user
   * to retry or delete). Non-zero turns the badge red instead of yellow, and
   * `queuedPromptCount - queuedPromptFailedCount` is what still counts as work
   * the agent is going to receive. ABSENT means none failed.
   */
  queuedPromptFailedCount?: number;
  /**
   * CDXC:Drafts 2026-09-04 DECISION:
   * User: a white dot on the agent icon marks a session whose chat composer
   * holds unsent text (the chat box only, never the terminal's own input
   * line). True when gxserver's synced draft for this session is non-blank;
   * ABSENT otherwise, never `false`, which is also what a daemon that predates
   * the flag publishes.
   */
  hasComposerDraft?: boolean;
  sessionId: GxserverSessionId;
  /**
   * CDXC:SessionNotes 2026-08-24:
   * The full note text the user filed against this session's provider
   * conversation (`agentSessionId`), so sidebar rows can show the note in their
   * tooltip and mark the row without a per-session read. The key is ABSENT when
   * there is no note — never an empty string — which is also what a daemon that
   * predates session notes publishes.
   */
  sessionNote?: string;
  /**
   * Prompts saved from this provider conversation, including legacy rows that
   * still carry only the raw Ghostex session id. Absent at zero and on daemons
   * that predate the terminal action-bar badge.
   */
  stashedPromptCount?: number;
  /**
   * CDXC:AgentProviders 2026-09-03:
   * The same-family agent configurations (accounts) this prompted session can
   * be resumed under, resolved by the owning daemon. ABSENT when there is
   * nothing to switch to and on daemons that predate the feature.
   */
  switchableAgents?: readonly GxserverSwitchableSessionAgent[];
  sessionPersistenceProvider?: 'tmux' | 'zmx' | 'zellij';
  sessionTag?: GxserverSessionTag;
  sendWhenAllProjectSessionsStopActive?: boolean;
  sendWhenAgentStopsActive?: boolean;
  /**
   * CDXC:StateSync 2026-07-29-00:00:
   * Server-owned Sidebar V2 inbox lifecycle. `settledOverride` is the explicit
   * user pin — "settled" forces the settled shelf, "active" pins the session
   * into the inbox and suppresses auto-settle — and gxserver clears it once
   * real activity outruns it. `settledAt` is stamped only by an explicit
   * settle; an inactivity auto-settle deliberately leaves it absent so the
   * settled shelf sorts the row by when its work ended. `snoozedUntil` is the
   * wake time and `snoozedAt` the moment the snooze was set; the wake itself is
   * derived from `snoozedUntil` (no event fires when it passes), and a snoozed
   * session that raises its hand stays snoozed here while clients surface it.
   * All four are absent when the session has no lifecycle state, which is also
   * what an older remote daemon publishes.
   */
  settledAt?: string;
  settledOverride?: GxserverPresentationSettledOverride;
  sidebarOrder?: number;
  snoozedAt?: string;
  snoozedUntil?: string;
  sortKey: string;
  subtitle?: string;
  surface: GxserverSessionSurface;
  displayTitle?: string;
  displayTitleTooltip?: string;
  isPrimaryTitleTerminalTitle: boolean;
  isTemporaryTitle: boolean;
  primaryTitle?: string;
  terminalTitle?: string;
  title: string;
  titleObservation?: GxserverTitleObservationState;
  titleSource: GxserverSessionTitleSource;
  trustedResumeTitle?: string;
  tooltip?: string;
  updatedAt: string;
  visibleInSidebarByDefault: boolean;
  zmxName: GxserverZmxSessionName;
}
