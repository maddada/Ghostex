import type { SessionSurfaceMode } from '@/packages/shared/session-chat';

export type WorkspaceSplitAxis = 'horizontal' | 'vertical';

export type WorkspacePresentationState = 'running' | 'sleeping' | 'mounting' | 'startup-failed' | 'restored-unmounted';

export type WorkspaceActivity = 'idle' | 'working' | 'attention';

export interface WorkspaceSession {
  machineId: string;
  projectId: string;
  sessionId: string;
  workspaceId?: string;
  title: string;
  commandId?: string;
  agentIcon?: string;
  /** gxserver agent id ("claude", "codex", …) used for chat eligibility. */
  agentId?: string;
  /** Stable provider session id required to resolve the transcript. */
  agentSessionId?: string;
  presentationState: WorkspacePresentationState;
  activity: WorkspaceActivity;
  statusMessage?: string;
  /**
   * Ghostex-owned queued prompts waiting on this session (plan 016). Comes off
   * the presentation projection, which gxserver republishes on every queue
   * mutation and every scheduler delivery, so the terminal view's "Queued: N"
   * button stays live without polling.
   */
  queuedPromptCount?: number;
  /**
   * How many of those rows failed to deliver and are held for the user. Any
   * non-zero value means the queue has stopped draining until they retry or
   * delete the row, so the terminal-view button says so in red.
   */
  queuedPromptFailedCount?: number;
  /** Terminal↔chat body toggle; defaults to "terminal" when absent. */
  sessionSurfaceMode?: SessionSurfaceMode;
}

export interface WorkspaceTab {
  sessionId: string;
}

export interface WorkspaceTabGroup {
  tabs: WorkspaceTab[];
  activeTab: string | null;
}

export interface WorkspaceLeaf {
  type: 'leaf';
  paneId: string;
  tabGroup: WorkspaceTabGroup;
}

export interface WorkspaceSplit {
  type: 'split';
  splitId: string;
  axis: WorkspaceSplitAxis;
  ratio: number;
  defaultRatio: number;
  first: WorkspaceNode;
  second: WorkspaceNode;
}

export type WorkspaceNode = WorkspaceLeaf | WorkspaceSplit;

export interface WorkspaceModel {
  sessions: WorkspaceSession[];
  root: WorkspaceNode;
  focusedPane: string;
  focusModePane: string | null;
  nextPaneId: number;
  nextSplitId: number;
  nextSessionId: number;
}

export type WorkspacePlaceholderAction = 'wake' | 'retry' | 'materialize';

export const WORKSPACE_LAYOUT_STORAGE_KEY = 'ghostexWeb.workspace.v1';

const MIN_SPLIT_RATIO = 0.1;
const MAX_SPLIT_RATIO = 0.9;

function cloneNode(node: WorkspaceNode): WorkspaceNode {
  if (node.type === 'leaf') {
    return {
      ...node,
      tabGroup: {
        activeTab: node.tabGroup.activeTab,
        tabs: node.tabGroup.tabs.map((tab) => ({ ...tab })),
      },
    };
  }
  return {
    ...node,
    first: cloneNode(node.first),
    second: cloneNode(node.second),
  };
}

function cloneModel(model: WorkspaceModel): WorkspaceModel {
  return {
    ...model,
    sessions: model.sessions.map((session) => ({ ...session })),
    root: cloneNode(model.root),
  };
}

function emptyLeaf(paneId: string): WorkspaceLeaf {
  return {
    type: 'leaf',
    paneId,
    tabGroup: { activeTab: null, tabs: [] },
  };
}

function findLeaf(node: WorkspaceNode, paneId: string): WorkspaceLeaf | undefined {
  if (node.type === 'leaf') {
    return node.paneId === paneId ? node : undefined;
  }
  return findLeaf(node.first, paneId) ?? findLeaf(node.second, paneId);
}

function findSplit(node: WorkspaceNode, splitId: string): WorkspaceSplit | undefined {
  if (node.type === 'leaf') {
    return undefined;
  }
  if (node.splitId === splitId) {
    return node;
  }
  return findSplit(node.first, splitId) ?? findSplit(node.second, splitId);
}

function replaceLeaf(node: WorkspaceNode, paneId: string, replacement: WorkspaceNode): WorkspaceNode {
  if (node.type === 'leaf') {
    return node.paneId === paneId ? replacement : node;
  }
  return {
    ...node,
    first: replaceLeaf(node.first, paneId, replacement),
    second: replaceLeaf(node.second, paneId, replacement),
  };
}

