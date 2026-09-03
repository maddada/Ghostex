/*
CDXC:RepoStructure 2026-08-23:
Directory split of gxserver-runtime/git.ts (~3,251 lines) into git/. This
slice covers merging a worktree into main (plus the conflict-agent launch
and multiple-commits release flow) and the commit review modal's changed-file
diff/IDE-open methods. See `index.ts` for how the runtime's Git methods are
recombined.
*/
import { GPUI_GIT_MULTIPLE_COMMITS_PROMPT } from '../constants';
import type { GpuiSidebarRuntime } from '../core';
import {
  buildGpuiMergeConflictPrompt,
  formatGpuiGitAgentWorkflowTitle,
  hasGpuiGxserverShortStatusChanges,
  normalizeGpuiRelativeGitFilePath,
  resolveGpuiSidebarGitConfirmLabel,
  resolveGpuiSidebarGitPromptDescription,
} from '../helpers/git';
import { stringFromRecord } from '../helpers/records';
import { normalizeGpuiProjectPath, normalizeGpuiWorktreeMetadata } from '../helpers/worktrees';
import type {
  GpuiPendingGitCommitRequest,
  GpuiRemoteProjectReference,
  GpuiRemoteProjectScope,
  GpuiTrustedGitReviewFileSelection,
  GpuiWorktreeMetadata,
} from '../types-and-protocol';
import { openAppModal, postAppModalHostMessage } from '@/packages/core-ui/app-modal-host-bridge';
import type {
  GxserverMergeWorktreeIntoMainResult,
  GxserverProjectDomainState,
} from '@/packages/shared/gxserver-protocol';
import type { SidebarPromptGitCommitMessage, SidebarToExtensionMessage } from '@/packages/shared/session-grid-contract';
import type { SidebarAgentButton } from '@/packages/shared/sidebar-agents';
import type { SidebarGitAction, SidebarGitFileDiffDraft, SidebarGitState } from '@/packages/shared/sidebar-git';

