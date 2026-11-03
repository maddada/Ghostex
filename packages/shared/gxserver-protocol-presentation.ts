import type { GxserverPresentationCloudBox } from './gxserver-protocol-cloud-boxes';
import type { DelayedSendAgentReference } from './delayed-send';
import type { GxserverProjectId, GxserverSessionId, GxserverZmxSessionName } from './gxserver-protocol-core';
import type {
  GxserverSessionKind,
  GxserverSessionSurface,
  GxserverSessionTag,
  GxserverSessionTagFilter,
  GxserverDomainLifecycleState,
  GxserverProjectDomainState,
} from './gxserver-protocol-domain';
import type { GxserverSwitchableSessionAgent } from './gxserver-protocol-sessions';

/**
 * CDXC:Drafts 2026-08-28:
 * `'draft'` is projection-only and never durable: gxserver publishes it for a
 * draft session whose synced composer content supplies the row's display title,
 * so a client can tell "the user's unsent text" apart from a real session title.
 */
export type GxserverSessionTitleSource =
  'browser-auto' | 'draft' | 'generated' | 'placeholder' | 'terminal-auto' | 'user';

export interface GxserverSessionTitleProjection {
  displayTitle?: string;
  displayTitleTooltip?: string;
  isPrimaryTitleTerminalTitle: boolean;
  isTemporaryTitle: boolean;
  primaryTitle?: string;
  terminalTitle?: string;
  title: string;
  titleSource: GxserverSessionTitleSource;
  trustedResumeTitle?: string;
}

export type GxserverPresentationRevision = number & {
  readonly __gxserverPresentationRevision: unique symbol;
};
export type GxserverPresentationSessionActivity = 'attention' | 'idle' | 'working';
/*
CDXC:SessionStatus 2026-06-07-00:30:
zmx title observation health is presentation metadata for working-status detection. Publish only coarse watcher states and timestamps so clients can avoid treating unavailable detection as idle without exposing terminal titles, commands, paths, or user content.
*/
export type GxserverTitleObservationStatus = 'active' | 'failed' | 'retrying' | 'starting';

export interface GxserverTitleObservationState {
  failureCount?: number;
  lastFailedAt?: string;
  lastObservedAt?: string;
  lastStartedAt?: string;
  nextRetryAt?: string;
  status: GxserverTitleObservationStatus;
}

export interface GxserverPresentationAttentionState {
  acknowledged: boolean;
  enteredAt?: string;
  eventId?: string;
}

export interface GxserverPresentationSessionActions {
  acknowledgeAttention: boolean;
  attach: boolean;
  focus: boolean;
  kill: boolean;
  readText: boolean;
  sendMessage: boolean;
  sendText: boolean;
  sleep: boolean;
  wake: boolean;
}

/*
CDXC:StateSync 2026-06-15-17:32:
Presentation clients need provider liveness as a first-class field because domain lifecycle and native pane lifecycle are separate resources. A row can remain visible while its zmx provider is missing or persistence is disabled, and clients must not infer provider existence from `running` alone.
*/
export type GxserverPresentationProviderSessionState = 'exists' | 'missing' | 'persistence-disabled' | 'unknown';

