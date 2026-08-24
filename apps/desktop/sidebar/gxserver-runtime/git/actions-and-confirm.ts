/*
CDXC:GxserverRuntimeSplit 2026-08-23:
Directory split of gxserver-runtime/git.ts (~3,251 lines) into git/. This
slice covers dispatching `runSidebarGitAction` (local and remote) and
confirming the commit / direct-merge review modals it opens. See `index.ts`
for how the runtime's Git methods are recombined.
*/
import {
  GPUI_GIT_MULTICOMMIT_RELEASE_PROMPT,
  GPUI_GIT_RELEASE_ONLY_PROMPT,
  GPUI_REMOTE_MERGE_CONFLICT_PROMPT,
} from '../constants';
import type { GpuiSidebarRuntime } from '../core';
import {
  GpuiUserVisibleGitError,
  buildGpuiGitSyncWithMainPrompt,
  createGpuiGitToastId,
  formatGpuiGitAgentWorkflowTitle,
  gpuiUserVisibleGitErrorMessage,
  isGpuiConfirmedOpenPullRequest,
  isGpuiConfirmedOpenRemotePullRequest,
  isGpuiRemotePendingGitCommitRequest,
  resolveGpuiSidebarGitConfirmLabel,
  resolveGpuiSidebarGitFinishedTitle,
  resolveGpuiSidebarGitPromptDescription,
  resolveGpuiSidebarGitStartedTitle,
} from '../helpers/git';
import { stringFromRecord } from '../helpers/records';
import {
  createGpuiRemotePresentationProjectId,
  parseGpuiRemotePresentationGroupId,
  parseGpuiRemotePresentationProjectId,
} from '../helpers/remote-presentation';
import { normalizeGpuiWorktreeMetadata, normalizeGpuiWorktreeParentProjectId } from '../helpers/worktrees';
import type {
  GpuiPendingGitCommitRequest,
  GpuiRemoteProjectReference,
  GpuiRemoteProjectScope,
  GpuiTrustedGitReviewFileSelection,
} from '../types-and-protocol';
import type { SidebarPromptGitCommitMessage, SidebarToExtensionMessage } from '@/packages/shared/session-grid-contract';
import type { SidebarGitAction, SidebarGitState } from '@/packages/shared/sidebar-git';
import { hasSidebarGitRemoteCommitDelta } from '@/packages/shared/sidebar-git';

