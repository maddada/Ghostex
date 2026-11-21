import type {
  GxserverServerId,
  GxserverProjectId,
  GxserverSessionId,
  GxserverGlobalSessionRef,
  GxserverZmxSessionName,
} from './gxserver-protocol-core';
import type { GxserverPresentationSettledOverride } from './gxserver-protocol-presentation';
import type {
  GxserverAttachSessionMetadataResult,
  GxserverProviderKillResult,
} from './gxserver-protocol-session-runtime';

export type GxserverSharedStateArea =
  | 'projects'
  | 'sessions'
  | 'zmxLifecycle'
  | 'sleepWakePolicy'
  | 'agentStatus'
  | 'remoteControl'
  | 'pinnedFavorite'
  | 'customAgentsCommands'
  | 'launchRuntimeSettings'
  | 'previousSessionHistory'
  | 'worktreeGitActions'
  | 'beadsProjectBoard';

export type GxserverClientLocalStateArea =
  | 'sidebarGroups'
  | 'splitTabLayout'
  | 'visibleSessionCount'
  | 'browserEditorCodeServerPanes'
  | 'cefBrowserProfiles'
  | 'popOutWindows'
  | 'visualSettings';

export type GxserverMixedStateArea = 'notificationRules' | 'commandDefinitions' | 'projectIcons' | 'theme';

export type GxserverSessionKind = 'terminal' | 'agent';
export type GxserverSessionSurface = 'workspace' | 'commands';
export type GxserverSessionTag =
  | 'favorite'
  | 'high-priority'
  | 'research'
  | 'todo'
  | 'in-progress'
  | 'testing'
  | 'blocked'
  | 'low-priority'
  | 'on-hold'
  | 'done'
  | 'bug'
  | 'feature'
  | 'design'
  /** A user-defined tag from the daemon's custom tag catalog (`/api/readCustomSessionTags`). */
  | `custom-${string}`;
export type GxserverSessionTagFilter = GxserverSessionTag | 'untagged';
export type GxserverDomainLifecycleState = 'running' | 'sleeping' | 'stopped' | 'missing' | 'unknown';
export type GxserverProviderLifecycleState = 'exists' | 'missing' | 'unknown';
export type GxserverStartupTextDisposition =
  | 'discardExistingProvider'
  | 'discardUnknownProvider'
  | 'none'
  | 'queueAfterTerminalReady';
export type GxserverRestoreBlockReason = 'missingCwd';

export interface GxserverProjectDomainState {
  attentionRules: Record<string, unknown>;
  completionRules: Record<string, unknown>;
  createdAt: string;
  customAgentOrder: readonly string[];
  customAgents: readonly Record<string, unknown>[];
  customCommandOrder: readonly string[];
  customCommands: readonly Record<string, unknown>[];
  defaultCommand?: string;
  deletedDefaultCommandIds: readonly string[];
  gitConfig: Record<string, unknown>;
  identityIcon?: Record<string, unknown>;
  isFavorite: boolean;
  isPinned: boolean;
  /*
  CDXC:Projects 2026-06-24-12:27:
  GPUI Recent Projects must be gxserver-owned project state, not a label/session
  inference. Parked projects stay in the project domain table with explicit
  recent fields so `/api/listRecentProjects` can return only trusted,
  path-bearing rows and presentation can omit them from active groups.
  */
  isRecentProject: boolean;
  launchSettings: Record<string, unknown>;
  name: string;
  notificationRules: Record<string, unknown>;
  path?: string;
  previousSessionHistory: readonly Record<string, unknown>[];
  projectBoardConfig: Record<string, unknown>;
  projectId: GxserverProjectId;
  recentClosedAt?: string;
  runtimeSettings: Record<string, unknown>;
  /*
  CDXC:Projects 2026-06-30-21:23:
  Active project visibility is gxserver-owned so mobile, CLI, GPUI, and macOS omit Remote Attach carrier projects and other hidden containers through the shared daemon contract instead of each client filtering macOS sidebar details.
  */
  systemKind?: 'remoteAttachCarrier';
  updatedAt: string;
  visibility?: 'visible' | 'hidden';
  worktree?: Record<string, unknown>;
}

export interface GxserverRecentProjectDomainState {
  icon?: Record<string, unknown>;
  iconDataUrl?: string;
  path: string;
  projectId: GxserverProjectId;
  recentClosedAt?: string;
  sessionCount: number;
  theme?: string;
  themeColor?: string;
  title: string;
}

