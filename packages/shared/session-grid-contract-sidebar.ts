import type { CompletionSoundSetting } from "./completion-sound";
import type { BundledGhostexAgentSkillId } from "./ghostex-agent-skills";
import type { AgentAcceptAllMode } from "./sidebar-agent-accept-all";
import type { SidebarAgentButton, SidebarAgentIcon } from "./sidebar-agents";
import type { SidebarCommandIcon } from "./sidebar-command-icons";
import type { WorkspaceProjectIcon } from "./workspace-project-appearance";
import type {
  SidebarActionType,
  SidebarCommandButton,
  SidebarCommandLink,
  SidebarCommandRunMode,
  SidebarCommandScope,
} from "./sidebar-commands";
import type {
  SidebarGitAction,
  SidebarGitChangedFile,
  SidebarGitFileDiffDraft,
  SidebarGitState,
} from "./sidebar-git";
import type { SidebarProjectDiffStats } from "./project-diff-stats";
import type {
  ghostexSettings,
  ghostexSettingsPatch,
  ghostexSettingsUpdateSource,
  DiagnosticLoggingScenarioId,
  KeepAwakeDurationMinutes,
} from "./ghostex-settings";
import type { ghostexHotkeyActionId } from "./ghostex-hotkeys";
import type { WorkspaceIdeTargetApp } from "./workspace-open-targets";
import type { SidebarPinnedPrompt } from "./sidebar-pinned-prompts";
import type { SidebarSessionTag, SidebarSessionTagFilter } from "./session-tags";
import type {
  GxserverPortlessPresentation,
  GxserverPortlessStatus,
  GxserverPresentationSessionGitStatus,
  GxserverSidebarProjectCollectionsState,
  GxserverStashedPrompt,
} from "./gxserver-protocol";
import type {
  NativePortlessAdminAction,
  NativePortlessAdminInstallAction,
  NativePortlessAdminResult,
  NativePortlessProtocol,
} from "./native-ghostty-host-protocol";
import type {
  SessionLifecycleState,
  SessionGridSnapshot,
  SessionRecord,
  SidebarTheme,
  TerminalSessionPersistenceProvider,
  TerminalViewMode,
  VisibleSessionCount,
} from "./session-grid-contract-core";

export type SidebarActiveSessionsSortMode = "manual" | "lastActivity";

export type SidebarTitleObservationState = {
  failureCount?: number;
  lastFailedAt?: string;
  lastObservedAt?: string;
  lastStartedAt?: string;
  nextRetryAt?: string;
  status: "active" | "failed" | "retrying" | "starting";
};

export type AgentsHubTab = "mds" | "skills" | "hooks" | "configs";

export type AgentsHubProfile = {
  agentIcon: SidebarAgentIcon;
  filePath: string;
  label: string;
  profilePath: string;
  targetPath?: string;
};

export type AgentsHubFile = {
  content?: string;
  id: string;
  language: string;
  name: string;
  path: string;
};

export type AgentsHubGroup = {
  description: string;
  files: AgentsHubFile[];
  id: string;
  name: string;
  path: string;
  profiles: AgentsHubProfile[];
};

export type AgentsHubCatalogMessage = {
  generatedAt: string;
  groupsByTab: Record<AgentsHubTab, AgentsHubGroup[]>;
  type: "agentsHubCatalog";
};

export type AgentsHubFileContentMessage = {
  content?: string;
  errorMessage?: string;
  filePath: string;
  requestId: string;
  type: "agentsHubFileContent";
};

export type SidebarAgentHookStatus = "installed" | "missing" | "cliMissing" | "notRequired" | "updateRequired";

export type SidebarAgentHookStatusItem = {
  agentId: string;
  cliCommand: string;
  cliInstalled: boolean;
  detail: string;
  hookInstalled: boolean;
  paths: string[];
  status: SidebarAgentHookStatus;
};

/**
 * CDXC:AgentHookSettings 2026-05-23-10:05:
 * Settings -> Agents shows machine-local hook setup status for the same reliable-resume agents Ghostex installs at startup. Native owns filesystem inspection and returns only normalized status rows so the modal host can render the result without direct filesystem access.
 */
export type SidebarAgentHookStatusMessage = {
  agents: SidebarAgentHookStatusItem[];
  errorMessage?: string;
  generatedAt: string;
  hookStateDirectory: string;
  notifyHookPath: string;
  type: "agentHookStatus";
};

export type SidebarGhostexCliStatusMessage = {
  /**
   * CDXC:BrowserAgentControl 2026-05-26-22:17:
   * First-launch CLI setup treats the Ghostex Browser Use skill as part of the
   * installed CLI experience because agents need both the executable and the
   * skill instructions before they can inspect embedded CEF logs and pages.
   *
   * CDXC:IntegrationsSetup 2026-05-27-04:17:
   * Settings -> Integrations and the first-launch flow need one native-owned
   * status payload for CLI, Ghostex Browser Use, and Ghostex Computer Use. Native owns
   * PATH and app-bundle checks so React can warn without guessing from UI state.
   *
   * CDXC:ComputerAgentControl 2026-05-27-06:58:
   * Desktop Control readiness includes the `$ghostex-computer-use` wrapper
   * skill, because Cua Driver alone does not teach agents the Ghostex-named
   * computer-use workflow.
   *
   * CDXC:AgentSkills 2026-05-31-09:18:
   * First launch and Settings must show each bundled Ghostex skill as an
   * explicit install item. Carry per-skill status for Browser Use, Computer Use,
   * Agent Orchestration, and Generate Title instead of only exposing the skills
   * that also have standalone guide pages.
   *
   * CDXC:CodexSessionMove 2026-06-26-13:24:
   * The Codex session-move skill is part of the bundled skills setup surface,
   * so the status payload must carry its installed state and path like the
   * other app-shipped skills.
   *
   * CDXC:CuaPermissions 2026-05-29-06:00:
   * The Cua Permissions row must report Cua Driver's own macOS privacy grants,
   * not Ghostex's Accessibility grant. Carry both Accessibility and Screen
   * Recording from `cua-driver check_permissions` in the setup status payload.
   *
   * CDXC:ContributorStart 2026-06-22-23:23:
   * `unavailable` means an optional local-build resource was intentionally not
   * bundled, while `missing` means a strict or release-shaped build is broken.
   */
  browserSkillInstalled: boolean;
  browserSkillPath?: string;
  embeddedBrowserSkillInstalled: boolean;
  embeddedBrowserSkillPath?: string;
  computerUseSkillInstalled: boolean;
  computerUseSkillPath?: string;
  agentOrchestrationSkillInstalled: boolean;
  agentOrchestrationSkillPath?: string;
  /**
   * CDXC:Fable56Orchestration 2026-07-04-00:00:
   * `$ghostex-fable-5.6-orchestration` shipped after existing hosts, so its
   * status fields stay optional and consumers must treat a missing value as
   * not installed instead of requiring every host build to send it.
   */
  fable56OrchestrationSkillInstalled?: boolean;
  fable56OrchestrationSkillPath?: string;
  findPrevSessionSkillInstalled?: boolean;
  findPrevSessionSkillPath?: string;
  generateTitleSkillInstalled: boolean;
  generateTitleSkillPath?: string;
  moveCodexSessionSkillInstalled: boolean;
  moveCodexSessionSkillPath?: string;
  cuaDriverAccessibilityPermissionGranted?: boolean;
  cuaAppInstalled: boolean;
  cuaDriverInstalled: boolean;
  /** True only when this host can install and update Cua Driver in-app. */
  cuaDriverManagedUpdatesSupported?: boolean;
  cuaDriverLatestVersion?: string;
  cuaDriverPermissionDetail?: string;
  cuaDriverPath?: string;
  cuaDriverScreenRecordingPermissionGranted?: boolean;
  cuaDriverUpdateAvailable?: boolean;
  cuaDriverVersion?: string;
  detail: string;
  generatedAt: string;
  ghostexPath?: string;
  gxBlockedByExistingCommand: boolean;
  gxPath?: string;
  gxUsable: boolean;
  installed: boolean;
  type: "ghostexCliStatus";
};

export type SidebarOSIntegrationStatusTarget =
  | "bundleRegistration"
  | "editor"
  | "platform"
  | "scriptRunner"
  | "terminalLinks";

export type SidebarOSIntegrationStatusOperation =
  | "readStatus"
  | "registerBundle"
  | "setDefault";

export type SidebarOSIntegrationStatusState =
  | "failed"
  | "skipped"
  | "unsupported";

export type SidebarOSIntegrationStatusReason =
  | "bundleIdentifierMissing"
  | "bundleRegistrationFailed"
  | "contentTypeUnavailable"
  | "invalidTarget"
  | "launchServicesRejected"
  | "unsupportedPlatform";

export type SidebarOSIntegrationStatusItem = {
  extension?: string;
  operation: SidebarOSIntegrationStatusOperation;
  reason: SidebarOSIntegrationStatusReason;
  scheme?: "ghostex";
  status: SidebarOSIntegrationStatusState;
  target: SidebarOSIntegrationStatusTarget;
};

export type SidebarOSIntegrationStatusMessage = {
  /**
   * CDXC:OSIntegration 2026-05-27-18:06:
   * Settings -> OS Integration shows native Launch Services diagnostics so the
   * user can tell whether Ghostex is merely available in Open With or is the
   * current default for editor, terminal-link, and script-runner roles.
   *
   * CDXC:OSIntegration 2026-06-24-15:10:
   * Reused Settings surfaces need a shared privacy-safe status channel for
   * Launch Services failures. `statusItems` carries only enum reasons, target,
   * operation, known file extensions, and the fixed ghostex scheme; it must not
   * expose bundle paths, file paths, URLs, command text, environment values,
   * tokens, stdout/stderr, daemon bodies, or raw OSStatus values.
   */
  bundleIdentifier: string;
  editorDefaults: Record<string, string>;
  generatedAt: string;
  registeredEditableFiles: boolean;
  registeredGhostexURLScheme: boolean;
  registeredScriptRunner: boolean;
  scriptDefaults: Record<string, string>;
  statusItems?: SidebarOSIntegrationStatusItem[];
  terminalLinkDefaultBundleId?: string;
  type: "osIntegrationStatus";
};

/**
 * CDXC:SidebarV2Lifecycle 2026-07-29:
 * The explicit user pin on a session's settle state. `"settled"` forces the
 * settled shelf, `"active"` holds a session in the inbox and suppresses
 * auto-settle. gxserver keeps an unpublished `settledOverrideAt` so real
 * activity newer than the override clears it server-side.
 */
export type SidebarSessionSettledOverride = "active" | "settled";

/**
 * CDXC:SidebarV2Git 2026-07-29:
 * A session's git/PR state is gxserver's to compute (only the daemon can run
 * git in the session's cwd), so the sidebar contract ALIASES the wire type
 * instead of restating it. One source of truth means a field the server adds
 * cannot silently stop at the projection.
 */
export type SidebarSessionGitStatus = GxserverPresentationSessionGitStatus;

/** Change-request state a `SidebarSessionGitStatus` can report. */
export type SidebarSessionPrState = NonNullable<SidebarSessionGitStatus["prState"]>;

/**
 * CDXC:SidebarV2Lifecycle 2026-07-29:
 * gxserver's per-daemon settle/snooze capability flags, mirrored from
 * `GxserverPresentationSnapshot.capabilities`. ABSENCE means "this daemon
 * predates session lifecycle": the affordances hide entirely and nothing
 * classifies as settled or snoozed, instead of clicking through to a 404.
 *
 * CDXC:SidebarV2Git 2026-07-29:
 * `sessionGitStatus` rides the same per-daemon block and the same
 * machine-scoped resolution, because git/PR data is exactly as machine-local
 * as settle state: one gxserver in the merged sidebar can publish it while
 * another cannot. A false/absent flag renders identically to a session that
 * simply has no `gitStatus`.
 */
export type SidebarSessionLifecycleCapabilities = {
  sessionGitStatus?: boolean;
  sessionSettlement?: boolean;
  sessionSnooze?: boolean;
  /**
   * CDXC:SidebarV2Worktree 2026-07-29:
   * The daemon serves the V2 worktree flow (`createWorktreeSession` /
   * `removeSessionWorktree`). Machine-scoped exactly like the flags above: only
   * the gxserver holding the repository can cut a worktree in it. A false or
   * absent flag collapses V2's split "+" back to the plain instant-session
   * button for that machine's projects, and hides the worktree context item.
   */
  worktreeSessions?: boolean;
};

