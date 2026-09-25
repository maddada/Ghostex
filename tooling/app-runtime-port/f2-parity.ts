/**
 * The app runtime port's F2 fixture gate: the pure logic family F2 moved from the QuickJS runtime
 * into gx-core, run on the same inputs on both sides, diffed as JSON. Zero differences is the bar.
 *
 *   bun tooling/app-runtime-port/f2-parity.ts [--inject <mutation>] [--keep <dir>]
 *
 * The TypeScript half calls the functions the runtime called (or, where the runtime file is
 * deleted, the shared function it called plus the few lines of shaping it did, quoted below); the
 * Rust half is `packages/gx-core/examples/f2_parity.rs`. `--inject` mutates the Rust answer after
 * the fact and expects the gate to report differences, which proves it can fail.
 *
 * Mutations: feed-drop-item, feed-unread-count, feed-jump, attention-sound, attention-report,
 * attention-visible, hud-settings, hud-recent, hud-scopes, indicators-count, indicators-order, pet.
 *
 * `hud` builds the sidebar HUD from the same sources on both sides: the runtime's deleted
 * `createGpuiSidebarHudState`, extracted from git at `HUD_BASE` (with the groups the runtime would have built from the same
 * presentations) normalized the way the sidebar store normalized it, against gx-core
 * `compose_sidebar_hud`. It compares the fields the HUD's Rust contract names (gx-core
 * `hud/mod.rs`), and of `settings` the keys `hud/settings.rs` normalizes.
 *
 * `indicators` builds the menu bar status payload and the pet overlay payload from the groups the
 * runtime would have built (this computer's, then each saved remote machine's) against gx-core
 * `indicators`, which reads one neutral sidebar view per machine.
 *
 * `attention` replays one scripted timeline (snapshots, deltas, acknowledgements, Escape, timer
 * ticks, local and remote) through the runtime's deleted tracker, extracted from git at
 * `ATTENTION_BASE`, and through gx-core's `Core`, and compares what each reported to a daemon,
 * which completion sounds it played, and the activity every row shows after each step.
 *
 * The runtime was deleted in step 3 (docs/2026-09-25/app-runtime-port/PLAN.md); what this gate reads
 * of it as it last was comes out of git at `FROZEN_RUNTIME_REVISION` (frozen-runtime.ts).
 */
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { frozenRuntimeRoot } from './frozen-runtime';
import { normalizeNotificationFeedState } from '@/packages/shared/notification-feed/notification-feed-contract';

type Json = any;

const args = process.argv.slice(2);
const option = (name: string) => {
  const index = args.indexOf(name);
  return index >= 0 ? args[index + 1] : undefined;
};
const inject = option('--inject');
const keep = option('--keep');
const root = fileURLToPath(new URL('../../', import.meta.url));
const frozen = frozenRuntimeRoot();
const { reduceGxserverPresentationDelta } = await import(`${frozen}/packages/shared/gxserver-presentation-cache`);

// ---------------------------------------------------------------- notification feed

const row = (id: string, extra: Record<string, unknown> = {}) => ({
  agentName: 'claude',
  body: `body ${id}`,
  createdAt: '2026-09-25T10:00:00.000Z',
  id,
  kind: 'finished',
  projectId: 'P1',
  read: false,
  sessionId: `S-${id}`,
  subtitle: 'Project',
  title: `Title ${id}`,
  ...extra,
});

function notificationFeedFixtures(): Json[] {
  return [
    { name: 'empty object', result: {}, deferred: null },
    { name: 'not an object', result: 'nope', deferred: null },
    { name: 'null', result: null, deferred: null },
    { name: 'array', result: [row('a')], deferred: null },
    {
      name: 'plain feed',
      result: { items: [row('a'), row('b', { read: true })], nextUnreadId: 'a', unreadCount: 1 },
      deferred: null,
    },
    {
      name: 'derived unread count',
      result: { items: [row('a'), row('b'), row('c', { read: true })] },
      deferred: null,
    },
    {
      name: 'bad unread counts',
      result: { items: [row('a')], unreadCount: -3, nextUnreadId: 'a' },
      deferred: null,
    },
    { name: 'fractional unread', result: { items: [row('a')], unreadCount: 2.7 }, deferred: null },
    { name: 'string unread', result: { items: [row('a')], unreadCount: '4' }, deferred: null },
    {
      name: 'rows dropped',
      result: {
        items: [
          row('a', { id: '' }),
          row('b', { projectId: 5 }),
          row('c', { kind: 'other' }),
          row('d', { createdAt: '' }),
          row('e', { sessionId: undefined }),
          'text',
          null,
          row('f', { agentName: '', body: 3, subtitle: null, title: undefined, read: 'true' }),
          row('g', { kind: 'needsInput' }),
          row('h', { kind: 'bell' }),
          row('i', { kind: 'custom', extra: 'ignored' }),
        ],
        nextUnreadId: 'g',
      },
      deferred: null,
    },
    { name: 'empty next id', result: { items: [row('a')], nextUnreadId: '' }, deferred: null },
    { name: 'next id not listed', result: { items: [row('a')], nextUnreadId: 'zz' }, deferred: null },
    { name: 'defer same session', result: { items: [row('a')], nextUnreadId: 'a' }, deferred: 'S-a' },
    { name: 'defer other session', result: { items: [row('a')], nextUnreadId: 'a' }, deferred: 'S-b' },
    {
      name: 'unsafe id passes normalize',
      result: { items: [row('a b')], nextUnreadId: 'a b' },
      deferred: null,
    },
  ];
}

/**
 * The deleted runtime's `postNotificationFeedState` message and its jump choice
 * (apps/desktop/sidebar/gxserver-runtime/notification-feed.ts, deleted 2026-09-25):
 *
 *   const message = { items: state.items, ...(state.nextUnreadId ? { nextUnreadId } : {}),
 *                     type: 'notificationFeedState', unreadCount: state.unreadCount };
 *   const item = state.items.find((entry) => entry.id === state.nextUnreadId);
 *   if (!item || item.sessionId === sessionId) return;
 */
