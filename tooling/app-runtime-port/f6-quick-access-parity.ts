/**
 * The app runtime port's F6 Quick Access gate: the TypeScript controller the QuickJS runtime ran
 * (apps/desktop/sidebar/native-quick-access/, deleted in step 3 and read from git at
 * `FROZEN_RUNTIME_REVISION` by frozen-runtime.ts) and the Rust model that replaces it
 * (packages/gx-core/src/quick_access/) driven through the same scenarios, and everything each asked
 * the host to do diffed as JSON: every snapshot, menu, close, post, modal, clipboard write and timer.
 * Zero differences is the bar.
 *
 *   bun tooling/app-runtime-port/f6-quick-access-parity.ts [--inject <mutation>] [--keep <dir>]
 *
 * Inputs are READ from the local gxserver (the presentation snapshot, a page of previous sessions,
 * the saved prompts and their tags, the recent projects; nothing is written) and projected with the
 * runtime's own sidebar projection, so the scenarios run over every session and project of this
 * computer. The same store is given to both halves; how the desktop fills that store is proved in
 * the app (PROGRESS.md), not here. The fixtures hold private data and live in a temporary folder
 * outside the repository (`--keep` names one).
 *
 * Both halves run in UTC and in two platforms (`mac`, `windows`), the TypeScript half in a child
 * process per platform because the controller reads the platform once at import.
 *
 * Mutations: drop-row, wrong-post, selection, hotkey-label.
 */
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { homedir, tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { frozenRuntimeRoot } from './frozen-runtime';

type Json = any;

const root = fileURLToPath(new URL('../../', import.meta.url));
const args = process.argv.slice(2);
const option = (name: string) => {
  const index = args.indexOf(name);
  return index >= 0 ? args[index + 1] : undefined;
};

// Scenario clock: a fixed instant, in UTC on both sides.
const NOW = Date.parse('2026-09-25T12:00:00.000Z');

// ------------------------------------------------------------------------ the daemon (read only)

async function readDaemon(): Promise<Json> {
  const stateDir = process.env.GX_STATE_DIR ?? join(homedir(), '.local/state/ghostex/gxserver');
  const token = readFileSync(join(stateDir, 'auth/token'), 'utf8').trim();
  const port = JSON.parse(readFileSync(join(stateDir, 'runtime/server.json'), 'utf8')).port;
  const call = async (path: string, params: Json = {}) => {
    const response = await fetch(`http://127.0.0.1:${port}${path}`, {
      body: JSON.stringify({ params, protocolVersion: 1 }),
      headers: { authorization: `Bearer ${token}`, 'content-type': 'application/json' },
      method: 'POST',
    });
    const body = (await response.json()) as Json;
    if (!body.ok) throw new Error(`${path} failed`);
    return body.result;
  };
  const { snapshot } = await call('/api/readPresentationSnapshot');
  const previous = await call('/api/listPreviousSessions', { includeActive: false, includePrevious: true, limit: 80 });
  const stashed = await call('/api/listStashedPrompts', { includeRecovery: false, includeDelivered: false });
  const recent = await call('/api/listRecentProjects');
  return { snapshot, previous, stashed, recent };
}

/** The Rust bridge's `previousSessionsResult` item, reduced to the fields Quick Access reads. */
function previousItem(result: Json): Json | undefined {
  if (!result.projectId || !result.sessionId || !result.projectTitle) return undefined;
  const title = result.displayTitle ?? result.primaryTitle ?? result.title ?? 'Previous Session';
  const closedAt = result.closedAt ?? result.updatedAt ?? result.createdAt;
  if (!closedAt) return undefined;
  const item: Json = {
    agentIcon: result.agentIcon ?? result.agentName ?? result.agentId,
    alias: title,
    closedAt,
    externalSession: result.externalSession === true,
    historyId: `gxserver:${result.projectId}:${result.sessionId}`,
    isFavorite: result.isFavorite === true,
    isRestorable: result.isRestorable ?? true,
    lastInteractionAt: result.lastActiveAt,
    primaryTitle: result.primaryTitle ?? title,
    projectId: result.projectId,
    projectName: result.projectTitle,
    sessionId: result.sessionId,
  };
  if (result.displayTitle) item.displayTitle = result.displayTitle;
  if (result.forkBranchCount) item.forkBranchCount = result.forkBranchCount;
  if (result.sessionTag) item.sessionTag = result.sessionTag;
  if (result.terminalTitle) item.terminalTitle = result.terminalTitle;
  return item;
}

// ------------------------------------------------------------------------ scenarios

async function buildScenarios(dir: string): Promise<void> {
  // The projection pulls in the client-storage adapter, which needs the shimmed browser first.
  await import('@/tooling/gx-core/browser-shim');
  const { createGxserverPresentationSidebarGroups } =
    await import('@/packages/shared/gxserver-presentation-sidebar-projection');
  const projection = await import(
    `${frozenRuntimeRoot()}/apps/desktop/sidebar/gxserver-runtime/helpers/presentation-projection`
  );
  const daemon = await readDaemon();
  // `createSidebarGroups` for this computer, as the runtime built its store (and as
  // tooling/gx-core/action-parity-typescript.ts does, without loading the runtime class).
  const metadata = projection.createGpuiPresentationProjectProjectionMetadata({
    domainProjects: [],
    presentation: daemon.snapshot,
    projectOrder: daemon.snapshot.workspaceGroups?.projectOrder,
    recentProjects: [],
  });
  const groups: Json[] = createGxserverPresentationSidebarGroups({
    activeProjectId: undefined,
    chatProjectIds: metadata.chatProjectIds,
    focusedSessionId: undefined,
    hiddenProjectIds: metadata.hiddenProjectIds,
    hiddenSessionKeys: undefined,
    presentation: daemon.snapshot,
    projectOverlays: metadata.projectOverlays,
    resolveAgentIcon: projection.resolveGpuiSidebarAgentIcon,
    resolveSessionRoutingId: projection.createGpuiSidebarSessionRoutingId,
    visibleSessionIds: undefined,
  });
  const storeGroups = groups.map((group: Json) => ({
    groupId: group.groupId,
    title: group.title ?? '',
    editorProjectId: group.projectContext?.editor?.projectId,
    remoteMachineId: group.remoteMachineContext?.machineId,
    remoteProjectId: group.remoteMachineContext?.projectId,
    sessions: (group.sessions ?? []).map((session: Json) => ({
      sessionId: session.sessionId,
      lastInteractionAt: session.lastInteractionAt,
      alias: session.alias ?? '',
      displayTitle: session.displayTitle,
      primaryTitle: session.primaryTitle,
      terminalTitle: session.terminalTitle,
      detail: session.detail,
      sessionNumber: session.sessionNumber,
      sessionTag: session.sessionTag,
      isFavorite: session.isFavorite === true,
      faviconDataUrl: session.faviconDataUrl,
      agentIcon: session.agentIcon,
      sessionKind: session.sessionKind,
      lifecycleState: session.lifecycleState,
      agentSessionId: session.agentSessionId,
      sessionRoutingId: session.sessionRoutingId,
    })),
  }));
  const firstProject = storeGroups.find((group) => group.editorProjectId);
  const firstSession = storeGroups.flatMap((group) => group.sessions)[0];
  const previous = (daemon.previous.results ?? []).map(previousItem).filter(Boolean);
  const prompts = daemon.stashed.prompts ?? [];
  const tags = daemon.stashed.tags ?? [];
  const recentProjects = [
    ...storeGroups
      .filter((group) => group.editorProjectId)
      .slice(0, 6)
      .map((group, index) => ({
        isOpen: true,
        path: `/fixture/${index}`,
        projectId: group.editorProjectId,
        sessionCount: group.sessions.length,
        title: group.title,
        updatedAt: new Date(NOW - index * 3_600_000).toISOString(),
      })),
    ...(daemon.recent.recentProjects ?? []).map((project: Json) => ({
      ...project,
      isOpen: false,
      sessionCount: project.sessionCount ?? 0,
    })),
    {
      isOpen: false,
      path: '/fixture/tabler',
      projectId: 'fixture-tabler',
      sessionCount: 2,
      title: 'Tabler',
      icon: { kind: 'tabler', icon: 'rocket', color: '#AABBCC' },
    },
    { isOpen: false, path: '/fixture/never', projectId: 'fixture-never', sessionCount: 0, title: 'Never Closed' },
  ];
  const commands = [
    {
      actionType: 'terminal',
      command: 'bun run dev\nsecond line',
      commandId: 'dev',
      name: 'Dev Server',
      closeTerminalOnExit: true,
    },
    { actionType: 'browser', commandId: 'docs', name: '', url: 'https://example.com', icon: 'book' },
    { actionType: 'terminal', commandId: 'empty', name: 'Unconfigured' },
    { actionType: 'terminal', commandId: 'nameless', name: '' },
    { actionType: 'terminal', command: 'make', commandId: 'iconnull', name: '', icon: null },
    { actionType: 'terminal', command: 'ls', commandId: 'sixth', name: 'Sixth' },
  ];
  const hudFor = (settings: Json) => ({
    activeProjectId: firstProject?.editorProjectId,
    activeProjectSpaceRefs: [{ sectionKey: 'local', spaceId: 'space-a' }],
    commands,
    settings,
  });
  const baseSettings = {
    hotkeys: {
      openFindPrompts: 'cmd+shift+f',
      runActionSlot2: 'ctrl+shift+2',
      focusGroup3: 'cmd+alt+3',
      openSettings: '',
    },
    petOverlayEnabled: false,
    preferredAgentInterface: 'chat',
    sidebarSessionTagListItems: undefined,
    workspaceOpenTargetHiddenIds: ['zed'],
    workspaceOpenTargetAvailability: { availableTargetIds: ['vscode', 'cursor', 'zed'] },
    customWorkspaceOpenTargets: [{ id: 'custom:one', label: 'My Editor' }],
  };
  const variants: [string, Json][] = [
    ['default', baseSettings],
    [
      'narrowed',
      {
        ...baseSettings,
        browserViewTabHidden: true,
        petOverlayEnabled: true,
        preferredAgentInterface: 'terminal',
        hotkeys: { createAgentSession: 'cmd+t', createSession: 'cmd+shift+t', openCommandPalette: 'cmd+k' },
        viewScopes: {
          'official:docs': { default: 'shown', projects: {}, spaces: { 'local:space-a': 'hidden' } },
          'official:code': {
            default: 'hidden',
            projects: firstProject ? { [firstProject.editorProjectId]: 'shown' } : {},
            spaces: {},
          },
        },
      },
    ],
  ];
  // What the desktop host hands the model for the settings above: the visible targets in
  // catalog order, Finder left out (apps/desktop/src/app/helpers/agents_hub/open_targets.rs).
  const openTargets = [
    { id: 'cursor', label: 'Cursor', custom: false },
    { id: 'vscode', label: 'VS Code', custom: false },
    { id: 'custom:one', label: 'My Editor', custom: true },
  ];
  const commandRunStates = { dev: { status: 'error', activeRunIds: [] } };
  const storage = {
    hidden: {
      collectionKeys: ['local:col-1'],
      groupIds: firstProject ? [`combined-project:${encodeURIComponent(firstProject.editorProjectId)}`] : [],
    },
    collections: [{ collectionId: 'col-1', projectIds: ['fixture-tabler'] }],
    recovered: [
      {
        sessionKey: 'p1:s1',
        projectId: 'p1',
        sessionId: 's1',
        text: 'recovered draft\nline two',
        updatedAt: NOW - 50_000,
      },
      {
        sessionKey: 'p2:s2',
        projectId: firstProject?.editorProjectId,
        sessionId: 's2',
        text: 'another',
        updatedAt: NOW - 90_000_000,
      },
    ],
    sent: [
      {
        promptId: 'sent:a',
        content: 'sent one',
        createdAt: new Date(NOW - 10_000).toISOString(),
        updatedAt: new Date(NOW - 10_000).toISOString(),
        cwd: null,
        projectId: firstProject?.editorProjectId ?? null,
        projectName: null,
        sessionId: 'x',
      },
      {
        promptId: 'sent:b',
        content: 'sent two',
        createdAt: new Date(NOW - 200_000_000).toISOString(),
        updatedAt: new Date(NOW - 200_000_000).toISOString(),
        cwd: null,
        projectId: null,
        projectName: null,
        sessionId: null,
      },
    ],
  };
  const firstClosed = previous[0];
  const promptSession = firstSession?.sessionId;
  const firstPrompt = prompts[0];
  const firstTag = tags.find((tag: Json) => tag.tagId !== 'favorite');

  const flows: Record<string, Json[]> = {
    commands: [
      { command: { type: 'open', tab: 'commands', query: '' } },
      { flush: true },
      { command: { type: 'query', query: 'new' } },
      { flush: true },
      { command: { type: 'query', query: 'zzzqqqx' } },
      { flush: true },
      { command: { type: 'query', query: 'dev' } },
      { flush: true },
      { command: { type: 'query', query: '' } },
      { command: { type: 'select', key: 'hotkey:openHotkeys', seq: 3 } },
      { flush: true },
      { command: { type: 'secondary', key: 'hotkey:openHotkeys', x: 1, y: 2 } },
      { command: { type: 'secondary', key: '', x: 1, y: 2 } },
      { command: { type: 'activate', key: 'pet' } },
      { command: { type: 'open', tab: 'commands' } },
      { command: { type: 'activate', key: 'appModal:previousSessions' } },
      { command: { type: 'activate', key: 'appModal:agentsHub' } },
      { command: { type: 'open', tab: 'commands' } },
      { command: { type: 'activate', key: 'project:dev' } },
      { command: { type: 'open', tab: 'commands' } },
      { command: { type: 'activate', key: 'project:empty' } },
      { command: { type: 'open', tab: 'commands' } },
      { command: { type: 'activate', key: 'openTarget:openTarget:custom:one' } },
      { command: { type: 'open', tab: 'commands' } },
      { command: { type: 'activate', key: 'sidebarMessage:changelog' } },
      { command: { type: 'open', tab: 'commands' } },
      { command: { type: 'activate', key: 'hotkey:delayedSend' } },
      { command: { type: 'activate', key: 'hotkey:openSettings' } },
      { command: { type: 'open', tab: 'commands' } },
      { command: { type: 'actionHotkey', key: 'pet', hotkey: 'cmd+d' } },
      { store: true },
      { flush: true },
      { command: { type: 'closed' } },
      { store: true },
    ],
    projects: [
      { command: { type: 'open', tab: 'recentProjects' } },
      { flush: true },
      { receive: { type: 'recentProjectsResult', recentProjects: recentProjects } },
      { flush: true },
      { command: { type: 'query', query: 'fix ta' } },
      { flush: true },
      { command: { type: 'query', query: '' } },
      { command: { type: 'secondary', key: 'project:fixture-tabler', x: 0, y: 0 } },
      { command: { type: 'menuItem', id: 'copyPath' } },
      { command: { type: 'secondary', key: 'project:fixture-tabler', x: 0, y: 0 } },
      { command: { type: 'menuItem', id: 'openLocation' } },
      { command: { type: 'actionHotkey', key: 'project:fixture-never', hotkey: 'cmd+d' } },
      { command: { type: 'actionHotkey', key: 'project:fixture-never', hotkey: 'cmd+shift+c' } },
      { flush: true },
      { command: { type: 'activate', key: 'project:fixture-tabler' } },
      { command: { type: 'open', tab: 'recentProjects', machineId: 'remote-x' } },
      { receive: { type: 'recentProjectsResult', recentProjects: recentProjects } },
      { receive: { type: 'recentProjectsResult', machineId: 'remote-x', recentProjects: recentProjects.slice(-3) } },
      { flush: true },
      { command: { type: 'secondary', key: 'project:fixture-never', x: 0, y: 0 } },
      { command: { type: 'menuItem', id: 'openTerminal' } },
      { command: { type: 'open', tab: 'recentProjects' } },
      { receive: { type: 'recentProjectsResult', recentProjects: recentProjects } },
      { command: { type: 'activate', key: firstProject ? `project:${firstProject.editorProjectId}` : 'project:none' } },
    ],
    sessions: [
      { command: { type: 'open', tab: 'recentSessions' } },
      { flush: true },
      { timer: true },
      {
        receive: {
          type: 'previousSessionsResult',
          requestId: '$last:previous-sessions',
          previousSessions: previous,
          cursor: 'next-1',
          projects: [{ projectId: 'zz-project', name: 'Zed Project' }],
        },
      },
      { flush: true },
      {
        receive: {
          type: 'sessionTranscriptSizesResult',
          requestId: '$last:session-transcript-sizes',
          sizes: [
            ...(firstClosed ? [{ key: `closed:${firstClosed.historyId}`, sizeBytes: 1280 }] : []),
            ...(firstSession ? [{ key: `open:${firstSession.sessionId}`, sizeBytes: null }] : []),
            { key: 'closed:none', sizeBytes: 5_000_000 },
          ],
        },
      },
      { flush: true },
      { command: { type: 'query', query: 'fix' } },
      { flush: true },
      { timer: true },
      {
        receive: {
          type: 'previousSessionsResult',
          requestId: '$last:previous-sessions',
          previousSessions: previous.slice(0, 5),
        },
      },
      { flush: true },
      { command: { type: 'query', query: 'ab' } },
      { flush: true },
      { command: { type: 'query', query: '' } },
      { command: { type: 'scope', value: 'cycle' } },
      { command: { type: 'scope', value: 'cycle' } },
      { timer: true },
      { flush: true },
      { command: { type: 'scope', value: 'all' } },
      { command: { type: 'tagFilter', value: 'favorite' } },
      { timer: true },
      { flush: true },
      { command: { type: 'tagFilter', value: 'favorite' } },
      { command: { type: 'project', value: firstProject?.editorProjectId ?? 'x' } },
      { timer: true },
      { flush: true },
      { command: { type: 'project', value: '' } },
      { command: { type: 'loadMore' } },
      { command: { type: 'query', query: 'a' } },
      { command: { type: 'loadMore' } },
      { command: { type: 'query', query: '' } },
      { timer: true },
      {
        receive: {
          type: 'previousSessionsResult',
          requestId: '$last:previous-sessions',
          previousSessions: previous,
          cursor: 'next-2',
        },
      },
      { flush: true },
      ...(firstClosed
        ? [
            { command: { type: 'secondary', key: `closed:${firstClosed.historyId}`, x: 0, y: 0 } },
            { command: { type: 'actionHotkey', key: `closed:${firstClosed.historyId}`, hotkey: 'cmd+d' } },
            { flush: true },
          ]
        : []),
      ...(firstSession
        ? [
            { command: { type: 'select', key: `open:${firstSession.sessionId}`, seq: 9 } },
            { flush: true },
            { command: { type: 'secondary', key: `open:${firstSession.sessionId}`, x: 0, y: 0 } },
            { command: { type: 'menuItem', id: 'findPrompts' } },
            { command: { type: 'open', tab: 'recentSessions', scope: 'external', projectId: 'p' } },
            { timer: true },
            { command: { type: 'activate', key: `open:${firstSession.sessionId}` } },
          ]
        : []),
      ...(previous[1]
        ? [
            { command: { type: 'open', tab: 'recentSessions' } },
            { timer: true },
            {
              receive: {
                type: 'previousSessionsResult',
                requestId: '$last:previous-sessions',
                previousSessions: previous,
              },
            },
            { command: { type: 'activate', key: `closed:${previous[1].historyId}` } },
          ]
        : []),
    ],
    prompts: [
      {
        command: {
          type: 'open',
          tab: 'savedPrompts',
          promptSessionId: promptSession,
          promptProjectId: firstProject?.editorProjectId,
        },
      },
      { flush: true },
      { receive: { type: 'stashedPromptsResult', requestId: '$last:stashed-prompts', prompts, tags } },
      { flush: true },
      { command: { type: 'query', query: 'the' } },
      { flush: true },
      { command: { type: 'query', query: '' } },
      { command: { type: 'project', value: 'scope:all' } },
      { flush: true },
      ...(firstTag ? [{ command: { type: 'tagFilter', value: `tag:${firstTag.tagId}` } }, { flush: true }] : []),
      { command: { type: 'tagFilter', value: 'tag:none' } },
      { flush: true },
      { command: { type: 'tagFilter', value: 'tag:all' } },
      { command: { type: 'view', value: 'recovered' } },
      { flush: true },
      {
        receive: {
          type: 'stashedPromptsResult',
          requestId: '$last:stashed-prompts-recovered',
          recoveryDrafts: [],
          drafts: [],
        },
      },
      { flush: true },
      { command: { type: 'secondary', key: 'prompt:recovered:p1:s1', x: 0, y: 0 } },
      { command: { type: 'actionHotkey', key: 'prompt:recovered:p1:s1', hotkey: 'cmd+s' } },
      { command: { type: 'actionHotkey', key: 'prompt:recovered:p1:s1', hotkey: 'cmd+d' } },
      { flush: true },
      { command: { type: 'view', value: 'sent' } },
      { receive: { type: 'stashedPromptsResult', requestId: '$last:stashed-prompts-sent', deliveredDrafts: [] } },
      { flush: true },
      { command: { type: 'actionHotkey', key: 'prompt:sent:b', hotkey: 'cmd+d' } },
      { flush: true },
      { command: { type: 'view', value: 'saved' } },
      { command: { type: 'tagFilter', value: 'tag:new' } },
      { command: { type: 'tagComposerField', field: 'name', value: '  New   Tag ' } },
      { command: { type: 'tagComposerSubmit' } },
      { flush: true },
      {
        receive: {
          type: 'stashedPromptTagsResult',
          ok: true,
          tags: [...tags, { tagId: 'made-1', name: 'new tag', color: '#e3b341' }],
        },
      },
      { flush: true },
      { command: { type: 'tagFilter', value: 'tag:all' } },
      ...(firstPrompt
        ? [
            { command: { type: 'select', key: `prompt:${firstPrompt.promptId}`, seq: 4 } },
            { flush: true },
            { command: { type: 'secondary', key: `prompt:${firstPrompt.promptId}`, x: 0, y: 0 } },
            { command: { type: 'actionHotkey', key: `prompt:${firstPrompt.promptId}`, hotkey: 'cmd+s' } },
            { command: { type: 'actionHotkey', key: `prompt:${firstPrompt.promptId}`, hotkey: 'cmd+t' } },
            ...(firstTag ? [{ command: { type: 'menuItem', id: `tag:${firstTag.tagId}` } }] : []),
            { command: { type: 'actionHotkey', key: `prompt:${firstPrompt.promptId}`, hotkey: 'cmd+t' } },
            { command: { type: 'menuItem', id: 'tag:new' } },
            { command: { type: 'tagComposerField', field: 'color', value: '#7f9cf5' } },
            { command: { type: 'tagComposerCancel' } },
            { command: { type: 'actionHotkey', key: `prompt:${firstPrompt.promptId}`, hotkey: 'cmd+shift+c' } },
            { command: { type: 'actionHotkey', key: `prompt:${firstPrompt.promptId}`, hotkey: 'cmd+e' } },
            { flush: true },
            { command: { type: 'editorField', field: 'content', value: 'edited text   \nsecond  \t' } },
            { command: { type: 'editorFavorite' } },
            { command: { type: 'editorSubmit' } },
            { flush: true },
            {
              receive: {
                type: 'saveStashedPromptResult',
                requestId: '$last:save-stashed-prompt',
                ok: true,
                prompt: { ...firstPrompt, content: 'edited text\nsecond' },
              },
            },
            { flush: true },
            { receive: { type: 'setStashedPromptTagsResult', ok: false } },
            { flush: true },
          ]
        : []),
      { command: { type: 'secondary', key: '', x: 0, y: 0 } },
      { command: { type: 'menuItem', id: 'addPrompt' } },
      { command: { type: 'editorField', field: 'project', value: 'project:none' } },
      { command: { type: 'editorField', field: 'content', value: 'brand new' } },
      { command: { type: 'editorSubmit' } },
      {
        receive: { type: 'saveStashedPromptResult', requestId: '$last:save-stashed-prompt', ok: false, error: 'nope' },
      },
      { flush: true },
      { command: { type: 'editorCancel' } },
      ...(firstPrompt
        ? [
            { command: { type: 'actionHotkey', key: `prompt:${firstPrompt.promptId}`, hotkey: 'cmd+o' } },
            { command: { type: 'open', tab: 'savedPrompts', promptScope: 'all' } },
            { receive: { type: 'stashedPromptsResult', requestId: '$last:stashed-prompts', prompts, tags } },
            { command: { type: 'actionHotkey', key: `prompt:${firstPrompt.promptId}`, hotkey: 'cmd+d' } },
            { flush: true },
            { command: { type: 'activate', key: `prompt:${prompts[1]?.promptId ?? firstPrompt.promptId}` } },
          ]
        : []),
    ],
  };

  mkdirSync(join(dir, 'scenarios'), { recursive: true });
  for (const platform of ['mac', 'windows']) {
    for (const [variant, settings] of variants) {
      for (const [flow, steps] of Object.entries(flows)) {
        const scenario = {
          clock: { nowMs: NOW, offsetMs: 0 },
          data: {
            commandRunStates,
            groups: storeGroups,
            hud: hudFor(settings),
            localCustomTags: undefined,
            openTargets,
            platform,
            remoteCustomTags: [],
          },
          platform,
          steps,
          storage,
        };
        writeFileSync(join(dir, 'scenarios', `${platform}-${variant}-${flow}.json`), JSON.stringify(scenario));
      }
    }
  }
}

// ------------------------------------------------------------------------ the TypeScript half

/** Replaces `$last:<kind>` with the newest request id of that kind the run posted. */
function resolvePlaceholders(value: Json, lastIds: Map<string, string>): Json {
  if (typeof value === 'string' && value.startsWith('$last:')) return lastIds.get(value.slice(6)) ?? value;
  if (Array.isArray(value)) return value.map((item) => resolvePlaceholders(item, lastIds));
  if (value && typeof value === 'object') {
    return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, resolvePlaceholders(item, lastIds)]));
  }
  return value;
}

