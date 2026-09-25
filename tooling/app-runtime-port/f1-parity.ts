/**
 * The app runtime port's F1 fixture gate: the CLI renderer command validation and target
 * resolution the QuickJS runtime did (`handleGxserverRendererCommand` and its resolvers in
 * `app-shot-and-misc.ts`, deleted in the same commit that added `plan_renderer_command`), run on the
 * same inputs as the Rust planner, diffed as JSON. Zero differences is the bar.
 *
 *   bun tooling/app-runtime-port/f1-parity.ts [--inject <mutation>] [--keep <dir>]
 *
 * Inputs are READ from the local gxserver (the presentation snapshot and the project list; nothing
 * is written), so every session and project of this computer is asked about; the same snapshot is
 * also loaded as a streamed remote machine so the remote-project branches run. The fixtures hold
 * private data and live in a temporary folder outside the repository (`--keep` names one).
 *
 * The TypeScript half is the deleted runtime code, quoted below with only its `this.` reads turned
 * into reads of the fixture. `hasGpuiRendererCommandLocalSession` also looked in `latestGroups`;
 * those groups are built from the same presentation, so they add no local session and are left out.
 * The verbs the desktop never answered before this port (switchProject and the rest) have no
 * TypeScript to compare with and are not in this gate.
 *
 * Mutations: session-drop, title-case, browser-reuse. Deleted with the runtime in step 3.
 */
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync, mkdirSync } from 'node:fs';
import { homedir, tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseGpuiWorkspaceSessionSubgroupId } from '@/packages/shared/workspace-session-subgroup-id';
import { SETTINGS_MODAL_NAVIGATION_TABS } from '@/packages/shared/ghostex-settings';
import {
  createGxserverPresentationProjectSessionId,
  parseGxserverPresentationProjectGroupId,
  parseGxserverPresentationProjectSessionId,
} from '@/packages/shared/gxserver-presentation-sidebar-projection';
import { createRemoteProjectId, parseRemoteProjectId } from '@/packages/shared/remote-terminal-selection';

type Json = any;

const args = process.argv.slice(2);
const option = (name: string) => {
  const index = args.indexOf(name);
  return index >= 0 ? args[index + 1] : undefined;
};
const inject = option('--inject');
const keep = option('--keep');
const root = fileURLToPath(new URL('../../', import.meta.url));
const REMOTE = 'remote-fixture';

// ---------------------------------------------------------------- the daemon's data (read only)

async function readDaemon(): Promise<{ snapshot: Json; projects: Json[] }> {
  const stateDir = process.env.GX_STATE_DIR ?? join(homedir(), '.local/state/ghostex/gxserver');
  const token = readFileSync(join(stateDir, 'auth/token'), 'utf8').trim();
  const port = JSON.parse(readFileSync(join(stateDir, 'runtime/server.json'), 'utf8')).port;
  const call = async (path: string) => {
    const response = await fetch(`http://127.0.0.1:${port}${path}`, {
      body: JSON.stringify({ params: {}, protocolVersion: 1 }),
      headers: { authorization: `Bearer ${token}`, 'content-type': 'application/json' },
      method: 'POST',
    });
    const body = (await response.json()) as Json;
    if (!body.ok) {
      throw new Error(`${path} failed`);
    }
    return body.result;
  };
  const { snapshot } = await call('/api/readPresentationSnapshot');
  const { projects } = await call('/api/listProjects');
  return { snapshot, projects };
}

// ---------------------------------------------------------------- the deleted runtime code, quoted

const readString = (record: Json, key: string): string | undefined =>
  record && typeof record[key] === 'string' ? record[key] : undefined;

class OldRuntime {
  constructor(
    private presentation: Json,
    private domainProjects: Json[],
    private remotePresentations: Map<string, Json>
  ) {}

  private hasLocalSession(projectId: string, sessionId: string): boolean {
    return this.presentation.sessions.some((s: Json) => s.projectId === projectId && s.sessionId === sessionId);
  }

