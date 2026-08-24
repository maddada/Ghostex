import { describe, expect, test } from 'vitest';
import {
  DEFAULT_SIDEBAR_V2_PROJECT_GROUPING_SETTINGS,
  createSidebarV2ProjectGroupingSettings,
  deriveSidebarV2LogicalProjectKey,
  deriveSidebarV2LogicalProjectKeyFromSettings,
  deriveSidebarV2PhysicalProjectKey,
  deriveSidebarV2ProjectGroupingOverrideKey,
  deriveSidebarV2ProjectGroupLabel,
  deriveSidebarV2RepositoryIdentity,
  groupSidebarV2ProjectsByLogicalKey,
  normalizeGitRemoteUrl,
  normalizeProjectPathForComparison,
  resolveSidebarV2ProjectGroupingMode,
  toSidebarV2Project,
  type SidebarV2Project,
} from './sidebar-v2-logical-project';

/*
CDXC:SidebarV2LogicalProjects 2026-07-29 (P5 fix round):
Every fixture below is built through `toSidebarV2Project` from the SHIPPED wire
shape — `gitRemoteOriginUrl` + `gitRepositoryRootPath` on the project context —
rather than by hand-writing a `repository.rootPath`. Hand-written roots were how
the "Repository + path" mode passed its unit tests while being inert in the app:
nothing in the real path ever produced one. Building the identity the way the
sidebar does means these tests fail the moment the derivation stops carrying the
daemon's root through.
*/
function storyGroup(input: {
  gitRemoteOriginUrl?: string | null;
  gitRepositoryRootPath?: string;
  machine?: { machineId: string; machineName: string };
  path?: string;
  projectId: string;
  title?: string;
}) {
  return {
    groupId: input.projectId,
    ...(input.path === undefined
      ? {}
      : {
          projectContext: {
            canRemoveProject: true,
            editor: {
              diffStats: { additions: 0, deletions: 0, files: 0, isLoading: false, isRepo: true },
              isOpen: false,
              isSleeping: false,
              projectId: input.projectId,
              status: 'idle' as const,
            },
            ...(input.gitRemoteOriginUrl === undefined ? {} : { gitRemoteOriginUrl: input.gitRemoteOriginUrl }),
            ...(input.gitRepositoryRootPath === undefined
              ? {}
              : { gitRepositoryRootPath: input.gitRepositoryRootPath }),
            path: input.path,
          },
        }),
    ...(input.machine ? { remoteMachineContext: input.machine } : {}),
    title: input.title ?? input.projectId,
  };
}

/** A project exactly as the sidebar builds it: identity derived from the two
    presentation fields, never assembled by the test. */
function shippedProject(input: Parameters<typeof storyGroup>[0]): SidebarV2Project {
  return toSidebarV2Project(storyGroup(input));
}

const GHOSTEX_ORIGIN = 'git@github.com:ghostex/ghostex.git';
const GHOSTEX_ROOT = '/Users/madda/dev/Ghostex';
const GHOSTEX_REPO = shippedProject({
  gitRemoteOriginUrl: GHOSTEX_ORIGIN,
  gitRepositoryRootPath: GHOSTEX_ROOT,
  path: GHOSTEX_ROOT,
  projectId: 'ghostex',
}).repository!;

function project(overrides: Partial<SidebarV2Project> = {}): SidebarV2Project {
  return { projectId: 'project-1', title: 'Ghostex', ...overrides };
}

