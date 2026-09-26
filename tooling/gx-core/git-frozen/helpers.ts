/**
 * FROZEN copies of the old app runtime's pure Git and worktree helpers, taken verbatim from
 * apps/desktop/sidebar/gxserver-runtime/helpers/git.ts, helpers/worktrees.ts and constants.ts at
 * commit 4140ffd0e, the last commit before the app runtime port's F5 deleted them. The Git parity
 * harness (tooling/gx-core/git-menu-parity.ts) diffs the Rust port (packages/gx-core/src/git_menu/)
 * against these, so a clean run proves Rust still matches what the app shipped that day.
 * Deleted with the runtime in step 3 of docs/2026-09-25/app-runtime-port/PLAN.md.
 */
/* eslint-disable */
import type { SidebarGitAction, SidebarGitState } from '@/packages/shared/sidebar-git';
import {
  buildSidebarGitMenuItems,
  getSidebarGitDisabledReason,
  hasSidebarGitRemoteCommitDelta,
  resolveSidebarGitPrimaryActionState,
} from '@/packages/shared/sidebar-git';

const GPUI_TITLEBAR_GIT_MENU_STATE_MESSAGE_TYPE = 'ghostex.gpui.sidebar.titlebarGitMenuState';
const GPUI_TITLEBAR_GIT_MENU_STATE_MESSAGE_VERSION = 1;
type GpuiWorktreeMetadata = { branch?: string; name?: string; parentProjectId: string; parentProjectName?: string };
type GxserverProjectDomainState = { name?: string; path?: string; projectId: string };
type GxserverTypedOperationResult = { branches?: Array<{ current?: boolean; name?: string; remote?: boolean }> };

export const GPUI_GIT_MULTIPLE_COMMITS_PROMPT = `Please review my current changes and commit them as multiple focused commits.

Commit-splitting rules:
- Group changes by related feature, fix, or topic.
- Do not combine unrelated work in the same commit.
- Use file-based splitting only; do not split individual hunks.
- Make each commit easy to revert or cherry-pick later.
- Use clear, concise commit messages.`;

export const GPUI_REMOTE_MERGE_CONFLICT_PROMPT =
  "A direct merge into main has conflicts in this remote project. Inspect the repository state, resolve the conflicts, and commit the merge when it is correct.";

export const GPUI_GIT_RELEASE_STEPS_PROMPT = `1. Push any local commits to remote.
2. Review the commits since the last released version.
3. Update CHANGELOG.md to mention the new changes.
4. Publish the next minor version to the usual places we publish this app.`;

export const GPUI_GIT_MULTICOMMIT_RELEASE_PROMPT = `${GPUI_GIT_MULTIPLE_COMMITS_PROMPT}

After all focused commits are created:
${GPUI_GIT_RELEASE_STEPS_PROMPT}`;

export const GPUI_GIT_RELEASE_ONLY_PROMPT = `Please release this app using the usual release workflow.

${GPUI_GIT_RELEASE_STEPS_PROMPT}`;

export function createGpuiTitlebarGitMenuStatePayload(state: SidebarGitState): {
  additions: number;
  aheadCount: number;
  behindCount: number;
  branch: string | null;
  deletions: number;
  hasWorkingTreeChanges: boolean;
  isBusy: boolean;
  isRepo: boolean;
  primaryAction: SidebarGitAction;
  rows: {
    action: SidebarGitAction;
    disabled: boolean;
    label: string;
    primary: boolean;
  }[];
  syncRemoteDisabled: boolean;
  type: string;
  version: number;
} {
  // The native titlebar renders this projection verbatim, so the shared menu
  // builders stay the single owner of row order, labels, and disabled gating.
  // The primary row carries the resolved split-primary label macOS shows on
  // its split button, since a native menu cannot express the split control.
  const primary = resolveSidebarGitPrimaryActionState(state);
  return {
    additions: state.additions,
    aheadCount: state.aheadCount,
    behindCount: state.behindCount,
    branch: state.branch,
    deletions: state.deletions,
    hasWorkingTreeChanges: state.hasWorkingTreeChanges,
    isBusy: state.isBusy,
    isRepo: state.isRepo,
    primaryAction: primary.action,
    rows: buildSidebarGitMenuItems(state).map((item) => ({
      action: item.action,
      disabled: item.action === primary.action ? primary.disabled : item.disabled,
      label: item.action === primary.action ? primary.label : item.label,
      primary: item.action === primary.action,
    })),
    syncRemoteDisabled:
      getSidebarGitDisabledReason(state, 'syncRemote') !== undefined || !hasSidebarGitRemoteCommitDelta(state),
    type: GPUI_TITLEBAR_GIT_MENU_STATE_MESSAGE_TYPE,
    version: GPUI_TITLEBAR_GIT_MENU_STATE_MESSAGE_VERSION,
  };
}