function notificationFeedTypescript(cases: Json[]): Json[] {
  return cases.map((fixture) => {
    const state = normalizeNotificationFeedState(fixture.result);
    const message = {
      items: state.items,
      ...(state.nextUnreadId ? { nextUnreadId: state.nextUnreadId } : {}),
      type: 'notificationFeedState',
      unreadCount: state.unreadCount,
    };
    const item = state.nextUnreadId ? state.items.find((entry) => entry.id === state.nextUnreadId) : undefined;
    const deferred = fixture.deferred ?? undefined;
    const jump = !item || (deferred !== undefined && item.sessionId === deferred) ? null : item.id;
    return { jump, message, name: fixture.name };
  });
}

// ---------------------------------------------------------------- attention

/** The last commit whose runtime still had its own attention tracker. */
const ATTENTION_BASE = '11d3fe9a8';
const RUNTIME = 'apps/desktop/sidebar/gxserver-runtime';

function attentionRow(projectId: string, sessionId: string, activity: string, eventId?: string, extra: Json = {}) {
  return {
    activity,
    alias: `Session ${sessionId}`,
    createdAt: '2026-09-01T01:00:00.000Z',
    groupId: `${projectId}:active`,
    isPinned: false,
    kind: 'agent',
    lastInteractionAt: '2026-09-15T01:18:43.055Z',
    lifecycleState: 'running',
    pendingQuestionCount: 0,
    projectId,
    sessionId,
    sortKey: `000${sessionId}`,
    surface: 'workspace',
    title: `Session ${sessionId}`,
    updatedAt: '2026-09-15T01:18:43.055Z',
    visibleInSidebarByDefault: true,
    zmxName: `S90-${projectId}-${sessionId}`,
    ...(activity === 'attention'
      ? { attention: { acknowledged: false, enteredAt: '2026-09-25T00:00:00.000Z', ...(eventId ? { eventId } : {}) } }
      : {}),
    ...extra,
  };
}

function attentionSnapshot(serverId: string, revision: number, sessions: Json[]) {
  const projects = [...new Set(sessions.map((session) => session.projectId))];
  return {
    protocolVersion: 1,
    revision,
    serverId,
    snapshot: {
      generatedAt: '2026-09-25T00:00:00.000Z',
      groups: projects.map((projectId) => ({
        groupId: `${projectId}:active`,
        projectId,
        sessionIds: sessions.filter((session) => session.projectId === projectId).map((session) => session.sessionId),
        sortKey: `1:${projectId}:active`,
        title: 'Active',
      })),
      projects: projects.map((projectId) => ({
        createdAt: '2026-06-29T13:10:42.091Z',
        groupIds: [`${projectId}:active`],
        path: `/tmp/${projectId}`,
        pathState: 'available',
        projectId,
        sortKey: `1:${projectId}`,
        title: projectId,
        updatedAt: '2026-09-15T01:18:43.055Z',
      })),
      revision,
      sessions,
    },
    type: 'presentationSnapshot',
  };
}

function attentionDelta(serverId: string, revision: number, session: Json) {
  return {
    delta: { session, type: 'sessionPresentationChanged' },
    protocolVersion: 1,
    revision,
    serverId,
    type: 'presentationDelta',
  };
}

/** One timeline that walks every rule of the tracker. Times are milliseconds. */
function attentionFixtures(): Json[] {
  const L = 'local';
  const R = 'm1';
  const s1 = (activity: string, eventId?: string) =>
    attentionRow('P1', 'S1', activity, eventId, { agentName: 'claude' });
  const s3 = (activity: string, eventId?: string) => attentionRow('P2', 'S3', activity, eventId);
  const local = (sessionId: string, projectId: string) =>
    `combined-session:${encodeURIComponent(projectId)}:${encodeURIComponent(sessionId)}`;
  return [
    {
      frame: attentionSnapshot('local-1', 1, [s1('idle'), attentionRow('P1', 'S2', 'attention', 'e0'), s3('working')]),
      machine: L,
      op: 'frame',
      t: 0,
    },
    {
      frame: attentionSnapshot('remote-1', 1, [attentionRow('P9', 'S9', 'working', undefined, { agentName: ' ' })]),
      machine: R,
      op: 'frame',
      t: 0,
    },
    { frame: attentionDelta('local-1', 2, s1('attention', 'e1')), machine: L, op: 'frame', t: 100 },
    { op: 'ack', sessionId: local('S1', 'P1'), t: 500 },
    { op: 'ack', sessionId: local('S1', 'P1'), t: 600 },
    { op: 'tick', t: 1599 },
    { op: 'tick', t: 1600 },
    { frame: attentionDelta('local-1', 3, s1('attention', 'e1')), machine: L, op: 'frame', t: 1700 },
    { frame: attentionDelta('local-1', 4, s1('idle')), machine: L, op: 'frame', t: 1800 },
    { frame: attentionDelta('local-1', 5, s1('attention', 'e2')), machine: L, op: 'frame', t: 2000 },
    { op: 'escape', projectId: 'P1', sessionId: 'S1', t: 2100 },
    { frame: attentionDelta('local-1', 6, s1('attention', 'e3')), machine: L, op: 'frame', t: 2200 },
    { frame: attentionDelta('local-1', 7, s3('attention', 'e4')), machine: L, op: 'frame', t: 8000 },
    { op: 'ack', sessionId: local('S3', 'P2'), t: 8000 },
    { frame: attentionDelta('local-1', 8, s3('attention', 'e5')), machine: L, op: 'frame', t: 8200 },
    { op: 'tick', t: 9500 },
    { op: 'ack', sessionId: local('S3', 'P2'), t: 9600 },
    { op: 'tick', t: 9700 },
    {
      frame: attentionDelta('remote-1', 2, attentionRow('P9', 'S9', 'attention', 'e9')),
      machine: R,
      op: 'frame',
      t: 10000,
    },
    { op: 'ack', sessionId: 'remote:m1:session:P9:S9', t: 10000 },
    { op: 'tick', t: 11500 },
    { op: 'ack', sessionId: local('S2', 'P1'), t: 12000 },
    { op: 'escape', projectId: 'P2', sessionId: 'S3', t: 13000 },
    { frame: attentionDelta('local-1', 9, s3('idle')), machine: L, op: 'frame', t: 13050 },
    { frame: attentionDelta('local-1', 10, s3('attention', 'e6')), machine: L, op: 'frame', t: 13100 },
    {
      frame: attentionDelta('local-1', 11, attentionRow('P2', 'S4', 'attention', 'e7')),
      machine: L,
      op: 'frame',
      t: 14000,
    },
    { frame: attentionDelta('local-1', 12, s1('idle')), machine: L, op: 'frame', t: 15000 },
    { frame: attentionDelta('local-1', 13, s1('attention', 'e2')), machine: L, op: 'frame', t: 16000 },
    {
      frame: attentionDelta('local-1', 14, attentionRow('P2', 'S3', 'attention', undefined)),
      machine: L,
      op: 'frame',
      t: 19000,
    },
    { frame: attentionDelta('local-1', 14, s1('idle')), machine: L, op: 'frame', t: 19100 },
    { op: 'ack', sessionId: local('S1', 'P1'), t: 19200 },
    { op: 'escape', projectId: 'P9', sessionId: 'S404', t: 19300 },
  ];
}

