/*
CDXC:GxserverRuntimeSplit 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import {
  GPUI_BACKGROUND_COMMIT_MESSAGE_DEFAULT_AGENT_IDS,
  GPUI_TITLEBAR_GIT_ACTIONS,
  GPUI_TITLEBAR_GIT_ACTION_MESSAGE_TYPE,
  GPUI_TITLEBAR_GIT_ACTION_MESSAGE_VERSION,
  GPUI_TITLEBAR_GIT_MENU_STATE_MESSAGE_TYPE,
  GPUI_TITLEBAR_GIT_MENU_STATE_MESSAGE_VERSION,
  GPUI_UNTRACKED_LINE_COUNT_BATCH_SIZE,
} from '../constants';
import type {
  GpuiGitCommitModalCommand,
  GpuiPendingGitCommitRequest,
  GpuiRemoteCreatePullRequestResult,
  GpuiRemoteProjectReference,
  GpuiWorktreeMetadata,
} from '../types-and-protocol';
import type { GxserverCreatePullRequestResult, GxserverProjectDomainState } from '@/packages/shared/gxserver-protocol';
import type { SidebarProjectDiffStats } from '@/packages/shared/project-diff-stats';
import type { SidebarAgentButton } from '@/packages/shared/sidebar-agents';
import { isDefaultSidebarAgentId } from '@/packages/shared/sidebar-agents';
import type { SidebarGitAction, SidebarGitChangedFile, SidebarGitState } from '@/packages/shared/sidebar-git';
import {
  buildSidebarGitMenuItems,
  getSidebarGitDisabledReason,
  hasSidebarGitRemoteCommitDelta,
  resolveSidebarGitPrimaryActionState,
} from '@/packages/shared/sidebar-git';

export class GpuiUserVisibleGitError extends Error {}

export function hasGpuiGitShortStatusChanges(stdout: string): boolean {
  return stdout.split('\n').some((line) => {
    const trimmed = line.trim();
    return trimmed.length > 0 && !trimmed.startsWith('##');
  });
}

export function parseGpuiGitCommitModalCommand(payload: unknown): GpuiGitCommitModalCommand | undefined {
  if (!payload || typeof payload !== 'object') {
    return undefined;
  }
  const record = payload as Record<string, unknown>;
  const stringField = (field: string, maxChars: number, allowEmpty = false): string | undefined => {
    const value = record[field];
    return typeof value === 'string' && (allowEmpty || value.length > 0) && value.length <= maxChars
      ? value
      : undefined;
  };
  const requestId = stringField('requestId', 120);
  if (!requestId) {
    return undefined;
  }
  const agentId = stringField('agentId', 300);
  switch (record.type) {
    case 'confirmSidebarGitCommit':
    case 'confirmSidebarGitDirectMerge': {
      const message = stringField('message', 20_000, true);
      if (message === undefined) {
        return undefined;
      }
      const filePaths = Array.isArray(record.filePaths)
        ? record.filePaths.filter(
            (value): value is string => typeof value === 'string' && value.length > 0 && value.length <= 1024
          )
        : undefined;
      return {
        agentId,
        deleteWorktreeAfter: record.deleteWorktreeAfter === true,
        filePaths,
        message,
        requestId,
        type: record.type,
        ...(record.type === 'confirmSidebarGitCommit' ? { commitOnNewRef: record.commitOnNewRef === true } : {}),
      };
    }
    case 'runSidebarGitMultipleCommits':
      return { agentId, requestId, type: 'runSidebarGitMultipleCommits' };
    case 'openSidebarGitChangedFileDiff': {
      const filePath = stringField('filePath', 1024);
      return filePath ? { filePath, requestId, type: 'openSidebarGitChangedFileDiff' } : undefined;
    }
    case 'cancelSidebarGitCommit':
      return { requestId, type: 'cancelSidebarGitCommit' };
    default:
      return undefined;
  }
}

export function parseGpuiTitlebarGitAction(payload: unknown): SidebarGitAction | 'refresh' | undefined {
  // Native titlebar Git menu selections carry a fixed action selector only;
  // reject everything else so this bridge can never smuggle command text,
  // paths, or ids into the Git pipeline.
  if (!payload || typeof payload !== 'object') {
    return undefined;
  }
  const record = payload as Record<string, unknown>;
  if (
    record.type !== GPUI_TITLEBAR_GIT_ACTION_MESSAGE_TYPE ||
    record.version !== GPUI_TITLEBAR_GIT_ACTION_MESSAGE_VERSION ||
    typeof record.action !== 'string'
  ) {
    return undefined;
  }
  if (record.action === 'refresh') {
    return 'refresh';
  }
  return GPUI_TITLEBAR_GIT_ACTIONS.has(record.action as SidebarGitAction)
    ? (record.action as SidebarGitAction)
    : undefined;
}

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

export function parseGpuiGitNumstatFiles(stdout: string): SidebarGitChangedFile[] {
  return stdout
    .trim()
    .split('\n')
    .filter(Boolean)
    .flatMap((line) => {
      const [additions, deletions, ...pathParts] = line.split(/\s+/);
      const path = normalizeGpuiRelativeGitFilePath(pathParts.join(' '));
      if (!path) {
        return [];
      }
      return [
        {
          additions: normalizeGpuiGitNumstatNumber(additions),
          deletions: normalizeGpuiGitNumstatNumber(deletions),
          path,
        },
      ];
    });
}

export function parseGpuiGitStatusPorcelainFiles(stdout: string): SidebarGitChangedFile[] {
  return stdout
    .split(/\r?\n/)
    .filter((line) => line.length >= 4)
    .flatMap((line) => {
      const rawPath = line.slice(3).trim();
      const path = normalizeGpuiRelativeGitFilePath(
        rawPath.includes(' -> ') ? (rawPath.split(' -> ').at(-1) ?? '') : rawPath
      );
      return path ? [{ additions: 0, deletions: 0, path }] : [];
    });
}

export function mergeGpuiGitChangedFiles(files: readonly SidebarGitChangedFile[]): SidebarGitChangedFile[] {
  const mergedFiles = new Map<string, SidebarGitChangedFile>();
  for (const file of files) {
    const existing = mergedFiles.get(file.path);
    mergedFiles.set(file.path, {
      additions: Math.max(existing?.additions ?? 0, file.additions),
      deletions: Math.max(existing?.deletions ?? 0, file.deletions),
      path: file.path,
    });
  }
  return [...mergedFiles.values()];
}

export function normalizeGpuiGitNumstatNumber(value: string | undefined): number {
  if (!value || value === '-') {
    return 0;
  }
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : 0;
}

export function summarizeGpuiGitChangedFiles(files: readonly SidebarGitChangedFile[]): {
  additions: number;
  deletions: number;
} {
  return files.reduce(
    (stats, file) => ({
      additions: stats.additions + file.additions,
      deletions: stats.deletions + file.deletions,
    }),
    { additions: 0, deletions: 0 }
  );
}

export function parseGpuiGitHubPullRequest(stdout: string, success: boolean): SidebarGitState['pr'] {
  if (!success || !stdout.trim()) {
    return null;
  }
  try {
    const candidate = JSON.parse(stdout) as Partial<NonNullable<SidebarGitState['pr']>>;
    const state = String(candidate.state || '').toLowerCase();
    if (!candidate.url || !candidate.title || !['open', 'closed', 'merged'].includes(state)) {
      return null;
    }
    return {
      number: typeof candidate.number === 'number' ? candidate.number : undefined,
      state: state as NonNullable<SidebarGitState['pr']>['state'],
      title: candidate.title,
      url: candidate.url,
    };
  } catch {
    return null;
  }
}

export function isGpuiConfirmedOpenPullRequest(result: GxserverCreatePullRequestResult): boolean {
  return (
    result.ok === true &&
    result.pr?.state === 'open' &&
    typeof result.pr.url === 'string' &&
    /^https:\/\/github\.com\/[^/\s]+\/[^/\s]+\/pull\/\d+$/u.test(result.pr.url)
  );
}

export function isGpuiConfirmedOpenRemotePullRequest(result: GpuiRemoteCreatePullRequestResult): boolean {
  return result.ok === true && result.pr?.state === 'open';
}

export function normalizeGpuiGitHubRemoteUrl(remoteUrl: string): string | undefined {
  const trimmed =
    remoteUrl
      .trim()
      .split(/\s+/)[0]
      ?.replace(/\.git$/u, '') ?? '';
  if (!trimmed) {
    return undefined;
  }
  const sshMatch = /^git@github\.com:(?<path>[^#?]+)$/u.exec(trimmed);
  const sshPath = sshMatch?.groups?.path;
  if (sshPath) {
    return `https://github.com/${sshPath.replace(/^\/+/u, '').replace(/\.git$/u, '')}`;
  }
  try {
    const parsed = new URL(trimmed);
    if (parsed.hostname !== 'github.com') {
      return undefined;
    }
    const repoPath = parsed.pathname.replace(/^\/+/u, '').replace(/\.git$/u, '');
    return repoPath ? `https://github.com/${repoPath}` : undefined;
  } catch {
    return undefined;
  }
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

/*
CDXC:GPUISidebarGit 2026-06-24-16:11:
Blank commit-message generation in GPUI mirrors the native background prompt
support set. Built-in agents that do not expose a safe headless prompt mode
must fail explicitly, while configured non-default custom agents may use their
stored command through the local gxserver generation endpoint.
*/
export function supportsGpuiBackgroundCommitMessageGeneration(agent: SidebarAgentButton): boolean {
  return GPUI_BACKGROUND_COMMIT_MESSAGE_DEFAULT_AGENT_IDS.has(agent.agentId) || !isDefaultSidebarAgentId(agent.agentId);
}