describe('normalizeGitRemoteUrl', () => {
  test('collapses every common remote shape onto host/owner/repo', () => {
    for (const remoteUrl of [
      'https://github.com/ghostex/Ghostex.git',
      'https://github.com/ghostex/ghostex',
      'https://github.com/ghostex/ghostex/',
      'git@github.com:ghostex/ghostex.git',
      'ssh://git@github.com/ghostex/ghostex.git',
      'git://github.com/ghostex/ghostex.git',
    ]) {
      expect(normalizeGitRemoteUrl(remoteUrl)).toBe('github.com/ghostex/ghostex');
    }
  });

  test('keeps nested group paths, as GitLab needs', () => {
    expect(normalizeGitRemoteUrl('https://gitlab.com/group/sub/app.git')).toBe('gitlab.com/group/sub/app');
    expect(normalizeGitRemoteUrl('git@gitlab.com:group/sub/app.git')).toBe('gitlab.com/group/sub/app');
  });

  test('distinct repositories never collide', () => {
    expect(normalizeGitRemoteUrl('git@github.com:a/one.git')).not.toBe(
      normalizeGitRemoteUrl('git@github.com:a/two.git')
    );
    expect(normalizeGitRemoteUrl('git@github.com:a/repo.git')).not.toBe(
      normalizeGitRemoteUrl('git@gitlab.com:a/repo.git')
    );
  });

  test('an unparseable value degrades to its trimmed lowercase form', () => {
    expect(normalizeGitRemoteUrl('  Some Local Thing  ')).toBe('some local thing');
  });
});

describe('deriveSidebarV2RepositoryIdentity', () => {
  test('derives owner, name, display name, and provider', () => {
    expect(GHOSTEX_REPO).toEqual({
      canonicalKey: 'github.com/ghostex/ghostex',
      displayName: 'ghostex/ghostex',
      name: 'ghostex',
      owner: 'ghostex',
      provider: 'github',
      rootPath: '/Users/madda/dev/Ghostex',
    });
  });

  test('detects the other hosted providers', () => {
    expect(deriveSidebarV2RepositoryIdentity({ remoteUrl: 'git@gitlab.com:a/b.git' })?.provider).toBe('gitlab');
    expect(deriveSidebarV2RepositoryIdentity({ remoteUrl: 'git@bitbucket.org:a/b.git' })?.provider).toBe('bitbucket');
    expect(
      deriveSidebarV2RepositoryIdentity({ remoteUrl: 'git@git.internal.example:a/b.git' })?.provider
    ).toBeUndefined();
  });

  test('a project with no remote gets no identity, so it can never merge', () => {
    expect(deriveSidebarV2RepositoryIdentity({ remoteUrl: null })).toBeNull();
    expect(deriveSidebarV2RepositoryIdentity({ remoteUrl: '   ' })).toBeNull();
  });
});

describe('normalizeProjectPathForComparison', () => {
  test('trims trailing separators without touching POSIX case', () => {
    expect(normalizeProjectPathForComparison('/Users/madda/dev/Ghostex/')).toBe('/Users/madda/dev/Ghostex');
    expect(normalizeProjectPathForComparison('  /a/b  ')).toBe('/a/b');
    expect(normalizeProjectPathForComparison('/A/b')).not.toBe(normalizeProjectPathForComparison('/a/b'));
  });

  test('folds Windows separators and case', () => {
    expect(normalizeProjectPathForComparison('C:/Users/Dev/App/')).toBe('c:\\users\\dev\\app');
    expect(normalizeProjectPathForComparison('\\\\server\\Share\\App')).toBe('\\\\server\\share\\app');
  });

  test('root paths survive intact', () => {
    expect(normalizeProjectPathForComparison('/')).toBe('/');
  });
});

describe('physical and override keys', () => {
  test('machine plus normalized path identifies one checkout', () => {
    expect(deriveSidebarV2PhysicalProjectKey(project({ machineId: 'mac-1', path: '/dev/app/' }))).toBe(
      'mac-1:/dev/app'
    );
  });

  test('a project with no path falls back to its own id, never to another project', () => {
    expect(deriveSidebarV2PhysicalProjectKey(project({ projectId: 'quick' }))).toBe('local:#quick');
    expect(deriveSidebarV2PhysicalProjectKey(project({ projectId: 'quick' }))).not.toBe(
      deriveSidebarV2PhysicalProjectKey(project({ projectId: 'chat' }))
    );
  });

  test('the same path on two machines is two physical checkouts', () => {
    expect(deriveSidebarV2PhysicalProjectKey(project({ machineId: 'a', path: '/dev/app' }))).not.toBe(
      deriveSidebarV2PhysicalProjectKey(project({ machineId: 'b', path: '/dev/app' }))
    );
  });

  test('overrides key by physical checkout', () => {
    const target = project({ machineId: 'mac-1', path: '/dev/app' });
    expect(deriveSidebarV2ProjectGroupingOverrideKey(target)).toBe(deriveSidebarV2PhysicalProjectKey(target));
    expect(
      resolveSidebarV2ProjectGroupingMode(target, {
        sidebarV2ProjectGroupingMode: 'repository',
        sidebarV2ProjectGroupingOverrides: { 'mac-1:/dev/app': 'separate' },
      })
    ).toBe('separate');
    expect(resolveSidebarV2ProjectGroupingMode(target, DEFAULT_SIDEBAR_V2_PROJECT_GROUPING_SETTINGS)).toBe(
      'repository'
    );
  });
});

