import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type DragEvent,
  type KeyboardEvent,
  type PointerEvent,
  type ReactNode,
} from 'react';
import { AppTooltip } from '@/packages/core-ui/app-tooltip';
import {
  addWorkspaceSession,
  closeWorkspaceTab,
  focusWorkspacePane,
  mergeAllWorkspaceTabs,
  moveWorkspaceTab,
  reconcileWorkspaceSessions,
  reconcileOpenWorkspaceSessions,
  rotateWorkspacePanes,
  selectWorkspaceTab,
  setWorkspaceSessionSurfaceMode,
  setWorkspaceSplitRatio,
  splitWorkspacePane,
  toggleWorkspaceFocusMode,
  workspaceLeaf,
  workspaceLeaves,
  workspacePaneForSession,
  workspaceSession,
  workspaceSessionId,
  updateWorkspaceSession,
  type WorkspaceLeaf,
  type WorkspaceModel,
  type WorkspaceNode,
  type WorkspacePlaceholderAction,
  type WorkspaceSession,
  type WorkspaceSplit,
  type WorkspaceSplitAxis,
} from './workspace-model';
import { loadPersistedWorkspaceLayout, loadWorkspaceLayout, saveWorkspaceLayout } from './workspace-storage';
import './workspace.css';

const TAB_DRAG_TYPE = 'application/x-ghostex-workspace-tab';
const EMPTY_SESSIONS: WorkspaceSession[] = [];
const EMPTY_MACHINE_IDS: string[] = [];

export interface WorkspaceFindEvent {
  type: 'open' | 'query' | 'next' | 'previous' | 'close';
  paneId: string;
  sessionId: string | null;
  query?: string;
}

export interface WorkspaceOpenRequest {
  placementTargetSessionId?: string;
  requestId: number;
  session: WorkspaceSession;
  splitAxis?: WorkspaceSplitAxis;
  targetPaneId?: string;
}

/** Pane controls handed to the chat body so in-chat chrome can switch back. */
export interface WorkspaceChatBodyControls {
  switchToTerminal(): void;
}

/** Pane controls handed to the terminal body for its floating surface chrome. */
export interface WorkspaceTerminalBodyControls {
  isActive: boolean;
  switchToChat(): void;
}

export interface AgentsWorkspaceProps {
  sessions?: WorkspaceSession[];
  availableSessions?: WorkspaceSession[];
  authoritativeMachineIds?: readonly string[];
  openRequest?: WorkspaceOpenRequest;
  primaryMachineId?: string;
  debugSeed?: boolean;
  renderTerminalBody?(session: WorkspaceSession, controls: WorkspaceTerminalBodyControls): ReactNode;
  renderChatBody?(session: WorkspaceSession, controls: WorkspaceChatBodyControls): ReactNode;
  onNewTerminal?(paneId: string, splitAxis?: WorkspaceSplitAxis): void;
  onPlaceholderAction?(session: WorkspaceSession, action: WorkspacePlaceholderAction): void;
  onFindEvent?(event: WorkspaceFindEvent): void;
  onModelChange?(model: WorkspaceModel): void;
}

interface DraggedTab {
  paneId: string;
  sessionId: string;
}

interface TabMenuState extends DraggedTab {
  x: number;
  y: number;
}

const STATE_BADGES: Partial<Record<WorkspaceSession['presentationState'], string>> = {
  mounting: 'MNT',
  'startup-failed': 'ERR',
  'restored-unmounted': 'RST',
};

const PLACEHOLDERS: Record<
  Exclude<WorkspaceSession['presentationState'], 'running'>,
  { label: string; title: string; message: string; action: string; actionId?: WorkspacePlaceholderAction }
