import type { SidebarSessionGroup } from './session-grid-contract-sidebar';

/*
CDXC:SidebarV2 2026-07-29-00:00:
Logical project and sidebar grouping helpers.
`packages/shared/src/git.ts` (`normalizeGitRemoteUrl`) and
`packages/shared/src/path.ts` (`normalizeProjectPathForComparison`).

Cross-machine logical projects: the same repository checked out on this Mac and
on a remote machine should read as ONE project in the V2 inbox. The grouping key
is the normalized git remote URL, probed server-side by gxserver (P5) and
shipped in the presentation snapshot. Non-git projects never merge — they fall
back to their physical machine+path key.

The upstream environment identity maps to Ghostex's `machineId` (project ids are already
machine-scoped in gpui), and `workspaceRoot` maps to the project path.
Consumed in P5; the key derivation lands now so the settings shape and the
gxserver probe can be built against it.
*/

/** Per-project override for how aggressively checkouts merge. */
export type SidebarV2ProjectGroupingMode = 'repository' | 'repositoryPath' | 'separate';

export type SidebarV2SourceControlProvider = 'bitbucket' | 'github' | 'gitlab';

export type SidebarV2RepositoryIdentity = {
  /** Normalized remote URL, e.g. `github.com/acme/example`. */
  canonicalKey: string;
  /** `owner/repo` when derivable. */
  displayName?: string;
  name?: string;
  owner?: string;
  provider?: SidebarV2SourceControlProvider;
  /** Repository root as reported by `git rev-parse --show-toplevel`. */
  rootPath?: string;
};

export type SidebarV2Project = {
  machineId?: string;
  machineName?: string;
  path?: string;
  projectId: string;
  repository?: SidebarV2RepositoryIdentity;
  title: string;
};

export type SidebarV2ProjectGroupingSettings = {
  sidebarV2ProjectGroupingMode: SidebarV2ProjectGroupingMode;
  sidebarV2ProjectGroupingOverrides: Readonly<Record<string, SidebarV2ProjectGroupingMode>>;
};

export const DEFAULT_SIDEBAR_V2_PROJECT_GROUPING_SETTINGS: SidebarV2ProjectGroupingSettings = {
  sidebarV2ProjectGroupingMode: 'repository',
  sidebarV2ProjectGroupingOverrides: {},
};

export const SIDEBAR_V2_LOCAL_MACHINE_ID = 'local';

function isWindowsDrivePath(value: string): boolean {
  return /^[a-zA-Z]:([/\\]|$)/.test(value);
}

function isUncPath(value: string): boolean {
  return value.startsWith('\\\\');
}

function isRootPath(value: string): boolean {
  return value === '/' || value === '\\' || /^[a-zA-Z]:[/\\]?$/.test(value);
}

function trimTrailingPathSeparators(value: string): string {
  if (value.length === 0 || isRootPath(value)) {
    return value;
  }
  const trimmed = value.startsWith('/') ? value.replace(/\/+$/g, '') : value.replace(/[\\/]+$/g, '');
  if (trimmed.length === 0) {
    return value;
  }
  return /^[a-zA-Z]:$/.test(trimmed) ? `${trimmed}\\` : trimmed;
}

/** Case-folds and separator-folds Windows paths only: POSIX paths are
    case-sensitive, so lowercasing them would merge distinct checkouts. */
export function normalizeProjectPathForComparison(value: string): string {
  const normalized = trimTrailingPathSeparators(value.trim());
  if (isWindowsDrivePath(normalized) || isUncPath(normalized)) {
    return normalized.replaceAll('/', '\\').toLowerCase();
  }
  return normalized;
}

/**
 * Collapses every shape of git remote URL onto one comparable key:
 * `host/owner/repo`, lowercased, without a `.git` suffix or trailing slash.
 * Handles `https://`, `ssh://`, `git://` and scp-style `git@host:owner/repo`.
 */
