import { useEffect, useMemo, useRef, useState, type FocusEvent, type PointerEvent } from 'react';
import type { GxserverProjectId, GxserverSessionId } from '@/packages/shared/gxserver-protocol';
import type { GhostexWebMachine } from '../connections/types';
import { SessionTerminal, type SessionTerminalHandle } from '../terminal';
import {
  workspaceSessionId,
  type WorkspacePlaceholderAction,
  type WorkspaceSession,
} from '../workspace/workspace-model';
import type { ActiveProject } from './types';

export interface CommandPaneOpenRequest {
  requestId: number;
  sessionId?: string;
  toggle?: boolean;
}

interface CommandPaneProps {
  activeProject?: ActiveProject;
  machine?: GhostexWebMachine;
  sessions: WorkspaceSession[];
  onAttach(session: WorkspaceSession, action: WorkspacePlaceholderAction): void;
  onClose(session: WorkspaceSession): Promise<void>;
  onCreate(project: ActiveProject): Promise<void>;
  onError(message: string): void;
  onReady(session: WorkspaceSession): void;
  openRequest?: CommandPaneOpenRequest;
}

const HEIGHT_STORAGE_KEY = 'ghostexWeb.commandPaneHeight.v1';
const DEFAULT_HEIGHT = 320;
const MIN_HEIGHT = 120;

function clampHeight(height: number): number {
  return Math.min(window.innerHeight * 0.7, Math.max(MIN_HEIGHT, height));
}

function readHeight(): number {
  const stored = Number(window.localStorage.getItem(HEIGHT_STORAGE_KEY));
  return clampHeight(Number.isFinite(stored) && stored > 0 ? stored : DEFAULT_HEIGHT);
}

