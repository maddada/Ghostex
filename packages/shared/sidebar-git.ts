export type SidebarGitAction = 'commit' | 'push' | 'pr' | 'syncRemote' | 'syncMain' | 'multiRelease' | 'release';

export type SidebarGitChangedFile = {
  additions: number;
  deletions: number;
  path: string;
};

export type SidebarGitFileDiffDraft = {
  additions?: number;
  deletions?: number;
  filePath: string;
  patch: string;
};

export type SidebarGitPullRequest = {
  number?: number;
  state: 'open' | 'closed' | 'merged';
  title: string;
  url: string;
};

export type SidebarGitState = {
  additions: number;
  aheadCount: number;
  behindCount: number;
  branch: string | null;
  confirmSuggestedCommit: boolean;
  deletions: number;
  generateCommitBody: boolean;
  hasCheckedGitHubRemote: boolean;
  hasGitHubCli: boolean;
  hasGitHubRemote: boolean;
  hasOriginRemote: boolean;
  hasUpstream: boolean;
  hasWorkingTreeChanges: boolean;
  isBusy: boolean;
  isRepo: boolean;
  files: SidebarGitChangedFile[];
  isWorktree: boolean;
  pr: SidebarGitPullRequest | null;
  primaryAction: SidebarGitAction;
  worktreeName?: string;
};

export type SidebarGitMenuItem = {
  action: SidebarGitAction;
  disabled: boolean;
  disabledReason?: string;
  label: string;
};

export type SidebarGitActionCategory = 'direct' | 'agent';

export type SidebarGitPrimaryActionState = {
  action: SidebarGitAction;
  disabled: boolean;
  disabledReason?: string;
  label: string;
};

export const DEFAULT_SIDEBAR_GIT_ACTION: SidebarGitAction = 'commit';

export function createDefaultSidebarGitState(
  primaryAction: SidebarGitAction = DEFAULT_SIDEBAR_GIT_ACTION,
  confirmSuggestedCommit = false,
  generateCommitBody = true
): SidebarGitState {
  return {
    additions: 0,
    aheadCount: 0,
    behindCount: 0,
    branch: null,
    confirmSuggestedCommit,
    deletions: 0,
    generateCommitBody,
    hasCheckedGitHubRemote: false,
    hasGitHubCli: false,
    hasGitHubRemote: false,
    hasOriginRemote: false,
    hasUpstream: false,
    hasWorkingTreeChanges: false,
    isBusy: false,
    isRepo: false,
    files: [],
    isWorktree: false,
    pr: null,
    primaryAction,
  };
}

export function hasSidebarGitDiffStat(state: Pick<SidebarGitState, 'additions' | 'deletions'>): boolean {
  return state.additions > 0 || state.deletions > 0;
}

export function hasSidebarGitRemoteCommitDelta(state: Pick<SidebarGitState, 'aheadCount' | 'behindCount'>): boolean {
  /**
   * CDXC:Git 2026-06-16-18:41:
   * Remote sync availability and titlebar copy share one normalized ahead/behind
   * check so a synced branch cannot run the sync action while the Commits row
   * shows ↑0 ↓0.
   */
  return Math.max(0, Math.trunc(state.behindCount)) > 0 || Math.max(0, Math.trunc(state.aheadCount)) > 0;
}

export function normalizeSidebarGitAction(candidate: string | undefined): SidebarGitAction {
  return candidate === 'push' || candidate === 'pr' ? candidate : 'commit';
}

export function buildSidebarGitMenuItems(state: SidebarGitState): SidebarGitMenuItem[] {
  /**
   * CDXC:Worktrees 2026-05-30-05:13:
   * Sync with Main is a worktree-only Git workflow. Show it beside Create PR in
   * the Git dropdown only when the active project is a worktree, because main
   * projects do not need to pull main into themselves before worktree merge.
   */
  return [
    buildSidebarGitMenuItem('commit', 'Commit', state),
    buildSidebarGitMenuItem('push', 'Push', state),
    buildSidebarGitMenuItem('pr', state.pr?.state === 'open' ? 'View PR' : 'Create PR', state),
    ...(state.isWorktree ? [buildSidebarGitMenuItem('syncMain', 'Sync with Main', state)] : []),
    buildSidebarGitMenuItem('multiRelease', 'Multicommit & Release', state),
    buildSidebarGitMenuItem('release', 'Release', state),
  ];
}