export interface GxserverPresentationProject {
  createdAt: string;
  /*
  CDXC:Git 2026-06-24-18:22:
  Remote Sidebar Git preferences need current per-project settings in the same trusted presentation row that supplies the project id. Presentation exposes only sanitized Git preference keys so GPUI can preserve existing values while updating one preference without fetching path-bearing domain project lists through the remote bridge.
  */
  gitConfig?: Record<string, unknown>;
  /*
  CDXC:StateSync 2026-07-29:
  The project's `origin` remote URL, probed server-side with TTL caching like
  the worktree topology probe. Sidebar V2 normalizes it client-side into a
  repository identity so the SAME repo checked out on this Mac and on a remote
  machine reads as ONE logical project.

  Three distinct states, and clients must not collapse them:
  - ABSENT: not probed yet, or the project is not a git work tree at all.
  - `null`: probed, and the repository has no `origin` remote.
  - a string: the raw remote URL exactly as git reports it. Normalization
    (scp-style vs https, `.git` suffix, case) is the CLIENT's job so one
    machine's git version cannot change how another machine's projects group.

  Absent and `null` behave identically for grouping — a project with no usable
  remote never merges with anything — but they are kept apart on the wire so a
  daemon that has not finished probing is distinguishable from a non-git folder.
  */
  gitRemoteOriginUrl?: string | null;
  /*
  CDXC:StateSync 2026-07-29 (P5 fix round):
  The repository root the project sits in (`git rev-parse --show-toplevel`),
  resolved in the SAME server-side probe and cache entry as
  `gitRemoteOriginUrl` above, and — like the URL — keyed on a worktree family's
  ROOT project, so a registered worktree reports its parent checkout's root.

  Only TWO states here: a string, or ABSENT (not a git work tree, not probed
  yet, or a repository whose root git would not report). There is deliberately
  no `null`, because a missing root means only "cannot tell where in the
  repository this project sits" — a fact with no separate wire meaning.

  It exists because Sidebar V2's "Repository + path" grouping mode measures a
  project's path against this root: two sub-projects of one monorepo differ
  only in their path BELOW the root, and without it that mode has nothing to
  measure and degrades to plain repository merging.
  */
  gitRepositoryRootPath?: string;
  /** The Hermes profile this project is the bot of (server/src/bot_projects.rs); absent otherwise. */
  botProfile?: string;
  /** Whether that bot's Hermes gateway runs; absent for every other project. */
  botGatewayRunning?: boolean;
  /** How many runs that bot's cron jobs delivered since local midnight; absent for every other project. */
  botRunsToday?: number;
  /** Issues Workflow (formerly Work mode) is on for this project: a Linear key applies, `gh` is signed in, or its workspace has a ticket command (server/src/work_mode/project.rs); present only when true. */
  workMode?: true;
  /** Issues Workflow is on and a Linear key applies to this project; present only when true. */
  workLinear?: true;
  /** Issues Workflow is on but `gh` is not signed in, so its GitHub parts are hidden; present only when true. */
  workGithubOff?: true;
  /** The primary tracker of the project's workspace (server/src/work_mode/tracker.rs); present only with work mode on. */
  workTracker?: 'linear' | 'github';
  /** The workspace this project belongs to (server/src/workspaces/); absent = the default workspace. */
  workspaceId?: string;
  /** The project shows in every workspace (the Ghostex config folder's project, home of the Help chats); present only when true. */
  everyWorkspace?: true;
  /** The ticket command of the project's workspace (server/src/work_mode/command/); absent without one, in a team workspace, and from an older daemon. */
  workSource?: {
    /** The id its `info` pinned, which its tickets' keys and link kinds start with. */
    id: string;
    /** Its name for the workspace; its icon is not on the row (server/src/work_mode/project.rs). */
    name: string;
    /** Whether it is the workspace's primary tracker; false until a command can be picked as one. */
    isTracker: boolean;
  };
  /*
  CDXC:Icons 2026-07-29 (discovered icons):
  The icon the PROJECT ITSELF ships, discovered server-side inside the checkout
  and shipped as a `data:` URL. Discovery checks well-known favicon and app-icon
  locations, then an icon declared by an HTML entry
  point's `<link rel="icon">`. Keyed on the worktree FAMILY ROOT like
  `gitRemoteOriginUrl`, so a worktree shows its parent checkout's icon.

  Two states only: a data URL string, or ABSENT (not probed yet, nothing
  discoverable, or an older daemon that does not publish it).

  This is NOT the icon a user attached to the project by hand — that one is
  host-owned and reaches the sidebar through the project overlay's `icon` /
  `iconDataUrl`. They stay SEPARATE wire fields so the client can rank them: a
  user-uploaded image outranks this, and this outranks a typed Tabler glyph.
  Merging them into one field would make that ordering impossible to express.
  */
  discoveredIconDataUrl?: string;
  groupIds: readonly string[];
  isFavorite: boolean;
  isPinned: boolean;
  path?: string;
  pathState?: 'available' | 'missing' | 'notDirectory' | 'unavailable';
  projectId: GxserverProjectId;
  sortKey: string;
  title: string;
  updatedAt: string;
  worktree?: Record<string, unknown>;
}

export interface GxserverPresentationGroup {
  groupId: string;
  projectId: GxserverProjectId;
  sessionIds: readonly GxserverSessionId[];
  sortKey: string;
  title: string;
}

export type GxserverPresentationSettledOverride = 'active' | 'settled';

