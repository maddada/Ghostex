/**
 * Diffs the Rust Git model (packages/gx-core/src/git_menu/) against the old app runtime's
 * TypeScript it replaces, over every combination of the facts it decides from: the titlebar Git
 * menu (rows, labels, disabled state, primary action), what each Git action does after the state
 * read, the review dialog's draft and trusted file selection, the labels and prompts the flows
 * show, the worktree dialogs' pure helpers, and the background poll's schedule.
 *
 * The TypeScript side is the FROZEN copy in `git-frozen/` (the runtime's own files were deleted
 * by the port) plus `packages/shared/sidebar-git.ts`, which the menu still shares. Nothing is
 * reimplemented on this side beyond returning a decision instead of performing it.
 *
 *   bun tooling/gx-core/git-menu-parity.ts scenarios <out-dir> [--inject]
 *   cargo run --example git_menu_parity -- <out-dir>      # from packages/gx-core
 *   bun tooling/gx-core/git-menu-parity.ts compare <out-dir>
 *
 * `--inject` changes one TypeScript answer, so `compare` must then report exactly one difference:
 * the proof that the gate can fail.
 */
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import type { SidebarGitAction, SidebarGitState } from '@/packages/shared/sidebar-git';
import { createDefaultSidebarGitState } from '@/packages/shared/sidebar-git';
import {
  buildGpuiGitPullRequestAgentPrompt,
  buildGpuiGitSyncWithMainPrompt,
  buildGpuiMergeConflictPrompt,
  createGpuiTitlebarGitMenuStatePayload,
  formatGpuiGitAgentWorkflowTitle,
  gpuiProjectNameFromPath,
  gpuiUserVisibleGitErrorMessage,
  gpuiWorktreeFolderSuffix,
  gpuiWorktreeListErrorText,
  gpuiWorktreeRenameUserVisibleErrorMessage,
  gpuiWorktreeSlugFromPrompt,
  gpuiWorktreeUserVisibleErrorMessage,
  hasGpuiGitShortStatusChanges,
  isGpuiManagedWorktreeBranch,
  normalizeGpuiExistingWorktreeOptions,
  normalizeGpuiProjectPath,
  normalizeGpuiRelativeGitFilePath,
  normalizeGpuiWorktreeBaseBranches,
  normalizeGpuiWorktreeDeleteBranchName,
  parseGpuiSidebarGitCommitMessage,
  resolveGpuiSidebarGitConfirmLabel,
  resolveGpuiSidebarGitFinishedTitle,
  resolveGpuiSidebarGitPromptDescription,
  resolveGpuiSidebarGitStartedTitle,
} from './git-frozen/helpers';
import {
  frozenPlanAfterRead,
  frozenPlanBeforeRead,
  frozenPollCycle,
  frozenReviewDraft,
  frozenTrustedSelection,
} from './git-frozen/flows';

type Scenario = { kind: string; input: unknown };

const [mode, outDir, flag] = process.argv.slice(2);
if (!outDir || (mode !== 'scenarios' && mode !== 'compare')) {
  console.error('usage: git-menu-parity.ts scenarios <out-dir> [--inject] | compare <out-dir>');
  process.exit(2);
}

function plain(value: unknown): unknown {
  return value === undefined ? null : JSON.parse(JSON.stringify(value));
}

function states(): SidebarGitState[] {
  const out: SidebarGitState[] = [];
  const bools = [false, true];
  for (const isBusy of bools)
    for (const isRepo of bools)
      for (const hasWorkingTreeChanges of bools)
        for (const hasUpstream of bools)
          for (const hasOriginRemote of bools)
            for (const hasGitHubCli of bools)
              for (const isWorktree of bools)
                for (const branch of [null, 'main', 'feature/x'])
                  for (const aheadCount of [0, 2])
                    for (const behindCount of [0, 1])
                      for (const pr of [null, 'open', 'merged'] as const)
                        for (const primaryAction of ['commit', 'push', 'pr'] as SidebarGitAction[]) {
                          out.push({
                            ...createDefaultSidebarGitState(primaryAction, false, true),
                            additions: hasWorkingTreeChanges ? 7 : 0,
                            aheadCount,
                            behindCount,
                            branch,
                            deletions: hasWorkingTreeChanges ? 3 : 0,
                            files: hasWorkingTreeChanges
                              ? [
                                  { additions: 5, deletions: 1, path: 'src/a.rs' },
                                  { additions: 2, deletions: 2, path: 'docs/b.md' },
                                ]
                              : [],
                            hasCheckedGitHubRemote: true,
                            hasGitHubCli,
                            hasGitHubRemote: hasOriginRemote,
                            hasOriginRemote,
                            hasUpstream,
                            hasWorkingTreeChanges,
                            isBusy,
                            isRepo,
                            isWorktree,
                            pr: pr ? { number: 12, state: pr, title: 'A change', url: 'https://github.com/o/r/pull/12' } : null,
                          });
                        }
  return out;
}

