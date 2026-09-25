/**
 * The gate for New Group, Rename and Close Group, which moved from the app runtime to Rust on
 * 2026-09-25 (packages/gx-core/src/workspace_groups/group_commands.rs).
 *
 *   cargo run --release --example group_commands_parity -- <out-dir>     # from packages/gx-core
 *   bun tooling/gx-core/group-commands-parity.ts <out-dir> [--inject <mutation>]
 *
 * The TypeScript half runs the handlers the app shipped (`createWorkspaceGroup`,
 * `renameWorkspaceGroup`, `closeWorkspaceGroup`, frozen in workspace-groups-edits-frozen.ts) on a
 * stand-in runtime whose edges are recorders: `persistWorkspaceGroups` (the document write),
 * `transitionSession` (a member's close), `postSidebarActionToast`, and the active group it leaves
 * behind (which the Rust host asks the runtime's `focusGroup` for). Every case of the Rust dump is
 * compared call for call. `--inject` mutates the Rust side and must produce differences.
 */
import './browser-shim';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { frozenWorkspaceGroupEditMethods } from './workspace-groups-edits-frozen';
import { parseGpuiWorkspaceSessionGroupsState } from './workspace-session-groups-frozen';

type Json = any;

const MUTATIONS = ['rename-keeps-padding', 'close-skips-members', 'create-ignores-limit', 'close-keeps-active'];

function canonical(value: unknown): string {
  return JSON.stringify(value, (_key, inner) =>
    inner && typeof inner === 'object' && !Array.isArray(inner)
      ? Object.fromEntries(Object.entries(inner).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)))
      : inner
  );
}

async function runTypeScript(entry: Json): Promise<Json[] | null> {
  const calls: Json[] = [];
  const runtime = Object.assign({}, frozenWorkspaceGroupEditMethods) as Json;
  runtime.workspaceGroups = parseGpuiWorkspaceSessionGroupsState(entry.documentState);
  runtime.activeProjectId = entry.activeProjectId ?? undefined;
  runtime.activeGroupId = entry.activeGroupId ?? undefined;
  runtime.presentation = {};
  runtime.persistWorkspaceGroups = () => calls.push({ call: 'editDocument', document: runtime.workspaceGroups });
  runtime.transitionSession = async (sessionId: string, action: string) =>
    calls.push({ call: action === 'close' ? 'closeSession' : action, sessionId });
  runtime.postSidebarActionToast = (level: string, title: string) => calls.push({ call: 'toast', level, title });
  runtime.refreshSidebarHudFromClient = () => {};
  runtime.publishPresentation = () => {};
  runtime.publishRemotePresentationPatch = () => {};
  const before = runtime.activeGroupId;
  const message = entry.message;
  switch (message.type) {
    case 'createGroup':
      runtime.createWorkspaceGroup(message.groupId);
      break;
    case 'renameGroup':
      runtime.renameWorkspaceGroup(message.groupId, message.title);
      break;
    case 'closeGroup':
      await runtime.closeWorkspaceGroup(message.groupId);
      break;
    default:
      return null;
  }
  const edited = calls.some((call) => call.call === 'editDocument');
  if ((message.type === 'createGroup' && edited) || runtime.activeGroupId !== before) {
    calls.push({ call: 'activate', groupId: runtime.activeGroupId });
  }
  return calls;
}

function mutate(name: string | undefined, entry: Json): Json[] | null {
  const calls: Json[] | null = entry.calls ? JSON.parse(JSON.stringify(entry.calls)) : null;
  if (!name || !calls) return calls;
  const type = entry.message.type;
  if (
    name === 'rename-keeps-padding' &&
    type === 'renameGroup' &&
    /^\s|\s$/.test(entry.message.title) &&
    entry.message.title.trim()
  ) {
    for (const call of calls) {
      if (call.call !== 'editDocument') continue;
      for (const project of Object.values(call.document.projects) as Json[])
        for (const group of project.groups)
          if (group.title === entry.message.title.trim()) group.title = entry.message.title;
    }
  }
  if (name === 'close-skips-members' && type === 'closeGroup')
    return calls.filter((call) => call.call !== 'closeSession');
  if (name === 'create-ignores-limit' && type === 'createGroup' && calls.some((call) => call.call === 'toast'))
    return [];
  if (name === 'close-keeps-active' && type === 'closeGroup') return calls.filter((call) => call.call !== 'activate');
  return calls;
}

async function main() {
  const [outDir, ...flags] = process.argv.slice(2);
  if (!outDir) throw new Error('group-commands-parity.ts <out-dir> [--inject <mutation>]');
  const injectAt = flags.indexOf('--inject');
  const mutation = injectAt >= 0 ? flags[injectAt + 1] : undefined;
  if (mutation && !MUTATIONS.includes(mutation)) {
    console.error(`unknown mutation ${mutation}; one of ${MUTATIONS.join(', ')}`);
    process.exit(2);
  }
  const dump = JSON.parse(readFileSync(join(outDir, 'rust-group-commands.json'), 'utf8')) as Json;
  const differences: string[] = [];
  const counts: Record<string, number> = { cases: 0, edits: 0, closes: 0, toasts: 0, activations: 0, nothing: 0 };
  for (const entry of dump.cases as Json[]) {
    counts.cases += 1;
    const mine = mutate(mutation, entry);
    const theirs = await runTypeScript(entry);
    for (const call of mine ?? []) {
      if (call.call === 'editDocument') counts.edits += 1;
      if (call.call === 'closeSession') counts.closes += 1;
      if (call.call === 'toast') counts.toasts += 1;
      if (call.call === 'activate') counts.activations += 1;
    }
    if (!mine?.length) counts.nothing += 1;
    if (canonical(mine) !== canonical(theirs))
      differences.push(
        `${entry.document}/${entry.active} ${canonical(entry.message)}: rust ${canonical(mine)} ts ${canonical(theirs)}`
      );
  }
  console.log(
    `group commands: ${Object.entries(counts)
      .map(([key, value]) => `${key} ${value}`)
      .join(' ')} differences ${differences.length}${mutation ? ` (injected ${mutation})` : ''}`
  );
  for (const difference of differences.slice(0, 10)) console.log(`  ${difference.slice(0, 600)}`);
  const collapsed = Object.entries(counts).filter(([, value]) => value === 0);
  if (mutation) {
    process.exitCode = differences.length > 0 ? 0 : 1;
    return;
  }
  if (collapsed.length) console.log(`  ${collapsed.map(([key]) => key).join(', ')} counted nothing`);
  process.exitCode = differences.length || collapsed.length ? 1 : 0;
}

await main();