export function normalizeGpuiRelativeGitFilePath(filePath: string): string | undefined {
  const normalizedFilePath = filePath.replaceAll('\\', '/').replace(/^\/+/, '').trim();
  if (!normalizedFilePath || normalizedFilePath.includes('\0')) {
    return undefined;
  }
  const segments = normalizedFilePath.split('/');
  if (segments.some((segment) => !segment || segment === '.' || segment === '..')) {
    return undefined;
  }
  return normalizedFilePath;
}

export function parseGpuiSidebarGitCommitMessage(message: string): {
  body: string;
  subject: string;
} {
  const trimmedMessage = message.trim();
  if (!trimmedMessage) {
    return { body: '', subject: '' };
  }
  const [firstLine = '', ...restLines] = trimmedMessage.split(/\r?\n/);
  return {
    body: restLines.join('\n').trim(),
    subject: firstLine.trim(),
  };
}

export function gpuiUserVisibleGitErrorMessage(error: unknown, fallback: string): string {
  /*
  CDXC:Git 2026-07-11-05:08:
  The gxserver client already converts daemon failures into bounded,
  user-facing Error messages. Preserve those messages at the Git mutation
  boundary so stale reviews, unavailable agents, and generation failures do
  not collapse into an unactionable generic toast. Generation runs inside the
  mutation's keyed progress toast, so it must not create a second unkeyed info
  toast that survives after the mutation fails.
  */
  if (!(error instanceof Error)) {
    return fallback;
  }
  const message = error.message
    .replace(/[\u0000-\u001f\u007f-\u009f]+/gu, ' ')
    .replace(/\s+/gu, ' ')
    .trim()
    .slice(0, 500);
  return message || fallback;
}

export function resolveGpuiSidebarGitConfirmLabel(
  action: Extract<SidebarGitAction, 'commit' | 'pr' | 'push'>,
  hasCommit: boolean
): string {
  if (action === 'commit') {
    return 'Commit';
  }
  if (action === 'push') {
    return hasCommit ? 'Commit & Push' : 'Push';
  }
  return hasCommit ? 'Commit, Push & PR' : 'Push & Create PR';
}

export function resolveGpuiSidebarGitPromptDescription(
  action: Extract<SidebarGitAction, 'commit' | 'pr' | 'push'>
): string {
  if (action === 'commit') {
    return 'Review and commit changes.';
  }
  if (action === 'push') {
    return 'Push the current branch.';
  }
  return 'Create or open a pull request.';
}

export function resolveGpuiSidebarGitStartedTitle(
  action: Extract<SidebarGitAction, 'commit' | 'pr' | 'push'>,
  hasCommit: boolean
): string {
  if (action === 'pr') {
    return hasCommit ? 'Committing, pushing, and creating PR' : 'Pushing and creating PR';
  }
  if (action === 'push') {
    return hasCommit ? 'Committing and pushing' : 'Pushing';
  }
  return 'Committing';
}

export function resolveGpuiSidebarGitFinishedTitle(
  action: Extract<SidebarGitAction, 'commit' | 'pr' | 'push'>
): string {
  if (action === 'pr') {
    return 'Pull request ready';
  }
  return action === 'push' ? 'Push complete' : 'Commit complete';
}

export function formatGpuiGitAgentWorkflowTitle(title: string): string {
  const normalizedTitle = title.trim();
  return normalizedTitle.startsWith('Git:') ? normalizedTitle : `Git: ${normalizedTitle}`;
}

export function buildGpuiGitSyncWithMainPrompt(): string {
  return [
    'Please sync the latest main branch changes into this worktree so it can be merged back to main afterward.',
    '',
    'Use the current repository and branch in this terminal. Inspect Git state directly before changing anything.',
    '',
    'Requirements:',
    '- Fetch the latest remote refs before syncing.',
    '- Bring main into this worktree branch using the safest normal project workflow for this repository, such as merge or rebase only if that is clearly the repo convention.',
    '- Preserve work from both main and this worktree. If conflicts happen, resolve them without dropping code, behavior, or UX from either side.',
    '- After resolving conflicts, run the relevant checks you can run locally.',
    '- Leave the worktree branch ready for the user to merge back into main.',
    '- Stop and explain clearly if the repository state is unsafe or if a decision is needed.',
  ]
    .filter(Boolean)
    .join('\n');
}

