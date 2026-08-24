/*
CDXC:GxserverRuntimeSplit 2026-08-23:
Directory split of gxserver-runtime/git.ts (~3,251 lines) into git/. This
slice covers the commit/branch mutation primitives (commit, generate commit
message, checkout, push, sync-with-remote) and their Beads pre-commit-hook
bypass checks, local and remote. See `index.ts` for how the runtime's Git
methods are recombined.
*/
import type { GpuiSidebarRuntime } from '../core';
import {
  GpuiUserVisibleGitError,
  createGpuiGitToastId,
  gpuiUserVisibleGitErrorMessage,
  isMissingGpuiBeadsDatabaseError,
  parseGpuiSidebarGitCommitMessage,
  sanitizeGpuiSidebarGitBranchName,
  supportsGpuiBackgroundCommitMessageGeneration,
} from '../helpers/git';
import type { GpuiRemoteProjectScope } from '../types-and-protocol';
import type {
  GxserverCheckoutProjectNewBranchResult,
  GxserverGenerateCommitMessageResult,
  GxserverProjectDomainState,
} from '@/packages/shared/gxserver-protocol';
import type { SidebarGitState } from '@/packages/shared/sidebar-git';

export const gpuiSidebarRuntimeGitBranchOperationsMethods = {
  async runGitMutation(
    this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState,
    startedTitle: string,
    finishedTitle: string,
    operation: () => Promise<void>
  ): Promise<boolean> {
    const toastId = createGpuiGitToastId();
    this.postGitToast('info', startedTitle, { persistent: true, toastId });
    this.gitState = { ...this.gitStateForHud(), isBusy: true };
    this.publishHudPatch();
    try {
      await operation();
      await this.refreshGitState({ force: true, project });
      this.postGitToast('success', finishedTitle, { toastId });
      return true;
    } catch (error) {
      this.gitState = { ...this.gitStateForHud(), isBusy: false };
      this.publishHudPatch();
      this.postGitToast('error', `${startedTitle} failed`, {
        description: gpuiUserVisibleGitErrorMessage(error, 'gxserver Git operation failed.'),
        toastId,
      });
      return false;
    }
  },

  async commitWithMessage(
    this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState,
    message: string,
    filePaths?: readonly string[],
    options: { agentId?: string; commitOnNewRef?: boolean } = {}
  ): Promise<void> {
    const parsedMessage = parseGpuiSidebarGitCommitMessage(message);
    let resolvedMessage = parsedMessage;
    if (parsedMessage.subject) {
      const addResult = await this.runGitAction(project, {
        action: 'addAll',
        filePaths,
      });
      if (addResult.exitCode !== 0) {
        throw new Error('Could not stage changes.');
      }
    } else {
      resolvedMessage = await this.generateCommitMessage(project, filePaths, options.agentId);
    }
    if (options.commitOnNewRef) {
      await this.checkoutSidebarGitFeatureBranch(project, resolvedMessage.subject);
    }
    const commitResult = await this.runGitAction(project, {
      action: 'commit',
      messageBody: resolvedMessage.body,
      messageSubject: resolvedMessage.subject,
      noVerify: await this.shouldBypassMissingBeadsDatabasePreCommitHook(project),
    });
    if (commitResult.exitCode !== 0) {
      throw new Error('Could not commit changes.');
    }
  },

  async generateCommitMessage(
    this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState,
    filePaths: readonly string[] | undefined,
    agentId?: string
  ): Promise<{ body: string; subject: string }> {
    if (!this.client) {
      throw new Error('gxserver is unavailable.');
    }
    if (!filePaths || filePaths.length === 0) {
      throw new Error('Select at least one changed file before generating a commit message.');
    }
    const agent = this.resolveDefaultPromptAgent(agentId);
    if (!agent?.command?.trim()) {
      throw new GpuiUserVisibleGitError('Choose a configured prompt agent before generating a commit message.');
    }
    if (!supportsGpuiBackgroundCommitMessageGeneration(agent)) {
      throw new GpuiUserVisibleGitError('Selected prompt agent does not support background commit message generation.');
    }
    return this.client.rpc<GxserverGenerateCommitMessageResult>('/api/generateCommitMessage', {
      agentId: agent.agentId,
      filePaths: [...filePaths],
      projectId: project.projectId,
    });
  },

  async generateRemoteCommitMessage(
    this: GpuiSidebarRuntime,
    remoteScope: GpuiRemoteProjectScope,
    filePaths: readonly string[] | undefined,
    agentId?: string
  ): Promise<{ body: string; subject: string }> {
    if (!filePaths || filePaths.length === 0) {
      throw new Error('Select at least one changed file before generating a commit message.');
    }
    const resolvedAgentId = this.resolveDefaultPromptAgentId(agentId);
    if (!resolvedAgentId) {
      throw new GpuiUserVisibleGitError('Choose a prompt agent before generating a remote commit message.');
    }
    return this.requestRemoteGxserver<GxserverGenerateCommitMessageResult>(
      remoteScope.machineId,
      '/api/generateCommitMessage',
      {
        agentId: resolvedAgentId,
        filePaths: [...filePaths],
        projectId: remoteScope.projectId,
      },
      { timeoutMs: 125_000 }
    );
  },

  async checkoutSidebarGitFeatureBranch(
    this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState,
    subject: string
  ): Promise<string> {
    const baseName = sanitizeGpuiSidebarGitBranchName(subject);
    for (let index = 0; index < 20; index += 1) {
      const candidate = index === 0 ? baseName : `${baseName}-${index + 1}`;
      const exists = await this.runGitAction(project, {
        action: 'verifyRef',
        ref: candidate,
      });
      if (exists.exitCode !== 0) {
        const checkout = await this.runGitAction(project, {
          action: 'checkoutNewBranch',
          branch: candidate,
        });
        if (checkout.exitCode !== 0) {
          throw new Error('Could not create a new branch.');
        }
        return candidate;
      }
    }
    throw new Error('Could not create a unique branch.');
  },

  async pushCurrentBranch(
    this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState,
    gitState: Pick<SidebarGitState, 'branch' | 'behindCount' | 'hasOriginRemote' | 'hasUpstream'>
  ): Promise<void> {
    const branch = gitState.branch;
    if (!branch) {
      throw new Error('Create and checkout a branch before pushing.');
    }
    if (gitState.behindCount > 0) {
      throw new Error('Branch is behind upstream.');
    }
    const push = gitState.hasUpstream
      ? await this.runGitAction(project, { action: 'push' })
      : gitState.hasOriginRemote
        ? await this.runGitAction(project, { action: 'pushSetUpstream', branch })
        : undefined;
    if (!push) {
      throw new Error('Add an "origin" remote before pushing.');
    }
    if (push.exitCode !== 0) {
      throw new Error('Could not push branch.');
    }
  },

  async syncCurrentBranchWithRemote(
    this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState,
    gitState: SidebarGitState
  ): Promise<void> {
    const branch = gitState.branch;
    if (!branch) {
      throw new Error('Create and checkout a branch before syncing.');
    }
    if (gitState.hasUpstream) {
      const pull = await this.runGitAction(project, { action: 'pullFastForward' });
      if (pull.exitCode !== 0) {
        throw new Error('Could not pull branch.');
      }
      const nextGitState = await this.refreshGitState({ force: true, project });
      if (nextGitState.aheadCount > 0) {
        await this.pushCurrentBranch(project, nextGitState);
      }
      return;
    }
    await this.pushCurrentBranch(project, gitState);
  },

  async commitRemoteWithMessage(
    this: GpuiSidebarRuntime,
    remoteScope: GpuiRemoteProjectScope,
    message: string,
    filePaths?: readonly string[],
    options: { agentId?: string; commitOnNewRef?: boolean } = {}
  ): Promise<void> {
    const parsedMessage = parseGpuiSidebarGitCommitMessage(message);
    let resolvedMessage = parsedMessage;
    if (parsedMessage.subject) {
      const addResult = await this.runRemoteGitAction(remoteScope, {
        action: 'addAll',
        filePaths,
      });
      if (addResult.exitCode !== 0) {
        throw new Error('Could not stage remote changes.');
      }
    } else {
      resolvedMessage = await this.generateRemoteCommitMessage(remoteScope, filePaths, options.agentId);
    }
    if (options.commitOnNewRef) {
      await this.checkoutRemoteSidebarGitFeatureBranch(remoteScope, resolvedMessage.subject);
    }
    const commitResult = await this.runRemoteGitAction(remoteScope, {
      action: 'commit',
      messageBody: resolvedMessage.body,
      messageSubject: resolvedMessage.subject,
      noVerify: await this.shouldBypassRemoteMissingBeadsDatabasePreCommitHook(remoteScope),
    });
    if (commitResult.exitCode !== 0) {
      throw new Error('Could not commit remote changes.');
    }
  },

  async checkoutRemoteSidebarGitFeatureBranch(
    this: GpuiSidebarRuntime,
    remoteScope: GpuiRemoteProjectScope,
    subject: string
  ): Promise<void> {
    const result = await this.requestRemoteGxserver<GxserverCheckoutProjectNewBranchResult>(
      remoteScope.machineId,
      '/api/checkoutProjectNewBranch',
      {
        branchLabel: subject,
        projectId: remoteScope.projectId,
      },
      { timeoutMs: 30_000 }
    );
    if (result.checkedOut !== true) {
      throw new Error('Could not create a new remote branch.');
    }
  },

  async pushRemoteCurrentBranch(
    this: GpuiSidebarRuntime,
    remoteScope: GpuiRemoteProjectScope,
    gitState: Pick<SidebarGitState, 'branch' | 'behindCount' | 'hasOriginRemote' | 'hasUpstream'>
  ): Promise<void> {
    const branch = gitState.branch;
    if (!branch) {
      throw new Error('Create and checkout a branch before pushing.');
    }
    if (gitState.behindCount > 0) {
      throw new Error('Remote branch is behind upstream.');
    }
    const push = gitState.hasUpstream
      ? await this.runRemoteGitAction(remoteScope, { action: 'push' })
      : gitState.hasOriginRemote
        ? await this.runRemoteGitAction(remoteScope, { action: 'pushSetUpstreamCurrent' })
        : undefined;
    if (!push) {
      throw new Error('Add an "origin" remote before pushing.');
    }
    if (push.exitCode !== 0) {
      throw new Error('Could not push remote branch.');
    }
  },

  async syncRemoteCurrentBranchWithRemote(
    this: GpuiSidebarRuntime,
    remoteScope: GpuiRemoteProjectScope,
    gitState: SidebarGitState
  ): Promise<void> {
    const branch = gitState.branch;
    if (!branch) {
      throw new Error('Create and checkout a branch before syncing.');
    }
    if (gitState.hasUpstream) {
      const pull = await this.runRemoteGitAction(remoteScope, { action: 'pullFastForward' });
      if (pull.exitCode !== 0) {
        throw new Error('Could not pull remote branch.');
      }
      const nextGitState = await this.readRemoteSidebarGitState(remoteScope);
      if (nextGitState.aheadCount > 0) {
        await this.pushRemoteCurrentBranch(remoteScope, nextGitState);
      }
      return;
    }
    await this.pushRemoteCurrentBranch(remoteScope, gitState);
  },

  async shouldBypassRemoteMissingBeadsDatabasePreCommitHook(
    this: GpuiSidebarRuntime,
    remoteScope: GpuiRemoteProjectScope
  ): Promise<boolean> {
    const beadsStorage = await this.runRemoteBeadsAction(remoteScope, { action: 'storageExists' });
    if (beadsStorage.exitCode !== 0 || beadsStorage.stdout.trim() !== 'true') {
      return false;
    }
    try {
      const status = await this.runRemoteBeadsAction(remoteScope, { action: 'status' });
      return status.exitCode !== 0 && isMissingGpuiBeadsDatabaseError(`${status.stderr}\n${status.stdout}`);
    } catch {
      return false;
    }
  },

  async shouldBypassMissingBeadsDatabasePreCommitHook(
    this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState
  ): Promise<boolean> {
    const beadsStorage = await this.runBeadsAction(project, { action: 'storageExists' });
    if (beadsStorage.exitCode !== 0 || beadsStorage.stdout.trim() !== 'true') {
      return false;
    }
    try {
      const status = await this.runBeadsAction(project, { action: 'status' });
      return status.exitCode !== 0 && isMissingGpuiBeadsDatabaseError(`${status.stderr}\n${status.stdout}`);
    } catch {
      return false;
    }
  },
};
