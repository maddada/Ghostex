import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from 'react';
import {
  getghostexHotkeyActionIdForKey,
  ghostexHotkeyTextFromKeyboardEvent,
  normalizeghostexHotkeySettings,
} from '@/packages/shared/ghostex-hotkeys';
import { hasActiveSidebarHotkeyRecorder } from '@/packages/core-ui/sidebar-app/session-ordering';
import type {
  GxserverPresentationSession,
  GxserverProjectId,
  GxserverSidebarHudCommandButton,
  GxserverSessionDomainState,
  GxserverSessionId,
} from '@/packages/shared/gxserver-protocol';
import { getConnectionStates, rpcForMachine, subscribeConnectionStates } from '../connections/connection-registry';
import type { MachineConnectionState } from '../connections/types';
import { getActiveSidebarProject, subscribeActiveSidebarProject } from '../sidebar-runtime/active-project-store';
import type { GhostexWebFocusSessionDetail } from '../sidebar-runtime/sidebar-runtime';
import { SessionTerminal, type SessionTerminalHandle, type TerminalWsClientError } from '../terminal';
import { AgentsWorkspace, type WorkspaceFindEvent, type WorkspaceOpenRequest } from '../workspace/agents-workspace';
import {
  workspaceLeaf,
  workspaceSession,
  workspaceSessionId,
  type WorkspaceModel,
  type WorkspacePlaceholderAction,
  type WorkspaceSession,
  type WorkspaceSplitAxis,
} from '../workspace/workspace-model';
import { prepareSessionAttach, RestoreBlockedError, type AttachIntent } from './attach-flow';
import './action-events';
import { CommandPane, type CommandPaneOpenRequest } from './command-pane';
import { readWebSettings } from './web-settings';
import { resolveSessionChatTranscriptAgent } from '@/packages/shared/session-chat';
import { SessionTerminalActionBar } from '@/packages/core-ui/chat/session-terminal-action-bar';
import { SessionChatQueuedPromptsButton } from '../chat/session-chat-queued-prompts-button';
import { createWebSessionHostActions, SessionChatHost } from './session-chat-host';
import {
  createWorkspaceSessionId,
  domainSessionToWorkspaceSession,
  presentationSessionToWorkspaceSession,
  type SessionReference,
} from './session-mapping';
import type { ActiveProject } from './types';
import './app.css';

interface PendingStartupText extends SessionReference {
  delivered: boolean;
  text: string;
}

interface OpenOptions {
  placementTargetSessionId?: string;
  splitAxis?: WorkspaceSplitAxis;
  targetPaneId?: string;
}

/**
 * The terminal bar's left label. `agentSessionId` is the agent's own
 * conversation id, which every supported agent writes as a UUID
 * (`01a00854-13cb-7500-…`), so the first group alone is short enough to sit in
 * a bar and still long enough to match against `ghostex sessions` output.
 */
function shortAgentSessionId(agentSessionId: string | undefined): string | undefined {
  const trimmed = agentSessionId?.trim();
  return trimmed ? trimmed.slice(0, 8) : undefined;
}

async function sendCommandSessionText(session: SessionReference, text: string): Promise<void> {
  await rpcForMachine(session.machineId, '/api/sendSessionText', {
    projectId: session.projectId,
    sessionId: session.sessionId,
    text,
  });
  await rpcForMachine(session.machineId, '/api/sendSessionEnter', {
    projectId: session.projectId,
    sessionId: session.sessionId,
  });
}