export function normalizeGitRemoteUrl(value: string): string {
  const normalized = value
    .trim()
    .replace(/\/+$/g, '')
    .replace(/\.git$/i, '')
    .toLowerCase();

  if (/^(?:ssh|https?|git):\/\//i.test(normalized)) {
    try {
      const url = new URL(normalized);
      const repositoryPath = url.pathname
        .split('/')
        .filter((segment) => segment.length > 0)
        .join('/');
      if (url.hostname && repositoryPath.includes('/')) {
        return `${url.hostname}/${repositoryPath}`;
      }
    } catch {
      return normalized;
    }
    return normalized;
  }

  const scpStyleHostAndPath = /^git@([^:/\s]+)[:/]([^/\s]+(?:\/[^/\s]+)+)$/i.exec(normalized);
  if (scpStyleHostAndPath?.[1] && scpStyleHostAndPath[2]) {
    return `${scpStyleHostAndPath[1]}/${scpStyleHostAndPath[2]}`;
  }

  return normalized;
}

function detectSourceControlProvider(canonicalKey: string): SidebarV2SourceControlProvider | undefined {
  const host = canonicalKey.split('/')[0] ?? '';
  if (host.includes('github')) {
    return 'github';
  }
  if (host.includes('gitlab')) {
    return 'gitlab';
  }
  if (host.includes('bitbucket')) {
    return 'bitbucket';
  }
  return undefined;
}

/**
 * Builds the repository identity gxserver ships in the presentation snapshot.
 * Returns null for a project with no usable remote so non-git projects can
 * never accidentally share a logical key.
 */
export function deriveSidebarV2RepositoryIdentity(input: {
  remoteUrl: string | null | undefined;
  rootPath?: string;
}): SidebarV2RepositoryIdentity | null {
  const remoteUrl = input.remoteUrl?.trim();
  if (!remoteUrl) {
    return null;
  }
  const canonicalKey = normalizeGitRemoteUrl(remoteUrl);
  if (canonicalKey.length === 0) {
    return null;
  }

  const repositoryPath = canonicalKey.split('/').slice(1).join('/');
  const segments = repositoryPath.split('/').filter((segment) => segment.length > 0);
  const provider = detectSourceControlProvider(canonicalKey);
  const owner = segments[0];
  const name = segments.at(-1);

  return {
    canonicalKey,
    ...(repositoryPath ? { displayName: repositoryPath } : {}),
    ...(name ? { name } : {}),
    ...(owner ? { owner } : {}),
    ...(provider ? { provider } : {}),
    ...(input.rootPath ? { rootPath: input.rootPath } : {}),
  };
}

/** Identity of one physical checkout: machine + path. Never merges. */
export function deriveSidebarV2PhysicalProjectKey(
  project: Pick<SidebarV2Project, 'machineId' | 'path' | 'projectId'>
): string {
  const machineId = project.machineId?.trim() || SIDEBAR_V2_LOCAL_MACHINE_ID;
  const path = project.path?.trim();
  return path ? `${machineId}:${normalizeProjectPathForComparison(path)}` : `${machineId}:#${project.projectId}`;
}

/** Grouping overrides are keyed by the physical checkout, so overriding one
    machine's copy never silently rewrites another machine's. */
export function deriveSidebarV2ProjectGroupingOverrideKey(
  project: Pick<SidebarV2Project, 'machineId' | 'path' | 'projectId'>
): string {
  return deriveSidebarV2PhysicalProjectKey(project);
}

export function resolveSidebarV2ProjectGroupingMode(
  project: Pick<SidebarV2Project, 'machineId' | 'path' | 'projectId'>,
  settings: SidebarV2ProjectGroupingSettings
): SidebarV2ProjectGroupingMode {
  return (
    settings.sidebarV2ProjectGroupingOverrides?.[deriveSidebarV2ProjectGroupingOverrideKey(project)] ??
    settings.sidebarV2ProjectGroupingMode
  );
}

/** Path of the project relative to its repository root, or null when the
    project is not inside the reported root. */
function deriveRepositoryRelativePath(project: SidebarV2Project): string | null {
  const rootPath = project.repository?.rootPath?.trim();
  const projectPath = project.path?.trim();
  if (!rootPath || !projectPath) {
    return null;
  }

  const normalizedRootPath = normalizeProjectPathForComparison(rootPath);
  const normalizedProjectPath = normalizeProjectPathForComparison(projectPath);
  if (normalizedRootPath.length === 0 || normalizedProjectPath.length === 0) {
    return null;
  }
  if (normalizedRootPath === normalizedProjectPath) {
    return '';
  }

  const separator = normalizedRootPath.includes('\\') ? '\\' : '/';
  const rootPrefix = `${normalizedRootPath}${separator}`;
  if (!normalizedProjectPath.startsWith(rootPrefix)) {
    return null;
  }
  return normalizedProjectPath.slice(rootPrefix.length).replaceAll('\\', '/');
}