function buildScenarios(): Scenario[] {
  const scenarios: Scenario[] = [];
  const all = states();
  for (const state of all) scenarios.push({ kind: 'menu', input: state });
  const actions: SidebarGitAction[] = ['commit', 'push', 'pr', 'syncMain', 'syncRemote', 'multiRelease', 'release'];
  // Every action over a spread of the states (each 7th), both worktree flags and both machines.
  for (const [index, state] of all.entries()) {
    if (index % 7 !== 0) continue;
    for (const action of actions)
      for (const isWorktree of [false, true])
        for (const remote of [false, true]) scenarios.push({ kind: 'plan', input: { action, isWorktree, remote, state } });
  }
  for (const [index, state] of all.entries()) {
    if (index % 97 !== 0) continue;
    for (const action of ['commit', 'push', 'pr'])
      for (const remote of [false, true])
        scenarios.push({
          kind: 'reviewDraft',
          input: {
            action,
            agentId: remote || index % 2 ? 'codex' : undefined,
            isWorktree: state.isWorktree,
            remote,
            requestId: `request-${index}`,
            state,
            worktreeName: index % 3 ? 'wt' : undefined,
          },
        });
  }
  const files = [
    { additions: 1, deletions: 0, path: 'src/a.rs' },
    { additions: 0, deletions: 2, path: 'b c/d.md' },
  ];
  for (const filePaths of [
    undefined,
    [],
    ['src/a.rs'],
    ['/src/a.rs', 'src/a.rs', 'b c/d.md'],
    ['src\\a.rs'],
    ['../x'],
    ['src/./a.rs'],
    ['nope'],
  ]) {
    scenarios.push({ kind: 'trusted', input: { files, filePaths } });
  }
  for (const action of ['commit', 'push', 'pr'])
    for (const hasCommit of [false, true]) scenarios.push({ kind: 'labels', input: { action, hasCommit } });
  for (const title of ['Release', ' Git: Release ', 'Git:X', 'Commit, Push & PR'])
    scenarios.push({ kind: 'workflowTitle', input: title });
  scenarios.push({ kind: 'syncPrompt', input: null });
  for (const hasExplicitFileSelection of [false, true])
    for (const hasCommit of [false, true])
      for (const message of ['', 'feat: thing\n\nbody'])
        for (const selectedFiles of [[], ['a.rs', ' ', 'b.rs']])
          scenarios.push({ kind: 'prPrompt', input: { hasCommit, hasExplicitFileSelection, message, selectedFiles } });
  for (const mergeOutput of ['', '  CONFLICT (content): a.rs \n'])
    scenarios.push({
      kind: 'mergePrompt',
      input: { branch: 'feature/x', mergeOutput, parentName: 'Parent', worktreeName: 'wt' },
    });
  for (const message of ['', '   ', 'subject', ' subject \n body line \r\n second ', 'a\r\nb'])
    scenarios.push({ kind: 'commitMessage', input: message });
  for (const path of ['a/b', '/a/b', '\\\\x\\y', 'a//b', 'a/../b', ' a ', '', 'a/.', 'x\0y'])
    scenarios.push({ kind: 'relativePath', input: path });
  for (const message of ['', 'Could not stage changes.', 'a\u0007b\n\n  c', 'x'.repeat(700)])
    scenarios.push({ kind: 'gitError', input: { fallback: 'gxserver Git operation failed.', message } });
  for (const stdout of ['', '## main', '## main\n M a.rs', '   \n'])
    scenarios.push({ kind: 'shortStatus', input: stdout });
  for (const [current, fallback] of [
    ['main\n', 'x'],
    ['HEAD', 'feat'],
    ['', undefined],
    [undefined, 'detached'],
    [' ', ' y '],
  ])
    scenarios.push({ kind: 'branchName', input: { current, fallback } });
  for (const [folder, parent] of [
    ['app-feature', 'app'],
    ['app', 'app'],
    ['other', 'app'],
    ['-x', ''],
  ])
    scenarios.push({ kind: 'folderSuffix', input: { folder, parent } });
  for (const branch of [undefined, 'ghostex/abc123', 'ghostex/automation', 'ghostex/automation/x', 'ghostex/UP', 'main', 'ghostex/'])
    scenarios.push({ kind: 'managedBranch', input: branch ?? null });
  for (const message of ['', 'Branch "feat/x" already exists.', 'No gxserver endpoint for POST /api/renameWorktreeProject.', 'a\nb', 'a\\b', 'x'.repeat(201)])
    scenarios.push({ kind: 'renameError', input: message });
  for (const message of ['', 'Worktree prompt is empty.', 'path/with/slash', 'x'.repeat(161)])
    scenarios.push({ kind: 'worktreeError', input: message });
  for (const [message, folder] of [
    ['', '/repo'],
    ['Git could not read /repo: bad', '/repo'],
    ['Could not list worktrees.', '/repo/'],
    ['boom', undefined],
  ])
    scenarios.push({ kind: 'listError', input: { folder, message } });
  for (const path of ['/a/b/', ' /a ', '', '/', null])
    scenarios.push({ kind: 'projectPath', input: path });
  for (const path of ['/a/b', 'C:\\dev\\proj', '\\\\srv\\share\\p', '/', 'a'])
    scenarios.push({ kind: 'projectName', input: path });
  for (const prompt of ['', 'Fix the "login" bug, now!', 'Ünïcode here and there', 'one two three four five six seven eight', 'a'.repeat(80)])
    scenarios.push({ kind: 'slug', input: prompt });
  scenarios.push({
    kind: 'baseBranches',
    input: [{ current: true, name: 'main' }, { name: ' main ' }, { name: 'origin/main', remote: true }, { name: '' }, {}],
  });
  scenarios.push({
    kind: 'existingWorktrees',
    input: [
      { branch: 'b', isCurrentProject: true, isRegistered: false, name: 'n', path: '/p/', worktreeKey: 'k1' },
      { path: '/q/r', worktreeKey: 'k2' },
      { path: '/no-key' },
      'junk',
    ],
  });
  for (const count of [0, 1, 3, 15, 16, 40])
    scenarios.push({
      kind: 'pollCycle',
      input: Array.from({ length: count }, (_, index) => `${index % 2 ? 'remote' : 'local'}:p${String(index).padStart(3, '0')}`),
    });
  return scenarios;
}

