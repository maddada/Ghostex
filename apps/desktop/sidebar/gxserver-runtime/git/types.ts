/*
CDXC:GxserverRuntimeSplit 2026-08-23:
Directory split of gxserver-runtime/git.ts (~3,251 lines) into git/. This
file holds the interface only, unchanged from the original: a standalone
type (rather than one derived from `typeof gpuiSidebarRuntimeGitMethods`)
because deriving it would make `GpuiSidebarRuntime` depend on the method
bodies that depend on it, which TypeScript reports as a circular base type.
`gpuiSidebarRuntimeGitMethodsShapeCheck` in `index.ts` is what keeps the two
in step.
*/
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
} from "../types-and-protocol";
import type { AppToastLevel } from "@/packages/shared/app-toast-contract";
import type {
  GxserverCreatePullRequestResult,
  GxserverMergeWorktreeIntoMainResult,
  GxserverPresentationProject,
  GxserverProjectDomainState,
  GxserverTypedOperationResult,
} from "@/packages/shared/gxserver-protocol";
import type { SidebarProjectDiffStats } from "@/packages/shared/project-diff-stats";
import type {
  SidebarPromptGitCommitMessage,
  SidebarToExtensionMessage,
} from "@/packages/shared/session-grid-contract";
import type { SidebarAgentButton } from "@/packages/shared/sidebar-agents";
import type {
  SidebarGitAction,
  SidebarGitFileDiffDraft,
  SidebarGitState,
} from "@/packages/shared/sidebar-git";

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

