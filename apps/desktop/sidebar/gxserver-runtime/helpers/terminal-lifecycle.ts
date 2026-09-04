/*
CDXC:RepoStructure 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import {
  GPUI_SIDEBAR_WORKSPACE_FIRST_PROMPT_TITLE_CANCEL_MESSAGE_TYPE,
  GPUI_SIDEBAR_WORKSPACE_FIRST_PROMPT_TITLE_CANCEL_MESSAGE_VERSION,
  GPUI_SIDEBAR_WORKSPACE_SESSION_ATTENTION_ACKNOWLEDGE_MESSAGE_TYPE,
  GPUI_SIDEBAR_WORKSPACE_SESSION_ATTENTION_ACKNOWLEDGE_MESSAGE_VERSION,
  GPUI_SIDEBAR_WORKSPACE_TERMINAL_BELL_MESSAGE_TYPE,
  GPUI_SIDEBAR_WORKSPACE_TERMINAL_BELL_MESSAGE_VERSION,
  GPUI_SIDEBAR_WORKSPACE_TERMINAL_ESCAPE_PRESSED_MESSAGE_TYPE,
  GPUI_SIDEBAR_WORKSPACE_TERMINAL_ESCAPE_PRESSED_MESSAGE_VERSION,
  GPUI_SIDEBAR_WORKSPACE_TERMINAL_LIFECYCLE_REQUEST_MESSAGE_TYPE,
  GPUI_SIDEBAR_WORKSPACE_TERMINAL_LIFECYCLE_REQUEST_MESSAGE_VERSION,
  GPUI_SIDEBAR_WORKSPACE_TERMINAL_RUNTIME_ACTION_MESSAGE_TYPE,
  GPUI_SIDEBAR_WORKSPACE_TERMINAL_RUNTIME_ACTION_MESSAGE_VERSION,
  GPUI_SIDEBAR_WORKSPACE_TERMINAL_TITLE_CHANGED_MESSAGE_TYPE,
  GPUI_SIDEBAR_WORKSPACE_TERMINAL_TITLE_CHANGED_MESSAGE_VERSION,
  GPUI_SIDEBAR_WORKSPACE_TERMINAL_TITLE_MAX_CHARS,
} from '../constants';
import type {
  GpuiWorkspaceFirstPromptTitleGenerationCancelPayload,
  GpuiWorkspaceSessionAttentionAcknowledgePayload,
  GpuiWorkspaceTerminalBellPayload,
  GpuiWorkspaceTerminalEscapePressedPayload,
  GpuiWorkspaceTerminalLifecycleRequest,
  GpuiWorkspaceTerminalRuntimeActionPayload,
  GpuiWorkspaceTerminalTitleChangedPayload,
} from '../types-and-protocol';
import { isObjectRecord, normalizeNonEmptyString } from './records';
import { parseGpuiRemotePresentationProjectId, parseGpuiRemotePresentationSessionId } from './remote-presentation';
import { gpuiStatusPetActivationSessionIdAllowed } from './status-indicators';
import { parseGxserverPresentationProjectSessionId } from '@/packages/shared/gxserver-presentation-sidebar-projection';
import type { GxserverSessionTransitionResult } from '@/packages/shared/gxserver-protocol';

export function gpuiWorkspaceTerminalTitleCommandForAgent(agentId: string): 'name' | 'rename' | 'title' {
  const normalizedAgentId = agentId.trim().toLowerCase();
  if (normalizedAgentId === 'pi' || normalizedAgentId === 'π') {
    return 'name';
  }
  if (normalizedAgentId === 'hermes' || normalizedAgentId === 'hermes agent' || normalizedAgentId === 'hermes-agent') {
    return 'title';
  }
  return 'rename';
}

export function normalizeGpuiWorkspaceTerminalTitleChanged(
  value: unknown
): GpuiWorkspaceTerminalTitleChangedPayload | undefined {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  if (
    Object.keys(record).some((key) => !['projectId', 'rawTitle', 'sessionId', 'type', 'version'].includes(key)) ||
    record.type !== GPUI_SIDEBAR_WORKSPACE_TERMINAL_TITLE_CHANGED_MESSAGE_TYPE ||
    record.version !== GPUI_SIDEBAR_WORKSPACE_TERMINAL_TITLE_CHANGED_MESSAGE_VERSION
  ) {
    return undefined;
  }
  const projectId = normalizeNonEmptyString(record.projectId)?.trim();
  const sessionId = normalizeNonEmptyString(record.sessionId)?.trim();
  const rawTitle = typeof record.rawTitle === 'string' ? record.rawTitle : undefined;
  if (
    !projectId ||
    !sessionId ||
    !rawTitle ||
    rawTitle.length > GPUI_SIDEBAR_WORKSPACE_TERMINAL_TITLE_MAX_CHARS ||
    /[\u0000-\u001f\u007f]/u.test(rawTitle) ||
    !gpuiLocalWorkspaceLifecycleProjectIdAllowed(projectId) ||
    !gpuiLocalWorkspaceLifecycleSessionIdAllowed(sessionId)
  ) {
    return undefined;
  }
  return { projectId, rawTitle, sessionId };
}

/*
CDXC:TranscriptExport 2026-08-20:
The follow-up conversation's staged input, not a prompt. gxserver types this
into the new agent's composer and never submits it, so the user writes their
own prompt around the mention: sending anything on their behalf was rejected.
The trailing space completes the `@` file mention and separates it from what
they type next, so it must survive untrimmed all the way to the daemon.
*/
export function createExportedTranscriptMentionDraft(path: string): string {
  return `@${path} `;
}