  resolveSession(payload: Json): Json | undefined {
    const target =
      payload.sessionTarget && typeof payload.sessionTarget === 'object' && !Array.isArray(payload.sessionTarget)
        ? payload.sessionTarget
        : undefined;
    const globalRaw = readString(target, 'globalRef') ?? readString(payload, 'globalRef');
    const parts = globalRaw?.trim().split(':');
    const global =
      parts?.length === 3 && parts[1] && parts[2] ? { projectId: parts[1], sessionId: parts[2] } : undefined;
    const projectId =
      readString(target, 'projectId')?.trim() || readString(payload, 'projectId')?.trim() || global?.projectId;
    const sessionId =
      readString(target, 'sessionId')?.trim() || readString(payload, 'sessionId')?.trim() || global?.sessionId;
    if (!sessionId) {
      return undefined;
    }
    const scoped = parseGxserverPresentationProjectSessionId(sessionId);
    if (scoped) {
      if (projectId && scoped.projectId !== projectId) {
        return undefined;
      }
      if (!this.hasLocalSession(scoped.projectId, scoped.sessionId)) {
        return undefined;
      }
      return { projectId: scoped.projectId, sessionId: scoped.sessionId, sidebarSessionId: sessionId };
    }
    if (!projectId || !this.hasLocalSession(projectId, sessionId)) {
      return undefined;
    }
    return { projectId, sessionId, sidebarSessionId: createGxserverPresentationProjectSessionId(projectId, sessionId) };
  }

  private knownProjectId(projectId: string): string | undefined {
    const remote = parseRemoteProjectId(projectId);
    if (remote) {
      const presentation = this.remotePresentations.get(remote.machineId);
      if (presentation?.projects.some((p: Json) => p.projectId === remote.projectId)) {
        return createRemoteProjectId(remote);
      }
      return undefined;
    }
    return this.domainProjects.find((p) => p.projectId === projectId)?.projectId;
  }

  private groupProjectId(groupId: string): string | undefined {
    const subgroup = parseGpuiWorkspaceSessionSubgroupId(groupId);
    if (subgroup) {
      return subgroup.projectId;
    }
    const remoteGroup = /^remote:([^:]+):group:(.+)$/u.exec(groupId);
    if (remoteGroup) {
      return createRemoteProjectId({ machineId: remoteGroup[1]!, projectId: remoteGroup[2]! });
    }
    return parseGxserverPresentationProjectGroupId(groupId);
  }

  private normalizePath(value: unknown): string | undefined {
    return typeof value === 'string' && value.trim().length > 0 ? value.trim().replace(/\/+$/u, '') : undefined;
  }