function removeLeaf(node: WorkspaceNode, paneId: string): WorkspaceNode | null {
  if (node.type === 'leaf') {
    return node.paneId === paneId ? null : node;
  }
  const first = removeLeaf(node.first, paneId);
  const second = removeLeaf(node.second, paneId);
  if (!first) {
    return second;
  }
  if (!second) {
    return first;
  }
  return { ...node, first, second };
}

function removeTab(group: WorkspaceTabGroup, sessionId: string): WorkspaceTab | undefined {
  const index = group.tabs.findIndex((tab) => tab.sessionId === sessionId);
  if (index < 0) {
    return undefined;
  }
  const [tab] = group.tabs.splice(index, 1);
  if (group.activeTab === sessionId) {
    group.activeTab = group.tabs[index]?.sessionId ?? group.tabs.at(-1)?.sessionId ?? null;
  }
  return tab;
}

function insertTab(group: WorkspaceTabGroup, tab: WorkspaceTab, insertionIndex: number): void {
  let targetIndex = Math.min(Math.max(0, insertionIndex), group.tabs.length);
  const existingIndex = group.tabs.findIndex((candidate) => candidate.sessionId === tab.sessionId);
  if (existingIndex >= 0) {
    const [existing] = group.tabs.splice(existingIndex, 1);
    if (existingIndex < targetIndex) {
      targetIndex -= 1;
    }
    group.tabs.splice(Math.min(targetIndex, group.tabs.length), 0, existing);
    return;
  }
  group.tabs.splice(targetIndex, 0, tab);
}

function collectLeaves(node: WorkspaceNode, leaves: WorkspaceLeaf[]): void {
  if (node.type === 'leaf') {
    leaves.push(node);
    return;
  }
  collectLeaves(node.first, leaves);
  collectLeaves(node.second, leaves);
}

function collectTabs(node: WorkspaceNode, tabs: WorkspaceTab[]): void {
  if (node.type === 'leaf') {
    tabs.push(...node.tabGroup.tabs.map((tab) => ({ ...tab })));
    return;
  }
  collectTabs(node.first, tabs);
  collectTabs(node.second, tabs);
}

function firstLeaf(node: WorkspaceNode): WorkspaceLeaf {
  return node.type === 'leaf' ? node : firstLeaf(node.first);
}

function paneForSession(node: WorkspaceNode, sessionId: string): WorkspaceLeaf | undefined {
  if (node.type === 'leaf') {
    return node.tabGroup.tabs.some((tab) => tab.sessionId === sessionId) ? node : undefined;
  }
  return paneForSession(node.first, sessionId) ?? paneForSession(node.second, sessionId);
}

function rotateNodeClockwise(node: WorkspaceNode): WorkspaceNode {
  if (node.type === 'leaf') {
    return cloneNode(node);
  }
  const first = rotateNodeClockwise(node.first);
  const second = rotateNodeClockwise(node.second);
  if (node.axis === 'horizontal') {
    return { ...node, axis: 'vertical', first, second };
  }
  return {
    ...node,
    axis: 'horizontal',
    ratio: clampSplitRatio(1 - node.ratio),
    defaultRatio: clampSplitRatio(1 - node.defaultRatio),
    first: second,
    second: first,
  };
}

function nextPaneId(model: WorkspaceModel): [string, number] {
  return [`pane-${model.nextPaneId}`, model.nextPaneId + 1];
}

function nextSplitId(model: WorkspaceModel): [string, number] {
  return [`split-${model.nextSplitId}`, model.nextSplitId + 1];
}

export function clampSplitRatio(ratio: number): number {
  return Math.min(MAX_SPLIT_RATIO, Math.max(MIN_SPLIT_RATIO, ratio));
}

export function createWorkspaceModel(sessions: WorkspaceSession[] = []): WorkspaceModel {
  const paneId = 'pane-1';
  return {
    sessions: sessions.map((session) => ({ ...session })),
    root: {
      ...emptyLeaf(paneId),
      tabGroup: {
        activeTab: sessions[0] ? workspaceSessionId(sessions[0]) : null,
        tabs: sessions.map((session) => ({ sessionId: workspaceSessionId(session) })),
      },
    },
    focusedPane: paneId,
    focusModePane: null,
    nextPaneId: 2,
    nextSplitId: 1,
    nextSessionId: sessions.length + 1,
  };
}

