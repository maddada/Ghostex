/*
CDXC:GxserverRuntimeSplit 2026-08-23:
Directory split of gxserver-runtime/git.ts (~3,251 lines) into git/. This
slice covers the titlebar Git menu state, the local/remote Git state
refresh pipeline, and the GitHub (`gh`) state probe. See `index.ts` for how
the runtime's Git methods are recombined.
*/
import {
  GPUI_SIDEBAR_BOOTSTRAP_MAX_ATTEMPTS,
  GPUI_SIDEBAR_BOOTSTRAP_RETRY_DELAY_MS,
  GPUI_SIDEBAR_GIT_HUB_DEFERRED_PROBE_DELAY_MS,
} from '../constants';
import type { GpuiSidebarRuntime } from '../core';
import {
  createGpuiTitlebarGitMenuStatePayload,
  mergeGpuiGitChangedFiles,
  normalizeGpuiGitHubRemoteUrl,
  normalizeGpuiRelativeGitFilePath,
  parseGpuiGitCommitModalCommand,
  parseGpuiGitHubPullRequest,
  parseGpuiGitNumstatFiles,
  parseGpuiGitStatusPorcelainFiles,
  parseGpuiTitlebarGitAction,
  summarizeGpuiGitChangedFiles,
} from '../helpers/git';
import { isGpuiPresentationQuickDomainProject } from '../helpers/presentation-projection';
import { stringFromRecord } from '../helpers/records';
import {
  createGpuiRemotePresentationGroupId,
  parseGpuiRemotePresentationGroupId,
} from '../helpers/remote-presentation';
import { normalizeGpuiProjectPath, normalizeGpuiWorktreeParentProjectId } from '../helpers/worktrees';
import type { GpuiRemoteProjectScope, GpuiSidebarGitHubState } from '../types-and-protocol';
import type { GxserverProjectDomainState } from '@/packages/shared/gxserver-protocol';
import { parseGitZeroDelimitedPaths } from '@/packages/shared/project-diff-stats';
import type { SidebarToExtensionMessage } from '@/packages/shared/session-grid-contract';
import type { SidebarGitState } from '@/packages/shared/sidebar-git';
import { createDefaultSidebarGitState } from '@/packages/shared/sidebar-git';