export type SidebarSessionItem = {
  kind?: "browser" | "workspace";
  sessionKind?: "browser" | "terminal";
  activity: "idle" | "working" | "attention";
  activityLabel?: string;
  agentIcon?: SidebarAgentIcon;
  /**
   * CDXC:SessionRestore 2026-05-22-23:59:
   * Agent CLI hook installs capture the stable provider session id separately from Ghostex's visible session id. Sidebar cards carry that value so hover tooltips can show the exact resume target while title-based restore remains a backup.
   */
  agentSessionId?: string;
  faviconDataUrl?: string;
  firstUserMessage?: string;
  isGeneratingFirstPromptTitle?: boolean;
  isReloading?: boolean;
  lifecycleState?: SessionLifecycleState;
  isFavorite?: boolean;
  /**
   * CDXC:SessionTags 2026-06-05-12:30:
   * Sidebar rows carry the expanded tag marker separately from legacy
   * `isFavorite`. Renderers use this for the leading icon, tag filters, and
   * tooltip prefix while older Favorite-only rows still project as Favorite.
   */
  sessionTag?: SidebarSessionTag;
  /**
   * CDXC:PinnedSessions 2026-05-28-12:04:
   * Sidebar rows carry project-local pin state so the React display sorter can
   * keep pinned sessions at the top of their project and render pin chrome
   * without overloading Favorite.
   */
  isPinned?: boolean;
  /**
   * CDXC:SidebarV2 2026-07-29:
   * Session creation stamp, projected straight from gxserver's presentation
   * session. Sidebar V2's inbox is ordered by creation and must never move a
   * row on activity, so it needs a clock that activity cannot advance —
   * `lastInteractionAt` and `sortKey` both change while a session works.
   */
  createdAt?: string;
  /**
   * CDXC:ActivitySuppressionPolicy 2026-07-29-12:00:
   * `lastInteractionAt` carries gxserver's meaningful-activity recency: short
   * working blips (tiny commands, wake redraws) do not advance it, so
   * activity-sorted lists stay stable. `workingStartedAt` marks the current
   * working stint so the sorter can tell whether that stint has already
   * qualified as meaningful (lastInteractionAt >= workingStartedAt) before
   * giving the row working priority.
   */
  lastInteractionAt?: string;
  workingStartedAt?: string;
  /**
   * CDXC:SidebarV2Lifecycle 2026-07-29:
   * Server-owned settle/snooze lifecycle, projected straight from gxserver's
   * presentation session. Every field is optional because an older daemon (a
   * remote machine that has not been upgraded) publishes none of them, and the
   * V2 shelves must degrade to "nothing is settled, nothing is snoozed" instead
   * of inventing lifecycle out of derived data.
   *
   * `settledAt` is stamped only by a MANUAL settle; the server-side auto-settle
   * sweep sets `settledOverride: "settled"` and leaves this null, so the settled
   * shelf falls back to the activity clock for sorting (see
   * `resolveSidebarV2SettledTimestampMs`).
   *
   * `snoozedUntil` is deliberately RETAINED after the wake time passes (gxserver
   * garbage-collects it ~24h later). The wake itself is derived client-side, to
   * the millisecond, and the retained pair is what drives the "Woke" indicator.
   */
  settledAt?: string;
  settledOverride?: SidebarSessionSettledOverride;
  snoozedAt?: string;
  snoozedUntil?: string;
  /**
   * CDXC:SidebarV2Git 2026-07-29:
   * Branch, diff stats, and change-request state for this session's cwd,
   * copied through from gxserver's presentation session. Absent for every
   * session the daemon could not (or does not yet) probe, which is also what
   * an un-upgraded remote machine publishes — Sidebar V2 then renders the card
   * exactly as it did before git data existed instead of reserving a blank
   * line for it.
   */
  gitStatus?: SidebarSessionGitStatus;
  /**
   * CDXC:SidebarV2Worktree 2026-07-29:
   * The session's working directory, copied through from gxserver's
   * presentation session. Sidebar V2 needs it because a worktree is an
   * ATTRIBUTE of a session: `cwd` IS the checkout, and pairing it with
   * `gitStatus.branch` is how the client tells a managed `ghostex/…` worktree
   * from a plain session in the project root. Absent for hosts that do not
   * publish it, and the worktree affordances simply do not appear.
   */
  cwd?: string;
  sessionId: string;
  /**
   * CDXC:SessionTooltips 2026-05-31-06:25:
   * macOS gxserver sessions need their full routed identity in hover tooltips
   * instead of the legacy two-digit display number, because the short display
   * number does not identify the server/project/session being restored.
   */
  sessionRoutingId?: string;
  sessionNumber?: string;
  sessionPersistenceName?: string;
  sessionPersistenceProvider?: TerminalSessionPersistenceProvider;
  /**
   * CDXC:GxserverSessionTitles 2026-06-07-09:33:
   * gxserver-owned rows carry the final visible title string. Sidebar clients render this directly so platform adapters do not duplicate terminal-title trust, placeholder, or unsynced-marker rules.
   */
  displayTitle?: string;
  displayTitleTooltip?: string;
  primaryTitle?: string;
  isPrimaryTitleTerminalTitle?: boolean;
  terminalTitle?: string;
  /**
   * CDXC:SessionStatus 2026-06-07-00:30:
   * Sidebar Auto Sleep must not interpret an idle activity value as reliable while gxserver's zmx title observer is starting or retrying. Carry only coarse observer health so the UI can defer sleep decisions without exposing terminal titles or user-owned terminal content.
   */
  titleObservation?: SidebarTitleObservationState;
  alias: string;
  shortcutLabel: string;
  row: number;
  column: number;
  isFocused: boolean;
  /**
   * CDXC:SessionLifecycle 2026-05-29-09:20:
   * Session lifecycle uses resource-specific state names so UI and batch
   * actions do not infer provider session existence from the legacy `isSleeping` and
   * `isRunning` booleans. A native pane can be unmounted while a zmx/tmux/zellij
   * provider session still exists, so both resource states are carried
   * explicitly and `isLive` is derived from them.
   *
   * CDXC:SessionLifecycle 2026-05-29-06:29:
   * Persistence-disabled terminal sessions must report `providerSessionState:
   * "persistence-disabled"` instead of `unknown`. Unknown is reserved for configured
   * providers whose existence check has not completed or failed.
   *
   * CDXC:SessionLifecycle 2026-05-29-07:19:
   * Name the providerless state `persistence-disabled` so payloads make it
   * clear the terminal provider is absent because persistence is off, not
   * because some unrelated disabled flag was set.
   */
  nativePaneState?: "mounted" | "mounting" | "unmounted";
  providerSessionState?: "exists" | "missing" | "persistence-disabled" | "unknown";
  isLive?: boolean;
  /** @deprecated Use nativePaneState/providerSessionState plus isLive. */
  isSleeping?: boolean;
  isVisible: boolean;
  /** @deprecated Use isLive for runtime liveness and activity for work state. */
  isRunning: boolean;
  detail?: string;
  /**
   * CDXC:CloseAfterDone 2026-06-15-21:00:
   * Sidebar cards need both the armed Close After Done flag and countdown
   * projection. The armed flag keeps the red clock visible before Done, while
   * the deadline fields drive the fading countdown once the session remains
   * Done long enough to be eligible for automatic close.
   */
  closeAfterDone?: boolean;
  closeAfterDoneDeadlineAt?: string;
  closeAfterDoneRemainingLabel?: string;
  closeAfterDoneRemainingMs?: number;
  /**
   * CDXC:RemoteSessionMenus 2026-06-30-15:22:
   * Remote session rows opt into sidebar actions that depend on host timers or local pane carriers. Absence is false so the shared context menu never assumes every remote terminal can schedule Delayed Send, toggle Close After Done, or pop out through AppKit.
   */
  canScheduleDelayedSend?: boolean;
  canToggleCloseAfterDone?: boolean;
  /**
   * CDXC:DelayedSend 2026-05-17-03:14
   * Delayed Send timers must be visible before they fire. Carry both the
   * absolute deadline and the display countdown so sidebar cards, titlebar
   * resources, and tooltips can show the same remaining time.
   */
  delayedSendDeadlineAt?: string;
  delayedSendRemainingLabel?: string;
  delayedSendRemainingMs?: number;
  /**
   * CDXC:SessionChatPromptQueue 2026-08-21-b:
   * Number of Ghostex-owned chat prompts held for this session, `failed` rows
   * included. Drives the count badge over the leading agent icon; absent or
   * zero means no badge, which is also what a daemon that predates the queue
   * reports.
   */
  queuedPromptCount?: number;
  /**
   * CDXC:SessionChatPromptQueue 2026-08-21-b:
   * How many of those rows failed to deliver and are held for the user. Any
   * non-zero value paints the same badge red instead of yellow, because a queue
   * that has stopped dead is the one queue state that needs the user to act.
   */
  queuedPromptFailedCount?: number;
  /** True when Delayed Send is armed for every agent in this project to finish. */
  sendWhenAllProjectSessionsStopActive?: boolean;
  /** True when Delayed Send is armed for this agent to finish. */
  sendWhenAgentStopsActive?: boolean;
  /**
   * CDXC:PanePopOut 2026-05-19-10:15:
   * Sidebar session context menus need the live pop-out presentation flag so
   * browser and agent cards can offer Pop Out Pane versus Restore Pane without
   * re-querying native chrome state.
   *
   * CDXC:RemoteAttach 2026-06-30-15:24:
   * Remote rows must expose Pop Out Pane as an explicit local-carrier capability. A remote gxserver session can look like a normal agent terminal, but AppKit pop-out is only valid when a live local attach carrier already exists.
   */
  canPopOutPane?: boolean;
  isPoppedOut?: boolean;
};

export function getSidebarSessionLifecycleState(
  session: Pick<
    SidebarSessionItem,
    "isLive" | "isRunning" | "isSleeping" | "lifecycleState" | "nativePaneState" | "providerSessionState"
  >,
): SessionLifecycleState {
  if (session.lifecycleState) {
    return session.lifecycleState;
  }

  if (session.isLive === true) {
    return "running";
  }

  if (session.nativePaneState === "mounted" || session.providerSessionState === "exists") {
    return "running";
  }

  if (session.isSleeping) {
    return "sleeping";
  }

  return session.isRunning ? "running" : "done";
}

export type SidebarPreviousSessionItem = SidebarSessionItem & {
  closedAt: string;
  groupId?: string;
  historyId: string;
  isGeneratedName: boolean;
  isRestorable: boolean;
  /**
   * CDXC:PreviousSessions 2026-05-05-05:30
   * Restoring from Previous Sessions must recreate the archived agent session,
   * not only its card title. Store the normalized session record and source
   * project/group metadata so native restore can preserve agent identity,
   * first-message metadata, title provenance, and resumable session details.
   */
  projectId?: string;
  projectName?: string;
  projectPath?: string;
  sessionRecord?: SessionRecord;
  sidebarOrder?: number;
};

