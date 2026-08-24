/*
CDXC:GxserverRuntimeSplit 2026-08-23:
Directory split of gxserver-runtime/git.ts (~3,251 lines) into git/. This
slice covers the typed-operation dispatchers for Git/worktree/GitHub/Beads
actions, pull request creation, and the shared Git toast helper. See
`index.ts` for how the runtime's Git methods are recombined.
*/
import { GPUI_MUTATING_GIT_ACTIONS } from '../constants';
import type { GpuiSidebarRuntime } from '../core';
import { createGpuiGitToastId, gpuiUserVisibleGitErrorMessage } from '../helpers/git';
import type {
  GpuiRemoteCreatePullRequestResult,
  GpuiRemoteProjectReference,
  GpuiRemoteProjectScope,
} from '../types-and-protocol';
import { postAppModalHostMessage } from '@/packages/core-ui/app-modal-host-bridge';
import type { AppToastLevel } from '@/packages/shared/app-toast-contract';
import { createAppToastRequest } from '@/packages/shared/app-toast-contract';
import type {
  GxserverCreatePullRequestResult,
  GxserverProjectDomainState,
  GxserverTypedOperationResult,
} from '@/packages/shared/gxserver-protocol';

export const gpuiSidebarRuntimeGitTypedOperationsMethods = {
  async runGitAction(
    this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState,
    params: Record<string, unknown>
  ): Promise<GxserverTypedOperationResult> {
    if (!this.client) {
      throw new Error('gxserver is unavailable.');
    }
    /*
    CDXC:SidebarGitMemo 2026-07-29:
    Invalidate at the single chokepoint every Git write goes through, so no
    caller can commit, push, or switch branches and then have a switch back to
    that project republish the pre-mutation state. Deleting before the RPC also
    covers a write that fails halfway.
    */
    if (GPUI_MUTATING_GIT_ACTIONS.has(String(params.action ?? ''))) {
      this.gitStateMemoByProjectId.delete(project.projectId);
      const result = await this.client.rpc<GxserverTypedOperationResult>('/api/runGitAction', {
        ...params,
        projectId: project.projectId,
      });
      /*
      CDXC:SidebarDiffStatsChurn 2026-08-16:
      The background diff-stats cycle stretches with sidebar size, so a commit
      or checkout could otherwise leave the project header's +/- counts stale
      for the better part of a minute. Re-probe this one project right after
      the write instead of waiting the cycle out.
      */
      void this.refreshProjectDiffStats(project);
      return result;
    }
    return this.client.rpc<GxserverTypedOperationResult>('/api/runGitAction', {
      ...params,
      projectId: project.projectId,
    });
  },

  async runRemoteGitAction(
    this: GpuiSidebarRuntime,
    remoteScope: GpuiRemoteProjectReference,
    params: Record<string, unknown>
  ): Promise<GxserverTypedOperationResult> {
    return this.requestRemoteGxserver<GxserverTypedOperationResult>(remoteScope.machineId, '/api/runGitAction', {
      ...params,
      projectId: remoteScope.projectId,
    });
  },

  /*
  CDXC:WorktreeRename 2026-08-09-18:40:
  Worktree actions scope to the PARENT project, not the worktree's own row: the
  typed operation derives the worktree family root from the parent of its cwd and
  then refuses a path equal to that cwd, so passing the worktree's id makes the
  operation refuse to act on itself. `createProjectWorktree` already sends the
  parent for the same reason.
  */
  async runWorktreeAction(
    this: GpuiSidebarRuntime,
    parentProject: GxserverProjectDomainState,
    params: Record<string, unknown>
  ): Promise<GxserverTypedOperationResult> {
    if (!this.client) {
      throw new Error('gxserver is unavailable.');
    }
    return this.client.rpc<GxserverTypedOperationResult>('/api/runWorktreeAction', {
      ...params,
      projectId: parentProject.projectId,
    });
  },

  async runGitHubAction(
    this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState,
    params: Record<string, unknown>
  ): Promise<GxserverTypedOperationResult> {
    if (!this.client) {
      throw new Error('gxserver is unavailable.');
    }
    return this.client.rpc<GxserverTypedOperationResult>('/api/runGitHubAction', {
      ...params,
      projectId: project.projectId,
    });
  },

  async runRemoteGitHubAction(
    this: GpuiSidebarRuntime,
    remoteScope: GpuiRemoteProjectReference,
    params: Record<string, unknown>
  ): Promise<GxserverTypedOperationResult> {
    return this.requestRemoteGxserver<GxserverTypedOperationResult>(remoteScope.machineId, '/api/runGitHubAction', {
      ...params,
      projectId: remoteScope.projectId,
    });
  },

  async createPullRequest(
    this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState
  ): Promise<GxserverCreatePullRequestResult> {
    if (!this.client) {
      throw new Error('gxserver is unavailable.');
    }
    /*
    CDXC:GPUISidebarGit 2026-06-24-16:28:
    Direct GPUI PR creation must use a gxserver completion result before opening
    the PR or deleting a worktree. The renderer sends only the trusted project
    id; gxserver owns `gh pr create --fill`, current-branch PR lookup, and
    validated state/URL return data.

    CDXC:SidebarGitMemo 2026-07-29:
    This is the sidebar's only pull-request write, so it is the one place the
    long GitHub lease must be torn down: otherwise the badge could keep saying
    "no pull request" for minutes after the user just created one.
    */
    this.gitHubStateMemoByProjectId.delete(project.projectId);
    this.gitStateMemoByProjectId.delete(project.projectId);
    return this.client.rpc<GxserverCreatePullRequestResult>('/api/createPullRequest', {
      projectId: project.projectId,
    });
  },

  async createRemotePullRequest(
    this: GpuiSidebarRuntime,
    remoteScope: GpuiRemoteProjectReference
  ): Promise<GpuiRemoteCreatePullRequestResult> {
    return this.requestRemoteGxserver<GpuiRemoteCreatePullRequestResult>(
      remoteScope.machineId,
      '/api/createPullRequest',
      {
        projectId: remoteScope.projectId,
      },
      { timeoutMs: 45_000 }
    );
  },

  async runBeadsAction(
    this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState,
    params: Record<string, unknown>
  ): Promise<GxserverTypedOperationResult> {
    if (!this.client) {
      throw new Error('gxserver is unavailable.');
    }
    return this.client.rpc<GxserverTypedOperationResult>('/api/runBeadsAction', {
      ...params,
      projectId: project.projectId,
    });
  },

  async runRemoteBeadsAction(
    this: GpuiSidebarRuntime,
    remoteScope: GpuiRemoteProjectReference,
    params: Record<string, unknown>
  ): Promise<GxserverTypedOperationResult> {
    return this.requestRemoteGxserver<GxserverTypedOperationResult>(
      remoteScope.machineId,
      '/api/runBeadsAction',
      {
        ...params,
        projectId: remoteScope.projectId,
      },
      { timeoutMs: 60_000 }
    );
  },

  async runRemoteGitMutation(
    this: GpuiSidebarRuntime,
    remoteScope: GpuiRemoteProjectScope,
    startedTitle: string,
    finishedTitle: string,
    operation: () => Promise<void>
  ): Promise<boolean> {
    const toastId = createGpuiGitToastId();
    this.postGitToast('info', startedTitle, { persistent: true, toastId });
    try {
      await operation();
      await this.refreshRemotePresentationFromGxserver(remoteScope.machineId).catch(() => undefined);
      this.postGitToast('success', finishedTitle, { toastId });
      return true;
    } catch (error) {
      this.postGitToast('error', `${startedTitle} failed`, {
        description: gpuiUserVisibleGitErrorMessage(error, 'Remote gxserver Git operation failed.'),
        toastId,
      });
      return false;
    }
  },

  postGitToast(
    this: GpuiSidebarRuntime,
    level: AppToastLevel,
    title: string,
    options: {
      description?: string;
      persistent?: boolean;
      toastId?: string;
    } = {}
  ): void {
    try {
      postAppModalHostMessage(
        createAppToastRequest(level, title, options.description, {
          persistent: options.persistent,
          toastId: options.toastId,
        }),
        'AppModals:gpuiGitToast'
      );
    } catch {
      /*
      CDXC:GPUISidebarGit 2026-06-24-15:22:
      Git mutations and agent workflows must not depend on toast-host availability. Missing toast presentation is not a reason to fake success or skip gxserver-owned Git state changes.
      */
    }
  },
};
