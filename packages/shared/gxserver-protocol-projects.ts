import type { GxserverProjectDomainState } from './gxserver-protocol-domain';
import type {
  GxserverServerId,
  GxserverProjectId,
  GxserverSessionId,
  GxserverLogLevel,
  GxserverLogOrder,
} from './gxserver-protocol-core';

export interface GxserverProjectDirectoryBrowseParams {
  cwd?: string;
  limit?: number;
  partialPath: string;
  inspectPath?: string;
}

export interface GxserverProjectDirectoryBrowseEntry {
  fullPath: string;
  name: string;
}

export interface GxserverProjectDirectoryBrowseResult {
  entries: GxserverProjectDirectoryBrowseEntry[];
  isDriveList?: boolean;
  parentPath: string;
  inspection?: {
    path: string;
    kind: 'directory' | 'file' | 'missing';
    projectPath?: string;
    projectId?: string;
    gitRoot?: string;
  };
}

/**
 * CDXC:AddProject 2026-08-18:
 * The Add Project dialog can create a destination folder before it adds or
 * clones into it. `name` is a single path segment validated by the daemon (no
 * separators, no `.`/`..`), so the caller names a child of a directory it just
 * browsed rather than an arbitrary path.
 */
export interface GxserverCreateProjectDirectoryParams {
  name: string;
  parentPath: string;
}

export interface GxserverCreateProjectDirectoryResult {
  name: string;
  parentPath: string;
  path: string;
}

export interface GxserverAddProjectPathParams {
  /**
   * Creates the workspace root (`mkdir -p`) when it does not exist yet, which
   * is what the Add Project dialog's "Create & Add" affordance submits. When
   * the flag is absent a missing path is still rejected with `notFound`.
   */
  createIfMissing?: boolean;
  name?: string;
  path: string;
  systemKind?: GxserverProjectDomainState['systemKind'];
  visibility?: GxserverProjectDomainState['visibility'];
}

export type GxserverSourceControlProviderKind = 'azure-devops' | 'bitbucket' | 'github' | 'gitlab';

/**
 * `unsupported` means gxserver itself has no implementation for the provider
 * (Bitbucket / Azure DevOps today), as opposed to `missing`, which means the
 * provider's CLI is simply not installed on that machine.
 */
export type GxserverSourceControlDiscoveryStatus = 'available' | 'missing' | 'unsupported';

export type GxserverSourceControlAuthStatus = 'authenticated' | 'unauthenticated' | 'unknown';

export interface GxserverSourceControlProviderAuth {
  account?: string;
  detail?: string;
  host?: string;
  status: GxserverSourceControlAuthStatus;
}

export interface GxserverSourceControlProviderDiscovery {
  auth: GxserverSourceControlProviderAuth;
  detail?: string;
  executable?: string;
  installHint: string;
  /** Present when the CLI is missing: the `/api/managedTools` tool that installs it (`gh`, `glab`). */
  installTool?: string;
  label: string;
  provider: GxserverSourceControlProviderKind;
  status: GxserverSourceControlDiscoveryStatus;
  version?: string;
}

export interface GxserverSourceControlDiscovery {
  checkedAt: string;
  providers: GxserverSourceControlProviderDiscovery[];
}

export interface GxserverDiscoverSourceControlParams {
  cwd?: string;
}

export interface GxserverDiscoverSourceControlResult {
  discovery: GxserverSourceControlDiscovery;
}

export interface GxserverLookupRepositoryParams {
  cwd?: string;
  provider: GxserverSourceControlProviderKind;
  repository: string;
}

export interface GxserverSourceControlRepositoryInfo {
  nameWithOwner: string;
  provider: GxserverSourceControlProviderKind;
  sshUrl: string;
  url: string;
}

export interface GxserverLookupRepositoryResult {
  repository: GxserverSourceControlRepositoryInfo;
}

export interface GxserverStoragePaths {
  authToken: string;
  config: string;
  identity: string;
  logs: string;
  migrations: string;
  root: string;
  runtime: string;
  stateDb: string;
  zmx: string;
}

export interface GxserverLogEntry {
  ts: string;
  level: GxserverLogLevel;
  event: string;
  serverId?: GxserverServerId;
  requestId?: string;
  projectId?: GxserverProjectId;
  sessionId?: GxserverSessionId;
  client?: string;
  durationMs?: number;
  error?: string;
  details?: Record<string, unknown>;
  legacyFile?: string;
  message?: string;
  source?: string;
}