/**
 * CDXC:Git 2026-07-29:
 * The state of the change request that owns a session's branch. `draft` is a
 * separate value rather than a flag on `open` because the sidebar paints it in
 * a different (deliberately quiet) hue: a draft is work in progress, not a
 * review waiting on anyone.
 */
export type GxserverPresentationSessionPrState = 'closed' | 'draft' | 'merged' | 'open';

/*
CDXC:Git 2026-07-29:
Per-session git/PR state, probed SERVER-side from the session's own cwd (a
worktree session's cwd is its worktree, so the session is the unit of git
truth). gxserver probes per unique cwd, caches (~60s git, ~5min PR), throttles,
and never blocks a snapshot on a git command.

Field rules the emitter and every client must agree on:
- `branch` is null for a detached HEAD or a cwd that is not a work tree. The
  whole object is simply ABSENT for a session gxserver could not probe (or a
  daemon that predates this feature) — absence is not an error state.
- `additions`/`deletions` are the session worktree measured against the
  merge-base with the repo's default branch, and include both committed-on-
  branch and uncommitted work. They are 0 when there is nothing to report,
  never negative, and the sidebar hides the pair entirely at 0/0.
- The `pr*` fields are present only when `gh` is installed AND authenticated
  AND a change request exists for the branch. No `gh` means no PR fields, not
  an error and not a stale badge.
- `updatedAt` stamps the probe, not the repository, so a client can tell a
  fresh answer from a cached one without inventing its own clock.
*/
export interface GxserverPresentationSessionGitStatus {
  additions: number;
  branch: string | null;
  deletions: number;
  prNumber?: number;
  prState?: GxserverPresentationSessionPrState;
  prUrl?: string;
  updatedAt: string;
}

/*
CDXC:StateSync 2026-07-29-00:00:
Machine-scoped capability flags. A GPUI sidebar merges snapshots from several
gxservers; an older daemon simply omits this object, and Sidebar V2 then hides
settle/snooze affordances and classifies nothing as settled for that machine
instead of inventing lifecycle out of derived data.

CDXC:Git 2026-07-29:
`sessionGitStatus` is optional on top of that, because a daemon can be new
enough to publish this block for settle/snooze and still predate the git probe.
A missing flag means "this machine has no git/PR data to give", and V2 renders
its cards exactly as it does for a session with no `gitStatus` at all.
*/
export interface GxserverPresentationCapabilities {
  sessionGitStatus?: boolean;
  sessionSettlement: boolean;
  sessionSnooze: boolean;
  /**
   * CDXC:Spaces 2026-08-27:
   * `/api/readSidebarSpaces` + `/api/updateSidebarSpaces` are served by this
   * daemon. Absent means the machine has no Spaces at all, and its sidebar
   * section renders its full unfiltered project list — no Space row, no Spaces
   * context submenu, not even the built-in Other view.
   */
  spaces?: boolean;
  /** `sidebarWorkspaces`, `workspaceId` on projects and Spaces, and the workspace routes. */
  workspaces?: boolean;
  /**
   * CDXC:Worktrees 2026-07-29:
   * `/api/createWorktreeSession` + `/api/removeSessionWorktree` are served by
   * this daemon. Optional for the same reason as `sessionGitStatus`: a machine
   * can be new enough for settle/snooze and still predate the worktree flow.
   * Absent means V2's split "+" collapses to the plain instant-session button
   * and the worktree affordances do not render at all.
   */
  worktreeSessions?: boolean;
}

/** What a session in a work-mode project is linked to (`PresentationSessionWork` in gx-protocol). */
export interface GxserverPresentationSessionWork {
  /** The checkout's branch; absent on the default branch or a detached HEAD. */
  branch?: string;
  pullRequest?: {
    number: number;
    state: GxserverPresentationSessionPrState;
    url?: string;
    checks?: 'passing' | 'failing' | 'pending';
  };
  /**
   * The tickets of `linearIssues` and `githubIssues` (from the branch, or set by hand) as one list
   * in the session links' shape. It is not the Links panel's list: removals made there are not
   * applied and stored links are not added.
   */
  tickets?: GxserverPresentationSessionLink[];
  /** Kept while the phone app still reads it (CDXC:WorkMode 2026-10-27 on `PresentationSessionWork.linear_issues`). */
  linearIssues?: Array<{
    /** `SPX-1245`. */
    identifier: string;
    title?: string;
    /** `triage`, `backlog`, `unstarted`, `started`, `completed` or `canceled`. */
    stateType?: string;
    stateName?: string;
    url?: string;
  }>;
  githubIssues?: Array<{ number: number; title?: string; state?: 'open' | 'closed'; url?: string }>;
  /** A Linear project is a release the team works on, never a repo. */
  linearProject?: { name: string; url?: string };
  /** A GitHub Project (Projects v2), instead of `linearProject` when the workspace's primary tracker is GitHub. */
  githubProject?: { owner: string; number: number; title?: string; url?: string; status?: string };
  /** Some link was set by hand, so "Back to automatic" has something to undo. */
  handSet?: true;
  /** The linked PR is merged and its Clean up / Keep offer is unanswered (server/src/work_mode/cleanup.rs). */
  offerCleanup?: true;
}

