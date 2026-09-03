/*
CDXC:RepoStructure 2026-08-23:
Directory split of gxserver-runtime/git.ts (~3,251 lines) into git/. This
slice covers the background Git-polling driver and project diff-stats
methods. See `index.ts` for how the runtime's Git methods are recombined.
*/
import {
  GPUI_PROJECT_DIFF_STATS_BACKGROUND_INTERVAL_MS,
  GPUI_PROJECT_DIFF_STATS_MIN_PROBE_SPACING_MS,
} from '../constants';
import type { GpuiSidebarRuntime } from '../core';
import { createGpuiSidebarSettings } from '../helpers/bootstrap';
import { chunkUntrackedLineCountPaths, haveSameSidebarProjectDiffStats } from '../helpers/git';
import { isGpuiPresentationQuickDomainProject } from '../helpers/presentation-projection';
import {
  createGpuiRemotePresentationProjectId,
  parseGpuiRemotePresentationProjectId,
} from '../helpers/remote-presentation';
import { normalizeGpuiProjectPath } from '../helpers/worktrees';
import type { GpuiProjectDiffStatsRefreshTarget, GpuiRemoteProjectReference } from '../types-and-protocol';
import type { GxserverProjectDomainState } from '@/packages/shared/gxserver-protocol';
import type { SidebarProjectDiffStats } from '@/packages/shared/project-diff-stats';
import {
  createDefaultSidebarProjectDiffStats,
  parseGitNumstatDiffStats,
  parseGitZeroDelimitedPaths,
  resolveSidebarProjectDiffStats,
} from '@/packages/shared/project-diff-stats';
import type { SidebarSessionGroup } from '@/packages/shared/session-grid-contract';