/** Copies the deleted tracker (and the two files it needs as they were) out of git into `dir`. */
async function loadOldAttention(dir: string): Promise<Json> {
  const out = join(dir, 'old');
  mkdirSync(join(out, 'helpers'), { recursive: true });
  const current = `${frozen}/${RUNTIME}`;
  const read = (path: string) =>
    execFileSync('git', ['show', `${ATTENTION_BASE}:${RUNTIME}/${path}`], { cwd: root, encoding: 'utf8' }).replaceAll(
      "'@/",
      `'${root}`
    );
  writeFileSync(join(out, 'constants.ts'), read('constants.ts').replaceAll('"@/', `"${root}`));
  writeFileSync(
    join(out, 'helpers/attention.ts'),
    read('helpers/attention.ts')
      .replaceAll("'../types-and-protocol'", `'${current}/types-and-protocol'`)
      .replaceAll("'./records'", `'${current}/helpers/records'`)
      .replaceAll("'./remote-presentation'", `'${current}/helpers/remote-presentation'`)
  );
  // Deleted from the runtime by the sweep, so it comes out of git with the tracker.
  writeFileSync(
    join(out, 'helpers/terminal-lifecycle.ts'),
    read('helpers/terminal-lifecycle.ts')
      .replaceAll("'../constants'", `'${out}/constants'`)
      .replaceAll("'../types-and-protocol'", `'${current}/types-and-protocol'`)
      .replace(/'\.\/(records|remote-presentation|status-indicators)'/g, `'${current}/helpers/$1'`)
  );
  writeFileSync(
    join(out, 'attention-tracking.ts'),
    read('attention-tracking.ts')
      .replaceAll("'./core'", `'${current}/core'`)
      .replaceAll("'./types-and-protocol'", `'${current}/types-and-protocol'`)
      .replaceAll("'./helpers/terminal-lifecycle'", `'${out}/helpers/terminal-lifecycle'`)
      .replace(/'\.\/helpers\/(bootstrap|records|remote-presentation)'/g, `'${current}/helpers/$1'`)
  );
  return (await import(join(out, 'attention-tracking.ts'))).gpuiSidebarRuntimeAttentionMethods;
}

