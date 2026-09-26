/**
 * FROZEN copies of the old app runtime's Git decisions, taken from
 * apps/desktop/sidebar/gxserver-runtime/git/actions-and-confirm.ts (`runSidebarGitAction`,
 * `runRemoteSidebarGitAction`), git/worktree-merge-and-review.ts (`promptSidebarGitActionReview`,
 * `resolveTrustedGitReviewFileSelection`) and git/diff-stats.ts (`scheduleGitPollingCycle`) at
 * commit 4140ffd0e, the last commit before the app runtime port's F5 deleted them. The branch
 * order and every literal are the originals'; the side effects are returned as records instead of
 * performed, which is the only edit. Deleted with the runtime in step 3.
 */
/* eslint-disable */
import type { SidebarGitAction, SidebarGitState } from '@/packages/shared/sidebar-git';
import { hasSidebarGitRemoteCommitDelta } from '@/packages/shared/sidebar-git';
import {
  GPUI_GIT_MULTICOMMIT_RELEASE_PROMPT,
  GPUI_GIT_RELEASE_ONLY_PROMPT,
  buildGpuiGitSyncWithMainPrompt,
  normalizeGpuiRelativeGitFilePath,
  resolveGpuiSidebarGitConfirmLabel,
  resolveGpuiSidebarGitPromptDescription,
} from './helpers';

export type FrozenStep =
  | { kind: 'toast'; level: string; title: string; description?: string }
  | { kind: 'promptWorkflow'; title: string; prompt: string }
  | { kind: 'review'; action: string }
  | { kind: 'openExistingPullRequest' }
  | { kind: 'pullRequestAgentWorkflow' }
  | { kind: 'mutation'; mutation: 'sync' | 'push'; started: string; finished: string };

/** `runSidebarGitAction`'s decisions before its Git state read. */
export function frozenPlanBeforeRead(action: SidebarGitAction): FrozenStep | undefined {
  if (action === 'multiRelease') {
    return { kind: 'promptWorkflow', title: 'Multicommit & Release', prompt: GPUI_GIT_MULTICOMMIT_RELEASE_PROMPT };
  }
  if (action === 'release') {
    return { kind: 'promptWorkflow', title: 'Release', prompt: GPUI_GIT_RELEASE_ONLY_PROMPT };
  }
  return undefined;
}

/** `runSidebarGitAction` (local) and `runRemoteSidebarGitAction` after the read. */
export function frozenPlanAfterRead(
  action: SidebarGitAction,
  gitState: SidebarGitState,
  isWorktree: boolean,
  remote: boolean
): FrozenStep | undefined {
  if (!gitState.isRepo) {
    return remote
      ? {
          kind: 'toast',
          level: 'warning',
          title: 'Remote Git unavailable',
          description: 'Open a Git repository on the remote machine to use Git actions.',
        }
      : { kind: 'toast', level: 'warning', title: 'Git unavailable', description: 'Open a Git repository to use Git actions.' };
  }
  if (action === 'syncMain') {
    if (!isWorktree) {
      return remote
        ? {
            kind: 'toast',
            level: 'warning',
            title: 'Remote worktree unavailable',
            description: 'Open a remote worktree project to sync with main.',
          }
        : {
            kind: 'toast',
            level: 'warning',
            title: 'Worktree unavailable',
            description: 'Open a worktree project to sync with main.',
          };
    }
    return { kind: 'promptWorkflow', title: 'Sync with Main', prompt: buildGpuiGitSyncWithMainPrompt() };
  }
  if (action === 'syncRemote') {
    if (!hasSidebarGitRemoteCommitDelta(gitState)) {
      return { kind: 'toast', level: 'info', title: 'Remote already synced' };
    }
    return { kind: 'mutation', mutation: 'sync', started: 'Syncing remote', finished: 'Remote sync complete' };
  }
  if (isWorktree && (action === 'commit' || action === 'push' || action === 'pr')) {
    return { kind: 'review', action };
  }
  if (action === 'pr') {
    if (gitState.pr?.state === 'open') {
      return { kind: 'openExistingPullRequest' };
    }
    if (!gitState.hasGitHubCli) {
      return remote
        ? {
            kind: 'toast',
            level: 'warning',
            title: 'Remote GitHub CLI unavailable',
            description: 'Install GitHub CLI on the remote machine before creating a pull request.',
          }
        : {
            kind: 'toast',
            level: 'warning',
            title: 'GitHub CLI unavailable',
            description: 'Install GitHub CLI before creating a pull request.',
          };
    }
    if (gitState.hasWorkingTreeChanges) {
      return { kind: 'review', action: 'pr' };
    }
    return { kind: 'pullRequestAgentWorkflow' };
  }
  if (action === 'commit') {
    if (!gitState.hasWorkingTreeChanges) {
      return { kind: 'toast', level: 'info', title: remote ? 'No remote changes to commit' : 'No changes to commit' };
    }
    return { kind: 'review', action: 'commit' };
  }
  if (action === 'push') {
    if (gitState.hasWorkingTreeChanges) {
      return { kind: 'review', action: 'push' };
    }
    return {
      kind: 'mutation',
      mutation: 'push',
      started: 'Pushing',
      finished: remote ? 'Remote push complete' : 'Push complete',
    };
  }
  return undefined;
}