/** One PR, GitHub issue or Linear ticket a session is linked to (`PresentationSessionLink` in gx-protocol). */
export interface GxserverPresentationSessionLink {
  /** `pr:owner/repo#318`, `issue:owner/repo#305` or `linear:GHX-142`. */
  key: string;
  /** `pullRequest`, `issue` or `linear`; a kind a client does not know draws a fallback icon. */
  kind: string;
  /** `#318`, `repo#12` (another repo) or `GHX-142`. */
  label: string;
  title?: string;
  /** A PR's `open|draft|merged|closed`, an issue's `open|closed`, a Linear ticket's state type. */
  state?: string;
  stateName?: string;
  /** A ticket's Work page group: `backlog`, `todo`, `progress`, `review`, `done`, `canceled` or `closed`. */
  stateGroup?: string;
  checks?: 'passing' | 'failing' | 'pending';
  review?: 'reviewRequired' | 'approved' | 'changesRequested';
  url?: string;
  source: string;
  /** "From branch", "Agent opened", "Closed by #318"… */
  sourceLabel: string;
  finished?: true;
  /** A person pinned it: it leads every list and always shows in the sidebar. */
  pinned?: true;
  /** Never read yet, and a read is on its way: no title or state to show. */
  loading?: true;
  /** A PR's branch (`headRefName`), for its hover card. */
  branch?: string;
}

/** Every link of a session, open work first, and the PR and issue URLs its chat only mentioned (`PresentationSessionLinks`). */
export interface GxserverPresentationSessionLinks {
  items: GxserverPresentationSessionLink[];
  suggested?: GxserverPresentationSuggestedLink[];
  /** Links a person removed, newest first, which the lists offer to restore. */
  removed?: GxserverPresentationSuggestedLink[];
}

/** A suggestion or a removed link (`PresentationSuggestedLink` in gx-protocol), drawn like a linked one. */
export interface GxserverPresentationSuggestedLink {
  key: string;
  kind: 'pullRequest' | 'issue' | 'linear';
  label: string;
  url?: string;
  title?: string;
  state?: string;
  stateName?: string;
  /** "Mentioned in the chat" for a suggestion, where a removed link came from (or "Removed"). */
  sourceLabel?: string;
  finished?: true;
  loading?: true;
  branch?: string;
}

/** A Claude cloud session (`PresentationClaudeCloud` in gx-protocol). */
export interface GxserverPresentationClaudeCloud {
  /** `draft` (no task yet), `starting`, `running`, `failed` or `moved` (brought to this computer). */
  state: string;
  /** `https://claude.ai/code/session_…`, once the session exists. */
  url?: string;
  /** The branch the session pushes to. */
  branch?: string;
  /** The latest status line, e.g. "Pull request #12 opened". */
  statusText?: string;
  prUrl?: string;
  /** Why the start failed. */
  error?: string;
  /** `live` once the terminal shows the cloud session itself, `unavailable` while Claude does not allow attaching. */
  attach?: string;
  /** The saved Claude account the session runs on. */
  accountId?: string;
  accountName?: string;
  /** The email the account's claude.ai page must be signed in as. */
  accountEmail?: string;
  /** "<account> hit its 5h limit · resumes 4:10 pm" while the account is out of usage. */
  limitNotice?: string;
}

/** The agentbox sandbox a session's agent runs in (`PresentationAgentbox` in gx-protocol). */
export interface GxserverPresentationAgentbox {
  /** `docker`, `hetzner`, `vercel`, `daytona`, `e2b`, `digitalocean`, or `docker:<host alias>`. */
  provider: string;
  /** The agentbox box name every `agentbox … <box>` command takes. */
  boxName: string;
  /** Short label for the provider, e.g. "Docker" or "Hetzner". */
  providerLabel: string;
  /** A draft whose chat Run on row picked this box; its first message creates the box. */
  pending?: true;
}