export function IntegratedAgentsPage() {
  const connections = useSyncExternalStore(subscribeConnectionStates, getConnectionStates, getConnectionStates);
  const activeTarget = useSyncExternalStore(
    subscribeActiveSidebarProject,
    getActiveSidebarProject,
    getActiveSidebarProject
  );
  const [openRequest, setOpenRequest] = useState<WorkspaceOpenRequest>();
  const [pendingFocus, setPendingFocus] = useState<GhostexWebFocusSessionDetail>();
  const [error, setError] = useState<string>();
  const [localCommandSessions, setLocalCommandSessions] = useState<Map<string, WorkspaceSession>>(() => new Map());
  const [hiddenCommandSessions, setHiddenCommandSessions] = useState<Set<string>>(() => new Set());
  const [commandPaneOpenRequest, setCommandPaneOpenRequest] = useState<CommandPaneOpenRequest>();
  const requestId = useRef(0);
  const commandPaneRequestId = useRef(0);
  const attachGenerations = useRef(new Map<string, number>());
  const workspaceModel = useRef<WorkspaceModel | undefined>(undefined);
  const terminalHandles = useRef(new Map<string, SessionTerminalHandle>());
  const pendingStartupText = useRef(new Map<string, PendingStartupText>());

  const availableWorkspaceSessions = useMemo(
    () =>
      connections.flatMap(
        (state) =>
          state.presentation?.sessions.flatMap((session) =>
            session.surface === 'workspace'
              ? [presentationSessionToWorkspaceSession(state.machine.machineId, session)]
              : []
          ) ?? []
      ),
    [connections]
  );
  const authoritativeMachineIds = useMemo(
    () => connections.flatMap((state) => (state.presentation ? [state.machine.machineId] : [])),
    [connections]
  );
  const activeProject = useMemo(() => resolveActiveProject(activeTarget, connections), [activeTarget, connections]);
  const commandSessions = useMemo(() => {
    const merged = new Map<string, WorkspaceSession>();
    for (const state of connections) {
      for (const session of state.presentation?.sessions ?? []) {
        if (session.surface === 'commands') {
          const workspace = presentationSessionToWorkspaceSession(state.machine.machineId, session);
          merged.set(workspaceSessionId(workspace), workspace);
        }
      }
    }
    for (const [id, session] of localCommandSessions) {
      merged.set(id, session);
    }
    return [...merged.values()].filter(
      (session) =>
        activeProject &&
        session.machineId === activeProject.machineId &&
        session.projectId === activeProject.projectId &&
        !hiddenCommandSessions.has(workspaceSessionId(session))
    );
  }, [activeProject, connections, hiddenCommandSessions, localCommandSessions]);
  const activeMachine = connections.find((state) => state.machine.machineId === activeProject?.machineId)?.machine;

  const emitOpen = useCallback((session: WorkspaceSession, options: OpenOptions = {}) => {
    setOpenRequest({
      ...options,
      requestId: ++requestId.current,
      session,
    });
  }, []);

  const rememberStartupText = useCallback(
    (reference: SessionReference, prepared: Awaited<ReturnType<typeof prepareSessionAttach>>) => {
      if (
        prepared.startupTextDisposition === 'queueAfterTerminalReady' &&
        prepared.persistenceSessionCreated === true &&
        prepared.startupText
      ) {
        pendingStartupText.current.set(createWorkspaceSessionId(reference), {
          ...reference,
          delivered: false,
          text: prepared.startupText,
        });
      }
    },
    []
  );

  const attachWorkspaceSession = useCallback(
    async (
      reference: SessionReference,
      intent: AttachIntent,
      options: OpenOptions = {},
      current?: WorkspaceSession
    ) => {
      const id = createWorkspaceSessionId(reference);
      const generation = (attachGenerations.current.get(id) ?? 0) + 1;
      attachGenerations.current.set(id, generation);
      const baseSession = current ?? findPresentationWorkspaceSession(connections, reference);
      if (!baseSession) {
        setError('The selected session is no longer present in gxserver.');
        return;
      }
      emitOpen({ ...baseSession, presentationState: 'mounting', statusMessage: undefined }, options);
      try {
        const prepared = await prepareSessionAttach(reference, intent);
        if (attachGenerations.current.get(id) !== generation) {
          return;
        }
        rememberStartupText(reference, prepared);
        emitOpen(domainSessionToWorkspaceSession(reference.machineId, prepared.attach.session, 'running'), options);
        setError(undefined);
      } catch (nextError) {
        if (attachGenerations.current.get(id) !== generation) {
          return;
        }
        const message = nextError instanceof Error ? nextError.message : String(nextError);
        emitOpen(
          {
            ...baseSession,
            presentationState: 'startup-failed',
            statusMessage: message,
          },
          options
        );
        setError(nextError instanceof RestoreBlockedError ? nextError.message : message);
      }
    },
    [connections, emitOpen, rememberStartupText]
  );

  const focusSidebarSession = useCallback(
    (detail: GhostexWebFocusSessionDetail) => {
      const reference: SessionReference = detail;
      const session = findPresentationWorkspaceSession(connections, reference);
      if (!session) {
        setPendingFocus(detail);
        return;
      }
      setPendingFocus(undefined);
      const id = workspaceSessionId(session);
      const options: OpenOptions = detail.placementTargetSessionId
        ? {
            placementTargetSessionId: createWorkspaceSessionId({
              machineId: detail.machineId,
              projectId: detail.projectId,
              sessionId: detail.placementTargetSessionId,
            }),
          }
        : {};
      if (workspaceModel.current && workspaceSession(workspaceModel.current, id)) {
        emitOpen(session, options);
        return;
      }
      if (session.presentationState === 'sleeping') {
        emitOpen(session, options);
        return;
      }
      void attachWorkspaceSession(reference, 'attach', options, session);
    },
    [attachWorkspaceSession, connections, emitOpen]
  );

  useWindowFocusSession(focusSidebarSession);

  useEffect(() => {
    if (pendingFocus && findPresentationWorkspaceSession(connections, pendingFocus)) {
      focusSidebarSession(pendingFocus);
    }
  }, [connections, focusSidebarSession, pendingFocus]);

  useEffect(() => {
    setLocalCommandSessions((current) => {
      const next = new Map(current);
      let changed = false;
      for (const [id, localSession] of current) {
        const live = findPresentationSession(connections, localSession);
        if (live && live.lifecycleState === 'running' && live.providerSessionState === 'exists') {
          next.delete(id);
          changed = true;
        }
      }
      return changed ? next : current;
    });
  }, [connections]);

  const deliverStartupText = useCallback(async (session: WorkspaceSession) => {
    const id = workspaceSessionId(session);
    const pending = pendingStartupText.current.get(id);
    if (!pending || pending.delivered) {
      return;
    }
    pending.delivered = true;
    try {
      await rpcForMachine(pending.machineId, '/api/sendSessionText', {
        projectId: pending.projectId,
        sessionId: pending.sessionId,
        text: pending.text,
      });
      await rpcForMachine(pending.machineId, '/api/sendSessionEnter', {
        projectId: pending.projectId,
        sessionId: pending.sessionId,
      });
      pendingStartupText.current.delete(id);
    } catch (nextError) {
      pending.delivered = false;
      setError(nextError instanceof Error ? nextError.message : String(nextError));
    }
  }, []);

  const createWorkspaceTerminal = useCallback(
    async (paneId: string, splitAxis?: WorkspaceSplitAxis) => {
      if (!activeProject) {
        setError('Select a project before creating a terminal.');
        return;
      }
      try {
        const { session } = await rpcForMachine<{ session: GxserverSessionDomainState }>(
          activeProject.machineId,
          '/api/createSession',
          {
            kind: 'terminal',
            lifecycleState: 'running',
            projectId: activeProject.projectId,
            surface: 'workspace',
            title: 'Terminal',
          }
        );
        await attachWorkspaceSession(
          {
            machineId: activeProject.machineId,
            projectId: session.projectId,
            sessionId: session.sessionId,
          },
          'attach',
          { splitAxis, targetPaneId: paneId },
          domainSessionToWorkspaceSession(activeProject.machineId, session, 'mounting')
        );
      } catch (nextError) {
        setError(nextError instanceof Error ? nextError.message : String(nextError));
      }
    },
    [activeProject, attachWorkspaceSession]
  );

  const handlePlaceholderAction = useCallback(
    (session: WorkspaceSession, action: WorkspacePlaceholderAction) => {
      void attachWorkspaceSession(session, action === 'wake' ? 'wake' : 'attach', {}, session);
    },
    [attachWorkspaceSession]
  );

  const handleFind = useCallback((event: WorkspaceFindEvent) => {
    if (!event.sessionId) {
      return;
    }
    const terminal = terminalHandles.current.get(event.sessionId);
    const query = event.query ?? '';
    if (event.type === 'close' || (event.type === 'query' && !query)) {
      terminal?.clearSearch();
    } else if (event.type === 'query') {
      terminal?.searchNext(query, { incremental: true });
    } else if (event.type === 'next') {
      terminal?.searchNext(query);
    } else if (event.type === 'previous') {
      terminal?.searchPrev(query);
    }
  }, []);

  const createCommandTerminal = useCallback(
    async (project: ActiveProject, action?: GxserverSidebarHudCommandButton) => {
      const startupText = action?.command?.trim();
      const reusableSession = action
        ? commandSessions.find(
            (session) =>
              session.commandId === action.commandId &&
              session.presentationState !== 'mounting' &&
              session.presentationState !== 'startup-failed'
          )
        : undefined;
      if (reusableSession) {
        const id = workspaceSessionId(reusableSession);
        setHiddenCommandSessions((current) => {
          const next = new Set(current);
          next.delete(id);
          return next;
        });
        setCommandPaneOpenRequest({
          requestId: ++commandPaneRequestId.current,
          sessionId: id,
        });

        /*
         * CDXC:CommandPane 2026-08-09:
         * The stable commandId owns one command pane across client reloads. A
         * non-idle owner is selected without submitting the Action a second time;
         * an idle running owner receives the next run directly, while an idle
         * sleeping/restored owner wakes first and receives it exactly once.
         */
        if (!startupText || reusableSession.activity !== 'idle') {
          return;
        }
        if (reusableSession.presentationState === 'running') {
          await sendCommandSessionText(reusableSession, startupText);
          return;
        }

        setLocalCommandSessions((current) =>
          new Map(current).set(id, { ...reusableSession, presentationState: 'mounting', statusMessage: undefined })
        );
        try {
          const prepared = await prepareSessionAttach(reusableSession, 'wake');
          const runningSession = domainSessionToWorkspaceSession(
            reusableSession.machineId,
            prepared.attach.session,
            'running'
          );
          setLocalCommandSessions((current) => new Map(current).set(id, runningSession));
          await sendCommandSessionText(reusableSession, startupText);
        } catch (nextError) {
          const message = nextError instanceof Error ? nextError.message : String(nextError);
          setLocalCommandSessions((current) =>
            new Map(current).set(id, {
              ...reusableSession,
              presentationState: 'startup-failed',
              statusMessage: message,
            })
          );
          throw nextError;
        }
        return;
      }
      const params: Record<string, unknown> = {
        ...(project.path ? { cwd: project.path } : {}),
        ...(action ? { commandId: action.commandId } : {}),
        kind: 'terminal',
        launchSettings: {
          ...(action ? { commandTitle: action.name } : {}),
          ...(startupText ? { startupText: `${startupText}\r` } : {}),
          surface: 'commands',
        },
        lifecycleState: 'running',
        projectId: project.projectId,
        providerState: { lifecycleState: 'exists', provider: 'zmx' },
        runtimeSettings: { sessionPersistenceProvider: 'zmx', titleSource: 'user' },
        surface: 'commands',
        title: action?.name ?? 'Command Terminal',
      };
      const { session } = await rpcForMachine<{ session: GxserverSessionDomainState }>(
        project.machineId,
        '/api/createSession',
        params
      );
      const reference = {
        machineId: project.machineId,
        projectId: session.projectId,
        sessionId: session.sessionId,
      };
      const id = createWorkspaceSessionId(reference);
      setHiddenCommandSessions((current) => {
        const next = new Set(current);
        next.delete(id);
        return next;
      });
      setLocalCommandSessions((current) =>
        new Map(current).set(id, domainSessionToWorkspaceSession(project.machineId, session, 'mounting'))
      );
      setCommandPaneOpenRequest({
        requestId: ++commandPaneRequestId.current,
        sessionId: id,
      });
      try {
        const prepared = await prepareSessionAttach(reference, 'wake', startupText);
        rememberStartupText(reference, prepared);
        setLocalCommandSessions((current) =>
          new Map(current).set(
            id,
            domainSessionToWorkspaceSession(project.machineId, prepared.attach.session, 'running')
          )
        );
      } catch (nextError) {
        const message = nextError instanceof Error ? nextError.message : String(nextError);
        setLocalCommandSessions((current) =>
          new Map(current).set(
            id,
            domainSessionToWorkspaceSession(project.machineId, session, 'startup-failed', message)
          )
        );
        throw nextError;
      }
    },
    [commandSessions, rememberStartupText]
  );

  useEffect(() => {
    const runAction = (event: WindowEventMap['ghostex-web:runTitlebarAction']) => {
      if (
        !activeProject ||
        activeProject.machineId !== event.detail.machineId ||
        activeProject.projectId !== event.detail.projectId
      ) {
        setError('The active project changed before the Action could run.');
        return;
      }
      void createCommandTerminal(activeProject, event.detail.action).catch((nextError: unknown) =>
        setError(nextError instanceof Error ? nextError.message : String(nextError))
      );
    };
    const openCommandPane = (event: WindowEventMap['ghostex-web:openCommandPane']) => {
      const toggle = event.detail?.toggle === true;
      if (toggle && commandSessions.length === 0) {
        if (!activeProject) return;
        void createCommandTerminal(activeProject).catch((nextError: unknown) =>
          setError(nextError instanceof Error ? nextError.message : String(nextError))
        );
        return;
      }
      setCommandPaneOpenRequest({
        requestId: ++commandPaneRequestId.current,
        ...(toggle ? { toggle: true } : {}),
      });
    };
    window.addEventListener('ghostex-web:runTitlebarAction', runAction);
    window.addEventListener('ghostex-web:openCommandPane', openCommandPane);
    return () => {
      window.removeEventListener('ghostex-web:runTitlebarAction', runAction);
      window.removeEventListener('ghostex-web:openCommandPane', openCommandPane);
    };
  }, [activeProject, commandSessions.length, createCommandTerminal]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.repeat || hasActiveSidebarHotkeyRecorder()) return;
      const hotkeyText = ghostexHotkeyTextFromKeyboardEvent(event);
      if (!hotkeyText) return;
      const actionId = getghostexHotkeyActionIdForKey(
        normalizeghostexHotkeySettings(readWebSettings().hotkeys),
        hotkeyText
      );
      if (actionId !== 'openCommandsPanel') return;
      event.preventDefault();
      event.stopPropagation();
      window.dispatchEvent(new CustomEvent('ghostex-web:openCommandPane', { detail: { toggle: true } }));
    };
    window.addEventListener('keydown', handleKeyDown, true);
    return () => window.removeEventListener('keydown', handleKeyDown, true);
  }, []);

  const attachCommandSession = useCallback(
    (session: WorkspaceSession, action: WorkspacePlaceholderAction) => {
      const id = workspaceSessionId(session);
      setLocalCommandSessions((current) =>
        new Map(current).set(id, { ...session, presentationState: 'mounting', statusMessage: undefined })
      );
      void prepareSessionAttach(session, action === 'wake' ? 'wake' : 'attach')
        .then((prepared) => {
          rememberStartupText(session, prepared);
          setLocalCommandSessions((current) =>
            new Map(current).set(
              id,
              domainSessionToWorkspaceSession(session.machineId, prepared.attach.session, 'running')
            )
          );
        })
        .catch((nextError: unknown) => {
          const message = nextError instanceof Error ? nextError.message : String(nextError);
          setLocalCommandSessions((current) =>
            new Map(current).set(id, { ...session, presentationState: 'startup-failed', statusMessage: message })
          );
          setError(message);
        });
    },
    [rememberStartupText]
  );

  const closeCommandSession = useCallback(async (session: WorkspaceSession) => {
    await rpcForMachine(session.machineId, '/api/killSession', {
      projectId: session.projectId,
      reason: 'ghostex-web-command-pane-close',
      sessionId: session.sessionId,
    });
    const id = workspaceSessionId(session);
    setHiddenCommandSessions((current) => new Set(current).add(id));
    setLocalCommandSessions((current) => {
      const next = new Map(current);
      next.delete(id);
      return next;
    });
  }, []);

  return (
    <div className='agents-page'>
      {error && (
        <div className='agents-page__error' role='status'>
          <span>{error}</span>
          <button aria-label='Dismiss error' onClick={() => setError(undefined)} type='button'>
            ×
          </button>
        </div>
      )}
      <div className='agents-page__workspace'>
        <AgentsWorkspace
          authoritativeMachineIds={authoritativeMachineIds}
          availableSessions={availableWorkspaceSessions}
          onFindEvent={handleFind}
          onModelChange={(model) => {
            workspaceModel.current = model;
            publishActiveSessionContext(model);
          }}
          onNewTerminal={(paneId, splitAxis) => void createWorkspaceTerminal(paneId, splitAxis)}
          onPlaceholderAction={handlePlaceholderAction}
          openRequest={openRequest}
          renderChatBody={(session, controls) => (
            <SessionChatHost onSwitchToTerminal={controls.switchToTerminal} session={session} />
          )}
          renderTerminalBody={(session, controls) => {
            const machine = connections.find((state) => state.machine.machineId === session.machineId)?.machine;
            const id = workspaceSessionId(session);
            if (!machine) {
              return <div className='workspace-terminal-unavailable'>Machine connection unavailable.</div>;
            }
            const chatEligible =
              Boolean(session.agentSessionId?.trim()) &&
              resolveSessionChatTranscriptAgent(session.agentId, session.agentIcon) !== null;
            const shortSessionId = shortAgentSessionId(session.agentSessionId);
            return (
              <div className='workspace-terminal-surface'>
                <SessionTerminal
                  aria-label={`Terminal ${session.title}`}
                  authToken={machine.authToken}
                  autoFocus={controls.isActive}
                  baseUrl={machine.baseUrl}
                  customKeyEventHandler={(event) =>
                    !((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'f')
                  }
                  visibility={
                    !controls.isActive ? 'parked' : session.sessionSurfaceMode === 'chat' ? 'chat' : 'visible'
                  }
                  onError={(nextError: TerminalWsClientError) => setError(nextError.message)}
                  onReady={() => void deliverStartupText(session)}
                  projectId={session.projectId as GxserverProjectId}
                  ref={(handle) => {
                    if (handle) terminalHandles.current.set(id, handle);
                    else terminalHandles.current.delete(id);
                  }}
                  sessionId={session.sessionId as GxserverSessionId}
                />
                {chatEligible ? (
                  <>
                    {/*
                      Plan 016: the queue keeps draining while the terminal is
                      on screen, so the count lives here and one click goes back
                      to the chat view where the rows can be edited.
                    */}
                    <SessionChatQueuedPromptsButton
                      count={session.queuedPromptCount ?? 0}
                      failedCount={session.queuedPromptFailedCount ?? 0}
                      onOpenChat={controls.switchToChat}
                    />
                    <SessionTerminalActionBar
                      hostActions={createWebSessionHostActions(session, controls.switchToChat)}
                      {...(shortSessionId ? { sessionId: shortSessionId } : {})}
                      stashedPromptCount={session.stashedPromptCount ?? 0}
                    />
                  </>
                ) : null}
              </div>
            );
          }}
        />
      </div>
      <CommandPane
        activeProject={activeProject}
        machine={activeMachine}
        onAttach={attachCommandSession}
        onClose={closeCommandSession}
        onCreate={createCommandTerminal}
        onError={setError}
        onReady={(session) => void deliverStartupText(session)}
        openRequest={commandPaneOpenRequest}
        sessions={commandSessions}
      />
    </div>
  );
}