export const gpuiSidebarRuntimeGitStateAndGithubMethods = {
  postTitlebarGitMenuState(this: GpuiSidebarRuntime, attempt = 0): void {
    if (this.titlebarGitMenuStateRetryId !== undefined) {
      window.clearTimeout(this.titlebarGitMenuStateRetryId);
      this.titlebarGitMenuStateRetryId = undefined;
    }
    const postTitlebarGitMenuState = window.ghostexGpui?.postTitlebarGitMenuState;
    if (typeof postTitlebarGitMenuState !== 'function') {
      if (attempt < GPUI_SIDEBAR_BOOTSTRAP_MAX_ATTEMPTS) {
        this.titlebarGitMenuStateRetryId = window.setTimeout(() => {
          this.postTitlebarGitMenuState(attempt + 1);
        }, GPUI_SIDEBAR_BOOTSTRAP_RETRY_DELAY_MS);
      }
      return;
    }
    const payload = JSON.stringify(createGpuiTitlebarGitMenuStatePayload(this.gitStateForHud()));
    if (payload === this.lastTitlebarGitMenuStatePayload) {
      return;
    }
    this.lastTitlebarGitMenuStatePayload = payload;
    postTitlebarGitMenuState(payload);
  },

  handleGpuiTitlebarGitAction(this: GpuiSidebarRuntime, payload: unknown): void {
    const action = parseGpuiTitlebarGitAction(payload);
    if (!action) {
      return;
    }
    if (action === 'refresh') {
      this.refreshTitlebarGitMenuState();
      return;
    }
    void this.runSidebarGitAction({
      ...(this.activeGroupId ? { groupId: this.activeGroupId } : {}),
      action,
      type: 'runSidebarGitAction',
    });
  },

  async handleGpuiGitCommitModalCommand(this: GpuiSidebarRuntime, payload: unknown): Promise<void> {
    const message = parseGpuiGitCommitModalCommand(payload);
    if (!message) {
      return;
    }
    await this.handleSidebarMessage(message);
  },

  refreshTitlebarGitMenuState(this: GpuiSidebarRuntime): void {
    if (this.activeGroupId && parseGpuiRemotePresentationGroupId(this.activeGroupId)) {
      void this.refreshGitStateForMessage({
        groupId: this.activeGroupId,
        type: 'refreshGitState',
      });
      return;
    }
    const project = this.activeDomainProject();
    if (!project) {
      return;
    }
    void this.refreshGitState({ force: true, project, toastOnFailure: false });
  },

  refreshGitStateForActiveProjectIfNeeded(this: GpuiSidebarRuntime): void {
    const project = this.activeDomainProject();
    if (!project || project.projectId === this.lastGitRefreshProjectId) {
      return;
    }
    this.lastGitRefreshProjectId = project.projectId;
    /*
    CDXC:SidebarGitMemo 2026-07-29:
    Project switching is on the critical path of terminal attach: every RPC this
    fires competes with the attach RPCs on the same daemon. A project the user
    switched away from seconds ago has not changed on disk, so publish its
    memoized state and issue nothing at all. Only a cold or stale project pays
    for a fan-out, and that fan-out leaves the GitHub CLI probe out of the
    burst.
    */
    const memoizedState = this.gitStateMemoByProjectId.get(project.projectId, Date.now());
    if (memoizedState) {
      this.gitState = this.applyLiveGitStateOverlays(project, memoizedState);
      this.publishHudPatch();
      return;
    }
    void this.refreshGitState({ deferGitHub: true, project, toastOnFailure: false });
  },

  /**
   * Re-apply the parts of a published Git state that the runtime mutates
   * outside a refresh, so a memoized state can never resurrect stale values:
   * Git preferences are patched straight onto `this.gitState` when the user
   * changes them, and GitHub CLI results carry their own longer-lived lease.
   */
  applyLiveGitStateOverlays(
    this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState,
    state: SidebarGitState
  ): SidebarGitState {
    const preferences = this.gitPreferencesForProject(project);
    const gitHubState = state.isRepo ? this.gitHubStateMemoByProjectId.peek(project.projectId) : undefined;
    return {
      ...state,
      ...(gitHubState ?? {}),
      confirmSuggestedCommit: preferences.confirmCommit,
      generateCommitBody: preferences.generateCommitBody,
      primaryAction: preferences.primaryAction,
    };
  },

  async refreshGitState(
    this: GpuiSidebarRuntime,
    {
      deferGitHub = false,
      force = false,
      project = this.activeDomainProject(),
      publishBusy = false,
      toastOnFailure = false,
    }: {
      /**
       * Leave `gh --version` / `gh pr view` out of the fan-out and publish the
       * memoized GitHub state instead, scheduling a probe once the local Git
       * state is out. Only background and switch-driven refreshes set this;
       * every caller that reads `pr` / `hasGitHubCli` off the returned state
       * keeps the synchronous probe.
       */
      deferGitHub?: boolean;
      force?: boolean;
      project?: GxserverProjectDomainState;
      publishBusy?: boolean;
      toastOnFailure?: boolean;
    } = {}
  ): Promise<SidebarGitState> {
    if (!project) {
      this.gitState = createDefaultSidebarGitState();
      this.publishHudPatch();
      return this.gitState;
    }
    if (force) {
      this.lastGitRefreshProjectId = project.projectId;
    }
    const nextState = await this.readSidebarGitState(project, {
      deferGitHub,
      publishBusy,
      toastOnFailure,
    });
    if (this.activeProjectId === project.projectId) {
      this.gitState = nextState;
      this.publishHudPatch();
    }
    return nextState;
  },

  async refreshGitStateForMessage(
    this: GpuiSidebarRuntime,
    message: Extract<SidebarToExtensionMessage, { type: 'refreshGitState' }>
  ): Promise<void> {
    /*
    CDXC:GPUISidebarGit 2026-06-24-21:26:
    Reused Git controls can refresh from a scoped local or remote project row. Resolve that owner before reading Git state; unscoped callers keep the active-project behavior, but scoped remote rows must never refresh the active local project by accident.
    */
    const explicitScope = Boolean(message.groupId?.trim() || message.projectId?.trim());
    const remoteScope = this.resolveGitPreferenceRemoteScope(message);
    if (remoteScope) {
      const activeRemoteGroupId = createGpuiRemotePresentationGroupId(remoteScope.machineId, remoteScope.projectId);
      if (this.activeGroupId === activeRemoteGroupId) {
        const preferences = this.gitPreferencesForPresentationProject(
          this.findRemotePresentationProject(remoteScope) ?? remoteScope.project
        );
        this.gitState = {
          ...createDefaultSidebarGitState(
            preferences.primaryAction,
            preferences.confirmCommit,
            preferences.generateCommitBody
          ),
          isBusy: true,
        };
        this.publishHudPatch();
      }
      const nextState = await this.readRemoteSidebarGitState(remoteScope);
      if (this.activeGroupId === activeRemoteGroupId) {
        this.gitState = nextState;
        this.publishHudPatch();
      }
      return;
    }
    if (explicitScope && this.isGitPreferenceRemoteScope(message)) {
      this.postRemoteToast('warning', 'Remote Git unavailable', {
        description: 'Reconnect the remote machine before refreshing Git state.',
      });
      return;
    }
    const project =
      this.resolveGitPreferenceLocalProject(message) ?? (explicitScope ? undefined : this.activeDomainProject());
    if (!project) {
      this.postGitToast('warning', 'Git unavailable', {
        description: 'No active gxserver project is available.',
      });
      return;
    }
    await this.refreshGitState({
      force: true,
      project,
      publishBusy: true,
      toastOnFailure: true,
    });
  },

  async readSidebarGitState(
    this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState,
    options: { deferGitHub?: boolean; publishBusy?: boolean; toastOnFailure?: boolean } = {}
  ): Promise<SidebarGitState> {
    const baseState = createDefaultSidebarGitState(
      this.gitPreferencesForProject(project).primaryAction,
      this.gitPreferencesForProject(project).confirmCommit,
      this.gitPreferencesForProject(project).generateCommitBody
    );
    if (
      !this.client ||
      project.isRecentProject === true ||
      isGpuiPresentationQuickDomainProject(project) ||
      !normalizeGpuiProjectPath(project.path)
    ) {
      return { ...baseState, hasCheckedGitHubRemote: true, isRepo: false };
    }
    if (options.publishBusy && this.activeProjectId === project.projectId) {
      this.gitState = { ...baseState, isBusy: true };
      this.publishHudPatch();
    }
    try {
      const repoCheck = await this.runGitAction(project, { action: 'isInsideWorkTree' });
      if (repoCheck.exitCode !== 0 || repoCheck.stdout.trim() !== 'true') {
        return this.memoizeSidebarGitState(project, {
          ...baseState,
          hasCheckedGitHubRemote: true,
          isRepo: false,
        });
      }

      /*
      CDXC:SidebarGitMemo 2026-07-29:
      `deferGitHub` keeps the two `gh` subprocesses (one of them a network call
      with a 120s server-side timeout) out of the burst a project switch fires.
      The switch publishes local Git state with the last known GitHub answer
      overlaid, and `scheduleDeferredGitHubProbe` fills in a fresh one shortly
      after, once the attach traffic has drained.
      */
      const [branch, status, diff, untrackedFiles, upstream, remotes, originRemote, gitHubState] = await Promise.all([
        this.runGitAction(project, { action: 'branch' }),
        this.runGitAction(project, { action: 'statusPorcelain' }),
        this.runGitAction(project, { action: 'diffNumstat' }),
        this.runGitAction(project, { action: 'listUntracked' }),
        this.runGitAction(project, { action: 'upstreamCounts' }),
        this.runGitAction(project, { action: 'listRemotes' }),
        this.runGitAction(project, { action: 'getOriginRemoteUrl' }),
        options.deferGitHub === true ? this.memoizedGitHubState(project) : this.readGitHubState(project),
      ]);
      if (options.deferGitHub === true) {
        this.scheduleDeferredGitHubProbeIfStale(project);
      }
      const files = mergeGpuiGitChangedFiles([
        ...parseGpuiGitNumstatFiles(diff.stdout),
        ...parseGpuiGitStatusPorcelainFiles(status.stdout),
        ...parseGitZeroDelimitedPaths(untrackedFiles.stdout).flatMap((path) => {
          const normalizedPath = normalizeGpuiRelativeGitFilePath(path);
          return normalizedPath
            ? [
                {
                  additions: 0,
                  deletions: 0,
                  path: normalizedPath,
                },
              ]
            : [];
        }),
      ]);
      const totals = summarizeGpuiGitChangedFiles(files);
      const upstreamParts = upstream.exitCode === 0 ? upstream.stdout.trim().split(/\s+/) : [];
      return this.memoizeSidebarGitState(project, {
        ...baseState,
        additions: totals.additions,
        aheadCount: Number(upstreamParts[0] || 0) || 0,
        behindCount: Number(upstreamParts[1] || 0) || 0,
        branch: branch.stdout.trim() || null,
        deletions: totals.deletions,
        hasCheckedGitHubRemote: true,
        hasGitHubCli: gitHubState.hasGitHubCli,
        hasGitHubRemote: originRemote.exitCode === 0 && normalizeGpuiGitHubRemoteUrl(originRemote.stdout) !== undefined,
        hasOriginRemote: remotes.stdout.split(/\s+/).includes('origin'),
        hasUpstream: upstream.exitCode === 0,
        hasWorkingTreeChanges: status.stdout.trim().length > 0,
        isBusy: false,
        isRepo: true,
        files,
        isWorktree: normalizeGpuiWorktreeParentProjectId(project.worktree) !== undefined,
        pr: gitHubState.pr,
        worktreeName: stringFromRecord(project.worktree, 'name'),
      });
    } catch {
      if (options.toastOnFailure) {
        this.postGitToast('error', 'Could not refresh Git state', {
          description: 'gxserver could not inspect the selected project.',
        });
      }
      /*
      CDXC:SidebarGitMemo 2026-07-29:
      A failed probe is not a cacheable answer. Drop any memoized entry so the
      next switch re-probes instead of republishing a state gxserver could no
      longer confirm.
      */
      this.gitStateMemoByProjectId.delete(project.projectId);
      return { ...baseState, isBusy: false };
    }
  },

  /**
   * Remember a freshly computed Git state for this project so a switch back to
   * it inside the memo TTL publishes without issuing any RPC.
   */
  memoizeSidebarGitState(
    this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState,
    state: SidebarGitState
  ): SidebarGitState {
    this.gitStateMemoByProjectId.set(project.projectId, state, Date.now());
    return state;
  },

  /** Run the GitHub CLI probes and memoize the pair under the longer lease. */
  async readGitHubState(
    this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState
  ): Promise<GpuiSidebarGitHubState> {
    const [ghVersion, pr] = await Promise.all([
      this.runGitHubAction(project, { action: 'version' }),
      this.runGitHubAction(project, { action: 'prView' }),
    ]);
    const gitHubState: GpuiSidebarGitHubState = {
      hasGitHubCli: ghVersion.exitCode === 0,
      pr: parseGpuiGitHubPullRequest(pr.stdout, pr.exitCode === 0),
    };
    this.gitHubStateMemoByProjectId.set(project.projectId, gitHubState, Date.now());
    return gitHubState;
  },

  /**
   * GitHub state for a refresh that must not spawn `gh`: the memoized answer,
   * stale or not. A stale-but-known answer beats a blank one, because pull
   * request state moves on a human timescale and the previous badge is the
   * accurate one far more often than an empty badge would be. A project that
   * has never been probed publishes no GitHub affordances until the deferred
   * probe lands.
   */
  memoizedGitHubState(this: GpuiSidebarRuntime, project: GxserverProjectDomainState): GpuiSidebarGitHubState {
    return this.gitHubStateMemoByProjectId.peek(project.projectId) ?? { hasGitHubCli: false, pr: null };
  },

  /**
   * Queue the GitHub probe this refresh skipped, once its lease has run out.
   * Called after the local fan-out resolves so the probe delay is measured from
   * the moment the switch-time RPC burst is actually over.
   */
  scheduleDeferredGitHubProbeIfStale(this: GpuiSidebarRuntime, project: GxserverProjectDomainState): void {
    if (
      this.gitHubStateMemoByProjectId.isFreshKey(project.projectId, Date.now()) ||
      this.pendingGitHubProbeProjectIds.has(project.projectId)
    ) {
      return;
    }
    this.pendingGitHubProbeProjectIds.add(project.projectId);
    const timeoutId = window.setTimeout(() => {
      this.gitHubProbeTimeoutIds.delete(timeoutId);
      void this.runDeferredGitHubProbe(project);
    }, GPUI_SIDEBAR_GIT_HUB_DEFERRED_PROBE_DELAY_MS);
    this.gitHubProbeTimeoutIds.add(timeoutId);
  },

  async runDeferredGitHubProbe(this: GpuiSidebarRuntime, project: GxserverProjectDomainState): Promise<void> {
    try {
      if (!this.client) {
        return;
      }
      /*
      `readGitHubState` refreshes the GitHub lease, which is all a memoized Git
      state needs: `applyLiveGitStateOverlays` reads that lease every time a
      memoized state is published, so the local-Git entry keeps its own,
      shorter, untouched lease.
      */
      const gitHubState = await this.readGitHubState(project);
      if (this.activeProjectId === project.projectId && this.gitState.isRepo) {
        this.gitState = { ...this.gitState, ...gitHubState };
        this.publishHudPatch();
      }
    } catch {
      /*
      A failed `gh` probe leaves the previous lease in place; the next
      background or switch-driven refresh reschedules it.
      */
    } finally {
      this.pendingGitHubProbeProjectIds.delete(project.projectId);
    }
  },

  async readRemoteSidebarGitState(
    this: GpuiSidebarRuntime,
    remoteScope: GpuiRemoteProjectScope
  ): Promise<SidebarGitState> {
    const remotePreferences = this.gitPreferencesForPresentationProject(
      this.findRemotePresentationProject(remoteScope) ?? remoteScope.project
    );
    const baseState = createDefaultSidebarGitState(
      remotePreferences.primaryAction,
      remotePreferences.confirmCommit,
      remotePreferences.generateCommitBody
    );
    try {
      const repoCheck = await this.runRemoteGitAction(remoteScope, { action: 'isInsideWorkTree' });
      if (repoCheck.exitCode !== 0 || repoCheck.stdout.trim() !== 'true') {
        return { ...baseState, hasCheckedGitHubRemote: true, isRepo: false };
      }

      const [branch, status, diff, untrackedFiles, upstream, remotes, originRemote, ghVersion, pr] = await Promise.all([
        this.runRemoteGitAction(remoteScope, { action: 'branch' }),
        this.runRemoteGitAction(remoteScope, { action: 'statusPorcelain' }),
        this.runRemoteGitAction(remoteScope, { action: 'diffNumstat' }),
        this.runRemoteGitAction(remoteScope, { action: 'listUntracked' }),
        this.runRemoteGitAction(remoteScope, { action: 'upstreamCounts' }),
        this.runRemoteGitAction(remoteScope, { action: 'listRemotes' }),
        this.runRemoteGitAction(remoteScope, { action: 'getOriginRemoteUrl' }),
        this.runRemoteGitHubAction(remoteScope, { action: 'version' }),
        this.runRemoteGitHubAction(remoteScope, { action: 'prView' }),
      ]);
      const files = mergeGpuiGitChangedFiles([
        ...parseGpuiGitNumstatFiles(diff.stdout),
        ...parseGpuiGitStatusPorcelainFiles(status.stdout),
        ...parseGitZeroDelimitedPaths(untrackedFiles.stdout).flatMap((path) => {
          const normalizedPath = normalizeGpuiRelativeGitFilePath(path);
          return normalizedPath
            ? [
                {
                  additions: 0,
                  deletions: 0,
                  path: normalizedPath,
                },
              ]
            : [];
        }),
      ]);
      const totals = summarizeGpuiGitChangedFiles(files);
      const upstreamParts = upstream.exitCode === 0 ? upstream.stdout.trim().split(/\s+/) : [];
      const presentationProject = this.findRemotePresentationProject(remoteScope) ?? remoteScope.project;
      return {
        ...baseState,
        additions: totals.additions,
        aheadCount: Number(upstreamParts[0] || 0) || 0,
        behindCount: Number(upstreamParts[1] || 0) || 0,
        branch: branch.stdout.trim() || null,
        deletions: totals.deletions,
        files,
        hasCheckedGitHubRemote: true,
        hasGitHubCli: ghVersion.exitCode === 0,
        hasGitHubRemote: originRemote.exitCode === 0 && normalizeGpuiGitHubRemoteUrl(originRemote.stdout) !== undefined,
        hasOriginRemote: remotes.stdout.split(/\s+/).includes('origin'),
        hasUpstream: upstream.exitCode === 0,
        hasWorkingTreeChanges: status.stdout.trim().length > 0,
        isBusy: false,
        isRepo: true,
        isWorktree: normalizeGpuiWorktreeParentProjectId(presentationProject.worktree) !== undefined,
        pr: parseGpuiGitHubPullRequest(pr.stdout, pr.exitCode === 0),
        worktreeName: stringFromRecord(presentationProject.worktree, 'name') ?? presentationProject.title,
      };
    } catch {
      this.postRemoteToast('warning', 'Remote Git unavailable', {
        description: 'The remote gxserver could not inspect the selected project.',
      });
      return { ...baseState, hasCheckedGitHubRemote: true, isBusy: false, isRepo: false };
    }
  },
};