export interface GxserverQueryLogsParams {
  client?: string;
  event?: string;
  eventPrefix?: string;
  level?: GxserverLogLevel | readonly GxserverLogLevel[];
  limit?: number;
  order?: GxserverLogOrder;
  projectId?: GxserverProjectId;
  reverse?: boolean;
  serverId?: GxserverServerId;
  sessionId?: GxserverSessionId;
  since?: string;
  /** A plain, case-sensitive substring the stored log line must contain. */
  text?: string;
  until?: string;
}

export interface GxserverQueryLogsResult {
  entries: GxserverLogEntry[];
  logFileSizeBytes?: number;
  malformedLineCount: number;
  malformedLineCountIsExact?: boolean;
  scannedBytes?: number;
  scannedLineCount?: number;
  totalMatched: number;
  totalMatchedIsExact?: boolean;
  truncated?: boolean;
  truncatedReason?: 'fileWindowExceeded';
}

export type GxserverGitAction =
  | 'branch'
  | 'addAll'
  | 'changesDiff'
  | 'changesRevert'
  | 'checkout'
  | 'checkoutNewBranch'
  | 'commit'
  | 'countFileLines'
  | 'deleteLocalBranch'
  | 'deleteRemoteBranch'
  | 'diff'
  | 'diffCached'
  | 'diffCachedFiles'
  | 'diffCachedStatFiles'
  | 'diffCachedNoExt'
  | 'diffCachedStat'
  | 'diffNoExt'
  | 'diffNoIndexAgainstNull'
  | 'diffNumstat'
  | 'getOriginRemoteUrl'
  | 'isInsideWorkTree'
  | 'isUntrackedFile'
  | 'list'
  | 'listBranches'
  | 'listRemotes'
  | 'listUntracked'
  | 'merge'
  | 'pullFastForward'
  | 'push'
  | 'pushSetUpstreamCurrent'
  | 'pushSetUpstream'
  | 'remoteBranchExists'
  | 'status'
  | 'statusPorcelain'
  | 'statusPorcelainZ'
  | 'upstreamCounts'
  | 'verifyRef';
export type GxserverWorktreeAction =
  'create' | 'ensureBeadsHooks' | 'list' | 'pathExists' | 'prune' | 'remove' | 'switch';
export type GxserverBeadsAction =
  | 'addLabel'
  | 'board'
  | 'close'
  | 'comment'
  | 'configGet'
  | 'configGetIssuePrefix'
  | 'configSet'
  | 'create'
  | 'delete'
  | 'depAdd'
  | 'depRemove'
  | 'list'
  | 'listAllLabels'
  | 'renamePrefix'
  | 'removeLabel'
  | 'search'
  | 'setLabels'
  | 'show'
  | 'status'
  | 'storageExists'
  | 'update'
  | 'updateDescription'
  | 'updateEstimate'
  | 'updatePriority'
  | 'updateStatus'
  | 'updateTitle';
export type GxserverBeadsStatus = 'backlog' | 'closed' | 'in_progress' | 'open' | 'review' | 'test';
export type GxserverGitHubAction = 'prCreateFill' | 'prView' | 'version';
export type GxserverProjectSetupAction = 'worktreeSetupCommand';

export interface GxserverProjectOperationScope {
  projectId?: GxserverProjectId;
  projectPath?: string;
}

export interface GxserverRunGitActionParams extends GxserverProjectOperationScope {
  action: GxserverGitAction;
  branch?: string;
  filePath?: string;
  filePaths?: readonly string[];
  messageBody?: string;
  messageSubject?: string;
  noVerify?: boolean;
  ref?: string;
  remoteName?: string;
}

/*
CDXC:Git 2026-06-24-16:11:
Blank GPUI commit messages are generated by gxserver from a registered project and
the review-approved file set. The renderer sends only project id, selected
project-relative paths, and the chosen prompt-agent id; gxserver owns staging,
diff extraction, prompt construction, and generated subject/body parsing.
Remote GPUI may use this only through the saved-machine Rust tunnel, where
Rust validates the machine endpoint and returns only subject/body to CEF.
*/
export interface GxserverGenerateCommitMessageParams extends GxserverProjectOperationScope {
  agentId?: string;
  filePaths: readonly string[];
}

export interface GxserverGenerateCommitMessageResult {
  body: string;
  subject: string;
}

export type GxserverPullRequestState = 'open' | 'closed' | 'merged';

export interface GxserverPullRequestSummary {
  number?: number;
  state: GxserverPullRequestState;
  url: string;
}