> = {
  sleeping: {
    label: 'Sleeping',
    title: 'Sleeping terminal',
    message: 'This terminal is parked. Wake it when you are ready to continue the session.',
    action: 'Wake',
    actionId: 'wake',
  },
  mounting: {
    label: 'Mounting',
    title: 'Mounting terminal',
    message: 'Startup or materialization is pending until the terminal runtime is ready.',
    action: 'Pending startup',
  },
  'startup-failed': {
    label: 'Startup failed',
    title: 'Terminal startup failed',
    message: 'The terminal runtime did not start. Retry keeps this tab in place.',
    action: 'Retry',
    actionId: 'retry',
  },
  'restored-unmounted': {
    label: 'Restored',
    title: 'Restored terminal',
    message: 'This restored tab is present, but its terminal surface has not been materialized.',
    action: 'Materialize',
    actionId: 'materialize',
  },
};

function parseDraggedTab(event: DragEvent): DraggedTab | null {
  try {
    const parsed = JSON.parse(event.dataTransfer.getData(TAB_DRAG_TYPE)) as Partial<DraggedTab>;
    return typeof parsed.paneId === 'string' && typeof parsed.sessionId === 'string'
      ? { paneId: parsed.paneId, sessionId: parsed.sessionId }
      : null;
  } catch {
    return null;
  }
}

function createPendingSession(model: WorkspaceModel): WorkspaceSession {
  const id = `web-pending-${model.nextSessionId}`;
  return {
    machineId: 'local',
    projectId: 'pending',
    sessionId: id,
    title: `Terminal ${model.nextSessionId}`,
    presentationState: 'mounting',
    activity: 'idle',
  };
}

function WorkspaceIcon({ name }: { name: 'chat' | 'close' | 'find' | 'menu' | 'plus' | 'terminal' }) {
  const paths = {
    chat: 'M4 4.5h12a.5.5 0 0 1 .5.5v8a.5.5 0 0 1-.5.5H9.5L6 16v-2.5H4a.5.5 0 0 1-.5-.5V5a.5.5 0 0 1 .5-.5Z',
    close: 'M5 5l10 10M15 5 5 15',
    find: 'M8.5 14a5.5 5.5 0 1 1 0-11 5.5 5.5 0 0 1 0 11Zm4-1 4 4',
    menu: 'M4 8h.01M10 8h.01M16 8h.01',
    plus: 'M10 3v14M3 10h14',
    terminal: 'M4 6l4 4-4 4M10.5 14.5H16',
  } as const;
  return (
    <svg aria-hidden='true' viewBox='0 0 20 20'>
      <path d={paths[name]} />
    </svg>
  );
}

function PlaceholderCard({
  session,
  onAction,
}: {
  session: WorkspaceSession;
  onAction(action: WorkspacePlaceholderAction): void;
}) {
  if (session.presentationState === 'running') {
    return null;
  }
  const placeholder = PLACEHOLDERS[session.presentationState];
  return (
    <div className={`workspace-placeholder workspace-placeholder--${session.presentationState}`}>
      <span className='workspace-placeholder__label'>{placeholder.label}</span>
      <h2>{placeholder.title}</h2>
      <p>{session.statusMessage ?? placeholder.message}</p>
      <button
        disabled={!placeholder.actionId}
        onClick={() => placeholder.actionId && onAction(placeholder.actionId)}
        type='button'
      >
        {placeholder.action}
      </button>
    </div>
  );
}

function FindBar({
  leaf,
  onClose,
  onEvent,
}: {
  leaf: WorkspaceLeaf;
  onClose(): void;
  onEvent(event: WorkspaceFindEvent): void;
}) {
  const [query, setQuery] = useState('');
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const emit = (type: WorkspaceFindEvent['type'], nextQuery = query) => {
    onEvent({
      type,
      paneId: leaf.paneId,
      sessionId: leaf.tabGroup.activeTab,
      query: nextQuery,
    });
  };

  return (
    <div className='workspace-findbar'>
      <WorkspaceIcon name='find' />
      <input
        aria-label='Find in terminal'
        onChange={(event) => {
          setQuery(event.target.value);
          emit('query', event.target.value);
        }}
        onKeyDown={(event) => {
          if (event.key === 'Enter') {
            emit(event.shiftKey ? 'previous' : 'next');
          } else if (event.key === 'Escape') {
            emit('close');
            onClose();
          }
        }}
        placeholder='Find'
        ref={inputRef}
        type='search'
        value={query}
      />
      <button aria-label='Previous match' onClick={() => emit('previous')} type='button'>
        ↑
      </button>
      <button aria-label='Next match' onClick={() => emit('next')} type='button'>
        ↓
      </button>
      <button
        aria-label='Close find'
        onClick={() => {
          emit('close');
          onClose();
        }}
        type='button'
      >
        <WorkspaceIcon name='close' />
      </button>
    </div>
  );
}