export function gpuiUserVisibleGitErrorMessage(error: unknown, fallback: string): string {
  /*
  CDXC:GPUISidebarGit 2026-07-11-05:08:
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

export function sanitizeGpuiSidebarGitBranchName(subject: string): string {
  return (
    subject
      .toLowerCase()
      .normalize('NFKD')
      .replace(/[^\w\s-]/gu, '')
      .trim()
      .replace(/[\s_]+/gu, '-')
      .replace(/-+/gu, '-')
      .replace(/^-|-$/gu, '')
      .slice(0, 48) || `change-${Date.now().toString(36)}`
  );
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

export function isMissingGpuiBeadsDatabaseError(message: string): boolean {
  return /no beads database found|run ['"]?bd init['"]?|not initialized|no storage/iu.test(message);
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

export function hasGpuiGxserverShortStatusChanges(stdout: string): boolean {
  return stdout.split('\n').some((line) => {
    const trimmed = line.trim();
    return trimmed.length > 0 && !trimmed.startsWith('##');
  });
}

export function createGpuiGitToastId(): string {
  return `toast-gpui-git-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}

export function chunkUntrackedLineCountPaths(paths: readonly string[]): string[][] {
  const chunks: string[][] = [];
  for (let start = 0; start < paths.length; start += GPUI_UNTRACKED_LINE_COUNT_BATCH_SIZE) {
    chunks.push(paths.slice(start, start + GPUI_UNTRACKED_LINE_COUNT_BATCH_SIZE));
  }
  return chunks;
}

/*
A pending commit request carries `remoteReference` only for remote projects, and
the remote confirm handlers require it. A bare `if (pending.remoteReference)`
narrows the property but not the request, so state that relationship once here
instead of at every remote branch.
*/
export function isGpuiRemotePendingGitCommitRequest(
  pending: GpuiPendingGitCommitRequest
): pending is GpuiPendingGitCommitRequest & { remoteReference: GpuiRemoteProjectReference } {
  return pending.remoteReference !== undefined;
}

export function haveSameSidebarProjectDiffStats(
  left: SidebarProjectDiffStats,
  right: SidebarProjectDiffStats
): boolean {
  return (
    left.additions === right.additions &&
    left.deletions === right.deletions &&
    left.files === right.files &&
    left.isLoading === right.isLoading &&
    left.isRepo === right.isRepo
  );
}