export function normalizeGpuiWorkspaceTerminalRuntimeAction(
  value: unknown
): GpuiWorkspaceTerminalRuntimeActionPayload | undefined {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  if (
    Object.keys(record).some((key) => !['action', 'agentId', 'projectId', 'sessionId', 'type', 'version'].includes(key))
  ) {
    return undefined;
  }
  if (
    record.type !== GPUI_SIDEBAR_WORKSPACE_TERMINAL_RUNTIME_ACTION_MESSAGE_TYPE ||
    record.version !== GPUI_SIDEBAR_WORKSPACE_TERMINAL_RUNTIME_ACTION_MESSAGE_VERSION
  ) {
    return undefined;
  }
  if (record.action === 'sleepInactiveSessions' || record.action === 'sleepAllDaemonSessions') {
    if (record.projectId !== undefined || record.sessionId !== undefined) {
      return undefined;
    }
    return { action: record.action };
  }
  const action =
    record.action === 'exportTranscript' ||
    record.action === 'forkSession' ||
    record.action === 'fullReloadSession' ||
    record.action === 'openSessionNote' ||
    record.action === 'sleepSession' ||
    record.action === 'switchSessionAgent'
      ? record.action
      : undefined;
  const projectId = normalizeNonEmptyString(record.projectId)?.trim();
  const sessionId = normalizeNonEmptyString(record.sessionId)?.trim();
  if (
    !action ||
    !projectId ||
    !sessionId ||
    !gpuiLocalWorkspaceLifecycleProjectIdAllowed(projectId) ||
    !gpuiLocalWorkspaceLifecycleSessionIdAllowed(sessionId)
  ) {
    return undefined;
  }
  if (action === 'switchSessionAgent') {
    const agentId = normalizeNonEmptyString(record.agentId)?.trim();
    if (!agentId) {
      return undefined;
    }
    return { action, agentId, projectId, sessionId };
  }
  if (record.agentId !== undefined) {
    return undefined;
  }
  return { action, projectId, sessionId };
}

export function normalizeGpuiWorkspaceTerminalBell(value: unknown): GpuiWorkspaceTerminalBellPayload | undefined {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  if (Object.keys(record).some((key) => !['projectId', 'sessionId', 'type', 'version'].includes(key))) {
    return undefined;
  }
  if (
    record.type !== GPUI_SIDEBAR_WORKSPACE_TERMINAL_BELL_MESSAGE_TYPE ||
    record.version !== GPUI_SIDEBAR_WORKSPACE_TERMINAL_BELL_MESSAGE_VERSION
  ) {
    return undefined;
  }
  const projectId = normalizeNonEmptyString(record.projectId)?.trim();
  const sessionId = normalizeNonEmptyString(record.sessionId)?.trim();
  if (
    !projectId ||
    !sessionId ||
    !gpuiLocalWorkspaceLifecycleProjectIdAllowed(projectId) ||
    !gpuiLocalWorkspaceLifecycleSessionIdAllowed(sessionId)
  ) {
    return undefined;
  }
  return { projectId, sessionId };
}