/*
CDXC:Git 2026-06-24-16:28:
Direct GPUI PR creation needs a gxserver-owned completion signal before opening the PR or deleting a finished worktree. The RPC accepts only trusted project scope, runs fixed GitHub CLI actions in gxserver, and returns sanitized PR state/URL metadata without raw command output, branch names, titles, commit messages, or shell text.

CDXC:RemoteMachines 2026-06-24-19:25:
Remote GPUI may use this completion signal only as gxserver-confirmed PR state; actual remote browser opens are a separate Rust-owned native side effect that re-runs `prView` through the saved-machine tunnel and opens only a validated HTTPS GitHub PR URL.
*/
export type GxserverCreatePullRequestParams = GxserverProjectOperationScope;

export interface GxserverCreatePullRequestResult {
  created: boolean;
  ok: boolean;
  pr?: GxserverPullRequestSummary;
  reason?: 'createFailed' | 'githubCliUnavailable' | 'invalidResult' | 'viewFailed';
}

export interface GxserverRunWorktreeActionParams extends GxserverProjectOperationScope {
  action: GxserverWorktreeAction;
  baseRef?: string;
  branch?: string;
  force?: boolean;
  worktreePath?: string;
}

/*
CDXC:RemoteMachines 2026-06-24-18:40:
Remote GPUI Add Worktree and Git branch parity must not trust renderer-provided
absolute paths or branch targets. These project-id-scoped RPCs let the owning
gxserver derive worktree paths, opaque open-existing keys, merge branches, and
new commit branches from registered project rows plus bounded user labels.
*/
export interface GxserverProjectWorktreeOption {
  branch: string;
  isCurrentProject: boolean;
  isRegistered: boolean;
  name: string;
  path: string;
  worktreeKey: string;
}

export interface GxserverProjectWorktreeListParams {
  projectId: GxserverProjectId;
}

export interface GxserverProjectWorktreeListResult {
  branches: readonly GxserverBranchListEntry[];
  parentProjectId: GxserverProjectId;
  sourceProjectId: GxserverProjectId;
  worktrees: readonly GxserverProjectWorktreeOption[];
}

export interface GxserverCreateProjectWorktreeParams {
  baseRef: string;
  nameHint: string;
  projectId: GxserverProjectId;
}

export interface GxserverOpenProjectWorktreeParams {
  projectId: GxserverProjectId;
  worktreeKey: string;
}

export interface GxserverProjectWorktreeMutationResult {
  project: GxserverProjectDomainState;
}

export interface GxserverMergeWorktreeIntoMainParams {
  projectId: GxserverProjectId;
}

export interface GxserverMergeWorktreeIntoMainResult {
  parentProjectId: GxserverProjectId;
  status: 'conflicts' | 'merged';
}

export interface GxserverCheckoutProjectNewBranchParams {
  branchLabel: string;
  projectId: GxserverProjectId;
}

export interface GxserverCheckoutProjectNewBranchResult {
  checkedOut: true;
}

export interface GxserverRunGitHubActionParams extends GxserverProjectOperationScope {
  action: GxserverGitHubAction;
}

export interface GxserverDeleteWorktreeProjectParams {
  deleteLocalBranch?: boolean;
  deleteRemoteBranch?: boolean;
  projectId: GxserverProjectId;
  remoteName?: string;
}

export type GxserverDeleteWorktreeProjectWarningKind =
  | 'localBranchDeleteFailed'
  | 'localBranchNotResolved'
  | 'pruneFailed'
  | 'remoteBranchDeleteFailed'
  | 'remoteBranchNotResolved';

export interface GxserverDeleteWorktreeProjectWarning {
  kind: GxserverDeleteWorktreeProjectWarningKind;
  message: string;
}

export interface GxserverDeleteWorktreeProjectResult {
  checkoutRemoval: {
    forced: boolean;
    retriedForSubmodules: boolean;
  };
  project: GxserverProjectDomainState;
  warnings: readonly GxserverDeleteWorktreeProjectWarning[];
}

export interface GxserverRunBeadsActionParams extends GxserverProjectOperationScope {
  action: GxserverBeadsAction;
  comment?: string;
  dependsOnId?: string;
  description?: string;
  depType?: string;
  estimate?: number;
  issueId?: string;
  label?: string;
  labels?: readonly string[];
  priority?: string;
  /**
   * CDXC:ProjectBoard 2026-06-13:
   * True only for Project Board originated Beads calls (set by the macOS board bridge). Gates the
   * per-project configurable Beads launch directory (projectBoardConfig.beadsDirectory): board
   * calls opt in, while native Git commit gating probes (storageExists/status on the same endpoint)
   * omit it and stay scoped to the project root.
   */
  projectBoardScope?: boolean;
  query?: string;
  status?: GxserverBeadsStatus;
  title?: string;
  value?: string;
}