function Pane({
  leaf,
  model,
  findOpen,
  insertionTarget,
  renderTerminalBody,
  renderChatBody,
  onChange,
  onFindEvent,
  onFindOpenChange,
  onMenu,
  onNewTerminal,
  onPlaceholderAction,
  onTabMenu,
}: {
  leaf: WorkspaceLeaf;
  model: WorkspaceModel;
  findOpen: boolean;
  insertionTarget: string | null;
  renderTerminalBody?: AgentsWorkspaceProps['renderTerminalBody'];
  renderChatBody?: AgentsWorkspaceProps['renderChatBody'];
  onChange(model: WorkspaceModel): void;
  onFindEvent(event: WorkspaceFindEvent): void;
  onFindOpenChange(open: boolean): void;
  onMenu(action: 'split-horizontal' | 'split-vertical' | 'rotate' | 'merge' | 'focus'): void;
  onNewTerminal(): void;
  onPlaceholderAction(session: WorkspaceSession, action: WorkspacePlaceholderAction): void;
  onTabMenu(event: React.MouseEvent, tab: DraggedTab): void;
}) {
  const active = leaf.tabGroup.activeTab ? workspaceSession(model, leaf.tabGroup.activeTab) : undefined;
  const runningSessions = leaf.tabGroup.tabs.flatMap((tab) => {
    const session = workspaceSession(model, tab.sessionId);
    return session?.presentationState === 'running' ? [session] : [];
  });
  const focused = model.focusedPane === leaf.paneId;
  // `find` is a legacy persisted pane mode. Search by Prompt is now an
  // app-level modal, so old layouts return directly to their terminal.
  const surfaceMode = active?.sessionSurfaceMode === 'chat' ? 'chat' : 'terminal';
  const setSurfaceMode = (mode: 'terminal' | 'chat') => {
    if (leaf.tabGroup.activeTab) {
      onChange(setWorkspaceSessionSurfaceMode(model, leaf.tabGroup.activeTab, mode));
    }
  };

  const dropTab = (event: DragEvent, insertionIndex: number) => {
    event.preventDefault();
    const dragged = parseDraggedTab(event);
    if (dragged) {
      onChange(moveWorkspaceTab(model, dragged.paneId, leaf.paneId, dragged.sessionId, insertionIndex));
    }
  };

  return (
    <section
      aria-label={`Terminal pane ${leaf.paneId}`}
      className={`workspace-pane${focused ? ' workspace-pane--focused' : ''}${
        findOpen ? ' workspace-pane--find-open' : ''
      }`}
      data-pane-id={leaf.paneId}
      onMouseDown={() => onChange(focusWorkspacePane(model, leaf.paneId))}
    >
      <div className='workspace-tabbar'>
        <div
          className='workspace-tabbar__tabs'
          onDragOver={(event) => event.preventDefault()}
          onDrop={(event) => dropTab(event, leaf.tabGroup.tabs.length)}
        >
          {leaf.tabGroup.tabs.map((tab, index) => {
            const session = workspaceSession(model, tab.sessionId);
            if (!session) return null;
            const selected = leaf.tabGroup.activeTab === tab.sessionId;
            const insertionKey = `${leaf.paneId}:${index}`;
            return (
              <AppTooltip content={session.title} key={tab.sessionId}>
                <div
                  aria-selected={selected}
                  className={`workspace-tab${selected ? ' workspace-tab--active' : ''}${
                    insertionTarget === insertionKey ? ' workspace-tab--drop-before' : ''
                  }`}
                  draggable
                  onContextMenu={(event) => onTabMenu(event, { paneId: leaf.paneId, sessionId: tab.sessionId })}
                  onDoubleClick={() =>
                    onChange(
                      toggleWorkspaceFocusMode(selectWorkspaceTab(model, leaf.paneId, tab.sessionId), leaf.paneId)
                    )
                  }
                  onDragOver={(event) => {
                    event.preventDefault();
                    event.stopPropagation();
                    event.dataTransfer.dropEffect = 'move';
                  }}
                  onDragStart={(event) => {
                    event.dataTransfer.effectAllowed = 'move';
                    event.dataTransfer.setData(
                      TAB_DRAG_TYPE,
                      JSON.stringify({ paneId: leaf.paneId, sessionId: tab.sessionId })
                    );
                  }}
                  onDrop={(event) => {
                    event.stopPropagation();
                    dropTab(event, index);
                  }}
                  onMouseDown={(event) => {
                    event.stopPropagation();
                    onChange(selectWorkspaceTab(model, leaf.paneId, tab.sessionId));
                  }}
                  role='tab'
                >
                  <span className={`workspace-tab__agent workspace-tab__agent--${session.agentIcon ?? 'terminal'}`}>
                    {(session.agentIcon ?? '>_').slice(0, 2).toUpperCase()}
                  </span>
                  <span className='workspace-tab__title'>{session.title}</span>
                  {session.activity !== 'idle' && (
                    <span
                      aria-label={session.activity}
                      className={`workspace-tab__status workspace-tab__status--${session.activity}`}
                    />
                  )}
                  {STATE_BADGES[session.presentationState] && (
                    <span className={`workspace-tab__badge workspace-tab__badge--${session.presentationState}`}>
                      {STATE_BADGES[session.presentationState]}
                    </span>
                  )}
                  <button
                    aria-label={`Close ${session.title}`}
                    className='workspace-tab__close'
                    onClick={(event) => {
                      event.stopPropagation();
                      onChange(
                        closeWorkspaceTab(
                          selectWorkspaceTab(model, leaf.paneId, tab.sessionId),
                          leaf.paneId,
                          tab.sessionId
                        )
                      );
                    }}
                    onMouseDown={(event) => event.stopPropagation()}
                    type='button'
                  >
                    <WorkspaceIcon name='close' />
                  </button>
                </div>
              </AppTooltip>
            );
          })}
          <div
            className={`workspace-tabbar__drop-end${
              insertionTarget === `${leaf.paneId}:${leaf.tabGroup.tabs.length}`
                ? ' workspace-tabbar__drop-end--active'
                : ''
            }`}
          />
        </div>
        <div className='workspace-tabbar__actions'>
          {/*
            The terminal↔chat switch lives on the surface itself: the chat
            composer's footer row one way (SessionChatComposerActions), the
            terminal's bottom bar the other (SessionTerminalActionBar) — no
            tab-bar toggle.
          */}
          <button aria-label='Find in terminal' onClick={() => onFindOpenChange(true)} type='button'>
            <WorkspaceIcon name='find' />
          </button>
          <button aria-label='New terminal' onClick={onNewTerminal} type='button'>
            <WorkspaceIcon name='plus' />
          </button>
          <details className='workspace-pane-menu'>
            <summary aria-label='Pane actions menu'>
              <WorkspaceIcon name='menu' />
            </summary>
            <div className='workspace-menu' role='menu'>
              <button onClick={() => onMenu('split-horizontal')} role='menuitem' type='button'>
                Split Sideways
              </button>
              <button onClick={() => onMenu('split-vertical')} role='menuitem' type='button'>
                Split Downwards
              </button>
              <button onClick={() => onMenu('rotate')} role='menuitem' type='button'>
                Rotate Panes Clockwise
              </button>
              <button onClick={() => onMenu('merge')} role='menuitem' type='button'>
                Merge All Tabs
              </button>
              <button onClick={() => onMenu('focus')} role='menuitem' type='button'>
                {model.focusModePane ? 'Exit Focus' : 'Focus'}
              </button>
            </div>
          </details>
        </div>
      </div>
      {findOpen && <FindBar leaf={leaf} onClose={() => onFindOpenChange(false)} onEvent={onFindEvent} />}
      <div className='workspace-pane__body'>
        {!active && (
          <div className='workspace-empty-pane'>
            <span>No terminals</span>
            <button onClick={onNewTerminal} type='button'>
              New Terminal
            </button>
          </div>
        )}
        {runningSessions.map((session) => {
          const sessionId = workspaceSessionId(session);
          const isActive = sessionId === leaf.tabGroup.activeTab;
          const sessionSurfaceMode = session.sessionSurfaceMode ?? 'terminal';
          return (
            <div
              aria-hidden={!isActive || sessionSurfaceMode === 'chat' || undefined}
              className={`workspace-surface-layer workspace-surface-layer--terminal${
                !isActive ? ' workspace-surface-layer--parked' : ''
              }${isActive && sessionSurfaceMode === 'chat' ? ' workspace-surface-layer--hidden' : ''}`}
              key={sessionId}
            >
              {renderTerminalBody?.(session, {
                isActive,
                switchToChat: () => onChange(setWorkspaceSessionSurfaceMode(model, sessionId, 'chat')),
              }) ?? (
                <div className='workspace-terminal-slot'>
                  <span>{session.title}</span>
                  <small>Terminal body slot</small>
                </div>
              )}
            </div>
          );
        })}
        {active?.presentationState === 'running' && (
          <>
            {surfaceMode === 'chat' && (
              <div className='workspace-surface-layer workspace-surface-layer--chat'>
                {renderChatBody?.(active, {
                  switchToTerminal: () => setSurfaceMode('terminal'),
                }) ?? (
                  <div className='workspace-terminal-slot'>
                    <span>{active.title}</span>
                    <small>Chat body slot</small>
                  </div>
                )}
              </div>
            )}
          </>
        )}
        {active && active.presentationState !== 'running' && (
          <PlaceholderCard onAction={(action) => onPlaceholderAction(active, action)} session={active} />
        )}
      </div>
    </section>
  );
}