export function getSidebarGitActionCategory(
  state: Pick<SidebarGitState, 'pr'>,
  action: SidebarGitAction
): SidebarGitActionCategory {
  /**
   * CDXC:Git 2026-06-02-13:41:
   * The Git dropdown separates direct gxserver-backed Git operations from
   * agent-run workflows. Create PR belongs with agent workflows, while View PR
   * stays direct because it only opens the existing pull request.
   */
  if (action === 'syncMain' || action === 'multiRelease' || action === 'release') {
    return 'agent';
  }
  if (action === 'pr' && state.pr?.state !== 'open') {
    return 'agent';
  }
  return 'direct';
}

export function resolveSidebarGitPrimaryActionState(state: SidebarGitState): SidebarGitPrimaryActionState {
  const action = normalizeSidebarGitAction(state.primaryAction);
  const disabledReason = getSidebarGitDisabledReason(state, action);

  if (action === 'push') {
    return {
      action,
      disabled: disabledReason !== undefined,
      disabledReason,
      label: state.hasWorkingTreeChanges ? 'Commit & Push' : 'Push',
    };
  }

  if (action === 'pr') {
    return {
      action,
      disabled: disabledReason !== undefined,
      disabledReason,
      label: resolveSidebarGitPrPrimaryLabel(state),
    };
  }

  return {
    action,
    disabled: disabledReason !== undefined,
    disabledReason,
    label: 'Commit',
  };
}

export function getSidebarGitDisabledReason(state: SidebarGitState, action: SidebarGitAction): string | undefined {
  if (state.isBusy) {
    return 'Git action already running.';
  }

  if (!state.isRepo) {
    return 'Open a Git repository to use Git actions.';
  }

  if (action === 'commit') {
    return state.hasWorkingTreeChanges ? undefined : 'No working tree changes to commit.';
  }

  if (action === 'multiRelease' || action === 'release') {
    return undefined;
  }

  if (!state.branch) {
    return action === 'syncRemote'
      ? 'Create and checkout a branch before syncing.'
      : 'Create and checkout a branch before pushing or creating a PR.';
  }

  if (action === 'syncMain') {
    return state.isWorktree ? undefined : 'Open a worktree project to sync with main.';
  }

  if (action === 'syncRemote') {
    /**
     * CDXC:Git 2026-06-16-07:31:
     * The macOS titlebar sync row is a direct remote branch sync, not the
     * worktree-only Sync with Main agent workflow. Enable it for any checked-out
     * branch with an upstream or an origin remote so normal projects can pull
     * and push from the titlebar menu.
     */
    if (state.hasUpstream || state.hasOriginRemote) {
      return undefined;
    }
    return 'Add an "origin" remote or set an upstream before syncing.';
  }

  if (state.behindCount > 0) {
    return 'Branch is behind upstream. Pull or rebase first.';
  }

  if (action === 'push') {
    if (state.hasWorkingTreeChanges) {
      return undefined;
    }

    if (state.aheadCount > 0) {
      return undefined;
    }

    if (!state.hasUpstream && state.hasOriginRemote) {
      return undefined;
    }

    if (!state.hasOriginRemote) {
      return 'Add an "origin" remote before pushing.';
    }

    return 'No local commits to push.';
  }

  if (!state.hasGitHubCli) {
    return 'Install GitHub CLI to create or view pull requests.';
  }

  if (state.hasWorkingTreeChanges) {
    return undefined;
  }

  if (state.pr?.state === 'open') {
    return undefined;
  }

  if (state.aheadCount > 0) {
    return undefined;
  }

  if (!state.hasUpstream && state.hasOriginRemote) {
    return undefined;
  }

  if (!state.hasOriginRemote) {
    return 'Add an "origin" remote before creating a PR.';
  }

  if (state.hasUpstream) {
    return undefined;
  }

  return 'No branch state available for PR creation.';
}

function buildSidebarGitMenuItem(action: SidebarGitAction, label: string, state: SidebarGitState): SidebarGitMenuItem {
  const disabledReason = getSidebarGitDisabledReason(state, action);
  return {
    action,
    disabled: disabledReason !== undefined,
    disabledReason,
    label,
  };
}

function resolveSidebarGitPrPrimaryLabel(state: SidebarGitState): string {
  const needsPush = state.hasWorkingTreeChanges || state.aheadCount > 0 || !state.hasUpstream;
  if (state.hasWorkingTreeChanges) {
    return 'Commit, Push & PR';
  }
  if (state.pr?.state === 'open' && !needsPush) {
    return 'View PR';
  }
  if (needsPush) {
    return state.pr?.state === 'open' ? 'Push & View PR' : 'Push & Create PR';
  }
  return state.pr?.state === 'open' ? 'View PR' : 'Create PR';
}