async function attentionTypescript(dir: string, steps: Json[]): Promise<Json[]> {
  let now = 0;
  const timers = new Map<number, { due: number; run: () => void }>();
  let nextTimer = 1;
  const rpcs: Json[] = [];
  const sounds: string[] = [];
  const g = globalThis as Json;
  g.window = {
    clearTimeout: (id: number) => timers.delete(id),
    ghostexGpui: {
      postSessionCompletionSound: (payload: string) => sounds.push(JSON.parse(payload).sessionId),
    },
    setTimeout: (run: () => void, delay: number) => {
      const id = nextTimer++;
      timers.set(id, { due: now + delay, run });
      return id;
    },
  };
  const realNow = Date.now;
  Date.now = () => now;
  const methods = await loadOldAttention(dir);
  const runtime: Json = {
    attentionAcknowledgementTimeoutsBySessionKey: new Map(),
    attentionCompletionSoundEventKeyOrder: [],
    attentionCompletionSoundEventKeys: new Set(),
    attentionCompletionSoundSuppressedUntilBySessionKey: new Map(),
    attentionEnteredAtBySessionKey: new Map(),
    attentionEventIdBySessionKey: new Map(),
    client: {
      rpc: async (path: string, params: Json) => {
        rpcs.push({ machine: 'local', path, ...params });
      },
    },
    findLocalPresentationSession(projectId: string, sessionId: string) {
      return this.presentation?.sessions.find(
        (session: Json) => session.projectId === projectId && session.sessionId === sessionId
      );
    },
    findRemotePresentationSession(reference: Json) {
      return this.remotePresentations
        .get(reference.machineId)
        ?.sessions.find(
          (session: Json) => session.projectId === reference.projectId && session.sessionId === reference.sessionId
        );
    },
    locallyAcknowledgedAttentionEventKeyOrder: [],
    locallyAcknowledgedAttentionEventKeys: new Set(),
    messageSource: { postMessage() {} },
    presentation: undefined,
    publishPresentation() {},
    publishRemotePresentationPatch() {},
    remotePresentations: new Map(),
    requestRemoteGxserver: async (machineId: string, path: string, params: Json) => {
      rpcs.push({ machine: machineId, path, ...params });
    },
    runtimeSettings: { settings: {} },
    ...methods,
  };
  const results: Json[] = [];
  try {
    for (const step of steps) {
      now = step.t;
      rpcs.length = 0;
      sounds.length = 0;
      if (step.op === 'frame' && step.machine === 'local') {
        const previousSessions = runtime.presentation?.sessions ?? [];
        if (step.frame.type === 'presentationSnapshot') {
          const projected = runtime.projectLocalPresentationAttentionAcknowledgementGuards(step.frame.snapshot);
          runtime.presentation = projected;
          runtime.syncLocalPresentationAttentionTracking(previousSessions, projected.sessions);
        } else if (runtime.presentation && step.frame.revision > runtime.presentation.revision) {
          const projected = runtime.projectLocalPresentationAttentionAcknowledgementGuards(
            reduceGxserverPresentationDelta(runtime.presentation, step.frame.delta, step.frame.revision)
          );
          runtime.presentation = projected;
          runtime.syncLocalPresentationAttentionTracking(previousSessions, projected.sessions);
          runtime.detectSessionAttentionCompletionSounds(previousSessions, projected.sessions);
        }
      } else if (step.op === 'frame') {
        const previous = runtime.remotePresentations.get(step.machine);
        if (step.frame.type === 'presentationSnapshot') {
          const snapshot = runtime.projectRemotePresentationAttentionAcknowledgementGuards(
            step.machine,
            step.frame.snapshot
          );
          runtime.remotePresentations.set(step.machine, snapshot);
          runtime.syncRemotePresentationAttentionTracking(step.machine, previous?.sessions ?? [], snapshot.sessions);
        } else if (previous && step.frame.revision > previous.revision) {
          const snapshot = runtime.projectRemotePresentationAttentionAcknowledgementGuards(
            step.machine,
            reduceGxserverPresentationDelta(previous, step.frame.delta, step.frame.revision)
          );
          runtime.remotePresentations.set(step.machine, snapshot);
          runtime.syncRemotePresentationAttentionTracking(step.machine, previous.sessions, snapshot.sessions);
        }
      } else if (step.op === 'ack') {
        runtime.acknowledgeSessionAttention(step.sessionId, 'native-focus');
      } else if (step.op === 'escape') {
        runtime.handleGpuiWorkspaceTerminalEscapePressed({
          projectId: step.projectId,
          sessionId: step.sessionId,
          type: 'ghostex.gpui.sidebar.workspaceTerminalEscapePressed',
          version: 1,
        });
      } else if (step.op === 'tick') {
        const due = [...timers.entries()].filter(([, timer]) => timer.due <= now).sort((a, b) => a[1].due - b[1].due);
        for (const [id, timer] of due) {
          if (!timers.has(id)) continue;
          timers.delete(id);
          timer.run();
        }
      }
      const visible: Record<string, string> = {};
      for (const session of runtime.presentation?.sessions ?? []) {
        visible[`combined-session:${encodeURIComponent(session.projectId)}:${encodeURIComponent(session.sessionId)}`] =
          session.activity;
      }
      for (const [machineId, snapshot] of runtime.remotePresentations) {
        for (const session of snapshot.sessions) {
          visible[`remote:${machineId}:session:${session.projectId}:${session.sessionId}`] = session.activity;
        }
      }
      results.push({
        rpcs: rpcs.map((rpc) => ({ ...rpc })),
        sounds: [...sounds],
        t: step.t,
        visible,
      });
    }
  } finally {
    Date.now = realNow;
  }
  return results;
}

// ---------------------------------------------------------------- HUD

const HUD_FIELDS = [
  'activeProjectId',
  'activeProjectSpaceRefs',
  'activeSessionsSortMode',
  'agentManagerZoomPercent',
  'agents',
  'commandsByProject',
  'createSessionOnSidebarDoubleClick',
  'debuggingMode',
  'globalCommands',
  'projectViewProjects',
  'projectViewSpaces',
  'recentProjects',
  'renameSessionOnDoubleClick',
  'theme',
];
const HUD_SETTINGS_KEYS = [
  'sidebarTheme',
  'sidebarTooltipDelayMs',
  'sidebarCollapseAnimationDurationMs',
  'showProjectIcons',
  'hideProjectHeaderDiffStats',
  'showProjectEditorDiffFileCount',
  'renameSessionOnDoubleClick',
  'createSessionOnSidebarDoubleClick',
  'hideBrowserFaviconUntilHover',
  'hideSessionAgentIconUntilHover',
  'sidebarSessionCycleSkipsSleeping',
  'agentManagerZoomPercent',
  'defaultPromptAgentId',
  'debuggingMode',
  'showBetaFeatures',
];

function hudProject(projectId: string, extra: Json = {}) {
  return {
    createdAt: '2026-06-29T13:10:42.091Z',
    groupIds: [`${projectId}:active`],
    path: `/tmp/${projectId}`,
    pathState: 'available',
    projectId,
    sortKey: `1:${projectId}`,
    title: `Project ${projectId}`,
    updatedAt: '2026-09-15T01:18:43.055Z',
    ...extra,
  };
}

function hudSnapshot(serverId: string, projects: Json[], sessions: Json[], docs: Json = {}) {
  return {
    protocolVersion: 1,
    revision: 1,
    serverId,
    snapshot: {
      generatedAt: '2026-09-25T00:00:00.000Z',
      groups: projects.map((project) => ({
        groupId: `${project.projectId}:active`,
        projectId: project.projectId,
        sessionIds: sessions.filter((session) => session.projectId === project.projectId).map((s) => s.sessionId),
        sortKey: `1:${project.projectId}:active`,
        title: 'Active',
      })),
      projects,
      revision: 1,
      sessions,
      ...docs,
    },
    type: 'presentationSnapshot',
  };
}

