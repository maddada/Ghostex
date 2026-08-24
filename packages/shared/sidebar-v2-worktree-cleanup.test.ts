import { describe, expect, test } from 'vitest';
import {
  findSessionsUsingWorktreePath,
  formatWorktreePathForDisplay,
  isSidebarV2ManagedWorktreeBranch,
  isSidebarV2TempWorktreeBranch,
  normalizeWorktreePath,
  normalizeWorktreePathForComparison,
  resolveOrphanedWorktreePathForSession,
  resolveOrphanedWorktreePathsForSessions,
  resolveSidebarV2ManagedWorktreePath,
  type SidebarV2WorktreeSession,
} from './sidebar-v2-worktree-cleanup';

function session(sessionId: string, worktreePath?: string): SidebarV2WorktreeSession {
  return worktreePath === undefined ? { sessionId } : { cwd: worktreePath, sessionId, worktreePath };
}

/** A session the git probe has not classified yet: cwd only, no managed path. */
function unprobedSession(sessionId: string, cwd: string): SidebarV2WorktreeSession {
  return { cwd, sessionId };
}

describe('normalizeWorktreePath', () => {
  test('treats blank paths as absent', () => {
    expect(normalizeWorktreePath('  /wt/a  ')).toBe('/wt/a');
    expect(normalizeWorktreePath('   ')).toBeNull();
    expect(normalizeWorktreePath(undefined)).toBeNull();
    expect(normalizeWorktreePath(null)).toBeNull();
  });
});

describe('normalizeWorktreePathForComparison', () => {
  test('one directory has one comparison form', () => {
    expect(normalizeWorktreePathForComparison('  /wt/a/  ')).toBe('/wt/a');
    expect(normalizeWorktreePathForComparison('/wt/a')).toBe('/wt/a');
    expect(normalizeWorktreePathForComparison('C:\\wt\\a\\')).toBe('C:\\wt\\a');
    expect(normalizeWorktreePathForComparison('/')).toBe('/');
    expect(normalizeWorktreePathForComparison('  ')).toBeNull();
    expect(normalizeWorktreePathForComparison(null)).toBeNull();
  });
});

describe('resolveOrphanedWorktreePathForSession', () => {
  test('the last session pointing at a worktree orphans it', () => {
    expect(
      resolveOrphanedWorktreePathForSession([session('a', '/wt/feature'), session('b', '/wt/other'), session('c')], 'a')
    ).toBe('/wt/feature');
  });

  test('a shared worktree is never orphaned', () => {
    expect(
      resolveOrphanedWorktreePathForSession([session('a', '/wt/feature'), session('b', ' /wt/feature ')], 'a')
    ).toBeNull();
  });

  test('an unprobed session in the same checkout blocks the offer', () => {
    /*
     * The git probe classifies branches on its own cycle, so a session created
     * seconds ago has a cwd and no gitStatus at all. It is still sitting in the
     * folder, so the worktree is not orphaned.
     */
    expect(
      resolveOrphanedWorktreePathForSession([session('a', '/wt/feature'), unprobedSession('b', '/wt/feature')], 'a')
    ).toBeNull();
    expect(
      resolveOrphanedWorktreePathForSession([session('a', '/wt/feature'), unprobedSession('b', '/wt/feature/')], 'a')
    ).toBeNull();
    expect(
      resolveOrphanedWorktreePathForSession([session('a', '/wt/feature'), unprobedSession('b', '/repo')], 'a')
    ).toBe('/wt/feature');
  });

  test('a trailing slash does not make a second worktree', () => {
    expect(
      resolveOrphanedWorktreePathForSession([session('a', '/wt/feature/'), session('b', '/wt/feature')], 'a')
    ).toBeNull();
  });

  test('a session without a worktree has nothing to clean up', () => {
    expect(resolveOrphanedWorktreePathForSession([session('a')], 'a')).toBeNull();
    expect(resolveOrphanedWorktreePathForSession([session('a', '   ')], 'a')).toBeNull();
  });

  test('an unknown session id resolves to nothing', () => {
    expect(resolveOrphanedWorktreePathForSession([session('a', '/wt/a')], 'gone')).toBeNull();
  });
});

describe('resolveOrphanedWorktreePathsForSessions', () => {
  test('a worktree is orphaned only when every session using it is removed', () => {
    const sessions = [session('a', '/wt/feature'), session('b', '/wt/feature'), session('c', '/wt/solo'), session('d')];
    expect(resolveOrphanedWorktreePathsForSessions(sessions, ['a'])).toEqual([]);
    expect(resolveOrphanedWorktreePathsForSessions(sessions, ['a', 'b'])).toEqual(['/wt/feature']);
    expect(resolveOrphanedWorktreePathsForSessions(sessions, ['a', 'b', 'c', 'd'])).toEqual([
      '/wt/feature',
      '/wt/solo',
    ]);
  });

  test('each orphaned path is reported once, in first-encountered order', () => {
    expect(
      resolveOrphanedWorktreePathsForSessions(
        [session('a', '/wt/z'), session('b', '/wt/z'), session('c', '/wt/a')],
        ['c', 'b', 'a']
      )
    ).toEqual(['/wt/z', '/wt/a']);
  });

  test('removing nothing orphans nothing', () => {
    expect(resolveOrphanedWorktreePathsForSessions([session('a', '/wt/a')], [])).toEqual([]);
  });

  test('a surviving unprobed session keeps its checkout', () => {
    expect(
      resolveOrphanedWorktreePathsForSessions([session('a', '/wt/feature'), unprobedSession('b', '/wt/feature')], ['a'])
    ).toEqual([]);
    expect(
      resolveOrphanedWorktreePathsForSessions(
        [session('a', '/wt/feature'), unprobedSession('b', '/wt/feature')],
        ['a', 'b']
      )
    ).toEqual(['/wt/feature']);
  });
});