/*
CDXC:AgentLauncher 2026-06-24-20:34:
GPUI sidebar and app-modal clients should read normalized launcher/action HUD
rows from gxserver instead of reimplementing project custom-agent/action
projection in host-specific Rust. The endpoint returns the shared Sidebar HUD
JSON shape only; gxserver owns default rows, hidden built-ins, custom row
validation, icon allowlists, display order, deleted default actions, and
active-project command ownership.

CDXC:AgentLauncher 2026-06-24-20:54:
Settings save/delete/order mutations for custom agents and actions use a
narrow gxserver contract instead of renderer-owned `/api/updateProject` field
patches. The payload carries only the explicit Settings intent; gxserver owns
validation, hidden/default semantics, worktree parent command ownership, and
the refreshed HUD/project rows returned after persistence.
*/
export interface GxserverReadSidebarHudParams {
  activeProjectId?: string;
  includeAllProjectCommands?: boolean;
}

export interface GxserverSidebarHudAgentButton {
  acceptAllMode?: 'inherit' | 'enabled' | 'disabled';
  agentId: string;
  command?: string;
  icon?: string;
  isDefault: boolean;
  name: string;
}

export interface GxserverSidebarHudCommandButton {
  actionType: 'browser' | 'terminal';
  closeTerminalOnExit: boolean;
  command?: string;
  commandId: string;
  icon?: string;
  isDefault: boolean;
  links?: readonly GxserverSidebarHudCommandLink[];
  name: string;
  playCompletionSound: boolean;
  showOnProjectRow?: boolean;
  url?: string;
}

export interface GxserverSidebarHudCommandLink {
  target: 'integrated' | 'external';
  url: string;
}

export interface GxserverSidebarHudResponse {
  agents: readonly GxserverSidebarHudAgentButton[];
  /**
   * CDXC:Projects 2026-08-01:
   * Present only when the caller asked for `includeAllProjectCommands`.
   * Keyed by project id with worktrees already resolved to their parent's
   * Actions, mirroring the active-project `commands` resolution per project.
   */
  commandsByProject?: Readonly<Record<string, readonly GxserverSidebarHudCommandButton[]>>;
  commands: readonly GxserverSidebarHudCommandButton[];
  /**
   * CDXC:AgentLauncher 2026-08-01:
   * Global Actions apply to every project and are stored daemon-side rather
   * than in project metadata, so they arrive as their own list instead of
   * inside `commands`. Optional because a gxserver older than the app drops
   * fields it does not know; surfaces normalize the gap to an empty list.
   */
  globalCommands?: readonly GxserverSidebarHudCommandButton[];
}

/**
 * CDXC:Projects 2026-08-01:
 * Any HUD settings mutation returns a full replacement HUD snapshot, so
 * clients that render per-project quick actions ask the mutation for the same
 * opt-in commandsByProject block readSidebarHud serves. The flag rides beside
 * every mutation variant instead of inside one so agent mutations cannot
 * silently drop the per-project rows.
 */
export type GxserverSidebarHudSettingsMutationParams = {
  includeAllProjectCommands?: boolean;
} & GxserverSidebarHudSettingsMutationIntent;

type GxserverSidebarHudSettingsMutationIntent =
  | {
      acceptAllMode?: 'inherit' | 'enabled' | 'disabled';
      activeProjectId?: string;
      agentId?: string;
      command: string;
      icon?: string;
      name: string;
      operation: 'save';
      target: 'agent';
    }
  | {
      activeProjectId?: string;
      agentId: string;
      operation: 'delete';
      target: 'agent';
    }
  | {
      activeProjectId?: string;
      agentIds: readonly string[];
      operation: 'order';
      target: 'agent';
    }
  | {
      actionType: 'browser' | 'terminal';
      activeProjectId?: string;
      closeTerminalOnExit?: boolean;
      command?: string;
      commandId?: string;
      icon?: string;
      links?: readonly GxserverSidebarHudCommandLink[];
      name: string;
      operation: 'save';
      playCompletionSound?: boolean;
      showOnProjectRow?: boolean;
      /**
       * CDXC:AgentLauncher 2026-08-01:
       * Global and Project Actions accept the identical action definition and
       * differ only in ownership, so the target selects the list rather than
       * the payload shape. gxserver validates both through one path, which is
       * what keeps the two lists from drifting into different action shapes.
       */
      target: 'command' | 'globalCommand';
      url?: string;
    }
  | {
      activeProjectId?: string;
      commandId: string;
      operation: 'delete';
      target: 'command' | 'globalCommand';
    }
  | {
      activeProjectId?: string;
      commandIds: readonly string[];
      operation: 'order';
      target: 'command' | 'globalCommand';
    };