describe('deriveSidebarV2LogicalProjectKey', () => {
  const main = shippedProject({
    gitRemoteOriginUrl: GHOSTEX_ORIGIN,
    gitRepositoryRootPath: GHOSTEX_ROOT,
    machine: { machineId: 'mac-1', machineName: 'Mac' },
    path: GHOSTEX_ROOT,
    projectId: 'project-1',
  });
  const remoteCopy = shippedProject({
    gitRemoteOriginUrl: GHOSTEX_ORIGIN,
    gitRepositoryRootPath: '/home/dev/ghostex',
    machine: { machineId: 'linux-box', machineName: 'Build Box' },
    path: '/home/dev/ghostex',
    projectId: 'project-2',
  });
  const subPackage = shippedProject({
    gitRemoteOriginUrl: GHOSTEX_ORIGIN,
    gitRepositoryRootPath: GHOSTEX_ROOT,
    machine: { machineId: 'mac-1', machineName: 'Mac' },
    path: `${GHOSTEX_ROOT}/packages/ui`,
    projectId: 'project-3',
  });

  test("'repository' merges every checkout of the repo across machines", () => {
    expect(deriveSidebarV2LogicalProjectKey(main)).toBe('github.com/ghostex/ghostex');
    expect(deriveSidebarV2LogicalProjectKey(remoteCopy)).toBe(deriveSidebarV2LogicalProjectKey(main));
    expect(deriveSidebarV2LogicalProjectKey(subPackage)).toBe(deriveSidebarV2LogicalProjectKey(main));
  });

  test("'repositoryPath' keeps sub-paths apart but still merges across machines", () => {
    const options = { groupingMode: 'repositoryPath' as const };
    expect(deriveSidebarV2LogicalProjectKey(main, options)).toBe('github.com/ghostex/ghostex');
    expect(deriveSidebarV2LogicalProjectKey(subPackage, options)).toBe('github.com/ghostex/ghostex::packages/ui');
    expect(deriveSidebarV2LogicalProjectKey(remoteCopy, options)).toBe(deriveSidebarV2LogicalProjectKey(main, options));
  });

  test("'separate' never merges", () => {
    const options = { groupingMode: 'separate' as const };
    expect(deriveSidebarV2LogicalProjectKey(main, options)).toBe(deriveSidebarV2PhysicalProjectKey(main));
    expect(deriveSidebarV2LogicalProjectKey(remoteCopy, options)).not.toBe(
      deriveSidebarV2LogicalProjectKey(main, options)
    );
  });

  test('non-git projects fall back to the physical key and never merge', () => {
    const left = project({ machineId: 'mac-1', path: '/tmp/a', projectId: 'a' });
    const right = project({ machineId: 'mac-1', path: '/tmp/b', projectId: 'b' });
    expect(deriveSidebarV2LogicalProjectKey(left)).toBe('mac-1:/tmp/a');
    expect(deriveSidebarV2LogicalProjectKey(left)).not.toBe(deriveSidebarV2LogicalProjectKey(right));
  });

  test('a project outside its reported repository root keeps the bare repository key', () => {
    expect(
      deriveSidebarV2LogicalProjectKey(
        shippedProject({
          gitRemoteOriginUrl: GHOSTEX_ORIGIN,
          gitRepositoryRootPath: GHOSTEX_ROOT,
          machine: { machineId: 'mac-1', machineName: 'Mac' },
          path: '/elsewhere/app',
          projectId: 'project-4',
        }),
        { groupingMode: 'repositoryPath' }
      )
    ).toBe('github.com/ghostex/ghostex');
  });

  test('a daemon that publishes no repository root cannot split by path', () => {
    /*
    An older daemon (or a checkout git will not report a root for) publishes the
    remote and nothing else. "Repository + path" must then degrade to plain
    repository merging rather than inventing a sub-path.
    */
    const rootless = shippedProject({
      gitRemoteOriginUrl: GHOSTEX_ORIGIN,
      machine: { machineId: 'mac-1', machineName: 'Mac' },
      path: `${GHOSTEX_ROOT}/packages/ui`,
      projectId: 'project-5',
    });
    expect(rootless.repository?.rootPath).toBeUndefined();
    expect(deriveSidebarV2LogicalProjectKey(rootless, { groupingMode: 'repositoryPath' })).toBe(
      'github.com/ghostex/ghostex'
    );
  });

  test('settings-driven derivation honours a per-project override', () => {
    expect(
      deriveSidebarV2LogicalProjectKeyFromSettings(remoteCopy, {
        sidebarV2ProjectGroupingMode: 'repository',
        sidebarV2ProjectGroupingOverrides: { 'linux-box:/home/dev/ghostex': 'separate' },
      })
    ).toBe('linux-box:/home/dev/ghostex');
  });
});

