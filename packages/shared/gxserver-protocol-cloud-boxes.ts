/**
 * CDXC:CloudBoxes 2026-11-02 SEE-ALSO:
 * The Cloud Boxes wire contract, the TypeScript twin of `packages/gx-protocol/src/cloud_box/`
 * (change both together). Sessions run on an Instabox or Boat machine (or, for zero-cost
 * development, a `local` Docker container) with gxserver on the box. Home gxserver owns the
 * registry and every provider call (`server/src/cloud_boxes/`); a box's own gxserver runs in box
 * mode. One route, `/api/cloudBoxes`, with an `action` param; pushes ride the event socket as
 * `cloudBoxesChanged`. Strings stay open: an unknown value must not fail a client.
 */

export const GXSERVER_CLOUD_BOXES_ROUTE = '/api/cloudBoxes' as const;
export const GXSERVER_CLOUD_BOXES_CHANGED_EVENT_TYPE = 'cloudBoxesChanged' as const;

/** `local` is the development provider: listed only with Show debug UI controls on. */
export type GxserverCloudProviderId = 'instabox' | 'boat' | 'local' | (string & {});

export type GxserverCloudBoxState =
  | 'pending'
  | 'creating'
  | 'booting'
  | 'running'
  | 'idle'
  | 'paused'
  | 'stopped'
  | 'deleting'
  | 'deleted'
  | 'failed'
  | (string & {});

export type GxserverCloudProviderConnectionState =
  'notConnected' | 'connecting' | 'connected' | 'expired' | 'error' | (string & {});

export type GxserverCloudConnectMethod = 'browser' | 'key' | 'none' | (string & {});
export type GxserverCloudSnapshotStatus = 'pending' | 'ready' | 'failed' | (string & {});
export type GxserverCloudBoxBootStep =
  'checkCap' | 'create' | 'boot' | 'secrets' | 'prepareSession' | 'connect' | (string & {});
export type GxserverCloudBoxStepState = 'waiting' | 'running' | 'done' | 'failed' | 'skipped' | (string & {});
export type GxserverCloudSpendState = 'off' | 'ok' | 'warn' | 'reached' | (string & {});
export type GxserverCloudSpendScope = 'account' | 'ghostexBoxes' | (string & {});
/** `computer`, `cli`, or `phone:<deviceId>`. */
export type GxserverCloudBoxCreatedBy = string;

export interface GxserverCloudProviderLimits {
  maxBoxes?: number;
  maxRunningBoxes?: number;
  maxSnapshots?: number;
  maxBoxRunSeconds?: number;
  /** Size ids the plan may create; empty means every size. */
  allowedSizes: string[];
  note?: string;
}

/** A provider row (`providers`). Keys never leave home gxserver's vault. */
export interface GxserverCloudProvider {
  id: GxserverCloudProviderId;
  label: string;
  state: GxserverCloudProviderConnectionState;
  connectMethods: GxserverCloudConnectMethod[];
  available: boolean;
  unavailableReason?: string;
  accountLabel?: string;
  accountEmail?: string;
  orgId?: string;
  plan?: string;
  limits?: GxserverCloudProviderLimits;
  connectedAt?: string;
  error?: string;
  debugOnly?: true;
}

export interface GxserverCloudBoxSize {
  /** `<vcpus>x<memGiB>`, e.g. `2x4`. */
  id: string;
  provider: GxserverCloudProviderId;
  vcpus: number;
  memMib: number;
  label: string;
  usdPerHour?: number;
  isDefault?: true;
}

export interface GxserverCloudBoxSessionRow {
  projectId: string;
  sessionId: string;
  title: string;
  agentId?: string;
  branch?: string;
  agentSessionId?: string;
  lastActiveAt?: string;
  homeSessionId?: string;
  archived?: true;
}

export interface GxserverCloudBoxDraftRef {
  projectId: string;
  sessionId: string;
}

export interface GxserverCloudBoxProgressStep {
  step: GxserverCloudBoxBootStep;
  state: GxserverCloudBoxStepState;
  label: string;
  startedAt?: string;
  finishedAt?: string;
  message?: string;
}

export interface GxserverCloudBoxProgress {
  draft?: GxserverCloudBoxDraftRef;
  /** `create` or `resume`. */
  kind: string;
  steps: GxserverCloudBoxProgressStep[];
  startedAt?: string;
  finishedAt?: string;
  error?: string;
}

