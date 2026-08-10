/*
 * CDXC:AddProject 2026-07-30:
 * The shared add-project dialog is transport-free: every server round trip is a
 * prop. gpui fulfils these callbacks with requestId bridge messages routed by
 * machineId in Rust, ghostex-web fulfils them with rpcForMachine, and Storybook
 * fulfils them with in-memory mocks. Nothing in this file may carry hosts,
 * users, tokens, or SSH details: the dialog only ever sees a bounded machineId
 * plus a human display label, so CEF surfaces stay free of remote credentials.
 *
 * Field names deliberately mirror the gxserver wire shapes (browse
 * {partialPath, cwd?} -> {parentPath, entries:[{name, fullPath}]}) so Part C/D
 * adapters are pure plumbing rather than translation layers.
 */

/** A machine the dialog can add a project on. */
export interface AddProjectMachineOption {
  /**
   * Optional per-machine "Add project starts in" directory. Empty/omitted means
   * the browser opens at `~/`.
   */
  readonly addProjectBaseDirectory?: string;
  /** Secondary line in the machine row. Display copy only, never a host. */
  readonly description?: string;
  /** Human display name, e.g. "This Mac" or a saved machine label. */
  readonly label: string;
  /** Bounded id. Safe for logs. */
  readonly machineId: string;
  /** navigator.platform-style string for the machine's OS ("MacIntel" | "Linux" | "Win32"). */
  readonly platform?: string;
}

export interface AddProjectBrowseInput {
  /** Absolute cwd used to resolve `./` and `../` queries. Omitted when unknown. */
  readonly cwd?: string;
  readonly machineId: string;
  /** The directory portion of the typed query, e.g. "~/dev/". */
  readonly partialPath: string;
}

export interface AddProjectBrowseEntry {
  readonly fullPath: string;
  readonly name: string;
}

export interface AddProjectBrowseResult {
  readonly entries: readonly AddProjectBrowseEntry[];
  /** Server-resolved absolute directory (`~` expanded, path.resolve'd). */
  readonly parentPath: string;
}

export interface AddProjectAddInput {
  /** True when the dialog decided the folder does not exist yet ("Create & Add"). */
  readonly createIfMissing: boolean;
  readonly machineId: string;
  readonly path: string;
}

export interface AddProjectAddResult {
  /** True when the machine already had this project registered. */
  readonly alreadyExists?: boolean;
  readonly machineId: string;
  /** Server-normalized absolute project path. */
  readonly path: string;
  readonly projectId?: string;
}

export type AddProjectProviderId = "azure-devops" | "bitbucket" | "github" | "gitlab";
export type AddProjectSourceId = AddProjectProviderId | "url";

export type AddProjectProviderAuthStatus = "authenticated" | "unauthenticated" | "unknown";
/**
 * Mirrors gxserver's `GxserverSourceControlDiscoveryStatus` (Part A) plus a
 * generic `error`: `missing` means the provider CLI is not installed on that
 * machine, `unsupported` means gxserver has no implementation for the provider
 * at all (Bitbucket / Azure DevOps today).
 */
export type AddProjectProviderStatus = "available" | "error" | "missing" | "unsupported";

export interface AddProjectProviderDiscovery {
  readonly auth?: {
    /** Human explanation shown in the Setup Required tooltip. */
    readonly detail?: string | null;
    readonly status: AddProjectProviderAuthStatus;
  };
  /** e.g. "Install the GitHub CLI (gh) to clone GitHub repositories." */
  readonly installHint?: string | null;
  readonly provider: AddProjectProviderId;
  readonly status: AddProjectProviderStatus;
  readonly version?: string | null;
}

export interface AddProjectSourceControlDiscovery {
  readonly providers: readonly AddProjectProviderDiscovery[];
}

export interface AddProjectSourceReadiness {
  readonly hint: string | null;
  readonly ready: boolean;
}

export interface AddProjectRepositoryLookupInput {
  readonly machineId: string;
  readonly provider: AddProjectProviderId;
  readonly repository: string;
}

export interface AddProjectRepositoryInfo {
  readonly nameWithOwner: string;
  readonly provider: AddProjectProviderId;
  readonly sshUrl: string;
  readonly url: string;
}

export interface AddProjectCloneStartInput {
  readonly destinationPath: string;
  readonly machineId: string;
  readonly remoteUrl: string;
}

export interface AddProjectCloneJobHandle {
  readonly jobId: string;
}

export interface AddProjectCloneJobInput {
  readonly jobId: string;
  readonly machineId: string;
}

/** Mirrors gxserver's `GxserverRepositoryCloneJobState` (Part A) verbatim. */
export type AddProjectCloneJobState = "canceled" | "completed" | "failed" | "running";

/**
 * A projection of gxserver's `GxserverRepositoryCloneJobStatus`: field names are
 * kept identical so a host adapter can forward the job record unchanged.
 */
export interface AddProjectCloneJob {
  readonly error?: string | null;
  readonly jobId: string;
  /** Short human status line. gxserver always sets it; it is optional here. */
  readonly message?: string | null;
  /** Set when `state === "completed"`: the cloned working directory to register. */
  readonly projectPath?: string | null;
  readonly state: AddProjectCloneJobState;
}

/** Every server round trip the dialog performs. */
export interface AddProjectModalCallbacks {
  readonly addProject: (input: AddProjectAddInput) => Promise<AddProjectAddResult>;
  readonly browse: (input: AddProjectBrowseInput) => Promise<AddProjectBrowseResult | null>;
  readonly cancelCloneJob?: (input: AddProjectCloneJobInput) => Promise<void>;
  readonly discoverSourceControl: (input: {
    readonly machineId: string;
  }) => Promise<AddProjectSourceControlDiscovery | null>;
  readonly listMachineOptions: () => Promise<readonly AddProjectMachineOption[]>;
  readonly lookupRepository: (
    input: AddProjectRepositoryLookupInput,
  ) => Promise<AddProjectRepositoryInfo>;
  readonly readCloneJob: (input: AddProjectCloneJobInput) => Promise<AddProjectCloneJob>;
  readonly startClone: (input: AddProjectCloneStartInput) => Promise<AddProjectCloneJobHandle>;
}

export interface AddProjectModalProps extends AddProjectModalCallbacks {
  /** Absolute cwd of the active project, used to resolve `./` / `../` queries. */
  readonly activeProjectCwd?: string | null;
  /** Poll interval for `readCloneJob`. Default 900ms. */
  readonly cloneJobPollIntervalMs?: number;
  /** Preselects a machine and skips the machine step. */
  readonly initialMachineId?: string;
  readonly isOpen: boolean;
  readonly onClose: () => void;
  /** Fired after a project was registered; the dialog closes right after. */
  readonly onProjectAdded?: (result: AddProjectAddResult) => void;
  /** "Setup Required" affordance on a not-ready provider row. */
  readonly onOpenSourceControlSettings?: (provider: AddProjectProviderId) => void;
  /** navigator.platform fallback when a machine option omits `platform`. */
  readonly platform?: string;
  /** How long a pending server call may run before the "still working" notice. Default 8000ms. */
  readonly slowOperationNoticeMs?: number;
}