export type SidebarSessionGroup = {
  kind?: "browser" | "workspace";
  groupId: string;
  isActive: boolean;
  /**
   * CDXC:Chats 2026-05-04-09:41
   * Native Combined mode renders all chat folders under one synthetic Chats
   * header. Mark it explicitly so the React sidebar can keep it non-draggable
   * and route its add button to creating a new chat folder.
   */
  isChatCollection?: boolean;
  /**
   * CDXC:SessionFocusMode 2026-05-28-12:52:
   * Focus is a split-pane zoom, not a tab selector. Sidebar groups must carry actual pane topology so session context menus can hide Focus when a project has only one pane, even if that pane has multiple tabs.
   *
   * CDXC:SessionFocusMode 2026-05-28-15:35:
   * The topology signal must reflect awake rendered pane owners, not only persisted paneLayout children, so sleeping-only split panes do not leave Focus visible while the user sees one native pane.
   */
  canFocusMode?: boolean;
  /**
   * CDXC:SidebarContract 2026-07-02-03:49:
   * The shared sidebar can expose named session-group creation only when the host can persist or emulate user-defined groups for the project.
   *
   * Hosts that support user-defined named session groups within a project set
   * this on groups that can spawn a new group (project groups and their
   * sub-groups). Hosts without the capability leave it unset so the New
   * Group / Move to New Group affordances never render.
   */
  canCreateSessionGroup?: boolean;
  isFocusModeActive: boolean;
  layoutVisibleCount: VisibleSessionCount;
  projectContext?: {
    canRemoveProject: boolean;
    /**
     * CDXC:GPUISettingsNotifications 2026-06-26-07:22:
     * GPUI session-attention notifications need the same project icon attachment source as the macOS host. Carry only the already-normalized project image data URL on project context so status bridges can attach icons without paths, URLs, file probes, command text, terminal output, or generic renderer IPC.
     */
    iconDataUrl?: string;
    /**
     * CDXC:SidebarV2ProjectIcons 2026-07-29:
     * The project's TYPED icon, exactly as the user chose it in the project
     * appearance UI. `iconDataUrl` above covers only the image variant, which
     * is the rarer of the two: most Ghostex projects carry a Tabler glyph plus
     * a color, and a surface that reads `iconDataUrl` alone shows those
     * projects a generic folder. Carry the whole icon so sidebar surfaces can
     * render the same identity the Recent Projects list does, with the folder
     * glyph reserved for projects that genuinely have no icon.
     */
    icon?: WorkspaceProjectIcon;
    /**
     * CDXC:SidebarV2ProjectIcons 2026-07-29 (discovered icons):
     * The icon the project's own repository ships through standard web metadata,
     * its favicon, or the icon its HTML entry point declares — discovered by
     * gxserver and carried as a `data:` URL
     * (`GxserverPresentationProject.discoveredIconDataUrl`).
     *
     * Rank: below a user-attached IMAGE (an uploaded picture is deliberate
     * intent that no automatic guess should override), above a typed Tabler
     * glyph (which V1 never renders on session rows at all, so it is usually a
     * legacy value migrated forward rather than a considered choice), and above
     * the folder — which is now reserved for projects that have nothing.
     *
     * Absent whenever the daemon found nothing, has not probed yet, or is too
     * old to publish it.
     */
    discoveredIconDataUrl?: string;
    /**
     * CDXC:SidebarV2LogicalProjects 2026-07-29:
     * The project's git `origin` remote URL, straight off the presentation
     * project (`GxserverPresentationProject.gitRemoteOriginUrl`) with no
     * client-side interpretation. Sidebar V2 normalizes it into a repository
     * identity so the same repository checked out on several machines merges
     * into ONE logical project; `null` means "probed, no origin" and an absent
     * key means "not probed / not a git work tree". Both never merge.
     *
     * It lives on `projectContext` rather than on the group because it is a
     * property of the checkout the group points at, exactly like `path`, and
     * because the Quick/Chats collection has no project context and therefore
     * can never participate in repository grouping.
     */
    gitRemoteOriginUrl?: string | null;
    /**
     * CDXC:SidebarV2LogicalProjects 2026-07-29 (P5 fix round):
     * The repository root the checkout belongs to, straight off
     * `GxserverPresentationProject.gitRepositoryRootPath`. Paired with
     * `gitRemoteOriginUrl` it lets Sidebar V2 derive each project's path
     * BELOW the repository root, which is the only thing that can tell two
     * sub-projects of one monorepo apart under "Repository + path".
     *
     * Absent whenever the daemon has no root to report; the client then keys
     * on the bare repository, which is what a single-checkout project wants
     * anyway.
     */
    gitRepositoryRootPath?: string;
    path: string;
    pathState?: "available" | "missing" | "notDirectory" | "unavailable";
    /**
     * CDXC:EditorPanes 2026-05-06-14:21
     * Combined project cards expose one project-owned code editor surface.
     * The editor is not a split session, so sidebar state carries it through
     * project context instead of mixing it into session card records.
     */
    editor: {
      diffStats: SidebarProjectDiffStats;
      /**
       * CDXC:EditorPanes 2026-05-09-17:24
       * Project editor rows represent attempted/running editor surfaces, not
       * only focused panes. Carry load status so the sidebar can keep the row
       * visible through startup failures and show timeout diagnostics.
       */
      errorMessage?: string;
      isOpen: boolean;
      isSleeping: boolean;
      projectId: string;
      status: "idle" | "opening" | "running" | "error";
    };
    theme?: SidebarTheme;
    themeColor?: string;
    worktree?: SidebarProjectWorktreeMetadata;
  };
  remoteMachineContext?: {
    machineId: string;
    machineName: string;
    /** Raw project id in that machine's gxserver; absent for synthetic groups such as Chats. */
    projectId?: string;
  };
  /**
   * CDXC:GPUIRemoteLastSeen 2026-07-12:
   * A stale group renders the last-seen state of a disconnected remote
   * machine: faded, with terminal/agent rows non-interactive while browser
   * rows (local CEF tabs) stay clickable. Hosts without last-seen retention
   * leave it unset.
   */
  isStale?: boolean;
  sessions: SidebarSessionItem[];
  title: string;
  viewMode: TerminalViewMode;
  visibleCount: VisibleSessionCount;
};

export type SidebarProjectWorktreeMetadata = {
  branch: string;
  createdAt?: string;
  name: string;
  parentProjectId: string;
  parentProjectName: string;
  parentProjectPath: string;
};

export type SidebarProjectWorktree = {
  branch?: string;
  directory: string;
  name: string;
};

export type SidebarProjectSettingsItem = {
  beadsDirectory?: string;
  beadsDisplayKey?: string;
  /**
   * CDXC:DocsRootDirectory 2026-08-09:
   * Absolute folder this project's Docs surface shows IN ADDITION to the
   * project's own docs. Absent/blank means the project inherits the Docs
   * directory Global Default, and an unset global adds nothing.
   *
   * CDXC:DocsRootAdditive 2026-08-09: it never replaces the project's own
   * README.md, CLAUDE.md, docs/, or configured Docs folders.
   */
  docsDirectory?: string;
  name: string;
  path: string;
  projectId: string;
  /**
   * CDXC:PortlessSettings 2026-06-23-03:47:
   * Projects settings groups read-only Portless domain summaries by project
   * and worktree family. Carry only the stable parent project id for worktree
   * rows so the Settings UI does not need branch names, parent paths, command
   * text, or slug-editing state.
   */
  worktreeParentProjectId?: string;
  worktreeCommand?: string;
};

export type SidebarRecentProject = {
  icon?: WorkspaceProjectIcon;
  iconDataUrl?: string;
  path: string;
  projectId: string;
  recentClosedAt?: string;
  /**
   * CDXC:RemoteRecentProjects 2026-06-24-10:36:
   * Remote closed projects share the Recent Projects drawer with local parked projects. Carry the owning machine separately so React can display "Project (Machine)" while native still routes restore/open/remove by the trusted scoped project id.
   */
  remoteMachineId?: string;
  remoteMachineName?: string;
  sessionCount: number;
  theme?: SidebarTheme;
  themeColor?: string;
  title: string;
};

export type SidebarCommandSessionIndicator = {
  commandId: string;
  /**
   * CDXC:GPUICommandPaneTimers 2026-06-27-02:05:
   * Command-session HUD indicators need the same safe timer projection as sidebar session cards so GPUI command panes can show Delayed Send and Close After Done parity without carrying command text, cwd/env, paths, URLs, output, run ids, status-file paths, tokens, or unknown native fields.
   */
  closeAfterDone?: boolean;
  closeAfterDoneDeadlineAt?: string;
  closeAfterDoneRemainingLabel?: string;
  closeAfterDoneRemainingMs?: number;
  delayedSendDeadlineAt?: string;
  delayedSendRemainingLabel?: string;
  delayedSendRemainingMs?: number;
  isActive?: boolean;
  sessionId: string;
  status: "idle" | "running" | "error";
  title?: string;
};

export type SidebarPortlessNativeAdminUnavailableReason =
  | "localMacOnly"
  | "notRecommended"
  | "setupNotGhostexOwned";

export type SidebarPortlessNativeAdminActionAvailability = {
  action: NativePortlessAdminAction;
  available: boolean;
  unavailableReason?: SidebarPortlessNativeAdminUnavailableReason;
};

export type SidebarPortlessState = {
  /*
  CDXC:PortlessProtocol 2026-06-23-00:25:
  React receives Portless setup state, route previews, local-only native action availability, and sanitized native admin results through HUD metadata. This keeps future modal/settings/resources UI off Portless files and prevents remote gxserver state from advertising runnable privileged actions.
  */
  health: GxserverPortlessStatus;
  nativeAdmin: {
    actions: Record<NativePortlessAdminAction, SidebarPortlessNativeAdminActionAvailability>;
    available: boolean;
    lastResult?: NativePortlessAdminResult;
  };
  presentation?: GxserverPortlessPresentation;
};

export type SidebarHudState = {
  activeSessionsSortMode: SidebarActiveSessionsSortMode;
  /**
   * CDXC:AgentHooks 2026-06-07-08:51:
   * Tips & Tricks and Settings consume gxserver-owned hook status from shared HUD state so every client can warn about unreliable agent statuses without probing local hook files or owning installer logic.
   */
  agentHookStatus?: SidebarAgentHookStatusMessage;
  agentManagerZoomPercent: number;
  agents: SidebarAgentButton[];
  /**
   * Hosts without a native App Icon subsystem (GPUI) set this so the shared
   * Settings modal hides the App Icon section instead of rendering a dead
   * picker. Absent means available, so macOS behavior is unchanged.
   */
  appIconPickerUnavailable?: boolean;
  buildStamp?: string;
  commands: SidebarCommandButton[];
  /**
   * CDXC:ProjectActions 2026-08-01:
   * Per-project Action buttons keyed by project id (worktrees already resolved
   * to their parent's Actions by gxserver). Project rows read their own entry
   * so showOnProjectRow actions render for every visible project, not just the
   * active one. Hosts that cannot serve the block omit it and rows fall back
   * to rendering nothing.
   */
  commandsByProject?: Record<string, SidebarCommandButton[]>;
  commandSessionIndicators: SidebarCommandSessionIndicator[];
  completionBellEnabled: boolean;
  completionSound: CompletionSoundSetting;
  completionSoundLabel: string;
  /**
   * CDXC:WorkspaceTheme 2026-05-05-02:58
   * The active workspace can override the preset sidebar theme with a custom
   * validated color. Keep the preset `theme` as the fallback, and send this
   * color separately so CSS can derive app-level theme variables.
   */
  customThemeColor?: string;
  debuggingMode: boolean;
  focusedSessionTitle?: string;
  git: SidebarGitState;
  /**
   * CDXC:GlobalActions 2026-08-01:
   * Actions that apply to every project, stored by the daemon rather than in
   * project metadata. Optional because hosts that do not serve them (legacy
   * macOS) leave it absent, and Settings renders an empty section rather than
   * a broken one.
   */
  globalCommands?: SidebarCommandButton[];
  isFocusModeActive: boolean;
  /**
   * CDXC:SidebarV2Lifecycle 2026-07-29:
   * Settle/snooze capability is a per-DAEMON fact, and one sidebar renders rows
   * from several daemons at once: the local gxserver plus one presentation
   * snapshot per connected remote machine (each remote group carries
   * `remoteMachineContext.machineId`, and its rows are built from that
   * machine's own snapshot). So the capability cannot be one global boolean.
   *
   * The scoping that falls out of that merge shape is machine-scoped with a
   * local default: `lifecycleCapabilities` is the LOCAL daemon's answer and
   * applies to every group without a `remoteMachineContext`, while
   * `lifecycleCapabilitiesByMachineId` answers for remote groups keyed by their
   * machine id. A remote machine missing from the map is treated as
   * incapable — never as "same as local" — because guessing would show a settle
   * button that the remote daemon would reject.
   */
  lifecycleCapabilities?: SidebarSessionLifecycleCapabilities;
  lifecycleCapabilitiesByMachineId?: Readonly<
    Record<string, SidebarSessionLifecycleCapabilities>
  >;
  /**
   * CDXC:SidebarV2LogicalProjects 2026-07-29:
   * The inactivity auto-settle window each daemon actually applies, in days,
   * scoped exactly like `lifecycleCapabilities` above: this field is the LOCAL
   * daemon's window and `autoSettleAfterDaysByMachineId` answers for remote
   * groups keyed by machine id.
   *
   * The absent/`null` distinction is load-bearing and mirrors the wire
   * (`GxserverPresentationSnapshot.autoSettleAfterDays`):
   * - absent (local): the daemon predates the field, so V2 falls back to the
   *   local `sidebarAutoSettleAfterDays` setting — the same file that daemon
   *   reads — which is the P2 behavior.
   * - absent (a remote machine): V2 applies NO client-side inactivity settle to
   *   that machine's rows. Applying the local window there is exactly the P2
   *   minor this field exists to fix, and the remote daemon's own
   *   `settledOverride` is authoritative for machines that cannot state a
   *   window.
   * - `null`: that daemon has inactivity auto-settle switched off.
   */
  autoSettleAfterDays?: number | null;
  autoSettleAfterDaysByMachineId?: Readonly<Record<string, number | null>>;
  pendingAgentIds: string[];
  portless?: SidebarPortlessState;
  /**
   * CDXC:Worktrees 2026-05-18-23:07:
   * The Worktrees settings surface needs the same project id/name/path projection as native workspace storage, plus an optional per-project command override for creating worktrees.
   */
  projectSettingsProjects?: SidebarProjectSettingsItem[];
  /**
   * CDXC:RecentProjects 2026-05-04-14:25
   * Combined sidebar hides projects without active/sleeping sessions in a
   * bottom Recent Projects drawer. The drawer receives a compact, sorted
   * projection so React can restore projects without owning native session
   * storage.
   */
  recentProjects: SidebarRecentProject[];
  projectWorktrees?: SidebarProjectWorktree[];
  settings?: ghostexSettings;
  createSessionOnSidebarDoubleClick: boolean;
  renameSessionOnDoubleClick: boolean;
  showCloseButtonOnSessionCards: boolean;
  theme:
    | "dark-1"
    | "dark-2"
    | "plain-dark"
    | "plain-light"
    | "dark-green"
    | "dark-blue"
    | "dark-red"
    | "dark-pink"
    | "dark-orange"
    | "light-blue"
    | "light-green"
    | "light-pink"
    | "light-orange";
  highlightedVisibleCount: VisibleSessionCount;
  visibleCount: VisibleSessionCount;
  visibleSlotLabels: string[];
  viewMode: TerminalViewMode;
};

