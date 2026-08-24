import { describe, expect, it } from 'vitest';
import {
  moveSidebarV2GroupRows,
  projectSidebarV2GroupOrderByMachine,
  type SidebarV2GroupOrderRow,
} from './sidebar-v2-group-order';

function row(groupId: string, memberGroupIds: readonly string[] = [groupId]): SidebarV2GroupOrderRow {
  return { groupId, memberGroupIds };
}

describe('moveSidebarV2GroupRows', () => {
  it('moves a row before its target', () => {
    const next = moveSidebarV2GroupRows([row('a'), row('b'), row('c')], 'c', { groupId: 'a', position: 'before' });
    expect(next?.map((entry) => entry.groupId)).toEqual(['c', 'a', 'b']);
  });

  it('moves a row after its target', () => {
    const next = moveSidebarV2GroupRows([row('a'), row('b'), row('c')], 'a', { groupId: 'c', position: 'after' });
    expect(next?.map((entry) => entry.groupId)).toEqual(['b', 'c', 'a']);
  });

  it('treats an adjacent boundary as a no-op', () => {
    expect(moveSidebarV2GroupRows([row('a'), row('b')], 'a', { groupId: 'b', position: 'before' })).toBeUndefined();
    expect(moveSidebarV2GroupRows([row('a'), row('b')], 'b', { groupId: 'a', position: 'after' })).toBeUndefined();
  });

  it('ignores a drop onto the dragged row itself and unknown ids', () => {
    expect(moveSidebarV2GroupRows([row('a'), row('b')], 'a', { groupId: 'a', position: 'after' })).toBeUndefined();
    expect(moveSidebarV2GroupRows([row('a'), row('b')], 'zzz', { groupId: 'b', position: 'after' })).toBeUndefined();
    expect(moveSidebarV2GroupRows([row('a'), row('b')], 'a', { groupId: 'zzz', position: 'after' })).toBeUndefined();
  });
});

describe('projectSidebarV2GroupOrderByMachine', () => {
  it('reorders a single-machine list exactly like the logical order', () => {
    const projected = projectSidebarV2GroupOrderByMachine({
      groupIdsByMachineId: { local: ['a', 'b', 'c'] },
      rows: [row('a'), row('b'), row('c')],
      sourceGroupId: 'c',
      target: { groupId: 'a', position: 'before' },
    });
    expect(projected).toEqual({ local: ['c', 'a', 'b'] });
  });

  it('posts one list per machine that owns a member of the dragged row', () => {
    /*
     * `repo` is checked out on both machines and merges into one logical row.
     * Dragging that row to the top has to move the LOCAL checkout inside the
     * local list and the REMOTE checkout inside the remote list, as two separate
     * orders — `syncGroupOrder` cannot carry a mixed list.
     */
    const projected = projectSidebarV2GroupOrderByMachine({
      groupIdsByMachineId: {
        local: ['local-alpha', 'local-repo', 'local-zeta'],
        'machine-1': ['remote-alpha', 'remote-repo'],
      },
      rows: [
        row('local-alpha', ['local-alpha', 'remote-alpha']),
        row('local-repo', ['local-repo', 'remote-repo']),
        row('local-zeta'),
      ],
      sourceGroupId: 'local-repo',
      target: { groupId: 'local-alpha', position: 'before' },
    });
    expect(projected).toEqual({
      local: ['local-repo', 'local-alpha', 'local-zeta'],
      'machine-1': ['remote-repo', 'remote-alpha'],
    });
  });

  it('leaves a machine untouched when it owns no member of the dragged row', () => {
    const projected = projectSidebarV2GroupOrderByMachine({
      groupIdsByMachineId: {
        local: ['local-a', 'local-b'],
        'machine-1': ['remote-x', 'remote-y'],
      },
      rows: [row('local-a'), row('local-b'), row('remote-x'), row('remote-y')],
      sourceGroupId: 'local-b',
      target: { groupId: 'local-a', position: 'before' },
    });
    expect(projected).toEqual({ local: ['local-b', 'local-a'] });
    expect(projected['machine-1']).toBeUndefined();
  });

  it('posts nothing for a machine whose own order already satisfies the drop', () => {
    /*
     * This machine's order disagrees with the merged logical order (`remote-repo`
     * is last here, `remote-beta` first). Dropping the row after `remote-beta`
     * asks for "repo below beta", which is ALREADY true here — so the projection
     * must post nothing rather than rewrite the list to match the logical order
     * and silently reshuffle `remote-beta`/`remote-gamma`.
     */
    const projected = projectSidebarV2GroupOrderByMachine({
      groupIdsByMachineId: {
        'machine-1': ['remote-beta', 'remote-gamma', 'remote-repo'],
      },
      rows: [row('remote-gamma'), row('remote-repo'), row('remote-beta')],
      sourceGroupId: 'remote-repo',
      target: { groupId: 'remote-beta', position: 'after' },
    });
    expect(projected).toEqual({});
  });

  it('lands the block after the nearest preceding neighbour that machine actually has', () => {
    const projected = projectSidebarV2GroupOrderByMachine({
      groupIdsByMachineId: {
        'machine-1': ['remote-one', 'remote-two', 'remote-three'],
      },
      rows: [row('local-only-a'), row('remote-one'), row('local-only-b'), row('remote-two'), row('remote-three')],
      sourceGroupId: 'remote-three',
      target: { groupId: 'local-only-b', position: 'before' },
    });
    // The logical boundary sits between `remote-one` and `local-only-b`; on this
    // machine that means directly after `remote-one`.
    expect(projected).toEqual({ 'machine-1': ['remote-one', 'remote-three', 'remote-two'] });
  });

  it('moves a project and its same-machine worktrees together, in machine order', () => {
    const projected = projectSidebarV2GroupOrderByMachine({
      groupIdsByMachineId: {
        local: ['other', 'repo', 'repo-worktree-1', 'repo-worktree-2', 'tail'],
      },
      rows: [row('other'), row('repo', ['repo', 'repo-worktree-2', 'repo-worktree-1']), row('tail')],
      sourceGroupId: 'repo',
      target: { groupId: 'tail', position: 'after' },
    });
    expect(projected).toEqual({
      local: ['other', 'tail', 'repo', 'repo-worktree-1', 'repo-worktree-2'],
    });
  });

  it('returns nothing for a no-op drop', () => {
    expect(
      projectSidebarV2GroupOrderByMachine({
        groupIdsByMachineId: { local: ['a', 'b'] },
        rows: [row('a'), row('b')],
        sourceGroupId: 'a',
        target: { groupId: 'b', position: 'before' },
      })
    ).toEqual({});
  });
});
