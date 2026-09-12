import { AccountPrivacyContext } from '@/packages/core-ui/accounts/account-text';
import { createSessionChatDiagnosticRecorder } from '@/packages/core-ui/chat/session-chat-diagnostics';
import { replayDraftSaves } from '@/packages/core-ui/chat/session-chat-draft-outbox';
import { importDraftRecovery } from '@/packages/core-ui/chat/session-chat-draft-recovery';
import { reconcileSessionChatDraftsFromServer } from '@/packages/core-ui/chat/session-chat-draft-storage';
import type { SessionChatBarExtension } from '@/packages/core-ui/chat/session-chat-extension-panel';
import { sessionChatDraftClientId } from '@/packages/core-ui/chat/session-chat-queue';
import type { SessionChatTransport } from '@/packages/core-ui/chat/session-chat-transport';
import {
  SessionChatView,
  type SessionChatHostActions,
  type SessionChatHostComposerBridge,
  type SessionChatHostLinks,
} from '@/packages/core-ui/chat/session-chat-view';
import type {
  GhostexBridgeError,
  GhostexExecChunk,
  GhostexExtensionContext,
} from '@/packages/shared/ghostex-extension-sdk';
import {
  GHOSTEX_CHAT_BAR_CONTEXT_CHANGED_EVENT,
  GHOSTEX_CHAT_BAR_PANEL_STORAGE_KEY,
  type GhostexChatBarBridgeRequestMessage,
  type GhostexChatBarPanelSessions,
  type GhostexChatBarPanelSessionState,
  type GhostexChatBarPanelToggleMessage,
  type GhostexExtensionLaunchContext,
  type GhostexExtensionRuntimeResult,
  type GhostexInstalledExtension,
  type GhostexListExtensionsResult,
  type GhostexSetExtensionStateResult,
} from '@/packages/shared/ghostex-extensions';
import { normalizeghostexHotkeySettings } from '@/packages/shared/ghostex-hotkeys';
import type { GxserverListSessionChatDraftsResult } from '@/packages/shared/gxserver-protocol';
import {
  type GxserverPresentationProject,
  type GxserverPresentationSession,
  type GxserverPresentationSnapshot,
  type GxserverSessionForkBranch,
} from '@/packages/shared/gxserver-protocol';
import { type SessionChatTheme } from '@/packages/shared/session-chat';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import type { ChatBridgeNamespace } from './chat-page-runtime';

interface ChatPageContext {
  rpc: <T>(
    bootstrap: { authToken: string; baseUrl: string },
    path: string,
    params: Record<string, unknown>,
    signal?: AbortSignal
  ) => Promise<T>;
  chatBridgeNamespace: () => ChatBridgeNamespace;
  appModalHostMessageHandler: () => { postMessage: (payload: string) => unknown } | undefined;
  postSessionChatHostAction: (action: string, fields?: Record<string, unknown>) => boolean;
  postSessionChatDiagnosticLog: ReturnType<typeof createSessionChatDiagnosticRecorder>;
  pageGeneration: string;
  searchParams: URLSearchParams;
  getSettings: () => {
    hideAccountEmails: boolean;
    chatCustomTranscriptWidthEnabled: boolean;
    hotkeysValue: unknown;
    GPUI_SESSION_CHAT_HOST_ACTIONS: SessionChatHostActions;
    GPUI_SESSION_CHAT_HOST_LINKS: SessionChatHostLinks;
    chatVerboseMode: boolean;
    chatFileEditPreviews: boolean;
    remote: boolean;
  };
}