export type SidebarHydrateMessage = {
  groups: SidebarSessionGroup[];
  pinnedPrompts: SidebarPinnedPrompt[];
  previousSessions: SidebarPreviousSessionItem[];
  remoteSidebarProjectCollectionsByMachineId?: Readonly<
    Record<string, GxserverSidebarProjectCollectionsState>
  >;
  revision: number;
  scratchPadContent: string;
  type: "hydrate";
  hud: SidebarHudState;
};

export type SidebarSessionStateMessage = {
  groups: SidebarSessionGroup[];
  pinnedPrompts: SidebarPinnedPrompt[];
  previousSessions: SidebarPreviousSessionItem[];
  revision: number;
  scratchPadContent: string;
  type: "sessionState";
  hud: SidebarHudState;
};

export type SidebarSessionPresentationChangedMessage = {
  revision?: number;
  session: SidebarSessionItem;
  type: "sessionPresentationChanged";
};

export type SidebarGroupsChangedMessage = {
  groupOrder: string[];
  groups: SidebarSessionGroup[];
  removedGroupIds?: string[];
  removedSessionIds?: string[];
  revision: number;
  /*
  CDXC:SidebarHydration 2026-06-09-23:01:
  Routine gxserver presentation changes must patch the React sidebar tree instead of posting a full hydrate. Carry changed groups, removals, and authoritative order so session add/remove/reorder/project deltas update visible rows without replacing unrelated sidebar state or letting WKWebView refreshes steal terminal focus.
  */
  type: "sidebarGroupsChanged";
};

export type SidebarProjectCollectionsChangedMessage = {
  /*
  CDXC:SidebarProjectCollections 2026-07-18-00:00:
  gxserver owns the shared colored "Group N" project-collection overlay so
  React Native Android edits the same grouped project list. Hosts forward the normalized
  wire state (snapshot field, live event, or update ack) to SidebarApp, which
  reconciles it into its localStorage-backed instant-edit state.
  */
  sidebarProjectCollections: GxserverSidebarProjectCollectionsState;
  remoteMachineId?: string;
  type: "sidebarProjectCollectionsChanged";
};

export type SidebarHudChangedMessage = {
  hud: SidebarHudState;
  revision: number;
  /*
  CDXC:SidebarHydration 2026-06-09-23:01:
  Live presentation patches still need HUD-derived chrome such as focused title, counts, and command indicators. Send HUD as its own patch so gxserver deltas do not force a session-tree hydrate just to keep non-row controls current.
  */
  type: "sidebarHudChanged";
};

export type SidebarPlayCompletionSoundMessage = {
  sound: CompletionSoundSetting;
  sessionId?: string;
  type: "playCompletionSound";
};

export type SidebarOrderSyncKind = "agent" | "command";

export type SidebarOrderSyncResultMessage = {
  itemIds: string[];
  kind: SidebarOrderSyncKind;
  requestId: string;
  status: "error" | "success";
  type: "sidebarOrderSyncResult";
};

export type SidebarCommandRunState = "error" | "running" | "success";

export type SidebarCommandRunStateChangedMessage = {
  commandId: string;
  runId: string;
  state: SidebarCommandRunState;
  type: "sidebarCommandRunStateChanged";
};

export type SidebarCommandRunStateClearedMessage = {
  commandId: string;
  type: "sidebarCommandRunStateCleared";
};

/**
 * CDXC:SidebarBrowserTabReveal 2026-08-18:
 * A host-owned request to make one existing session row visible in the sidebar:
 * expand every collapsed container above it and scroll it into view if it is
 * off screen. gpui sends this when the user opens a new Browser tab, because a
 * background tab (middle-click) has no other visible feedback. `requestId`
 * makes repeat reveals of the same row distinct one-shot requests, since a
 * second middle-click on the same link must reveal it again.
 */
export type SidebarRevealSessionMessage = {
  requestId: number;
  sessionId: string;
  type: "revealSidebarSession";
};

export type SidebarDaemonInfo = {
  pid: number;
  port: number;
  protocolVersion: number;
  startedAt: string;
};

export type SidebarDaemonSessionItem = {
  agentName?: string;
  agentStatus: "idle" | "working" | "attention";
  cols: number;
  cwd: string;
  endedAt?: string;
  errorMessage?: string;
  exitCode?: number;
  isCurrentWorkspace: boolean;
  isLocalOnly?: boolean;
  /**
   * CDXC:SessionInventoryOwnership 2026-06-02-17:19:
   * Running Sessions may show gxserver-backed terminal rows and macOS-local panes in one modal. Carry ownership on the contract so the UI and external consumers can label local-only rows instead of treating every row as shared daemon state.
   */
  ownership?: "gxserver" | "local";
  restoreState: "live" | "replayed";
  rows: number;
  sessionId: string;
  shell: string;
  startedAt: string;
  status: "starting" | "running" | "exited" | "error" | "disconnected";
  title?: string;
  workspaceId: string;
};

export type SidebarDaemonSessionsStateMessage = {
  daemon?: SidebarDaemonInfo;
  errorMessage?: string;
  sessions: SidebarDaemonSessionItem[];
  type: "daemonSessionsState";
};

export type SidebarPromptGitCommitMessage = {
  /**
   * CDXC:PromptAgents 2026-05-29-10:53:
   * Git commit review, Multiple Commits, Release, and generated rename/title flows
   * must carry the user-selected prompt agent explicitly. Modal-specific choices
   * are remembered by the modal host, while Settings default-agent changes clear
   * those remembered choices so every modal returns to the new default.
   */
  action: SidebarGitAction;
  agentId?: string;
  branch?: string | null;
  changedFiles?: SidebarGitChangedFile[];
  confirmLabel: string;
  deleteWorktreeAfterDefault?: boolean;
  description: string;
  isWorktree?: boolean;
  isDefaultRef?: boolean;
  requestId: string;
  showCommitMessage?: boolean;
  suggestedBody?: string;
  suggestedSubject: string;
  type: "promptGitCommit";
  worktreeName?: string;
};

export type SidebarGitFileDiffMessage = {
  /*
  CDXC:GitReview 2026-06-24-15:22:
  Reused SidebarApp commit review can run outside the native app-modal host.
  Return selected-file diffs through a request-scoped shared message so non-native hosts can fill the inline review pane without opening files, trusting renderer paths as authority, or adding GPUI-only UI.
  */
  draft: SidebarGitFileDiffDraft;
  requestId: string;
  type: "sidebarGitFileDiff";
};

export type SidebarGitPreferenceScope = {
  /*
  CDXC:GPUIRemoteGit 2026-06-24-18:22:
  Git preference writes are project-scoped when the shared UI knows the owning project row. Carry a trusted group id or machine-scoped project id with preference changes so GPUI can route remote writes through the owning gxserver tunnel instead of inferring from the active local project, labels, or DOM text.
  */
  groupId?: string;
  projectId?: string;
};

export type SidebarGhostexFolderStat = {
  name: string;
  path: string;
  sizeBytes: number;
};

/**
 * CDXC:SettingsStorage 2026-05-09-15:25
 * Settings exposes Ghostex data-directory usage only after the user scrolls to the
 * bottom of the modal. The native sidebar sends per-folder byte counts back as
 * a sidebar message so the full-window modal can render stats without owning
 * filesystem access or accepting client-provided paths.
 */
export type SidebarGhostexFolderStatsMessage = {
  errorMessage?: string;
  folderPath: string;
  folders: SidebarGhostexFolderStat[];
  generatedAt: string;
  totalBytes: number;
  type: "ghostexFolderStats";
};

export type SidebarPluginSettingsItem = {
  canReinstall: boolean;
  errorMessage?: string;
  id: "code" | "kanban" | "cef";
  sizeBytes: number;
  status: "installed" | "notInstalled" | "checking" | "downloading" | "verifying" | "installing" | "finishing" | "failed";
  statusLabel: string;
  version: string;
};

/** Native component-store state shown by Settings -> Plugins. */
export type SidebarPluginSettingsStatusMessage = {
  plugins: SidebarPluginSettingsItem[];
  type: "pluginSettingsStatus";
};

/**
 * CDXC:AppModals 2026-04-28-16:18
 * User-input flows must not use VS Code input boxes, quick picks, or modal
 * editors. Extension-initiated prompts are represented as sidebar messages so
 * the existing React modal host owns rendering and styling.
 */
export type SidebarShowSessionRenameModalMessage = {
  initialTitle: string;
  sessionAgentIcon?: string;
  sessionId: string;
  type: "showSessionRenameModal";
};

export type SidebarPreviousSessionsResultMessage = {
  cursor?: string;
  previousSessions: SidebarPreviousSessionItem[];
  query?: string;
  requestId: string;
  type: "previousSessionsResult";
};

/*
 * CDXC:StashedPrompts 2026-07-29:
 * Answer to `requestStashedPrompts`, correlated by requestId. Rows carry
 * user-authored prompt bodies from gxserver, so hosts must forward them to the
 * Prompts modal verbatim and never log or persist them outside that surface.
 */
export type SidebarStashedPromptsResultMessage = {
  prompts: GxserverStashedPrompt[];
  requestId: string;
  type: "stashedPromptsResult";
};

/**
 * Result of creating a prompt directly from the saved-prompts modal. The host
 * returns the canonical gxserver row so the modal never has to invent ids,
 * timestamps, or project presentation metadata optimistically.
 */
export type SidebarSaveStashedPromptResultMessage = {
  error?: string;
  ok: boolean;
  prompt?: GxserverStashedPrompt;
  requestId: string;
  type: "saveStashedPromptResult";
};

/*
 * CDXC:SidebarV2Worktree 2026-07-29:
 * Answers to the two V2 worktree commands, correlated by the `requestId` the
 * sidebar minted. They exist ONLY to end a pending state: the created session
 * itself arrives the normal way, as a presentation delta, and the host is the
 * one that focuses it. `ok: false` carries a short, already-sanitized reason
 * for the popover's inline error line — never raw git or daemon output.
 */
export type SidebarWorktreeSessionResultMessage = {
  branch?: string;
  error?: string;
  ok: boolean;
  requestId: string;
  /** Sidebar-scoped id of the created session, when the host published one. */
  sessionId?: string;
  type: "worktreeSessionResult";
  worktreePath?: string;
};

/**
 * `dirty: true` with `removed: false` is a REFUSAL, not a failure: the checkout
 * has uncommitted work and the prompt re-asks with a force option.
 */
export type SidebarSessionWorktreeRemovalResultMessage = {
  dirty?: boolean;
  error?: string;
  ok: boolean;
  removed: boolean;
  requestId: string;
  type: "sessionWorktreeRemovalResult";
  warnings?: string[];
  worktreePath: string;
};

export type SidebarRecentProjectsResultMessage = {
  machineId?: string;
  recentProjects: SidebarRecentProject[];
  type: "recentProjectsResult";
};

export type SidebarRemoteMachineStatusMessage = {
  machineId: string;
  /**
   * CDXC:GPUIRemoteConnectFeedback 2026-07-12:
   * Optional sanitized failure summary authored by the native host (the same
   * text as the failure toast) so the remote header's error control can explain
   * why a connect attempt failed. Never raw SSH/daemon output.
   */
  message?: string;
  /**
   * CDXC:GPUIRemoteConnectFeedback 2026-07-12:
   * The union now names the granular native connect states that hosts were
   * already sending as raw strings, so the sidebar can show real progress
   * ("Installing…", "Downloading…") and per-cause failure text instead of
   * collapsing everything that is not "connected".
   */
  state:
    | "connecting"
    | "connected"
    | "disconnected"
    | "downloadingRemoteServerPackage"
    | "installApprovalRequired"
    | "installFailed"
    | "installing"
    | "invalid"
    | "keychainFailed"
    | "presentationStreamFailed"
    | "presentationSubscribeFailed"
    | "sshFailed"
    | "tokenUnavailable"
    | "tunnelFailed"
    | "unsupported"
    | "unsupportedRemotePlatform"
    | "failed";
  type: "remoteMachineStatus";
};

export type SidebarNativeHotkeyMessage = {
  /**
   * CDXC:Hotkeys 2026-06-05-20:53:
   * AppKit owns Cmd+number while terminal panes have focus, then forwards the shared hotkey action id into the sidebar so React can resolve session slots from the currently rendered row order, including collapsed-project filtering.
   */
  actionId: ghostexHotkeyActionId;
  type: "nativeHotkey";
};