export function workspaceLeaves(model: WorkspaceModel): WorkspaceLeaf[] {
  const leaves: WorkspaceLeaf[] = [];
  collectLeaves(model.root, leaves);
  return leaves;
}

export function workspaceLeaf(model: WorkspaceModel, paneId: string): WorkspaceLeaf | undefined {
  return findLeaf(model.root, paneId);
}

export function workspaceSession(model: WorkspaceModel, sessionId: string): WorkspaceSession | undefined {
  return model.sessions.find((session) => workspaceSessionId(session) === sessionId);
}

export function workspaceSessionId(session: WorkspaceSession): string {
  return session.workspaceId ?? session.sessionId;
}

export function workspacePaneForSession(model: WorkspaceModel, sessionId: string): WorkspaceLeaf | undefined {
  return paneForSession(model.root, sessionId);
}

export function selectWorkspaceTab(model: WorkspaceModel, paneId: string, sessionId: string): WorkspaceModel {
  const next = cloneModel(model);
  const leaf = findLeaf(next.root, paneId);
  if (!leaf?.tabGroup.tabs.some((tab) => tab.sessionId === sessionId)) {
    return model;
  }
  leaf.tabGroup.activeTab = sessionId;
  next.focusedPane = paneId;
  return next;
}

export function focusWorkspacePane(model: WorkspaceModel, paneId: string): WorkspaceModel {
  if (!findLeaf(model.root, paneId) || model.focusedPane === paneId) {
    return model;
  }
  return { ...cloneModel(model), focusedPane: paneId };
}

export function addWorkspaceSession(model: WorkspaceModel, paneId: string, session: WorkspaceSession): WorkspaceModel {
  const next = cloneModel(model);
  const leaf = findLeaf(next.root, paneId) ?? findLeaf(next.root, next.focusedPane);
  const sessionId = workspaceSessionId(session);
  if (!leaf || next.sessions.some((candidate) => workspaceSessionId(candidate) === sessionId)) {
    return model;
  }
  next.sessions.push({ ...session });
  const activeIndex = leaf.tabGroup.tabs.findIndex((tab) => tab.sessionId === leaf.tabGroup.activeTab);
  insertTab(leaf.tabGroup, { sessionId }, activeIndex + 1 || 0);
  leaf.tabGroup.activeTab = sessionId;
  next.focusedPane = leaf.paneId;
  next.nextSessionId += 1;
  return next;
}

export function splitWorkspacePane(
  model: WorkspaceModel,
  paneId: string,
  axis: WorkspaceSplitAxis,
  session: WorkspaceSession
): WorkspaceModel {
  const sessionId = workspaceSessionId(session);
  if (
    !findLeaf(model.root, paneId) ||
    model.sessions.some((candidate) => workspaceSessionId(candidate) === sessionId)
  ) {
    return model;
  }
  const next = cloneModel(model);
  const [newPaneId, nextPane] = nextPaneId(next);
  const [splitId, nextSplit] = nextSplitId(next);
  const existing = findLeaf(next.root, paneId);
  if (!existing) {
    return model;
  }
  const newLeaf: WorkspaceLeaf = {
    type: 'leaf',
    paneId: newPaneId,
    tabGroup: { activeTab: sessionId, tabs: [{ sessionId }] },
  };
  const split: WorkspaceSplit = {
    type: 'split',
    splitId,
    axis,
    ratio: 0.5,
    defaultRatio: 0.5,
    first: existing,
    second: newLeaf,
  };
  next.root = replaceLeaf(next.root, paneId, split);
  next.sessions.push({ ...session });
  next.focusedPane = newPaneId;
  next.focusModePane = null;
  next.nextPaneId = nextPane;
  next.nextSplitId = nextSplit;
  next.nextSessionId += 1;
  return next;
}

export function setWorkspaceSplitRatio(model: WorkspaceModel, splitId: string, ratio: number): WorkspaceModel {
  const nextRatio = clampSplitRatio(ratio);
  const next = cloneModel(model);
  const split = findSplit(next.root, splitId);
  if (!split || Math.abs(split.ratio - nextRatio) < 0.001) {
    return model;
  }
  split.ratio = nextRatio;
  return next;
}