export interface GxserverSidebarHudSettingsMutationResult {
  hud: GxserverSidebarHudResponse;
  itemIds?: readonly string[];
  projects: readonly GxserverProjectDomainState[];
}

export type GxserverConnectionTransport = 'local' | 'tailscale' | 'direct' | 'ssh' | 'tailcat';
export type GxserverConnectionProfileId = string & {
  readonly __gxserverConnectionProfileId: unique symbol;
};

export interface GxserverCredentialSecretRef {
  account: string;
  service: 'ghostex.gxserver';
}

export interface GxserverConnectionProfile {
  baseUrl?: string;
  createdAt: string;
  id: string;
  name: string;
  /**
   * CDXC:RemotePairing 2026-09-01:
   * The tailcat transport reaches the remote gxserver's own API port through a
   * client-side pipe, so the profile carries the peer's address blob and the
   * port it serves instead of a baseUrl. `remotePort` defaults to the gxserver
   * API port when absent.
   */
  remotePort?: number;
  serverId?: GxserverServerId;
  sshUrl?: string;
  tailcatToken?: string;
  tokenSecretRef?: GxserverCredentialSecretRef;
  transport: GxserverConnectionTransport;
  updatedAt: string;
}

export interface GxserverConnectionProfilesFile {
  profiles: readonly GxserverConnectionProfile[];
  version: 1;
}

export interface GxserverRouteRef {
  projectId?: GxserverProjectId;
  serverId: GxserverServerId;
  sessionId?: GxserverSessionId;
}

export interface GxserverRemoteProjectListMetadata {
  icon: 'cloud';
  profileId: string;
  serverId: GxserverServerId;
  transport: Exclude<GxserverConnectionTransport, 'local'>;
}

export interface GxserverSshForwardPlan {
  baseUrl: string;
  checkCommand: readonly string[];
  installGuidance: string;
  localPort: number;
  portForwardCommand: readonly string[];
  remoteLocalPort: number;
  startCommand: readonly string[];
}

export interface GxserverRemoteAttachMetadata {
  attachCommand: string;
  profileId: string;
  provider: 'zmx';
  serverId?: GxserverServerId;
  transport: 'ssh';
  zmxName: GxserverZmxSessionName;
}

export interface GxserverSessionHiddenMetadata {
  restoredFromHistoryId?: string;
  restoredFromSessionId?: GxserverSessionId;
}

export interface GxserverSessionDomainState {
  agentId?: string;
  attentionRules: Record<string, unknown>;
  commandId?: string;
  completionRules: Record<string, unknown>;
  createdAt: string;
  cwd?: string;
  globalRef: GxserverGlobalSessionRef;
  hiddenMetadata: GxserverSessionHiddenMetadata;
  isFavorite: boolean;
  isParked?: boolean;
  isPinned: boolean;
  kind: GxserverSessionKind;
  lastActiveAt?: string;
  launchSettings: Record<string, unknown>;
  lifecycleState: GxserverDomainLifecycleState;
  notificationRules: Record<string, unknown>;
  projectId: GxserverProjectId;
  providerState: {
    lifecycleState: GxserverProviderLifecycleState;
    zmxName: GxserverZmxSessionName;
  } & Record<string, unknown>;
  runtimeSettings: Record<string, unknown>;
  sessionId: GxserverSessionId;
  sessionTag?: GxserverSessionTag;
  /**
   * CDXC:StateSync 2026-07-29-00:00:
   * Durable Sidebar V2 lifecycle. Writable only through the guarded
   * settle/snooze RPCs — `/api/updateSession` deliberately ignores these keys —
   * and absent when the session has no lifecycle state. The server-internal
   * override stamp gxserver uses to decide when activity has outrun an override
   * is not part of the wire contract.
   */
  settledAt?: string;
  settledOverride?: GxserverPresentationSettledOverride;
  sidebarOrder?: number;
  snoozedAt?: string;
  snoozedUntil?: string;
  surface: GxserverSessionSurface;
  title: string;
  updatedAt: string;
  worktree?: Record<string, unknown>;
  zmxName: GxserverZmxSessionName;
}