export function CommandPane({
  activeProject,
  machine,
  sessions,
  onAttach,
  onClose,
  onCreate,
  onError,
  onReady,
  openRequest,
}: CommandPaneProps) {
  const projectKey = activeProject ? `${activeProject.machineId}/${activeProject.projectId}` : 'none';
  const [activeByProject, setActiveByProject] = useState<Record<string, string>>({});
  const [expandedProjects, setExpandedProjects] = useState<Set<string>>(() => new Set());
  const [creating, setCreating] = useState(false);
  const [height, setHeight] = useState(readHeight);
  const handledOpenRequest = useRef(0);
  const paneActiveRef = useRef(false);
  const expandedProjectsRef = useRef(expandedProjects);
  const paneRef = useRef<HTMLElement>(null);
  const terminalRef = useRef<SessionTerminalHandle>(null);
  const pendingFocusRef = useRef(false);
  expandedProjectsRef.current = expandedProjects;
  const activeId = activeByProject[projectKey];
  const activeSession = useMemo(
    () => sessions.find((session) => workspaceSessionId(session) === activeId) ?? sessions[0],
    [activeId, sessions]
  );
  const expanded = expandedProjects.has(projectKey) && Boolean(activeSession);

  const collapse = () => {
    paneActiveRef.current = false;
    setExpandedProjects((current) => {
      const next = new Set(current);
      next.delete(projectKey);
      return next;
    });
  };

  const markPaneActive = () => {
    paneActiveRef.current = true;
  };

  const markPaneInactiveIfLeaving = (event: FocusEvent<HTMLElement>) => {
    const next = event.relatedTarget;
    if (next instanceof Node && event.currentTarget.contains(next)) return;
    paneActiveRef.current = false;
  };

  useEffect(() => {
    if (!activeSession || activeId === workspaceSessionId(activeSession)) return;
    setActiveByProject((current) => ({
      ...current,
      [projectKey]: workspaceSessionId(activeSession),
    }));
  }, [activeId, activeSession, projectKey]);

  useEffect(() => {
    if (!openRequest || openRequest.requestId === handledOpenRequest.current || sessions.length === 0) return;
    handledOpenRequest.current = openRequest.requestId;
    const currentlyExpanded = expandedProjectsRef.current.has(projectKey);
    if (openRequest.toggle && currentlyExpanded && paneActiveRef.current) {
      paneActiveRef.current = false;
      setExpandedProjects((current) => {
        const next = new Set(current);
        next.delete(projectKey);
        return next;
      });
      return;
    }
    const requestedSession = openRequest.sessionId
      ? sessions.find((session) => workspaceSessionId(session) === openRequest.sessionId)
      : undefined;
    if (requestedSession) {
      setActiveByProject((current) => ({
        ...current,
        [projectKey]: workspaceSessionId(requestedSession),
      }));
    }
    paneActiveRef.current = true;
    pendingFocusRef.current = true;
    setExpandedProjects((current) => new Set(current).add(projectKey));
  }, [openRequest, projectKey, sessions]);

  useEffect(() => {
    if (!expanded || !pendingFocusRef.current) return;
    pendingFocusRef.current = false;
    if (terminalRef.current) {
      terminalRef.current.focus();
      return;
    }
    paneRef.current?.focus();
  }, [activeSession, expanded, openRequest]);

  useEffect(() => {
    window.localStorage.setItem(HEIGHT_STORAGE_KEY, String(Math.round(height)));
  }, [height]);

  useEffect(() => {
    const clampToViewport = () => setHeight((current) => clampHeight(current));
    window.addEventListener('resize', clampToViewport);
    return () => window.removeEventListener('resize', clampToViewport);
  }, []);

  const selectSession = (session: WorkspaceSession) => {
    setActiveByProject((current) => ({
      ...current,
      [projectKey]: workspaceSessionId(session),
    }));
  };

  const create = async () => {
    if (!activeProject || creating) return;
    setCreating(true);
    try {
      await onCreate(activeProject);
    } catch (error) {
      onError(error instanceof Error ? error.message : String(error));
    } finally {
      setCreating(false);
    }
  };

  const beginResize = (event: PointerEvent<HTMLDivElement>) => {
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
  };
  const resize = (event: PointerEvent<HTMLDivElement>) => {
    if (event.buttons === 1) setHeight(clampHeight(window.innerHeight - event.clientY));
  };
  const endResize = (event: PointerEvent<HTMLDivElement>) => {
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
  };

  if (!expanded || !activeSession) return null;

  return (
    <section
      className='command-pane'
      onBlurCapture={markPaneInactiveIfLeaving}
      onFocusCapture={markPaneActive}
      onPointerDown={markPaneActive}
      ref={paneRef}
      style={{ height }}
      tabIndex={-1}
    >
      <div
        aria-label='Resize command pane'
        className='command-pane__resize'
        onDoubleClick={() => setHeight(clampHeight(DEFAULT_HEIGHT))}
        onPointerCancel={endResize}
        onPointerDown={beginResize}
        onPointerMove={resize}
        onPointerUp={endResize}
        role='separator'
      />
      <div className='command-pane__header'>
        <strong className='command-pane__label'>Command Terminal</strong>
        <div className='command-pane__tabs' role='tablist'>
          {sessions.map((session) => {
            const id = workspaceSessionId(session);
            return (
              <button
                aria-selected={workspaceSessionId(activeSession) === id}
                className='command-pane__tab'
                key={id}
                onClick={() => selectSession(session)}
                role='tab'
                type='button'
              >
                <span>{session.title}</span>
                <span
                  aria-label={`Close ${session.title}`}
                  className='command-pane__close'
                  onClick={(event) => {
                    event.stopPropagation();
                    void onClose(session).catch((error: unknown) =>
                      onError(error instanceof Error ? error.message : String(error))
                    );
                  }}
                  role='button'
                >
                  ×
                </span>
              </button>
            );
          })}
        </div>
        <button
          aria-label='New command terminal'
          className='command-pane__new'
          disabled={!activeProject || creating}
          onClick={() => void create()}
          type='button'
        >
          {creating ? '…' : '+'}
        </button>
        <button
          aria-label='Hide command pane'
          className='command-pane__hide'
          onClick={collapse}
          type='button'
        >
          ⌄
        </button>
      </div>
      <div className='command-pane__body'>
        {activeSession.presentationState === 'running' && machine && (
          <SessionTerminal
            aria-label={`Command terminal ${activeSession.title}`}
            authToken={machine.authToken}
            autoFocus
            baseUrl={machine.baseUrl}
            onError={(error) => onError(error.message)}
            onReady={() => onReady(activeSession)}
            projectId={activeSession.projectId as GxserverProjectId}
            ref={terminalRef}
            sessionId={activeSession.sessionId as GxserverSessionId}
          />
        )}
        {activeSession.presentationState !== 'running' && (
          <div className='command-pane__placeholder'>
            <span>{activeSession.statusMessage ?? commandStateLabel(activeSession)}</span>
            {activeSession.presentationState !== 'mounting' && (
              <button
                onClick={() =>
                  onAttach(activeSession, activeSession.presentationState === 'sleeping' ? 'wake' : 'materialize')
                }
                type='button'
              >
                {activeSession.presentationState === 'sleeping' ? 'Wake' : 'Retry'}
              </button>
            )}
          </div>
        )}
      </div>
    </section>
  );
}

function commandStateLabel(session: WorkspaceSession): string {
  switch (session.presentationState) {
    case 'sleeping':
      return 'This command terminal is sleeping.';
    case 'mounting':
      return 'Starting command terminal…';
    case 'restored-unmounted':
      return 'This command terminal needs to be materialized.';
    case 'startup-failed':
      return 'Command terminal startup failed.';
    case 'running':
      return '';
  }
}
