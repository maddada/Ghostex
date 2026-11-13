import type { GxserverProjectId, GxserverSessionId, GxserverZmxSessionName } from './gxserver-protocol-core';
import type { GxserverSessionLifecycleParams } from './gxserver-protocol-sessions';
import type { GxserverSessionTitleSource, GxserverSessionTitleProjection } from './gxserver-protocol-presentation';
import type {
  GxserverProviderLifecycleState,
  GxserverStartupTextDisposition,
  GxserverRestoreBlockReason,
  GxserverSessionDomainState,
} from './gxserver-protocol-domain';

/**
 * CDXC:SessionChat 2026-09-02:
 * `/api/rewindSessionChat` drives the agent's own rewind flow in its terminal
 * (Claude: `/rewind`, pick the prompt, "Restore conversation") to the point
 * before `messageId`, a user prompt row of the active conversation. The daemon
 * verifies every dialog step against the screen and cancels on any mismatch.
 * On success the chat readers hide the rewound rows immediately; the transcript
 * confirms the branch when the next prompt is sent. Codex drives its Escape picker and adopts the resulting conversation.
 */
export interface GxserverRewindSessionChatParams {
  projectId: GxserverProjectId;
  sessionId: GxserverSessionId;
  /** Id of the user prompt row to rewind to (the point before it was sent). */
  messageId: string;
}

export interface GxserverRewindSessionChatResult {
  ok: true;
  /** Row id the conversation is now positioned before. */
  targetMessageId: string;
  /** Claude's new active leaf UUID; null before the first prompt or for Codex's new branch. */
  leafId: string | null;
  /** Repeating this target only retries synchronization, without sending rewind keys again. */
  synchronizationPending?: boolean;
  /** Synchronization or terminal draft cleanup needs attention. */
  warning?: string | null;
}

/**
 * CDXC:AgentScreenDetection 2026-09-03 WHY:
 * `/api/selectSessionChatModel` drives Codex's own `/model` picker in the
 * session's terminal (the numbered model list, then the numbered reasoning
 * list) because Codex has no command form for it: `/model <name>` is sent to
 * the model as a prompt. The daemon reads the row digits off the screen and
 * aborts on any mismatch. With `defer`, Codex and Claude choices enter the durable queue and return before delivery.
 */
export interface GxserverSelectSessionChatModelParams {
  options?: import('./session-chat').SessionChatSelectionOptions;
  projectId: GxserverProjectId;
  sessionId: GxserverSessionId;
  /** Store the choice durably, attempt immediately, and retry when the terminal can accept it (Codex and Claude). */
  defer?: boolean;
  /** Model id from the published catalog; empty for an options-only change. */
  model: string;
  /** Effort id the reasoning list must offer for that model (`high`). */
  effort: string;
  /**
   * `'session'` applies the choice to this session alone and leaves the agent's saved default untouched;
   * `'default'` (the default when omitted) keeps the original behaviour. Only Claude can honour `'session'`.
   */
  scope?: import('./session-chat').SessionChatModelSelectionScope;
}

export interface GxserverSelectSessionChatModelResult {
  ok: true;
  queued?: boolean;
  pendingModelSelection?: import('./session-chat').SessionChatPendingModelSelection;
  model: string;
  effort: string;
  /** Echoed so a client can tell a scope-aware daemon from one that ignored the field. */
  scope?: import('./session-chat').SessionChatModelSelectionScope;
}

export interface GxserverTerminalTitleEventParams extends GxserverSessionLifecycleParams {
  agentName?: string;
  previousTerminalTitle?: string;
  rawTitle?: string;
  sessionPersistenceProvider?: 'off' | 'tmux' | 'zellij' | 'zmx';
}

export interface GxserverTerminalTitleEventResult {
  agentSessionId?: string;
  activity: GxserverAgentActivityState;
  changed: boolean;
  enteredAttention: boolean;
  previousActivity: GxserverAgentActivityState['activity'];
  projection: GxserverSessionTitleProjection;
  reason: string;
  session: GxserverSessionDomainState;
  visibleTitle?: string;
}

export type GxserverFirstPromptTitleGenerationAgent =
  'codex' | 'cursor' | 'claude' | 'grok' | 'pi' | 'antigravity' | 'custom';

export interface GxserverSessionRenameRequestParams extends GxserverSessionLifecycleParams {
  agentName?: string;
  agentSessionId?: string;
  agentSessionPath?: string;
  title: string;
  titleSource?: Extract<GxserverSessionTitleSource, 'generated' | 'user'>;
}

export interface GxserverSessionRenameRequestResult {
  changed: boolean;
  pendingAgentMetadata: boolean;
  projection: GxserverSessionTitleProjection;
  reason: string;
  session: GxserverSessionDomainState;
  shouldSendAgentRenameCommand: boolean;
}

export interface GxserverAttachSessionMetadataParams extends GxserverSessionLifecycleParams {
  promptEditor?: 'code-server' | 'monaco';
  startupText?: string;
}

export type GxserverAgentStartupTextDisposition = 'none' | 'queueAfterTerminalReady';