export const gpuiSidebarRuntimeGitDiffStatsMethods = {
  /**
   * One background Git polling driver owns both project diff stats (all
   * visible non-Quick projects, local and remote) and the full titlebar Git
   * state (active local project only). Individual project probes stagger
   * across the interval so large sidebars do not shell out for every repo at
   * once, matching the macOS refresh loop.
   */
  startGitPollingDriver(this: GpuiSidebarRuntime): void {
    if (this.gitPollingCycleTimeoutId !== undefined) {
      return;
    }
    this.scheduleGitPollingCycle();
  },

  scheduleGitPollingCycle(this: GpuiSidebarRuntime): void {
    for (const timeoutId of this.gitPollingTimeoutIds) {
      window.clearTimeout(timeoutId);
    }
    this.gitPollingTimeoutIds.clear();
    const targets = this.getVisibleProjectDiffStatsRefreshTargets();
    /*
    CDXC:Git 2026-08-16:
    The cycle stretches past the base interval once the sidebar renders more
    project rows than the interval can hold at the capped probe rate, so a
    sidebar with 100+ rows polls each row less often instead of probing many
    times per second.
    */
    const cycleLengthMs = Math.max(
      GPUI_PROJECT_DIFF_STATS_BACKGROUND_INTERVAL_MS,
      targets.length * GPUI_PROJECT_DIFF_STATS_MIN_PROBE_SPACING_MS
    );
    const staggerStepMs = cycleLengthMs / Math.max(1, targets.length);
    targets.forEach((target, index) => {
      const timeoutId = window.setTimeout(
        () => {
          this.gitPollingTimeoutIds.delete(timeoutId);
          this.refreshProjectDiffStatsTarget(target);
        },
        Math.floor(index * staggerStepMs)
      );
      this.gitPollingTimeoutIds.add(timeoutId);
    });
    this.gitPollingCycleTimeoutId = window.setTimeout(() => {
      this.gitPollingCycleTimeoutId = undefined;
      this.scheduleGitPollingCycle();
    }, cycleLengthMs);
  },

  /*
  CDXC:Git 2026-08-16:
  Poll only projects that currently render a sidebar group header, instead of
  every registered local project plus every project of every connected remote
  machine. Diff stats exist purely for those headers, so the rendered group
  projection is the authoritative visibility set; remote machines that are only
  showing a stale last-seen snapshot are unreachable and are skipped.
  */
  getVisibleProjectDiffStatsRefreshTargets(this: GpuiSidebarRuntime): GpuiProjectDiffStatsRefreshTarget[] {
    const targetsByKey = new Map<string, GpuiProjectDiffStatsRefreshTarget>();
    /*
    Keyed by plain string: the lookup keys come from rendered group metadata
    (`projectContext.editor.projectId`), which is an opaque id string rather
    than a `GxserverProjectId` the compiler can vouch for.
    */
    const localProjectsById = new Map<string, GxserverProjectDomainState>(
      this.domainProjects.map((project) => [project.projectId, project])
    );
    for (const group of this.latestGroups) {
      const projectId = group.projectContext?.editor.projectId;
      if (!projectId) {
        continue;
      }
      const remoteReference = parseGpuiRemotePresentationProjectId(projectId);
      if (remoteReference) {
        if (!this.remotePresentations.has(remoteReference.machineId)) {
          continue;
        }
        targetsByKey.set(`remote:${projectId}`, {
          key: `remote:${projectId}`,
          kind: 'remote',
          reference: remoteReference,
        });
        continue;
      }
      const project = this.client ? localProjectsById.get(projectId) : undefined;
      if (
        !project ||
        isGpuiPresentationQuickDomainProject(project) ||
        project.isRecentProject === true ||
        !normalizeGpuiProjectPath(project.path)
      ) {
        continue;
      }
      targetsByKey.set(`local:${projectId}`, {
        key: `local:${projectId}`,
        kind: 'local',
        project,
      });
    }
    return [...targetsByKey.values()].sort((left, right) => left.key.localeCompare(right.key));
  },

  refreshProjectDiffStatsTarget(this: GpuiSidebarRuntime, target: GpuiProjectDiffStatsRefreshTarget): void {
    if (target.kind === 'remote') {
      void this.refreshRemoteProjectDiffStats(target.reference);
      return;
    }
    void this.refreshProjectDiffStats(target.project);
    if (this.activeProjectId === target.project.projectId) {
      /*
      CDXC:Git 2026-07-29:
      This background cycle runs every 15s and can land on the same instant as a
      project switch. Local Git probes stay on their 15s cadence, but the
      GitHub CLI probe defers so this loop cannot reintroduce a `gh pr view`
      network call into a switch-time RPC burst, and so PR state is re-fetched
      on its own (much slower) lease instead of four times a minute.
      */
      void this.refreshGitState({
        deferGitHub: true,
        force: true,
        project: target.project,
        toastOnFailure: false,
      });
    }
  },

  async refreshProjectDiffStats(this: GpuiSidebarRuntime, project: GxserverProjectDomainState): Promise<void> {
    const projectId = project.projectId;
    if (this.pendingProjectDiffRefreshProjectIds.has(projectId) || !this.client) {
      return;
    }
    this.pendingProjectDiffRefreshProjectIds.add(projectId);
    /*
    CDXC:Git 2026-08-16:
    Background polls must be invisible unless the numbers actually change:
    the old pre-probe `isLoading: true` republish plus the post-probe
    republish meant every poll of every project rebuilt and re-sent the whole
    sidebar tree twice, even with nothing to report. `isLoading` only feeds a
    hover tooltip, so silent background probes simply publish the resolved
    stats (deduplicated inside setProjectDiffStats).
    */
    try {
      if (!this.gitRepoProjectIds.has(projectId)) {
        const repoCheck = await this.runGitAction(project, { action: 'isInsideWorkTree' });
        if (repoCheck.exitCode !== 0 || repoCheck.stdout.trim() !== 'true') {
          this.setProjectDiffStats(projectId, createDefaultSidebarProjectDiffStats(false));
          return;
        }
        this.gitRepoProjectIds.add(projectId);
      }
      const trackedDiff = await this.runGitAction(project, { action: 'diffNumstat' });
      if (trackedDiff.exitCode !== 0) {
        this.gitRepoProjectIds.delete(projectId);
        return;
      }
      const trackedStats = parseGitNumstatDiffStats(trackedDiff.stdout);
      const hasTrackedLineChanges = trackedStats.additions > 0 || trackedStats.deletions > 0;
      const settings = createGpuiSidebarSettings(this.runtimeSettings);
      let resolvedStats = trackedStats;
      if (settings.showUntrackedProjectDiffWhenNoTrackedChanges && !hasTrackedLineChanges) {
        const untrackedFiles = await this.runGitAction(project, { action: 'listUntracked' });
        const untrackedPaths = parseGitZeroDelimitedPaths(untrackedFiles.stdout);
        resolvedStats = resolveSidebarProjectDiffStats({
          showUntrackedWhenNoTrackedChanges: true,
          trackedStats,
          untrackedStats: {
            additions: await this.countUntrackedProjectLines(project, untrackedPaths),
            deletions: 0,
            files: untrackedPaths.length,
            isLoading: false,
            isRepo: true,
          },
        });
      }
      this.setProjectDiffStats(projectId, resolvedStats);
    } catch {
      // Keep the last published stats; the next polling cycle re-probes.
    } finally {
      this.pendingProjectDiffRefreshProjectIds.delete(projectId);
    }
  },

  async refreshRemoteProjectDiffStats(this: GpuiSidebarRuntime, reference: GpuiRemoteProjectReference): Promise<void> {
    const scopedProjectId = createGpuiRemotePresentationProjectId(reference.machineId, reference.projectId);
    if (this.pendingProjectDiffRefreshProjectIds.has(scopedProjectId)) {
      return;
    }
    this.pendingProjectDiffRefreshProjectIds.add(scopedProjectId);
    // Silent background probe: publish only resolved, changed stats (see
    // refreshProjectDiffStats for the churn rationale).
    try {
      if (!this.gitRepoProjectIds.has(scopedProjectId)) {
        const repoCheck = await this.runRemoteGitAction(reference, {
          action: 'isInsideWorkTree',
        });
        if (repoCheck.exitCode !== 0 || repoCheck.stdout.trim() !== 'true') {
          this.setProjectDiffStats(scopedProjectId, createDefaultSidebarProjectDiffStats(false));
          return;
        }
        this.gitRepoProjectIds.add(scopedProjectId);
      }
      const trackedDiff = await this.runRemoteGitAction(reference, { action: 'diffNumstat' });
      if (trackedDiff.exitCode !== 0) {
        this.gitRepoProjectIds.delete(scopedProjectId);
        return;
      }
      const trackedStats = parseGitNumstatDiffStats(trackedDiff.stdout);
      const hasTrackedLineChanges = trackedStats.additions > 0 || trackedStats.deletions > 0;
      const settings = createGpuiSidebarSettings(this.runtimeSettings);
      let resolvedStats = trackedStats;
      if (settings.showUntrackedProjectDiffWhenNoTrackedChanges && !hasTrackedLineChanges) {
        const untrackedFiles = await this.runRemoteGitAction(reference, {
          action: 'listUntracked',
        });
        const untrackedPaths = parseGitZeroDelimitedPaths(untrackedFiles.stdout);
        resolvedStats = resolveSidebarProjectDiffStats({
          showUntrackedWhenNoTrackedChanges: true,
          trackedStats,
          untrackedStats: {
            additions: await this.countRemoteUntrackedProjectLines(reference, untrackedPaths),
            deletions: 0,
            files: untrackedPaths.length,
            isLoading: false,
            isRepo: true,
          },
        });
      }
      this.setProjectDiffStats(scopedProjectId, resolvedStats);
    } catch {
      // Keep the last published stats; the next polling cycle re-probes.
    } finally {
      this.pendingProjectDiffRefreshProjectIds.delete(scopedProjectId);
    }
  },

  async countUntrackedProjectLines(
    this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState,
    paths: readonly string[]
  ): Promise<number> {
    let lines = 0;
    for (const filePaths of chunkUntrackedLineCountPaths(paths)) {
      const result = await this.runGitAction(project, {
        action: 'countFileLines',
        filePaths,
      });
      if (result.exitCode !== 0) {
        throw new Error('Could not count untracked file lines.');
      }
      lines += Number(result.stdout.trim()) || 0;
    }
    return lines;
  },

  async countRemoteUntrackedProjectLines(
    this: GpuiSidebarRuntime,
    reference: GpuiRemoteProjectReference,
    paths: readonly string[]
  ): Promise<number> {
    let lines = 0;
    for (const filePaths of chunkUntrackedLineCountPaths(paths)) {
      const result = await this.runRemoteGitAction(reference, {
        action: 'countFileLines',
        filePaths,
      });
      if (result.exitCode !== 0) {
        throw new Error('Could not count remote untracked file lines.');
      }
      lines += Number(result.stdout.trim()) || 0;
    }
    return lines;
  },

  setProjectDiffStats(this: GpuiSidebarRuntime, projectId: string, stats: SidebarProjectDiffStats): void {
    const previous = this.projectDiffStatsByProjectId.get(projectId);
    this.projectDiffStatsByProjectId.set(projectId, stats);
    if (!this.hasHydrated || (previous && haveSameSidebarProjectDiffStats(previous, stats))) {
      return;
    }
    this.publishProjectDiffStatsPatch(projectId);
  },

  /*
  CDXC:Git 2026-08-16:
  Diff stats live inside the group projection (projectContext.editor), but a
  changed +/- count for one project must not rebuild and re-send all groups:
  with 40+ groups the old full `publishRemotePresentationPatch` here ran many
  times per second and pinned the sidebar renderer in GC. Patch only the
  group rows owned by the project, reusing every other group reference, and
  skip the HUD message entirely (the HUD carries no diff stats). Bridge posts
  (status pet, active project context, focus state, titlebar Git menu) carry
  no diff stats either, so they stay out of this path too.
  */
  publishProjectDiffStatsPatch(this: GpuiSidebarRuntime, projectId: string): void {
    const stats = this.projectDiffStatsByProjectId.get(projectId);
    if (!stats) {
      return;
    }
    const changedGroups: SidebarSessionGroup[] = [];
    const nextGroups = this.latestGroups.map((group) => {
      const projectContext = group.projectContext;
      if (!projectContext || projectContext.editor.projectId !== projectId) {
        return group;
      }
      const nextGroup = {
        ...group,
        projectContext: {
          ...projectContext,
          editor: { ...projectContext.editor, diffStats: stats },
        },
      };
      changedGroups.push(nextGroup);
      return nextGroup;
    });
    if (changedGroups.length === 0) {
      // No rendered group shows this project; the stored stats overlay onto
      // the next full projection rebuild instead.
      return;
    }
    this.latestGroups = nextGroups;
    this.messageSource.postMessage({
      groupOrder: nextGroups.map((group) => group.groupId),
      groups: changedGroups,
      removedGroupIds: [],
      removedSessionIds: [],
      revision: ++this.revision,
      type: 'sidebarGroupsChanged',
    });
  },
};