describe('groupSidebarV2ProjectsByLogicalKey', () => {
  const local = shippedProject({
    gitRemoteOriginUrl: GHOSTEX_ORIGIN,
    gitRepositoryRootPath: GHOSTEX_ROOT,
    machine: { machineId: 'mac-1', machineName: 'Mac' },
    path: GHOSTEX_ROOT,
    projectId: 'local',
    title: 'Ghostex',
  });
  const remote = shippedProject({
    gitRemoteOriginUrl: GHOSTEX_ORIGIN,
    gitRepositoryRootPath: '/home/dev/ghostex',
    machine: { machineId: 'linux-box', machineName: 'Build Box' },
    path: '/home/dev/ghostex',
    projectId: 'remote',
    title: 'ghostex',
  });
  const unrelated = project({ machineId: 'mac-1', path: '/tmp/notes', projectId: 'notes', title: 'Notes' });

  test('merges cross-machine checkouts into one group', () => {
    const groups = groupSidebarV2ProjectsByLogicalKey({
      primaryMachineId: 'mac-1',
      projects: [local, remote, unrelated],
    });
    expect(groups.map((group) => group.projectKey)).toEqual(['github.com/ghostex/ghostex', 'mac-1:/tmp/notes']);
    expect(groups[0]!.members.map((member) => member.projectId)).toEqual(['local', 'remote']);
    expect(groups[0]!.representative.projectId).toBe('local');
    expect(groups[0]!.machinePresence).toBe('mixed');
    expect(groups[0]!.remoteMachineNames).toEqual(['Build Box']);
    expect(groups[0]!.displayName).toBe('ghostex/ghostex');
  });

  test('a single-machine group keeps its own title and local presence', () => {
    const groups = groupSidebarV2ProjectsByLogicalKey({
      primaryMachineId: 'mac-1',
      projects: [local, unrelated],
    });
    expect(groups[0]!.displayName).toBe('Ghostex');
    expect(groups[0]!.machinePresence).toBe('local-only');
    expect(groups[1]!.machinePresence).toBe('local-only');
  });

  test('a repository that only lives remotely reports remote-only', () => {
    const groups = groupSidebarV2ProjectsByLogicalKey({
      primaryMachineId: 'mac-1',
      projects: [remote],
    });
    expect(groups[0]!.machinePresence).toBe('remote-only');
    expect(groups[0]!.representative.projectId).toBe('remote');
  });

  test("the 'separate' setting disables merging entirely", () => {
    const groups = groupSidebarV2ProjectsByLogicalKey({
      primaryMachineId: 'mac-1',
      projects: [local, remote],
      settings: {
        sidebarV2ProjectGroupingMode: 'separate',
        sidebarV2ProjectGroupingOverrides: {},
      },
    });
    expect(groups).toHaveLength(2);
  });

  test('group order follows first appearance in the input', () => {
    const groups = groupSidebarV2ProjectsByLogicalKey({
      primaryMachineId: 'mac-1',
      projects: [unrelated, remote, local],
    });
    expect(groups.map((group) => group.projectKey)).toEqual(['mac-1:/tmp/notes', 'github.com/ghostex/ghostex']);
  });
});