export function closeWorkspaceTab(model: WorkspaceModel, paneId: string, sessionId: string): WorkspaceModel {
  const next = cloneModel(model);
  const leaf = findLeaf(next.root, paneId);
  if (!leaf || !removeTab(leaf.tabGroup, sessionId)) {
    return model;
  }
  next.sessions = next.sessions.filter((session) => workspaceSessionId(session) !== sessionId);
  if (next.sessions.length === 0) {
    next.root = emptyLeaf(paneId);
    next.focusedPane = paneId;
    next.focusModePane = null;
    return next;
  }
  if (leaf.tabGroup.tabs.length === 0) {
    next.root = removeLeaf(next.root, paneId) ?? emptyLeaf(paneId);
    next.focusedPane = firstLeaf(next.root).paneId;
  } else {
    next.focusedPane = paneId;
  }
  if (next.focusModePane && !findLeaf(next.root, next.focusModePane)) {
    next.focusModePane = null;
  }
  return next;
}

export function moveWorkspaceTab(
  model: WorkspaceModel,
  sourcePaneId: string,
  targetPaneId: string,
  sessionId: string,
  insertionIndex: number
): WorkspaceModel {
  const next = cloneModel(model);
  const source = findLeaf(next.root, sourcePaneId);
  const target = findLeaf(next.root, targetPaneId);
  if (!source || !target) {
    return model;
  }
  const tab = removeTab(source.tabGroup, sessionId);
  if (!tab) {
    return model;
  }
  if (sourcePaneId === targetPaneId) {
    insertTab(source.tabGroup, tab, insertionIndex);
    next.focusedPane = sourcePaneId;
    return next;
  }
  if (source.tabGroup.tabs.length === 0) {
    next.root = removeLeaf(next.root, sourcePaneId) ?? emptyLeaf(sourcePaneId);
  }
  const liveTarget = findLeaf(next.root, targetPaneId);
  if (!liveTarget) {
    return model;
  }
  insertTab(liveTarget.tabGroup, tab, insertionIndex);
  liveTarget.tabGroup.activeTab = sessionId;
  next.focusedPane = targetPaneId;
  if (next.focusModePane === sourcePaneId) {
    next.focusModePane = null;
  }
  return next;
}

export function rotateWorkspacePanes(model: WorkspaceModel): WorkspaceModel {
  if (workspaceLeaves(model).length <= 1) {
    return model;
  }
  return { ...cloneModel(model), root: rotateNodeClockwise(model.root), focusModePane: null };
}

export function mergeAllWorkspaceTabs(model: WorkspaceModel, requestedPaneId = model.focusedPane): WorkspaceModel {
  if (workspaceLeaves(model).length <= 1) {
    return model;
  }
  const target = findLeaf(model.root, requestedPaneId) ?? firstLeaf(model.root);
  const tabs: WorkspaceTab[] = [];
  collectTabs(model.root, tabs);
  const validTabs = tabs.filter((tab) =>
    model.sessions.some((session) => workspaceSessionId(session) === tab.sessionId)
  );
  if (validTabs.length === 0) {
    return model;
  }
  const activeTab =
    target.tabGroup.activeTab && validTabs.some((tab) => tab.sessionId === target.tabGroup.activeTab)
      ? target.tabGroup.activeTab
      : validTabs[0].sessionId;
  const next = cloneModel(model);
  next.root = {
    type: 'leaf',
    paneId: target.paneId,
    tabGroup: { tabs: validTabs, activeTab },
  };
  next.focusedPane = target.paneId;
  next.focusModePane = null;
  return next;
}

export function toggleWorkspaceFocusMode(model: WorkspaceModel, paneId = model.focusedPane): WorkspaceModel {
  if (model.focusModePane) {
    return { ...cloneModel(model), focusModePane: null };
  }
  const leaf = findLeaf(model.root, paneId);
  const visibleLeaves = workspaceLeaves(model).filter((candidate) =>
    candidate.tabGroup.tabs.some((tab) => {
      const session = workspaceSession(model, tab.sessionId);
      return session && session.presentationState !== 'sleeping';
    })
  );
  if (!leaf || visibleLeaves.length <= 1 || !visibleLeaves.some(({ paneId: id }) => id === paneId)) {
    return model;
  }
  return { ...cloneModel(model), focusedPane: paneId, focusModePane: paneId };
}

