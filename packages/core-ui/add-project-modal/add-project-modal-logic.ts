/*
 * CDXC:AddProject 2026-07-30:
 * Pure presentation logic for the add-project dialog's
 * CommandPalette (labels, path hints, provider readiness, source ordering,
 * placeholders, empty-state copy, submit-button label). Keeping it out of the
 * component keeps the React file about state transitions and lets Storybook /
 * unit tests assert the copy directly.
 */

import type {
  AddProjectMachineOption,
  AddProjectProviderId,
  AddProjectSourceControlDiscovery,
  AddProjectSourceId,
  AddProjectSourceReadiness,
} from "./types";

export const ADD_PROJECT_PROVIDER_SOURCES: readonly AddProjectProviderId[] = [
  "github",
  "gitlab",
  "bitbucket",
  "azure-devops",
];

export const ADD_PROJECT_SOURCES: readonly AddProjectSourceId[] = [
  "url",
  ...ADD_PROJECT_PROVIDER_SOURCES,
];

const ADD_PROJECT_SOURCE_LABELS: Record<AddProjectSourceId, string> = {
  "azure-devops": "Azure DevOps",
  bitbucket: "Bitbucket",
  github: "GitHub",
  gitlab: "GitLab",
  url: "Git URL",
};

const ADD_PROJECT_SOURCE_PATH_HINTS: Record<AddProjectSourceId, string> = {
  "azure-devops": "project/repository",
  bitbucket: "workspace/repository",
  github: "owner/repo",
  gitlab: "group/project",
  url: "URL",
};

const ADD_PROJECT_PROVIDER_UNAVAILABLE_HINT =
  "Provider status unavailable. Open Settings -> Source Control and rescan.";

export const ADD_PROJECT_DEFAULT_BROWSE_PATH = "~/";

/*
 * CDXC:AddProjectRootBrowse 2026-08-19:
 * Home is the default starting point, but external volumes and system folders
 * live outside it (`/Volumes` on macOS, `/mnt` and `/media` on Linux), and the
 * browser can only walk down from wherever it starts. The root entry gives that
 * whole half of the filesystem a starting point instead of making the user type
 * the path. On Windows machines the daemon resolves `/` to the current drive
 * root, which is the same "everything else" location.
 */
export const ADD_PROJECT_ROOT_BROWSE_PATH = "/";

export function addProjectSourceLabel(source: AddProjectSourceId): string {
  return ADD_PROJECT_SOURCE_LABELS[source];
}

export function addProjectSourcePathHint(source: AddProjectSourceId): string {
  return ADD_PROJECT_SOURCE_PATH_HINTS[source];
}

export function addProjectSourceRowTitle(source: AddProjectSourceId): string {
  return source === "url" ? "Git URL" : `${addProjectSourceLabel(source)} repository`;
}

export function addProjectSourceRowDescription(source: AddProjectSourceId): string {
  return source === "url"
    ? "Clone from a remote URL"
    : `Clone ${addProjectSourceLabel(source)} ${addProjectSourcePathHint(source)}`;
}

/**
 * Remote source readiness: `url` is always ready, every
 * provider defaults to unavailable, `auth.status === "unknown"` still counts as
 * ready (the CLI may simply not report auth state).
 */
export function buildAddProjectSourceReadiness(
  discovery: AddProjectSourceControlDiscovery | null | undefined,
): Record<AddProjectSourceId, AddProjectSourceReadiness> {
  const unavailable: AddProjectSourceReadiness = {
    hint: ADD_PROJECT_PROVIDER_UNAVAILABLE_HINT,
    ready: false,
  };
  const readiness: Record<AddProjectSourceId, AddProjectSourceReadiness> = {
    "azure-devops": unavailable,
    bitbucket: unavailable,
    github: unavailable,
    gitlab: unavailable,
    url: { hint: null, ready: true },
  };

  if (!discovery) {
    return readiness;
  }

  for (const source of ADD_PROJECT_PROVIDER_SOURCES) {
    const provider = discovery.providers.find((entry) => entry.provider === source);
    if (!provider) {
      continue;
    }
    if (provider.status !== "available") {
      readiness[source] = {
        hint: provider.installHint?.trim() || ADD_PROJECT_PROVIDER_UNAVAILABLE_HINT,
        ready: false,
      };
      continue;
    }
    if (provider.auth?.status === "unauthenticated") {
      readiness[source] = {
        hint:
          provider.auth.detail?.trim() ||
          `${addProjectSourceLabel(source)} is not authenticated. Open Settings -> Source Control for setup guidance.`,
        ready: false,
      };
      continue;
    }
    readiness[source] = { hint: null, ready: true };
  }

  return readiness;
}