export interface GxserverRepositoryCloneOptions {
  branchName?: string;
  cloneMainOnly?: boolean;
  shallowClone?: boolean;
}

/**
 * Two destination shapes are accepted:
 *
 * - `parentPath` + `destinationFolderName` — the Clone Repository modal's
 *   shape. The parent must already exist and ANY existing destination blocks
 *   the clone.
 * - `destinationPath` — the Add Project dialog's shape: one absolute (or `~/`)
 *   path. Missing parents are created by the clone job, an existing empty
 *   directory is cloned into directly, and an existing non-empty directory is
 *   treated as the parent for a new folder named after the repository.
 *
 * `remoteUrl` is an alias for `repositoryInput`; exactly one of them is
 * required.
 */
export interface GxserverRepositoryClonePreviewParams extends GxserverRepositoryCloneOptions {
  destinationFolderName?: string;
  destinationPath?: string;
  folderPath?: string;
  newFolderName?: string;
  parentPath?: string;
  remoteUrl?: string;
  repositoryInput?: string;
}

export interface GxserverRepositoryCloneStartParams extends GxserverRepositoryClonePreviewParams {}

export interface GxserverRepositoryCloneJobParams {
  jobId: string;
}

export interface GxserverRepositoryClonePreviewResult {
  branchName?: string;
  cloneMainOnly: boolean;
  cloneUrl: string;
  defaultFolderName: string;
  /**
   * Whether the resolved destination refuses the clone. This is what
   * `/api/startRepositoryClone` enforces: it is `destinationExists` for the
   * `parentPath` shape. For the `destinationPath` shape, a non-empty selected
   * directory first resolves to its repository-named child, and only that
   * resolved target can block the clone.
   */
  destinationBlocked: boolean;
  destinationExists: boolean;
  destinationExistsKind?: 'directory' | 'file' | 'other';
  destinationFolderName: string;
  destinationIsEmpty?: boolean;
  destinationPath: string;
  parentPath: string;
  repositoryName: string;
  shallowClone: boolean;
  warning?: string;
}

export type GxserverRepositoryCloneJobState = 'running' | 'completed' | 'failed' | 'canceled';

export interface GxserverRepositoryCloneJobStatus {
  completedAt?: string;
  error?: string;
  exitCode?: number;
  jobId: string;
  message: string;
  preview: GxserverRepositoryClonePreviewResult;
  project?: GxserverProjectDomainState;
  projectPath?: string;
  startedAt: string;
  state: GxserverRepositoryCloneJobState;
  stderr?: string;
  stdout?: string;
}

export interface GxserverRepositoryClonePreviewRpcResult {
  preview: GxserverRepositoryClonePreviewResult;
}

export interface GxserverRepositoryCloneJobRpcResult {
  job: GxserverRepositoryCloneJobStatus;
}

export interface GxserverTypedCommand {
  args: readonly string[];
  cwd: string;
  executable: string;
}

export type GxserverTypedOperationFailureCode =
  'aborted' | 'stderrLimitExceeded' | 'stdinFailed' | 'stdoutLimitExceeded' | 'timeout';

export interface GxserverTypedOperationFailure {
  capturedBytes?: number;
  code: GxserverTypedOperationFailureCode;
  limitBytes?: number;
  message: string;
  stream?: 'stderr' | 'stdout';
  timeoutMs?: number;
}

export interface GxserverWorktreeListEntry {
  bare: boolean;
  branch: string;
  detached: boolean;
  path: string;
}

export interface GxserverBranchListEntry {
  current: boolean;
  name: string;
  remote: boolean;
}

export interface GxserverTypedOperationResult {
  action:
    | GxserverGitAction
    | GxserverGitHubAction
    | GxserverWorktreeAction
    | GxserverProjectSetupAction
    | GxserverBeadsAction;
  command?: GxserverTypedCommand;
  error?: GxserverTypedOperationFailure;
  exitCode: number;
  stderr: string;
  stdout: string;
  branches?: readonly GxserverBranchListEntry[];
  issue?: Record<string, unknown>;
  worktrees?: readonly GxserverWorktreeListEntry[];
}

export interface GxserverBeadsBoardResult extends GxserverTypedOperationResult {
  issues: readonly Record<string, unknown>[];
}