function hudFixtures(): Json[] {
  const localProjects = [
    hudProject('P1'),
    hudProject('P2', { title: '  Spaced title  ' }),
    hudProject('P3', { worktree: { parentProjectId: 'P1', branch: 'feature' }, title: 'P1 worktree' }),
    hudProject('P4', { title: 'Parked' }),
    hudProject('P5', { path: '/Users/x/.ghostex/chats/P5', title: 'Chat' }),
    hudProject('P6', { title: 'Collected' }),
  ];
  const localSessions = [
    attentionRow('P1', 'S1', 'working'),
    attentionRow('P2', 'S2', 'idle'),
    attentionRow('P6', 'S6', 'idle'),
  ];
  const localDocs = {
    sidebarProjectCollections: {
      collections: { 'col-1': { collectionId: 'col-1', color: '#112233', projectIds: ['P6'], title: 'Group' } },
      nextCollectionNumber: 2,
      order: ['col-1'],
    },
    sidebarSpaces: {
      order: ['space-b', 'space-a', 'space-c'],
      spaces: {
        'space-a': { memberCollectionIds: [], memberProjectIds: ['P1'], name: '  Alpha ', spaceId: 'space-a' },
        'space-b': { memberCollectionIds: ['col-1'], memberProjectIds: ['P2'], name: 'Beta', spaceId: 'space-b' },
        'space-c': { memberCollectionIds: [], memberProjectIds: ['P1', 'P2'], name: '   ', spaceId: 'space-c' },
      },
    },
    workspaceGroups: { projectOrder: ['P6', 'P2', 'P1'], projects: {} },
  };
  const remoteProjects = [hudProject('R1', { title: 'Remote one' }), hudProject('R2', { title: 'Remote two' })];
  const remoteSessions = [
    attentionRow('R1', 'RS1', 'idle'),
    attentionRow('R1', 'RS2', 'idle', undefined, { visibleInSidebarByDefault: false }),
    attentionRow('R2', 'RS3', 'idle', undefined, { surface: 'commands' }),
  ];
  const remoteDocs = {
    sidebarSpaces: {
      order: ['rs-1'],
      spaces: { 'rs-1': { memberCollectionIds: [], memberProjectIds: ['R1'], name: 'Remote space', spaceId: 'rs-1' } },
    },
  };
  const settingsVariants: Json[] = [
    {},
    {
      agentManagerZoomPercent: 137.5,
      createSessionOnSidebarDoubleClick: true,
      defaultPromptAgentId: '   ',
      hideBrowserFaviconUntilHover: 'yes',
      remoteMachines: [{ id: 'remote-m1', name: 'Studio', sshHost: 'studio.local' }],
      renameSessionOnDoubleClick: true,
      showProjectIcons: false,
      sidebarCollapseAnimationDurationMs: 1450,
      sidebarSessionCycleSkipsSleeping: true,
      sidebarTheme: 'plain-light',
      sidebarTooltipDelayMs: 649,
      unknownKey: 'kept',
    },
    {
      agentManagerZoomPercent: 12,
      defaultPromptAgentId: ` ${'x'.repeat(130)} `,
      hideProjectHeaderDiffStats: true,
      remoteMachines: [
        { id: 'remote-m1', name: 'Studio', sshHost: 'studio.local' },
        { id: 'remote-m2', name: 'Off', sshHost: 'off.local' },
      ],
      showProjectEditorDiffFileCount: true,
      sidebarCollapseAnimationDurationMs: -5,
      sidebarTheme: 'dark-blue',
      sidebarTooltipDelayMs: 50,
    },
    { sidebarTheme: 42, sidebarTooltipDelayMs: 'slow', agentManagerZoomPercent: 250.5 },
  ];
  const sidebarHud = {
    agents: [{ agentId: 'codex', command: 'codex', icon: 'codex', isDefault: true, name: 'Codex' }],
    commands: [{ actionType: 'terminal', commandId: 'c1', name: 'Build', showOnProjectRow: true }],
    commandsByProject: {
      P1: [{ actionType: 'terminal', commandId: 'c1', name: 'Build' }],
      P2: [{ actionType: 'browser', commandId: 'c2', name: 'Open', showOnProjectRow: 'no' }],
    },
    globalCommands: [{ actionType: 'terminal', commandId: 'g1', name: 'Global', showOnProjectRow: true }],
  };
  const remoteHud = {
    agents: [],
    commands: [],
    commandsByProject: { R1: [{ actionType: 'terminal', commandId: 'rc', name: 'Remote build' }] },
    globalCommands: [{ commandId: 'rg', name: 'Remote global' }],
  };
  const recentProjects = [
    {
      path: '/tmp/P4/',
      projectId: 'P4',
      recentClosedAt: '2026-09-20T10:00:00.000Z',
      sessionCount: 3.7,
      title: ' Parked ',
    },
    {
      icon: { color: '#AABBCC', icon: 'rocket', kind: 'tabler' },
      path: '/tmp/P9',
      projectId: 'P9',
      recentClosedAt: '2026-09-22T10:00:00Z',
      sessionCount: -2,
      theme: 'plain-dark',
      themeColor: '#ABCDEF',
      title: 'Nine',
    },
    {
      icon: { dataUrl: 'data:image/png;base64,AAAA', kind: 'image' },
      iconDataUrl: 'data:image/jpeg;base64,AAAA',
      path: '/tmp/P10',
      projectId: 'P10',
      theme: 'neon',
      title: 'Ten',
    },
    {
      icon: { icon: 'notAnIcon', kind: 'tabler' },
      path: '/tmp/P11',
      projectId: 'P11',
      recentClosedAt: '   ',
      title: 'Eleven',
    },
    { path: '   ', projectId: 'P12', title: 'No path' },
    { path: '/tmp/P13', projectId: '', title: 'No id' },
    { path: '/tmp/P14', projectId: 'P14', recentClosedAt: '2026-09-22T10:00:00.000Z', title: 'Tie' },
  ];
  const remoteRecents = [
    [
      'remote-m1',
      [
        {
          path: '/tmp/R2-stored',
          projectId: 'R2',
          recentClosedAt: '2026-09-23T00:00:00.000Z',
          sessionCount: 9,
          title: 'Stored',
        },
        { path: '/tmp/R9', projectId: 'R9', recentClosedAt: '2026-09-24T00:00:00.000Z', title: 'Gone' },
      ],
    ],
    [
      'remote-m2',
      [
        {
          path: '/tmp/X',
          projectId: 'X1',
          recentClosedAt: '2026-09-21T00:00:00.000Z',
          sessionCount: 4,
          title: 'Offline',
        },
      ],
    ],
    ['remote-m9', [{ path: '/tmp/Y', projectId: 'Y1', title: 'Unsaved machine' }]],
  ];
  const domainProjects = [{ projectId: 'P3', worktree: { parentProjectId: 'P1' } }];
  const cases: Json[] = [];
  const bases = [
    { active: undefined, name: 'before any read', read: false },
    { active: 'P1', name: 'local active in spaces', read: true },
    { active: 'P6', name: 'collected project', read: true },
    { active: 'P3', name: 'worktree inherits parent', read: true },
    { active: 'remote:remote-m1:project:R1', name: 'remote active', read: true },
  ];
  for (const base of bases) {
    for (const [index, settings] of settingsVariants.entries()) {
      cases.push({
        activeProjectId: base.active,
        debuggingMode: index === 1,
        domainProjects,
        local: hudSnapshot('local-1', localProjects, localSessions, localDocs),
        name: `${base.name} / settings ${index}`,
        recentProjects: base.read ? recentProjects : [],
        remote: hudSnapshot('remote-1', remoteProjects, remoteSessions, remoteDocs),
        remoteHud: base.read ? remoteHud : undefined,
        remoteRecents,
        settings,
        showBetaFeatures: index === 2,
        sidebarHud: base.read ? sidebarHud : undefined,
      });
    }
  }
  return cases;
}

