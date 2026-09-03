/*
 * CDXC:AddProject 2026-07-30:
 * In-memory mocks for the add-project dialog stories. There is no mock gxserver
 * client in this repo, and the dialog is callback-driven precisely so Storybook
 * can stand in for gpui/web transports: a fixture directory tree, scripted
 * latency, and scripted failures are enough to drive every step of the flow.
 */

import type {
  AddProjectAddInput,
  AddProjectAddResult,
  AddProjectBrowseInput,
  AddProjectBrowseResult,
  AddProjectCloneJob,
  AddProjectCloneJobHandle,
  AddProjectCloneJobInput,
  AddProjectClonePreview,
  AddProjectClonePreviewInput,
  AddProjectCloneStartInput,
  AddProjectCreateDirectoryInput,
  AddProjectCreateDirectoryResult,
  AddProjectMachineOption,
  AddProjectModalCallbacks,
  AddProjectProviderDiscovery,
  AddProjectRepositoryInfo,
  AddProjectRepositoryLookupInput,
  AddProjectSourceControlDiscovery,
} from './types';

export const ADD_PROJECT_STORY_LOCAL_MACHINE: AddProjectMachineOption = {
  description: 'This computer',
  label: 'Local',
  machineId: 'local',
  platform: 'MacIntel',
};

export const ADD_PROJECT_STORY_REMOTE_MACHINE: AddProjectMachineOption = {
  addProjectBaseDirectory: '~/projects/',
  description: 'Connected remote machine',
  label: 'Bigbox',
  machineId: 'machine-bigbox',
  platform: 'Linux',
};

/**
 * Fixture filesystem. Keys are absolute directories with a trailing separator,
 * values are the directory names inside them (hidden entries included; the
 * dialog filters those the way the server would).
 */
const ADD_PROJECT_STORY_TREE: Readonly<Record<string, readonly string[]>> = {
  '/': ['Applications', 'Library', 'Users', 'Volumes', 'opt', 'srv', 'tmp'],
  '/Volumes/': ['Backup SSD', 'Macintosh HD', 'Scratch'],
  '/Users/story/': ['.config', 'Desktop', 'dev', 'Documents', 'Downloads'],
  '/Users/story/dev/': ['.cache', 'ghostex', 'ghostex-web', 'playground', 'scratch'],
  '/Users/story/dev/ghostex/': ['gpui', 'sidebar', 'shared'],
  '/Users/story/dev/playground/': ['alpha', 'beta'],
  '/srv/': ['deploy', 'projects'],
  '/srv/projects/': ['api', 'worker'],
};

const ADD_PROJECT_STORY_HOME = '/Users/story';
const ADD_PROJECT_STORY_REMOTE_HOME = '/srv';

export interface AddProjectStoryMockOptions {
  /** Milliseconds every mocked call waits before answering. Default 0. */
  readonly latencyMs?: number;
  readonly machines?: readonly AddProjectMachineOption[];
  /** Fails `addProject` with this message. */
  readonly addProjectError?: string;
  /** Fails `createDirectory` with this message. */
  readonly createDirectoryError?: string;
  /** Fails `lookupRepository` with this message. */
  readonly lookupError?: string;
  /** Fails the clone job with this message. */
  readonly cloneError?: string;
  /** How many `readCloneJob` polls report "running" before the job settles. Default 1. */
  readonly cloneRunningPolls?: number;
  readonly providers?: readonly AddProjectProviderDiscovery[];
  /** Rejects `discoverSourceControl` so every provider falls back to unavailable. */
  readonly discoveryUnavailable?: boolean;
}

export interface AddProjectStoryMocks extends AddProjectModalCallbacks {
  /** Every call the dialog made, in order, for assertions in play functions. */
  readonly calls: AddProjectStoryCall[];
}

export interface AddProjectStoryCall {
  readonly name: string;
  readonly payload: unknown;
}

export const ADD_PROJECT_STORY_READY_PROVIDERS: readonly AddProjectProviderDiscovery[] = [
  {
    auth: { detail: null, status: 'authenticated' },
    installHint: null,
    provider: 'github',
    status: 'available',
    version: '2.62.0',
  },
  {
    auth: { detail: 'GitLab CLI is not authenticated. Run glab auth login.', status: 'unauthenticated' },
    installHint: null,
    provider: 'gitlab',
    status: 'available',
    version: '1.48.0',
  },
  {
    installHint: 'Bitbucket support needs a CLI Ghostex does not ship yet.',
    provider: 'bitbucket',
    status: 'missing',
  },
  {
    installHint: 'Azure DevOps support needs a CLI Ghostex does not ship yet.',
    provider: 'azure-devops',
    status: 'missing',
  },
];