/** Ready providers first, then alphabetical by label within each bucket. */
export function sortAddProjectProviderSources(
  readiness: Record<AddProjectSourceId, AddProjectSourceReadiness>,
): AddProjectProviderId[] {
  return [...ADD_PROJECT_PROVIDER_SOURCES].sort((left, right) => {
    const leftReady = readiness[left].ready;
    const rightReady = readiness[right].ready;
    if (leftReady !== rightReady) {
      return leftReady ? -1 : 1;
    }
    return addProjectSourceLabel(left).localeCompare(addProjectSourceLabel(right));
  });
}

export function orderedAddProjectSources(
  readiness: Record<AddProjectSourceId, AddProjectSourceReadiness>,
): AddProjectSourceId[] {
  return ["url", ...sortAddProjectProviderSources(readiness)];
}

export function addProjectRepositoryPlaceholder(source: AddProjectSourceId): string {
  if (source === "url") {
    return "Enter repository, URL, or clone command";
  }
  if (source === "github") {
    return "Enter GitHub repository, URL, or clone command";
  }
  return `Enter ${addProjectSourceLabel(source)} repository (${addProjectSourcePathHint(source)})`;
}

export function addProjectRepositoryActionLabel(source: AddProjectSourceId): string {
  return source === "url" ? "Continue" : "Lookup";
}

export function addProjectPathPlaceholder(isSubmenu: boolean): string {
  return isSubmenu
    ? "Enter path (e.g. ~/projects/my-app)"
    : "Enter project path (e.g. ~/projects/my-app)";
}

export function addProjectInitialBrowseQuery(machine: AddProjectMachineOption | null): string {
  const baseDirectory = machine?.addProjectBaseDirectory?.trim() ?? "";
  return baseDirectory.length === 0 ? ADD_PROJECT_DEFAULT_BROWSE_PATH : baseDirectory;
}

export interface AddProjectSubmitLabelInput {
  readonly isCloneDestinationStep: boolean;
  readonly willCreateProjectPath: boolean;
}

export function addProjectSubmitActionLabel(input: AddProjectSubmitLabelInput): string {
  if (input.isCloneDestinationStep) {
    return input.willCreateProjectPath ? "Create & Clone" : "Clone";
  }
  return input.willCreateProjectPath ? "Create & Add" : "Add";
}

export interface AddProjectEmptyStateInput {
  readonly cloneStep: "destination" | "repository" | null;
  readonly cloneSource: AddProjectSourceId | null;
  readonly isLoadingMachines: boolean;
  readonly hasMachines: boolean;
  readonly relativePathNeedsActiveProject: boolean;
  readonly unsupportedWindowsPath: boolean;
  readonly willCreateProjectPath: boolean;
}

export function addProjectEmptyStateMessage(input: AddProjectEmptyStateInput): string {
  if (input.isLoadingMachines) {
    return "Loading machines...";
  }
  if (!input.hasMachines) {
    return "No machine is available.";
  }
  if (input.cloneStep === "repository") {
    if (input.cloneSource === "url") {
      return "Enter a repository, URL, or clone command and press Enter to continue.";
    }
    if (input.cloneSource === "github") {
      return "Enter owner/repo, a GitHub URL, or a clone command and press Enter to continue.";
    }
    return "Enter a repository path and press Enter to look it up.";
  }
  if (input.cloneStep === "destination" && input.willCreateProjectPath) {
    return "Press Enter to review this new clone destination.";
  }
  if (input.cloneStep === "destination") {
    return "Choose a destination path and press Enter to review the clone.";
  }
  if (input.unsupportedWindowsPath) {
    return "Windows-style paths are only supported on Windows machines.";
  }
  if (input.relativePathNeedsActiveProject) {
    return "Relative paths require an active project.";
  }
  if (input.willCreateProjectPath) {
    return "Press Enter to create this folder and add it as a project.";
  }
  return "No matching directories.";
}

/*
 * CDXC:AddProjectNewFolder 2026-08-18:
 * The new-folder step reuses the dialog's single input, so the list area is
 * where the pending folder is spelled out in full. It names the parent the
 * folder lands in, because the typed name alone never shows that.
 */
export function addProjectNewFolderMessage(input: {
  readonly name: string;
  readonly parentPath: string;
}): string {
  const name = input.name.trim();
  if (name.length === 0) {
    return `Name the new folder to create in ${input.parentPath}.`;
  }
  if (/[/\\]/u.test(name)) {
    return "A folder name cannot contain a path separator.";
  }
  return `Press Enter to create ${input.parentPath.replace(/[/\\]+$/u, "")}/${name}.`;
}

export function isPrimaryModifierPlatform(platform: string): boolean {
  return /mac/iu.test(platform);
}

export function addProjectModifierLabel(platform: string): string {
  return isPrimaryModifierPlatform(platform) ? "⌘" : "Ctrl";
}

/** Case-insensitive substring match over a row's title plus its search terms. */
export function matchesAddProjectFilter(
  query: string,
  title: string,
  searchTerms: readonly string[] = [],
): boolean {
  const normalized = query.trim().toLowerCase();
  if (normalized.length === 0) {
    return true;
  }
  return [title, ...searchTerms].some((term) => term.toLowerCase().includes(normalized));
}