export function normalizeGpuiWorkspaceTerminalEscapePressed(
  value: unknown
): GpuiWorkspaceTerminalEscapePressedPayload | undefined {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  if (Object.keys(record).some((key) => !['projectId', 'sessionId', 'type', 'version'].includes(key))) {
    return undefined;
  }
  if (
    record.type !== GPUI_SIDEBAR_WORKSPACE_TERMINAL_ESCAPE_PRESSED_MESSAGE_TYPE ||
    record.version !== GPUI_SIDEBAR_WORKSPACE_TERMINAL_ESCAPE_PRESSED_MESSAGE_VERSION
  ) {
    return undefined;
  }
  const projectId = normalizeNonEmptyString(record.projectId)?.trim();
  const sessionId = normalizeNonEmptyString(record.sessionId)?.trim();
  if (
    !projectId ||
    !sessionId ||
    !gpuiLocalWorkspaceLifecycleProjectIdAllowed(projectId) ||
    !gpuiLocalWorkspaceLifecycleSessionIdAllowed(sessionId)
  ) {
    return undefined;
  }
  return { projectId, sessionId };
}

export function normalizeGpuiWorkspaceFirstPromptTitleGenerationCancel(
  value: unknown
): GpuiWorkspaceFirstPromptTitleGenerationCancelPayload | undefined {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  if (Object.keys(record).some((key) => !['projectId', 'sessionId', 'type', 'version'].includes(key))) {
    return undefined;
  }
  if (
    record.type !== GPUI_SIDEBAR_WORKSPACE_FIRST_PROMPT_TITLE_CANCEL_MESSAGE_TYPE ||
    record.version !== GPUI_SIDEBAR_WORKSPACE_FIRST_PROMPT_TITLE_CANCEL_MESSAGE_VERSION
  ) {
    return undefined;
  }
  const projectId = normalizeNonEmptyString(record.projectId)?.trim();
  const sessionId = normalizeNonEmptyString(record.sessionId)?.trim();
  if (
    !projectId ||
    !sessionId ||
    !gpuiLocalWorkspaceLifecycleProjectIdAllowed(projectId) ||
    !gpuiLocalWorkspaceLifecycleSessionIdAllowed(sessionId)
  ) {
    return undefined;
  }
  return { projectId, sessionId };
}

export function normalizeGpuiWorkspaceSessionAttentionAcknowledge(
  value: unknown
): GpuiWorkspaceSessionAttentionAcknowledgePayload | undefined {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  if (Object.keys(record).some((key) => !['projectId', 'sessionId', 'type', 'version'].includes(key))) {
    return undefined;
  }
  if (
    record.type !== GPUI_SIDEBAR_WORKSPACE_SESSION_ATTENTION_ACKNOWLEDGE_MESSAGE_TYPE ||
    record.version !== GPUI_SIDEBAR_WORKSPACE_SESSION_ATTENTION_ACKNOWLEDGE_MESSAGE_VERSION
  ) {
    return undefined;
  }
  const projectId = normalizeNonEmptyString(record.projectId)?.trim();
  const sessionId = normalizeNonEmptyString(record.sessionId)?.trim();
  if (
    !projectId ||
    !sessionId ||
    !gpuiLocalWorkspaceLifecycleProjectIdAllowed(projectId) ||
    !gpuiLocalWorkspaceLifecycleSessionIdAllowed(sessionId)
  ) {
    return undefined;
  }
  return { projectId, sessionId };
}

export function normalizeQueuedGpuiWorkspaceTerminalLifecycleRequest(
  value: unknown
): GpuiWorkspaceTerminalLifecycleRequest | undefined {
  /*
  CDXC:Workarea 2026-06-26-05:23:
  Lifecycle retries may contain either the raw fixed bridge payload queued before React started or the runtime's already-normalized id-only request queued while the CEF result bridge was missing. Accept only those two bounded shapes so retries do not reintroduce paths, commands, terminal text, URLs, tokens, or generic IPC fields.
  */
  return (
    normalizeGpuiWorkspaceTerminalLifecycleRequest(value) ?? normalizeGpuiWorkspaceTerminalLifecycleQueuedRequest(value)
  );
}