/** One registry row (`list`, `get`, `create`, `cloudBoxesChanged`). Never carries secrets. */
export interface GxserverCloudBox {
  /** `gx_<12 lowercase base32>`. */
  boxId: string;
  provider: GxserverCloudProviderId;
  providerBoxId?: string;
  /** e.g. `box-c31`. */
  name: string;
  state: GxserverCloudBoxState;
  stateDetail?: string;
  projectKey: string;
  homeProjectId?: string;
  snapshotId?: string;
  size?: string;
  tailcatAddress?: string;
  gxserverVersion?: string;
  createdBy?: GxserverCloudBoxCreatedBy;
  createdAt?: string;
  startedAt?: string;
  lastActiveAt?: string;
  stoppedAt?: string;
  deleteAfter?: string;
  idleStopsAt?: string;
  costTotalUsd?: number;
  costMonthUsd?: number;
  costEstimated?: true;
  usdPerHour?: number;
  hasUnpushedWork?: boolean;
  snapshotUpdateReason?: string;
  sessions: GxserverCloudBoxSessionRow[];
  progress?: GxserverCloudBoxProgress;
}

export interface GxserverCloudSnapshot {
  snapshotId: string;
  provider: GxserverCloudProviderId;
  providerSnapshotId?: string;
  projectKey: string;
  name: string;
  isDefault: boolean;
  status: GxserverCloudSnapshotStatus;
  sourceBoxId?: string;
  baseBranch?: string;
  commitSha?: string;
  sizeBytes?: number;
  size?: string;
  createdAt?: string;
  setupScriptPath?: string;
  previousOf?: string;
  expiresAt?: string;
  sharedWithOrg?: true;
  error?: string;
}

export interface GxserverCloudSpendLine {
  provider: GxserverCloudProviderId;
  usd: number;
  scope: GxserverCloudSpendScope;
  readAt?: string;
  estimated?: true;
  error?: string;
}

/** Warn at 80% of the cap, refuse new boxes at 100% (running boxes keep running). */
export const GXSERVER_CLOUD_SPEND_WARN_FRACTION = 0.8;

export interface GxserverCloudSpend {
  /** `YYYY-MM`, UTC. */
  month: string;
  providers: GxserverCloudSpendLine[];
  totalUsd: number;
  capUsd?: number;
  state: GxserverCloudSpendState;
}

/** Prefix of a box's hidden machine id, `cloudbox:<boxId>`. */
export const GXSERVER_CLOUD_BOX_MACHINE_PREFIX = 'cloudbox:';

export interface GxserverCloudBoxMachine {
  machineId: string;
  boxId: string;
  provider: GxserverCloudProviderId;
  state: GxserverCloudBoxState;
  homeProjectId?: string;
  startedAt?: string;
  costUsd?: number;
}

/** `connectInfo`: `tailcat` (Instabox, Boat) or `direct` (`local`, a loopback port). */
export interface GxserverCloudBoxConnectInfo {
  boxId: string;
  transport: 'tailcat' | 'direct' | (string & {});
  tailcatAddress?: string;
  sshPort?: number;
  sshUser?: string;
  gxserverPort: number;
  baseUrl?: string;
  gxserverToken: string;
  gxserverVersion?: string;
}

/** Settings keys gxserver reads (rows in `packages/settings-catalog`). */
export const GXSERVER_CLOUD_BOXES_SETTING_KEYS = {
  monthlyCapUsd: 'cloudBoxesMonthlyCapUsd',
  idleStopMinutes: 'cloudBoxesIdleStopMinutes',
  deleteStoppedAfterDays: 'cloudBoxesDeleteStoppedAfterDays',
  phoneStartEnabled: 'cloudBoxesPhoneStartEnabled',
} as const;

export type GxserverCloudBoxRemember = 'local' | 'cloud' | (string & {});

/** `projects.runtimeSettingsJson.cloudBox`. Secret values never go here. */
export interface GxserverProjectCloudBoxSettings {
  remember?: GxserverCloudBoxRemember;
  provider?: GxserverCloudProviderId;
  snapshotId?: string;
  size?: string;
  secrets: { files: string[]; vars: string[] };
}

/** A draft whose tray picked Cloud Box: `runtimeSettings.cloudBox` on the home draft. */
export interface GxserverDraftCloudBox {
  pending: boolean;
  provider?: GxserverCloudProviderId;
  snapshotId?: string;
  size?: string;
  reuseBoxId?: string;
  worktree?: boolean;
  /** Start from the provider's base image (Set up cloud box, guided setup). */
  blank?: boolean;
  /** The first message's pictures on this computer; the box start copies them onto the box. */
  imagePaths?: string[];
  boxId?: string;
}

/** A session on a box: `runtimeSettings.cloudBox` on the box's gxserver. */
export interface GxserverSessionCloudBoxRuntime {
  boxId: string;
  provider: GxserverCloudProviderId;
  homeProjectId?: string;
  homeMachineId?: string;
  homeSessionId?: string;
  snapshotId?: string;
  branch?: string;
  agentSessionId?: string;
}