function noteRequestIds(effects: Json[], lastIds: Map<string, string>): void {
  for (const effect of effects) {
    const requestId = effect.post?.requestId;
    const match = typeof requestId === 'string' ? /^(.*)-\d+-\d+$/u.exec(requestId) : null;
    if (match) lastIds.set(match[1], requestId);
  }
}

async function runTypeScript(dir: string, platform: string): Promise<void> {
  process.env.TZ = 'UTC';
  const globals = globalThis as Json;
  // The controller reads the platform once at import (`STASH_PROMPT_HINT`), so it is set first.
  // The runtime ran in QuickJS, whose `localeCompare` ignores its options and compares NFC code
  // points; Bun's is ICU collation. The TypeScript half is measured as the app ran it.
  String.prototype.localeCompare = function (this: string, that: string) {
    const left = [...String(this).normalize('NFC')].map((c) => c.codePointAt(0)!);
    const right = [...String(that).normalize('NFC')].map((c) => c.codePointAt(0)!);
    for (let index = 0; index < Math.min(left.length, right.length); index++) {
      if (left[index] !== right[index]) return left[index] < right[index] ? -1 : 1;
    }
    return left.length === right.length ? 0 : left.length < right.length ? -1 : 1;
  } as typeof String.prototype.localeCompare;
  Object.defineProperty(globals, 'navigator', {
    configurable: true,
    value: {
      platform: platform === 'mac' ? 'MacIntel' : 'Win32',
      userAgent: platform === 'mac' ? 'Macintosh' : 'Windows',
    },
  });
  await import('@/tooling/gx-core/browser-shim');
  const { resetBrowserStorage } = await import('@/tooling/gx-core/browser-shim');
  /*
   * The Recovered and Sent views read and write the chat's own stores, which have their own gates;
   * this one hands both halves the same lists and the same deletes, so the modules are replaced
   * before the controller links against them.
   */
  const chat = { recovered: [] as Json[], sent: [] as Json[] };
  const { mock } = await import('bun:test');
  const draftStoragePath = '@/packages/core-ui/chat/session-chat-draft-storage';
  const recoveryPath = '@/packages/core-ui/chat/session-chat-draft-recovery';
  const sentPath = '@/packages/core-ui/chat/session-chat-sent-history';
  const realDrafts = await import(draftStoragePath);
  const realRecovery = await import(recoveryPath);
  const realSent = await import(sentPath);
  mock.module(draftStoragePath, () => ({
    ...realDrafts,
    listRecoveredSessionChatDrafts: () => chat.recovered.map((draft) => ({ ...draft })),
    deleteStoredSessionChatDraft: (key: string) => {
      chat.recovered = chat.recovered.filter((draft) => draft.sessionKey !== key);
    },
    reconcileSessionChatDraftsFromServer: () => {},
  }));
  mock.module(recoveryPath, () => ({ ...realRecovery, dismissDraftRecovery: () => {}, importDraftRecovery: () => {} }));
  mock.module(sentPath, () => ({
    ...realSent,
    listSentSessionChatMessages: () => chat.sent.map((message) => ({ ...message })),
    deleteSentSessionChatMessage: (id: string) => {
      chat.sent = chat.sent.filter((message) => message.promptId !== id);
    },
    recordDeliveredSessionChatDrafts: () => {},
  }));
  const { sidebarStore } = await import('@/packages/core-ui/sidebar-store-model');
  const { connectNativeQuickAccess } = await import(
    `${frozenRuntimeRoot()}/apps/desktop/sidebar/native-quick-access/controller`
  );
  const { storageScope } = await import('@/packages/client-storage');

  let recorded: Json[] = [];
  const window = globals.window as Json;
  let rafQueue: (() => void)[] = [];
  let timers = new Map<number, () => void>();
  let lastTimer: number | undefined;
  let nextId = 1;
  window.requestAnimationFrame = (callback: () => void) => {
    recorded.push({ schedulePublish: true });
    rafQueue.push(callback);
    return nextId++;
  };
  window.cancelAnimationFrame = () => {};
  window.setTimeout = (callback: () => void, delay: number) => {
    recorded.push({ scheduleSessions: delay });
    const id = nextId++;
    timers.set(id, callback);
    lastTimer = id;
    return id;
  };
  window.clearTimeout = (id: number) => timers.delete(id);
  window.webkit = {
    messageHandlers: {
      ghostexAppModalHost: {
        postMessage(message: Json) {
          if (message.type === 'sidebarCommand') recorded.push({ post: message.message });
          else if (message.type === 'copySessionDetails') recorded.push({ copyText: message.detailsText });
          else recorded.push({ openModal: message });
        },
      },
    },
  };
  window.__ghostex_APP_MODAL_HOST_SURFACE__ = undefined;
  const realNow = Date.now;

  const tsDir = join(dir, `ts`);
  mkdirSync(tsDir, { recursive: true });
  for (const name of readdirSync(join(dir, 'scenarios')).sort()) {
    if (!name.startsWith(`${platform}-`)) continue;
    const scenario = JSON.parse(readFileSync(join(dir, 'scenarios', name), 'utf8'));
    resetBrowserStorage();
    let now = scenario.clock.nowMs as number;
    Date.now = () => now;
    // Storage the controller reads in place.
    storageScope(['hiddenItems']).setItem('ghostex.sidebar.hidden-items.v1', JSON.stringify(scenario.storage.hidden));
    storageScope(['collections']).setItem(
      'ghostex.sidebar.projectCollections.v1',
      JSON.stringify({
        collections: scenario.storage.collections.map((collection: Json) => ({
          ...collection,
          title: 'Fixture',
          color: '#2f9b95',
        })),
        nextCollectionNumber: 2,
      })
    );
    chat.sent = [...scenario.storage.sent];
    chat.recovered = [...scenario.storage.recovered];
    const data = scenario.data;
    sidebarStore.setState({
      commandRunStates: data.commandRunStates,
      customSessionTags: data.localCustomTags,
      groupOrder: data.groups.map((group: Json) => group.groupId),
      groupsById: Object.fromEntries(
        data.groups.map((group: Json) => [
          group.groupId,
          {
            groupId: group.groupId,
            title: group.title,
            ...(group.editorProjectId ? { projectContext: { editor: { projectId: group.editorProjectId } } } : {}),
            ...(group.remoteMachineId
              ? { remoteMachineContext: { machineId: group.remoteMachineId, projectId: group.remoteProjectId } }
              : {}),
          },
        ])
      ),
      hud: data.hud,
      previousSessions: [],
      remoteCustomSessionTagsByMachineId: {},
      sessionIdsByGroup: Object.fromEntries(
        data.groups.map((group: Json) => [group.groupId, group.sessions.map((session: Json) => session.sessionId)])
      ),
      sessionsById: Object.fromEntries(
        data.groups.flatMap((group: Json) => group.sessions.map((session: Json) => [session.sessionId, session]))
      ),
    });
    const messageSource = new EventTarget();
    window.ghostexGpui = {
      postNativeQuickAccessSnapshot(payload: string) {
        recorded.push({ update: JSON.parse(payload) });
        return true;
      },
    };
    rafQueue = [];
    timers = new Map();
    lastTimer = undefined;
    const disconnect = connectNativeQuickAccess({ messageSource, vscode: { postMessage() {} } } as Json);
    const out: Json[] = [];
    const lastIds = new Map<string, string>();
    for (const step of scenario.steps) {
      recorded = [];
      if (typeof step.now === 'number') {
        now = step.now;
      } else if (step.command) {
        window.ghostexGpui.onNativeQuickAccessCommand(resolvePlaceholders(step.command, lastIds));
      } else if (step.receive) {
        messageSource.dispatchEvent(new MessageEvent('message', { data: resolvePlaceholders(step.receive, lastIds) }));
      } else if (step.flush) {
        const queue = rafQueue;
        rafQueue = [];
        for (const callback of queue) callback();
      } else if (step.timer) {
        const id = lastTimer;
        lastTimer = undefined;
        const callback = id === undefined ? undefined : timers.get(id);
        if (id !== undefined) timers.delete(id);
        callback?.();
      } else if (step.store) {
        sidebarStore.setState({ revision: Math.random() });
      }
      noteRequestIds(recorded, lastIds);
      out.push(JSON.parse(JSON.stringify(recorded)));
    }
    disconnect();
    writeFileSync(join(tsDir, name), JSON.stringify(out, null, 2));
  }
  Date.now = realNow;
}