export function normalizeGpuiWorkspaceTerminalLifecycleQueuedRequest(
  value: unknown
): GpuiWorkspaceTerminalLifecycleRequest | undefined {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  if (
    Object.keys(record).some(
      (key) =>
        ![
          'action',
          'keepSidebarFocus',
          'projectId',
          'replacementProjectId',
          'replacementSessionId',
          'requestId',
          'sessionId',
          'skipReplacementFallback',
        ].includes(key)
    )
  ) {
    return undefined;
  }
  if (typeof record.requestId !== 'number' || !Number.isSafeInteger(record.requestId) || record.requestId <= 0) {
    return undefined;
  }
  const action =
    record.action === 'close' || record.action === 'sleep' || record.action === 'wake' ? record.action : undefined;
  const projectId = normalizeNonEmptyString(record.projectId)?.trim();
  const sessionId = normalizeNonEmptyString(record.sessionId)?.trim();
  const replacementProjectId = normalizeNonEmptyString(record.replacementProjectId)?.trim();
  const replacementSessionId = normalizeNonEmptyString(record.replacementSessionId)?.trim();
  if (
    !action ||
    !projectId ||
    !sessionId ||
    (record.skipReplacementFallback !== true && record.skipReplacementFallback !== false) ||
    (record.keepSidebarFocus !== undefined && record.keepSidebarFocus !== true) ||
    !gpuiWorkspaceLifecycleProjectIdAllowed(projectId) ||
    !gpuiLocalWorkspaceLifecycleSessionIdAllowed(sessionId)
  ) {
    return undefined;
  }
  if ((replacementProjectId && !replacementSessionId) || (!replacementProjectId && replacementSessionId)) {
    return undefined;
  }
  if (record.skipReplacementFallback === true && replacementProjectId && replacementSessionId) {
    return undefined;
  }
  if (
    replacementProjectId &&
    replacementSessionId &&
    (!gpuiWorkspaceLifecycleProjectIdAllowed(replacementProjectId) ||
      !gpuiLocalWorkspaceLifecycleSessionIdAllowed(replacementSessionId))
  ) {
    return undefined;
  }
  return {
    action,
    keepSidebarFocus: record.keepSidebarFocus === true,
    projectId,
    ...(replacementProjectId && replacementSessionId ? { replacementProjectId, replacementSessionId } : {}),
    requestId: record.requestId,
    sessionId,
    skipReplacementFallback: record.skipReplacementFallback,
  };
}

export function normalizeGpuiWorkspaceTerminalLifecycleRequest(
  value: unknown
): GpuiWorkspaceTerminalLifecycleRequest | undefined {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  if (
    Object.keys(record).some(
      (key) =>
        ![
          'action',
          'keepSidebarFocus',
          'projectId',
          'replacementProjectId',
          'replacementSessionId',
          'requestId',
          'sessionId',
          'skipReplacementFallback',
          'type',
          'version',
        ].includes(key)
    )
  ) {
    return undefined;
  }
  if (
    record.type !== GPUI_SIDEBAR_WORKSPACE_TERMINAL_LIFECYCLE_REQUEST_MESSAGE_TYPE ||
    record.version !== GPUI_SIDEBAR_WORKSPACE_TERMINAL_LIFECYCLE_REQUEST_MESSAGE_VERSION ||
    typeof record.requestId !== 'number' ||
    !Number.isSafeInteger(record.requestId) ||
    record.requestId <= 0
  ) {
    return undefined;
  }
  const action =
    record.action === 'close' || record.action === 'sleep' || record.action === 'wake' ? record.action : undefined;
  if (!action) {
    return undefined;
  }
  const projectId = normalizeNonEmptyString(record.projectId)?.trim();
  const sessionId = normalizeNonEmptyString(record.sessionId)?.trim();
  const replacementProjectId = normalizeNonEmptyString(record.replacementProjectId)?.trim();
  const replacementSessionId = normalizeNonEmptyString(record.replacementSessionId)?.trim();
  const skipReplacementFallback =
    record.skipReplacementFallback === undefined ? false : record.skipReplacementFallback === true;
  const keepSidebarFocus = record.keepSidebarFocus === undefined ? false : record.keepSidebarFocus === true;
  if (record.keepSidebarFocus !== undefined && record.keepSidebarFocus !== true) {
    return undefined;
  }
  if (
    !projectId ||
    !sessionId ||
    !gpuiWorkspaceLifecycleProjectIdAllowed(projectId) ||
    !gpuiLocalWorkspaceLifecycleSessionIdAllowed(sessionId)
  ) {
    return undefined;
  }
  if (record.skipReplacementFallback !== undefined && record.skipReplacementFallback !== true) {
    return undefined;
  }
  if ((replacementProjectId && !replacementSessionId) || (!replacementProjectId && replacementSessionId)) {
    return undefined;
  }
  if (skipReplacementFallback && replacementProjectId && replacementSessionId) {
    return undefined;
  }
  if (
    replacementProjectId &&
    replacementSessionId &&
    (!gpuiWorkspaceLifecycleProjectIdAllowed(replacementProjectId) ||
      !gpuiLocalWorkspaceLifecycleSessionIdAllowed(replacementSessionId))
  ) {
    return undefined;
  }
  return {
    action,
    keepSidebarFocus,
    projectId,
    ...(replacementProjectId && replacementSessionId ? { replacementProjectId, replacementSessionId } : {}),
    requestId: record.requestId,
    sessionId,
    skipReplacementFallback,
  };
}