export function buildGpuiGitPullRequestAgentPrompt(input: {
  filePaths?: readonly string[];
  hasExplicitFileSelection: boolean;
  hasCommit: boolean;
  message: string;
  selectedFiles: readonly string[];
}): string {
  const selectedFiles = input.selectedFiles.filter((filePath) => filePath.trim().length > 0);
  return [
    'Please complete the Git pull request flow in this terminal.',
    '',
    'Use the current repository checkout in this terminal. Inspect branch, remote, and PR state directly before changing anything.',
    '',
    'Do these steps visibly:',
    input.hasCommit
      ? input.hasExplicitFileSelection
        ? '- Stage and commit only the selected files listed below. Do not stage excluded files.'
        : '- Stage and commit all new/modified files.'
      : '- There were no working tree changes when the modal opened, so skip committing unless you find new user changes.',
    input.message
      ? '- Use the requested commit message below unless it is clearly invalid for the actual diff.'
      : '- Write a concise commit message that matches the staged diff.',
    '- If you encounter conflicts, rebases, merge state, or divergent local/remote changes, make sure not to lose changes from either side.',
    '- Push the current branch to origin, setting upstream if needed.',
    '- Create a GitHub pull request with `gh pr create --fill`, or open/show the existing PR if one already exists.',
    "- Stop and explain clearly if a command fails, authentication is missing, or a merge/rebase/conflict situation needs the user's decision.",
    '',
    input.hasExplicitFileSelection && selectedFiles.length > 0
      ? ['Selected files:', ...selectedFiles.map((filePath) => `- ${filePath}`)].join('\n')
      : 'Selected files: all new/modified files.',
    input.message ? `\nRequested commit message:\n${input.message}` : '',
  ]
    .filter(Boolean)
    .join('\n');
}

export function buildGpuiMergeConflictPrompt(input: {
  branch: string;
  mergeOutput: string;
  parentProject: GxserverProjectDomainState;
  worktree: GpuiWorktreeMetadata;
  worktreeProject: GxserverProjectDomainState;
}): string {
  const output = input.mergeOutput.trim();
  const worktreeName = input.worktree.name ?? input.worktreeProject.name ?? 'this worktree';
  const parentName = input.parentProject.name || input.worktree.parentProjectName || 'the main project';
  return [
    'Please handle the current Git merge conflicts on the main branch.',
    '',
    `Target project: ${parentName}`,
    'Target branch: main',
    `Merged worktree branch: ${input.branch}`,
    `Worktree: ${worktreeName}`,
    '',
    'Resolve the conflicts without losing any code, behavior, or UX from either side.',
    'Inspect the conflict markers, preserve the important intent from main and the worktree branch, run the relevant checks you can run locally, stage the resolved files, and leave the final state ready for review.',
    output ? `\nMerge output:\n${output}` : '',
  ]
    .filter(Boolean)
    .join('\n');
}

export function hasGpuiGitShortStatusChanges(stdout: string): boolean {
  return stdout.split('\n').some((line) => {
    const trimmed = line.trim();
    return trimmed.length > 0 && !trimmed.startsWith('##');
  });
}

export function normalizeGpuiWorktreeDeleteBranchName(
  currentBranch: string | null | undefined,
  fallbackBranch: string | null | undefined
): string | undefined {
  for (const candidate of [currentBranch, fallbackBranch]) {
    const branch = candidate?.trim();
    if (branch && branch !== 'HEAD' && branch !== 'detached') {
      return branch;
    }
  }
  return undefined;
}

export function gpuiWorktreeFolderSuffix(folderName: string, parentFolderName: string): string {
  const prefix = `${parentFolderName}-`;
  return parentFolderName && folderName.startsWith(prefix) ? folderName.slice(prefix.length) : folderName;
}

export function isGpuiManagedWorktreeBranch(branch: string | undefined): boolean {
  const slug = branch?.startsWith('ghostex/') ? branch.slice('ghostex/'.length) : undefined;
  return Boolean(slug && slug !== 'automation' && !slug.startsWith('automation/') && /^[a-z0-9-]+$/.test(slug));
}