export function createAddProjectStoryMocks(options: AddProjectStoryMockOptions = {}): AddProjectStoryMocks {
  const calls: AddProjectStoryCall[] = [];
  const latencyMs = options.latencyMs ?? 0;
  const machines = options.machines ?? [ADD_PROJECT_STORY_LOCAL_MACHINE];
  const providers = options.providers ?? ADD_PROJECT_STORY_READY_PROVIDERS;
  const cloneRunningPolls = options.cloneRunningPolls ?? 1;
  const cloneJobPolls = new Map<string, number>();
  const cloneJobDestinations = new Map<string, string>();
  const cancelledCloneJobs = new Set<string>();
  /*
   * CDXC:AddProject 2026-08-18:
   * A created folder has to be browsable right after it is made, because the
   * dialog steps into it. The fixture tree is immutable, so the mock keeps an
   * overlay of created children per parent and layers it over every browse.
   */
  const createdChildren = new Map<string, string[]>();

  function browseWithCreatedDirectories(input: AddProjectBrowseInput): AddProjectBrowseResult | null {
    const base = browseStoryTree(input);
    const parentPath = base
      ? base.parentPath
      : trimStoryTrailingSeparator(
          expandStoryHome(
            input.partialPath,
            input.machineId === ADD_PROJECT_STORY_REMOTE_MACHINE.machineId
              ? ADD_PROJECT_STORY_REMOTE_HOME
              : ADD_PROJECT_STORY_HOME
          )
        );
    const created = createdChildren.get(parentPath);
    if (!created) {
      return base;
    }
    const entries = [...(base?.entries ?? [])];
    for (const name of created) {
      if (entries.some((entry) => entry.name === name)) {
        continue;
      }
      entries.push({ fullPath: `${parentPath}/${name}`, name });
    }
    entries.sort((left, right) => left.name.localeCompare(right.name));
    return { entries, parentPath };
  }

  function record(name: string, payload: unknown): void {
    calls.push({ name, payload });
  }

  async function settle<T>(value: T): Promise<T> {
    if (latencyMs > 0) {
      await new Promise((resolve) => {
        setTimeout(resolve, latencyMs);
      });
    }
    return value;
  }

  return {
    calls,
    addProject: async (input: AddProjectAddInput): Promise<AddProjectAddResult> => {
      record('addProject', input);
      await settle(null);
      if (options.addProjectError) {
        throw new Error(options.addProjectError);
      }
      return {
        machineId: input.machineId,
        path: input.path,
        projectId: `project-${input.path.replace(/[^\w-]+/gu, '-')}`,
      };
    },
    browse: async (input: AddProjectBrowseInput): Promise<AddProjectBrowseResult | null> => {
      record('browse', input);
      await settle(null);
      return browseWithCreatedDirectories(input);
    },
    cancelCloneJob: async (input: AddProjectCloneJobInput): Promise<void> => {
      record('cancelCloneJob', input);
      cancelledCloneJobs.add(input.jobId);
      await settle(null);
    },
    createDirectory: async (input: AddProjectCreateDirectoryInput): Promise<AddProjectCreateDirectoryResult> => {
      record('createDirectory', input);
      await settle(null);
      if (options.createDirectoryError) {
        throw new Error(options.createDirectoryError);
      }
      const parentPath = trimStoryTrailingSeparator(input.parentPath);
      const path = `${parentPath}/${input.name}`;
      createdChildren.set(parentPath, [...(createdChildren.get(parentPath) ?? []), input.name]);
      /* The new folder itself is browsable and empty. */
      createdChildren.set(path, createdChildren.get(path) ?? []);
      return { name: input.name, parentPath, path };
    },
    discoverSourceControl: async (input: {
      readonly machineId: string;
    }): Promise<AddProjectSourceControlDiscovery | null> => {
      record('discoverSourceControl', input);
      await settle(null);
      if (options.discoveryUnavailable) {
        return null;
      }
      return { providers };
    },
    listMachineOptions: async (): Promise<readonly AddProjectMachineOption[]> => {
      record('listMachineOptions', null);
      return settle(machines);
    },
    lookupRepository: async (input: AddProjectRepositoryLookupInput): Promise<AddProjectRepositoryInfo> => {
      record('lookupRepository', input);
      await settle(null);
      if (options.lookupError) {
        throw new Error(options.lookupError);
      }
      return {
        nameWithOwner: input.repository,
        provider: input.provider,
        sshUrl: `git@${input.provider}.com:${input.repository}.git`,
        url: `https://${input.provider}.com/${input.repository}`,
      };
    },
    previewClone: async (input: AddProjectClonePreviewInput): Promise<AddProjectClonePreview> => {
      record('previewClone', input);
      await settle(null);
      const normalizedDestination = input.destinationPath.replace(/\/+$/u, '');
      const separatorIndex = normalizedDestination.lastIndexOf('/');
      return {
        ...(input.branchName ? { branchName: input.branchName } : {}),
        cloneMainOnly: input.cloneMainOnly,
        cloneUrl: input.remoteUrl,
        destinationBlocked: false,
        destinationExists: false,
        destinationFolderName: normalizedDestination.slice(separatorIndex + 1),
        destinationPath: normalizedDestination,
        parentPath: normalizedDestination.slice(0, separatorIndex) || '/',
        repositoryName: normalizedDestination.slice(separatorIndex + 1),
        shallowClone: input.shallowClone,
      };
    },
    readCloneJob: async (input: AddProjectCloneJobInput): Promise<AddProjectCloneJob> => {
      record('readCloneJob', input);
      await settle(null);
      if (cancelledCloneJobs.has(input.jobId)) {
        return { jobId: input.jobId, message: 'Clone canceled', state: 'canceled' };
      }
      const polls = (cloneJobPolls.get(input.jobId) ?? 0) + 1;
      cloneJobPolls.set(input.jobId, polls);
      if (polls <= cloneRunningPolls) {
        return { jobId: input.jobId, message: 'Receiving objects', state: 'running' };
      }
      if (options.cloneError) {
        return {
          error: options.cloneError,
          jobId: input.jobId,
          message: options.cloneError,
          state: 'failed',
        };
      }
      return {
        jobId: input.jobId,
        message: 'Clone completed',
        projectPath: cloneJobDestinations.get(input.jobId) ?? '',
        state: 'completed',
      };
    },
    startClone: async (input: AddProjectCloneStartInput): Promise<AddProjectCloneJobHandle> => {
      record('startClone', input);
      await settle(null);
      const jobId = `clone-job-${cloneJobDestinations.size + 1}`;
      /* gxserver clones INTO destinationPath and registers that directory (spec §6.5). */
      cloneJobDestinations.set(jobId, input.destinationPath);
      return { jobId };
    },
  };
}