describe('deriveSidebarV2ProjectGroupLabel', () => {
  test('prefers the shared owner/repo display name', () => {
    expect(
      deriveSidebarV2ProjectGroupLabel({
        members: [
          project({ repository: GHOSTEX_REPO, title: 'Ghostex' }),
          project({ repository: GHOSTEX_REPO, title: 'ghostex-linux' }),
        ],
        representative: project({ title: 'Ghostex' }),
      })
    ).toBe('ghostex/ghostex');
  });

  test('falls back to the representative title when members disagree', () => {
    expect(
      deriveSidebarV2ProjectGroupLabel({
        members: [project({ title: 'A' }), project({ title: 'B' })],
        representative: project({ title: 'A' }),
      })
    ).toBe('A');
  });
});

describe('toSidebarV2Project', () => {
  test('adapts a sidebar group, carrying machine and path context', () => {
    expect(
      toSidebarV2Project({
        groupId: 'project-1',
        projectContext: {
          canRemoveProject: true,
          editor: {
            diffStats: {
              additions: 0,
              deletions: 0,
              files: 0,
              isLoading: false,
              isRepo: true,
            },
            isOpen: false,
            isSleeping: false,
            projectId: 'project-1',
            status: 'idle',
          },
          path: '/Users/madda/dev/Ghostex',
        },
        remoteMachineContext: { machineId: 'linux-box', machineName: 'Build Box' },
        title: 'Ghostex',
      })
    ).toEqual({
      machineId: 'linux-box',
      machineName: 'Build Box',
      path: '/Users/madda/dev/Ghostex',
      projectId: 'project-1',
      title: 'Ghostex',
    });
  });

  test('a local group with no probe result stays unmergeable', () => {
    const adapted = toSidebarV2Project({ groupId: 'project-1', title: 'Ghostex' });
    expect(adapted.repository).toBeUndefined();
    expect(deriveSidebarV2LogicalProjectKey(adapted)).toBe('local:#project-1');
  });
});