/**
 * CDXC:AppIconPicker 2026-06-25-21:50:
 * One available Dock/app-switcher icon as reported by native. thumbnailDataUrl
 * is a self-contained data: URL so the picker grid renders without native file
 * reads, and `selected` mirrors which entry native currently has applied.
 */
export type SidebarAppIconInfo = {
  id: string;
  name: string;
  thumbnailDataUrl: string;
  selected: boolean;
};

/**
 * CDXC:AppIconPicker 2026-06-25-21:50:
 * Native -> Settings App Icon state. Already trimmed by native to the newest 10
 * icons plus the selected one. `ok: false` with `error` describes a failed list
 * or swap; the sidebar persists appIconSourceId only when ok is true.
 */
export type SidebarAppIconStateMessage = {
  error: string | null;
  icons: SidebarAppIconInfo[];
  ok: boolean;
  selectedId: string;
  type: "appIconState";
};

export type SidebarGpuiProjectSlotHotkeyMessage = {
  /**
   * CDXC:GPUIProjectHotkeys 2026-06-26-23:42:
   * GPUI project slot hotkeys resolve locally in SidebarApp because SidebarApp owns the rendered Projects row order. Carry only the 1-based slot number so host payloads do not expose paths, titles, session ids, command text, URLs, or project metadata.
   */
  slotNumber: 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9;
  type: "gpuiProjectSlotHotkey";
};

export type ExtensionToSidebarMessage =
  | SidebarHydrateMessage
  | SidebarSessionStateMessage
  | SidebarNativeHotkeyMessage
  | SidebarGpuiProjectSlotHotkeyMessage
  | AgentsHubCatalogMessage
  | AgentsHubFileContentMessage
  | SidebarSessionPresentationChangedMessage
  | SidebarGroupsChangedMessage
  | SidebarProjectCollectionsChangedMessage
  | SidebarHudChangedMessage
  | SidebarPlayCompletionSoundMessage
  | SidebarOrderSyncResultMessage
  | SidebarCommandRunStateChangedMessage
  | SidebarCommandRunStateClearedMessage
  | SidebarRevealSessionMessage
  | SidebarDaemonSessionsStateMessage
  | SidebarPromptGitCommitMessage
  | SidebarGitFileDiffMessage
  | SidebarGhostexFolderStatsMessage
  | SidebarPluginSettingsStatusMessage
  | SidebarAgentHookStatusMessage
  | SidebarGhostexCliStatusMessage
  | SidebarOSIntegrationStatusMessage
  | SidebarShowSessionRenameModalMessage
  | SidebarPreviousSessionsResultMessage
  | SidebarStashedPromptsResultMessage
  | SidebarSaveStashedPromptResultMessage
  | SidebarWorktreeSessionResultMessage
  | SidebarSessionWorktreeRemovalResultMessage
  | SidebarRecentProjectsResultMessage
  | SidebarRemoteMachineStatusMessage
  // CDXC:AppIconPicker 2026-06-25-21:50: Native pushes App Icon list/selection state into Settings.
  | SidebarAppIconStateMessage;

/**
 * CDXC:AddProject 2026-07-30:
 * The operations the shared add-project dialog can ask its host to perform.
 * Each one maps to exactly one gxserver endpoint, and the mapping lives in the
 * host (gpui's Rust bridge, ghostex-web's rpcForMachine) rather than in the
 * dialog, so no surface has to know an endpoint path to render this dialog.
 * `listMachines` is the exception: it is answered from the host's own machine
 * registry without any daemon round trip.
 */
export type SidebarAddProjectDialogOperation =
  | "add"
  | "browse"
  | "cancelCloneJob"
  | "createDirectory"
  | "discoverSourceControl"
  | "listMachines"
  | "lookupRepository"
  | "previewClone"
  | "readCloneJob"
  | "startClone";

/**
 * CDXC:AddProject 2026-07-30:
 * The complete set of fields any add-project operation may carry. Keeping it
 * one flat bounded record (instead of a per-operation union) is what lets the
 * host validate it field by field against the operation it received, and makes
 * it impossible for a new field to reach a daemon without being named here.
 */
export type SidebarAddProjectDialogRequestParams = {
  readonly branchName?: string;
  readonly cloneMainOnly?: boolean;
  readonly createIfMissing?: boolean;
  readonly cwd?: string;
  readonly destinationPath?: string;
  readonly jobId?: string;
  /** New-folder step: the single path segment to create under `parentPath`. */
  readonly name?: string;
  readonly parentPath?: string;
  readonly partialPath?: string;
  readonly path?: string;
  readonly provider?: string;
  readonly remoteUrl?: string;
  readonly repository?: string;
  readonly shallowClone?: boolean;
};