/** Mirrors gxserver's browse semantics: directories only, prefix match, `~` expanded server-side. */
export function browseStoryTree(input: AddProjectBrowseInput): AddProjectBrowseResult | null {
  const home =
    input.machineId === ADD_PROJECT_STORY_REMOTE_MACHINE.machineId
      ? ADD_PROJECT_STORY_REMOTE_HOME
      : ADD_PROJECT_STORY_HOME;
  const expanded = expandStoryHome(input.partialPath, home);
  const endsWithSeparator = expanded.endsWith('/');
  const parentPath = endsWithSeparator ? expanded : `${expanded.slice(0, expanded.lastIndexOf('/') + 1)}`;
  const prefix = endsWithSeparator ? '' : expanded.slice(expanded.lastIndexOf('/') + 1);
  const entries = ADD_PROJECT_STORY_TREE[parentPath];
  if (!entries) {
    return null;
  }
  const showHidden = endsWithSeparator || prefix.startsWith('.');
  return {
    entries: entries
      .filter((name) => name.toLowerCase().startsWith(prefix.toLowerCase()))
      .filter((name) => showHidden || !name.startsWith('.'))
      .map((name) => ({ fullPath: `${parentPath}${name}`, name })),
    parentPath: trimStoryTrailingSeparator(parentPath),
  };
}

function expandStoryHome(value: string, home: string): string {
  if (value === '~') {
    return `${home}/`;
  }
  if (value.startsWith('~/')) {
    return `${home}/${value.slice(2)}`;
  }
  return value;
}

function trimStoryTrailingSeparator(value: string): string {
  return value.length > 1 && value.endsWith('/') ? value.slice(0, -1) : value;
}
