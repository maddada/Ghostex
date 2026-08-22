/*
CDXC:GxserverRuntimeSplit 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import {
  DEFAULT_GPUI_PROMPT_AGENT_ID,
  GPUI_GIT_MULTICOMMIT_RELEASE_PROMPT,
  GPUI_GIT_MULTIPLE_COMMITS_PROMPT,
  GPUI_GIT_RELEASE_ONLY_PROMPT,
  GPUI_MUTATING_GIT_ACTIONS,
  GPUI_PROJECT_DIFF_STATS_BACKGROUND_INTERVAL_MS,
  GPUI_PROJECT_DIFF_STATS_MIN_PROBE_SPACING_MS,
  GPUI_REMOTE_MERGE_CONFLICT_PROMPT,
  GPUI_SIDEBAR_BOOTSTRAP_MAX_ATTEMPTS,
  GPUI_SIDEBAR_BOOTSTRAP_RETRY_DELAY_MS,
  GPUI_SIDEBAR_GIT_HUB_DEFERRED_PROBE_DELAY_MS,
} from "./constants";
import type { GpuiSidebarRuntime } from "./core";
import { createGpuiSidebarSettings } from "./helpers/bootstrap";
import {
  GpuiUserVisibleGitError,
  buildGpuiGitPullRequestAgentPrompt,
  buildGpuiGitSyncWithMainPrompt,
  buildGpuiMergeConflictPrompt,
  chunkUntrackedLineCountPaths,
  createGpuiGitToastId,
  createGpuiTitlebarGitMenuStatePayload,
  formatGpuiGitAgentWorkflowTitle,
  gpuiUserVisibleGitErrorMessage,
  hasGpuiGxserverShortStatusChanges,
  haveSameSidebarProjectDiffStats,
  isGpuiConfirmedOpenPullRequest,
  isGpuiConfirmedOpenRemotePullRequest,
  isGpuiRemotePendingGitCommitRequest,
  isMissingGpuiBeadsDatabaseError,
  mergeGpuiGitChangedFiles,
  normalizeGpuiGitHubRemoteUrl,
  normalizeGpuiRelativeGitFilePath,
  parseGpuiGitCommitModalCommand,
  parseGpuiGitHubPullRequest,
  parseGpuiGitNumstatFiles,
  parseGpuiGitStatusPorcelainFiles,
  parseGpuiSidebarGitCommitMessage,
  parseGpuiTitlebarGitAction,
  resolveGpuiSidebarGitConfirmLabel,
  resolveGpuiSidebarGitFinishedTitle,
  resolveGpuiSidebarGitPromptDescription,
  resolveGpuiSidebarGitStartedTitle,
  sanitizeGpuiSidebarGitBranchName,
  summarizeGpuiGitChangedFiles,
  supportsGpuiBackgroundCommitMessageGeneration,
} from "./helpers/git";
import { isGpuiPresentationQuickDomainProject } from "./helpers/presentation-projection";
import { booleanFromRecord, stringFromRecord } from "./helpers/records";
import {
  createGpuiRemotePresentationGroupId,
  createGpuiRemotePresentationProjectId,
  parseGpuiRemotePresentationGroupId,
  parseGpuiRemotePresentationProjectId,
} from "./helpers/remote-presentation";
import {
  normalizeGpuiProjectPath,
  normalizeGpuiWorktreeMetadata,
  normalizeGpuiWorktreeParentProjectId,
} from "./helpers/worktrees";
import type {
  GpuiGitPreferences,
  GpuiPendingGitCommitRequest,
  GpuiProjectDiffStatsRefreshTarget,
  GpuiRemoteCreatePullRequestResult,
  GpuiRemoteProjectReference,
  GpuiRemoteProjectScope,
  GpuiSidebarGitHubState,
  GpuiTrustedGitReviewFileSelection,
  GpuiWorktreeMetadata,
} from "./types-and-protocol";
import { openAppModal, postAppModalHostMessage } from "@/packages/core-ui/app-modal-host-bridge";
import type { AppToastLevel } from "@/packages/shared/app-toast-contract";
import { createAppToastRequest } from "@/packages/shared/app-toast-contract";
import type {
  GxserverCheckoutProjectNewBranchResult,
  GxserverCreatePullRequestResult,
  GxserverGenerateCommitMessageResult,
  GxserverMergeWorktreeIntoMainResult,
  GxserverPresentationProject,
  GxserverProjectDomainState,
  GxserverTypedOperationResult,
} from "@/packages/shared/gxserver-protocol";
import type { SidebarProjectDiffStats } from "@/packages/shared/project-diff-stats";
import {
  createDefaultSidebarProjectDiffStats,
  parseGitNumstatDiffStats,
  parseGitZeroDelimitedPaths,
  resolveSidebarProjectDiffStats,
} from "@/packages/shared/project-diff-stats";
import type {
  SidebarPromptGitCommitMessage,
  SidebarSessionGroup,
  SidebarToExtensionMessage,
} from "@/packages/shared/session-grid-contract";
import type { SidebarAgentButton } from "@/packages/shared/sidebar-agents";
import type {
  SidebarGitAction,
  SidebarGitFileDiffDraft,
  SidebarGitState,
} from "@/packages/shared/sidebar-git";
import {
  createDefaultSidebarGitState,
  hasSidebarGitRemoteCommitDelta,
  normalizeSidebarGitAction,
} from "@/packages/shared/sidebar-git";

/*
CDXC:GxserverRuntimeSplit 2026-08-22:
The method signatures below are copied verbatim from the original class body.
They exist as a standalone interface — rather than being derived from
`typeof gpuiSidebarRuntimeGitMethods` — because deriving them would make
`GpuiSidebarRuntime` depend on the bodies that depend on it, which TypeScript
reports as a circular base type. `gpuiSidebarRuntimeGitMethodsShapeCheck`
at the bottom of this file is what keeps the two in step.
*/
export interface GpuiSidebarRuntimeGitMethods {
  startGitPollingDriver(): void;
  scheduleGitPollingCycle(): void;
  getVisibleProjectDiffStatsRefreshTargets(): GpuiProjectDiffStatsRefreshTarget[];
  refreshProjectDiffStatsTarget(target: GpuiProjectDiffStatsRefreshTarget): void;
  refreshProjectDiffStats(project: GxserverProjectDomainState): Promise<void>;
  refreshRemoteProjectDiffStats(reference: GpuiRemoteProjectReference): Promise<void>;
  countUntrackedProjectLines(project: GxserverProjectDomainState, paths: readonly string[]): Promise<number>;
  countRemoteUntrackedProjectLines(reference: GpuiRemoteProjectReference, paths: readonly string[]): Promise<number>;
  setProjectDiffStats(projectId: string, stats: SidebarProjectDiffStats): void;
  publishProjectDiffStatsPatch(projectId: string): void;
  postTitlebarGitMenuState(attempt?: number): void;
  handleGpuiTitlebarGitAction(payload: unknown): void;
  handleGpuiGitCommitModalCommand(payload: unknown): Promise<void>;
  refreshTitlebarGitMenuState(): void;
  refreshGitStateForActiveProjectIfNeeded(): void;
  applyLiveGitStateOverlays(project: GxserverProjectDomainState, state: SidebarGitState): SidebarGitState;
  refreshGitState(options?: {
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
  }): Promise<SidebarGitState>;
  refreshGitStateForMessage(message: Extract<SidebarToExtensionMessage, { type: "refreshGitState" }>): Promise<void>;
  readSidebarGitState(project: GxserverProjectDomainState, options?: { deferGitHub?: boolean; publishBusy?: boolean; toastOnFailure?: boolean }): Promise<SidebarGitState>;
  memoizeSidebarGitState(project: GxserverProjectDomainState, state: SidebarGitState): SidebarGitState;
  readGitHubState(project: GxserverProjectDomainState): Promise<GpuiSidebarGitHubState>;
  memoizedGitHubState(project: GxserverProjectDomainState): GpuiSidebarGitHubState;
  scheduleDeferredGitHubProbeIfStale(project: GxserverProjectDomainState): void;
  runDeferredGitHubProbe(project: GxserverProjectDomainState): Promise<void>;
  readRemoteSidebarGitState(remoteScope: GpuiRemoteProjectScope): Promise<SidebarGitState>;
  runRemoteSidebarGitAction(message: Extract<SidebarToExtensionMessage, { type: "runSidebarGitAction" }>, remoteScope: GpuiRemoteProjectScope): Promise<void>;
  promptRemoteSidebarGitActionReview(remoteScope: GpuiRemoteProjectScope, gitState: SidebarGitState, action: Extract<SidebarGitAction, "commit" | "pr" | "push">): void;
  runSidebarGitAction(message: Extract<SidebarToExtensionMessage, { type: "runSidebarGitAction" }>): Promise<void>;
  confirmSidebarGitCommit(message: Extract<SidebarToExtensionMessage, { type: "confirmSidebarGitCommit" }>): Promise<void>;
  confirmRemoteSidebarGitCommit(pending: GpuiPendingGitCommitRequest & { remoteReference: GpuiRemoteProjectReference }, message: Extract<SidebarToExtensionMessage, { type: "confirmSidebarGitCommit" }>): Promise<void>;
  confirmSidebarGitDirectMerge(message: Extract<SidebarToExtensionMessage, { type: "confirmSidebarGitDirectMerge" }>): Promise<void>;
  confirmRemoteSidebarGitDirectMerge(pending: GpuiPendingGitCommitRequest & { remoteReference: GpuiRemoteProjectReference }, message: Extract<SidebarToExtensionMessage, { type: "confirmSidebarGitDirectMerge" }>): Promise<void>;
  mergeRemoteWorktreeIntoMain(remoteScope: GpuiRemoteProjectScope): Promise<GxserverMergeWorktreeIntoMainResult>;
  mergeWorktreeIntoMain(input: {
    branch?: string | null;
    conflictAgent: SidebarAgentButton;
    deleteWorktreeAfter: boolean;
    worktreeProject: GxserverProjectDomainState;
  }): Promise<"conflicts" | "merged">;
  launchMergeConflictAgent(input: {
    agent: SidebarAgentButton;
    branch: string;
    mergeOutput: string;
    parentProject: GxserverProjectDomainState;
    worktree: GpuiWorktreeMetadata;
    worktreeProject: GxserverProjectDomainState;
  }): Promise<void>;
  runSidebarGitMultipleCommits(requestId: string, agentId?: string): Promise<void>;
  promptSidebarGitActionReview(project: GxserverProjectDomainState, gitState: SidebarGitState, action: Extract<SidebarGitAction, "commit" | "pr" | "push">): void;
  openSidebarGitCommitReviewModal(draft: SidebarPromptGitCommitMessage): void;
  openSidebarGitChangedFileDiff(filePath: string, requestId?: string): Promise<void>;
  openRemoteSidebarGitChangedFileDiff(remoteReference: GpuiRemoteProjectReference, filePath: string, requestId?: string): Promise<void>;
  openSidebarGitChangedFileInIde(message: Extract<SidebarToExtensionMessage, { type: "openSidebarGitChangedFile" }>): Promise<void>;
  postSidebarGitFileDiff(requestId: string, draft: SidebarGitFileDiffDraft): void;
  resolveTrustedGitReviewFileSelection(request: GpuiPendingGitCommitRequest, filePaths?: readonly string[]): GpuiTrustedGitReviewFileSelection;
  runGitMutation(project: GxserverProjectDomainState, startedTitle: string, finishedTitle: string, operation: () => Promise<void>): Promise<boolean>;
  commitWithMessage(project: GxserverProjectDomainState, message: string, filePaths?: readonly string[], options?: { agentId?: string; commitOnNewRef?: boolean }): Promise<void>;
  generateCommitMessage(project: GxserverProjectDomainState, filePaths: readonly string[] | undefined, agentId?: string): Promise<{ body: string; subject: string }>;
  generateRemoteCommitMessage(remoteScope: GpuiRemoteProjectScope, filePaths: readonly string[] | undefined, agentId?: string): Promise<{ body: string; subject: string }>;
  checkoutSidebarGitFeatureBranch(project: GxserverProjectDomainState, subject: string): Promise<string>;
  pushCurrentBranch(project: GxserverProjectDomainState, gitState: Pick<SidebarGitState, "branch" | "behindCount" | "hasOriginRemote" | "hasUpstream">): Promise<void>;
  syncCurrentBranchWithRemote(project: GxserverProjectDomainState, gitState: SidebarGitState): Promise<void>;
  commitRemoteWithMessage(remoteScope: GpuiRemoteProjectScope, message: string, filePaths?: readonly string[], options?: { agentId?: string; commitOnNewRef?: boolean }): Promise<void>;
  checkoutRemoteSidebarGitFeatureBranch(remoteScope: GpuiRemoteProjectScope, subject: string): Promise<void>;
  pushRemoteCurrentBranch(remoteScope: GpuiRemoteProjectScope, gitState: Pick<SidebarGitState, "branch" | "behindCount" | "hasOriginRemote" | "hasUpstream">): Promise<void>;
  syncRemoteCurrentBranchWithRemote(remoteScope: GpuiRemoteProjectScope, gitState: SidebarGitState): Promise<void>;
  shouldBypassRemoteMissingBeadsDatabasePreCommitHook(remoteScope: GpuiRemoteProjectScope): Promise<boolean>;
  shouldBypassMissingBeadsDatabasePreCommitHook(project: GxserverProjectDomainState): Promise<boolean>;
  runSidebarGitPromptAction(project: GxserverProjectDomainState, title: string, prompt: string, agentId?: string): Promise<void>;
  runRemoteSidebarGitPromptAction(remoteScope: GpuiRemoteProjectScope, title: string, prompt: string, agentId?: string): Promise<void>;
  runSidebarGitPullRequestAgentWorkflow(input: {
    agentId?: string;
    filePaths?: readonly string[];
    gitState: SidebarGitState;
    hasExplicitFileSelection: boolean;
    hasCommit: boolean;
    message: string;
    project: GxserverProjectDomainState;
  }): Promise<void>;
  runRemoteSidebarGitPullRequestAgentWorkflow(input: {
    agentId?: string;
    filePaths?: readonly string[];
    gitState: SidebarGitState;
    hasExplicitFileSelection: boolean;
    hasCommit: boolean;
    message: string;
    remoteScope: GpuiRemoteProjectScope;
  }): Promise<void>;
  persistGitPreferences(updates: Partial<GpuiGitPreferences>, scopeMessage?: {
      groupId?: string;
      projectId?: string;
    }): Promise<void>;
  resolveGitPreferenceRemoteScope(scopeMessage?: {
    groupId?: string;
    projectId?: string;
  }): GpuiRemoteProjectScope | undefined;
  isGitPreferenceRemoteScope(scopeMessage?: {
    groupId?: string;
    projectId?: string;
  }): boolean;
  resolveGitPreferenceLocalProject(scopeMessage?: {
    groupId?: string;
    projectId?: string;
  }): GxserverProjectDomainState | undefined;
  persistRemoteGitPreferences(remoteScope: GpuiRemoteProjectScope, updates: Partial<GpuiGitPreferences>): Promise<void>;
  resolveGitProjectForMessage(message: Extract<SidebarToExtensionMessage, { type: "runSidebarGitAction" }>): GxserverProjectDomainState | undefined;
  gitStateForHud(): SidebarGitState;
  gitPreferencesForProject(project: GxserverProjectDomainState | undefined): GpuiGitPreferences;
  gitPreferencesForPresentationProject(project: GxserverPresentationProject | undefined): GpuiGitPreferences;
  resolveDefaultPromptAgent(agentId?: string): SidebarAgentButton | undefined;
  resolveDefaultPromptAgentId(agentId?: string): string;
  runGitAction(project: GxserverProjectDomainState, params: Record<string, unknown>): Promise<GxserverTypedOperationResult>;
  runRemoteGitAction(remoteScope: GpuiRemoteProjectReference, params: Record<string, unknown>): Promise<GxserverTypedOperationResult>;
  runWorktreeAction(parentProject: GxserverProjectDomainState, params: Record<string, unknown>): Promise<GxserverTypedOperationResult>;
  runGitHubAction(project: GxserverProjectDomainState, params: Record<string, unknown>): Promise<GxserverTypedOperationResult>;
  runRemoteGitHubAction(remoteScope: GpuiRemoteProjectReference, params: Record<string, unknown>): Promise<GxserverTypedOperationResult>;
  createPullRequest(project: GxserverProjectDomainState): Promise<GxserverCreatePullRequestResult>;
  createRemotePullRequest(remoteScope: GpuiRemoteProjectReference): Promise<GpuiRemoteCreatePullRequestResult>;
  runBeadsAction(project: GxserverProjectDomainState, params: Record<string, unknown>): Promise<GxserverTypedOperationResult>;
  runRemoteBeadsAction(remoteScope: GpuiRemoteProjectReference, params: Record<string, unknown>): Promise<GxserverTypedOperationResult>;
  runRemoteGitMutation(remoteScope: GpuiRemoteProjectScope, startedTitle: string, finishedTitle: string, operation: () => Promise<void>): Promise<boolean>;
  postGitToast(level: AppToastLevel, title: string, options?: {
      description?: string;
      persistent?: boolean;
      toastId?: string;
    }): void;
}