export interface GxserverCreateProjectParams {
  attentionRules?: Record<string, unknown>;
  completionRules?: Record<string, unknown>;
  customAgentOrder?: readonly string[];
  customAgents?: readonly Record<string, unknown>[];
  customCommandOrder?: readonly string[];
  customCommands?: readonly Record<string, unknown>[];
  defaultCommand?: string;
  deletedDefaultCommandIds?: readonly string[];
  gitConfig?: Record<string, unknown>;
  identityIcon?: Record<string, unknown>;
  isFavorite?: boolean;
  isPinned?: boolean;
  launchSettings?: Record<string, unknown>;
  name: string;
  notificationRules?: Record<string, unknown>;
  path?: string;
  previousSessionHistory?: readonly Record<string, unknown>[];
  projectBoardConfig?: Record<string, unknown>;
  runtimeSettings?: Record<string, unknown>;
  worktree?: Record<string, unknown>;
}

export type GxserverUpdateProjectParams = Partial<GxserverCreateProjectParams> & {
  projectId: GxserverProjectId;
};

export interface GxserverCreateSessionParams {
  agentId?: string;
  attentionRules?: Record<string, unknown>;
  commandId?: string;
  completionRules?: Record<string, unknown>;
  cwd?: string;
  /**
   * CDXC:Drafts 2026-08-28:
   * Create this agent session as a DRAFT: a real durable row whose agent CLI is
   * started in the background, but which has not received a first prompt yet.
   * gxserver records it as `runtimeSettings.draftStatus = 'draft'` and REMOVES
   * that marker the moment a first user prompt actually reaches the agent, so a
   * promoted draft is indistinguishable from an ordinary session. Never combine
   * with a first prompt (`firstUserMessage` / `firstUserInputDraft`): a flow
   * that already carries a prompt is not a draft.
   */
  draft?: boolean;
  isFavorite?: boolean;
  isParked?: boolean;
  isPinned?: boolean;
  kind?: GxserverSessionKind;
  lastActiveAt?: string;
  launchSettings?: Record<string, unknown>;
  lifecycleState?: GxserverDomainLifecycleState;
  notificationRules?: Record<string, unknown>;
  projectId?: GxserverProjectId;
  projectName?: string;
  projectPath?: string;
  providerState?: Partial<GxserverSessionDomainState['providerState']>;
  /**
   * CDXC:RemoteMachines 2026-06-24-17:19:
   * Remote agent starts can ask gxserver to reject unknown custom/default agent ids instead of creating an inert row with no launch command. This lets clients avoid sending renderer-owned command text while still failing honestly when remote project metadata cannot resolve the selected agent.
   */
  requireLaunchCommand?: boolean;
  restoredFromHistoryId?: string;
  restoredFromSessionId?: GxserverSessionId;
  runtimeSettings?: Record<string, unknown>;
  sessionTag?: GxserverSessionTag | null;
  sidebarOrder?: number;
  surface?: GxserverSessionSurface;
  title?: string;
  worktree?: Record<string, unknown>;
}

export type GxserverUpdateSessionParams = Partial<Omit<GxserverCreateSessionParams, 'projectId'>> & {
  projectId: GxserverProjectId;
  sessionId: GxserverSessionId;
};

/*
CDXC:StateSync 2026-07-29-00:00:
Sidebar V2's settle/snooze commands. gxserver enforces the guards its client
twin (`packages/shared/sidebar-v2-lifecycle.ts`) mirrors: a working or blocked-on-you
session cannot be settled, a blocked-on-you session cannot be snoozed, and a
wake time that is not strictly in the future is rejected rather than silently
normalized. Every command is idempotent — `changed: false` marks a no-op
(double click, bulk settle, waking an awake session), which emits no
presentation delta.
*/
export interface GxserverSettleSessionParams {
  projectId: GxserverProjectId;
  sessionId: GxserverSessionId;
}

export type GxserverUnsettleSessionParams = GxserverSettleSessionParams;

export interface GxserverSnoozeSessionParams extends GxserverSettleSessionParams {
  /** ISO wake time; must be strictly in the future. */
  snoozedUntil: string;
}

export type GxserverUnsnoozeSessionParams = GxserverSettleSessionParams;

export interface GxserverSessionLifecycleResult {
  changed: boolean;
  session: GxserverSessionDomainState;
}

export interface GxserverSessionLifecycleResult {
  attach?: GxserverAttachSessionMetadataResult;
  kill?: GxserverProviderKillResult;
  session: GxserverSessionDomainState;
}