export const gpuiSidebarRuntimeGitActionsAndConfirmMethods = {
  async runRemoteSidebarGitAction(
    this: GpuiSidebarRuntime,
    message: Extract<SidebarToExtensionMessage, { type: 'runSidebarGitAction' }>,
    remoteScope: GpuiRemoteProjectScope
  ): Promise<void> {
    if (message.action === 'multiRelease') {
      await this.runRemoteSidebarGitPromptAction(
        remoteScope,
        'Multicommit & Release',
        GPUI_GIT_MULTICOMMIT_RELEASE_PROMPT
      );
      return;
    }
    if (message.action === 'release') {
      await this.runRemoteSidebarGitPromptAction(remoteScope, 'Release', GPUI_GIT_RELEASE_ONLY_PROMPT);
      return;
    }

    const gitState = await this.readRemoteSidebarGitState(remoteScope);
    if (!gitState.isRepo) {
      this.postRemoteToast('warning', 'Remote Git unavailable', {
        description: 'Open a Git repository on the remote machine to use Git actions.',
      });
      return;
    }

    if (message.action === 'syncMain') {
      if (!normalizeGpuiWorktreeParentProjectId(remoteScope.project.worktree)) {
        this.postRemoteToast('warning', 'Remote worktree unavailable', {
          description: 'Open a remote worktree project to sync with main.',
        });
        return;
      }
      await this.runRemoteSidebarGitPromptAction(remoteScope, 'Sync with Main', buildGpuiGitSyncWithMainPrompt());
      return;
    }

    if (message.action === 'syncRemote') {
      if (!hasSidebarGitRemoteCommitDelta(gitState)) {
        this.postRemoteToast('info', 'Remote already synced');
        return;
      }
      await this.runRemoteGitMutation(remoteScope, 'Syncing remote', 'Remote sync complete', async () => {
        await this.syncRemoteCurrentBranchWithRemote(remoteScope, gitState);
      });
      return;
    }

    if (
      normalizeGpuiWorktreeMetadata(remoteScope.project.worktree) &&
      (message.action === 'commit' || message.action === 'push' || message.action === 'pr')
    ) {
      this.promptRemoteSidebarGitActionReview(remoteScope, gitState, message.action);
      return;
    }

    if (message.action === 'pr') {
      if (gitState.pr?.state === 'open') {
        this.postRemoteProjectNativeAction('openRemoteExistingPullRequestInBrowser', remoteScope, message);
        return;
      }
      if (!gitState.hasGitHubCli) {
        this.postRemoteToast('warning', 'Remote GitHub CLI unavailable', {
          description: 'Install GitHub CLI on the remote machine before creating a pull request.',
        });
        return;
      }
      if (gitState.hasWorkingTreeChanges) {
        this.promptRemoteSidebarGitActionReview(remoteScope, gitState, 'pr');
        return;
      }
      await this.runRemoteSidebarGitPullRequestAgentWorkflow({
        gitState,
        hasCommit: false,
        hasExplicitFileSelection: false,
        message: '',
        remoteScope,
      });
      return;
    }

    if (message.action === 'commit') {
      if (!gitState.hasWorkingTreeChanges) {
        this.postRemoteToast('info', 'No remote changes to commit');
        return;
      }
      this.promptRemoteSidebarGitActionReview(remoteScope, gitState, 'commit');
      return;
    }

    if (message.action === 'push') {
      if (gitState.hasWorkingTreeChanges) {
        this.promptRemoteSidebarGitActionReview(remoteScope, gitState, 'push');
        return;
      }
      await this.runRemoteGitMutation(remoteScope, 'Pushing', 'Remote push complete', async () => {
        await this.pushRemoteCurrentBranch(remoteScope, gitState);
      });
    }
  },

  promptRemoteSidebarGitActionReview(
    this: GpuiSidebarRuntime,
    remoteScope: GpuiRemoteProjectScope,
    gitState: SidebarGitState,
    action: Extract<SidebarGitAction, 'commit' | 'pr' | 'push'>
  ): void {
    const requestId = `gpui-remote-git-action-${Date.now().toString(36)}`;
    const hasCommit = gitState.hasWorkingTreeChanges;
    this.pendingGitCommitRequests.set(requestId, {
      action,
      files: [...gitState.files],
      hasCommit,
      projectId: createGpuiRemotePresentationProjectId(remoteScope.machineId, remoteScope.projectId),
      remoteReference: {
        machineId: remoteScope.machineId,
        projectId: remoteScope.projectId,
      },
      remoteTitle: remoteScope.project.title || remoteScope.machineName || 'Remote project',
      subject: '',
    });
    const modalDraft: SidebarPromptGitCommitMessage = {
      action,
      agentId: this.resolveDefaultPromptAgentId(),
      branch: gitState.branch,
      changedFiles: gitState.files,
      confirmLabel: resolveGpuiSidebarGitConfirmLabel(action, hasCommit),
      deleteWorktreeAfterDefault: false,
      description: hasCommit
        ? 'Review and confirm your remote commit. Leave the message blank to auto-generate one.'
        : resolveGpuiSidebarGitPromptDescription(action),
      isDefaultRef: gitState.branch === 'main' || gitState.branch === 'master',
      isWorktree: normalizeGpuiWorktreeMetadata(remoteScope.project.worktree) !== undefined,
      requestId,
      showCommitMessage: hasCommit,
      suggestedBody: undefined,
      suggestedSubject: '',
      type: 'promptGitCommit',
      worktreeName: stringFromRecord(remoteScope.project.worktree, 'name') ?? remoteScope.project.title,
    };
    this.openSidebarGitCommitReviewModal(modalDraft);
  },

  async runSidebarGitAction(
    this: GpuiSidebarRuntime,
    message: Extract<SidebarToExtensionMessage, { type: 'runSidebarGitAction' }>
  ): Promise<void> {
    const remoteReference = message.groupId
      ? parseGpuiRemotePresentationGroupId(message.groupId)
      : message.projectId
        ? parseGpuiRemotePresentationProjectId(message.projectId)
        : undefined;
    if (remoteReference) {
      const remoteScope = this.resolveRemotePresentationProjectScope({
        groupId: message.groupId,
        projectId: message.projectId,
      });
      if (!remoteScope) {
        this.postRemoteToast('warning', 'Remote Git unavailable', {
          description: 'Reconnect the remote machine before using Git actions.',
        });
        return;
      }
      await this.runRemoteSidebarGitAction(message, remoteScope);
      return;
    }
    const project = this.resolveGitProjectForMessage(message);
    if (!project) {
      this.postGitToast('warning', 'Git unavailable', {
        description: 'No active gxserver project is available.',
      });
      return;
    }

    if (message.action === 'multiRelease') {
      await this.runSidebarGitPromptAction(project, 'Multicommit & Release', GPUI_GIT_MULTICOMMIT_RELEASE_PROMPT);
      return;
    }
    if (message.action === 'release') {
      await this.runSidebarGitPromptAction(project, 'Release', GPUI_GIT_RELEASE_ONLY_PROMPT);
      return;
    }

    const gitState = await this.refreshGitState({
      force: true,
      project,
      publishBusy: true,
      toastOnFailure: true,
    });
    if (!gitState.isRepo) {
      this.postGitToast('warning', 'Git unavailable', {
        description: 'Open a Git repository to use Git actions.',
      });
      return;
    }

    if (message.action === 'syncMain') {
      if (!normalizeGpuiWorktreeParentProjectId(project.worktree)) {
        this.postGitToast('warning', 'Worktree unavailable', {
          description: 'Open a worktree project to sync with main.',
        });
        return;
      }
      await this.runSidebarGitPromptAction(project, 'Sync with Main', buildGpuiGitSyncWithMainPrompt());
      return;
    }

    if (message.action === 'syncRemote') {
      if (!hasSidebarGitRemoteCommitDelta(gitState)) {
        this.postGitToast('info', 'Remote already synced');
        return;
      }
      await this.runGitMutation(project, 'Syncing remote', 'Remote sync complete', async () => {
        await this.syncCurrentBranchWithRemote(project, gitState);
      });
      return;
    }

    if (
      normalizeGpuiWorktreeMetadata(project.worktree) &&
      (message.action === 'commit' || message.action === 'push' || message.action === 'pr')
    ) {
      this.promptSidebarGitActionReview(project, gitState, message.action);
      return;
    }

    if (message.action === 'pr') {
      if (gitState.pr?.state === 'open') {
        this.postNativeProjectPathAction('openExistingPullRequestInBrowser', project.projectId, message);
        return;
      }
      if (!gitState.hasGitHubCli) {
        this.postGitToast('warning', 'GitHub CLI unavailable', {
          description: 'Install GitHub CLI before creating a pull request.',
        });
        return;
      }
      if (gitState.hasWorkingTreeChanges) {
        this.promptSidebarGitActionReview(project, gitState, 'pr');
        return;
      }
      await this.runSidebarGitPullRequestAgentWorkflow({
        gitState,
        hasCommit: false,
        hasExplicitFileSelection: false,
        message: '',
        project,
      });
      return;
    }

    if (message.action === 'commit') {
      if (!gitState.hasWorkingTreeChanges) {
        this.postGitToast('info', 'No changes to commit');
        return;
      }
      this.promptSidebarGitActionReview(project, gitState, 'commit');
      return;
    }

    if (message.action === 'push') {
      if (gitState.hasWorkingTreeChanges) {
        this.promptSidebarGitActionReview(project, gitState, 'push');
        return;
      }
      await this.runGitMutation(project, 'Pushing', 'Push complete', async () => {
        await this.pushCurrentBranch(project, gitState);
      });
    }
  },

  async confirmSidebarGitCommit(
    this: GpuiSidebarRuntime,
    message: Extract<SidebarToExtensionMessage, { type: 'confirmSidebarGitCommit' }>
  ): Promise<void> {
    const pending = this.pendingGitCommitRequests.get(message.requestId);
    this.pendingGitCommitRequests.delete(message.requestId);
    if (!pending) {
      this.publishHudPatch();
      return;
    }
    if (isGpuiRemotePendingGitCommitRequest(pending)) {
      await this.confirmRemoteSidebarGitCommit(pending, message);
      return;
    }
    const project = this.domainProjectById(pending.projectId);
    if (!project) {
      this.postGitToast('error', 'Git action unavailable', {
        description: 'The selected gxserver project is no longer available.',
      });
      this.publishHudPatch();
      return;
    }
    const gitState = await this.refreshGitState({
      force: true,
      project,
      publishBusy: true,
      toastOnFailure: true,
    });
    if (!gitState.isRepo) {
      this.postGitToast('warning', 'Git unavailable', {
        description: 'Open a Git repository to use Git actions.',
      });
      return;
    }
    if (pending.action === 'pr') {
      let trustedFileSelection: GpuiTrustedGitReviewFileSelection | undefined;
      if (pending.hasCommit) {
        try {
          trustedFileSelection = this.resolveTrustedGitReviewFileSelection(pending, message.filePaths);
        } catch {
          this.postGitToast('warning', 'Invalid file selection', {
            description: 'Choose files from the current Git review before creating a pull request.',
          });
          this.gitState = { ...this.gitStateForHud(), isBusy: false };
          this.publishHudPatch();
          return;
        }
      }
      if (message.deleteWorktreeAfter !== true) {
        await this.runSidebarGitPullRequestAgentWorkflow({
          agentId: message.agentId,
          filePaths: trustedFileSelection?.filePaths,
          gitState,
          hasCommit: pending.hasCommit,
          hasExplicitFileSelection: trustedFileSelection?.explicit ?? false,
          message: message.message,
          project,
        });
        return;
      }
      let confirmedPullRequest = false;
      const completed = await this.runGitMutation(
        project,
        resolveGpuiSidebarGitStartedTitle('pr', pending.hasCommit),
        resolveGpuiSidebarGitFinishedTitle('pr'),
        async () => {
          if (pending.hasCommit) {
            await this.commitWithMessage(project, message.message, trustedFileSelection?.filePaths, {
              agentId: message.agentId,
              commitOnNewRef: message.commitOnNewRef === true,
            });
          }
          const nextGitState = await this.refreshGitState({ force: true, project });
          await this.pushCurrentBranch(project, nextGitState);
          const result = await this.createPullRequest(project);
          if (!isGpuiConfirmedOpenPullRequest(result)) {
            throw new GpuiUserVisibleGitError('GitHub CLI could not create or find an open pull request.');
          }
          confirmedPullRequest = true;
          this.postNativeProjectPathAction('openExistingPullRequestInBrowser', project.projectId, message);
        }
      );
      if (completed && confirmedPullRequest) {
        await this.deleteWorktreeAfterCompletedGitAction(project);
      }
      if (completed && !confirmedPullRequest) {
        this.postGitToast('warning', 'Worktree cleanup skipped', {
          description: 'Pull request creation was not confirmed.',
        });
      }
      if (!completed) {
        this.postGitToast('warning', 'Worktree cleanup skipped', {
          description: 'Pull request creation did not complete.',
        });
      }
      return;
    }
    let trustedFileSelection: GpuiTrustedGitReviewFileSelection | undefined;
    if (pending.hasCommit) {
      try {
        trustedFileSelection = this.resolveTrustedGitReviewFileSelection(pending, message.filePaths);
      } catch {
        this.postGitToast('warning', 'Invalid file selection', {
          description: 'Choose files from the current Git review before committing.',
        });
        this.gitState = { ...this.gitStateForHud(), isBusy: false };
        this.publishHudPatch();
        return;
      }
    }

    const completed = await this.runGitMutation(
      project,
      resolveGpuiSidebarGitStartedTitle(pending.action, pending.hasCommit),
      resolveGpuiSidebarGitFinishedTitle(pending.action),
      async () => {
        if (pending.hasCommit) {
          await this.commitWithMessage(project, message.message, trustedFileSelection?.filePaths, {
            agentId: message.agentId,
            commitOnNewRef: message.commitOnNewRef === true,
          });
        }
        if (pending.action === 'push') {
          const nextState = await this.refreshGitState({ force: true, project });
          await this.pushCurrentBranch(project, nextState);
        }
      }
    );
    if (completed && message.deleteWorktreeAfter === true) {
      await this.deleteWorktreeAfterCompletedGitAction(project);
    }
  },

  async confirmRemoteSidebarGitCommit(
    this: GpuiSidebarRuntime,
    pending: GpuiPendingGitCommitRequest & { remoteReference: GpuiRemoteProjectReference },
    message: Extract<SidebarToExtensionMessage, { type: 'confirmSidebarGitCommit' }>
  ): Promise<void> {
    const remoteScope = this.resolveRemotePresentationProjectScope(pending.remoteReference);
    if (!remoteScope) {
      this.postRemoteToast('warning', 'Remote Git unavailable', {
        description: 'Reconnect the remote machine before confirming this Git action.',
      });
      return;
    }
    const gitState = await this.readRemoteSidebarGitState(remoteScope);
    if (!gitState.isRepo) {
      this.postRemoteToast('warning', 'Remote Git unavailable', {
        description: 'Open a Git repository on the remote machine to use Git actions.',
      });
      return;
    }
    if (pending.action === 'pr') {
      let trustedFileSelection: GpuiTrustedGitReviewFileSelection | undefined;
      if (pending.hasCommit) {
        try {
          trustedFileSelection = this.resolveTrustedGitReviewFileSelection(pending, message.filePaths);
        } catch {
          this.postRemoteToast('warning', 'Invalid file selection', {
            description: 'Choose files from the current remote Git review before creating a pull request.',
          });
          return;
        }
      }
      if (message.deleteWorktreeAfter !== true) {
        await this.runRemoteSidebarGitPullRequestAgentWorkflow({
          agentId: message.agentId,
          filePaths: trustedFileSelection?.filePaths,
          gitState,
          hasCommit: pending.hasCommit,
          hasExplicitFileSelection: trustedFileSelection?.explicit ?? false,
          message: message.message,
          remoteScope,
        });
        return;
      }
      let confirmedPullRequest = false;
      const completed = await this.runRemoteGitMutation(
        remoteScope,
        resolveGpuiSidebarGitStartedTitle('pr', pending.hasCommit),
        resolveGpuiSidebarGitFinishedTitle('pr'),
        async () => {
          if (pending.hasCommit) {
            await this.commitRemoteWithMessage(remoteScope, message.message, trustedFileSelection?.filePaths, {
              agentId: message.agentId,
              commitOnNewRef: message.commitOnNewRef === true,
            });
          }
          const nextGitState = await this.readRemoteSidebarGitState(remoteScope);
          await this.pushRemoteCurrentBranch(remoteScope, nextGitState);
          const result = await this.createRemotePullRequest(remoteScope);
          if (!isGpuiConfirmedOpenRemotePullRequest(result)) {
            throw new GpuiUserVisibleGitError('GitHub CLI could not create or find an open remote pull request.');
          }
          confirmedPullRequest = true;
          this.postRemoteProjectNativeAction('openRemoteExistingPullRequestInBrowser', remoteScope, message);
        }
      );
      if (completed && confirmedPullRequest) {
        await this.deleteRemoteWorktreeAfterCompletedGitAction(remoteScope);
      }
      if (completed && !confirmedPullRequest) {
        this.postRemoteToast('warning', 'Remote worktree cleanup skipped', {
          description: 'Pull request creation was not confirmed.',
        });
      }
      if (!completed) {
        this.postRemoteToast('warning', 'Remote worktree cleanup skipped', {
          description: 'Pull request creation did not complete.',
        });
      }
      return;
    }

    let trustedFileSelection: GpuiTrustedGitReviewFileSelection | undefined;
    if (pending.hasCommit) {
      try {
        trustedFileSelection = this.resolveTrustedGitReviewFileSelection(pending, message.filePaths);
      } catch {
        this.postRemoteToast('warning', 'Invalid file selection', {
          description: 'Choose files from the current remote Git review before committing.',
        });
        return;
      }
    }

    const completed = await this.runRemoteGitMutation(
      remoteScope,
      resolveGpuiSidebarGitStartedTitle(pending.action, pending.hasCommit),
      resolveGpuiSidebarGitFinishedTitle(pending.action),
      async () => {
        if (pending.hasCommit) {
          await this.commitRemoteWithMessage(remoteScope, message.message, trustedFileSelection?.filePaths, {
            agentId: message.agentId,
            commitOnNewRef: message.commitOnNewRef === true,
          });
        }
        if (pending.action === 'push') {
          const nextState = await this.readRemoteSidebarGitState(remoteScope);
          await this.pushRemoteCurrentBranch(remoteScope, nextState);
        }
      }
    );
    if (completed && message.deleteWorktreeAfter === true) {
      await this.deleteRemoteWorktreeAfterCompletedGitAction(remoteScope);
    }
  },

  async confirmSidebarGitDirectMerge(
    this: GpuiSidebarRuntime,
    message: Extract<SidebarToExtensionMessage, { type: 'confirmSidebarGitDirectMerge' }>
  ): Promise<void> {
    const pending = this.pendingGitCommitRequests.get(message.requestId);
    this.pendingGitCommitRequests.delete(message.requestId);
    if (!pending) {
      this.publishHudPatch();
      return;
    }
    if (isGpuiRemotePendingGitCommitRequest(pending)) {
      await this.confirmRemoteSidebarGitDirectMerge(pending, message);
      return;
    }
    const project = this.domainProjectById(pending.projectId);
    if (!project) {
      this.postGitToast('error', 'Direct merge unavailable', {
        description: 'The selected gxserver project is no longer available.',
      });
      this.publishHudPatch();
      return;
    }
    const worktree = normalizeGpuiWorktreeMetadata(project.worktree);
    if (!worktree) {
      this.postGitToast('warning', 'Worktree unavailable', {
        description: 'Direct merge is only available from a gxserver worktree project.',
      });
      this.publishHudPatch();
      return;
    }
    const conflictAgent = this.resolveDefaultPromptAgent(message.agentId);
    if (!conflictAgent?.command?.trim()) {
      this.postGitToast('error', 'Agent unavailable', {
        description: 'Choose a configured prompt agent before merging.',
      });
      this.publishHudPatch();
      return;
    }

    const gitState = await this.refreshGitState({
      force: true,
      project,
      publishBusy: true,
      toastOnFailure: true,
    });
    if (!gitState.isRepo) {
      this.postGitToast('warning', 'Git unavailable', {
        description: 'Open a Git repository before merging this worktree.',
      });
      return;
    }

    let trustedFileSelection: GpuiTrustedGitReviewFileSelection | undefined;
    if (pending.hasCommit) {
      try {
        trustedFileSelection = this.resolveTrustedGitReviewFileSelection(pending, message.filePaths);
      } catch {
        this.postGitToast('warning', 'Invalid file selection', {
          description: 'Choose files from the current Git review before merging.',
        });
        this.gitState = { ...this.gitStateForHud(), isBusy: false };
        this.publishHudPatch();
        return;
      }
    }

    const toastId = createGpuiGitToastId();
    this.postGitToast('info', 'Merging worktree into main', {
      persistent: true,
      toastId,
    });
    this.gitState = { ...this.gitStateForHud(), isBusy: true };
    this.publishHudPatch();
    try {
      if (pending.hasCommit) {
        await this.commitWithMessage(project, message.message, trustedFileSelection?.filePaths, {
          agentId: message.agentId,
        });
      }
      const nextGitState = await this.readSidebarGitState(project);
      const result = await this.mergeWorktreeIntoMain({
        branch: nextGitState.branch ?? worktree.branch,
        conflictAgent,
        deleteWorktreeAfter: message.deleteWorktreeAfter === true,
        worktreeProject: project,
      });
      this.gitState = { ...this.gitStateForHud(), isBusy: false };
      this.publishHudPatch();
      if (result === 'conflicts') {
        this.postGitToast('warning', 'Merge conflicts need resolution', { toastId });
        return;
      }
      await this.refreshDomainPresentationFromClient('patch').catch(() => undefined);
      this.postGitToast('success', 'Worktree merged to main', { toastId });
    } catch (error) {
      this.gitState = { ...this.gitStateForHud(), isBusy: false };
      this.publishHudPatch();
      this.postGitToast('error', 'Direct merge failed', {
        description: gpuiUserVisibleGitErrorMessage(error, 'gxserver could not merge the selected worktree.'),
        toastId,
      });
    }
  },

  async confirmRemoteSidebarGitDirectMerge(
    this: GpuiSidebarRuntime,
    pending: GpuiPendingGitCommitRequest & { remoteReference: GpuiRemoteProjectReference },
    message: Extract<SidebarToExtensionMessage, { type: 'confirmSidebarGitDirectMerge' }>
  ): Promise<void> {
    const remoteScope = this.resolveRemotePresentationProjectScope(pending.remoteReference);
    if (!remoteScope) {
      this.postRemoteToast('warning', 'Remote merge unavailable', {
        description: 'Reconnect the remote machine before merging this worktree.',
      });
      return;
    }
    if (!normalizeGpuiWorktreeMetadata(remoteScope.project.worktree)) {
      this.postRemoteToast('warning', 'Remote worktree unavailable', {
        description: 'Direct merge is only available from a remote worktree project.',
      });
      return;
    }
    let trustedFileSelection: GpuiTrustedGitReviewFileSelection | undefined;
    if (pending.hasCommit) {
      try {
        trustedFileSelection = this.resolveTrustedGitReviewFileSelection(pending, message.filePaths);
      } catch {
        this.postRemoteToast('warning', 'Invalid file selection', {
          description: 'Choose files from the current remote Git review before merging.',
        });
        return;
      }
    }
    const toastId = createGpuiGitToastId();
    this.postGitToast('info', 'Merging remote worktree', {
      persistent: true,
      toastId,
    });
    /*
    CDXC:RemoteGitBranching 2026-06-24-18:55:
    Remote direct merge and commit-on-new-branch must go through id-scoped gxserver operations so the daemon derives main, parent, and branch targets. GPUI may refresh presentation and create a conflict-resolution agent session, but it must not attach terminals, focus remote panes, open native apps, or expose branch/path/command details in status text.
    */
    try {
      if (pending.hasCommit) {
        await this.commitRemoteWithMessage(remoteScope, message.message, trustedFileSelection?.filePaths, {
          agentId: message.agentId,
        });
      }
      const result = await this.mergeRemoteWorktreeIntoMain(remoteScope);
      await this.refreshRemotePresentationFromGxserver(remoteScope.machineId).catch(() => undefined);
      if (result.status === 'conflicts') {
        this.postGitToast('warning', 'Remote merge conflicts need resolution', { toastId });
        const conflictAgentId = this.resolveDefaultPromptAgentId(message.agentId);
        if (conflictAgentId && result.parentProjectId) {
          await this.createRemoteAgentSessionForProject(
            { machineId: remoteScope.machineId, projectId: result.parentProjectId },
            conflictAgentId,
            GPUI_REMOTE_MERGE_CONFLICT_PROMPT,
            formatGpuiGitAgentWorkflowTitle('Merge Conflicts')
          ).catch(() => undefined);
        }
        return;
      }
      this.postGitToast('success', 'Remote worktree merged', { toastId });
      if (message.deleteWorktreeAfter === true) {
        await this.deleteRemoteWorktreeAfterCompletedGitAction(remoteScope);
      }
    } catch (error) {
      this.postGitToast('error', 'Remote direct merge failed', {
        description: gpuiUserVisibleGitErrorMessage(error, 'Remote gxserver could not merge the selected worktree.'),
        toastId,
      });
    }
  },
};