export function createGpuiSessionChatPage({
  rpc,
  chatBridgeNamespace,
  appModalHostMessageHandler,
  postSessionChatHostAction,
  postSessionChatDiagnosticLog,
  pageGeneration,
  searchParams,
  getSettings,
}: ChatPageContext) {
  interface GxserverReadPresentationSnapshotResult {
    snapshot: GxserverPresentationSnapshot;
  }

  function recordText(value: unknown, key: string): string | undefined {
    if (!value || typeof value !== 'object') {
      return undefined;
    }
    const field = (value as Record<string, unknown>)[key];
    return typeof field === 'string' && field.trim() ? field : undefined;
  }

  function isChatBarExtension(extension: GhostexInstalledExtension): boolean {
    return (
      extension.state.enabled &&
      extension.manifest.placements?.includes('chat-bar') === true &&
      (extension.state.placement ?? extension.manifest.defaultPlacement) === 'chat-bar'
    );
  }

  function readChatBarPanelSessions(value: unknown): GhostexChatBarPanelSessions {
    if (!value || typeof value !== 'object' || Array.isArray(value)) {
      return {};
    }
    const sessions: GhostexChatBarPanelSessions = {};
    for (const [sessionKey, candidate] of Object.entries(value)) {
      if (!candidate || typeof candidate !== 'object' || Array.isArray(candidate)) {
        continue;
      }
      const state = candidate as Partial<GhostexChatBarPanelSessionState>;
      if (typeof state.open !== 'boolean' || typeof state.minimized !== 'boolean') {
        continue;
      }
      sessions[sessionKey] = {
        open: state.open,
        minimized: state.minimized,
        ...(typeof state.activeExtensionId === 'string' && state.activeExtensionId
          ? { activeExtensionId: state.activeExtensionId }
          : {}),
      };
    }
    return sessions;
  }

  function extensionIconUrl(extension: GhostexInstalledExtension): string | undefined {
    const icon = extension.manifest.icon.trim();
    if (!icon) {
      return undefined;
    }
    if (icon.startsWith('data:') || icon.startsWith('http://') || icon.startsWith('https://')) {
      return icon;
    }
    const runtimeUrl = extension.runtime.url;
    if (!runtimeUrl) {
      return undefined;
    }
    try {
      return new URL(icon, runtimeUrl).toString();
    } catch {
      return undefined;
    }
  }

  function extensionLaunchContext(
    project: GxserverPresentationProject,
    session: GxserverPresentationSession
  ): GhostexExtensionLaunchContext {
    return {
      sessionId: session.sessionId,
      projectName: project.title,
      ...(project.path ? { projectPath: project.path } : {}),
      worktree: Boolean(project.worktree),
      ...(recordText(project.worktree, 'branch') ? { worktreeBranch: recordText(project.worktree, 'branch') } : {}),
    };
  }

  interface NativeExtensionBridgeMessage {
    requestId: string;
    ok?: boolean;
    result?: unknown;
    chunk?: GhostexExecChunk;
    error?: {
      code?: GhostexBridgeError['code'];
      message?: string;
      permission?: GhostexBridgeError['permission'];
    };
  }

  interface PendingNativeExtensionBridgeCall {
    onChunk: (chunk: GhostexExecChunk) => void;
    reject: (error: GhostexBridgeError) => void;
    resolve: (result: unknown) => void;
    timer: ReturnType<typeof setTimeout>;
    refreshDeadline: () => void;
  }

  const NATIVE_BRIDGE_TIMEOUT_MS = 180_000;
  /**
   * CDXC:Extensions 2026-09-12 WHY:
   * Native exec and cli have no execution deadline or cancellation contract, so their bridge waiters allow a day of silence and streamed output renews that deadline.
   * Expiry releases the unanswered bridge call; it does not cancel or report completion of the native command.
   */
  const NATIVE_COMMAND_IDLE_TIMEOUT_MS = 24 * 60 * 60 * 1_000;

  function nativeExtensionBridgeError(
    code: GhostexBridgeError['code'],
    message: string,
    permission?: GhostexBridgeError['permission']
  ): GhostexBridgeError {
    return Object.assign(new Error(message), { code, ...(permission ? { permission } : {}) });
  }

  interface GpuiSessionChatPageProps {
    agentLabel: string | null;
    bootstrap: { authToken: string; baseUrl: string };
    composerBridge: SessionChatHostComposerBridge;
    projectId: string;
    sessionId: string;
    theme: SessionChatTheme;
    transport: SessionChatTransport;
  }

  function GpuiSessionChatPage({
    agentLabel,
    bootstrap,
    composerBridge,
    projectId,
    sessionId,
    theme,
    transport,
  }: GpuiSessionChatPageProps) {
    const {
      hideAccountEmails,
      chatCustomTranscriptWidthEnabled,
      hotkeysValue,
      GPUI_SESSION_CHAT_HOST_ACTIONS,
      GPUI_SESSION_CHAT_HOST_LINKS,
      chatVerboseMode,
      chatFileEditPreviews,
      remote,
    } = getSettings();
    const sessionKey = `${projectId}:${sessionId}`;
    const remoteMachineId = searchParams.get('remoteMachineId');
    const draftSessionKey = remoteMachineId ? `remote-${remoteMachineId}:${sessionKey}` : sessionKey;
    useEffect(() => {
      if (!remoteMachineId) return;
      const prefix = `remote-${remoteMachineId}:`;
      replayDraftSaves(prefix, async (draft, projectId, sessionId) => {
        await rpc(bootstrap, '/api/setSessionChatDraft', {
          projectId,
          sessionId,
          content: draft.content,
          draftVersion: draft.version,
          clientId: sessionChatDraftClientId(),
        });
      });
      void rpc<GxserverListSessionChatDraftsResult>(bootstrap, '/api/listSessionChatDrafts', {})
        .then((result) => {
          importDraftRecovery(result.recoveryDrafts, prefix);
          reconcileSessionChatDraftsFromServer(result.drafts, prefix);
        })
        .catch(() => {});
    }, [bootstrap, remoteMachineId]);
    const [sessionTitle, setSessionTitle] = useState('');
    const [extensions, setExtensions] = useState<GhostexInstalledExtension[]>([]);
    const extensionsRef = useRef<GhostexInstalledExtension[]>([]);
    const [panelState, setPanelState] = useState<GhostexChatBarPanelSessionState>({
      open: false,
      minimized: false,
    });
    const panelStateRef = useRef(panelState);
    const panelSessionsRef = useRef<GhostexChatBarPanelSessions>({});
    const ownerExtensionIdRef = useRef<string | undefined>(undefined);
    const extensionContextRef = useRef<{
      project: GxserverPresentationProject;
      session: GxserverPresentationSession;
    } | null>(null);
    const persistenceQueueRef = useRef<Promise<void>>(Promise.resolve());
    const nativeBridgeSequenceRef = useRef(0);
    const pendingNativeBridgeCallsRef = useRef(new Map<string, PendingNativeExtensionBridgeCall>());

    useEffect(() => {
      const namespace = chatBridgeNamespace();
      const previousMessageHandler = namespace.onSessionChatExtensionBridgeMessage;
      const previousContextHandler = namespace.onSessionChatExtensionContextChanged;
      const handleMessage = (payload: unknown): void => {
        if (!payload || typeof payload !== 'object') {
          return;
        }
        const message = payload as Partial<NativeExtensionBridgeMessage>;
        if (typeof message.requestId !== 'string') {
          return;
        }
        const pending = pendingNativeBridgeCallsRef.current.get(message.requestId);
        if (!pending) {
          return;
        }
        if (
          message.chunk &&
          (message.chunk.stream === 'stdout' || message.chunk.stream === 'stderr') &&
          typeof message.chunk.text === 'string'
        ) {
          pending.refreshDeadline();
          pending.onChunk(message.chunk);
          return;
        }
        pendingNativeBridgeCallsRef.current.delete(message.requestId);
        clearTimeout(pending.timer);
        if (message.ok === true) {
          pending.resolve(message.result);
          return;
        }
        const error = message.error;
        pending.reject(
          nativeExtensionBridgeError(
            error?.code === 'invalidRequest' ||
              error?.code === 'notFound' ||
              error?.code === 'permissionDenied' ||
              error?.code === 'operationFailed'
              ? error.code
              : 'operationFailed',
            typeof error?.message === 'string' ? error.message : 'The extension call failed.',
            error?.permission
          )
        );
      };
      const handleContextChanged = (context: GhostexExtensionContext): void => {
        if (context.activeSession?.title) {
          setSessionTitle(context.activeSession.title);
        }
        window.dispatchEvent(new CustomEvent(GHOSTEX_CHAT_BAR_CONTEXT_CHANGED_EVENT, { detail: context }));
      };
      namespace.onSessionChatExtensionBridgeMessage = handleMessage;
      namespace.onSessionChatExtensionContextChanged = handleContextChanged;
      const pendingCalls = pendingNativeBridgeCallsRef.current;
      return () => {
        if (namespace.onSessionChatExtensionBridgeMessage === handleMessage) {
          namespace.onSessionChatExtensionBridgeMessage = previousMessageHandler;
        }
        if (namespace.onSessionChatExtensionContextChanged === handleContextChanged) {
          namespace.onSessionChatExtensionContextChanged = previousContextHandler;
        }
        for (const pending of pendingCalls.values()) {
          clearTimeout(pending.timer);
          pending.reject(nativeExtensionBridgeError('operationFailed', 'The chat-bar extension host closed.'));
        }
        pendingCalls.clear();
      };
    }, []);

    const publishExtensions = useCallback((next: GhostexInstalledExtension[]): void => {
      extensionsRef.current = next;
      setExtensions(next);
    }, []);

    const replaceExtension = useCallback(
      (replacement: GhostexInstalledExtension): void => {
        publishExtensions(
          extensionsRef.current.map((extension) => (extension.id === replacement.id ? replacement : extension))
        );
      },
      [publishExtensions]
    );

    const startExtension = useCallback(
      async (extensionId: string): Promise<void> => {
        const extension = extensionsRef.current.find((candidate) => candidate.id === extensionId);
        const context = extensionContextRef.current;
        if (!extension || !context || (extension.runtime.state === 'ready' && extension.runtime.url)) {
          return;
        }
        replaceExtension({
          ...extension,
          runtime: { state: 'starting' },
        });
        try {
          const result = await rpc<GhostexExtensionRuntimeResult>(bootstrap, '/api/startExtension', {
            id: extensionId,
            context: extensionLaunchContext(context.project, context.session),
          });
          const current = extensionsRef.current.find((candidate) => candidate.id === extensionId);
          if (current) {
            replaceExtension({ ...current, runtime: result.status });
          }
        } catch (error) {
          const current = extensionsRef.current.find((candidate) => candidate.id === extensionId);
          if (current) {
            replaceExtension({
              ...current,
              runtime: {
                state: 'failed',
                error: error instanceof Error ? error.message : 'The extension could not be started.',
              },
            });
          }
        }
      },
      [bootstrap, replaceExtension]
    );

    useEffect(() => {
      let active = true;
      void Promise.all([
        rpc<GhostexListExtensionsResult>(bootstrap, '/api/listExtensions', {}),
        rpc<GxserverReadPresentationSnapshotResult>(bootstrap, '/api/readPresentationSnapshot', {}),
      ])
        .then(([extensionResult, presentationResult]) => {
          if (!active) {
            return;
          }
          const project = presentationResult.snapshot.projects.find((candidate) => candidate.projectId === projectId);
          const session = presentationResult.snapshot.sessions.find(
            (candidate) => candidate.projectId === projectId && candidate.sessionId === sessionId
          );
          if (!project || !session) {
            throw new Error(`Session ${sessionId} was not found in project ${projectId}.`);
          }
          setSessionTitle(session.displayTitle ?? session.primaryTitle ?? session.title);
          const chatBarExtensions = extensionResult.extensions
            .filter(isChatBarExtension)
            .sort((a, b) => a.id.localeCompare(b.id));
          const owner = chatBarExtensions[0];
          const autoOpenExtension = chatBarExtensions.find((extension) => extension.state.chatBarAutoOpen);
          const storedSessions = readChatBarPanelSessions(owner?.state.storage[GHOSTEX_CHAT_BAR_PANEL_STORAGE_KEY]);
          const storedState = storedSessions[sessionKey];
          const requestedActiveId = storedState?.activeExtensionId;
          const activeExtensionId = chatBarExtensions.some((extension) => extension.id === requestedActiveId)
            ? requestedActiveId
            : (autoOpenExtension?.id ?? chatBarExtensions[0]?.id);
          const initialState: GhostexChatBarPanelSessionState = {
            open: storedState?.open ?? Boolean(autoOpenExtension),
            minimized: storedState?.minimized ?? false,
            ...(activeExtensionId ? { activeExtensionId } : {}),
          };
          extensionContextRef.current = { project, session };
          ownerExtensionIdRef.current = owner?.id;
          panelSessionsRef.current = storedSessions;
          panelStateRef.current = initialState;
          publishExtensions(chatBarExtensions);
          setPanelState(initialState);
          if (initialState.open && activeExtensionId) {
            void startExtension(activeExtensionId);
          }
        })
        .catch((error: unknown) => {
          if (active) {
            console.error('Could not initialize chat-bar extensions.', error);
          }
        });
      return () => {
        active = false;
      };
    }, [bootstrap, projectId, publishExtensions, sessionId, sessionKey, startExtension]);

    const persistPanelState = useCallback(
      (next: GhostexChatBarPanelSessionState): void => {
        const ownerExtensionId = ownerExtensionIdRef.current;
        if (!ownerExtensionId) {
          return;
        }
        const nextSessions = { ...panelSessionsRef.current, [sessionKey]: next };
        panelSessionsRef.current = nextSessions;
        persistenceQueueRef.current = persistenceQueueRef.current
          .catch(() => undefined)
          .then(async () => {
            const result = await rpc<GhostexSetExtensionStateResult>(bootstrap, '/api/updateExtensionState', {
              id: ownerExtensionId,
              patch: {
                storage: { [GHOSTEX_CHAT_BAR_PANEL_STORAGE_KEY]: nextSessions },
              },
            });
            replaceExtension(result.extension);
          })
          .catch((error: unknown) => {
            console.error('Could not persist chat-bar panel state.', error);
          });
      },
      [bootstrap, replaceExtension, sessionKey]
    );

    const updatePanelState = useCallback(
      (patch: { activeExtensionId?: string; minimized?: boolean; open?: boolean }): void => {
        const next: GhostexChatBarPanelSessionState = {
          ...panelStateRef.current,
          ...patch,
        };
        panelStateRef.current = next;
        setPanelState(next);
        persistPanelState(next);
        if (next.open && patch.activeExtensionId) {
          void startExtension(patch.activeExtensionId);
        }
      },
      [persistPanelState, startExtension]
    );

    useEffect(() => {
      if (extensions.length === 0) {
        return;
      }
      const namespace = chatBridgeNamespace();
      const previous = namespace.onSessionChatExtensionRequested;
      const handleExtensionRequest = (payload: GhostexChatBarPanelToggleMessage): void => {
        const extensionId =
          payload?.type === 'ghostexChatBarPanelToggle' && typeof payload.extensionId === 'string'
            ? payload.extensionId
            : '';
        if (!extensionsRef.current.some((extension) => extension.id === extensionId)) {
          return;
        }
        updatePanelState({
          activeExtensionId: extensionId,
          minimized: false,
          open: !panelStateRef.current.open,
        });
      };
      namespace.onSessionChatExtensionRequested = handleExtensionRequest;
      return () => {
        if (namespace.onSessionChatExtensionRequested === handleExtensionRequest) {
          namespace.onSessionChatExtensionRequested = previous;
        }
      };
    }, [extensions.length, updatePanelState]);

    /*
  CDXC:SessionFork 2026-08-28:
  Switching branches is a workspace focus change, and this page is bound to one
  session, so it asks the daemon to do it: `/api/focusSession` dispatches the
  renderer command the sidebar page already answers with its normal focusSession
  routing (pane selection, terminal materialization, presentation focus). No new
  native message shape is involved.

  Remote chats deliberately get NO switch: their bootstrap points at the remote
  machine's own daemon through the SSH tunnel, and that daemon has no renderer
  attached from this Mac, so the command would only sit until it timed out. The
  switcher then lists the family read-only, which is the honest state.

  CDXC:SessionFork 2026-09-03:
  A fork's ancestor is usually STOPPED: forking kills the source's provider and
  Previous Sessions hides the row. A stopped row is not in the live
  presentation, so `/api/focusSession` alone answers "No matching session was
  found" and the click did nothing. Wake it first: `/api/wakeSession` has no
  lifecycle guard, respawns the provider with the row's saved agent resume
  command, marks the SAME registry row running, and broadcasts its presentation
  delta before it answers, so the follow-up focus resolves. Reviving in place
  keeps the family edge intact; a Previous-Sessions-style restore would create a
  new row and remove the parent both leaves point at.
  */
    const focusForkBranch = useCallback(
      (branch: GxserverSessionForkBranch): void => {
        const target = { projectId: branch.projectId, sessionId: branch.sessionId };
        const wake =
          branch.lifecycleState === 'stopped'
            ? rpc(bootstrap, '/api/wakeSession', target).then(() => undefined)
            : Promise.resolve();
        void wake
          .then(() => rpc(bootstrap, '/api/focusSession', target))
          .catch((error: unknown) => {
            postSessionChatDiagnosticLog('sessionChat.forkBranchSwitchFailed', {
              lifecycleState: branch.lifecycleState,
              message: error instanceof Error ? error.message : String(error),
            });
          });
      },
      [bootstrap]
    );

    const handleBridgeRequest = useCallback(
      async (
        extensionId: string,
        request: GhostexChatBarBridgeRequestMessage,
        onChunk: (chunk: GhostexExecChunk) => void
      ): Promise<unknown> => {
        const extension = extensionsRef.current.find((candidate) => candidate.id === extensionId);
        if (!extension) {
          throw nativeExtensionBridgeError('notFound', 'The chat-bar extension is not available.');
        }
        const handler = appModalHostMessageHandler();
        if (!handler) {
          throw nativeExtensionBridgeError('operationFailed', 'The native extension bridge is unavailable.');
        }
        const nativeRequestId = `chat-bar-${pageGeneration}-${Date.now().toString(36)}-${(++nativeBridgeSequenceRef.current).toString(36)}`;
        return new Promise((resolve, reject) => {
          const command = request.method === 'exec' || request.method === 'cli';
          const timeoutMs = command ? NATIVE_COMMAND_IDLE_TIMEOUT_MS : NATIVE_BRIDGE_TIMEOUT_MS;
          const expire = (): void => {
            if (!pendingNativeBridgeCallsRef.current.delete(nativeRequestId)) return;
            reject(
              nativeExtensionBridgeError(
                'operationFailed',
                command
                  ? 'The native command stopped responding. The command may still be running.'
                  : 'The native extension call timed out.'
              )
            );
          };
          const pending: PendingNativeExtensionBridgeCall = {
            onChunk,
            reject,
            resolve,
            timer: setTimeout(expire, timeoutMs),
            refreshDeadline: () => {
              clearTimeout(pending.timer);
              pending.timer = setTimeout(expire, timeoutMs);
            },
          };
          pendingNativeBridgeCallsRef.current.set(nativeRequestId, pending);
          try {
            const accepted = handler.postMessage(
              JSON.stringify({
                type: 'sessionChatExtensionBridgeRequest',
                pageGeneration: pageGeneration,
                extensionId,
                request: {
                  requestId: nativeRequestId,
                  method: request.method,
                  params: request.params ?? {},
                },
              })
            );
            if (accepted === false) {
              pendingNativeBridgeCallsRef.current.delete(nativeRequestId);
              clearTimeout(pending.timer);
              reject(nativeExtensionBridgeError('operationFailed', 'Ghostex rejected the extension call.'));
            }
          } catch {
            pendingNativeBridgeCallsRef.current.delete(nativeRequestId);
            clearTimeout(pending.timer);
            reject(nativeExtensionBridgeError('operationFailed', 'Ghostex could not send the extension call.'));
          }
        });
      },
      []
    );

    const chatBarExtensions = useMemo<SessionChatBarExtension[]>(
      () =>
        extensions.map((extension) => ({
          id: extension.id,
          title: extension.manifest.title,
          ...(extensionIconUrl(extension) ? { iconUrl: extensionIconUrl(extension) } : {}),
          ...(extension.runtime.url ? { url: extension.runtime.url } : {}),
          ...(extension.runtime.error ? { error: extension.runtime.error } : {}),
        })),
      [extensions]
    );

    return (
      <AccountPrivacyContext value={hideAccountEmails}>
        <div className='native-sidebar-shell gpui-session-chat'>
          <SessionChatView
            agentLabel={agentLabel}
            chatBarExtensions={chatBarExtensions}
            chatBarPanelState={panelState}
            className='gpui-session-chat-view'
            customTranscriptWidthEnabled={chatCustomTranscriptWidthEnabled}
            diagnosticLog={postSessionChatDiagnosticLog}
            hostActions={GPUI_SESSION_CHAT_HOST_ACTIONS}
            hotkeys={normalizeghostexHotkeySettings(hotkeysValue)}
            hostComposerBridge={composerBridge}
            hostLinks={GPUI_SESSION_CHAT_HOST_LINKS}
            inputBackend='lexical'
            onChatBarBridgeRequest={handleBridgeRequest}
            onChatBarPanelStateChange={updatePanelState}
            onDelayedActions={() => postSessionChatHostAction('delayedActions')}
            {...(remote ? {} : { onSelectForkBranch: focusForkBranch })}
            sessionKey={draftSessionKey}
            sessionTitle={sessionTitle}
            theme={theme}
            transport={transport}
            verboseMode={chatVerboseMode}
            fileEditPreviews={chatFileEditPreviews}
          />
        </div>
      </AccountPrivacyContext>
    );
  }

  return GpuiSessionChatPage;
}