function hudContract(hud: Json): Json {
  const out: Json = {};
  for (const field of HUD_FIELDS) {
    if (hud[field] !== undefined) out[field] = hud[field];
  }
  if (Array.isArray(out.activeProjectSpaceRefs)) {
    // The runtime listed them in the spaces object's insertion order, the store in id order.
    out.activeProjectSpaceRefs = [...out.activeProjectSpaceRefs].sort((a: Json, b: Json) =>
      a.spaceId.localeCompare(b.spaceId)
    );
  }
  out.settings = Object.fromEntries(HUD_SETTINGS_KEYS.map((key) => [key, hud.settings?.[key]]));
  return out;
}

/** The last commit whose runtime still composed its own HUD. */
const HUD_BASE = '3062a6481';

/** Copies the runtime's helpers, constants and types as they were at `HUD_BASE` out of git. */
function extractOldRuntimeHelpers(dir: string): string {
  const out = join(dir, 'old-hud');
  mkdirSync(out, { recursive: true });
  const paths = [`${RUNTIME}/helpers`, `${RUNTIME}/constants.ts`, `${RUNTIME}/types-and-protocol.ts`];
  const archive = spawnSync('git', ['archive', HUD_BASE, ...paths], { cwd: root, maxBuffer: 1 << 28 });
  if (archive.status !== 0) throw new Error(`git archive ${HUD_BASE} failed`);
  const untar = spawnSync('tar', ['-x', '-C', out], { input: archive.stdout });
  if (untar.status !== 0) throw new Error('tar failed');
  const files = execFileSync('find', [join(out, RUNTIME), '-name', '*.ts'], { encoding: 'utf8' })
    .trim()
    .split('\n');
  for (const file of files) {
    writeFileSync(file, readFileSync(file, 'utf8').replaceAll("'@/", `'${root}`).replaceAll('"@/', `"${root}`));
  }
  return join(out, RUNTIME);
}

async function hudTypescript(dir: string, cases: Json[]): Promise<Json[]> {
  const old = extractOldRuntimeHelpers(dir);
  const { createGpuiSidebarHudState } = await import(`${old}/helpers/command-pane`);
  const { createGpuiPresentationProjectProjectionMetadata, resolveGpuiSidebarAgentIcon } = await import(
    `${old}/helpers/presentation-projection`
  );
  const { createGpuiRemotePresentationSidebarGroups } = await import(`${old}/helpers/remote-presentation`);
  const { createGpuiSidebarSettings } = await import(`${old}/helpers/bootstrap`);
  const { createGxserverPresentationSidebarGroups } =
    await import('@/packages/shared/gxserver-presentation-sidebar-projection');
  const { normalizeghostexSettings } = await import('@/packages/shared/ghostex-settings');
  return cases.map((fixture) => {
    const presentation = fixture.local.snapshot;
    const runtimeSettings = {
      debuggingMode: fixture.debuggingMode,
      settings: fixture.settings,
      showBetaFeatures: fixture.showBetaFeatures,
    };
    const settings = createGpuiSidebarSettings(runtimeSettings);
    const remotePresentations = new Map([['remote-m1', fixture.remote.snapshot]]);
    const remoteRecents = new Map(fixture.remoteRecents.map(([machine, rows]: Json) => [machine, rows]));
    const meta = createGpuiPresentationProjectProjectionMetadata({
      domainProjects: fixture.domainProjects,
      presentation,
      projectOrder: presentation.workspaceGroups?.projectOrder ?? [],
      recentProjects: fixture.recentProjects,
    });
    const groups = [
      ...createGxserverPresentationSidebarGroups({
        chatProjectIds: meta.chatProjectIds,
        hiddenProjectIds: meta.hiddenProjectIds,
        presentation,
        projectOverlays: meta.projectOverlays,
        resolveAgentIcon: resolveGpuiSidebarAgentIcon,
      }),
      ...createGpuiRemotePresentationSidebarGroups({
        presentationsByMachineId: remotePresentations,
        remoteRecentProjectsByMachineId: remoteRecents,
        resolveAgentIcon: resolveGpuiSidebarAgentIcon,
        settings,
      }),
    ];
    const hud = createGpuiSidebarHudState({
      activeProjectId: fixture.activeProjectId,
      commandPaneSessions: [],
      domainProjects: fixture.domainProjects,
      groups,
      presentation,
      recentProjects: fixture.recentProjects,
      remotePresentationsByMachineId: remotePresentations,
      remoteRecentProjectsByMachineId: remoteRecents,
      remoteSidebarHudsByMachineId: fixture.remoteHud ? new Map([['remote-m1', fixture.remoteHud]]) : new Map(),
      runtimeSettings,
      sidebarHud: fixture.sidebarHud,
    });
    // `normalizeHydratedSidebarHud` (packages/core-ui/sidebar-store-model.ts), then a JSON round
    // trip, which is what the facts channel carried.
    const stored = JSON.parse(
      JSON.stringify({
        ...hud,
        projectSettingsProjects: hud.projectSettingsProjects ?? [],
        recentProjects: hud.recentProjects ?? [],
        settings: { ...hud.settings, ...normalizeghostexSettings(hud.settings) },
      })
    );
    return { hud: hudContract(stored), name: fixture.name };
  });
}