export function didGpuiGxserverProviderTransitionCommit(result: GxserverSessionTransitionResult): boolean {
  /*
  CDXC:Workarea 2026-06-26-08:01:
  GPUI sleep must match macOS gxserver lifecycle ownership: `/api/transitionSession` resolving is not proof that zmx stopped. Only publish local sleep state after the returned session lifecycle matches the action, provider lifecycle is `missing`, and the optional kill result did not explicitly fail.
  */
  if (!isObjectRecord(result) || !isObjectRecord(result.session)) {
    return false;
  }
  const providerState = result.session.providerState;
  if (!isObjectRecord(providerState)) {
    return false;
  }
  const expectedLifecycleState = result.action === 'sleep' ? 'sleeping' : 'stopped';
  const killSucceeded = readGpuiTransitionKillSucceeded(
    isObjectRecord(result.transition) ? result.transition : undefined
  );
  return (
    result.session.lifecycleState === expectedLifecycleState &&
    providerState.lifecycleState === 'missing' &&
    killSucceeded !== false
  );
}

export function shouldApplyGpuiLocalWorkspaceTransition(
  result: GxserverSessionTransitionResult,
  action: 'close' | 'sleep'
): boolean {
  /*
  CDXC:Workarea 2026-06-26-23:44:
  macOS close and sleep intentionally diverge after gxserver handles a provider transition. Close removes the local pane/sidebar row once `/api/transitionSession` returns a valid close result, even when provider kill did not commit; sleep must stay strict so GPUI does not show a cold sleeping placeholder while the zmx runtime is still live.
  */
  if (!isObjectRecord(result) || result.action !== action || !isObjectRecord(result.session)) {
    return false;
  }
  return action === 'close' || didGpuiGxserverProviderTransitionCommit(result);
}

export function readGpuiTransitionKillSucceeded(transition: Record<string, unknown> | undefined): boolean | undefined {
  const kill = transition?.kill;
  if (!isObjectRecord(kill)) {
    return undefined;
  }
  return typeof kill.killed === 'boolean' ? kill.killed : undefined;
}

export function gpuiWorkspaceLifecycleProjectIdAllowed(value: string): boolean {
  return (
    gpuiLocalWorkspaceLifecycleProjectIdAllowed(value) || parseGpuiRemotePresentationProjectId(value) !== undefined
  );
}

export function gpuiLocalWorkspaceLifecycleProjectIdAllowed(value: string): boolean {
  return /^P[0-9][a-z0-9]{0,30}$/u.test(value);
}

export function gpuiLocalWorkspaceLifecycleSessionIdAllowed(value: string): boolean {
  return (
    gpuiStatusPetActivationSessionIdAllowed(value) &&
    !value.includes(':') &&
    !parseGpuiRemotePresentationSessionId(value) &&
    !parseGxserverPresentationProjectSessionId(value)
  );
}
