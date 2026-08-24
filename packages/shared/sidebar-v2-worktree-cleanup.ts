import type { SidebarV2Session } from './sidebar-v2-session';

/*
CDXC:SidebarV2 2026-07-29-00:00:
Implements worktree cleanup and the temp-branch naming in
`packages/shared/src/git.ts`.

In V2 a worktree is an ATTRIBUTE of a session (its cwd plus branch), not a
registered sibling project (spec decision 4). Deleting the LAST session pointing
at a worktree therefore orphans the folder, and the delete flow offers cleanup.
Consumed in P4; the detection is pure so the confirm dialog and the server-side
delete path can agree without re-walking sessions.
*/

/** Temp branch namespace for lazily created V2 worktrees: `ghostex/<8hex>`. */
export const SIDEBAR_V2_TEMP_BRANCH_PREFIX = 'ghostex/';

const TEMP_BRANCH_PATTERN = /^ghostex\/[0-9a-f]{8}$/;

export type SidebarV2WorktreeSession = Pick<SidebarV2Session, 'sessionId' | 'worktreePath'> & {
  /**
   * CDXC:SidebarV2Worktree 2026-07-29:
   * The session's working directory, which EVERY session carries from its first
   * frame — unlike `worktreePath`, which only exists once the ~60s git probe has
   * classified the row's branch as one of ours.
   *
   * That asymmetry is why sharing is decided here and not on `worktreePath`: a
   * session created seconds ago in the same checkout must already block the
   * "remove this worktree?" offer, even though nothing has probed it yet.
   * `worktreePath` stays the MANAGED answer for the row being closed.
   */
  cwd?: string | null;
};

export function normalizeWorktreePath(path: string | null | undefined): string | null {
  const trimmed = path?.trim();
  return trimmed ? trimmed : null;
}

/**
 * Comparison form for checkout paths: trimmed, with trailing separators dropped,
 * so `/wt/feature` and `/wt/feature/` are one worktree rather than two.
 *
 * String comparison only — a checkout reached through a symlink still reads as a
 * different path here; resolving that is the daemon's job, and it is the same
 * caveat the rest of this flow carries.
 */
export function normalizeWorktreePathForComparison(path: string | null | undefined): string | null {
  const trimmed = normalizeWorktreePath(path);
  if (trimmed === null) {
    return null;
  }
  const stripped = trimmed.replace(/[/\\]+$/, '');
  return stripped.length > 0 ? stripped : trimmed;
}

/** The checkout a session occupies, for "is anyone else in this folder?". */
function worktreeSharingKey(session: SidebarV2WorktreeSession): string | null {
  return normalizeWorktreePathForComparison(session.cwd ?? session.worktreePath);
}

/**
 * The worktree path left orphaned by removing `sessionId`, or null when the
 * session has no worktree or another session still points at the same folder.
 */
export function resolveOrphanedWorktreePathForSession(
  sessions: readonly SidebarV2WorktreeSession[],
  sessionId: string
): string | null {
  const target = sessions.find((session) => session.sessionId === sessionId);
  if (!target) {
    return null;
  }
  const targetWorktreePath = normalizeWorktreePath(target.worktreePath);
  if (!targetWorktreePath) {
    return null;
  }

  const targetKey = normalizeWorktreePathForComparison(targetWorktreePath);
  const isShared = sessions.some(
    (session) => session.sessionId !== sessionId && worktreeSharingKey(session) === targetKey
  );
  return isShared ? null : targetWorktreePath;
}

/**
 * Bulk twin for multi-select delete: a worktree only counts as orphaned when
 * EVERY session pointing at it is being removed. Returns each path once, in
 * first-encountered order.
 */
export function resolveOrphanedWorktreePathsForSessions(
  sessions: readonly SidebarV2WorktreeSession[],
  removedSessionIds: readonly string[]
): string[] {
  const removedIds = new Set(removedSessionIds);
  const survivingPaths = new Set(
    sessions
      .filter((session) => !removedIds.has(session.sessionId))
      .flatMap((session) => {
        const key = worktreeSharingKey(session);
        return key ? [key] : [];
      })
  );

  const orphaned: string[] = [];
  const seen = new Set<string>();
  for (const session of sessions) {
    if (!removedIds.has(session.sessionId)) {
      continue;
    }
    const path = normalizeWorktreePath(session.worktreePath);
    const key = normalizeWorktreePathForComparison(path);
    if (!path || !key || survivingPaths.has(key) || seen.has(key)) {
      continue;
    }
    seen.add(key);
    orphaned.push(path);
  }
  return orphaned;
}

/** Sessions still using a worktree path, for "3 other sessions use this
    worktree" copy in the delete confirm. */
export function findSessionsUsingWorktreePath(
  sessions: readonly SidebarV2WorktreeSession[],
  worktreePath: string
): SidebarV2WorktreeSession[] {
  const normalized = normalizeWorktreePathForComparison(worktreePath);
  if (!normalized) {
    return [];
  }
  return sessions.filter((session) => worktreeSharingKey(session) === normalized);
}

/** Last path segment, for compact row/dialog labels. Falls back to the trimmed
    input when the path has no usable segment (a bare "/" for instance). */
export function formatWorktreePathForDisplay(worktreePath: string): string {
  const trimmed = worktreePath.trim();
  if (!trimmed) {
    return worktreePath;
  }
  const normalized = trimmed.replaceAll('\\', '/').replace(/\/+$/, '');
  const lastSegment = normalized.split('/').at(-1)?.trim() ?? '';
  return lastSegment.length > 0 ? lastSegment : trimmed;
}

/** True for the auto-generated `ghostex/<8hex>` branches the V2 worktree flow
    creates before auto-rename gives them a descriptive slug. */
export function isSidebarV2TempWorktreeBranch(branch: string | null | undefined): boolean {
  const trimmed = branch?.trim().toLowerCase();
  return trimmed ? TEMP_BRANCH_PATTERN.test(trimmed) : false;
}

/**
 * CDXC:SidebarV2Worktree 2026-07-29:
 * True for any branch inside the flow's OWN namespace — the temp
 * `ghostex/<8hex>` and the `ghostex/<slug>` it is auto-renamed to.
 *
 * This is what "managed" means for cleanup: the client only ever offers to
 * remove a checkout THIS flow created. A worktree the user made by hand, or one
 * opened through the "open existing" path, keeps its own branch name, so it is
 * never proposed for deletion — the session just closes, exactly as it does in
 * V1. The narrower `isSidebarV2TempWorktreeBranch` stays for the pre-rename
 * question ("is this branch still nameless?").
 */
export function isSidebarV2ManagedWorktreeBranch(branch: string | null | undefined): boolean {
  const trimmed = branch?.trim().toLowerCase();
  return trimmed ? trimmed.startsWith(SIDEBAR_V2_TEMP_BRANCH_PREFIX) : false;
}

/**
 * The managed worktree a session lives in, or null.
 *
 * A worktree session is identified by the PAIR: its `cwd` is the checkout and
 * its probed branch is in the flow's namespace. Neither half is enough on its
 * own — every session has a cwd, and a branch name says nothing about which
 * folder is on disk.
 */
export function resolveSidebarV2ManagedWorktreePath(session: {
  cwd?: string | null;
  gitStatus?: { branch?: string | null } | undefined;
}): string | null {
  if (!isSidebarV2ManagedWorktreeBranch(session.gitStatus?.branch)) {
    return null;
  }
  return normalizeWorktreePath(session.cwd);
}