// ---------------------------------------------------------------- status indicators and pet

function indicatorRow(projectId: string, sessionId: string, extra: Json = {}) {
  return attentionRow(projectId, sessionId, extra.activity ?? 'idle', extra.eventId, {
    lastActiveAt: '2026-09-20T10:00:00.000Z',
    providerSessionState: 'exists',
    sessionPersistenceProvider: 'zmx',
    ...extra,
  });
}

function indicatorFixtures(): Json[] {
  const local = [
    hudProject('P1', { title: 'Alpha' }),
    hudProject('P2', { title: '   ' }),
    hudProject('P5', { path: '/Users/x/.ghostex/chats/P5', title: 'Chat' }),
    hudProject('P6', { title: 'Many' }),
  ];
  const sessions = [
    indicatorRow('P1', 'S1', {
      activity: 'attention',
      eventId: 'e1',
      meaningfulActivityAt: '2026-09-24T10:00:00.000Z',
    }),
    indicatorRow('P1', 'S2', { activity: 'working', title: 'Worker', displayTitle: '  Shown  ' }),
    indicatorRow('P1', 'S3', { primaryTitle: 'Primary', providerSessionState: 'missing' }),
    indicatorRow('P1', 'S4', { isPinned: true, terminalTitle: 'Term', title: '', primaryTitle: '  ' }),
    indicatorRow('P1', 'S7', { isParked: true, lastActiveAt: '2026-09-25T09:00:00.000Z' }),
    indicatorRow('P1', 'S8', { sessionPersistenceProvider: 'tmux', activity: 'working' }),
    indicatorRow('P1', 'S9', { kind: 'terminal', zmxName: '' }),
    indicatorRow('P2', 'S5', { activity: 'working', updatedAt: '2026-09-23T00:00:00.000Z' }),
    indicatorRow('P5', 'C1', { activity: 'attention', eventId: 'c1' }),
    indicatorRow('P5', 'C2', {}),
    ...Array.from({ length: 20 }, (_, index) =>
      indicatorRow('P6', `M${index}`, {
        activity: index % 3 === 0 ? 'working' : 'idle',
        lastActiveAt: `2026-09-${String(10 + index).padStart(2, '0')}T00:00:00.000Z`,
      })
    ),
  ];
  const remoteProjects = [
    hudProject('R1', { title: 'Remote' }),
    hudProject('R3', { path: '/Users/x/.ghostex/chats/R3' }),
  ];
  const remoteSessions = [
    indicatorRow('R1', 'RS1', { activity: 'attention', eventId: 'r1' }),
    indicatorRow('R1', 'RS2', {}),
    indicatorRow('R3', 'RC1', { activity: 'working' }),
  ];
  const many = Array.from({ length: 12 }, (_, project) => hudProject(`Q${project}`, { title: `Q ${project}` }));
  const manySessions = many.flatMap((project, projectIndex) =>
    Array.from({ length: 9 }, (_, index) =>
      indicatorRow(project.projectId, `${project.projectId}-${index}`, {
        activity: (projectIndex + index) % 4 === 0 ? 'attention' : 'idle',
        eventId: `ev-${projectIndex}-${index}`,
      })
    )
  );
  const settingsVariants: Json[] = [
    {},
    {
      enableSessionParking: false,
      hideMenuBarSessionStatusIndicators: true,
      petOverlayEnabled: true,
      remoteMachines: [{ id: 'remote-m1', name: 'Studio', sshHost: 'studio.local' }],
      selectedPetId: 'dewey',
    },
    { remoteMachines: [{ id: 'remote-m1', name: 'Studio', sshHost: 'studio.local' }], selectedPetId: 'cat' },
  ];
  const cases: Json[] = [];
  for (const [index, settings] of settingsVariants.entries()) {
    cases.push({
      local: hudSnapshot('local-1', local, sessions),
      name: `mixed / settings ${index}`,
      remote: hudSnapshot('remote-1', remoteProjects, remoteSessions),
      settings,
    });
  }
  cases.push({
    local: hudSnapshot('local-1', many, manySessions),
    name: 'caps',
    remote: hudSnapshot('remote-1', remoteProjects, remoteSessions),
    settings: settingsVariants[1],
  });
  cases.push({
    local: hudSnapshot(
      'local-1',
      [hudProject('P1')],
      [indicatorRow('P1', 'S1', {}), indicatorRow('P1', 'S2', { providerSessionState: 'missing' })]
    ),
    name: 'nothing actionable',
    remote: hudSnapshot('remote-1', [], []),
    settings: {},
  });
  return cases;
}

/** The last commit whose runtime still built the status item and pet payloads itself. */
const INDICATORS_BASE = '912fcb0af';

/** Copies the deleted payload builders (and the constants they read, as they were) out of git. */
async function loadOldIndicators(dir: string): Promise<Json> {
  const out = join(dir, 'old-indicators');
  mkdirSync(join(out, 'helpers'), { recursive: true });
  const current = `${frozen}/${RUNTIME}`;
  const read = (path: string) =>
    execFileSync('git', ['show', `${INDICATORS_BASE}:${RUNTIME}/${path}`], { cwd: root, encoding: 'utf8' })
      .replaceAll("'@/", `'${root}`)
      .replaceAll('"@/', `"${root}`);
  writeFileSync(join(out, 'constants.ts'), read('constants.ts'));
  writeFileSync(
    join(out, 'helpers/status-indicators.ts'),
    read('helpers/status-indicators.ts')
      .replaceAll("'../types-and-protocol'", `'${current}/types-and-protocol'`)
      .replaceAll("'./records'", `'${current}/helpers/records'`)
      .replaceAll("'./remote-presentation'", `'${current}/helpers/remote-presentation'`)
  );
  return import(join(out, 'helpers/status-indicators.ts'));
}