function typescriptAnswer(scenario: Scenario): unknown {
  const input = scenario.input as any;
  switch (scenario.kind) {
    case 'menu': {
      const { type: _type, version: _version, ...menu } = createGpuiTitlebarGitMenuStatePayload(input);
      return menu;
    }
    case 'plan':
      return frozenPlanBeforeRead(input.action) ?? frozenPlanAfterRead(input.action, input.state, input.isWorktree, input.remote) ?? null;
    case 'reviewDraft':
      return frozenReviewDraft({ ...input, gitState: input.state });
    case 'trusted':
      return frozenTrustedSelection(input.files, input.filePaths);
    case 'labels':
      return {
        confirm: resolveGpuiSidebarGitConfirmLabel(input.action, input.hasCommit),
        description: resolveGpuiSidebarGitPromptDescription(input.action),
        finished: resolveGpuiSidebarGitFinishedTitle(input.action),
        started: resolveGpuiSidebarGitStartedTitle(input.action, input.hasCommit),
      };
    case 'workflowTitle':
      return formatGpuiGitAgentWorkflowTitle(input);
    case 'syncPrompt':
      return buildGpuiGitSyncWithMainPrompt();
    case 'prPrompt':
      return buildGpuiGitPullRequestAgentPrompt({ ...input, filePaths: undefined });
    case 'mergePrompt':
      return buildGpuiMergeConflictPrompt({
        branch: input.branch,
        mergeOutput: input.mergeOutput,
        parentProject: { name: input.parentName, projectId: 'p' },
        worktree: { name: input.worktreeName, parentProjectId: 'p' },
        worktreeProject: { projectId: 'w' },
      } as any);
    case 'commitMessage':
      return parseGpuiSidebarGitCommitMessage(input);
    case 'relativePath':
      return normalizeGpuiRelativeGitFilePath(input);
    case 'gitError':
      return gpuiUserVisibleGitErrorMessage(new Error(input.message), input.fallback);
    case 'shortStatus':
      return hasGpuiGitShortStatusChanges(input);
    case 'branchName':
      return normalizeGpuiWorktreeDeleteBranchName(input.current, input.fallback);
    case 'folderSuffix':
      return gpuiWorktreeFolderSuffix(input.folder, input.parent);
    case 'managedBranch':
      return isGpuiManagedWorktreeBranch(input ?? undefined);
    case 'renameError':
      return gpuiWorktreeRenameUserVisibleErrorMessage(new Error(input));
    case 'worktreeError':
      return gpuiWorktreeUserVisibleErrorMessage(new Error(input));
    case 'listError':
      return gpuiWorktreeListErrorText(new Error(input.message), input.folder);
    case 'projectPath':
      return normalizeGpuiProjectPath(input) || undefined;
    case 'projectName':
      return gpuiProjectNameFromPath(input);
    case 'slug':
      return gpuiWorktreeSlugFromPrompt(input);
    case 'baseBranches':
      return normalizeGpuiWorktreeBaseBranches(input);
    case 'existingWorktrees':
      return normalizeGpuiExistingWorktreeOptions(input);
    case 'pollCycle':
      return frozenPollCycle(input);
    default:
      return { unknownKind: scenario.kind };
  }
}