function deriveRepositoryScopedKey(
  project: SidebarV2Project,
  groupingMode: SidebarV2ProjectGroupingMode
): string | null {
  const canonicalKey = project.repository?.canonicalKey?.trim();
  if (!canonicalKey) {
    return null;
  }
  if (groupingMode === 'repository') {
    return canonicalKey;
  }

  const relativePath = deriveRepositoryRelativePath(project);
  if (relativePath === null) {
    return canonicalKey;
  }
  return relativePath.length === 0 ? canonicalKey : `${canonicalKey}::${relativePath}`;
}

/**
 * The key sessions group by in V2. `repository` merges every checkout of the
 * repo (including worktrees, per decision 4); `repositoryPath` keeps distinct
 * sub-paths apart but still merges the same sub-path across machines;
 * `separate` never merges.
 */
export function deriveSidebarV2LogicalProjectKey(
  project: SidebarV2Project,
  options: { groupingMode?: SidebarV2ProjectGroupingMode } = {}
): string {
  const groupingMode = options.groupingMode ?? 'repository';
  if (groupingMode === 'separate') {
    return deriveSidebarV2PhysicalProjectKey(project);
  }
  return deriveRepositoryScopedKey(project, groupingMode) ?? deriveSidebarV2PhysicalProjectKey(project);
}

export function deriveSidebarV2LogicalProjectKeyFromSettings(
  project: SidebarV2Project,
  settings: SidebarV2ProjectGroupingSettings
): string {
  return deriveSidebarV2LogicalProjectKey(project, {
    groupingMode: resolveSidebarV2ProjectGroupingMode(project, settings),
  });
}

function uniqueNonEmptyValues(values: readonly (string | null | undefined)[]): string[] {
  const seen = new Set<string>();
  const unique: string[] = [];
  for (const value of values) {
    const trimmed = value?.trim();
    if (!trimmed || seen.has(trimmed)) {
      continue;
    }
    seen.add(trimmed);
    unique.push(trimmed);
  }
  return unique;
}

/** A merged group shows the shared repository name rather than whichever
    member happened to be picked as representative. */
export function deriveSidebarV2ProjectGroupLabel(input: {
  members: readonly SidebarV2Project[];
  representative: SidebarV2Project;
}): string {
  const sharedDisplayNames = uniqueNonEmptyValues(input.members.map((member) => member.repository?.displayName));
  if (sharedDisplayNames.length === 1) {
    return sharedDisplayNames[0]!;
  }
  const sharedNames = uniqueNonEmptyValues(input.members.map((member) => member.repository?.name));
  if (sharedNames.length === 1) {
    return sharedNames[0]!;
  }
  return input.representative.title;
}

export type SidebarV2MachinePresence = 'local-only' | 'mixed' | 'remote-only';

export type SidebarV2LogicalProjectGroup = {
  displayName: string;
  machinePresence: SidebarV2MachinePresence;
  members: SidebarV2Project[];
  projectKey: string;
  remoteMachineNames: string[];
  representative: SidebarV2Project;
};

/**
 * Groups projects by logical key, preserving input order for both the groups
 * and the members inside them. The representative is the local member when one
 * exists, so single-machine setups are unaffected by the merge.
 */