/** One OSC 7501 record of a session (`ProgramStatusRecord` in gx-protocol). */
export interface GxserverProgramStatusRecord {
  /** `a/b/c` path; empty for the root record. */
  id: string;
  state: 'idle' | 'working' | 'done' | 'blocked' | 'error';
  kind?: 'permission' | 'question' | 'auth';
  progress?: number;
  app?: string;
  title?: string;
  /** One line from the program; shown on hover, never as markup. */
  msg?: string;
  updatedAtMs: number;
}

/** What the session's own program reported over OSC 7501 (`ProgramStatus` in gx-protocol). */
export interface GxserverProgramStatus {
  /** The records drive `activity` under the Session status setting. */
  governs: boolean;
  state: 'idle' | 'working' | 'done' | 'blocked' | 'error';
  kind?: 'permission' | 'question' | 'auth';
  progress?: number;
  app?: string;
  message?: string;
  records: GxserverProgramStatusRecord[];
}

export interface GxserverPresentationSession {
  accountId?: string;
  accountName?: string;
  accountSlot?: string;
  /** Present only when the agent runs inside an agentbox sandbox instead of on this machine. */
  agentbox?: GxserverPresentationAgentbox;
  /** Present only for a Claude cloud session: the agent runs on claude.ai's computers. */
  claudeCloud?: GxserverPresentationClaudeCloud;
  /** Present only for a Cloud Box session, or a draft whose tray picked Cloud Box (☁ badge). */
  cloudBox?: GxserverPresentationCloudBox;
  /** The session's OSC 7501 program status; absent when it has no records to show. */
  programStatus?: GxserverProgramStatus;
  actions: GxserverPresentationSessionActions;
  activity: GxserverPresentationSessionActivity;
  /** Unanswered async questions, independent of working/completion activity. */
  pendingQuestionCount?: number;
  /** Stuck on a permission prompt or approval card only the user can grant (the red dot); absent when not. */
  awaitingApproval?: boolean;
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
  /** `coordinator` or `thread`; absent for every other session (gxserver `coordinators/presentation.rs`). */
  coordinatorRole?: 'coordinator' | 'thread';
  coordinatorProjectId?: GxserverProjectId;
  coordinatorSessionId?: GxserverSessionId;
  coordinatorThreadState?: 'waiting' | 'working' | 'finished' | 'sleeping' | 'closed' | 'done';
  /** The main session this session is a side chat of; side chats are left out of the session lists. */
  sideChatParentSessionId?: string;
  /** Stable Action identity used to reuse an existing command-surface session. */
  commandId?: string;
  attention?: GxserverPresentationAttentionState;
  createdAt: string;
  cwd?: string;
  /** Daemon-owned Delayed Send state; absent when no send is armed. */
  delayedSendDeadlineAt?: string;
  /** Close After Done is armed (gxserver owns the timer); `closeAfterDoneDeadlineAt` while its countdown runs. */
  closeAfterDone?: boolean;
  closeAfterDoneDeadlineAt?: string;
  delayedSendRemainingLabel?: string;
  delayedSendRemainingMs?: number;
  /**
   * CDXC:Git 2026-07-29:
   * Branch, diff stats, and change-request state for this session's cwd.
   * Absent whenever gxserver has nothing to publish: no probe yet, not a git
   * work tree, or a daemon that predates the probe entirely.
   */
  gitStatus?: GxserverPresentationSessionGitStatus;
  /** Present only for a session of a project with work mode on. */
  work?: GxserverPresentationSessionWork;
  /** Every session's PR, issue and Linear links; absent when it has none and nothing to suggest. */
  links?: GxserverPresentationSessionLinks;
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
  /** Present while a background shell or monitor the agent started is still running after its turn. */
  backgroundWorkDetectedAt?: string;
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
   * CDXC:SessionTitles 2026-10-12 SEE-ALSO: `pending_session_rename_title` in
   * server/src/presentation/session_attributes.rs. The name of the user's last
   * rename the agent never confirmed within 8 seconds (the title shakes when it
   * appears and the next Rename opens with it), and when gxserver gave up on it.
   * Absent once a rename is confirmed or another is requested.
   */
  renameFailedAt?: string;
  renameFailedTitle?: string;
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
  sendWhenSpecificAgentFinishes?: DelayedSendAgentReference;
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

/*
CDXC:Projects 2026-07-18-00:00:
Colored "Group N" project collections are server-owned structure shared by the
desktop sidebar and React Native Android. Expansion is client-local UI state.
The wire state is fully normalized by
gxserver: `order` is the authoritative collection ordering, `collections` is
keyed by collectionId, a project id appears in at most one collection, and
collections with no project ids are dropped. Clients write-through-sync the
whole state via /api/updateSidebarProjectCollections and read it back from the
same endpoint, the presentation snapshot, or the mobile session summary.
*/
export interface GxserverSidebarProjectCollection {
  collectionId: string;
  color: string;
  projectIds: readonly string[];
  title: string;
}

export interface GxserverSidebarProjectCollectionsState {
  collections: Readonly<Record<string, GxserverSidebarProjectCollection>>;
  nextCollectionNumber: number;
  order: readonly string[];
}

/*
CDXC:Spaces 2026-08-27:
A Space is a server-owned saved sidebar filter: a name, an icon id, a color, a
manual position, and the sidebar members it shows. Members are sidebar project
collections ("groups") and ungrouped projects, and a member belongs to at most
one Space. gxserver owns the document so every client on that daemon
shares one Space set, and a remote daemon's Spaces stay that daemon's own.

The wire state is fully normalized by gxserver:
  - `order` is the authoritative Space ordering; `spaces` is keyed by spaceId.
  - A project held by a collection can never carry direct membership, so
    gxserver strips grouped project ids from `memberProjectIds`.
  - Member collection ids that no longer exist are dropped; a collection
    disappears from the collections document as soon as it empties.
  - Member ids are unique across Spaces; the first Space in sidebar order wins.
  - Member ids are deduped, and ids/names/icon ids are bounded (256 chars,
    512 ids per list, 256 Spaces).
  - `color` is normalized to lowercase `#rrggbb`, falling back to the shared
    sidebar palette.
  - An EMPTY Space is valid and kept, unlike an empty project collection.
Member project ids for a deleted project may linger as soft references, so
clients must tolerate member ids they cannot resolve. Worktree inheritance and
the built-in "Other" view (packages/shared/sidebar-spaces-other.ts, deleted 2026-10-01) are pure
client concerns and never stored.
Clients write-through-sync the whole state via /api/updateSidebarSpaces and read
it back from the same endpoint, the presentation snapshot, or the
`sidebarSpacesChanged` event.
*/
export interface GxserverSidebarSpace {
  color: string;
  icon: string;
  memberCollectionIds: readonly string[];
  memberProjectIds: readonly string[];
  name: string;
  spaceId: string;
  /** The workspace the Space belongs to; absent = the default workspace. */
  workspaceId?: string;
}

export interface GxserverSidebarSpacesState {
  order: readonly string[];
  spaces: Readonly<Record<string, GxserverSidebarSpace>>;
}

/** One workspace (server/src/workspaces/store.rs). `kind` sets its projects' work-mode default. */
export interface GxserverSidebarWorkspace {
  claudeAccountId?: string;
  color: string;
  kind: 'work' | 'personal';
  letter: string;
  name: string;
  /** The primary tracker picked on this computer (server/src/work_mode/tracker.rs); absent = never picked. */
  tracker?: 'linear' | 'github';
  workspaceId: string;
}

/** Projects and Spaces with no `workspaceId` belong to `defaultWorkspaceId`, which always exists. */
export interface GxserverSidebarWorkspacesState {
  defaultWorkspaceId: string;
  /** A remote machine's sidebar tab (this computer's settings id for it) → the workspace it shows in here; unlisted machines show in the default workspace. */
  machineWorkspaces?: Readonly<Record<string, string>>;
  order: readonly string[];
  workspaces: Readonly<Record<string, GxserverSidebarWorkspace>>;
}

export interface GxserverWorkspaceSessionGroup {
  groupId: string;
  sessionIds: readonly string[];
  title: string;
}

export interface GxserverWorkspaceProjectGroups {
  groups: readonly GxserverWorkspaceSessionGroup[];
  nextGroupNumber?: number;
}

export interface GxserverWorkspaceSessionGroupsState {
  projectOrder: readonly string[];
  projects: Readonly<Record<string, GxserverWorkspaceProjectGroups>>;
}

export interface GxserverReadSidebarProjectCollectionsResult {
  sidebarProjectCollections: GxserverSidebarProjectCollectionsState;
}

export interface GxserverUpdateSidebarProjectCollectionsParams {
  state: GxserverSidebarProjectCollectionsState;
}

export interface GxserverUpdateSidebarProjectCollectionsResult {
  sidebarProjectCollections: GxserverSidebarProjectCollectionsState;
}

export interface GxserverReadSidebarSpacesResult {
  sidebarSpaces: GxserverSidebarSpacesState;
}

export interface GxserverUpdateSidebarSpacesParams {
  state: GxserverSidebarSpacesState;
}

export interface GxserverUpdateSidebarSpacesResult {
  sidebarSpaces: GxserverSidebarSpacesState;
}

/*
CDXC:Sessions 2026-09-11 WHY:
User-defined session tags are a gxserver-owned catalog, per daemon, exactly like
Spaces: `order` is authoritative, `tags` is keyed by tag id, and clients
write-through-sync the whole document via /api/updateCustomSessionTags. A tag
removed from the document is cleared from every session that carried it in the
same write, so a session can never point at a tag the daemon no longer knows.
The catalog rides the presentation snapshot and the mobile summary so every
client (desktop, web, phone, CLI) resolves the same id to the same name, icon
id, and color.
*/
export interface GxserverCustomSessionTag {
  /** `#rrggbb`, lowercase. */
  color: string;
  /** A SIDEBAR_COMMAND_ICON_IDS id; the daemon bounds it but does not validate it. */
  icon: string;
  name: string;
  tagId: string;
}

export interface GxserverCustomSessionTagsState {
  order: readonly string[];
  tags: Readonly<Record<string, GxserverCustomSessionTag>>;
}

export interface GxserverReadCustomSessionTagsResult {
  customSessionTags: GxserverCustomSessionTagsState;
}

export interface GxserverUpdateCustomSessionTagsParams {
  state: GxserverCustomSessionTagsState;
}

export interface GxserverUpdateCustomSessionTagsResult {
  customSessionTags: GxserverCustomSessionTagsState;
}

export interface GxserverPresentationSnapshot {
  /*
  CDXC:StateSync 2026-07-29:
  The inactivity window THIS daemon actually applies in its auto-settle sweep,
  in days. One sidebar renders rows from several daemons, and each daemon reads
  its OWN `sidebarAutoSettleAfterDays`, so a client that applied the local
  window to every machine would park remote sessions the remote daemon still
  considers active (the recorded P2 minor).

  - ABSENT: this daemon predates the field. The client then keeps the P2
    behavior for LOCAL rows (the local settings value, which is the same file
    the local daemon reads) and applies NO client-side inactivity settle to
    remote rows — the remote server's own `settledOverride` is the only truth
    for a machine that cannot state its window.
  - `null`: this daemon has inactivity auto-settle disabled.
  - a number: that daemon's window in days.
  */
  autoSettleAfterDays?: number | null;
  capabilities?: GxserverPresentationCapabilities;
  generatedAt: string;
  groups: readonly GxserverPresentationGroup[];
  projects: readonly GxserverPresentationProject[];
  revision: GxserverPresentationRevision;
  sessions: readonly GxserverPresentationSession[];
  sidebarProjectCollections?: GxserverSidebarProjectCollectionsState;
  sidebarSpaces?: GxserverSidebarSpacesState;
  sidebarWorkspaces?: GxserverSidebarWorkspacesState;
  customSessionTags?: GxserverCustomSessionTagsState;
  workspaceGroups?: GxserverWorkspaceSessionGroupsState;
}

export type GxserverPresentationDelta =
  | {
      domainProject?: GxserverProjectDomainState;
      project: GxserverPresentationProject;
      type: 'projectAdded' | 'projectUpdated';
    }
  | {
      projectId: GxserverProjectId;
      type: 'projectRemoved';
    }
  | {
      group: GxserverPresentationGroup;
      type: 'groupAdded' | 'groupUpdated' | 'groupOrderChanged';
    }
  | {
      groupId: string;
      projectId: GxserverProjectId;
      type: 'groupRemoved';
    }
  | {
      session: GxserverPresentationSession;
      type:
        | 'sessionAdded'
        | 'sessionUpdated'
        | 'sessionMoved'
        | 'sessionTitleChanged'
        | 'sessionActivityChanged'
        | 'sessionLifecycleChanged'
        | 'sessionSurfaceChanged'
        | 'sessionPresentationChanged';
    }
  | {
      projectId: GxserverProjectId;
      sessionId: GxserverSessionId;
      type: 'sessionRemoved';
    };

export interface GxserverPresentationDeltaEvent {
  delta: GxserverPresentationDelta;
  revision: GxserverPresentationRevision;
}

export interface GxserverPresentationSubscribeMessage {
  clientId?: string;
  lastRevision?: GxserverPresentationRevision;
  type: 'subscribePresentation';
}

export interface GxserverPresentationSearchParams {
  externalOnly?: boolean;
  refreshExternalSessions?: boolean;
  cursor?: string;
  includeActive?: boolean;
  includePrevious?: boolean;
  limit?: number;
  projectId?: GxserverProjectId;
  query?: string;
  sessionTags?: readonly GxserverSessionTagFilter[];
}

export interface GxserverPresentationSearchResult {
  isRestorable?: boolean;
  restoreUnavailableReason?: string;
  externalSession?: boolean;
  agentIcon?: string;
  agentId?: string;
  agentName?: string;
  agentSessionId?: string;
  agentSessionPath?: string;
  /**
   * CDXC:Sessions 2026-06-17-17:06:
   * Previous Sessions list/search responses expose close time separately from lastActiveAt so clients can group and sort restore rows by when the session was closed while still rendering Last Active from actual user activity.
   */
  closedAt?: string;
  createdAt: string;
  cwd?: string;
  displayTitle?: string;
  displayTitleTooltip?: string;
  isFavorite: boolean;
  isParked?: boolean;
  isPinned: boolean;
  isPrimaryTitleTerminalTitle: boolean;
  isTemporaryTitle: boolean;
  lastActiveAt?: string;
  lifecycleState: GxserverDomainLifecycleState;
  match?: {
    field: 'agent' | 'command' | 'cwd' | 'id' | 'project' | 'timestamp' | 'title';
    snippet?: string;
  };
  projectId: GxserverProjectId;
  projectTitle: string;
  primaryTitle?: string;
  sessionId: GxserverSessionId;
  /**
   * CDXC:Sessions 2026-06-13-15:36:
   * Previous Sessions search results must carry the same identity and provider metadata needed to render and restore stopped agent rows without rehydrating native sidebar history. Keep raw prompt/user text out of list/search responses; restore-specific command construction stays behind readAgentResumePlan for the selected session.
   */
  sessionPersistenceName?: string;
  sessionPersistenceProvider?: 'tmux' | 'zmx' | 'zellij';
  sessionTag?: GxserverSessionTag;
  sidebarOrder?: number;
  subtitle?: string;
  surface: GxserverSessionSurface;
  terminalTitle?: string;
  title: string;
  titleSource: GxserverSessionTitleSource;
  trustedResumeTitle?: string;
  updatedAt: string;
  zmxName?: GxserverZmxSessionName;
}

export interface GxserverPresentationSearchResponse {
  projects?: Array<{ projectId: string; name: string; path?: string }>;
  cursor?: string;
  results: readonly GxserverPresentationSearchResult[];
}

/**
 * CDXC:SessionFork 2026-08-28:
 * `/api/sessionForkBranches` answers "what else shares this conversation's
 * history". The daemon derives the family from its own registry, so the reply
 * includes the ancestors Previous Sessions hides once something continues from
 * them, flagged `ancestor`. A session with no relatives answers with just
 * itself, which is why a caller can ask unconditionally and gate its UI on the
 * branch count instead of on an error.
 */
export interface GxserverSessionForkBranchesParams {
  projectId: GxserverProjectId;
  sessionId: GxserverSessionId;
}

export interface GxserverSessionForkBranch {
  /** Present only for a superseded row: a branch with no card of its own. */
  ancestor?: boolean;
  agentSessionId?: string;
  /** True for the session that asked. */
  current: boolean;
  lastActiveMs: number;
  lifecycleState: GxserverDomainLifecycleState;
  projectId: GxserverProjectId;
  sessionId: GxserverSessionId;
  title: string;
}

export interface GxserverSessionForkBranchesResult {
  /** Newest activity first. */
  branches: readonly GxserverSessionForkBranch[];
}