function SplitNode({
  node,
  renderLeaf,
  onRatioChange,
}: {
  node: WorkspaceNode;
  renderLeaf(leaf: WorkspaceLeaf): ReactNode;
  onRatioChange(splitId: string, ratio: number): void;
}) {
  if (node.type === 'leaf') {
    return renderLeaf(node);
  }
  const split = node as WorkspaceSplit;
  const horizontal = split.axis === 'horizontal';
  const beginResize = (event: PointerEvent<HTMLDivElement>) => {
    event.currentTarget.setPointerCapture(event.pointerId);
  };
  const resize = (event: PointerEvent<HTMLDivElement>) => {
    if (event.buttons !== 1) return;
    const parent = event.currentTarget.parentElement;
    if (!parent) return;
    const bounds = parent.getBoundingClientRect();
    const ratio = horizontal
      ? (event.clientX - bounds.left) / bounds.width
      : (event.clientY - bounds.top) / bounds.height;
    onRatioChange(split.splitId, ratio);
  };
  const endResize = (event: PointerEvent<HTMLDivElement>) => {
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
  };
  const style = horizontal
    ? { gridTemplateColumns: `${split.ratio}fr 5px ${1 - split.ratio}fr` }
    : { gridTemplateRows: `${split.ratio}fr 5px ${1 - split.ratio}fr` };

  return (
    <div className={`workspace-split workspace-split--${split.axis}`} style={style}>
      <div className='workspace-split__branch'>
        <SplitNode node={split.first} onRatioChange={onRatioChange} renderLeaf={renderLeaf} />
      </div>
      <div
        aria-label={`Resize ${split.axis} panes`}
        className='workspace-split__divider'
        onDoubleClick={() => onRatioChange(split.splitId, split.defaultRatio)}
        onPointerCancel={endResize}
        onPointerDown={beginResize}
        onPointerMove={resize}
        onPointerUp={endResize}
        role='separator'
      />
      <div className='workspace-split__branch'>
        <SplitNode node={split.second} onRatioChange={onRatioChange} renderLeaf={renderLeaf} />
      </div>
    </div>
  );
}