if (mode === 'scenarios') {
  mkdirSync(outDir, { recursive: true });
  const scenarios = buildScenarios();
  const answers = scenarios.map((scenario) => plain(typescriptAnswer(scenario)));
  if (flag === '--inject') {
    const index = scenarios.findIndex((scenario) => scenario.kind === 'labels');
    answers[index] = { ...(answers[index] as object), confirm: 'Injected' };
  }
  writeFileSync(`${outDir}/scenarios.json`, JSON.stringify(scenarios.map((scenario) => ({ ...scenario, input: plain(scenario.input) }))));
  writeFileSync(`${outDir}/typescript.json`, JSON.stringify(answers));
  console.log(`${scenarios.length} scenarios written`);
} else {
  const scenarios = JSON.parse(readFileSync(`${outDir}/scenarios.json`, 'utf8')) as Scenario[];
  const typescript = JSON.parse(readFileSync(`${outDir}/typescript.json`, 'utf8')) as unknown[];
  const rust = JSON.parse(readFileSync(`${outDir}/rust.json`, 'utf8')) as unknown[];
  const byKind = new Map<string, { total: number; differences: number }>();
  const examples: string[] = [];
  scenarios.forEach((scenario, index) => {
    const row = byKind.get(scenario.kind) ?? { total: 0, differences: 0 };
    row.total += 1;
    const left = JSON.stringify(sortKeys(typescript[index]));
    const right = JSON.stringify(sortKeys(rust[index]));
    if (left !== right) {
      row.differences += 1;
      if (examples.length < 8) examples.push(`${scenario.kind} #${index}\n  ts:   ${left.slice(0, 400)}\n  rust: ${right.slice(0, 400)}`);
    }
    byKind.set(scenario.kind, row);
  });
  let differences = 0;
  for (const [kind, row] of [...byKind].sort()) {
    differences += row.differences;
    console.log(`${kind.padEnd(18)} ${String(row.total).padStart(6)} scenarios, ${row.differences} differences`);
  }
  for (const example of examples) console.log(example);
  console.log(`${scenarios.length} scenarios, ${differences} differences`);
  process.exit(differences === 0 ? 0 : 1);
}

function sortKeys(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(sortKeys);
  if (value && typeof value === 'object') {
    return Object.fromEntries(
      Object.entries(value as Record<string, unknown>)
        .filter(([, entry]) => entry !== undefined && entry !== null)
        .sort(([left], [right]) => left.localeCompare(right))
        .map(([key, entry]) => [key, sortKeys(entry)])
    );
  }
  return value;
}