/** The `gitCommitDraft` `promptSidebarGitActionReview` / `promptRemoteSidebarGitActionReview` open. */
export function frozenReviewDraft(input: {
  action: Extract<SidebarGitAction, 'commit' | 'pr' | 'push'>;
  agentId?: string;
  gitState: SidebarGitState;
  isWorktree: boolean;
  remote: boolean;
  requestId: string;
  worktreeName?: string;
}): Record<string, unknown> {
  const hasCommit = input.gitState.hasWorkingTreeChanges;
  return {
    action: input.action,
    agentId: input.agentId,
    branch: input.gitState.branch,
    changedFiles: input.gitState.files,
    confirmLabel: resolveGpuiSidebarGitConfirmLabel(input.action, hasCommit),
    deleteWorktreeAfterDefault: false,
    description: hasCommit
      ? input.remote
        ? 'Review and confirm your remote commit. Leave the message blank to auto-generate one.'
        : 'Review and confirm your commit. Leave the message blank to auto-generate one.'
      : resolveGpuiSidebarGitPromptDescription(input.action),
    isDefaultRef: input.gitState.branch === 'main' || input.gitState.branch === 'master',
    isWorktree: input.isWorktree,
    requestId: input.requestId,
    showCommitMessage: hasCommit,
    suggestedBody: undefined,
    suggestedSubject: '',
    type: 'promptGitCommit',
    worktreeName: input.worktreeName,
  };
}

/** `resolveTrustedGitReviewFileSelection`. */
export function frozenTrustedSelection(
  files: readonly { path: string }[],
  filePaths?: readonly string[]
): { explicit: boolean; filePaths: string[] } | { error: string } {
  const explicit = filePaths !== undefined;
  const candidatePaths = explicit ? filePaths : files.map((file) => file.path);
  const allowedPaths = new Map(files.map((file) => [file.path, file.path]));
  const selectedPaths: string[] = [];
  for (const filePath of candidatePaths) {
    const normalizedPath = normalizeGpuiRelativeGitFilePath(filePath);
    const trustedPath = normalizedPath ? allowedPaths.get(normalizedPath) : undefined;
    if (!trustedPath) {
      return { error: 'Selected file is not part of the current Git review.' };
    }
    if (!selectedPaths.includes(trustedPath)) {
      selectedPaths.push(trustedPath);
    }
  }
  if (selectedPaths.length === 0) {
    return { error: 'Select at least one changed file.' };
  }
  return { explicit, filePaths: selectedPaths };
}

/** `scheduleGitPollingCycle`'s schedule over sorted target keys. */
export function frozenPollCycle(keys: readonly string[]): { cycleMs: number; probes: Array<[number, string]> } {
  const targets = [...new Set(keys)].sort((left, right) => left.localeCompare(right));
  const cycleLengthMs = Math.max(15 * 1000, targets.length * 1000);
  const staggerStepMs = cycleLengthMs / Math.max(1, targets.length);
  return { cycleMs: cycleLengthMs, probes: targets.map((key, index) => [Math.floor(index * staggerStepMs), key]) };
}