export type SidebarToExtensionMessage =
  | {
      /**
       * CDXC:GxserverBootstrap 2026-05-31-03:56:
       * The gxserver failure toast needs a Retry action that returns to the
       * trusted sidebar command router, then native performs the daemon restart.
       */
      type: "retryGxserverStart";
    }
  | {
      type: "openSettings";
    }
  | {
      /**
       * CDXC:SidebarDiscord 2026-05-27-05:04:
       * Sidebar surfaces can link to the public Ghostex Discord for support,
       * questions, and contributors. Native owns URL opening so the sidebar
       * does not depend on webview popup behavior.
       */
      type: "openExternalUrl";
      url: string;
    }
  | {
      /**
       * CDXC:SidebarTopChrome 2026-06-29-01:43:
       * The sidebar Keep Awake dropdown moved into top chrome but must command the existing titlebar runtime owner instead of duplicating caffeinate lifecycle state in the sidebar renderer.
       */
      action: "start";
      durationMinutes: KeepAwakeDurationMinutes;
      type: "runTitlebarKeepAwakeCommand";
    }
  | {
      action: "stop";
      type: "runTitlebarKeepAwakeCommand";
    }
  | {
      /**
       * CDXC:SettingsStorage 2026-05-09-15:25
       * The settings modal can request Ghostex folder stats lazily, but native
       * resolves the folder path itself and never trusts a path from React.
       */
      type: "requestGhostexFolderStats" | "openGhostexFolder";
    }
  | {
      /**
       * CDXC:AgentHookSettings 2026-05-23-10:05:
       * Settings -> Agents can refresh hook status and trigger the existing hook installer, but native remains the owner of config paths, executable checks, and hook-file mutation.
       *
       * CDXC:FirstLaunchSetup 2026-06-18-02:38:
       * First launch narrows hook setup to Codex, Claude, and Pi by passing
       * agentIds while Settings can omit agentIds to inspect the full supported
       * provider set.
       */
      type:
        | "requestAgentHookStatus"
        | "installAgentHooks"
        | "uninstallAgentHooks";
      agentIds?: readonly string[];
    }
  | {
      /**
       * CDXC:FirstLaunchSetup 2026-05-26-17:12:
       * First launch CLI setup must distinguish a missing CLI from an app that
       * was already installed through Homebrew. Native owns PATH inspection so
       * the production modal and Storybook mock can share the same UI contract.
       */
      type: "requestGhostexCliStatus";
    }
  | {
      /**
       * CDXC:IntegrationsSetup 2026-05-27-04:17:
       * First launch and Settings -> Integrations expose one-click install
       * actions for optional integrations. Native runs the actual commands and
       * refreshes the shared integration status afterward.
       */
      type:
        | "installGhostexCli"
        | "installBrowserControl"
        | "installBrowserUseSkill"
        | "installComputerUseSkill"
        | "installAgentOrchestrationSkill"
        | "installFable56OrchestrationSkill"
        | "installFindPrevSessionSkill"
        | "installGenerateTitleSkill"
        | "installMoveCodexSessionSkill"
        | "uninstallBundledAgentSkills"
        | "installCuaDriver";
    }
  | {
      /**
       * CDXC:AgentSkills 2026-07-29:
       * Per-row uninstall identifies one catalog-owned bundled skill. Native
       * maps this closed ID to a fixed directory instead of accepting a path.
       */
      skillId: BundledGhostexAgentSkillId;
      type: "uninstallBundledAgentSkill";
    }
  | {
      /**
       * CDXC:OSIntegration 2026-05-27-18:06:
       * Settings exposes explicit OS default actions. Installing Ghostex only
       * registers it as an available handler; default editor, terminal-link,
       * and script-runner ownership changes happen only through this command.
       */
      target: "editor" | "terminalLinks" | "scriptRunner" | "all";
      type: "setOSIntegrationDefaults";
    }
  | {
      type: "requestOSIntegrationStatus";
    }
  | {
      type: "requestPluginSettingsStatus";
    }
  | {
      pluginId: SidebarPluginSettingsItem["id"];
      type: "reinstallPlugin";
    }
  | {
      source?: ghostexSettingsUpdateSource;
      settings: ghostexSettings;
      type: "updateSettings";
    }
  | {
      baseRevision?: number;
      patch: ghostexSettingsPatch;
      source: ghostexSettingsUpdateSource;
      type: "updateSettingsPatch";
    }
  | {
      /*
      CDXC:PortlessSetupModal 2026-06-23-13:42:
      Portless setup prompts run in a separate app-modal child-window document,
      and native logs modal sidebar commands as JSON. Keep this boundary
      metadata-only: admin actions carry action/protocol/request id, Disable
      carries one boolean, and dismissals carry only intent so full settings,
      project data, paths, domains, URLs, and command text never cross here.
      */
      action: Extract<NativePortlessAdminInstallAction, "install" | "reconfigure">;
      protocol: NativePortlessProtocol;
      requestId: string;
      type: "runPortlessSetupPromptAdminAction";
    }
  | {
      /*
      CDXC:PortlessSettings 2026-06-23-03:47:
      Settings -> Projects exposes explicit Portless setup actions outside the
      setup prompt. Keep this command metadata-only: install/reconfigure/retry
      carry the selected HTTP/HTTPS mode, remove carries only intent, and
      native still owns privileged execution and sanitized results.
      */
      action: NativePortlessAdminInstallAction;
      protocol: NativePortlessProtocol;
      requestId: string;
      type: "runPortlessSettingsAdminAction";
    }
  | {
      action: "remove";
      requestId: string;
      type: "runPortlessSettingsAdminAction";
    }
  | {
      type: "postponePortlessSetupPrompt" | "cancelPortlessSetupPrompt";
    }
  | {
      enabled: false;
      type: "setPortlessEnabled";
    }
  | {
      /**
       * CDXC:RemoteMachines 2026-06-09-18:23:
       * Remote settings can save an SSH password, but the password must cross
       * the webview boundary only as an explicit user action. Native writes it
       * to macOS Keychain and settings store only the saved-password marker.
       */
      password: string;
      remoteMachineId: string;
      type: "saveRemoteMachinePassword";
    }
  | {
      /**
       * CDXC:GhosttySettings 2026-04-30-01:48
       * The settings modal exposes Ghostty-specific actions that are not plain
       * ghostex preference changes: reset managed config keys, apply the
       * recommended config block, open docs, and open the platform config file.
       *
       * CDXC:AccessibilityPermissions 2026-05-08-13:08
       * The same modal action channel also carries a direct open-settings
       * command for macOS Accessibility status. It does not enable attachment
       * or trigger the permission prompt by itself.
       */
      type:
        | "applyRecommendedGhosttySettings"
        | "openAccessibilityPreferences"
        | "openScreenRecordingPreferences"
        | "requestMacOSNotificationPermission"
        | "openMacOSNotificationSettings"
        | "openGhosttyConfigFile"
        | "openGhosttySettingsDocs"
        | "resetGhosttySettingsToDefault";
    }
  | {
      /**
       * CDXC:ExportTranscript 2026-08-20:
       * The Export Transcript result dialog's two host side effects. Neither
       * carries the exported path: the host already holds it from the dialog's
       * own open message, so a modal page can only ever ask it to act on the
       * export it is currently describing. `agentId` picks which configured
       * agent the seeded follow-up conversation runs.
       */
      agentId?: string;
      type: "revealExportedTranscript" | "startExportedTranscriptConversation";
    }
  | {
      /**
       * CDXC:SessionAttentionNotifications 2026-05-11-01:14
       * Settings' test button should exercise the same native attention
       * completion flow as a real agent task without mutating any session.
       */
      type: "testAgentTaskCompletion";
    }
  | {
      /**
       * CDXC:Settings 2026-05-11-02:06
       * Settings sound dropdown preview buttons play only the selected sound,
       * using the same native audio path as real completion alerts while
       * avoiding notification side effects.
       */
      sound: CompletionSoundSetting;
      type: "playCompletionSoundPreview";
    }
  | {
      type: "toggleCompletionBell";
    }
  | {
      type: "cycleSessionPersistenceProvider";
    }
  | {
      delta: -1 | 1;
      type: "adjustTerminalFontSize";
    }
  | {
      type: "refreshDaemonSessions";
    }
  | {
      type: "killTerminalDaemon";
    }
  | {
      type: "killDaemonSession";
      sessionId: string;
      workspaceId: string;
    }
  | {
      type: "moveSidebarToOtherSide";
    }
  | {
      /**
       * CDXC:SidebarCollapse 2026-06-12-02:23:
       * Cmd+B and command-palette execution need a native chrome command that
       * collapses the whole AppKit sidebar, not only React sidebar content.
       */
      type: "toggleSidebarCollapsed";
    }
  | {
      /**
       * CDXC:SidebarContextMenu 2026-05-20-13:05:
       * Session and project context menus notify native when open so clicks on
       * terminal, titlebar, and other non-sidebar surfaces dismiss the menu
       * while the original AppKit click still reaches its target.
       */
      type: "sidebarContextMenuOpened";
    }
  | {
      type: "sidebarContextMenuClosed";
    }
  | {
      /**
       * CDXC:CommandPalette 2026-05-16-08:18:
       * The full-window command palette needs a pet wake/sleep action that
       * reuses the sidebar settings owner instead of mutating pet visibility
       * inside the detached modal host.
       */
      type: "togglePetOverlay";
    }
  | {
      type: "createSession";
    }
  | {
      /**
       * CDXC:CommandPalette 2026-05-15-20:38:
       * Palette selections for built-in Ghostex commands should execute through
       * the same native hotkey action dispatcher as physical shortcuts so the
       * available command list cannot drift from actual app behavior.
       */
      actionId: ghostexHotkeyActionId;
      type: "runGhostexHotkeyAction";
    }
  | {
      /**
       * CDXC:PaneTabs 2026-05-11-11:51
       * The combined sidebar Settings row has a legacy-named secondary terminal
       * action. It targets the currently active project and creates the new
       * terminal as the selected tab in the focused session's tab group so pane
       * sizes and tab groupings remain unchanged.
       */
      type: "createFullWidthTerminalPane";
    }
  | {
      /**
       * CDXC:Chats 2026-05-04-09:30
       * Chats are projectless AI work areas. The native sidebar owns chat
       * folder creation and then opens a normal empty terminal there so agent
       * title/icon detection stays identical to project sessions.
       */
      title?: string;
      type: "createChat";
    }
  | {
      /**
       * CDXC:Plugins 2026-05-08-10:44
       * The top-sidebar Plugins entry opens the skills directory as a Chromium
       * browser pane under Chats, not inside the active project. Keep this
       * separate from generic browser actions because its destination is fixed.
       */
      type: "openPluginsBrowserChat";
    }
  | {
      /**
       * CDXC:Mobile 2026-06-16-00:45:
       * The top-sidebar Mobile entry opens Ghostex's download page as a Chromium
       * browser pane under Chats, matching Plugins' fixed-destination
       * projectless browser-chat behavior while avoiding the active project.
       *
       * CDXC:Mobile 2026-06-16-01:23:
       * Mobile should use the canonical download URL rather than the GitHub
       * README anchor so mobile setup starts from the product site.
       */
      type: "openMobileBrowserChat";
    }
  | {
      /**
       * CDXC:Automations 2026-06-29-15:55:
       * The top-sidebar Automations entry now opens the project Automation page backed by gxserver-rs, but keep the old toast message in the contract for older native bundles during the cutover.
       */
      type: "showAutomationsComingSoonToast";
    }
  | {
      /**
       * CDXC:Automations 2026-06-29-15:55:
       * The sidebar shortcut should open a real first-party Automation page backed by gxserver.
       *
       * CDXC:Automations 2026-06-30-11:05:
       * Sidebar Automations is a Quick-level global page that aggregates automations from all projects. Project-scoped automation access belongs to the titlebar Automate view instead of reusing the Kanban surface.
       */
      type: "openAutomationsPage";
    }
  | {
      /**
       * CDXC:AgentsHub 2026-05-12-09:21
       * Agents Hub runs in the full-window modal host, but profile/file actions
       * still need native filesystem affordances from the sidebar bridge.
       */
      path: string;
      type: "openAgentsHubPathInFinder";
    }
  | {
      /**
       * Agents Hub file rows open in Ghostex's owned Source editor. Hosts must
       * validate the catalog path before handing it to their embedded editor.
       */
      filePath: string;
      type: "openAgentsHubFileInBuiltInEditor";
    }
  | {
      /**
       * CDXC:AgentsHub 2026-05-14-08:29:
       * Agents Hub must show the real files installed on the user's machine, including files owned by Claude/Codex profiles and plugin caches.
       * The modal host requests a fresh native filesystem catalog whenever the Hub opens instead of relying on a bundled placeholder list.
       *
       * CDXC:AgentsHub 2026-06-12-02:53:
       * Catalog requests return metadata only so large profile/plugin trees can
       * paint the Hub immediately without pushing every file buffer through the
       * native process-result bridge.
       */
      type: "requestAgentsHubCatalog";
    }
  | {
      /**
       * CDXC:AgentsHub 2026-06-12-02:53:
       * Agents Hub reads file contents only after selection because the left
       * tree needs metadata, while editor buffers can be large enough to block
       * the modal bridge when loaded for every file at once.
       */
      filePath: string;
      requestId: string;
      type: "requestAgentsHubFileContent";
    }
  | {
      /**
       * CDXC:AgentsHub 2026-05-14-08:27:
       * The Hub modal edits real agent instruction/config files and enables Save only after text changes.
       * Persist the current editor buffer through the native sidebar command contract so the modal host keeps using the same catalog-validated filesystem bridge as the built-in Source action.
       */
      content: string;
      filePath: string;
      type: "saveAgentsHubFile";
    }
  | {
      /**
       * CDXC:Chats 2026-05-08-11:53
       * The reference-style Chats section header has a hover-only browser
       * action beside New Chat. It creates a new projectless chat and opens a
       * browser pane there, without requiring a concrete chat group id.
       */
      type: "openBrowserChat";
    }
  | {
      type: "openBrowser";
    }
  | {
      /**
       * CDXC:ChromiumBrowserPanes 2026-05-27-07:24
       * Browser actions always create in-workspace browser panes now that the
       * legacy Chrome Canary attachment route has been removed.
       */
      url?: string;
      type: "openBrowserPane";
    }
  | {
      /**
       * CDXC:ProjectGroups 2026-05-06-18:42
       * Project headers expose New Browser beside the create-session control.
       * Carry the group id so native can focus that project/group before
       * creating the browser pane.
       */
      groupId: string;
      type: "openBrowserPaneInGroup";
    }
  | {
      type: "openWorkspaceWelcome";
    }
  | {
      /*
       * CDXC:HighlightedFeatures 2026-06-16-08:17:
       * The titlebar Tips & Tricks panel can open the replayable highlighted
       * features modal. Keep the request in the sidebar command contract so
       * the native sidebar remains the single owner of app modal presentation.
       *
       * CDXC:GhostexTutorialVideo 2026-06-18-05:31:
       * Keep this legacy command for callers, but native routes it to the
       * tutorial video modal so Highlighted Features can remain unused.
       */
      type: "openHighlightedFeatures";
    }
  | {
     /*
      * CDXC:GhostexTutorialVideo 2026-06-18-04:49:
      * Help surfaces need a dedicated request for the one-page Ghostex tutorial
       * video modal.
       *
       * CDXC:GhostexTutorialVideo 2026-06-18-05:31:
       * Current Features/help entry points should open this video modal while
       * leaving the old Highlighted Features modal unused.
      */
     type: "openGhostexTutorialVideo";
   }
  | {
      /**
       * CDXC:NativeWorkspacePicker 2026-05-08-18:45
       * The reference Projects header add button should open the trusted native
       * folder picker.
       */
      type: "pickWorkspaceFolder";
    }
  | {
      /** The missing-folder child modal asks native to locate this durable project again. */
      projectId: string;
      type: "pickReplacementProjectFolder";
    }
  | {
      /*
       * CDXC:CommandPalette 2026-06-18-03:46:
       * Cmd+Shift+P exposes the main-window Open In actions. The modal host
       * sends only a target id; native resolves it against the active project,
       * current Settings visibility, and detected target availability so the
       * palette does not carry workspace paths or duplicate titlebar launch
       * rules.
       */
      targetId: string;
      type: "openCurrentProjectInTarget";
    }
  | {
      /*
       * CDXC:CommandPalette 2026-06-18-03:46:
       * Open Current Project in Finder is a global command-palette action that
       * mirrors the main titlebar Open In affordance without sending a raw path
       * through React.
       */
      type: "openCurrentProjectInFinder";
    }
  | {
      /**
       * CDXC:RemoteMachines 2026-06-02-23:47:
       * Disconnected Remote sidebar sections stay visible and expose only Reload. Native owns the SSH reconnect/start/install gxserver flow, so React sends the saved machine id instead of handling SSH details in the sidebar.
       *
       * CDXC:RemoteMachines 2026-06-02-23:38:
       * Missing gxserver installation requires explicit React modal approval.
       * The approval flag is carried back through the same reconnect command
       * so native can upload/install only after the user accepts.
       */
      installApproved?: boolean;
      remoteMachineId: string;
      type: "reconnectRemoteMachine";
    }
  | {
      /**
       * CDXC:RemoteMachines 2026-08-19:
       * The Remote settings install action reads as Install for a machine that
       * has no gxserver yet and as Update for one that already runs it, so
       * React asks native whether the saved machine already has a gxserver
       * package and which version it is. The request carries only the bounded
       * machine id; native owns the SSH probe and answers with a
       * `remoteGxserverInstallState` host message.
       */
      remoteMachineId: string;
      type: "probeRemoteGxserverInstall";
    }
  | {
      /**
       * CDXC:RemoteProjectPicker 2026-06-02-23:22:
       * Remote Add Project uses a path-aware directory picker, but every
       * browse request is machine-scoped. Native must route it to that
       * machine's gxserver after SSH reconnect/token setup instead of exposing
       * local filesystem browsing for remote machines.
       */
      partialPath: string;
      remoteMachineId: string;
      requestId: string;
      type: "browseRemoteProjectDirectories";
    }
  | {
      /**
       * CDXC:RemoteProjects 2026-06-03-00:18:
       * Adding a remote project is not the local Add Project command. Carry the
       * remote machine id with the selected path so native can add the project
       * through that machine's gxserver and later render it under that machine's
       * sidebar section.
       */
      path: string;
      remoteMachineId: string;
      requestId: string;
      type: "addRemoteProjectPath";
    }
  | {
      /**
       * CDXC:AddProject 2026-07-30:
       * Every server round trip the shared add-project dialog performs travels
       * on this one request. `machineId` is the whole routing vocabulary — the
       * host resolves it to the local daemon or to that machine's tunnel — so
       * the dialog never learns a host, a port, or a token. The host answers on
       * its own result channel keyed by `requestId`; nothing is optimistic, and
       * a dismissed dialog simply abandons the answer.
       */
      machineId?: string;
      operation: SidebarAddProjectDialogOperation;
      params?: SidebarAddProjectDialogRequestParams;
      requestId: string;
      type: "addProjectDialogRequest";
    }
  | {
      type: "createSessionInGroup";
      groupId: string;
    }
  | {
      /**
       * Project-heading terminal creation is a distinct intent from the generic
       * subgroup add button so the GPUI Windows host can own the WSL
       * create-and-attach sequence without changing other group creation.
       */
      type: "createProjectTerminal";
      groupId: string;
    }
  | {
      type: "focusGroup";
      groupId: string;
    }
  | {
      type: "toggleFullscreenSession";
    }
  | {
      /*
       * CDXC:SidebarSessionFocus 2026-06-29-02:04:
       * The macOS focused-pane border must be preserved only for real sidebar session-row clicks. Send a hover-scoped native hint from the session card so AppKit's pre-dispatch mouseDown path can distinguish session focus clicks from other sidebar chrome before WebKit temporarily becomes first responder.
       */
      isSessionCard: boolean;
      type: "setSidebarSessionFocusBorderHandoffHitTarget";
    }
  | {
      /*
       * CDXC:SidebarSessionFocus 2026-06-29-02:04:
       * If a session-row mouseDown is actually a child control, modified click, or context-menu path, cancel the pre-dispatch border handoff so only session focus selection keeps the old border during the AppKit sidebar responder gap.
       */
      type: "cancelSidebarSessionFocusBorderHandoff";
    }
  | {
      type: "focusSession";
      sessionId: string;
    }
  | {
      /**
       * CDXC:SessionFocusMode 2026-05-23-09:28:
       * Session-card and pane-tab Focus is a reversible zoom for the clicked
       * session's pane tab group. The native/sidebar controller owns this
       * command because it must also switch from Code/Browser/Project/Manage
       * surfaces back to Agents while remembering the prior surface for unfocus.
       */
      type: "focusSessionMode";
      sessionId: string;
    }
  | {
      type: "promptRenameSession";
      sessionId: string;
    }
  | {
      type: "restartSession";
      sessionId: string;
    }
  | {
      type: "renameSession";
      agentId?: string;
      sessionId: string;
      title: string;
      /**
       * CDXC:SessionNaming 2026-05-09-17:25
       * Generate Title reuses renameSession with the saved 1st user message,
       * but must force controller-side title generation even when that message
       * is shorter than the rename modal's 70-character Generate Name threshold.
       */
      shouldGenerateTitle?: boolean;
    }
  | {
      type: "renameGroup";
      groupId: string;
      title: string;
    }
  | {
      /**
       * CDXC:WorktreeDelete 2026-05-28-07:46:
       * Combined project rows render worktrees as project headers, so project-name edits and delete confirmation prompts must route through trusted group ids instead of trusting DOM-provided paths.
       */
      type: "renameWorkspaceProjectForGroup";
      groupId: string;
      title: string;
    }
  | {
      type: "copyWorkspaceProjectPathForGroup";
      groupId: string;
    }
  | {
      type: "restoreRecentProject";
      projectId: string;
    }
  | {
      /**
       * CDXC:RecentProjects 2026-05-27-07:04:
       * Recent Projects rows have their own right-click menu because they are
       * parked projects without a rendered project group id. Route filesystem
       * and removal actions by trusted project id so the sidebar does not send
       * raw paths back to native.
       */
      type:
        | "copyRecentProjectPath"
        | "openRecentProjectInFinder"
        | "openRecentProjectTerminal"
        | "removeRecentProject";
      projectId: string;
    }
  | {
      /**
       * CDXC:WorkspaceActions 2026-05-04-08:22
       * Combined-mode project cards expose native open actions from the
       * right-click menu. The native sidebar resolves the group id to its
       * trusted stored workspace path instead of accepting a client path.
       */
      type: "openWorkspaceProjectInFinderForGroup" | "openWorkspaceProjectInIdeForGroup";
      groupId: string;
    }
  | {
      /**
       * CDXC:EditorPanes 2026-05-06-14:21
       * Project editor buttons are trusted group-scoped commands. Native
       * resolves the group id to its stored project path before launching the
       * embedded code-server editor or refreshing its diff stats.
       *
       * CDXC:EditorPanes 2026-05-06-18:55
       * The editor card also accepts middle-click close, but the editor is not a
       * session; route close through the same trusted project/group resolver.
       */
      type:
        | "closeWorkspaceProjectEditorForGroup"
        | "openWorkspaceProjectEditorForGroup"
        | "refreshWorkspaceProjectDiffForGroup";
      groupId: string;
    }
  | {
      /**
       * CDXC:SidebarActions 2026-05-05-02:47
       * Sidebar Open In dropdowns know the active project but not a group id.
       * Route these commands through the native sidebar so stored workspace
       * paths remain trusted on the app side instead of being accepted from DOM.
       */
      type: "openActiveWorkspaceProjectInFinder";
    }
  | {
      /**
       * CDXC:SidebarActions 2026-05-05-03:11
       * The sidebar Open In dropdown lists explicit IDE targets. The selected
       * target must travel with the active-project open command instead of
       * being inferred from Settings, so choosing VS Code or Zed immediately
       * opens the project in that exact app.
       */
      targetApp: Extract<WorkspaceIdeTargetApp, "vscode" | "zed">;
      type: "openActiveWorkspaceProjectInIde";
    }
  | {
      /**
       * CDXC:WorkspaceTheme 2026-05-05-05:01
       * Preset theme selection must actively clear a previous Custom color.
       * `themeColor: null` is the sidebar-to-native signal that the custom
       * override is being removed, so icon and project-header tinting cannot
       * keep using stale custom CSS variables after a preset is selected.
       */
      type: "setWorkspaceProjectThemeForGroup";
      groupId: string;
      theme?: SidebarTheme;
      themeColor?: string | null;
    }
  | {
      /**
       * CDXC:RecentProjects 2026-05-04-14:25
       * Combined project context menus close projects into the Recent Projects
       * drawer instead of deleting their stored sessions. Remove remains the
       * explicit project-delete path.
       */
      type: "closeWorkspaceProjectForGroup" | "removeWorkspaceProjectForGroup";
      groupId: string;
    }
  | {
      /**
       * CDXC:WorktreeDelete 2026-06-02-13:41:
       * Delete Worktree first asks gxserver for a fresh Git status summary,
       * then the native sidebar opens the full-window confirmation modal before
       * any checkout directory is removed.
       */
      type: "promptDeleteWorktreeForGroup";
      groupId: string;
    }
  | {
      /**
       * CDXC:WorktreeRename 2026-08-09-18:40:
       * Rename Worktree collects the worktree's git state — populated
       * submodules, lock, pushed branch, uncommitted changes, live sessions —
       * before the native modal opens, because those answers decide whether the
       * rename can happen at all and the modal has no way to ask for them
       * itself. This is separate from the label-only `Rename` above it, which
       * changes the project row's title and nothing on disk.
       */
      type: "promptRenameWorktreeForGroup";
      groupId: string;
    }
  | {
      type: "closeGroup";
      groupId: string;
    }
  | {
      type: "closeSession";
      sessionId: string;
    }
  | {
      type: "closeSessions";
      sessionIds: string[];
    }
  | {
      type: "setSessionSleeping";
      sessionId: string;
      sleeping: boolean;
    }
  | {
      type: "setSessionsSleeping";
      sessionIds: string[];
      sleeping: boolean;
      /**
       * CDXC:NativeSidebarBulkActions 2026-06-13-12:59:
       * Bulk sleep diagnostics need to distinguish Sleep below from other
       * setSessionsSleeping callers without logging session ids, titles, paths,
       * commands, or user text. Keep this as an enum-like action source only.
       */
      source?: "sleepBelow";
    }
  | {
      favorite: boolean;
      type: "setSessionFavorite";
      sessionId: string;
    }
  | {
      sessionId: string;
      sessionTag?: SidebarSessionTag | null;
      type: "setSessionTag";
    }
  | {
      pinned: boolean;
      type: "setSessionPinned";
      sessionId: string;
    }
  | {
      /**
       * CDXC:SidebarV2Lifecycle 2026-07-29:
       * Sidebar V2's settle/snooze commands. They carry only the sidebar
       * session id, exactly like `setSessionPinned` and `setSessionSleeping`:
       * the host already owns the id -> (machine, project, session) mapping and
       * must not accept a project scope from the renderer.
       *
       * Every command is idempotent server-side, and the UI is NOT optimistic:
       * the presentation delta that follows the write is what moves the row.
       */
      sessionId: string;
      type: "settleSession" | "unsettleSession" | "unsnoozeSession";
    }
  | {
      sessionId: string;
      /** ISO wake time; gxserver rejects anything not strictly in the future. */
      snoozedUntil: string;
      type: "snoozeSession";
    }
  | {
      type: "setGroupSleeping";
      groupId: string;
      sleeping: boolean;
    }
  | {
      /**
       * CDXC:ProjectSleep 2026-05-27-01:50:
       * Combined project rows do not map to one native workspace group. Their
       * context-menu sleep action must be project-scoped and must only sleep
       * inactive sessions so running, working, and attention sessions stay
       * awake.
       */
      type: "sleepInactiveProjectSessions";
      groupId: string;
    }
  | {
      /**
       * CDXC:ProjectClose 2026-06-04-23:40:
       * Combined project-row Close inactive is project-scoped, not group-scoped.
       * It closes idle terminal sessions while preserving working and attention
       * sessions, and it must not park the whole project in Recent Projects.
       */
      type: "closeInactiveProjectSessions";
      groupId: string;
    }
  | {
      /**
       * CDXC:ProjectSleep 2026-05-27-02:18:
       * Combined project-row Wake must wake sleeping terminal sessions across
       * every workspace group because the row does not carry a concrete native
       * workspace group id.
       */
      type: "wakeProjectSleepingSessions";
      groupId: string;
    }
  | {
      type: "copyResumeCommand";
      sessionId: string;
    }
  | {
      type: "copyAttachCommand";
      sessionId: string;
    }
  | {
      /**
       * CDXC:SidebarContextMenu 2026-06-11-23:08:
       * The React sidebar builds Copy details text from its rendered session row
       * and sends only that user-requested clipboard payload to native.
       */
      type: "copySessionDetails";
      detailsText: string;
      sessionId: string;
    }
  | {
      /**
       * CDXC:DelayedSend 2026-05-11-11:56
       * Delayed Send schedules an Enter keypress for an already-staged terminal
       * command. The sidebar/modal sends only the trusted session id and delay;
       * native resolves the terminal and uses the existing Enter-key path.
       *
       * CDXC:DelayedSend 2026-08-19:
       * Exactly one trigger travels in this message. `delayMs` is present only
       * for the "after a delay" trigger, so the status triggers cannot be read
       * as a second, conflicting trigger by the daemon endpoint.
       */
      delayMs?: number;
      sendWhenAllProjectSessionsStop?: boolean;
      sendWhenAgentStops?: boolean;
      sessionId: string;
      type: "scheduleDelayedSend";
    }
  | {
      /**
       * CDXC:DelayedSend 2026-05-17-03:14
       * Users must be able to cancel a scheduled delayed send from the same
       * modal/sidebar affordance that shows the remaining countdown.
       */
      sessionId: string;
      type: "cancelDelayedSend";
    }
  | {
      /**
       * CDXC:CloseAfterDone 2026-06-15-21:00:
       * Session context menus toggle Close After Done without sending titles,
       * commands, or terminal content. Native owns the actual three-minute Done
       * stability timer and routes closure through the existing close path.
       */
      sessionId: string;
      type: "toggleCloseAfterDone";
    }
  | {
      type: "forkSession";
      sessionId: string;
    }
  | {
      type: "fullReloadSession";
      sessionId: string;
    }
  | {
      /**
       * CDXC:PanePopOut 2026-05-19-10:15:
       * Browser and agent session cards expose Pop Out Pane in the sidebar
       * context menu. The controller toggles pop-out presentation from the
       * current session record, matching the focused-pane hotkey behavior.
       */
      sessionId: string;
      type: "popOutPane";
    }
  | {
      type: "fullReloadGroup";
      groupId: string;
    }
  | {
      /**
       * CDXC:ProjectReload 2026-05-27-02:18:
       * Combined project rows need a project-scoped full reload because their
       * sidebar group id is synthetic. Reload only idle attached zmx terminals
       * so project-level reload never interrupts working or attention sessions
       * and never tries to restore sleeping/detached history records.
       */
      type: "fullReloadProjectZmxSessions";
      groupId: string;
    }
  | {
      /**
       * CDXC:BrowserPanes 2026-05-02-06:35
       * Browser session cards expose pane-specific controls copied from the
       * native browser workflow: DevTools, the Settings-selected feedback tool,
       * and profile selection. The native host owns the macOS UI and WebKit/CEF
       * work.
       */
      action: "devtools" | "feedback-tool" | "profile-picker";
      sessionId: string;
      type: "runBrowserPaneAction";
    }
  | {
      /**
       * CDXC:GxserverPresentationSearch 2026-06-01-15:08:
       * Previous Sessions is loaded on demand from gxserver after the presentation hard cutover. React sends debounced metadata queries through native so startup no longer hydrates all previous-session history into the sidebar store.
       */
      limit?: number;
      cursor?: string;
      query?: string;
      requestId: string;
      sessionTags?: SidebarSessionTagFilter[];
      type: "requestPreviousSessions";
    }
  | {
      machineId?: string;
      type: "requestRecentProjects";
    }
  | {
      historyId: string;
      type: "restorePreviousSession";
    }
  | {
      historyId: string;
      type: "deletePreviousSession";
    }
  | {
      /**
       * CDXC:PreviousSessions 2026-05-29-12:36:
       * Previous Sessions needs a direct text-search launcher. Keep it as an
       * explicit sidebar command so the Search row can start a fresh terminal
       * running `gx f`.
       *
       * CDXC:PreviousSessions 2026-05-29-20:32:
       * Search by Text must create that terminal in the currently active
       * project, not in the Quick/projectless terminal area.
       *
       * CDXC:PreviousSessions 2026-06-13-01:09:
       * The Previous Sessions modal no longer renders launch buttons, and the
       * agent-based previous-session prompt path has been removed. This command
       * remains the direct Search row launcher only.
       */
      type: "searchPreviousSessionsByText";
    }
  | {
      content: string;
      type: "saveScratchPad";
    }
  | {
      content: string;
      promptId?: string;
      title: string;
      type: "savePinnedPrompt";
    }
  | {
      /**
       * CDXC:StashedPrompts 2026-07-29:
       * The session Prompts modal loads gxserver-stashed prompt-editor saves on
       * demand. projectId limits the answer to that project plus its worktree
       * family; omitting it returns every stashed prompt.
       */
      projectId?: string;
      requestId: string;
      type: "requestStashedPrompts";
    }
  | {
      promptId: string;
      type: "deleteStashedPrompt";
    }
  | {
      content: string;
      promptId?: string;
      projectId?: string;
      requestId: string;
      sessionId?: string;
      type: "saveStashedPrompt";
    }
  | {
      /**
       * CDXC:StashedPrompts 2026-07-29:
       * Selecting a stashed prompt inserts its text into the named session's
       * active composer without submitting it. The host owns the chat/native
       * input mechanics; the modal only supplies the prompt body and target.
       */
      content: string;
      promptId: string;
      sessionId?: string;
      type: "insertStashedPrompt";
    }
  | {
      type: "moveSessionToGroup";
      groupId: string;
      sessionId: string;
      targetIndex?: number;
    }
  | {
      type: "sidebarDebugLog";
      event: string;
      details?: unknown;
      scenarioId: DiagnosticLoggingScenarioId;
    }
  | {
      type: "createGroupFromSession";
      sessionId: string;
    }
  | {
      type: "createGroup";
      /**
       * CDXC:SidebarContract 2026-07-02-03:49:
       * GPUI can create a group inside a specific project section, while legacy macOS sidebar handlers can still use the active project fallback.
       *
       * Optional sidebar group id identifying the project the new group should
       * belong to. Hosts without it (macOS legacy handler) fall back to the
       * active project.
       */
      groupId?: string;
    }
  | {
      type: "setVisibleCount";
      visibleCount: VisibleSessionCount;
      groupId?: string;
    }
  | {
      type: "setViewMode";
      viewMode: TerminalViewMode;
    }
  | {
      type: "toggleActiveSessionsSortMode";
    }
  | {
      manualSessionIdsByGroup?: Record<string, string[]>;
      sortMode: SidebarActiveSessionsSortMode;
      type: "setActiveSessionsSortMode";
    }
  | {
      type: "syncSessionOrder";
      groupId: string;
      sessionIds: string[];
    }
  | {
      type: "syncGroupOrder";
      groupIds: string[];
    }
  | {
      /*
      CDXC:SidebarProjectCollections 2026-07-18-00:00:
      SidebarApp write-through-syncs its whole project-collection overlay after
      each local edit. The host debounces and pushes the wire state to
      gxserver's /api/updateSidebarProjectCollections; only bounded metadata
      (ids, titles, colors, project membership, ordering) crosses this message.
      */
      state: GxserverSidebarProjectCollectionsState;
      remoteMachineId?: string;
      type: "updateSidebarProjectCollections";
    }
  | {
      /*
      CDXC:GPUICommandPane 2026-06-26-05:11:
      `runSidebarCommand` is a narrow Action selector: renderer messages may provide only the saved command id and optional run mode. Native and GPUI hosts must resolve command text, URLs, saved close-on-exit metadata, cwd/env, paths, output, and launch behavior from trusted command/HUD state.

      CDXC:ProjectActions 2026-08-01:
      Project-row Action buttons add an optional group selector like
      `runSidebarAgent` already carries. The host resolves the group to its
      project, activates that project through the existing focus flow, and only
      then dispatches the trusted launch — the message never gains launch
      metadata or project paths.

      CDXC:GlobalActions 2026-08-07:
      Project rows also render Global Actions flagged showOnProjectRow, so the
      selector must say which list its id belongs to: the two scopes are
      separate id spaces and an id alone cannot pick one. Scope stays optional
      and absent means project, so senders that only ever run Project Actions
      are unchanged. A global selector may still carry the row's group id —
      that names the project the Action runs in, not the list it came from.
      */
      type: "runSidebarCommand";
      commandId: string;
      groupId?: string;
      runMode?: SidebarCommandRunMode;
      scope?: SidebarCommandScope;
    }
  | {
      type: "endSidebarCommandRun";
      commandId: string;
    }
  | {
      action: SidebarGitAction;
      groupId?: string;
      projectId?: string;
      type: "runSidebarGitAction";
    }
  | {
      action: SidebarGitAction;
      groupId?: string;
      projectId?: string;
      type: "setSidebarGitPrimaryAction";
    }
  | {
      /*
      CDXC:GPUISidebarGit 2026-06-24-21:26:
      Git refreshes can originate from reused project-scoped controls, including remote project rows. Carry the optional group/project scope so GPUI refreshes the owning gxserver project instead of falling back to the active local project.
      */
      groupId?: string;
      projectId?: string;
      type: "refreshGitState";
    }
  | {
      enabled: boolean;
      groupId?: string;
      projectId?: string;
      type: "setSidebarGitCommitConfirmationEnabled";
    }
  | {
      enabled: boolean;
      groupId?: string;
      projectId?: string;
      type: "setSidebarGitGenerateCommitBodyEnabled";
    }
  | {
      commitOnNewRef?: boolean;
      deleteWorktreeAfter?: boolean;
      agentId?: string;
      filePaths?: string[];
      message: string;
      requestId: string;
      type: "confirmSidebarGitCommit";
    }
  | {
      deleteWorktreeAfter?: boolean;
      agentId?: string;
      filePaths?: string[];
      message: string;
      requestId: string;
      type: "confirmSidebarGitDirectMerge";
    }
  | {
      agentId?: string;
      requestId: string;
      type: "runSidebarGitMultipleCommits";
    }
  | {
      filePath: string;
      /*
      CDXC:GPUISidebarGit 2026-06-24-15:43:
      Commit-review changed-file opens may include the active review request id so non-native hosts can validate the file against the gxserver-derived review list before native code resolves and opens the project-relative path.

      CDXC:GPUISidebarGit 2026-06-24-21:26:
      Non-review changed-file opens may come from scoped Git controls. Carry the same optional group/project scope as Git actions so GPUI can re-read the owning local or remote gxserver project before opening a file.
      */
      groupId?: string;
      projectId?: string;
      requestId?: string;
      type: "openSidebarGitChangedFile";
    }
  | {
      filePath: string;
      requestId?: string;
      type: "openSidebarGitChangedFileDiff";
    }
  | {
      requestId: string;
      type: "cancelSidebarGitCommit";
    }
  | {
      /**
       * CDXC:WorktreeDelete 2026-06-10-22:56:
       * Delete Worktree confirmation may request branch cleanup after the
       * checkout is removed. Keep only boolean user choices in the sidebar
       * bridge message; native re-resolves branch names before mutating Git.
       */
      deleteLocalBranch?: boolean;
      deleteRemoteBranch?: boolean;
      projectId: string;
      type: "confirmDeleteWorktree";
    }
  | {
      groupId: string;
      type: "commitWorktreeBeforeDelete";
    }
  | {
      /**
       * CDXC:WorktreeRename 2026-08-09-18:40:
       * One typed name plus one boolean. gxserver derives the destination folder
       * (`<ParentFolder>-<slug>`) and the project label from it, so the modal
       * never names a path; `renameBranch` stays opt-in because renaming a
       * pushed branch breaks the user's next push.
       */
      name: string;
      projectId: string;
      renameBranch?: boolean;
      type: "confirmRenameWorktree";
    }
  | {
      type: "saveSidebarCommand";
      actionType: SidebarActionType;
      closeTerminalOnExit: boolean;
      commandId?: string;
      icon?: SidebarCommandIcon;
      links?: SidebarCommandLink[];
      name: string;
      playCompletionSound: boolean;
      showOnProjectRow: boolean;
      command?: string;
      url?: string;
    }
  | {
      type: "deleteSidebarCommand";
      commandId: string;
    }
  | {
      requestId: string;
      type: "syncSidebarCommandOrder";
      commandIds: string[];
    }
  /*
   * CDXC:GlobalActions 2026-08-01:
   * Global Actions get their own message types rather than a scope flag on the
   * project ones. A host that predates this feature drops an unknown message
   * type through the unsupported-message path, which is a visible no-op; a host
   * that ignored an unknown scope field would instead write the Global Action
   * into whichever project happened to be active.
   */
  | {
      type: "saveGlobalSidebarCommand";
      actionType: SidebarActionType;
      closeTerminalOnExit: boolean;
      commandId?: string;
      icon?: SidebarCommandIcon;
      links?: SidebarCommandLink[];
      name: string;
      playCompletionSound: boolean;
      /*
       * CDXC:GlobalActions 2026-08-07:
       * Settings offers the project-row toggle on Global Actions too, and
       * gxserver stores it for both lists. The field was missing here, so the
       * host had nothing to forward and every global save wrote the flag back
       * as false — the toggle looked saved and did nothing.
       */
      showOnProjectRow: boolean;
      command?: string;
      url?: string;
    }
  | {
      type: "deleteGlobalSidebarCommand";
      commandId: string;
    }
  | {
      requestId: string;
      type: "syncGlobalSidebarCommandOrder";
      commandIds: string[];
    }
  | {
      type: "runSidebarAgent";
      agentId: string;
      groupId?: string;
    }
  | {
      type: "createProjectWorktree";
      agentId?: string;
      baseBranch?: string;
      existingWorktreeKey?: string;
      existingWorktreePath?: string;
      mode?: "create" | "openExisting";
      prompt?: string;
      projectId?: string;
      projectPath?: string;
      remoteMachineId?: string;
    }
  | {
      type: "requestProjectWorktrees";
      projectId?: string;
      projectPath?: string;
      requestId: string;
      remoteMachineId?: string;
    }
  /*
   * CDXC:SidebarV2Worktree 2026-07-29:
   * Sidebar V2's worktree flow. These mirror gxserver's
   * `/api/createWorktreeSession` and `/api/removeSessionWorktree` one-for-one,
   * with two deliberate differences:
   * - `projectId` is the SIDEBAR project/group id (the same value V2 rows carry
   *   as `projectId`), so the host resolves the owning daemon and gxserver
   *   project itself. The renderer never picks a daemon.
   * - `existingWorktreePath` is flat here and nests into `existingWorktree` on
   *   the wire; the sidebar has one field to fill, the endpoint has one shape.
   * `requestId` exists because the popover has a pending state: the answer
   * comes back as `worktreeSessionResult` / `sessionWorktreeRemovalResult`, and
   * a stale answer must never re-enable a form the user already reopened.
   */
  | {
      agentId?: string;
      baseBranch?: string;
      existingWorktreePath?: string;
      firstPrompt?: string;
      projectId: string;
      requestId: string;
      startFromOrigin?: boolean;
      type: "createWorktreeSession";
    }
  | {
      /** Retry after a `dirty: true` answer: remove the checkout anyway. */
      force?: boolean;
      projectId: string;
      requestId: string;
      type: "removeSessionWorktree";
      worktreePath: string;
    }
  | {
      type: "setProjectWorktreeCommand";
      command: string;
      projectId: string;
    }
  | {
      type: "setProjectBeadsDisplayKey";
      displayKey: string;
      projectId: string;
    }
  | {
      type: "setProjectBeadsDirectory";
      directory: string;
      projectId: string;
    }
  | {
      /*
       * CDXC:DocsRootDirectory 2026-08-09:
       * Absolute folder this project's Docs surface shows in addition to the
       * project's own docs. Blank clears the override so the project inherits
       * the Docs directory Global Default.
       */
      type: "setProjectDocsDirectory";
      directory: string;
      projectId: string;
    }
  | {
      /*
       * CDXC:ProjectSettings 2026-06-17-17:13:
       * The Projects settings selector lists durable project rows, so Settings must be able to remove any selected project through the same native removeProject path used by project headers instead of limiting deletion to sidebar context menus.
       */
      type: "removeProject";
      projectId: string;
    }
  | {
      acceptAllMode?: AgentAcceptAllMode;
      type: "saveSidebarAgent";
      agentId?: string;
      command: string;
      icon?: SidebarAgentIcon;
      name: string;
    }
  | {
      type: "deleteSidebarAgent";
      agentId: string;
    }
  | {
      requestId: string;
      type: "syncSidebarAgentOrder";
      agentIds: string[];
    }
  /**
   * CDXC:AppIconPicker 2026-06-25-21:50:
   * Settings -> App Icon talks to native through these four messages. Native
   * owns the icons folder, file picking, and the live Dock/app-switcher icon;
   * the sidebar only requests state and selections. sourceId is a filename in
   * the icons folder, or "" to restore the default bundled icon. The sidebar
   * waits for an ok appIconState event before persisting appIconSourceId
   * (confirm-before-persist) so a failed native swap never sticks in settings.
   */
  | {
      type: "listAppIcons";
    }
  | {
      type: "setAppIcon";
      sourceId: string;
    }
  | {
      type: "pickAppIconFile";
    }
  | {
      type: "revealAppIconsFolder";
    }
  /**
   * CDXC:TerminalBackgroundImage 2026-08-01:
   * Settings -> Terminal Background Image "Browse" opens a native file dialog
   * host-side; the picked absolute path comes back to the settings modal as a
   * terminalBackgroundImageFilePicked host message and fills the path field.
   */
  | {
      type: "pickTerminalBackgroundImageFile";
    };

export type SidebarHudSnapshot = Pick<
  SessionGridSnapshot,
  | "focusedSessionId"
  | "fullscreenRestoreVisibleCount"
  | "sessions"
  | "visibleCount"
  | "visibleSessionIds"
  | "viewMode"
>;