  answer(action: string, payload: Json): Json {
    const fail = (message: string) => ({ error: message });
    switch (action) {
      case 'focusSession': {
        const session = this.resolveSession(payload);
        return session ? { verb: 'focusSession', ...session } : fail('No matching session was found.');
      }
      case 'renameCommand': {
        const session = this.resolveSession(payload);
        if (!session) {
          return fail('No matching session was found.');
        }
        const rawTitle = readString(payload, 'title');
        let title: string | undefined;
        if (rawTitle !== undefined && !/[\u0000-\u001f\u007f-\u009f]/u.test(rawTitle)) {
          const trimmed = rawTitle.trim();
          title = trimmed && trimmed.length <= 120 ? trimmed : undefined;
        }
        if (!title) {
          return fail('Invalid renderer command title.');
        }
        const row = this.presentation.sessions.find(
          (s: Json) => s.projectId === session.projectId && s.sessionId === session.sessionId
        );
        const agent = (row?.agentId ?? row?.agentName ?? '').trim().toLowerCase();
        const command =
          agent === 'pi' || agent === 'π'
            ? 'name'
            : agent === 'hermes' || agent === 'hermes agent' || agent === 'hermes-agent'
              ? 'title'
              : 'rename';
        return { verb: 'renameCommand', ...session, title, command };
      }
      case 'runCommand':
      case 'clickButton': {
        if (action === 'clickButton' && readString(payload, 'kind')?.trim() !== 'command') {
          return fail('Unsupported renderer command.');
        }
        const commandId = readString(payload, action === 'runCommand' ? 'commandId' : 'id')?.trim();
        return commandId ? { verb: 'runCommand', commandId } : fail('Unsupported renderer command.');
      }
      case 'readResourcesSnapshot':
        return { verb: 'readResourcesSnapshot' };
      case 'updateSettingsPatch': {
        const rawPatch = payload.patch;
        if (typeof rawPatch !== 'object' || rawPatch === null || Array.isArray(rawPatch)) {
          return fail('Invalid settings patch.');
        }
        const keys = Object.keys(rawPatch);
        if (keys.length === 0 || keys.length > 50) {
          return fail('Invalid settings patch.');
        }
        for (const key of keys) {
          const value = rawPatch[key];
          const scalar = typeof value === 'boolean' || typeof value === 'string' || typeof value === 'number';
          if (!scalar || (typeof value === 'number' && !Number.isFinite(value))) {
            return fail('Invalid settings patch.');
          }
        }
        return { verb: 'updateSettingsPatch', keys: [...keys].sort(), patch: rawPatch };
      }
      case 'openSettings': {
        const tab = readString(payload, 'tab')?.trim() || 'settings';
        if (!(SETTINGS_MODAL_NAVIGATION_TABS as readonly string[]).includes(tab)) {
          return fail('Invalid settings tab.');
        }
        const searchQuery = readString(payload, 'searchQuery')?.trim().slice(0, 200) || undefined;
        return { verb: 'openSettings', tab, searchQuery: searchQuery ?? null };
      }
      case 'openBrowser':
      case 'openBrowserPane': {
        const url = readString(payload, 'url')?.trim() ?? '';
        if (url.length > 16 * 1024) {
          // "Invalid renderer command URL." was not on the safe list, so the CLI saw this.
          return fail('Renderer command failed.');
        }
        const rawReuse = readString(payload, 'reuse')?.trim().toLowerCase();
        const reuse = rawReuse === 'exact' || rawReuse === 'none' ? rawReuse : 'similar';
        const groupId = readString(payload, 'groupId')?.trim();
        const requestedProjectId = readString(payload, 'projectId')?.trim();
        const projectPath = readString(payload, 'projectPath')?.trim();
        let projectId: string | undefined;
        if (groupId) {
          const groupProjectId = this.groupProjectId(groupId);
          projectId = groupProjectId ? this.knownProjectId(groupProjectId) : undefined;
        } else if (requestedProjectId) {
          projectId = this.knownProjectId(requestedProjectId);
        } else {
          const wanted = this.normalizePath(projectPath);
          projectId = wanted
            ? this.domainProjects.find((p) => this.normalizePath(p.path) === wanted)?.projectId
            : undefined;
        }
        if ((groupId || requestedProjectId || projectPath) && !projectId) {
          return fail('No matching project was found.');
        }
        return { verb: 'openBrowser', projectId: projectId ?? null, reuse, url };
      }
      default:
        return fail('Unsupported renderer command.');
    }
  }
}

// ---------------------------------------------------------------- cases