/** The presentation session field `cloudBox` (☁ badge and its state dot). */
export interface GxserverPresentationCloudBox {
  boxId: string;
  provider: GxserverCloudProviderId;
  state: GxserverCloudBoxState;
  boxName: string;
  homeProjectId?: string;
  pending?: true;
}

export type GxserverCloudBoxesAction =
  | 'providers'
  | 'connect'
  | 'connectStatus'
  | 'disconnect'
  | 'sizes'
  | 'list'
  | 'get'
  | 'create'
  | 'resume'
  | 'stop'
  | 'delete'
  | 'connectInfo'
  | 'snapshots'
  | 'saveSnapshot'
  | 'setDefaultSnapshot'
  | 'deleteSnapshot'
  | 'spend'
  | 'projectSettings'
  | 'reportBoxSessions'
  | 'agentLogins'
  | 'setAgentLogin'
  | 'boxIdleStatus'
  | 'boxWorkStatus'
  | 'boxWriteBack'
  | 'pullBranch'
  | 'cloudKit';

export const GXSERVER_CLOUD_BOXES_ERROR_CODES = {
  capReached: 'cloudCapReached',
  providerNotConnected: 'cloudProviderNotConnected',
  providerUnavailable: 'cloudProviderUnavailable',
  planLimit: 'cloudPlanLimit',
  providerError: 'cloudProviderError',
  hasUnpushedWork: 'cloudBoxHasUnpushedWork',
  boxInUse: 'cloudBoxInUse',
  wrongState: 'cloudBoxWrongState',
  kitOff: 'cloudKitOff',
} as const;

/** Params of each action (every request also carries `action`). */
export interface GxserverCloudBoxesParams {
  providers: Record<string, never>;
  connect: {
    provider: GxserverCloudProviderId;
    method?: GxserverCloudConnectMethod;
    key?: string;
    email?: string;
    openBrowser?: boolean;
  };
  connectStatus: { provider: GxserverCloudProviderId; connectId: string };
  disconnect: { provider: GxserverCloudProviderId };
  sizes: { provider?: GxserverCloudProviderId };
  list: { projectId?: string; includeDeleted?: boolean; refresh?: boolean };
  get: { boxId: string };
  create: {
    projectId: string;
    provider?: GxserverCloudProviderId;
    snapshotId?: string;
    size?: string;
    blank?: boolean;
    draft?: GxserverCloudBoxDraftRef;
    /** Start the draft's session on this box instead of a new one (its own worktree there). */
    reuseBoxId?: string;
    idempotencyKey?: string;
  };
  resume: { boxId: string; idempotencyKey?: string };
  stop: { boxId: string };
  delete: { boxId: string; force?: boolean };
  connectInfo: { boxId: string };
  snapshots: { projectId?: string; provider?: GxserverCloudProviderId };
  saveSnapshot: { boxId: string; name?: string; makeDefault?: boolean; replaceDefault?: boolean };
  setDefaultSnapshot: { snapshotId: string };
  deleteSnapshot: { snapshotId: string };
  spend: { refresh?: boolean };
  projectSettings: { projectId: string; settings?: GxserverProjectCloudBoxSettings };
  agentLogins: Record<string, never>;
  setAgentLogin: { accountId: string; allowed: boolean };
  reportBoxSessions: { boxId: string; sessions: GxserverCloudBoxSessionRow[] };
  boxIdleStatus: Record<string, never>;
  boxWorkStatus: Record<string, never>;
  boxWriteBack: { ack?: string[] };
  /** A box session's pushed branch into a worktree of its home project (Q1014). */
  pullBranch: { boxId: string; sessionId?: string };
  cloudKit: Record<string, never>;
}

export interface GxserverCloudConnectResult {
  provider: GxserverCloudProviderId;
  state: GxserverCloudProviderConnectionState;
  connectId?: string;
  authUrl?: string;
  /** Boat: the 6-digit code the user types on the claim page. */
  userCode?: string;
  browserOpened: boolean;
  expiresAt?: string;
  providerRow?: GxserverCloudProvider;
  error?: string;
}

export type GxserverCloudAgentLoginState = 'notAllowed' | 'waiting' | 'ready' | 'error' | (string & {});

/**
 * One Claude or Codex account and whether boxes may use it (D5 Claude box token from
 * `claude setup-token`, D6 Codex `auth.json` copied at boot). Tokens never appear here.
 */
export interface GxserverCloudAgentLogin {
  accountId: string;
  agent: 'claude' | 'codex' | (string & {});
  name: string;
  state: GxserverCloudAgentLoginState;
  authUrl?: string;
  detail?: string;
  allowedAt?: string;
}

export interface GxserverBoxIdleStatus {
  idle: boolean;
  runningTurns: number;
  openTerminals: number;
  idleSince?: string;
  stopsAt?: string;
  idleStopMinutes: number;
  selfStop: boolean;
}

