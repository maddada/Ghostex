export type RemoteCodeServerComponentPlatform = 'linux-x64' | 'linux-arm64';

export type RemoteCodeViewTarget = Readonly<{
  kind: 'remote';
  machineId: string;
  projectId: string;
  projectPath: string;
  connectionGeneration: number;
  componentPlatform: RemoteCodeServerComponentPlatform;
}>;

export type RemoteCodeViewLaunchRequest = Readonly<{
  target: RemoteCodeViewTarget;
}>;

export type RemotePromptEditorTarget = Readonly<{
  codeView: RemoteCodeViewTarget;
  sessionId: string;
}>;

export type RemotePromptEditorShortcutPlan = Readonly<{
  activateCodeView: true;
  attachCapability: 'code-server';
  codeView: RemoteCodeViewLifecyclePlan;
  delivery: RemotePromptEditorDeliveryDecision;
  deliverControlG: boolean;
  target: RemotePromptEditorTarget;
}>;

export type RemotePromptEditorDeliveryDecision =
  | Readonly<{ status: 'waiting'; request: RemotePromptEditorTarget }>
  | Readonly<{ status: 'deliver'; request: RemotePromptEditorTarget }>
  | Readonly<{
      status: 'cancelled';
      reason: 'runtime-failed' | 'session-closed' | 'stale-request' | 'stale-runtime';
      request: RemotePromptEditorTarget;
    }>;

export type RemoteCodeViewLifecycleState =
  | Readonly<{ status: 'idle' }>
  | Readonly<{ status: 'launching'; target: RemoteCodeViewTarget }>
  | Readonly<{
      status: 'ready';
      target: RemoteCodeViewTarget;
      runtimeOrigin: string;
      promptEditorIpcReady: boolean;
    }>
  | Readonly<{
      status: 'failed';
      target: RemoteCodeViewTarget;
      error: string;
    }>;

export type RemoteCodeViewLifecyclePlan = Readonly<{
  cleanup: boolean;
  launch: RemoteCodeViewLaunchRequest | null;
  state: RemoteCodeViewLifecycleState;
}>;

export function remoteCodeViewConnectionKey(target: RemoteCodeViewTarget): string {
  return `${target.machineId}\u0000${target.connectionGeneration}`;
}

export function remoteCodeViewTargetKey(target: RemoteCodeViewTarget): string {
  return `${remoteCodeViewConnectionKey(target)}\u0000${target.projectId}\u0000${target.projectPath}`;
}

export function selectRemoteCodeViewTarget(
  current: RemoteCodeViewLifecycleState,
  target: RemoteCodeViewTarget
): RemoteCodeViewLifecyclePlan {
  if (current.status !== 'idle') {
    const sharesConnection = remoteCodeViewConnectionKey(current.target) === remoteCodeViewConnectionKey(target);
    if (sharesConnection && current.status === 'ready') {
      return {
        cleanup: false,
        launch: null,
        state: { ...current, target },
      };
    }
    if (remoteCodeViewTargetKey(current.target) === remoteCodeViewTargetKey(target)) {
      return { cleanup: false, launch: null, state: current };
    }
  }
  return {
    cleanup: current.status !== 'idle',
    launch: { target },
    state: { status: 'launching', target },
  };
}

export function finishRemoteCodeViewLaunch(
  current: RemoteCodeViewLifecycleState,
  target: RemoteCodeViewTarget,
  result:
    | Readonly<{ ok: true; runtimeOrigin: string; promptEditorIpcReady: boolean }>
    | Readonly<{ ok: false; error: string }>
): RemoteCodeViewLifecycleState {
  if (current.status !== 'launching' || remoteCodeViewTargetKey(current.target) !== remoteCodeViewTargetKey(target)) {
    return current;
  }
  if (!result.ok) {
    return { status: 'failed', target, error: result.error };
  }
  return result.runtimeOrigin.trim() && result.promptEditorIpcReady
    ? {
        status: 'ready',
        target,
        runtimeOrigin: result.runtimeOrigin,
        promptEditorIpcReady: true,
      }
    : { status: 'failed', target, error: 'Remote Code runtime did not claim IPC ownership.' };
}

export function disconnectRemoteCodeView(
  current: RemoteCodeViewLifecycleState,
  machineId: string,
  connectionGeneration: number
): RemoteCodeViewLifecyclePlan {
  const ownsConnection =
    current.status !== 'idle' &&
    current.target.machineId === machineId &&
    current.target.connectionGeneration === connectionGeneration;
  return ownsConnection
    ? { cleanup: true, launch: null, state: { status: 'idle' } }
    : { cleanup: false, launch: null, state: current };
}

/**
 * Ctrl+G for a remote session is a request to edit a file on that session's
 * machine, not a request to reinterpret its path on the browser/app host.
 * Hosts activate the owned remote Code runtime for the exact connection and
 * advertise the fixed code-server capability to zmx; the remote CLI receives
 * the eventual file argument and opens it through that runtime's IPC.
 */
export function planRemotePromptEditorShortcut(
  current: RemoteCodeViewLifecycleState,
  target: RemotePromptEditorTarget
): RemotePromptEditorShortcutPlan {
  const codeView = selectRemoteCodeViewTarget(current, target.codeView);
  const delivery = resolveRemotePromptEditorRequest(codeView.state, target, target);
  return {
    activateCodeView: true,
    attachCapability: 'code-server',
    codeView,
    delivery,
    deliverControlG: delivery.status === 'deliver',
    target,
  };
}

/**
 * A host may deliver Ctrl+G only at the event edge where its runtime owner is
 * Ready for the exact queued request and still owns both its HTTP runtime and
 * the configured prompt-editor IPC socket.
 * Reconnect, project/session replacement, close, and failure cancel the
 * request; callers must not turn the CLI's own wait into a readiness signal.
 */
export function resolveRemotePromptEditorRequest(
  current: RemoteCodeViewLifecycleState,
  request: RemotePromptEditorTarget,
  authoritativeTarget: RemotePromptEditorTarget | null
): RemotePromptEditorDeliveryDecision {
  if (!authoritativeTarget) {
    return { status: 'cancelled', reason: 'session-closed', request };
  }
  if (
    authoritativeTarget.sessionId !== request.sessionId ||
    remoteCodeViewTargetKey(authoritativeTarget.codeView) !== remoteCodeViewTargetKey(request.codeView)
  ) {
    return { status: 'cancelled', reason: 'stale-request', request };
  }
  if (
    current.status === 'launching' &&
    remoteCodeViewTargetKey(current.target) === remoteCodeViewTargetKey(request.codeView)
  ) {
    return { status: 'waiting', request };
  }
  if (
    current.status === 'ready' &&
    current.runtimeOrigin.trim() &&
    current.promptEditorIpcReady &&
    remoteCodeViewTargetKey(current.target) === remoteCodeViewTargetKey(request.codeView)
  ) {
    return { status: 'deliver', request };
  }
  if (
    current.status === 'failed' &&
    remoteCodeViewTargetKey(current.target) === remoteCodeViewTargetKey(request.codeView)
  ) {
    return { status: 'cancelled', reason: 'runtime-failed', request };
  }
  return { status: 'cancelled', reason: 'stale-runtime', request };
}