export const gpuiSidebarRuntimeGitWorktreeMergeAndReviewMethods = {
  async mergeRemoteWorktreeIntoMain(
    this: GpuiSidebarRuntime,
    remoteScope: GpuiRemoteProjectScope
  ): Promise<GxserverMergeWorktreeIntoMainResult> {
    return this.requestRemoteGxserver<GxserverMergeWorktreeIntoMainResult>(
      remoteScope.machineId,
      '/api/mergeWorktreeIntoMain',
      { projectId: remoteScope.projectId },
      { timeoutMs: 60_000 }
    );
  },

  async mergeWorktreeIntoMain(
    this: GpuiSidebarRuntime,
    input: {
      branch?: string | null;
      conflictAgent: SidebarAgentButton;
      deleteWorktreeAfter: boolean;
      worktreeProject: GxserverProjectDomainState;
    }
  ): Promise<'conflicts' | 'merged'> {
    const worktree = normalizeGpuiWorktreeMetadata(input.worktreeProject.worktree);
    if (!worktree) {
      throw new Error('Direct merge requires a worktree project.');
    }
    const branch = input.branch?.trim() || worktree.branch;
    if (!branch) {
      throw new Error('Create and checkout a branch before merging.');
    }
    const parentProject = this.domainProjectById(worktree.parentProjectId);
    if (
      !parentProject ||
      parentProject.projectId === input.worktreeProject.projectId ||
      parentProject.isRecentProject === true ||
      !normalizeGpuiProjectPath(parentProject.path)
    ) {
      throw new Error('The gxserver worktree parent project is unavailable.');
    }

    const mainCheck = await this.runGitAction(parentProject, {
      action: 'verifyRef',
      ref: 'main',
    });
    if (mainCheck.exitCode !== 0) {
      throw new Error('The parent project does not have a local "main" branch.');
    }
    const parentStatus = await this.runGitAction(parentProject, { action: 'status' });
    if (parentStatus.exitCode !== 0) {
      throw new Error('Could not read parent project status.');
    }
    if (hasGpuiGxserverShortStatusChanges(parentStatus.stdout)) {
      throw new Error('Commit or stash changes in the main project before merging this worktree.');
    }

    const checkoutResult = await this.runGitAction(parentProject, {
      action: 'checkout',
      branch: 'main',
    });
    if (checkoutResult.exitCode !== 0) {
      throw new Error('Could not checkout main.');
    }
    const mergeResult = await this.runGitAction(parentProject, {
      action: 'merge',
      branch,
    });
    /*
    CDXC:Git 2026-07-29:
    Direct merge is the one Git flow whose writes land in a project other than
    the one the user is looking at: everything here mutates the *parent* repo
    while the flow only ever re-reads the worktree project. `runGitAction`
    drops the parent lease before each write, but that is not enough on its
    own here, because a parent read that was already in flight when the merge
    landed stores its pre-merge answer afterwards and would then be republished
    for the rest of the TTL. Invalidate once the merge has actually returned,
    for both outcomes: a conflicted merge still leaves the parent checked out
    on `main` with a merge in progress, and that path focuses the parent
    immediately.

    The GitHub lease goes with it. It is keyed by project but its content is
    per-branch, this flow checks the parent out onto `main`, and a merge can
    close the pull request the memo is describing. It also has no TTL check on
    the publish path (`applyLiveGitStateOverlays` peeks it), so a wrong entry
    here would otherwise survive until the next explicit probe.
    */
    this.gitStateMemoByProjectId.delete(parentProject.projectId);
    this.gitHubStateMemoByProjectId.delete(parentProject.projectId);
    if (mergeResult.exitCode !== 0) {
      await this.launchMergeConflictAgent({
        agent: input.conflictAgent,
        branch,
        mergeOutput: mergeResult.stderr.trim() || mergeResult.stdout.trim(),
        parentProject,
        worktree,
        worktreeProject: input.worktreeProject,
      });
      return 'conflicts';
    }

    if (input.deleteWorktreeAfter) {
      await this.deleteWorktreeAfterCompletedGitAction(input.worktreeProject);
    }
    return 'merged';
  },

  async launchMergeConflictAgent(
    this: GpuiSidebarRuntime,
    input: {
      agent: SidebarAgentButton;
      branch: string;
      mergeOutput: string;
      parentProject: GxserverProjectDomainState;
      worktree: GpuiWorktreeMetadata;
      worktreeProject: GxserverProjectDomainState;
    }
  ): Promise<void> {
    this.focusProjectId(input.parentProject.projectId);
    await this.createAgentSessionForProject(
      input.parentProject,
      input.agent,
      buildGpuiMergeConflictPrompt(input),
      formatGpuiGitAgentWorkflowTitle('Merge Conflicts')
    );
  },

  async runSidebarGitMultipleCommits(this: GpuiSidebarRuntime, requestId: string, agentId?: string): Promise<void> {
    const pending = this.pendingGitCommitRequests.get(requestId);
    this.pendingGitCommitRequests.delete(requestId);
    if (pending?.remoteReference) {
      const remoteScope = this.resolveRemotePresentationProjectScope(pending.remoteReference);
      if (!remoteScope) {
        this.postRemoteToast('warning', 'Remote Git unavailable', {
          description: 'Reconnect the remote machine before starting this Git workflow.',
        });
        return;
      }
      await this.runRemoteSidebarGitPromptAction(
        remoteScope,
        'Multiple Commits',
        GPUI_GIT_MULTIPLE_COMMITS_PROMPT,
        agentId
      );
      return;
    }
    const project = pending ? this.domainProjectById(pending.projectId) : this.activeDomainProject();
    if (!project) {
      this.postGitToast('warning', 'Git unavailable', {
        description: 'No active gxserver project is available.',
      });
      this.publishHudPatch();
      return;
    }
    await this.runSidebarGitPromptAction(project, 'Multiple Commits', GPUI_GIT_MULTIPLE_COMMITS_PROMPT, agentId);
  },

  promptSidebarGitActionReview(
    this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState,
    gitState: SidebarGitState,
    action: Extract<SidebarGitAction, 'commit' | 'pr' | 'push'>
  ): void {
    const requestId = `gpui-git-action-${Date.now().toString(36)}`;
    const hasCommit = gitState.hasWorkingTreeChanges;
    /*
    CDXC:Git 2026-06-24-15:22:
    GPUI commit review stores the gxserver-derived changed-file list with the request id. Later modal selections and diff clicks may only reference those paths, so CEF cannot stage or inspect arbitrary renderer-supplied paths.
    Treat the modal's all-selected case as that stored review list instead of a fresh unbounded add-all, so files created after review opens cannot slip into the confirmed commit.
    */
    this.pendingGitCommitRequests.set(requestId, {
      action,
      files: [...gitState.files],
      hasCommit,
      projectId: project.projectId,
      subject: '',
    });
    const modalDraft: SidebarPromptGitCommitMessage = {
      action,
      agentId: this.resolveDefaultPromptAgent()?.agentId,
      branch: gitState.branch,
      changedFiles: gitState.files,
      confirmLabel: resolveGpuiSidebarGitConfirmLabel(action, hasCommit),
      deleteWorktreeAfterDefault: false,
      description: hasCommit
        ? 'Review and confirm your commit. Leave the message blank to auto-generate one.'
        : resolveGpuiSidebarGitPromptDescription(action),
      isDefaultRef: gitState.branch === 'main' || gitState.branch === 'master',
      isWorktree: normalizeGpuiWorktreeMetadata(project.worktree) !== undefined,
      requestId,
      showCommitMessage: hasCommit,
      suggestedBody: undefined,
      suggestedSubject: '',
      type: 'promptGitCommit',
      worktreeName: stringFromRecord(project.worktree, 'name'),
    };
    this.openSidebarGitCommitReviewModal(modalDraft);
    this.gitState = { ...gitState, isBusy: false };
    this.publishHudPatch();
  },

  openSidebarGitCommitReviewModal(this: GpuiSidebarRuntime, draft: SidebarPromptGitCommitMessage): void {
    openAppModal({
      gitCommitDraft: draft,
      modal: 'gitCommit',
      type: 'open',
    });
  },

  async openSidebarGitChangedFileDiff(this: GpuiSidebarRuntime, filePath: string, requestId?: string): Promise<void> {
    const request = requestId ? this.pendingGitCommitRequests.get(requestId) : undefined;
    if (request?.remoteReference) {
      await this.openRemoteSidebarGitChangedFileDiff(request.remoteReference, filePath, requestId);
      return;
    }
    const project = request ? this.domainProjectById(request.projectId) : undefined;
    const normalizedFilePath = normalizeGpuiRelativeGitFilePath(filePath);
    if (!requestId || !request || !project || !normalizedFilePath) {
      return;
    }
    const reviewFile = request.files.find((file) => file.path === normalizedFilePath);
    if (!reviewFile) {
      return;
    }
    try {
      const [stagedDiff, unstagedDiff] = await Promise.all([
        this.runGitAction(project, {
          action: 'diffCachedNoExt',
          filePath: normalizedFilePath,
        }),
        this.runGitAction(project, {
          action: 'diffNoExt',
          filePath: normalizedFilePath,
        }),
      ]);
      const patchParts = [stagedDiff.stdout.trimEnd(), unstagedDiff.stdout.trimEnd()].filter(
        (part) => part.trim().length > 0
      );
      let patch = patchParts.join('\n\n');
      if (!patch.trim()) {
        const untracked = await this.runGitAction(project, {
          action: 'isUntrackedFile',
          filePath: normalizedFilePath,
        });
        if (untracked.stdout.trim()) {
          const noIndexDiff = await this.runGitAction(project, {
            action: 'diffNoIndexAgainstNull',
            filePath: normalizedFilePath,
          });
          patch = noIndexDiff.stdout.trimEnd() || noIndexDiff.stderr.trimEnd();
        }
      }
      this.postSidebarGitFileDiff(requestId, {
        additions: reviewFile.additions,
        deletions: reviewFile.deletions,
        filePath: normalizedFilePath,
        patch: patch.trim() || `No diff is available for ${normalizedFilePath}.`,
      });
    } catch {
      this.postSidebarGitFileDiff(requestId, {
        additions: reviewFile.additions,
        deletions: reviewFile.deletions,
        filePath: normalizedFilePath,
        patch: `No diff is available for ${normalizedFilePath}.`,
      });
    }
  },

  async openRemoteSidebarGitChangedFileDiff(
    this: GpuiSidebarRuntime,
    remoteReference: GpuiRemoteProjectReference,
    filePath: string,
    requestId?: string
  ): Promise<void> {
    const request = requestId ? this.pendingGitCommitRequests.get(requestId) : undefined;
    const remoteScope = this.resolveRemotePresentationProjectScope(remoteReference);
    const normalizedFilePath = normalizeGpuiRelativeGitFilePath(filePath);
    if (!requestId || !request || !remoteScope || !normalizedFilePath) {
      return;
    }
    const reviewFile = request.files.find((file) => file.path === normalizedFilePath);
    if (!reviewFile) {
      return;
    }
    try {
      const [stagedDiff, unstagedDiff] = await Promise.all([
        this.runRemoteGitAction(remoteScope, {
          action: 'diffCachedNoExt',
          filePath: normalizedFilePath,
        }),
        this.runRemoteGitAction(remoteScope, {
          action: 'diffNoExt',
          filePath: normalizedFilePath,
        }),
      ]);
      const patchParts = [stagedDiff.stdout.trimEnd(), unstagedDiff.stdout.trimEnd()].filter(
        (part) => part.trim().length > 0
      );
      let patch = patchParts.join('\n\n');
      if (!patch.trim()) {
        const untracked = await this.runRemoteGitAction(remoteScope, {
          action: 'isUntrackedFile',
          filePath: normalizedFilePath,
        });
        if (untracked.stdout.trim()) {
          const noIndexDiff = await this.runRemoteGitAction(remoteScope, {
            action: 'diffNoIndexAgainstNull',
            filePath: normalizedFilePath,
          });
          patch = noIndexDiff.stdout.trimEnd() || noIndexDiff.stderr.trimEnd();
        }
      }
      this.postSidebarGitFileDiff(requestId, {
        additions: reviewFile.additions,
        deletions: reviewFile.deletions,
        filePath: normalizedFilePath,
        patch: patch.trim() || `No diff is available for ${normalizedFilePath}.`,
      });
    } catch {
      this.postSidebarGitFileDiff(requestId, {
        additions: reviewFile.additions,
        deletions: reviewFile.deletions,
        filePath: normalizedFilePath,
        patch: `No diff is available for ${normalizedFilePath}.`,
      });
    }
  },

  async openSidebarGitChangedFileInIde(
    this: GpuiSidebarRuntime,
    message: Extract<SidebarToExtensionMessage, { type: 'openSidebarGitChangedFile' }>
  ): Promise<void> {
    /*
    CDXC:Git 2026-06-24-21:26:
    Changed-file IDE opens reuse the shared SidebarApp file row. GPUI sends Rust only the gxserver project id and a normalized relative file candidate already present in the current HUD or review request; Rust remains authoritative and re-validates the file against gxserver before resolving an absolute path.
    Scoped non-review opens must re-read the owning local or remote gxserver project instead of using the active local HUD file list, so remote rows cannot open stale or cross-project file candidates.
    */
    const normalizedFilePath = normalizeGpuiRelativeGitFilePath(message.filePath);
    const request = message.requestId ? this.pendingGitCommitRequests.get(message.requestId) : undefined;
    if (request?.remoteReference) {
      const remoteScope = this.resolveRemotePresentationProjectScope(request.remoteReference);
      if (!normalizedFilePath || !remoteScope || !request.files.some((file) => file.path === normalizedFilePath)) {
        this.postRemoteToast('warning', 'Remote file open unavailable', {
          description: 'Choose a changed file from the current remote Git review.',
        });
        return;
      }
      this.postRemoteProjectNativeAction('openRemoteSidebarGitChangedFileInIde', remoteScope, message, {
        filePath: normalizedFilePath,
      });
      return;
    }
    if (!request) {
      const remoteScope = this.resolveGitPreferenceRemoteScope(message);
      if (remoteScope) {
        if (!normalizedFilePath) {
          this.postRemoteToast('warning', 'Remote file open unavailable', {
            description: 'Choose a changed file from the current remote Git state.',
          });
          return;
        }
        const gitState = await this.readRemoteSidebarGitState(remoteScope);
        if (!gitState.files.some((file) => file.path === normalizedFilePath)) {
          this.postRemoteToast('warning', 'Remote file open unavailable', {
            description: 'Choose a changed file from the current remote Git state.',
          });
          return;
        }
        this.postRemoteProjectNativeAction('openRemoteSidebarGitChangedFileInIde', remoteScope, message, {
          filePath: normalizedFilePath,
        });
        return;
      }
      if (this.isGitPreferenceRemoteScope(message)) {
        this.postRemoteToast('warning', 'Remote file open unavailable', {
          description: 'Reconnect the remote machine before opening changed files.',
        });
        return;
      }
    }
    const project = request ? this.domainProjectById(request.projectId) : this.activeDomainProject();
    const explicitScope = !request && Boolean(message.groupId?.trim() || message.projectId?.trim());
    const scopedProject = request
      ? project
      : (this.resolveGitPreferenceLocalProject(message) ?? (explicitScope ? undefined : project));
    const trustedFiles =
      request?.files ??
      (scopedProject && scopedProject.projectId !== this.activeProjectId
        ? (await this.readSidebarGitState(scopedProject)).files
        : this.gitState.files);
    if (
      !normalizedFilePath ||
      !scopedProject ||
      scopedProject.isRecentProject === true ||
      !trustedFiles.some((file) => file.path === normalizedFilePath)
    ) {
      this.postGitToast('warning', 'Open file unavailable', {
        description: 'Choose a changed file from the current Git state.',
      });
      return;
    }
    this.postNativeProjectPathAction('openSidebarGitChangedFileInIde', scopedProject.projectId, message, {
      filePath: normalizedFilePath,
    });
  },

  postSidebarGitFileDiff(this: GpuiSidebarRuntime, requestId: string, draft: SidebarGitFileDiffDraft): void {
    postAppModalHostMessage(
      {
        gitFileDiff: draft,
        modal: 'gitFileDiff',
        requestId,
        type: 'open',
      },
      'AppModals:gpuiGitFileDiff'
    );
  },

  resolveTrustedGitReviewFileSelection(
    this: GpuiSidebarRuntime,
    request: GpuiPendingGitCommitRequest,
    filePaths?: readonly string[]
  ): GpuiTrustedGitReviewFileSelection {
    const explicit = filePaths !== undefined;
    const candidatePaths = explicit ? filePaths : request.files.map((file) => file.path);
    const allowedPaths = new Map(request.files.map((file) => [file.path, file.path]));
    const selectedPaths: string[] = [];
    for (const filePath of candidatePaths) {
      const normalizedPath = normalizeGpuiRelativeGitFilePath(filePath);
      const trustedPath = normalizedPath ? allowedPaths.get(normalizedPath) : undefined;
      if (!trustedPath) {
        throw new Error('Selected file is not part of the current Git review.');
      }
      if (!selectedPaths.includes(trustedPath)) {
        selectedPaths.push(trustedPath);
      }
    }
    if (selectedPaths.length === 0) {
      throw new Error('Select at least one changed file.');
    }
    return { explicit, filePaths: selectedPaths };
  },
};