export interface GxserverBoxWorkStatus {
  hasUnpushedWork: boolean;
  checkouts: Array<{
    path: string;
    branch?: string;
    uncommittedFiles: number;
    unpushedCommits: number;
  }>;
}

export interface GxserverBoxWriteBackResult {
  items: Array<{ id: string; accountId: string; kind: string; value: string; refreshedAt: string }>;
}

/** Results of each action. */
export interface GxserverCloudBoxesResults {
  providers: { providers: GxserverCloudProvider[] };
  connect: GxserverCloudConnectResult;
  connectStatus: GxserverCloudConnectResult;
  disconnect: { providers: GxserverCloudProvider[] };
  sizes: { sizes: GxserverCloudBoxSize[] };
  list: { boxes: GxserverCloudBox[]; machines: GxserverCloudBoxMachine[] };
  get: { box: GxserverCloudBox };
  create: { box: GxserverCloudBox; reused: boolean };
  resume: { box: GxserverCloudBox };
  stop: { box: GxserverCloudBox };
  delete: { box: GxserverCloudBox };
  connectInfo: GxserverCloudBoxConnectInfo;
  snapshots: { snapshots: GxserverCloudSnapshot[] };
  saveSnapshot: { snapshot: GxserverCloudSnapshot };
  setDefaultSnapshot: { snapshot: GxserverCloudSnapshot };
  deleteSnapshot: { snapshot: GxserverCloudSnapshot };
  spend: GxserverCloudSpend;
  /** `envFiles`: the project's `.env*` files, the candidates of "Send to the box". */
  projectSettings: {
    projectId: string;
    settings: GxserverProjectCloudBoxSettings;
    envFiles: string[];
  };
  agentLogins: { logins: GxserverCloudAgentLogin[] };
  setAgentLogin: { logins: GxserverCloudAgentLogin[] };
  reportBoxSessions: { box: GxserverCloudBox };
  boxIdleStatus: GxserverBoxIdleStatus;
  boxWorkStatus: GxserverBoxWorkStatus;
  boxWriteBack: GxserverBoxWriteBackResult;
  pullBranch: { branch: string; projectId: string; path: string; created: boolean; note?: string };
  cloudKit: GxserverCloudKit;
}

/** One secret of `secrets.json` on a box (`BoxSecretFile`, `BoxSecretVar`) and an allowed login (`BoxInjectedAccount`). */
export interface GxserverBoxSecretFile {
  path: string;
  contents: string;
}
export interface GxserverBoxSecretVar {
  name: string;
  value: string;
}
export interface GxserverBoxInjectedAccount {
  accountId: string;
  agent: string;
  name: string;
  /** `claudeBoxToken` or `codexAuthJson`. */
  kind: string;
  value: string;
}

/**
 * `cloudKit` (`packages/gx-protocol/src/cloud_box/kit.rs`): what a paired phone keeps to start a box
 * from a project's Default snapshot while the computer is off (Q1033 D7). Only while Settings ›
 * Cloud Boxes › "Start boxes from your phone" is on; it carries secrets.
 */
export interface GxserverCloudKit {
  version: number;
  homeMachineId: string;
  gxserverVersion: string;
  issuedAt: string;
  providers: Array<{ provider: GxserverCloudProviderId; key: string }>;
  accounts: GxserverBoxInjectedAccount[];
  allowDeviceKeys: string[];
  idleStopMinutes: number;
  projects: Array<{
    homeProjectId: string;
    projectKey: string;
    name: string;
    repoUrl?: string;
    baseBranch?: string;
    snapshot: {
      snapshotId: string;
      provider: GxserverCloudProviderId;
      providerSnapshotId: string;
      size?: string;
      name: string;
    };
    files: GxserverBoxSecretFile[];
    vars: GxserverBoxSecretVar[];
  }>;
  /** The cap and the spend the computer last read: the phone refuses a new box at the cap (D8). */
  spend: { month: string; totalUsd: number; capUsd?: number };
}

/** `cloudBoxesChanged`: `box` is the whole new row (replace; `deleted` = drop); without it refetch. */
export interface GxserverCloudBoxesChangedEvent {
  protocolVersion: number;
  serverId: string;
  type: 'cloudBoxesChanged';
  box?: GxserverCloudBox;
  /** Any of `boxes`, `providers`, `snapshots`, `spend`. */
  scope: string[];
}

/** The `ghostex box …` verbs run on a box (box mode, W2). Each prints one JSON object. */
export const GXSERVER_BOX_VERBS = [
  'boot',
  'prepare-session',
  'prepare-snapshot',
  'finish-snapshot',
  'idle-status',
  'work-status',
  'write-back',
  /** `local` provider only: TCP forwarder standing in for tailcat. */
  'relay',
  /** Started by `boot`, one per box: the idle countdown and the self-stop. */
  'idle-watch',
] as const;