// ------------------------------------------------------------------------ compare

const MUTATIONS: Record<string, (steps: Json[]) => Json[]> = {
  'drop-row': (steps) => {
    for (const effects of steps)
      for (const effect of effects)
        if (effect.update?.kind === 'snapshot' && effect.update.groups?.[0]?.rows?.length) {
          effect.update.groups[0].rows.shift();
          return steps;
        }
    return steps;
  },
  'wrong-post': (steps) => {
    for (const effects of steps)
      for (const effect of effects)
        if (effect.post?.type) {
          effect.post.type = `${effect.post.type}X`;
          return steps;
        }
    return steps;
  },
  selection: (steps) => {
    for (const effects of steps)
      for (const effect of effects)
        if (effect.update?.kind === 'snapshot' && effect.update.selectedKey) {
          effect.update.selectedKey = 'nothing';
          return steps;
        }
    return steps;
  },
  'hotkey-label': (steps) => {
    for (const effects of steps)
      for (const effect of effects)
        if (effect.update?.kind === 'snapshot') {
          effect.update.tabs[0].hotkey = 'X';
          return steps;
        }
    return steps;
  },
};

function diff(left: Json, right: Json, path: string, out: string[]): void {
  if (out.length > 40) return;
  if (
    typeof left !== typeof right ||
    Array.isArray(left) !== Array.isArray(right) ||
    (left === null) !== (right === null)
  ) {
    out.push(`${path}: ${JSON.stringify(left)?.slice(0, 160)} != ${JSON.stringify(right)?.slice(0, 160)}`);
    return;
  }
  if (Array.isArray(left)) {
    if (left.length !== right.length) out.push(`${path}: length ${left.length} != ${right.length}`);
    for (let index = 0; index < Math.min(left.length, right.length); index++)
      diff(left[index], right[index], `${path}[${index}]`, out);
    return;
  }
  if (left && typeof left === 'object') {
    for (const key of new Set([...Object.keys(left), ...Object.keys(right)])) {
      diff(left[key], right[key], `${path}.${key}`, out);
    }
    return;
  }
  if (left !== right)
    out.push(`${path}: ${JSON.stringify(left)?.slice(0, 160)} != ${JSON.stringify(right)?.slice(0, 160)}`);
}