function buildCases(snapshot: Json, projects: Json[]): Json[] {
  const cases: Json[] = [];
  const add = (name: string, action: string, payload: Json) => cases.push({ action, name, payload });
  const sessions = snapshot.sessions as Json[];
  sessions.forEach((s, i) => {
    const p = s.projectId as string;
    const id = s.sessionId as string;
    const combined = createGxserverPresentationProjectSessionId(p, id);
    const other = projects.find((candidate) => candidate.projectId !== p)?.projectId ?? 'no-such-project';
    const selectors: [string, Json][] = [
      ['target', { sessionTarget: { projectId: p, sessionId: id } }],
      ['flat', { projectId: p, sessionId: id }],
      ['combined', { sessionId: combined }],
      ['combinedWrongProject', { projectId: other, sessionId: combined }],
      ['globalRef', { globalRef: `local:${p}:${id}` }],
      ['targetGlobalRef', { sessionTarget: { globalRef: ` local:${p}:${id} ` } }],
      ['rawOnly', { sessionId: id }],
      ['padded', { sessionTarget: { projectId: ` ${p} `, sessionId: `\t${id} ` } }],
      ['targetArray', { sessionTarget: [p, id], projectId: p, sessionId: id }],
      ['emptyTargetFallsBack', { sessionTarget: { projectId: '  ', sessionId: '' }, projectId: p, sessionId: id }],
    ];
    for (const [label, selector] of selectors) {
      add(`focus ${i} ${label}`, 'focusSession', selector);
    }
    const titles: [string, unknown][] = [
      ['plain', 'Fix the parser'],
      ['padded', '  Padded  '],
      ['control', 'bad\u0007title'],
      ['c1control', 'bad\u0085title'],
      ['empty', ''],
      ['blank', '   '],
      ['max', 'x'.repeat(120)],
      ['over', 'x'.repeat(121)],
      ['emojiOver', '😀'.repeat(61)],
      ['emojiMax', '😀'.repeat(60)],
      ['number', 42],
    ];
    for (const [label, title] of titles) {
      add(`rename ${i} ${label}`, 'renameCommand', { sessionTarget: { projectId: p, sessionId: id }, title });
    }
  });
  add('focus unknown', 'focusSession', { projectId: 'nope', sessionId: 'nope' });
  add('focus nothing', 'focusSession', {});
  add('focus remote-shaped', 'focusSession', { sessionId: `remote:${REMOTE}:session:a:b` });

  for (const [label, payload] of [
    ['id', { commandId: 'dev' }],
    ['padded', { commandId: '  dev  ' }],
    ['blank', { commandId: '  ' }],
    ['missing', {}],
    ['number', { commandId: 3 }],
  ] as [string, Json][]) {
    add(`runCommand ${label}`, 'runCommand', payload);
  }
  for (const [label, payload] of [
    ['command', { kind: 'command', id: 'dev' }],
    ['paddedKind', { kind: ' command ', id: 'dev' }],
    ['agent', { kind: 'agent', id: 'codex' }],
    ['noKind', { id: 'dev' }],
    ['noId', { kind: 'command' }],
  ] as [string, Json][]) {
    add(`clickButton ${label}`, 'clickButton', payload);
  }
  add('resources', 'readResourcesSnapshot', {});

  const manyKeys = Object.fromEntries(Array.from({ length: 51 }, (_, i) => [`k${i}`, i]));
  const fiftyKeys = Object.fromEntries(Array.from({ length: 50 }, (_, i) => [`k${i}`, true]));
  for (const [label, patch] of [
    ['one', { theme: 'dark' }],
    ['mixed', { a: true, b: 'x', c: 1.5, d: -2 }],
    ['empty', {}],
    ['array', ['a']],
    ['null', null],
    ['string', 'theme=dark'],
    ['nested', { a: { b: 1 } }],
    ['nullValue', { a: null }],
    ['fifty', fiftyKeys],
    ['fiftyOne', manyKeys],
    ['missing', undefined],
  ] as [string, Json][]) {
    add(`settingsPatch ${label}`, 'updateSettingsPatch', patch === undefined ? {} : { patch });
  }
  for (const [label, payload] of [
    ['none', {}],
    ['tab', { tab: 'hotkeys' }],
    ['paddedTab', { tab: ' agents ' }],
    ['blankTab', { tab: '  ' }],
    ['badTab', { tab: 'nope' }],
    ['search', { searchQuery: '  sleep  ' }],
    ['blankSearch', { searchQuery: '   ' }],
    ['longSearch', { searchQuery: 'q'.repeat(300) }],
    ['numberTab', { tab: 3 }],
  ] as [string, Json][]) {
    add(`openSettings ${label}`, 'openSettings', payload);
  }

  const browser = (label: string, payload: Json) => {
    add(`openBrowser ${label}`, 'openBrowser', payload);
    add(`openBrowserPane ${label}`, 'openBrowserPane', payload);
  };
  browser('bare', { url: 'https://example.com' });
  browser('empty', {});
  browser('long', { url: 'u'.repeat(16 * 1024 + 1) });
  browser('max', { url: 'u'.repeat(16 * 1024) });
  for (const reuse of ['exact', ' NONE ', 'similar', 'weird']) {
    browser(`reuse ${reuse}`, { reuse, url: 'x' });
  }
  browser('unknown project', { projectId: 'nope', url: 'x' });
  browser('unknown path', { projectPath: '/definitely/not/here', url: 'x' });
  browser('unknown group', { groupId: 'combined-project:nope', url: 'x' });
  browser('chats group', { groupId: 'combined-chats', url: 'x' });
  browser('remote unknown machine', { projectId: 'remote:nobody:project:x', url: 'x' });
  projects.forEach((project, i) => {
    const id = project.projectId as string;
    browser(`project ${i}`, { projectId: id, url: 'x' });
    browser(`project ${i} group`, { groupId: `combined-project:${encodeURIComponent(id)}`, url: 'x' });
    browser(`project ${i} subgroup`, { groupId: `gpui-wsg:${encodeURIComponent(id)}:g1`, url: 'x' });
    browser(`project ${i} remote`, { projectId: `remote:${REMOTE}:project:${id}`, url: 'x' });
    browser(`project ${i} remote group`, { groupId: `remote:${REMOTE}:group:${id}`, url: 'x' });
    browser(`project ${i} group wins`, { groupId: 'combined-project:nope', projectId: id, url: 'x' });
    if (typeof project.path === 'string') {
      browser(`project ${i} path`, { projectPath: project.path, url: 'x' });
      browser(`project ${i} path slash`, { projectPath: ` ${project.path}/// `, url: 'x' });
    }
  });
  for (const retired of [
    'assertSidebarCard',
    'waitFor',
    'saveAgent',
    'sendMessage',
    'setViewMode',
    'setVisibleCount',
    'nope',
  ]) {
    add(`retired ${retired}`, retired, {});
  }
  return cases;
}