export function groupSidebarV2ProjectsByLogicalKey(input: {
  primaryMachineId?: string | null;
  projects: readonly SidebarV2Project[];
  settings?: SidebarV2ProjectGroupingSettings;
}): SidebarV2LogicalProjectGroup[] {
  const settings = input.settings ?? DEFAULT_SIDEBAR_V2_PROJECT_GROUPING_SETTINGS;
  const primaryMachineId = input.primaryMachineId?.trim() || SIDEBAR_V2_LOCAL_MACHINE_ID;
  const membersByKey = new Map<string, SidebarV2Project[]>();
  const keyOrder: string[] = [];

  for (const project of input.projects) {
    const projectKey = deriveSidebarV2LogicalProjectKeyFromSettings(project, settings);
    const existing = membersByKey.get(projectKey);
    if (existing) {
      existing.push(project);
    } else {
      membersByKey.set(projectKey, [project]);
      keyOrder.push(projectKey);
    }
  }

  const isLocal = (project: SidebarV2Project) =>
    (project.machineId?.trim() || SIDEBAR_V2_LOCAL_MACHINE_ID) === primaryMachineId;

  return keyOrder.flatMap((projectKey) => {
    const members = membersByKey.get(projectKey) ?? [];
    const representative = members.find(isLocal) ?? members[0];
    if (!representative) {
      return [];
    }
    const hasLocal = members.some(isLocal);
    const remoteMembers = members.filter((member) => !isLocal(member));
    return [
      {
        displayName:
          members.length > 1 ? deriveSidebarV2ProjectGroupLabel({ members, representative }) : representative.title,
        machinePresence:
          hasLocal && remoteMembers.length > 0
            ? ('mixed' as const)
            : remoteMembers.length > 0
              ? ('remote-only' as const)
              : ('local-only' as const),
        members,
        projectKey,
        remoteMachineNames: uniqueNonEmptyValues(remoteMembers.map((member) => member.machineName)),
        representative,
      },
    ];
  });
}

/**
 * Adapter from the sidebar contract's project group.
 *
 * CDXC:SidebarV2LogicalProjects 2026-07-29:
 * The repository identity is derived from `projectContext.gitRemoteOriginUrl`,
 * which gxserver probes per project and the projection carries verbatim. A
 * caller may still pass an explicit identity (tests, fixtures); passing `null`
 * forces the physical key, which is how a non-git project is expressed.
 *
 * The Quick/Chats collection has no `projectContext` at all, so it can never
 * acquire a repository identity and therefore never merges — no special case
 * is needed for it anywhere downstream.
 *
 * CDXC:SidebarV2LogicalProjects 2026-07-29 (P5 fix round):
 * `rootPath` comes from the daemon's `gitRepositoryRootPath` on the same
 * project context. Until it did, nothing on the client ever populated a root,
 * so `deriveRepositoryRelativePath` always returned null and "Repository +
 * path" produced the same key as "Repository" — a mode that could not split
 * anything. Two sub-projects of one monorepo now differ by their path below
 * the shared root, which is exactly what that mode promises.
 */
export function toSidebarV2Project(
  group: Pick<SidebarSessionGroup, 'groupId' | 'projectContext' | 'remoteMachineContext' | 'title'>,
  repository?: SidebarV2RepositoryIdentity | null
): SidebarV2Project {
  const resolvedRepository =
    repository === undefined
      ? deriveSidebarV2RepositoryIdentity({
          remoteUrl: group.projectContext?.gitRemoteOriginUrl,
          rootPath: group.projectContext?.gitRepositoryRootPath,
        })
      : repository;
  return {
    ...(group.remoteMachineContext ? { machineId: group.remoteMachineContext.machineId } : {}),
    ...(group.remoteMachineContext ? { machineName: group.remoteMachineContext.machineName } : {}),
    ...(group.projectContext?.path ? { path: group.projectContext.path } : {}),
    projectId: group.groupId,
    ...(resolvedRepository ? { repository: resolvedRepository } : {}),
    title: group.title,
  };
}

/**
 * CDXC:SidebarV2LogicalProjects 2026-07-29:
 * Builds the grouping settings the V2 view model consumes from the two pieces
 * the sidebar actually has: the automatic rule (always "repository" — decision
 * 3 makes origin-remote merging the default, with no global switch) and the
 * user's per-checkout overrides out of `ghostex-settings`.
 *
 * It exists so callers cannot accidentally pass a global mode of their own and
 * quietly change what "no override" means for every project at once.
 */
export function createSidebarV2ProjectGroupingSettings(
  overrides: Readonly<Record<string, SidebarV2ProjectGroupingMode>> | undefined
): SidebarV2ProjectGroupingSettings {
  return {
    sidebarV2ProjectGroupingMode: 'repository',
    sidebarV2ProjectGroupingOverrides: overrides ?? {},
  };
}