export function gpuiWorktreeRenameUserVisibleErrorMessage(error: unknown): string {
  const message = error instanceof Error ? error.message.trim() : '';
  /*
  CDXC:Worktrees 2026-08-09-18:40:
  A daemon older than this feature cannot route the rename endpoint at all, and
  says so by naming the path back at the user — verified live as
  `notFound: "No gxserver endpoint for POST /api/renameWorktreeProject."`, though
  gxserver has more than one phrasing for it. Match on the endpoint path instead
  of on any one sentence: an error that names this route is always the daemon
  being older than the app, never anything the user did to their worktree.

  This is not hypothetical. A freshly built app attaches to whatever gxserver is
  already listening on 127.0.0.1:58744, which is normally the daemon the
  installed app started — so the very first run of a new build hits it.
  */
  if (message.includes('/api/renameWorktreeProject')) {
    return "This Ghostex build's background service is out of date. Quit Ghostex fully, reopen it, and try again.";
  }
  if (message && !message.includes('\\') && !message.includes('\n') && message.length <= 200) {
    return message;
  }
  return 'The gxserver worktree rename failed.';
}

export function normalizeGpuiProjectPath(value: unknown): string | undefined {
  return typeof value === 'string' && value.trim().length > 0 ? value.trim().replace(/\/+$/u, '') : undefined;
}

export function gpuiWorktreeListErrorText(error: unknown, folder: string | undefined): string {
  const message = error instanceof Error ? error.message.trim() : '';
  if (!message) {
    return 'Could not load gxserver worktrees.';
  }
  if (message.startsWith('Git could not read ')) {
    return message;
  }
  const where = normalizeGpuiProjectPath(folder) ?? 'the project folder';
  return `Could not load worktrees for ${where}: ${message}`;
}

export function normalizeGpuiWorktreeBaseBranches(
  branches: GxserverTypedOperationResult['branches']
): Array<{ current: boolean; name: string; remote: boolean }> {
  const seenBranches = new Set<string>();
  return (branches ?? []).flatMap((branch) => {
    const name = branch.name?.trim();
    if (!name || seenBranches.has(name)) {
      return [];
    }
    seenBranches.add(name);
    return [
      {
        current: branch.current === true,
        name,
        remote: branch.remote === true,
      },
    ];
  });
}

export function gpuiProjectNameFromPath(path: string): string {
  return gpuiProjectPathSeparators(path).split('/').filter(Boolean).at(-1) ?? 'Project';
}

export function gpuiWorktreeSlugFromPrompt(prompt: string): string {
  const firstWords = prompt
    .trim()
    .toLowerCase()
    .replace(/[`'"]/gu, '')
    .replace(/[^a-z0-9]+/gu, '-')
    .replace(/^-+|-+$/gu, '')
    .split('-')
    .filter(Boolean)
    .slice(0, 6)
    .join('-');
  return (firstWords || 'worktree').slice(0, 48).replace(/-+$/u, '') || 'worktree';
}

export function gpuiWorktreeUserVisibleErrorMessage(error: unknown): string {
  const message = error instanceof Error ? error.message.trim() : '';
  if (
    message &&
    !message.includes('/') &&
    !message.includes('\\') &&
    !message.includes('\n') &&
    message.length <= 160
  ) {
    return message;
  }
  return 'The gxserver worktree operation failed.';
}

function gpuiProjectPathSeparators(path: string): string {
  return /^(?:[a-z]:[\\/]|\\\\|\/\/)/iu.test(path) ? path.replace(/\\/gu, '/') : path;
}

function stringFromRecord(record: Record<string, unknown> | undefined, key: string): string | undefined {
  const value = record?.[key];
  return typeof value === 'string' && value.trim().length > 0 ? value.trim() : undefined;
}

function booleanFromRecord(record: Record<string, unknown> | undefined, key: string): boolean | undefined {
  const value = record?.[key];
  return typeof value === 'boolean' ? value : undefined;
}

export function normalizeGpuiExistingWorktreeOptions(
  worktrees: unknown
): Array<{
  branch: string;
  isCurrentProject: boolean;
  isRegistered: boolean;
  name: string;
  path: string;
  worktreeKey: string;
}> {
  if (!Array.isArray(worktrees)) {
    return [];
  }
  return worktrees.flatMap((entry) => {
    if (!entry || typeof entry !== 'object') {
      return [];
    }
    const worktree = entry as Record<string, unknown>;
    const path = normalizeGpuiProjectPath(worktree.path);
    const name = stringFromRecord(worktree, 'name') ?? (path ? gpuiProjectNameFromPath(path) : undefined);
    const worktreeKey = stringFromRecord(worktree, 'worktreeKey');
    if (!path || !name || !worktreeKey) {
      return [];
    }
    return [
      {
        branch: stringFromRecord(worktree, 'branch') ?? '',
        isCurrentProject: booleanFromRecord(worktree, 'isCurrentProject') === true,
        isRegistered: booleanFromRecord(worktree, 'isRegistered') === true,
        name,
        path,
        worktreeKey,
      },
    ];
  });
}