/*
CDXC:SidebarV2LogicalProjects 2026-07-29 (P5):
The adapter now derives the repository identity from the presentation field the
projection carries, so these cases pin the three wire states apart: a probed
remote merges, an explicitly-null remote never does, and an unprobed project
behaves exactly like the null one without being confused with it.
*/
describe('toSidebarV2Project — gitRemoteOriginUrl', () => {
  function groupWithRemote(
    groupId: string,
    path: string,
    gitRemoteOriginUrl: string | null | undefined,
    machine?: { machineId: string; machineName: string }
  ) {
    return {
      groupId,
      projectContext: {
        canRemoveProject: true,
        editor: {
          diffStats: { additions: 0, deletions: 0, files: 0, isLoading: false, isRepo: true },
          isOpen: false,
          isSleeping: false,
          projectId: groupId,
          status: 'idle' as const,
        },
        ...(gitRemoteOriginUrl === undefined ? {} : { gitRemoteOriginUrl }),
        path,
      },
      ...(machine ? { remoteMachineContext: machine } : {}),
      title: groupId,
    };
  }

  test('derives the repository identity from the probed origin remote', () => {
    const adapted = toSidebarV2Project(
      groupWithRemote('ghostex', '/Users/madda/dev/Ghostex', 'git@github.com:ghostex/ghostex.git')
    );
    expect(adapted.repository?.canonicalKey).toBe('github.com/ghostex/ghostex');
    expect(adapted.repository?.displayName).toBe('ghostex/ghostex');
  });

  test('a null origin and an unprobed project both stay unmergeable', () => {
    for (const remote of [null, undefined]) {
      const adapted = toSidebarV2Project(groupWithRemote('solo', '/Users/madda/dev/solo', remote));
      expect(adapted.repository).toBeUndefined();
      expect(deriveSidebarV2LogicalProjectKey(adapted)).toBe('local:/Users/madda/dev/solo');
    }
  });

  test('an explicit null identity beats the probed remote (non-git by request)', () => {
    const adapted = toSidebarV2Project(
      groupWithRemote('ghostex', '/Users/madda/dev/Ghostex', 'git@github.com:ghostex/ghostex.git'),
      null
    );
    expect(adapted.repository).toBeUndefined();
  });

  test('the same repository on two machines lands on one logical key', () => {
    const local = toSidebarV2Project(
      groupWithRemote('local', '/Users/madda/dev/Ghostex', 'git@github.com:ghostex/ghostex.git')
    );
    const remote = toSidebarV2Project(
      groupWithRemote('remote', '/home/build/ghostex', 'https://github.com/ghostex/Ghostex.git', {
        machineId: 'build-box',
        machineName: 'Build Box',
      })
    );
    const [group, ...rest] = groupSidebarV2ProjectsByLogicalKey({
      projects: [local, remote],
      settings: DEFAULT_SIDEBAR_V2_PROJECT_GROUPING_SETTINGS,
    });
    expect(rest).toHaveLength(0);
    expect(group?.members).toHaveLength(2);
    expect(group?.displayName).toBe('ghostex/ghostex');
    expect(group?.machinePresence).toBe('mixed');
    expect(group?.representative.projectId).toBe('local');
    expect(group?.remoteMachineNames).toEqual(['Build Box']);
  });

  test('two local clones of one repository merge as well', () => {
    const first = toSidebarV2Project(
      groupWithRemote('a', '/Users/madda/dev/Ghostex', 'git@github.com:ghostex/ghostex.git')
    );
    const second = toSidebarV2Project(
      groupWithRemote('b', '/Users/madda/dev/Ghostex-copy', 'git@github.com:ghostex/ghostex.git')
    );
    const groups = groupSidebarV2ProjectsByLogicalKey({ projects: [first, second] });
    expect(groups).toHaveLength(1);
    expect(groups[0]?.machinePresence).toBe('local-only');
  });

  test("a 'separate' override on both members splits them back apart", () => {
    const local = toSidebarV2Project(
      groupWithRemote('local', '/Users/madda/dev/Ghostex', 'git@github.com:ghostex/ghostex.git')
    );
    const remote = toSidebarV2Project(
      groupWithRemote('remote', '/home/build/ghostex', 'git@github.com:ghostex/ghostex.git', {
        machineId: 'build-box',
        machineName: 'Build Box',
      })
    );
    const groups = groupSidebarV2ProjectsByLogicalKey({
      projects: [local, remote],
      settings: {
        sidebarV2ProjectGroupingMode: 'repository',
        sidebarV2ProjectGroupingOverrides: {
          [deriveSidebarV2ProjectGroupingOverrideKey(local)]: 'separate',
          [deriveSidebarV2ProjectGroupingOverrideKey(remote)]: 'separate',
        },
      },
    });
    expect(groups.map((group) => group.projectKey)).toEqual([
      'local:/Users/madda/dev/Ghostex',
      'build-box:/home/build/ghostex',
    ]);
    expect(groups.every((group) => group.members.length === 1)).toBe(true);
  });
});