function useWindowFocusSession(onFocus: (detail: GhostexWebFocusSessionDetail) => void): void {
  const callback = useRef(onFocus);
  callback.current = onFocus;
  useEffect(() => {
    const listener = (event: WindowEventMap['ghostex-web:focusSession']) => callback.current(event.detail);
    window.addEventListener('ghostex-web:focusSession', listener);
    return () => window.removeEventListener('ghostex-web:focusSession', listener);
  }, []);
}

function findPresentationWorkspaceSession(
  connections: readonly MachineConnectionState[],
  reference: SessionReference
): WorkspaceSession | undefined {
  const session = findPresentationSession(connections, reference);
  return session ? presentationSessionToWorkspaceSession(reference.machineId, session) : undefined;
}

function findPresentationSession(
  connections: readonly MachineConnectionState[],
  reference: SessionReference
): GxserverPresentationSession | undefined {
  return connections
    .find((state) => state.machine.machineId === reference.machineId)
    ?.presentation?.sessions.find(
      (session) => session.projectId === reference.projectId && session.sessionId === reference.sessionId
    );
}

let lastActiveSessionContextKey = '';

function publishActiveSessionContext(model: WorkspaceModel): void {
  const activeTabId = workspaceLeaf(model, model.focusedPane)?.tabGroup.activeTab;
  const session = activeTabId
    ? model.sessions.find((candidate) => workspaceSessionId(candidate) === activeTabId)
    : undefined;
  if (!session) {
    return;
  }
  const key = `${session.machineId}\n${session.projectId}\n${session.sessionId}`;
  if (key === lastActiveSessionContextKey) {
    return;
  }
  lastActiveSessionContextKey = key;
  window.dispatchEvent(
    new CustomEvent('ghostex-web:activeSessionContext', {
      detail: {
        machineId: session.machineId,
        projectId: session.projectId,
        sessionId: session.sessionId,
      },
    })
  );
}

function resolveActiveProject(
  target: ReturnType<typeof getActiveSidebarProject>,
  connections: readonly MachineConnectionState[]
): ActiveProject | undefined {
  if (!target) {
    return undefined;
  }
  const project = connections
    .find((state) => state.machine.machineId === target.machineId)
    ?.presentation?.projects.find((candidate) => candidate.projectId === target.projectId);
  return project
    ? {
        machineId: target.machineId,
        ...(project.path ? { path: project.path } : {}),
        projectId: target.projectId,
        title: project.title,
      }
    : undefined;
}