async function main(): Promise<void> {
  if (args[0] === '__typescript') {
    await runTypeScript(args[1], args[2]);
    return;
  }
  const keep = option('--keep');
  const inject = option('--inject');
  if (inject && !MUTATIONS[inject]) {
    console.error(`unknown mutation ${inject}; one of ${Object.keys(MUTATIONS).join(', ')}`);
    process.exit(2);
  }
  const dir = keep ?? mkdtempSync(join(tmpdir(), 'f6-quick-access-'));
  mkdirSync(dir, { recursive: true });
  try {
    await buildScenarios(dir);
    for (const platform of ['mac', 'windows']) {
      const child = spawnSync('bun', [fileURLToPath(import.meta.url), '__typescript', dir, platform], {
        cwd: root,
        env: { ...process.env, TZ: 'UTC' },
        stdio: 'inherit',
      });
      if (child.status !== 0) throw new Error(`TypeScript half failed for ${platform}`);
    }
    const rust = spawnSync('cargo', ['run', '--quiet', '--example', 'quick_access_parity', '--', dir], {
      cwd: join(root, 'packages/gx-core'),
      stdio: 'inherit',
    });
    if (rust.status !== 0) throw new Error('Rust half failed');
    let differences = 0;
    let steps = 0;
    let effects = 0;
    for (const name of readdirSync(join(dir, 'scenarios')).sort()) {
      const ts = JSON.parse(readFileSync(join(dir, 'ts', name), 'utf8'));
      let rs = JSON.parse(readFileSync(join(dir, 'rust', name), 'utf8'));
      if (inject) rs = MUTATIONS[inject](rs);
      steps += ts.length;
      effects += ts.reduce((sum: number, step: Json[]) => sum + step.length, 0);
      const found: string[] = [];
      diff(ts, rs, name, found);
      if (found.length > 0) {
        differences += found.length;
        console.log(`${name}: ${found.length} difference(s)`);
        for (const line of found.slice(0, 12)) console.log(`  ${line}`);
      }
    }
    console.log(
      `${readdirSync(join(dir, 'scenarios')).length} scenarios, ${steps} steps, ${effects} effects, ${differences} differences`
    );
    process.exitCode = differences === 0 ? 0 : 1;
  } finally {
    if (!keep) rmSync(dir, { force: true, recursive: true });
  }
}

await main();