/*
CDXC:SidebarV2LogicalProjects 2026-07-29 (P5 fix round):
The monorepo case the "Repository + path" mode exists for, driven end to end
through the shipped wire shape. Before the daemon published
`gitRepositoryRootPath` this whole block was unexpressible: with no root, both
sub-projects keyed on the bare repository under EVERY mode, so the override was
present in the menu and inert in the list.
*/
describe('toSidebarV2Project — gitRepositoryRootPath', () => {
  const MONO_ORIGIN = 'git@github.com:ghostex/mono.git';
  const MONO_ROOT = '/Users/madda/dev/mono';
  const web = shippedProject({
    gitRemoteOriginUrl: MONO_ORIGIN,
    gitRepositoryRootPath: MONO_ROOT,
    path: `${MONO_ROOT}/apps/web`,
    projectId: 'web',
    title: 'web',
  });
  const api = shippedProject({
    gitRemoteOriginUrl: MONO_ORIGIN,
    gitRepositoryRootPath: MONO_ROOT,
    path: `${MONO_ROOT}/services/api`,
    projectId: 'api',
    title: 'api',
  });

  function keysFor(mode: 'repository' | 'repositoryPath' | 'separate') {
    return groupSidebarV2ProjectsByLogicalKey({
      projects: [web, api],
      settings: {
        sidebarV2ProjectGroupingMode: 'repository',
        sidebarV2ProjectGroupingOverrides: {
          [deriveSidebarV2ProjectGroupingOverrideKey(api)]: mode,
          [deriveSidebarV2ProjectGroupingOverrideKey(web)]: mode,
        },
      },
    }).map((group) => group.projectKey);
  }

  test("carries the daemon's repository root onto the identity", () => {
    expect(web.repository?.rootPath).toBe(MONO_ROOT);
    expect(web.repository?.canonicalKey).toBe('github.com/ghostex/mono');
  });

  test("'repository' merges both sub-projects of the monorepo", () => {
    expect(keysFor('repository')).toEqual(['github.com/ghostex/mono']);
  });

  test("'repositoryPath' splits them on their path below the shared root", () => {
    expect(keysFor('repositoryPath')).toEqual([
      'github.com/ghostex/mono::apps/web',
      'github.com/ghostex/mono::services/api',
    ]);
  });

  test("'separate' splits them by physical checkout", () => {
    expect(keysFor('separate')).toEqual([
      'local:/Users/madda/dev/mono/apps/web',
      'local:/Users/madda/dev/mono/services/api',
    ]);
  });

  test("the same sub-path on another machine still merges under 'repositoryPath'", () => {
    /*
    The mode splits on the SUB-PATH, not on the machine: `apps/web` checked out
    on a build box is the same logical project as `apps/web` here.
    */
    const remoteWeb = shippedProject({
      gitRemoteOriginUrl: 'https://github.com/ghostex/Mono.git',
      gitRepositoryRootPath: '/home/build/mono',
      machine: { machineId: 'build-box', machineName: 'Build Box' },
      path: '/home/build/mono/apps/web',
      projectId: 'remote-web',
      title: 'web',
    });
    const groups = groupSidebarV2ProjectsByLogicalKey({
      projects: [web, api, remoteWeb],
      settings: {
        sidebarV2ProjectGroupingMode: 'repository',
        sidebarV2ProjectGroupingOverrides: {
          [deriveSidebarV2ProjectGroupingOverrideKey(api)]: 'repositoryPath',
          [deriveSidebarV2ProjectGroupingOverrideKey(remoteWeb)]: 'repositoryPath',
          [deriveSidebarV2ProjectGroupingOverrideKey(web)]: 'repositoryPath',
        },
      },
    });
    expect(groups.map((group) => group.projectKey)).toEqual([
      'github.com/ghostex/mono::apps/web',
      'github.com/ghostex/mono::services/api',
    ]);
    expect(groups[0]?.members.map((member) => member.projectId)).toEqual(['web', 'remote-web']);
    expect(groups[0]?.machinePresence).toBe('mixed');
  });
});

describe('createSidebarV2ProjectGroupingSettings', () => {
  test('always pins the automatic rule to repository merging', () => {
    expect(createSidebarV2ProjectGroupingSettings(undefined)).toEqual({
      sidebarV2ProjectGroupingMode: 'repository',
      sidebarV2ProjectGroupingOverrides: {},
    });
  });

  test("carries the user's overrides through untouched", () => {
    const overrides = { 'local:/Users/madda/dev/Ghostex': 'separate' as const };
    expect(createSidebarV2ProjectGroupingSettings(overrides).sidebarV2ProjectGroupingOverrides).toEqual(overrides);
  });
});