export function AgentsWorkspace({
  sessions = EMPTY_SESSIONS,
  availableSessions,
  authoritativeMachineIds = EMPTY_MACHINE_IDS,
  openRequest,
  primaryMachineId = 'local',
  debugSeed = false,
  renderTerminalBody,
  renderChatBody,
  onNewTerminal,
  onPlaceholderAction,
  onFindEvent,
  onModelChange,
}: AgentsWorkspaceProps) {
  const integrated = availableSessions !== undefined;
  const [model, setModel] = useState(() =>
    integrated
      ? (loadPersistedWorkspaceLayout(primaryMachineId) ?? loadWorkspaceLayout(primaryMachineId, []))
      : loadWorkspaceLayout(primaryMachineId, sessions)
  );
  const [findPanes, setFindPanes] = useState<Set<string>>(() => new Set());
  const [tabMenu, setTabMenu] = useState<TabMenuState | null>(null);

  const authoritativeMachines = useMemo(() => new Set(authoritativeMachineIds), [authoritativeMachineIds]);

  useEffect(() => {
    setModel((current) =>
      integrated
        ? reconcileOpenWorkspaceSessions(current, availableSessions, authoritativeMachines)
        : reconcileWorkspaceSessions(current, sessions)
    );
  }, [authoritativeMachines, availableSessions, integrated, sessions]);

  useEffect(() => {
    if (!openRequest) {
      return;
    }
    setModel((current) => {
      const sessionId = workspaceSessionId(openRequest.session);
      const existingPane = workspacePaneForSession(current, sessionId);
      if (existingPane) {
        return selectWorkspaceTab(updateWorkspaceSession(current, openRequest.session), existingPane.paneId, sessionId);
      }
      const placementPane = openRequest.placementTargetSessionId
        ? workspacePaneForSession(current, openRequest.placementTargetSessionId)?.paneId
        : undefined;
      const paneId =
        placementPane ??
        (openRequest.targetPaneId && workspaceLeaf(current, openRequest.targetPaneId)
          ? openRequest.targetPaneId
          : current.focusedPane);
      return openRequest.splitAxis
        ? splitWorkspacePane(current, paneId, openRequest.splitAxis, openRequest.session)
        : addWorkspaceSession(current, paneId, openRequest.session);
    });
  }, [openRequest]);

  useEffect(() => {
    saveWorkspaceLayout(primaryMachineId, model);
    onModelChange?.(model);
  }, [model, onModelChange, primaryMachineId]);

  useEffect(() => {
    if (!tabMenu) return;
    const close = () => setTabMenu(null);
    window.addEventListener('pointerdown', close);
    return () => window.removeEventListener('pointerdown', close);
  }, [tabMenu]);

  const update = (next: WorkspaceModel) => {
    if (next !== model) setModel(next);
  };

  const createTerminal = (paneId: string, splitAxis?: WorkspaceSplitAxis) => {
    if (onNewTerminal) {
      onNewTerminal(paneId, splitAxis);
      return;
    }
    const pending = createPendingSession(model);
    update(
      splitAxis ? splitWorkspacePane(model, paneId, splitAxis, pending) : addWorkspaceSession(model, paneId, pending)
    );
    window.dispatchEvent(
      new CustomEvent('ghostex-web:newTerminal', {
        detail: { paneId, splitAxis: splitAxis ?? null, provisionalSessionId: pending.sessionId },
      })
    );
  };

  const handleMenu = (paneId: string, action: 'split-horizontal' | 'split-vertical' | 'rotate' | 'merge' | 'focus') => {
    if (action === 'split-horizontal') createTerminal(paneId, 'horizontal');
    else if (action === 'split-vertical') createTerminal(paneId, 'vertical');
    else if (action === 'rotate') update(rotateWorkspacePanes(model));
    else if (action === 'merge') update(mergeAllWorkspaceTabs(model, paneId));
    else update(toggleWorkspaceFocusMode(focusWorkspacePane(model, paneId), paneId));
  };

  const emitFind = (event: WorkspaceFindEvent) => {
    onFindEvent?.(event);
    window.dispatchEvent(new CustomEvent('ghostex-web:findTerminal', { detail: event }));
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'f') {
      event.preventDefault();
      const paneId = model.focusedPane;
      setFindPanes((current) => new Set(current).add(paneId));
      const leaf = workspaceLeaf(model, paneId);
      emitFind({ type: 'open', paneId, sessionId: leaf?.tabGroup.activeTab ?? null });
    } else if (event.key === 'Escape' && model.focusModePane) {
      update(toggleWorkspaceFocusMode(model));
    }
  };

  const visibleRoot = model.focusModePane ? (workspaceLeaf(model, model.focusModePane) ?? model.root) : model.root;

  return (
    <div
      className={`agents-workspace${debugSeed ? ' agents-workspace--debug' : ''}`}
      onKeyDown={handleKeyDown}
      tabIndex={-1}
    >
      {debugSeed && (
        <div className='workspace-debug-badge'>
          Debug workspace · {workspaceLeaves(model).length} pane
          {workspaceLeaves(model).length === 1 ? '' : 's'}
        </div>
      )}
      {model.focusModePane && (
        <button className='workspace-exit-focus' onClick={() => update(toggleWorkspaceFocusMode(model))} type='button'>
          Exit Focus
        </button>
      )}
      <SplitNode
        node={visibleRoot}
        onRatioChange={(splitId, ratio) => update(setWorkspaceSplitRatio(model, splitId, ratio))}
        renderLeaf={(leaf) => (
          <Pane
            findOpen={findPanes.has(leaf.paneId)}
            insertionTarget={null}
            key={leaf.paneId}
            leaf={leaf}
            model={model}
            onChange={update}
            onFindEvent={emitFind}
            onFindOpenChange={(open) => {
              setFindPanes((current) => {
                const next = new Set(current);
                if (open) next.add(leaf.paneId);
                else next.delete(leaf.paneId);
                return next;
              });
              emitFind({
                type: open ? 'open' : 'close',
                paneId: leaf.paneId,
                sessionId: leaf.tabGroup.activeTab,
              });
            }}
            onMenu={(action) => handleMenu(leaf.paneId, action)}
            onNewTerminal={() => createTerminal(leaf.paneId)}
            onPlaceholderAction={(session, action) => {
              onPlaceholderAction?.(session, action);
              window.dispatchEvent(
                new CustomEvent('ghostex-web:workspacePlaceholderAction', {
                  detail: { session, action },
                })
              );
            }}
            onTabMenu={(event, tab) => {
              event.preventDefault();
              event.stopPropagation();
              setTabMenu({ ...tab, x: event.clientX, y: event.clientY });
            }}
            renderChatBody={renderChatBody}
            renderTerminalBody={renderTerminalBody}
          />
        )}
      />
      {tabMenu && (
        <div
          className='workspace-context-menu workspace-menu'
          onPointerDown={(event) => event.stopPropagation()}
          role='menu'
          style={{ left: tabMenu.x, top: tabMenu.y }}
        >
          <button
            onClick={() => {
              update(
                toggleWorkspaceFocusMode(selectWorkspaceTab(model, tabMenu.paneId, tabMenu.sessionId), tabMenu.paneId)
              );
              setTabMenu(null);
            }}
            role='menuitem'
            type='button'
          >
            Focus
          </button>
          <button
            onClick={() => {
              update(closeWorkspaceTab(model, tabMenu.paneId, tabMenu.sessionId));
              setTabMenu(null);
            }}
            role='menuitem'
            type='button'
          >
            Close
          </button>
        </div>
      )}
    </div>
  );
}