describe('findSessionsUsingWorktreePath', () => {
  test('lists every session sharing the folder', () => {
    expect(
      findSessionsUsingWorktreePath(
        [session('a', '/wt/feature'), session('b', ' /wt/feature'), session('c', '/wt/other')],
        '/wt/feature'
      ).map((entry) => entry.sessionId)
    ).toEqual(['a', 'b']);
  });

  test('counts unprobed sessions by cwd too', () => {
    expect(
      findSessionsUsingWorktreePath(
        [session('a', '/wt/feature'), unprobedSession('b', '/wt/feature/'), session('c', '/wt/x')],
        '/wt/feature'
      ).map((entry) => entry.sessionId)
    ).toEqual(['a', 'b']);
  });

  test('a blank query matches nothing', () => {
    expect(findSessionsUsingWorktreePath([session('a', '/wt/a')], '  ')).toEqual([]);
  });
});

describe('formatWorktreePathForDisplay', () => {
  test('shows the last path segment', () => {
    expect(formatWorktreePathForDisplay('/Users/madda/worktrees/feature-x')).toBe('feature-x');
    expect(formatWorktreePathForDisplay('/Users/madda/worktrees/feature-x/')).toBe('feature-x');
    expect(formatWorktreePathForDisplay('C:\\dev\\worktrees\\feature-x')).toBe('feature-x');
  });

  test('degrades to the trimmed input when there is no usable segment', () => {
    expect(formatWorktreePathForDisplay('/')).toBe('/');
    expect(formatWorktreePathForDisplay('   ')).toBe('   ');
  });
});

describe('isSidebarV2TempWorktreeBranch', () => {
  test('matches the generated ghostex/<8hex> namespace only', () => {
    expect(isSidebarV2TempWorktreeBranch('ghostex/1a2b3c4d')).toBe(true);
    expect(isSidebarV2TempWorktreeBranch('  GHOSTEX/1A2B3C4D  ')).toBe(true);
    expect(isSidebarV2TempWorktreeBranch('ghostex/1a2b3c')).toBe(false);
    expect(isSidebarV2TempWorktreeBranch('ghostex/add-inbox-sidebar')).toBe(false);
    expect(isSidebarV2TempWorktreeBranch('feature/1a2b3c4d')).toBe(false);
    expect(isSidebarV2TempWorktreeBranch(undefined)).toBe(false);
  });
});

describe('isSidebarV2ManagedWorktreeBranch', () => {
  test('covers the whole ghostex/ namespace, before and after auto-rename', () => {
    expect(isSidebarV2ManagedWorktreeBranch('ghostex/1a2b3c4d')).toBe(true);
    expect(isSidebarV2ManagedWorktreeBranch('ghostex/add-inbox-sidebar')).toBe(true);
    expect(isSidebarV2ManagedWorktreeBranch('  GHOSTEX/Add-Inbox  ')).toBe(true);
    expect(isSidebarV2ManagedWorktreeBranch('feature/add-inbox')).toBe(false);
    expect(isSidebarV2ManagedWorktreeBranch(null)).toBe(false);
    expect(isSidebarV2ManagedWorktreeBranch(undefined)).toBe(false);
  });
});

describe('resolveSidebarV2ManagedWorktreePath', () => {
  test('needs BOTH a managed branch and a cwd', () => {
    expect(
      resolveSidebarV2ManagedWorktreePath({
        cwd: '/wt/feature',
        gitStatus: { branch: 'ghostex/1a2b3c4d' },
      })
    ).toBe('/wt/feature');
    expect(
      resolveSidebarV2ManagedWorktreePath({
        cwd: '/repo',
        gitStatus: { branch: 'main' },
      })
    ).toBeNull();
    expect(resolveSidebarV2ManagedWorktreePath({ gitStatus: { branch: 'ghostex/1a2b3c4d' } })).toBeNull();
    expect(resolveSidebarV2ManagedWorktreePath({ cwd: '/wt/feature' })).toBeNull();
  });

  test('a detached-HEAD probe is never a managed worktree', () => {
    expect(resolveSidebarV2ManagedWorktreePath({ cwd: '/wt/x', gitStatus: { branch: null } })).toBeNull();
  });
});