export interface GxserverAgentLaunchPlan {
  agentCommand?: string;
  command: string;
  delayedSend?: {
    deadlineAt: string;
    disposition: 'scheduled';
  };
  firstUserMessage?: string;
  startupText: string;
  startupTextDisposition: GxserverAgentStartupTextDisposition;
}

export interface GxserverAgentResumePlanParams extends GxserverSessionLifecycleParams {}

export interface GxserverAgentResumePlan {
  agentId?: string;
  baseCommand?: string;
  copyCommand?: string;
  displayCommand?: string;
  fallbackCommand?: string;
  lookupCommand?: string;
  primaryCommand?: string;
  runtimeCommand?: string;
  startupText?: string;
  startupTextDisposition: GxserverAgentStartupTextDisposition;
}

export type GxserverAgentActivityEvent =
  | 'acknowledge'
  | 'agentDetected'
  | 'bell'
  | 'escape'
  | 'launch'
  | 'resume'
  | 'terminalError'
  | 'terminalExited'
  | 'title'
  | 'wake';

export interface GxserverAgentActivityState {
  activity: 'attention' | 'idle' | 'working';
  agentName?: 'antigravity' | 'claude' | 'codex' | 'copilot' | 'cursor' | 'opencode' | 'pi';
  attentionEventId?: string;
  attentionSuppressedUntil?: string;
  hasSeenWorking?: boolean;
  isAcknowledged?: boolean;
  lastChangedAt?: string;
  lastMeaningfulActivityAt?: string;
  lastTitle?: string;
  lastTitleChangeAt?: string;
  suppressedUntil?: string;
  workingSource?: 'explicit' | 'title';
  workingStartedAt?: string;
}

export interface GxserverAgentActivityInput {
  activity?: GxserverAgentActivityState['activity'];
  agentId?: string;
  event?: GxserverAgentActivityEvent;
  nowIso?: string;
  nowMs?: number;
  settledTitle?: string;
  previous?: unknown;
  title?: string;
}

export interface GxserverUpdateAgentActivityParams extends GxserverSessionLifecycleParams {
  activity?: GxserverAgentActivityState['activity'];
  agentName?: string;
  event?: GxserverAgentActivityEvent;
  nowMs?: number;
  settledTitle?: string;
  title?: string;
}

export interface GxserverUpdateAgentActivityResult {
  activity: GxserverAgentActivityState;
  enteredAttention: boolean;
  previousActivity: GxserverAgentActivityState['activity'];
  session: GxserverSessionDomainState;
}

export interface GxserverProviderProbeResult {
  error?: string;
  lifecycleState: GxserverProviderLifecycleState;
  probedAt: string;
  zmxName: GxserverZmxSessionName;
}

export interface GxserverSessionRestoreBlocked {
  cwd?: string;
  reason: GxserverRestoreBlockReason;
}

export interface GxserverAttachSessionMetadataResult {
  attachCommand?: string;
  cwd?: string;
  persistenceSessionCreated?: boolean;
  provider: 'zmx';
  providerState: GxserverProviderProbeResult;
  restoreBlocked?: GxserverSessionRestoreBlocked;
  session: GxserverSessionDomainState;
  startupText?: string;
  startupTextDisposition: GxserverStartupTextDisposition;
  zmxName: GxserverZmxSessionName;
}

export type GxserverTerminalWsErrorCode = 'unauthorized' | 'protocolMismatch' | 'notFound' | 'providerNotRunning';

export interface GxserverTerminalWsReadyMessage {
  cols: number;
  rows: number;
  type: 'ready';
  zmxName: GxserverZmxSessionName;
}

export interface GxserverTerminalWsExitMessage {
  code: number | null;
  type: 'exit';
}

export interface GxserverTerminalWsErrorMessage {
  code: GxserverTerminalWsErrorCode;
  message: string;
  type: 'error';
}

export interface GxserverTerminalWsResizeMessage {
  cols: number;
  rows: number;
  type: 'resize';
}

export type GxserverTerminalWsClientControlMessage = GxserverTerminalWsResizeMessage;
export type GxserverTerminalWsServerControlMessage =
  GxserverTerminalWsReadyMessage | GxserverTerminalWsExitMessage | GxserverTerminalWsErrorMessage;
export type GxserverTerminalWsControlMessage =
  GxserverTerminalWsClientControlMessage | GxserverTerminalWsServerControlMessage;

export interface GxserverStartSessionProviderParams extends GxserverSessionLifecycleParams {
  promptEditor?: 'code-server' | 'monaco';
  startupText?: string;
}

export interface GxserverStartSessionProviderResult {
  exitCode?: number;
  provider: 'zmx';
  providerState: GxserverProviderProbeResult;
  session: GxserverSessionDomainState;
  started: boolean;
  startupTextDisposition: GxserverStartupTextDisposition;
  zmxName: GxserverZmxSessionName;
}

export interface GxserverProviderKillResult {
  error?: string;
  exitCode: number;
  killed: boolean;
  stderr: string;
  stdout: string;
  zmxName: GxserverZmxSessionName;
}