async function indicatorsTypescript(dir: string, cases: Json[]): Promise<Json[]> {
  const indicators = await loadOldIndicators(dir);
  const { createGpuiPresentationProjectProjectionMetadata, resolveGpuiSidebarAgentIcon } = await import(
    `${frozen}/${RUNTIME}/helpers/presentation-projection`
  );
  const { createGpuiRemotePresentationSidebarGroups } = await import(`${frozen}/${RUNTIME}/helpers/remote-presentation`);
  const { createGpuiSidebarSettings } = await import(`${frozen}/${RUNTIME}/helpers/bootstrap`);
  const { createGxserverPresentationSidebarGroups } =
    await import('@/packages/shared/gxserver-presentation-sidebar-projection');
  return cases.map((fixture) => {
    const presentation = fixture.local.snapshot;
    const settings = createGpuiSidebarSettings({ settings: fixture.settings });
    const meta = createGpuiPresentationProjectProjectionMetadata({
      domainProjects: [],
      presentation,
      projectOrder: [],
    });
    const groups = [
      ...createGxserverPresentationSidebarGroups({
        chatProjectIds: meta.chatProjectIds,
        hiddenProjectIds: meta.hiddenProjectIds,
        presentation,
        projectOverlays: meta.projectOverlays,
        resolveAgentIcon: resolveGpuiSidebarAgentIcon,
      }),
      ...createGpuiRemotePresentationSidebarGroups({
        presentationsByMachineId: new Map([['remote-m1', fixture.remote.snapshot]]),
        resolveAgentIcon: resolveGpuiSidebarAgentIcon,
        settings,
      }),
    ];
    const candidates = indicators.createGpuiSessionStatusIndicatorCandidatesFromSidebarGroups(
      groups,
      settings.enableSessionParking
    );
    return JSON.parse(
      JSON.stringify({
        name: fixture.name,
        pet: indicators.createGpuiPetOverlayStatePayload(candidates, settings),
        status: indicators.createGpuiSessionStatusIndicatorsPayload(candidates, settings),
      })
    );
  });
}

// ---------------------------------------------------------------- driver

function canonical(value: Json): Json {
  if (Array.isArray(value)) return value.map(canonical);
  if (value && typeof value === 'object') {
    return Object.fromEntries(
      Object.keys(value)
        .sort()
        .map((key) => [key, canonical(value[key])])
    );
  }
  return value;
}

function injectMutation(rust: Json): void {
  switch (inject) {
    case undefined:
      return;
    case 'feed-drop-item':
      rust.notificationFeed[4].message.items.pop();
      return;
    case 'feed-unread-count':
      rust.notificationFeed[5].message.unreadCount += 1;
      return;
    case 'feed-jump':
      rust.notificationFeed[12].jump = 'a';
      return;
    case 'attention-sound':
      rust.attention[2].sounds = [];
      return;
    case 'attention-report':
      rust.attention[6].rpcs[0].event = 'escape';
      return;
    case 'hud-settings':
      rust.hud[6].hud.settings.sidebarTooltipDelayMs += 100;
      return;
    case 'hud-recent':
      rust.hud[7].hud.recentProjects.reverse();
      return;
    case 'hud-scopes':
      rust.hud[8].hud.projectViewProjects.pop();
      return;
    case 'indicators-count':
      rust.indicators[0].status.attentionCount += 1;
      return;
    case 'indicators-order':
      rust.indicators[0].status.projects.reverse();
      return;
    case 'pet':
      rust.indicators[1].pet.selectedPetId = 'boo';
      return;
    case 'attention-visible':
      rust.attention[7].visible[Object.keys(rust.attention[7].visible)[0]] = 'attention';
      return;
    default:
      console.error(`Unknown mutation ${inject}.`);
      process.exit(2);
  }
}

const dir = keep ?? mkdtempSync(join(tmpdir(), 'f2-parity-'));
try {
  const fixtures = {
    attention: attentionFixtures(),
    hud: hudFixtures(),
    indicators: indicatorFixtures(),
    notificationFeed: notificationFeedFixtures(),
  };
  // JSON round trip first, so both halves read the same bytes (undefined fields vanish).
  writeFileSync(join(dir, 'fixtures.json'), JSON.stringify(fixtures));
  const read = JSON.parse(readFileSync(join(dir, 'fixtures.json'), 'utf8'));
  const typescript = {
    attention: await attentionTypescript(dir, read.attention),
    hud: await hudTypescript(dir, read.hud),
    indicators: await indicatorsTypescript(dir, read.indicators),
    notificationFeed: notificationFeedTypescript(read.notificationFeed),
  };
  writeFileSync(join(dir, 'typescript.json'), JSON.stringify(typescript, null, 2));
  const cargo = spawnSync('cargo', ['run', '-q', '--example', 'f2_parity', '--', dir], {
    cwd: join(root, 'packages/gx-core'),
    stdio: ['ignore', 'inherit', 'inherit'],
  });
  if (cargo.status !== 0) {
    console.error('The Rust half failed.');
    process.exit(2);
  }
  const rust = JSON.parse(readFileSync(join(dir, 'rust.json'), 'utf8'));
  for (const entry of rust.hud) entry.hud = hudContract(entry.hud);
  injectMutation(rust);
  let differences = 0;
  let cases = 0;
  for (const family of Object.keys(typescript) as Array<keyof typeof typescript>) {
    const left = typescript[family] as Json[];
    const right = rust[family] as Json[];
    for (let index = 0; index < Math.max(left.length, right.length); index++) {
      cases++;
      const a = JSON.stringify(canonical(left[index]));
      const b = JSON.stringify(canonical(right[index]));
      if (a !== b) {
        differences++;
        console.log(`DIFF ${family} #${index} ${left[index]?.name ?? right[index]?.name}\n  ts:   ${a}\n  rust: ${b}`);
      }
    }
  }
  console.log(`F2 parity: ${cases} cases, ${differences} difference(s)${inject ? ` (injected ${inject})` : ''}.`);
  process.exit(inject ? (differences > 0 ? 0 : 1) : differences > 0 ? 1 : 0);
} finally {
  if (!keep) rmSync(dir, { force: true, recursive: true });
}