// ---------------------------------------------------------------- mutations (prove the gate fails)

function mutate(answers: Json[], mutation: string): void {
  const first = (predicate: (answer: Json) => boolean) => answers.find((entry) => predicate(entry.answer));
  if (mutation === 'session-drop') {
    const hit = first((answer) => answer.verb === 'focusSession');
    if (hit) hit.answer = { error: 'No matching session was found.' };
  } else if (mutation === 'title-case') {
    const hit = first((answer) => answer.verb === 'renameCommand');
    if (hit) hit.answer.title = String(hit.answer.title).toUpperCase() + '!';
  } else if (mutation === 'browser-reuse') {
    const hit = first((answer) => answer.verb === 'openBrowser');
    if (hit) hit.answer.reuse = 'exact' === hit.answer.reuse ? 'none' : 'exact';
  } else {
    throw new Error(`unknown mutation ${mutation}`);
  }
}

// ---------------------------------------------------------------- driver

const dir = keep ?? mkdtempSync(join(tmpdir(), 'f1-parity-'));
mkdirSync(dir, { recursive: true });
try {
  const { snapshot, projects } = await readDaemon();
  const cases = buildCases(snapshot, projects);
  writeFileSync(
    join(dir, 'fixtures.json'),
    JSON.stringify({ cases, domainProjects: projects, remoteMachineId: REMOTE, remoteSnapshot: snapshot, snapshot })
  );
  const old = new OldRuntime(snapshot, projects, new Map([[REMOTE, snapshot]]));
  const expected = cases.map((c) => ({ answer: old.answer(c.action, c.payload), name: c.name }));
  writeFileSync(join(dir, 'typescript.json'), JSON.stringify(expected, null, 2));
  const run = spawnSync('cargo', ['run', '-q', '--example', 'f1_parity', '--', dir], {
    cwd: join(root, 'packages/gx-core'),
    stdio: ['ignore', 'inherit', 'inherit'],
  });
  if (run.status !== 0) {
    console.error('the Rust half failed');
    process.exit(2);
  }
  const actual: Json[] = JSON.parse(readFileSync(join(dir, 'rust.json'), 'utf8'));
  if (inject) {
    mutate(actual, inject);
  }
  const normalize = (value: Json) => JSON.stringify(value);
  const sortKeys = (value: Json): Json =>
    Array.isArray(value)
      ? value.map(sortKeys)
      : value && typeof value === 'object'
        ? Object.fromEntries(
            Object.keys(value)
              .sort()
              .map((key) => [key, sortKeys(value[key])])
          )
        : value;
  let differences = 0;
  const verbs = new Map<string, number>();
  expected.forEach((entry, index) => {
    const theirs = actual[index];
    const verb = entry.answer.verb ?? `error:${entry.answer.error}`;
    verbs.set(verb, (verbs.get(verb) ?? 0) + 1);
    if (normalize(sortKeys(entry)) !== normalize(sortKeys(theirs))) {
      differences += 1;
      if (differences <= 10) {
        // Names only: the answers can hold ids and titles of this computer.
        console.error(`difference: ${entry.name}`);
      }
    }
  });
  console.log(`cases ${cases.length}, sessions ${snapshot.sessions.length}, projects ${projects.length}`);
  for (const [verb, count] of [...verbs].sort()) {
    console.log(`  ${verb}: ${count}`);
  }
  console.log(`differences ${differences}`);
  if (inject) {
    process.exit(differences > 0 ? 0 : 1);
  }
  process.exit(differences === 0 ? 0 : 1);
} finally {
  if (!keep) {
    rmSync(dir, { force: true, recursive: true });
  }
}