export const gpuiSidebarRuntimeGitMethods = {

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
    CDXC:SidebarDiffStatsChurn 2026-08-16:
    The cycle stretches past the base interval once the sidebar renders more
    project rows than the interval can hold at the capped probe rate, so a
    sidebar with 100+ rows polls each row less often instead of probing many
    times per second.
    */
    const cycleLengthMs = Math.max(
      GPUI_PROJECT_DIFF_STATS_BACKGROUND_INTERVAL_MS,
      targets.length * GPUI_PROJECT_DIFF_STATS_MIN_PROBE_SPACING_MS,
    );
    const staggerStepMs = cycleLengthMs / Math.max(1, targets.length);
    targets.forEach((target, index) => {
      const timeoutId = window.setTimeout(
        () => {
          this.gitPollingTimeoutIds.delete(timeoutId);
          this.refreshProjectDiffStatsTarget(target);
        },
        Math.floor(index * staggerStepMs),
      );
      this.gitPollingTimeoutIds.add(timeoutId);
    });
    this.gitPollingCycleTimeoutId = window.setTimeout(() => {
      this.gitPollingCycleTimeoutId = undefined;
      this.scheduleGitPollingCycle();
    }, cycleLengthMs);
  },

  /*
  CDXC:SidebarDiffStatsChurn 2026-08-16:
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
      this.domainProjects.map((project) => [project.projectId, project]),
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
          kind: "remote",
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
        kind: "local",
        project,
      });
    }
    return [...targetsByKey.values()].sort((left, right) => left.key.localeCompare(right.key));
  },

  refreshProjectDiffStatsTarget(this: GpuiSidebarRuntime, target: GpuiProjectDiffStatsRefreshTarget): void {
    if (target.kind === "remote") {
      void this.refreshRemoteProjectDiffStats(target.reference);
      return;
    }
    void this.refreshProjectDiffStats(target.project);
    if (this.activeProjectId === target.project.projectId) {
      /*
      CDXC:SidebarGitMemo 2026-07-29:
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
    CDXC:SidebarDiffStatsChurn 2026-08-16:
    Background polls must be invisible unless the numbers actually change:
    the old pre-probe `isLoading: true` republish plus the post-probe
    republish meant every poll of every project rebuilt and re-sent the whole
    sidebar tree twice, even with nothing to report. `isLoading` only feeds a
    hover tooltip, so silent background probes simply publish the resolved
    stats (deduplicated inside setProjectDiffStats).
    */
    try {
      if (!this.gitRepoProjectIds.has(projectId)) {
        const repoCheck = await this.runGitAction(project, { action: "isInsideWorkTree" });
        if (repoCheck.exitCode !== 0 || repoCheck.stdout.trim() !== "true") {
          this.setProjectDiffStats(projectId, createDefaultSidebarProjectDiffStats(false));
          return;
        }
        this.gitRepoProjectIds.add(projectId);
      }
      const trackedDiff = await this.runGitAction(project, { action: "diffNumstat" });
      if (trackedDiff.exitCode !== 0) {
        this.gitRepoProjectIds.delete(projectId);
        return;
      }
      const trackedStats = parseGitNumstatDiffStats(trackedDiff.stdout);
      const hasTrackedLineChanges = trackedStats.additions > 0 || trackedStats.deletions > 0;
      const settings = createGpuiSidebarSettings(this.runtimeSettings);
      let resolvedStats = trackedStats;
      if (settings.showUntrackedProjectDiffWhenNoTrackedChanges && !hasTrackedLineChanges) {
        const untrackedFiles = await this.runGitAction(project, { action: "listUntracked" });
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

  async refreshRemoteProjectDiffStats(this: GpuiSidebarRuntime,
    reference: GpuiRemoteProjectReference,
  ): Promise<void> {
    const scopedProjectId = createGpuiRemotePresentationProjectId(
      reference.machineId,
      reference.projectId,
    );
    if (this.pendingProjectDiffRefreshProjectIds.has(scopedProjectId)) {
      return;
    }
    this.pendingProjectDiffRefreshProjectIds.add(scopedProjectId);
    // Silent background probe: publish only resolved, changed stats (see
    // refreshProjectDiffStats for the churn rationale).
    try {
      if (!this.gitRepoProjectIds.has(scopedProjectId)) {
        const repoCheck = await this.runRemoteGitAction(reference, {
          action: "isInsideWorkTree",
        });
        if (repoCheck.exitCode !== 0 || repoCheck.stdout.trim() !== "true") {
          this.setProjectDiffStats(scopedProjectId, createDefaultSidebarProjectDiffStats(false));
          return;
        }
        this.gitRepoProjectIds.add(scopedProjectId);
      }
      const trackedDiff = await this.runRemoteGitAction(reference, { action: "diffNumstat" });
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
          action: "listUntracked",
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

  async countUntrackedProjectLines(this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState,
    paths: readonly string[],
  ): Promise<number> {
    let lines = 0;
    for (const filePaths of chunkUntrackedLineCountPaths(paths)) {
      const result = await this.runGitAction(project, {
        action: "countFileLines",
        filePaths,
      });
      if (result.exitCode !== 0) {
        throw new Error("Could not count untracked file lines.");
      }
      lines += Number(result.stdout.trim()) || 0;
    }
    return lines;
  },

  async countRemoteUntrackedProjectLines(this: GpuiSidebarRuntime,
    reference: GpuiRemoteProjectReference,
    paths: readonly string[],
  ): Promise<number> {
    let lines = 0;
    for (const filePaths of chunkUntrackedLineCountPaths(paths)) {
      const result = await this.runRemoteGitAction(reference, {
        action: "countFileLines",
        filePaths,
      });
      if (result.exitCode !== 0) {
        throw new Error("Could not count remote untracked file lines.");
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
  CDXC:SidebarDiffStatsChurn 2026-08-16:
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
      type: "sidebarGroupsChanged",
    });
  },

  postTitlebarGitMenuState(this: GpuiSidebarRuntime, attempt = 0): void {
    if (this.titlebarGitMenuStateRetryId !== undefined) {
      window.clearTimeout(this.titlebarGitMenuStateRetryId);
      this.titlebarGitMenuStateRetryId = undefined;
    }
    const postTitlebarGitMenuState = window.ghostexGpui?.postTitlebarGitMenuState;
    if (typeof postTitlebarGitMenuState !== "function") {
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
    if (action === "refresh") {
      this.refreshTitlebarGitMenuState();
      return;
    }
    void this.runSidebarGitAction({
      ...(this.activeGroupId ? { groupId: this.activeGroupId } : {}),
      action,
      type: "runSidebarGitAction",
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
        type: "refreshGitState",
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
  applyLiveGitStateOverlays(this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState,
    state: SidebarGitState,
  ): SidebarGitState {
    const preferences = this.gitPreferencesForProject(project);
    const gitHubState = state.isRepo
      ? this.gitHubStateMemoByProjectId.peek(project.projectId)
      : undefined;
    return {
      ...state,
      ...(gitHubState ?? {}),
      confirmSuggestedCommit: preferences.confirmCommit,
      generateCommitBody: preferences.generateCommitBody,
      primaryAction: preferences.primaryAction,
    };
  },

  async refreshGitState(this: GpuiSidebarRuntime, {
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
  } = {}): Promise<SidebarGitState> {
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

  async refreshGitStateForMessage(this: GpuiSidebarRuntime,
    message: Extract<SidebarToExtensionMessage, { type: "refreshGitState" }>,
  ): Promise<void> {
    /*
    CDXC:GPUISidebarGit 2026-06-24-21:26:
    Reused Git controls can refresh from a scoped local or remote project row. Resolve that owner before reading Git state; unscoped callers keep the active-project behavior, but scoped remote rows must never refresh the active local project by accident.
    */
    const explicitScope = Boolean(message.groupId?.trim() || message.projectId?.trim());
    const remoteScope = this.resolveGitPreferenceRemoteScope(message);
    if (remoteScope) {
      const activeRemoteGroupId = createGpuiRemotePresentationGroupId(
        remoteScope.machineId,
        remoteScope.projectId,
      );
      if (this.activeGroupId === activeRemoteGroupId) {
        const preferences = this.gitPreferencesForPresentationProject(
          this.findRemotePresentationProject(remoteScope) ?? remoteScope.project,
        );
        this.gitState = {
          ...createDefaultSidebarGitState(
            preferences.primaryAction,
            preferences.confirmCommit,
            preferences.generateCommitBody,
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
      this.postRemoteToast("warning", "Remote Git unavailable", {
        description: "Reconnect the remote machine before refreshing Git state.",
      });
      return;
    }
    const project =
      this.resolveGitPreferenceLocalProject(message) ??
      (explicitScope ? undefined : this.activeDomainProject());
    if (!project) {
      this.postGitToast("warning", "Git unavailable", {
        description: "No active gxserver project is available.",
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

  async readSidebarGitState(this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState,
    options: { deferGitHub?: boolean; publishBusy?: boolean; toastOnFailure?: boolean } = {},
  ): Promise<SidebarGitState> {
    const baseState = createDefaultSidebarGitState(
      this.gitPreferencesForProject(project).primaryAction,
      this.gitPreferencesForProject(project).confirmCommit,
      this.gitPreferencesForProject(project).generateCommitBody,
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
      const repoCheck = await this.runGitAction(project, { action: "isInsideWorkTree" });
      if (repoCheck.exitCode !== 0 || repoCheck.stdout.trim() !== "true") {
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
      const [branch, status, diff, untrackedFiles, upstream, remotes, originRemote, gitHubState] =
        await Promise.all([
          this.runGitAction(project, { action: "branch" }),
          this.runGitAction(project, { action: "statusPorcelain" }),
          this.runGitAction(project, { action: "diffNumstat" }),
          this.runGitAction(project, { action: "listUntracked" }),
          this.runGitAction(project, { action: "upstreamCounts" }),
          this.runGitAction(project, { action: "listRemotes" }),
          this.runGitAction(project, { action: "getOriginRemoteUrl" }),
          options.deferGitHub === true
            ? this.memoizedGitHubState(project)
            : this.readGitHubState(project),
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
        hasGitHubRemote:
          originRemote.exitCode === 0 &&
          normalizeGpuiGitHubRemoteUrl(originRemote.stdout) !== undefined,
        hasOriginRemote: remotes.stdout.split(/\s+/).includes("origin"),
        hasUpstream: upstream.exitCode === 0,
        hasWorkingTreeChanges: status.stdout.trim().length > 0,
        isBusy: false,
        isRepo: true,
        files,
        isWorktree: normalizeGpuiWorktreeParentProjectId(project.worktree) !== undefined,
        pr: gitHubState.pr,
        worktreeName: stringFromRecord(project.worktree, "name"),
      });
    } catch {
      if (options.toastOnFailure) {
        this.postGitToast("error", "Could not refresh Git state", {
          description: "gxserver could not inspect the selected project.",
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
  memoizeSidebarGitState(this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState,
    state: SidebarGitState,
  ): SidebarGitState {
    this.gitStateMemoByProjectId.set(project.projectId, state, Date.now());
    return state;
  },

  /** Run the GitHub CLI probes and memoize the pair under the longer lease. */
  async readGitHubState(this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState,
  ): Promise<GpuiSidebarGitHubState> {
    const [ghVersion, pr] = await Promise.all([
      this.runGitHubAction(project, { action: "version" }),
      this.runGitHubAction(project, { action: "prView" }),
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
    return (
      this.gitHubStateMemoByProjectId.peek(project.projectId) ?? { hasGitHubCli: false, pr: null }
    );
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

  async readRemoteSidebarGitState(this: GpuiSidebarRuntime,
    remoteScope: GpuiRemoteProjectScope,
  ): Promise<SidebarGitState> {
    const remotePreferences = this.gitPreferencesForPresentationProject(
      this.findRemotePresentationProject(remoteScope) ?? remoteScope.project,
    );
    const baseState = createDefaultSidebarGitState(
      remotePreferences.primaryAction,
      remotePreferences.confirmCommit,
      remotePreferences.generateCommitBody,
    );
    try {
      const repoCheck = await this.runRemoteGitAction(remoteScope, { action: "isInsideWorkTree" });
      if (repoCheck.exitCode !== 0 || repoCheck.stdout.trim() !== "true") {
        return { ...baseState, hasCheckedGitHubRemote: true, isRepo: false };
      }

      const [branch, status, diff, untrackedFiles, upstream, remotes, originRemote, ghVersion, pr] =
        await Promise.all([
          this.runRemoteGitAction(remoteScope, { action: "branch" }),
          this.runRemoteGitAction(remoteScope, { action: "statusPorcelain" }),
          this.runRemoteGitAction(remoteScope, { action: "diffNumstat" }),
          this.runRemoteGitAction(remoteScope, { action: "listUntracked" }),
          this.runRemoteGitAction(remoteScope, { action: "upstreamCounts" }),
          this.runRemoteGitAction(remoteScope, { action: "listRemotes" }),
          this.runRemoteGitAction(remoteScope, { action: "getOriginRemoteUrl" }),
          this.runRemoteGitHubAction(remoteScope, { action: "version" }),
          this.runRemoteGitHubAction(remoteScope, { action: "prView" }),
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
      const presentationProject =
        this.findRemotePresentationProject(remoteScope) ?? remoteScope.project;
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
        hasGitHubRemote:
          originRemote.exitCode === 0 &&
          normalizeGpuiGitHubRemoteUrl(originRemote.stdout) !== undefined,
        hasOriginRemote: remotes.stdout.split(/\s+/).includes("origin"),
        hasUpstream: upstream.exitCode === 0,
        hasWorkingTreeChanges: status.stdout.trim().length > 0,
        isBusy: false,
        isRepo: true,
        isWorktree:
          normalizeGpuiWorktreeParentProjectId(presentationProject.worktree) !== undefined,
        pr: parseGpuiGitHubPullRequest(pr.stdout, pr.exitCode === 0),
        worktreeName:
          stringFromRecord(presentationProject.worktree, "name") ?? presentationProject.title,
      };
    } catch {
      this.postRemoteToast("warning", "Remote Git unavailable", {
        description: "The remote gxserver could not inspect the selected project.",
      });
      return { ...baseState, hasCheckedGitHubRemote: true, isBusy: false, isRepo: false };
    }
  },

  async runRemoteSidebarGitAction(this: GpuiSidebarRuntime,
    message: Extract<SidebarToExtensionMessage, { type: "runSidebarGitAction" }>,
    remoteScope: GpuiRemoteProjectScope,
  ): Promise<void> {
    if (message.action === "multiRelease") {
      await this.runRemoteSidebarGitPromptAction(
        remoteScope,
        "Multicommit & Release",
        GPUI_GIT_MULTICOMMIT_RELEASE_PROMPT,
      );
      return;
    }
    if (message.action === "release") {
      await this.runRemoteSidebarGitPromptAction(
        remoteScope,
        "Release",
        GPUI_GIT_RELEASE_ONLY_PROMPT,
      );
      return;
    }

    const gitState = await this.readRemoteSidebarGitState(remoteScope);
    if (!gitState.isRepo) {
      this.postRemoteToast("warning", "Remote Git unavailable", {
        description: "Open a Git repository on the remote machine to use Git actions.",
      });
      return;
    }

    if (message.action === "syncMain") {
      if (!normalizeGpuiWorktreeParentProjectId(remoteScope.project.worktree)) {
        this.postRemoteToast("warning", "Remote worktree unavailable", {
          description: "Open a remote worktree project to sync with main.",
        });
        return;
      }
      await this.runRemoteSidebarGitPromptAction(
        remoteScope,
        "Sync with Main",
        buildGpuiGitSyncWithMainPrompt(),
      );
      return;
    }

    if (message.action === "syncRemote") {
      if (!hasSidebarGitRemoteCommitDelta(gitState)) {
        this.postRemoteToast("info", "Remote already synced");
        return;
      }
      await this.runRemoteGitMutation(
        remoteScope,
        "Syncing remote",
        "Remote sync complete",
        async () => {
          await this.syncRemoteCurrentBranchWithRemote(remoteScope, gitState);
        },
      );
      return;
    }

    if (
      normalizeGpuiWorktreeMetadata(remoteScope.project.worktree) &&
      (message.action === "commit" || message.action === "push" || message.action === "pr")
    ) {
      this.promptRemoteSidebarGitActionReview(remoteScope, gitState, message.action);
      return;
    }

    if (message.action === "pr") {
      if (gitState.pr?.state === "open") {
        this.postRemoteProjectNativeAction(
          "openRemoteExistingPullRequestInBrowser",
          remoteScope,
          message,
        );
        return;
      }
      if (!gitState.hasGitHubCli) {
        this.postRemoteToast("warning", "Remote GitHub CLI unavailable", {
          description: "Install GitHub CLI on the remote machine before creating a pull request.",
        });
        return;
      }
      if (gitState.hasWorkingTreeChanges) {
        this.promptRemoteSidebarGitActionReview(remoteScope, gitState, "pr");
        return;
      }
      await this.runRemoteSidebarGitPullRequestAgentWorkflow({
        gitState,
        hasCommit: false,
        hasExplicitFileSelection: false,
        message: "",
        remoteScope,
      });
      return;
    }

    if (message.action === "commit") {
      if (!gitState.hasWorkingTreeChanges) {
        this.postRemoteToast("info", "No remote changes to commit");
        return;
      }
      this.promptRemoteSidebarGitActionReview(remoteScope, gitState, "commit");
      return;
    }

    if (message.action === "push") {
      if (gitState.hasWorkingTreeChanges) {
        this.promptRemoteSidebarGitActionReview(remoteScope, gitState, "push");
        return;
      }
      await this.runRemoteGitMutation(remoteScope, "Pushing", "Remote push complete", async () => {
        await this.pushRemoteCurrentBranch(remoteScope, gitState);
      });
    }
  },

  promptRemoteSidebarGitActionReview(this: GpuiSidebarRuntime,
    remoteScope: GpuiRemoteProjectScope,
    gitState: SidebarGitState,
    action: Extract<SidebarGitAction, "commit" | "pr" | "push">,
  ): void {
    const requestId = `gpui-remote-git-action-${Date.now().toString(36)}`;
    const hasCommit = gitState.hasWorkingTreeChanges;
    this.pendingGitCommitRequests.set(requestId, {
      action,
      files: [...gitState.files],
      hasCommit,
      projectId: createGpuiRemotePresentationProjectId(
        remoteScope.machineId,
        remoteScope.projectId,
      ),
      remoteReference: {
        machineId: remoteScope.machineId,
        projectId: remoteScope.projectId,
      },
      remoteTitle: remoteScope.project.title || remoteScope.machineName || "Remote project",
      subject: "",
    });
    const modalDraft: SidebarPromptGitCommitMessage = {
      action,
      agentId: this.resolveDefaultPromptAgentId(),
      branch: gitState.branch,
      changedFiles: gitState.files,
      confirmLabel: resolveGpuiSidebarGitConfirmLabel(action, hasCommit),
      deleteWorktreeAfterDefault: false,
      description: hasCommit
        ? "Review and confirm your remote commit. Leave the message blank to auto-generate one."
        : resolveGpuiSidebarGitPromptDescription(action),
      isDefaultRef: gitState.branch === "main" || gitState.branch === "master",
      isWorktree: normalizeGpuiWorktreeMetadata(remoteScope.project.worktree) !== undefined,
      requestId,
      showCommitMessage: hasCommit,
      suggestedBody: undefined,
      suggestedSubject: "",
      type: "promptGitCommit",
      worktreeName:
        stringFromRecord(remoteScope.project.worktree, "name") ?? remoteScope.project.title,
    };
    this.openSidebarGitCommitReviewModal(modalDraft);
  },

  async runSidebarGitAction(this: GpuiSidebarRuntime,
    message: Extract<SidebarToExtensionMessage, { type: "runSidebarGitAction" }>,
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
        this.postRemoteToast("warning", "Remote Git unavailable", {
          description: "Reconnect the remote machine before using Git actions.",
        });
        return;
      }
      await this.runRemoteSidebarGitAction(message, remoteScope);
      return;
    }
    const project = this.resolveGitProjectForMessage(message);
    if (!project) {
      this.postGitToast("warning", "Git unavailable", {
        description: "No active gxserver project is available.",
      });
      return;
    }

    if (message.action === "multiRelease") {
      await this.runSidebarGitPromptAction(
        project,
        "Multicommit & Release",
        GPUI_GIT_MULTICOMMIT_RELEASE_PROMPT,
      );
      return;
    }
    if (message.action === "release") {
      await this.runSidebarGitPromptAction(project, "Release", GPUI_GIT_RELEASE_ONLY_PROMPT);
      return;
    }

    const gitState = await this.refreshGitState({
      force: true,
      project,
      publishBusy: true,
      toastOnFailure: true,
    });
    if (!gitState.isRepo) {
      this.postGitToast("warning", "Git unavailable", {
        description: "Open a Git repository to use Git actions.",
      });
      return;
    }

    if (message.action === "syncMain") {
      if (!normalizeGpuiWorktreeParentProjectId(project.worktree)) {
        this.postGitToast("warning", "Worktree unavailable", {
          description: "Open a worktree project to sync with main.",
        });
        return;
      }
      await this.runSidebarGitPromptAction(
        project,
        "Sync with Main",
        buildGpuiGitSyncWithMainPrompt(),
      );
      return;
    }

    if (message.action === "syncRemote") {
      if (!hasSidebarGitRemoteCommitDelta(gitState)) {
        this.postGitToast("info", "Remote already synced");
        return;
      }
      await this.runGitMutation(project, "Syncing remote", "Remote sync complete", async () => {
        await this.syncCurrentBranchWithRemote(project, gitState);
      });
      return;
    }

    if (
      normalizeGpuiWorktreeMetadata(project.worktree) &&
      (message.action === "commit" || message.action === "push" || message.action === "pr")
    ) {
      this.promptSidebarGitActionReview(project, gitState, message.action);
      return;
    }

    if (message.action === "pr") {
      if (gitState.pr?.state === "open") {
        this.postNativeProjectPathAction(
          "openExistingPullRequestInBrowser",
          project.projectId,
          message,
        );
        return;
      }
      if (!gitState.hasGitHubCli) {
        this.postGitToast("warning", "GitHub CLI unavailable", {
          description: "Install GitHub CLI before creating a pull request.",
        });
        return;
      }
      if (gitState.hasWorkingTreeChanges) {
        this.promptSidebarGitActionReview(project, gitState, "pr");
        return;
      }
      await this.runSidebarGitPullRequestAgentWorkflow({
        gitState,
        hasCommit: false,
        hasExplicitFileSelection: false,
        message: "",
        project,
      });
      return;
    }

    if (message.action === "commit") {
      if (!gitState.hasWorkingTreeChanges) {
        this.postGitToast("info", "No changes to commit");
        return;
      }
      this.promptSidebarGitActionReview(project, gitState, "commit");
      return;
    }

    if (message.action === "push") {
      if (gitState.hasWorkingTreeChanges) {
        this.promptSidebarGitActionReview(project, gitState, "push");
        return;
      }
      await this.runGitMutation(project, "Pushing", "Push complete", async () => {
        await this.pushCurrentBranch(project, gitState);
      });
    }
  },

  async confirmSidebarGitCommit(this: GpuiSidebarRuntime,
    message: Extract<SidebarToExtensionMessage, { type: "confirmSidebarGitCommit" }>,
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
      this.postGitToast("error", "Git action unavailable", {
        description: "The selected gxserver project is no longer available.",
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
      this.postGitToast("warning", "Git unavailable", {
        description: "Open a Git repository to use Git actions.",
      });
      return;
    }
    if (pending.action === "pr") {
      let trustedFileSelection: GpuiTrustedGitReviewFileSelection | undefined;
      if (pending.hasCommit) {
        try {
          trustedFileSelection = this.resolveTrustedGitReviewFileSelection(
            pending,
            message.filePaths,
          );
        } catch {
          this.postGitToast("warning", "Invalid file selection", {
            description: "Choose files from the current Git review before creating a pull request.",
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
        resolveGpuiSidebarGitStartedTitle("pr", pending.hasCommit),
        resolveGpuiSidebarGitFinishedTitle("pr"),
        async () => {
          if (pending.hasCommit) {
            await this.commitWithMessage(
              project,
              message.message,
              trustedFileSelection?.filePaths,
              {
                agentId: message.agentId,
                commitOnNewRef: message.commitOnNewRef === true,
              },
            );
          }
          const nextGitState = await this.refreshGitState({ force: true, project });
          await this.pushCurrentBranch(project, nextGitState);
          const result = await this.createPullRequest(project);
          if (!isGpuiConfirmedOpenPullRequest(result)) {
            throw new GpuiUserVisibleGitError(
              "GitHub CLI could not create or find an open pull request.",
            );
          }
          confirmedPullRequest = true;
          this.postNativeProjectPathAction(
            "openExistingPullRequestInBrowser",
            project.projectId,
            message,
          );
        },
      );
      if (completed && confirmedPullRequest) {
        await this.deleteWorktreeAfterCompletedGitAction(project);
      }
      if (completed && !confirmedPullRequest) {
        this.postGitToast("warning", "Worktree cleanup skipped", {
          description: "Pull request creation was not confirmed.",
        });
      }
      if (!completed) {
        this.postGitToast("warning", "Worktree cleanup skipped", {
          description: "Pull request creation did not complete.",
        });
      }
      return;
    }
    let trustedFileSelection: GpuiTrustedGitReviewFileSelection | undefined;
    if (pending.hasCommit) {
      try {
        trustedFileSelection = this.resolveTrustedGitReviewFileSelection(
          pending,
          message.filePaths,
        );
      } catch {
        this.postGitToast("warning", "Invalid file selection", {
          description: "Choose files from the current Git review before committing.",
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
        if (pending.action === "push") {
          const nextState = await this.refreshGitState({ force: true, project });
          await this.pushCurrentBranch(project, nextState);
        }
      },
    );
    if (completed && message.deleteWorktreeAfter === true) {
      await this.deleteWorktreeAfterCompletedGitAction(project);
    }
  },

  async confirmRemoteSidebarGitCommit(this: GpuiSidebarRuntime,
    pending: GpuiPendingGitCommitRequest & { remoteReference: GpuiRemoteProjectReference },
    message: Extract<SidebarToExtensionMessage, { type: "confirmSidebarGitCommit" }>,
  ): Promise<void> {
    const remoteScope = this.resolveRemotePresentationProjectScope(pending.remoteReference);
    if (!remoteScope) {
      this.postRemoteToast("warning", "Remote Git unavailable", {
        description: "Reconnect the remote machine before confirming this Git action.",
      });
      return;
    }
    const gitState = await this.readRemoteSidebarGitState(remoteScope);
    if (!gitState.isRepo) {
      this.postRemoteToast("warning", "Remote Git unavailable", {
        description: "Open a Git repository on the remote machine to use Git actions.",
      });
      return;
    }
    if (pending.action === "pr") {
      let trustedFileSelection: GpuiTrustedGitReviewFileSelection | undefined;
      if (pending.hasCommit) {
        try {
          trustedFileSelection = this.resolveTrustedGitReviewFileSelection(
            pending,
            message.filePaths,
          );
        } catch {
          this.postRemoteToast("warning", "Invalid file selection", {
            description:
              "Choose files from the current remote Git review before creating a pull request.",
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
        resolveGpuiSidebarGitStartedTitle("pr", pending.hasCommit),
        resolveGpuiSidebarGitFinishedTitle("pr"),
        async () => {
          if (pending.hasCommit) {
            await this.commitRemoteWithMessage(
              remoteScope,
              message.message,
              trustedFileSelection?.filePaths,
              {
                agentId: message.agentId,
                commitOnNewRef: message.commitOnNewRef === true,
              },
            );
          }
          const nextGitState = await this.readRemoteSidebarGitState(remoteScope);
          await this.pushRemoteCurrentBranch(remoteScope, nextGitState);
          const result = await this.createRemotePullRequest(remoteScope);
          if (!isGpuiConfirmedOpenRemotePullRequest(result)) {
            throw new GpuiUserVisibleGitError(
              "GitHub CLI could not create or find an open remote pull request.",
            );
          }
          confirmedPullRequest = true;
          this.postRemoteProjectNativeAction(
            "openRemoteExistingPullRequestInBrowser",
            remoteScope,
            message,
          );
        },
      );
      if (completed && confirmedPullRequest) {
        await this.deleteRemoteWorktreeAfterCompletedGitAction(remoteScope);
      }
      if (completed && !confirmedPullRequest) {
        this.postRemoteToast("warning", "Remote worktree cleanup skipped", {
          description: "Pull request creation was not confirmed.",
        });
      }
      if (!completed) {
        this.postRemoteToast("warning", "Remote worktree cleanup skipped", {
          description: "Pull request creation did not complete.",
        });
      }
      return;
    }

    let trustedFileSelection: GpuiTrustedGitReviewFileSelection | undefined;
    if (pending.hasCommit) {
      try {
        trustedFileSelection = this.resolveTrustedGitReviewFileSelection(
          pending,
          message.filePaths,
        );
      } catch {
        this.postRemoteToast("warning", "Invalid file selection", {
          description: "Choose files from the current remote Git review before committing.",
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
          await this.commitRemoteWithMessage(
            remoteScope,
            message.message,
            trustedFileSelection?.filePaths,
            {
              agentId: message.agentId,
              commitOnNewRef: message.commitOnNewRef === true,
            },
          );
        }
        if (pending.action === "push") {
          const nextState = await this.readRemoteSidebarGitState(remoteScope);
          await this.pushRemoteCurrentBranch(remoteScope, nextState);
        }
      },
    );
    if (completed && message.deleteWorktreeAfter === true) {
      await this.deleteRemoteWorktreeAfterCompletedGitAction(remoteScope);
    }
  },

  async confirmSidebarGitDirectMerge(this: GpuiSidebarRuntime,
    message: Extract<SidebarToExtensionMessage, { type: "confirmSidebarGitDirectMerge" }>,
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
      this.postGitToast("error", "Direct merge unavailable", {
        description: "The selected gxserver project is no longer available.",
      });
      this.publishHudPatch();
      return;
    }
    const worktree = normalizeGpuiWorktreeMetadata(project.worktree);
    if (!worktree) {
      this.postGitToast("warning", "Worktree unavailable", {
        description: "Direct merge is only available from a gxserver worktree project.",
      });
      this.publishHudPatch();
      return;
    }
    const conflictAgent = this.resolveDefaultPromptAgent(message.agentId);
    if (!conflictAgent?.command?.trim()) {
      this.postGitToast("error", "Agent unavailable", {
        description: "Choose a configured prompt agent before merging.",
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
      this.postGitToast("warning", "Git unavailable", {
        description: "Open a Git repository before merging this worktree.",
      });
      return;
    }

    let trustedFileSelection: GpuiTrustedGitReviewFileSelection | undefined;
    if (pending.hasCommit) {
      try {
        trustedFileSelection = this.resolveTrustedGitReviewFileSelection(
          pending,
          message.filePaths,
        );
      } catch {
        this.postGitToast("warning", "Invalid file selection", {
          description: "Choose files from the current Git review before merging.",
        });
        this.gitState = { ...this.gitStateForHud(), isBusy: false };
        this.publishHudPatch();
        return;
      }
    }

    const toastId = createGpuiGitToastId();
    this.postGitToast("info", "Merging worktree into main", {
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
      if (result === "conflicts") {
        this.postGitToast("warning", "Merge conflicts need resolution", { toastId });
        return;
      }
      await this.refreshDomainPresentationFromClient("patch").catch(() => undefined);
      this.postGitToast("success", "Worktree merged to main", { toastId });
    } catch (error) {
      this.gitState = { ...this.gitStateForHud(), isBusy: false };
      this.publishHudPatch();
      this.postGitToast("error", "Direct merge failed", {
        description: gpuiUserVisibleGitErrorMessage(
          error,
          "gxserver could not merge the selected worktree.",
        ),
        toastId,
      });
    }
  },

  async confirmRemoteSidebarGitDirectMerge(this: GpuiSidebarRuntime,
    pending: GpuiPendingGitCommitRequest & { remoteReference: GpuiRemoteProjectReference },
    message: Extract<SidebarToExtensionMessage, { type: "confirmSidebarGitDirectMerge" }>,
  ): Promise<void> {
    const remoteScope = this.resolveRemotePresentationProjectScope(pending.remoteReference);
    if (!remoteScope) {
      this.postRemoteToast("warning", "Remote merge unavailable", {
        description: "Reconnect the remote machine before merging this worktree.",
      });
      return;
    }
    if (!normalizeGpuiWorktreeMetadata(remoteScope.project.worktree)) {
      this.postRemoteToast("warning", "Remote worktree unavailable", {
        description: "Direct merge is only available from a remote worktree project.",
      });
      return;
    }
    let trustedFileSelection: GpuiTrustedGitReviewFileSelection | undefined;
    if (pending.hasCommit) {
      try {
        trustedFileSelection = this.resolveTrustedGitReviewFileSelection(
          pending,
          message.filePaths,
        );
      } catch {
        this.postRemoteToast("warning", "Invalid file selection", {
          description: "Choose files from the current remote Git review before merging.",
        });
        return;
      }
    }
    const toastId = createGpuiGitToastId();
    this.postGitToast("info", "Merging remote worktree", {
      persistent: true,
      toastId,
    });
    /*
    CDXC:RemoteGitBranching 2026-06-24-18:55:
    Remote direct merge and commit-on-new-branch must go through id-scoped gxserver operations so the daemon derives main, parent, and branch targets. GPUI may refresh presentation and create a conflict-resolution agent session, but it must not attach terminals, focus remote panes, open native apps, or expose branch/path/command details in status text.
    */
    try {
      if (pending.hasCommit) {
        await this.commitRemoteWithMessage(
          remoteScope,
          message.message,
          trustedFileSelection?.filePaths,
          {
            agentId: message.agentId,
          },
        );
      }
      const result = await this.mergeRemoteWorktreeIntoMain(remoteScope);
      await this.refreshRemotePresentationFromGxserver(remoteScope.machineId).catch(
        () => undefined,
      );
      if (result.status === "conflicts") {
        this.postGitToast("warning", "Remote merge conflicts need resolution", { toastId });
        const conflictAgentId = this.resolveDefaultPromptAgentId(message.agentId);
        if (conflictAgentId && result.parentProjectId) {
          await this.createRemoteAgentSessionForProject(
            { machineId: remoteScope.machineId, projectId: result.parentProjectId },
            conflictAgentId,
            GPUI_REMOTE_MERGE_CONFLICT_PROMPT,
            formatGpuiGitAgentWorkflowTitle("Merge Conflicts"),
          ).catch(() => undefined);
        }
        return;
      }
      this.postGitToast("success", "Remote worktree merged", { toastId });
      if (message.deleteWorktreeAfter === true) {
        await this.deleteRemoteWorktreeAfterCompletedGitAction(remoteScope);
      }
    } catch (error) {
      this.postGitToast("error", "Remote direct merge failed", {
        description: gpuiUserVisibleGitErrorMessage(
          error,
          "Remote gxserver could not merge the selected worktree.",
        ),
        toastId,
      });
    }
  },

  async mergeRemoteWorktreeIntoMain(this: GpuiSidebarRuntime,
    remoteScope: GpuiRemoteProjectScope,
  ): Promise<GxserverMergeWorktreeIntoMainResult> {
    return this.requestRemoteGxserver<GxserverMergeWorktreeIntoMainResult>(
      remoteScope.machineId,
      "/api/mergeWorktreeIntoMain",
      { projectId: remoteScope.projectId },
      { timeoutMs: 60_000 },
    );
  },

  async mergeWorktreeIntoMain(this: GpuiSidebarRuntime, input: {
    branch?: string | null;
    conflictAgent: SidebarAgentButton;
    deleteWorktreeAfter: boolean;
    worktreeProject: GxserverProjectDomainState;
  }): Promise<"conflicts" | "merged"> {
    const worktree = normalizeGpuiWorktreeMetadata(input.worktreeProject.worktree);
    if (!worktree) {
      throw new Error("Direct merge requires a worktree project.");
    }
    const branch = input.branch?.trim() || worktree.branch;
    if (!branch) {
      throw new Error("Create and checkout a branch before merging.");
    }
    const parentProject = this.domainProjectById(worktree.parentProjectId);
    if (
      !parentProject ||
      parentProject.projectId === input.worktreeProject.projectId ||
      parentProject.isRecentProject === true ||
      !normalizeGpuiProjectPath(parentProject.path)
    ) {
      throw new Error("The gxserver worktree parent project is unavailable.");
    }

    const mainCheck = await this.runGitAction(parentProject, {
      action: "verifyRef",
      ref: "main",
    });
    if (mainCheck.exitCode !== 0) {
      throw new Error('The parent project does not have a local "main" branch.');
    }
    const parentStatus = await this.runGitAction(parentProject, { action: "status" });
    if (parentStatus.exitCode !== 0) {
      throw new Error("Could not read parent project status.");
    }
    if (hasGpuiGxserverShortStatusChanges(parentStatus.stdout)) {
      throw new Error("Commit or stash changes in the main project before merging this worktree.");
    }

    const checkoutResult = await this.runGitAction(parentProject, {
      action: "checkout",
      branch: "main",
    });
    if (checkoutResult.exitCode !== 0) {
      throw new Error("Could not checkout main.");
    }
    const mergeResult = await this.runGitAction(parentProject, {
      action: "merge",
      branch,
    });
    /*
    CDXC:SidebarGitMemo 2026-07-29:
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
      return "conflicts";
    }

    if (input.deleteWorktreeAfter) {
      await this.deleteWorktreeAfterCompletedGitAction(input.worktreeProject);
    }
    return "merged";
  },

  async launchMergeConflictAgent(this: GpuiSidebarRuntime, input: {
    agent: SidebarAgentButton;
    branch: string;
    mergeOutput: string;
    parentProject: GxserverProjectDomainState;
    worktree: GpuiWorktreeMetadata;
    worktreeProject: GxserverProjectDomainState;
  }): Promise<void> {
    this.focusProjectId(input.parentProject.projectId);
    await this.createAgentSessionForProject(
      input.parentProject,
      input.agent,
      buildGpuiMergeConflictPrompt(input),
      formatGpuiGitAgentWorkflowTitle("Merge Conflicts"),
    );
  },

  async runSidebarGitMultipleCommits(this: GpuiSidebarRuntime, requestId: string, agentId?: string): Promise<void> {
    const pending = this.pendingGitCommitRequests.get(requestId);
    this.pendingGitCommitRequests.delete(requestId);
    if (pending?.remoteReference) {
      const remoteScope = this.resolveRemotePresentationProjectScope(pending.remoteReference);
      if (!remoteScope) {
        this.postRemoteToast("warning", "Remote Git unavailable", {
          description: "Reconnect the remote machine before starting this Git workflow.",
        });
        return;
      }
      await this.runRemoteSidebarGitPromptAction(
        remoteScope,
        "Multiple Commits",
        GPUI_GIT_MULTIPLE_COMMITS_PROMPT,
        agentId,
      );
      return;
    }
    const project = pending
      ? this.domainProjectById(pending.projectId)
      : this.activeDomainProject();
    if (!project) {
      this.postGitToast("warning", "Git unavailable", {
        description: "No active gxserver project is available.",
      });
      this.publishHudPatch();
      return;
    }
    await this.runSidebarGitPromptAction(
      project,
      "Multiple Commits",
      GPUI_GIT_MULTIPLE_COMMITS_PROMPT,
      agentId,
    );
  },

  promptSidebarGitActionReview(this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState,
    gitState: SidebarGitState,
    action: Extract<SidebarGitAction, "commit" | "pr" | "push">,
  ): void {
    const requestId = `gpui-git-action-${Date.now().toString(36)}`;
    const hasCommit = gitState.hasWorkingTreeChanges;
    /*
    CDXC:GPUISidebarGit 2026-06-24-15:22:
    GPUI commit review stores the gxserver-derived changed-file list with the request id. Later modal selections and diff clicks may only reference those paths, so CEF cannot stage or inspect arbitrary renderer-supplied paths.
    Treat the modal's all-selected case as that stored review list instead of a fresh unbounded add-all, so files created after review opens cannot slip into the confirmed commit.
    */
    this.pendingGitCommitRequests.set(requestId, {
      action,
      files: [...gitState.files],
      hasCommit,
      projectId: project.projectId,
      subject: "",
    });
    const modalDraft: SidebarPromptGitCommitMessage = {
      action,
      agentId: this.resolveDefaultPromptAgent()?.agentId,
      branch: gitState.branch,
      changedFiles: gitState.files,
      confirmLabel: resolveGpuiSidebarGitConfirmLabel(action, hasCommit),
      deleteWorktreeAfterDefault: false,
      description: hasCommit
        ? "Review and confirm your commit. Leave the message blank to auto-generate one."
        : resolveGpuiSidebarGitPromptDescription(action),
      isDefaultRef: gitState.branch === "main" || gitState.branch === "master",
      isWorktree: normalizeGpuiWorktreeMetadata(project.worktree) !== undefined,
      requestId,
      showCommitMessage: hasCommit,
      suggestedBody: undefined,
      suggestedSubject: "",
      type: "promptGitCommit",
      worktreeName: stringFromRecord(project.worktree, "name"),
    };
    this.openSidebarGitCommitReviewModal(modalDraft);
    this.gitState = { ...gitState, isBusy: false };
    this.publishHudPatch();
  },

  openSidebarGitCommitReviewModal(this: GpuiSidebarRuntime, draft: SidebarPromptGitCommitMessage): void {
    openAppModal({
      gitCommitDraft: draft,
      modal: "gitCommit",
      type: "open",
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
          action: "diffCachedNoExt",
          filePath: normalizedFilePath,
        }),
        this.runGitAction(project, {
          action: "diffNoExt",
          filePath: normalizedFilePath,
        }),
      ]);
      const patchParts = [stagedDiff.stdout.trimEnd(), unstagedDiff.stdout.trimEnd()].filter(
        (part) => part.trim().length > 0,
      );
      let patch = patchParts.join("\n\n");
      if (!patch.trim()) {
        const untracked = await this.runGitAction(project, {
          action: "isUntrackedFile",
          filePath: normalizedFilePath,
        });
        if (untracked.stdout.trim()) {
          const noIndexDiff = await this.runGitAction(project, {
            action: "diffNoIndexAgainstNull",
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

  async openRemoteSidebarGitChangedFileDiff(this: GpuiSidebarRuntime,
    remoteReference: GpuiRemoteProjectReference,
    filePath: string,
    requestId?: string,
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
          action: "diffCachedNoExt",
          filePath: normalizedFilePath,
        }),
        this.runRemoteGitAction(remoteScope, {
          action: "diffNoExt",
          filePath: normalizedFilePath,
        }),
      ]);
      const patchParts = [stagedDiff.stdout.trimEnd(), unstagedDiff.stdout.trimEnd()].filter(
        (part) => part.trim().length > 0,
      );
      let patch = patchParts.join("\n\n");
      if (!patch.trim()) {
        const untracked = await this.runRemoteGitAction(remoteScope, {
          action: "isUntrackedFile",
          filePath: normalizedFilePath,
        });
        if (untracked.stdout.trim()) {
          const noIndexDiff = await this.runRemoteGitAction(remoteScope, {
            action: "diffNoIndexAgainstNull",
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

  async openSidebarGitChangedFileInIde(this: GpuiSidebarRuntime,
    message: Extract<SidebarToExtensionMessage, { type: "openSidebarGitChangedFile" }>,
  ): Promise<void> {
    /*
    CDXC:GPUISidebarGit 2026-06-24-21:26:
    Changed-file IDE opens reuse the shared SidebarApp file row. GPUI sends Rust only the gxserver project id and a normalized relative file candidate already present in the current HUD or review request; Rust remains authoritative and re-validates the file against gxserver before resolving an absolute path.
    Scoped non-review opens must re-read the owning local or remote gxserver project instead of using the active local HUD file list, so remote rows cannot open stale or cross-project file candidates.
    */
    const normalizedFilePath = normalizeGpuiRelativeGitFilePath(message.filePath);
    const request = message.requestId
      ? this.pendingGitCommitRequests.get(message.requestId)
      : undefined;
    if (request?.remoteReference) {
      const remoteScope = this.resolveRemotePresentationProjectScope(request.remoteReference);
      if (
        !normalizedFilePath ||
        !remoteScope ||
        !request.files.some((file) => file.path === normalizedFilePath)
      ) {
        this.postRemoteToast("warning", "Remote file open unavailable", {
          description: "Choose a changed file from the current remote Git review.",
        });
        return;
      }
      this.postRemoteProjectNativeAction(
        "openRemoteSidebarGitChangedFileInIde",
        remoteScope,
        message,
        {
          filePath: normalizedFilePath,
        },
      );
      return;
    }
    if (!request) {
      const remoteScope = this.resolveGitPreferenceRemoteScope(message);
      if (remoteScope) {
        if (!normalizedFilePath) {
          this.postRemoteToast("warning", "Remote file open unavailable", {
            description: "Choose a changed file from the current remote Git state.",
          });
          return;
        }
        const gitState = await this.readRemoteSidebarGitState(remoteScope);
        if (!gitState.files.some((file) => file.path === normalizedFilePath)) {
          this.postRemoteToast("warning", "Remote file open unavailable", {
            description: "Choose a changed file from the current remote Git state.",
          });
          return;
        }
        this.postRemoteProjectNativeAction(
          "openRemoteSidebarGitChangedFileInIde",
          remoteScope,
          message,
          {
            filePath: normalizedFilePath,
          },
        );
        return;
      }
      if (this.isGitPreferenceRemoteScope(message)) {
        this.postRemoteToast("warning", "Remote file open unavailable", {
          description: "Reconnect the remote machine before opening changed files.",
        });
        return;
      }
    }
    const project = request
      ? this.domainProjectById(request.projectId)
      : this.activeDomainProject();
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
      this.postGitToast("warning", "Open file unavailable", {
        description: "Choose a changed file from the current Git state.",
      });
      return;
    }
    this.postNativeProjectPathAction(
      "openSidebarGitChangedFileInIde",
      scopedProject.projectId,
      message,
      {
        filePath: normalizedFilePath,
      },
    );
  },

  postSidebarGitFileDiff(this: GpuiSidebarRuntime, requestId: string, draft: SidebarGitFileDiffDraft): void {
    postAppModalHostMessage(
      {
        gitFileDiff: draft,
        modal: "gitFileDiff",
        requestId,
        type: "open",
      },
      "AppModals:gpuiGitFileDiff",
    );
  },

  resolveTrustedGitReviewFileSelection(this: GpuiSidebarRuntime,
    request: GpuiPendingGitCommitRequest,
    filePaths?: readonly string[],
  ): GpuiTrustedGitReviewFileSelection {
    const explicit = filePaths !== undefined;
    const candidatePaths = explicit ? filePaths : request.files.map((file) => file.path);
    const allowedPaths = new Map(request.files.map((file) => [file.path, file.path]));
    const selectedPaths: string[] = [];
    for (const filePath of candidatePaths) {
      const normalizedPath = normalizeGpuiRelativeGitFilePath(filePath);
      const trustedPath = normalizedPath ? allowedPaths.get(normalizedPath) : undefined;
      if (!trustedPath) {
        throw new Error("Selected file is not part of the current Git review.");
      }
      if (!selectedPaths.includes(trustedPath)) {
        selectedPaths.push(trustedPath);
      }
    }
    if (selectedPaths.length === 0) {
      throw new Error("Select at least one changed file.");
    }
    return { explicit, filePaths: selectedPaths };
  },

  async runGitMutation(this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState,
    startedTitle: string,
    finishedTitle: string,
    operation: () => Promise<void>,
  ): Promise<boolean> {
    const toastId = createGpuiGitToastId();
    this.postGitToast("info", startedTitle, { persistent: true, toastId });
    this.gitState = { ...this.gitStateForHud(), isBusy: true };
    this.publishHudPatch();
    try {
      await operation();
      await this.refreshGitState({ force: true, project });
      this.postGitToast("success", finishedTitle, { toastId });
      return true;
    } catch (error) {
      this.gitState = { ...this.gitStateForHud(), isBusy: false };
      this.publishHudPatch();
      this.postGitToast("error", `${startedTitle} failed`, {
        description: gpuiUserVisibleGitErrorMessage(error, "gxserver Git operation failed."),
        toastId,
      });
      return false;
    }
  },

  async commitWithMessage(this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState,
    message: string,
    filePaths?: readonly string[],
    options: { agentId?: string; commitOnNewRef?: boolean } = {},
  ): Promise<void> {
    const parsedMessage = parseGpuiSidebarGitCommitMessage(message);
    let resolvedMessage = parsedMessage;
    if (parsedMessage.subject) {
      const addResult = await this.runGitAction(project, {
        action: "addAll",
        filePaths,
      });
      if (addResult.exitCode !== 0) {
        throw new Error("Could not stage changes.");
      }
    } else {
      resolvedMessage = await this.generateCommitMessage(project, filePaths, options.agentId);
    }
    if (options.commitOnNewRef) {
      await this.checkoutSidebarGitFeatureBranch(project, resolvedMessage.subject);
    }
    const commitResult = await this.runGitAction(project, {
      action: "commit",
      messageBody: resolvedMessage.body,
      messageSubject: resolvedMessage.subject,
      noVerify: await this.shouldBypassMissingBeadsDatabasePreCommitHook(project),
    });
    if (commitResult.exitCode !== 0) {
      throw new Error("Could not commit changes.");
    }
  },

  async generateCommitMessage(this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState,
    filePaths: readonly string[] | undefined,
    agentId?: string,
  ): Promise<{ body: string; subject: string }> {
    if (!this.client) {
      throw new Error("gxserver is unavailable.");
    }
    if (!filePaths || filePaths.length === 0) {
      throw new Error("Select at least one changed file before generating a commit message.");
    }
    const agent = this.resolveDefaultPromptAgent(agentId);
    if (!agent?.command?.trim()) {
      throw new GpuiUserVisibleGitError(
        "Choose a configured prompt agent before generating a commit message.",
      );
    }
    if (!supportsGpuiBackgroundCommitMessageGeneration(agent)) {
      throw new GpuiUserVisibleGitError(
        "Selected prompt agent does not support background commit message generation.",
      );
    }
    return this.client.rpc<GxserverGenerateCommitMessageResult>("/api/generateCommitMessage", {
      agentId: agent.agentId,
      filePaths: [...filePaths],
      projectId: project.projectId,
    });
  },

  async generateRemoteCommitMessage(this: GpuiSidebarRuntime,
    remoteScope: GpuiRemoteProjectScope,
    filePaths: readonly string[] | undefined,
    agentId?: string,
  ): Promise<{ body: string; subject: string }> {
    if (!filePaths || filePaths.length === 0) {
      throw new Error("Select at least one changed file before generating a commit message.");
    }
    const resolvedAgentId = this.resolveDefaultPromptAgentId(agentId);
    if (!resolvedAgentId) {
      throw new GpuiUserVisibleGitError(
        "Choose a prompt agent before generating a remote commit message.",
      );
    }
    return this.requestRemoteGxserver<GxserverGenerateCommitMessageResult>(
      remoteScope.machineId,
      "/api/generateCommitMessage",
      {
        agentId: resolvedAgentId,
        filePaths: [...filePaths],
        projectId: remoteScope.projectId,
      },
      { timeoutMs: 125_000 },
    );
  },

  async checkoutSidebarGitFeatureBranch(this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState,
    subject: string,
  ): Promise<string> {
    const baseName = sanitizeGpuiSidebarGitBranchName(subject);
    for (let index = 0; index < 20; index += 1) {
      const candidate = index === 0 ? baseName : `${baseName}-${index + 1}`;
      const exists = await this.runGitAction(project, {
        action: "verifyRef",
        ref: candidate,
      });
      if (exists.exitCode !== 0) {
        const checkout = await this.runGitAction(project, {
          action: "checkoutNewBranch",
          branch: candidate,
        });
        if (checkout.exitCode !== 0) {
          throw new Error("Could not create a new branch.");
        }
        return candidate;
      }
    }
    throw new Error("Could not create a unique branch.");
  },

  async pushCurrentBranch(this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState,
    gitState: Pick<SidebarGitState, "branch" | "behindCount" | "hasOriginRemote" | "hasUpstream">,
  ): Promise<void> {
    const branch = gitState.branch;
    if (!branch) {
      throw new Error("Create and checkout a branch before pushing.");
    }
    if (gitState.behindCount > 0) {
      throw new Error("Branch is behind upstream.");
    }
    const push = gitState.hasUpstream
      ? await this.runGitAction(project, { action: "push" })
      : gitState.hasOriginRemote
        ? await this.runGitAction(project, { action: "pushSetUpstream", branch })
        : undefined;
    if (!push) {
      throw new Error('Add an "origin" remote before pushing.');
    }
    if (push.exitCode !== 0) {
      throw new Error("Could not push branch.");
    }
  },

  async syncCurrentBranchWithRemote(this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState,
    gitState: SidebarGitState,
  ): Promise<void> {
    const branch = gitState.branch;
    if (!branch) {
      throw new Error("Create and checkout a branch before syncing.");
    }
    if (gitState.hasUpstream) {
      const pull = await this.runGitAction(project, { action: "pullFastForward" });
      if (pull.exitCode !== 0) {
        throw new Error("Could not pull branch.");
      }
      const nextGitState = await this.refreshGitState({ force: true, project });
      if (nextGitState.aheadCount > 0) {
        await this.pushCurrentBranch(project, nextGitState);
      }
      return;
    }
    await this.pushCurrentBranch(project, gitState);
  },

  async commitRemoteWithMessage(this: GpuiSidebarRuntime,
    remoteScope: GpuiRemoteProjectScope,
    message: string,
    filePaths?: readonly string[],
    options: { agentId?: string; commitOnNewRef?: boolean } = {},
  ): Promise<void> {
    const parsedMessage = parseGpuiSidebarGitCommitMessage(message);
    let resolvedMessage = parsedMessage;
    if (parsedMessage.subject) {
      const addResult = await this.runRemoteGitAction(remoteScope, {
        action: "addAll",
        filePaths,
      });
      if (addResult.exitCode !== 0) {
        throw new Error("Could not stage remote changes.");
      }
    } else {
      resolvedMessage = await this.generateRemoteCommitMessage(
        remoteScope,
        filePaths,
        options.agentId,
      );
    }
    if (options.commitOnNewRef) {
      await this.checkoutRemoteSidebarGitFeatureBranch(remoteScope, resolvedMessage.subject);
    }
    const commitResult = await this.runRemoteGitAction(remoteScope, {
      action: "commit",
      messageBody: resolvedMessage.body,
      messageSubject: resolvedMessage.subject,
      noVerify: await this.shouldBypassRemoteMissingBeadsDatabasePreCommitHook(remoteScope),
    });
    if (commitResult.exitCode !== 0) {
      throw new Error("Could not commit remote changes.");
    }
  },

  async checkoutRemoteSidebarGitFeatureBranch(this: GpuiSidebarRuntime,
    remoteScope: GpuiRemoteProjectScope,
    subject: string,
  ): Promise<void> {
    const result = await this.requestRemoteGxserver<GxserverCheckoutProjectNewBranchResult>(
      remoteScope.machineId,
      "/api/checkoutProjectNewBranch",
      {
        branchLabel: subject,
        projectId: remoteScope.projectId,
      },
      { timeoutMs: 30_000 },
    );
    if (result.checkedOut !== true) {
      throw new Error("Could not create a new remote branch.");
    }
  },

  async pushRemoteCurrentBranch(this: GpuiSidebarRuntime,
    remoteScope: GpuiRemoteProjectScope,
    gitState: Pick<SidebarGitState, "branch" | "behindCount" | "hasOriginRemote" | "hasUpstream">,
  ): Promise<void> {
    const branch = gitState.branch;
    if (!branch) {
      throw new Error("Create and checkout a branch before pushing.");
    }
    if (gitState.behindCount > 0) {
      throw new Error("Remote branch is behind upstream.");
    }
    const push = gitState.hasUpstream
      ? await this.runRemoteGitAction(remoteScope, { action: "push" })
      : gitState.hasOriginRemote
        ? await this.runRemoteGitAction(remoteScope, { action: "pushSetUpstreamCurrent" })
        : undefined;
    if (!push) {
      throw new Error('Add an "origin" remote before pushing.');
    }
    if (push.exitCode !== 0) {
      throw new Error("Could not push remote branch.");
    }
  },

  async syncRemoteCurrentBranchWithRemote(this: GpuiSidebarRuntime,
    remoteScope: GpuiRemoteProjectScope,
    gitState: SidebarGitState,
  ): Promise<void> {
    const branch = gitState.branch;
    if (!branch) {
      throw new Error("Create and checkout a branch before syncing.");
    }
    if (gitState.hasUpstream) {
      const pull = await this.runRemoteGitAction(remoteScope, { action: "pullFastForward" });
      if (pull.exitCode !== 0) {
        throw new Error("Could not pull remote branch.");
      }
      const nextGitState = await this.readRemoteSidebarGitState(remoteScope);
      if (nextGitState.aheadCount > 0) {
        await this.pushRemoteCurrentBranch(remoteScope, nextGitState);
      }
      return;
    }
    await this.pushRemoteCurrentBranch(remoteScope, gitState);
  },

  async shouldBypassRemoteMissingBeadsDatabasePreCommitHook(this: GpuiSidebarRuntime,
    remoteScope: GpuiRemoteProjectScope,
  ): Promise<boolean> {
    const beadsStorage = await this.runRemoteBeadsAction(remoteScope, { action: "storageExists" });
    if (beadsStorage.exitCode !== 0 || beadsStorage.stdout.trim() !== "true") {
      return false;
    }
    try {
      const status = await this.runRemoteBeadsAction(remoteScope, { action: "status" });
      return (
        status.exitCode !== 0 &&
        isMissingGpuiBeadsDatabaseError(`${status.stderr}\n${status.stdout}`)
      );
    } catch {
      return false;
    }
  },

  async shouldBypassMissingBeadsDatabasePreCommitHook(this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState,
  ): Promise<boolean> {
    const beadsStorage = await this.runBeadsAction(project, { action: "storageExists" });
    if (beadsStorage.exitCode !== 0 || beadsStorage.stdout.trim() !== "true") {
      return false;
    }
    try {
      const status = await this.runBeadsAction(project, { action: "status" });
      return (
        status.exitCode !== 0 &&
        isMissingGpuiBeadsDatabaseError(`${status.stderr}\n${status.stdout}`)
      );
    } catch {
      return false;
    }
  },

  async runSidebarGitPromptAction(this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState,
    title: string,
    prompt: string,
    agentId?: string,
  ): Promise<void> {
    const gitState = await this.refreshGitState({
      force: true,
      project,
      publishBusy: true,
      toastOnFailure: true,
    });
    if (!gitState.isRepo) {
      this.postGitToast("warning", "Git unavailable", {
        description: "Open a Git repository to use this workflow.",
      });
      return;
    }
    const agent = this.resolveDefaultPromptAgent(agentId);
    if (!agent?.command?.trim()) {
      this.postGitToast("error", "Agent unavailable", {
        description: "Choose a configured prompt agent before starting this Git workflow.",
      });
      return;
    }
    await this.createAgentSessionForProject(
      project,
      agent,
      prompt,
      formatGpuiGitAgentWorkflowTitle(title),
    );
    this.postGitToast("success", "Git workflow started");
  },

  async runRemoteSidebarGitPromptAction(this: GpuiSidebarRuntime,
    remoteScope: GpuiRemoteProjectScope,
    title: string,
    prompt: string,
    agentId?: string,
  ): Promise<void> {
    const gitState = await this.readRemoteSidebarGitState(remoteScope);
    if (!gitState.isRepo) {
      this.postRemoteToast("warning", "Remote Git unavailable", {
        description: "Open a Git repository on the remote machine to use this workflow.",
      });
      return;
    }
    const resolvedAgentId = this.resolveDefaultPromptAgentId(agentId);
    try {
      await this.createRemoteAgentSessionForProject(
        remoteScope,
        resolvedAgentId,
        prompt,
        formatGpuiGitAgentWorkflowTitle(title),
      );
      this.postRemoteToast("success", "Remote Git workflow started");
    } catch {
      this.postRemoteToast("error", "Remote Git workflow failed", {
        description: "The remote gxserver could not start the selected prompt agent.",
      });
    }
  },

  async runSidebarGitPullRequestAgentWorkflow(this: GpuiSidebarRuntime, input: {
    agentId?: string;
    filePaths?: readonly string[];
    gitState: SidebarGitState;
    hasExplicitFileSelection: boolean;
    hasCommit: boolean;
    message: string;
    project: GxserverProjectDomainState;
  }): Promise<void> {
    const agent = this.resolveDefaultPromptAgent(input.agentId);
    if (!agent?.command?.trim()) {
      this.postGitToast("error", "Agent unavailable", {
        description: "Choose a configured prompt agent before creating a pull request.",
      });
      return;
    }
    /*
    CDXC:GPUISidebarGit 2026-06-24-16:45:
    Visible PR-agent workflows are for user-observable, non-delete PR creation only. The terminal session can report gxserver lifecycle/activity, but it cannot prove that `gh pr create` produced an open PR; delete-after cleanup must stay on the direct gxserver PR result path.
    */
    const prompt = buildGpuiGitPullRequestAgentPrompt({
      filePaths: input.filePaths,
      hasExplicitFileSelection: input.hasExplicitFileSelection,
      hasCommit: input.hasCommit,
      message: input.message.trim(),
      selectedFiles:
        input.filePaths && input.filePaths.length > 0
          ? input.filePaths
          : input.gitState.files.map((file) => file.path),
    });
    try {
      await this.createAgentSessionForProject(
        input.project,
        agent,
        prompt,
        formatGpuiGitAgentWorkflowTitle("Commit, Push & PR"),
      );
      this.postGitToast("success", "Pull request workflow started");
    } catch {
      this.postGitToast("error", "Pull request workflow failed", {
        description: "gxserver could not start the selected prompt agent.",
      });
    }
  },

  async runRemoteSidebarGitPullRequestAgentWorkflow(this: GpuiSidebarRuntime, input: {
    agentId?: string;
    filePaths?: readonly string[];
    gitState: SidebarGitState;
    hasExplicitFileSelection: boolean;
    hasCommit: boolean;
    message: string;
    remoteScope: GpuiRemoteProjectScope;
  }): Promise<void> {
    const resolvedAgentId = this.resolveDefaultPromptAgentId(input.agentId);
    const prompt = buildGpuiGitPullRequestAgentPrompt({
      filePaths: input.filePaths,
      hasExplicitFileSelection: input.hasExplicitFileSelection,
      hasCommit: input.hasCommit,
      message: input.message.trim(),
      selectedFiles:
        input.filePaths && input.filePaths.length > 0
          ? input.filePaths
          : input.gitState.files.map((file) => file.path),
    });
    try {
      await this.createRemoteAgentSessionForProject(
        input.remoteScope,
        resolvedAgentId,
        prompt,
        formatGpuiGitAgentWorkflowTitle("Commit, Push & PR"),
      );
      this.postRemoteToast("success", "Remote pull request workflow started");
    } catch {
      this.postRemoteToast("error", "Remote pull request workflow failed", {
        description: "The remote gxserver could not start the selected prompt agent.",
      });
    }
  },

  async persistGitPreferences(this: GpuiSidebarRuntime,
    updates: Partial<GpuiGitPreferences>,
    scopeMessage?: {
      groupId?: string;
      projectId?: string;
    },
  ): Promise<void> {
    const explicitScope = Boolean(scopeMessage?.groupId?.trim() || scopeMessage?.projectId?.trim());
    const remoteScope = this.resolveGitPreferenceRemoteScope(scopeMessage);
    if (remoteScope) {
      await this.persistRemoteGitPreferences(remoteScope, updates);
      return;
    }
    if (explicitScope && this.isGitPreferenceRemoteScope(scopeMessage)) {
      this.postRemoteToast("warning", "Remote Git preferences unavailable", {
        description: "Reconnect the remote machine before changing Git preferences.",
      });
      return;
    }

    const scopedProject = this.resolveGitPreferenceLocalProject(scopeMessage);
    if (explicitScope && !scopedProject) {
      this.postGitToast("warning", "Git preferences unavailable", {
        description: "Choose a current project before changing Git preferences.",
      });
      return;
    }
    const currentPreferences = this.gitPreferencesForProject(
      scopedProject ?? this.activeDomainProject(),
    );
    const nextPreferences: GpuiGitPreferences = {
      ...currentPreferences,
      ...updates,
      primaryAction: normalizeSidebarGitAction(
        updates.primaryAction ?? currentPreferences.primaryAction,
      ),
    };
    if (scopedProject && this.client) {
      const nextProject = await this.updateProjectDomainState(scopedProject.projectId, {
        gitConfig: {
          ...scopedProject.gitConfig,
          confirmCommit: nextPreferences.confirmCommit,
          generateCommitBody: nextPreferences.generateCommitBody,
          primaryAction: nextPreferences.primaryAction,
        },
      });
      if (
        this.activeProjectId === scopedProject.projectId ||
        this.activeProjectId === nextProject?.projectId
      ) {
        this.gitState = {
          ...this.gitState,
          confirmSuggestedCommit: nextPreferences.confirmCommit,
          generateCommitBody: nextPreferences.generateCommitBody,
          primaryAction: nextPreferences.primaryAction,
        };
        this.publishHudPatch();
      }
      return;
    }
    if (!this.client || this.domainProjects.length === 0) {
      this.gitState = {
        ...this.gitState,
        confirmSuggestedCommit: nextPreferences.confirmCommit,
        generateCommitBody: nextPreferences.generateCommitBody,
        primaryAction: nextPreferences.primaryAction,
      };
      this.publishHudPatch();
      return;
    }
    await Promise.all(
      this.domainProjects.map((project) =>
        this.updateProjectDomainState(project.projectId, {
          gitConfig: {
            ...project.gitConfig,
            confirmCommit: nextPreferences.confirmCommit,
            generateCommitBody: nextPreferences.generateCommitBody,
            primaryAction: nextPreferences.primaryAction,
          },
        }),
      ),
    );
    this.gitState = {
      ...this.gitState,
      confirmSuggestedCommit: nextPreferences.confirmCommit,
      generateCommitBody: nextPreferences.generateCommitBody,
      primaryAction: nextPreferences.primaryAction,
    };
    this.publishHudPatch();
  },

  resolveGitPreferenceRemoteScope(this: GpuiSidebarRuntime, scopeMessage?: {
    groupId?: string;
    projectId?: string;
  }): GpuiRemoteProjectScope | undefined {
    if (!scopeMessage) {
      return undefined;
    }
    if (scopeMessage.groupId && parseGpuiRemotePresentationGroupId(scopeMessage.groupId)) {
      return this.resolveRemotePresentationProjectScope({ groupId: scopeMessage.groupId });
    }
    const remoteProject = scopeMessage.projectId
      ? parseGpuiRemotePresentationProjectId(scopeMessage.projectId)
      : undefined;
    return remoteProject ? this.resolveRemotePresentationProjectScope(remoteProject) : undefined;
  },

  isGitPreferenceRemoteScope(this: GpuiSidebarRuntime, scopeMessage?: {
    groupId?: string;
    projectId?: string;
  }): boolean {
    return Boolean(
      (scopeMessage?.groupId && parseGpuiRemotePresentationGroupId(scopeMessage.groupId)) ||
      (scopeMessage?.projectId && parseGpuiRemotePresentationProjectId(scopeMessage.projectId)),
    );
  },

  resolveGitPreferenceLocalProject(this: GpuiSidebarRuntime, scopeMessage?: {
    groupId?: string;
    projectId?: string;
  }): GxserverProjectDomainState | undefined {
    if (scopeMessage?.groupId) {
      const projectId = this.resolveProjectIdForGroup(scopeMessage.groupId);
      return projectId ? this.domainProjectById(projectId) : undefined;
    }
    if (scopeMessage?.projectId) {
      return this.domainProjectById(scopeMessage.projectId);
    }
    return undefined;
  },

  async persistRemoteGitPreferences(this: GpuiSidebarRuntime,
    remoteScope: GpuiRemoteProjectScope,
    updates: Partial<GpuiGitPreferences>,
  ): Promise<void> {
    const currentPreferences = this.gitPreferencesForPresentationProject(
      this.findRemotePresentationProject(remoteScope) ?? remoteScope.project,
    );
    const nextPreferences: GpuiGitPreferences = {
      ...currentPreferences,
      ...updates,
      primaryAction: normalizeSidebarGitAction(
        updates.primaryAction ?? currentPreferences.primaryAction,
      ),
    };
    /*
    CDXC:GPUIRemoteGit 2026-06-24-18:22:
    Remote Git preference writes use only the selected machine id, gxserver project id, and the three known preference keys. Rust owns the tunnel and response shaping; the renderer never sends paths, labels, branch names, command text, URLs, tokens, stdout/stderr, or raw daemon bodies as write authority.
    */
    try {
      const response = await this.requestRemoteGxserver<{
        project?: GxserverPresentationProject;
      }>(remoteScope.machineId, "/api/updateProject", {
        gitConfig: {
          confirmCommit: nextPreferences.confirmCommit,
          generateCommitBody: nextPreferences.generateCommitBody,
          primaryAction: nextPreferences.primaryAction,
        },
        projectId: remoteScope.projectId,
      });
      if (response.project) {
        this.upsertRemotePresentationProject(remoteScope.machineId, response.project);
      } else {
        await this.refreshRemotePresentationFromGxserver(remoteScope.machineId).catch(
          () => undefined,
        );
      }
      if (
        this.activeGroupId ===
        createGpuiRemotePresentationGroupId(remoteScope.machineId, remoteScope.projectId)
      ) {
        this.gitState = {
          ...this.gitState,
          confirmSuggestedCommit: nextPreferences.confirmCommit,
          generateCommitBody: nextPreferences.generateCommitBody,
          primaryAction: nextPreferences.primaryAction,
        };
      }
      this.publishRemotePresentationPatch();
    } catch {
      this.postRemoteToast("warning", "Remote Git preferences unavailable", {
        description: "The remote gxserver could not save that Git preference.",
      });
    }
  },

  resolveGitProjectForMessage(this: GpuiSidebarRuntime,
    message: Extract<SidebarToExtensionMessage, { type: "runSidebarGitAction" }>,
  ): GxserverProjectDomainState | undefined {
    const projectId = message.groupId
      ? this.resolveProjectIdForGroup(message.groupId)
      : (message.projectId ?? this.activeProjectId);
    const project = projectId ? this.domainProjectById(projectId) : this.activeDomainProject();
    if (project && this.activeProjectId !== project.projectId) {
      this.focusProjectId(project.projectId);
      this.publishPresentation("patch");
    }
    return project;
  },

  gitStateForHud(this: GpuiSidebarRuntime): SidebarGitState {
    const preferences = this.gitPreferencesForProject(this.activeDomainProject());
    return {
      ...this.gitState,
      confirmSuggestedCommit: preferences.confirmCommit,
      generateCommitBody: preferences.generateCommitBody,
      primaryAction: preferences.primaryAction,
    };
  },

  gitPreferencesForProject(this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState | undefined,
  ): GpuiGitPreferences {
    return {
      confirmCommit: booleanFromRecord(project?.gitConfig, "confirmCommit") ?? false,
      generateCommitBody: booleanFromRecord(project?.gitConfig, "generateCommitBody") ?? true,
      primaryAction: normalizeSidebarGitAction(
        stringFromRecord(project?.gitConfig, "primaryAction"),
      ),
    };
  },

  gitPreferencesForPresentationProject(this: GpuiSidebarRuntime,
    project: GxserverPresentationProject | undefined,
  ): GpuiGitPreferences {
    return {
      confirmCommit: booleanFromRecord(project?.gitConfig, "confirmCommit") ?? false,
      generateCommitBody: booleanFromRecord(project?.gitConfig, "generateCommitBody") ?? true,
      primaryAction: normalizeSidebarGitAction(
        stringFromRecord(project?.gitConfig, "primaryAction"),
      ),
    };
  },

  resolveDefaultPromptAgent(this: GpuiSidebarRuntime, agentId?: string): SidebarAgentButton | undefined {
    const requestedAgentId = this.resolveDefaultPromptAgentId(agentId);
    return this.resolveSidebarAgent(requestedAgentId);
  },

  resolveDefaultPromptAgentId(this: GpuiSidebarRuntime, agentId?: string): string {
    return (
      agentId?.trim() ||
      this.latestHud.settings?.defaultPromptAgentId?.trim() ||
      DEFAULT_GPUI_PROMPT_AGENT_ID
    );
  },

  async runGitAction(this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState,
    params: Record<string, unknown>,
  ): Promise<GxserverTypedOperationResult> {
    if (!this.client) {
      throw new Error("gxserver is unavailable.");
    }
    /*
    CDXC:SidebarGitMemo 2026-07-29:
    Invalidate at the single chokepoint every Git write goes through, so no
    caller can commit, push, or switch branches and then have a switch back to
    that project republish the pre-mutation state. Deleting before the RPC also
    covers a write that fails halfway.
    */
    if (GPUI_MUTATING_GIT_ACTIONS.has(String(params.action ?? ""))) {
      this.gitStateMemoByProjectId.delete(project.projectId);
      const result = await this.client.rpc<GxserverTypedOperationResult>("/api/runGitAction", {
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
    return this.client.rpc<GxserverTypedOperationResult>("/api/runGitAction", {
      ...params,
      projectId: project.projectId,
    });
  },

  async runRemoteGitAction(this: GpuiSidebarRuntime,
    remoteScope: GpuiRemoteProjectReference,
    params: Record<string, unknown>,
  ): Promise<GxserverTypedOperationResult> {
    return this.requestRemoteGxserver<GxserverTypedOperationResult>(
      remoteScope.machineId,
      "/api/runGitAction",
      {
        ...params,
        projectId: remoteScope.projectId,
      },
    );
  },

  /*
  CDXC:WorktreeRename 2026-08-09-18:40:
  Worktree actions scope to the PARENT project, not the worktree's own row: the
  typed operation derives the worktree family root from the parent of its cwd and
  then refuses a path equal to that cwd, so passing the worktree's id makes the
  operation refuse to act on itself. `createProjectWorktree` already sends the
  parent for the same reason.
  */
  async runWorktreeAction(this: GpuiSidebarRuntime,
    parentProject: GxserverProjectDomainState,
    params: Record<string, unknown>,
  ): Promise<GxserverTypedOperationResult> {
    if (!this.client) {
      throw new Error("gxserver is unavailable.");
    }
    return this.client.rpc<GxserverTypedOperationResult>("/api/runWorktreeAction", {
      ...params,
      projectId: parentProject.projectId,
    });
  },

  async runGitHubAction(this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState,
    params: Record<string, unknown>,
  ): Promise<GxserverTypedOperationResult> {
    if (!this.client) {
      throw new Error("gxserver is unavailable.");
    }
    return this.client.rpc<GxserverTypedOperationResult>("/api/runGitHubAction", {
      ...params,
      projectId: project.projectId,
    });
  },

  async runRemoteGitHubAction(this: GpuiSidebarRuntime,
    remoteScope: GpuiRemoteProjectReference,
    params: Record<string, unknown>,
  ): Promise<GxserverTypedOperationResult> {
    return this.requestRemoteGxserver<GxserverTypedOperationResult>(
      remoteScope.machineId,
      "/api/runGitHubAction",
      {
        ...params,
        projectId: remoteScope.projectId,
      },
    );
  },

  async createPullRequest(this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState,
  ): Promise<GxserverCreatePullRequestResult> {
    if (!this.client) {
      throw new Error("gxserver is unavailable.");
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
    return this.client.rpc<GxserverCreatePullRequestResult>("/api/createPullRequest", {
      projectId: project.projectId,
    });
  },

  async createRemotePullRequest(this: GpuiSidebarRuntime,
    remoteScope: GpuiRemoteProjectReference,
  ): Promise<GpuiRemoteCreatePullRequestResult> {
    return this.requestRemoteGxserver<GpuiRemoteCreatePullRequestResult>(
      remoteScope.machineId,
      "/api/createPullRequest",
      {
        projectId: remoteScope.projectId,
      },
      { timeoutMs: 45_000 },
    );
  },

  async runBeadsAction(this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState,
    params: Record<string, unknown>,
  ): Promise<GxserverTypedOperationResult> {
    if (!this.client) {
      throw new Error("gxserver is unavailable.");
    }
    return this.client.rpc<GxserverTypedOperationResult>("/api/runBeadsAction", {
      ...params,
      projectId: project.projectId,
    });
  },

  async runRemoteBeadsAction(this: GpuiSidebarRuntime,
    remoteScope: GpuiRemoteProjectReference,
    params: Record<string, unknown>,
  ): Promise<GxserverTypedOperationResult> {
    return this.requestRemoteGxserver<GxserverTypedOperationResult>(
      remoteScope.machineId,
      "/api/runBeadsAction",
      {
        ...params,
        projectId: remoteScope.projectId,
      },
      { timeoutMs: 60_000 },
    );
  },

  async runRemoteGitMutation(this: GpuiSidebarRuntime,
    remoteScope: GpuiRemoteProjectScope,
    startedTitle: string,
    finishedTitle: string,
    operation: () => Promise<void>,
  ): Promise<boolean> {
    const toastId = createGpuiGitToastId();
    this.postGitToast("info", startedTitle, { persistent: true, toastId });
    try {
      await operation();
      await this.refreshRemotePresentationFromGxserver(remoteScope.machineId).catch(
        () => undefined,
      );
      this.postGitToast("success", finishedTitle, { toastId });
      return true;
    } catch (error) {
      this.postGitToast("error", `${startedTitle} failed`, {
        description: gpuiUserVisibleGitErrorMessage(error, "Remote gxserver Git operation failed."),
        toastId,
      });
      return false;
    }
  },

  postGitToast(this: GpuiSidebarRuntime,
    level: AppToastLevel,
    title: string,
    options: {
      description?: string;
      persistent?: boolean;
      toastId?: string;
    } = {},
  ): void {
    try {
      postAppModalHostMessage(
        createAppToastRequest(level, title, options.description, {
          persistent: options.persistent,
          toastId: options.toastId,
        }),
        "AppModals:gpuiGitToast",
      );
    } catch {
      /*
      CDXC:GPUISidebarGit 2026-06-24-15:22:
      Git mutations and agent workflows must not depend on toast-host availability. Missing toast presentation is not a reason to fake success or skip gxserver-owned Git state changes.
      */
    }
  },
};

const gpuiSidebarRuntimeGitMethodsShapeCheck: GpuiSidebarRuntimeGitMethods = gpuiSidebarRuntimeGitMethods;
void gpuiSidebarRuntimeGitMethodsShapeCheck;