export function reconcileWorkspaceSessions(model: WorkspaceModel, sessions: WorkspaceSession[]): WorkspaceModel {
  const byId = new Map(sessions.map((session) => [workspaceSessionId(session), session]));
  const next = cloneModel(model);
  const priorSurfaceModes = new Map(
    model.sessions.flatMap((session) =>
      session.sessionSurfaceMode !== undefined
        ? [[workspaceSessionId(session), session.sessionSurfaceMode] as const]
        : []
    )
  );
  next.sessions = sessions.map((session) => ({
    ...session,
    // Incoming session lists (presentation feeds, loadWorkspaceLayout boot)
    // never carry the client-local surface mode; keep the persisted choice.
    ...(session.sessionSurfaceMode === undefined && priorSurfaceModes.has(workspaceSessionId(session))
      ? { sessionSurfaceMode: priorSurfaceModes.get(workspaceSessionId(session)) }
      : {}),
  }));

  const reconcileNode = (node: WorkspaceNode): WorkspaceNode | null => {
    if (node.type === 'leaf') {
      node.tabGroup.tabs = node.tabGroup.tabs.filter((tab) => byId.has(tab.sessionId));
      if (!node.tabGroup.tabs.some((tab) => tab.sessionId === node.tabGroup.activeTab)) {
        node.tabGroup.activeTab = node.tabGroup.tabs[0]?.sessionId ?? null;
      }
      return node.tabGroup.tabs.length > 0 ? node : null;
    }
    const first = reconcileNode(node.first);
    const second = reconcileNode(node.second);
    if (!first) return second;
    if (!second) return first;
    return { ...node, first, second };
  };

  next.root = reconcileNode(next.root) ?? emptyLeaf(next.focusedPane);
  const target = findLeaf(next.root, next.focusedPane) ?? firstLeaf(next.root);
  for (const session of sessions) {
    const sessionId = workspaceSessionId(session);
    if (!paneForSession(next.root, sessionId)) {
      target.tabGroup.tabs.push({ sessionId });
    }
  }
  if (!target.tabGroup.activeTab) {
    target.tabGroup.activeTab = target.tabGroup.tabs[0]?.sessionId ?? null;
  }
  if (!findLeaf(next.root, next.focusedPane)) {
    next.focusedPane = firstLeaf(next.root).paneId;
  }
  if (next.focusModePane && !findLeaf(next.root, next.focusModePane)) {
    next.focusModePane = null;
  }
  return next;
}

export function updateWorkspaceSession(model: WorkspaceModel, session: WorkspaceSession): WorkspaceModel {
  const sessionId = workspaceSessionId(session);
  const index = model.sessions.findIndex((candidate) => workspaceSessionId(candidate) === sessionId);
  if (index < 0) {
    return model;
  }
  const next = cloneModel(model);
  next.sessions[index] = {
    ...session,
    // Incoming sessions from presentation/attach flows never carry the
    // client-local surface mode; keep the persisted choice intact.
    ...(session.sessionSurfaceMode === undefined && model.sessions[index].sessionSurfaceMode !== undefined
      ? { sessionSurfaceMode: model.sessions[index].sessionSurfaceMode }
      : {}),
  };
  return next;
}

export function setWorkspaceSessionSurfaceMode(
  model: WorkspaceModel,
  sessionId: string,
  mode: SessionSurfaceMode
): WorkspaceModel {
  const index = model.sessions.findIndex((candidate) => workspaceSessionId(candidate) === sessionId);
  if (index < 0 || (model.sessions[index].sessionSurfaceMode ?? 'terminal') === mode) {
    return model;
  }
  const next = cloneModel(model);
  next.sessions[index].sessionSurfaceMode = mode;
  return next;
}

export function reconcileOpenWorkspaceSessions(
  model: WorkspaceModel,
  availableSessions: WorkspaceSession[],
  authoritativeMachineIds: ReadonlySet<string>
): WorkspaceModel {
  const availableById = new Map(availableSessions.map((session) => [workspaceSessionId(session), session]));
  const sessions = model.sessions.flatMap((session) => {
    const available = availableById.get(workspaceSessionId(session));
    if (available) {
      return [
        {
          ...available,
          // Presentation feeds never carry the client-local surface mode.
          ...(session.sessionSurfaceMode !== undefined ? { sessionSurfaceMode: session.sessionSurfaceMode } : {}),
        },
      ];
    }
    return authoritativeMachineIds.has(session.machineId) ? [] : [{ ...session }];
  });
  return reconcileWorkspaceSessions(model, sessions);
}
