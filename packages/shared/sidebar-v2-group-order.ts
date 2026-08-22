/*
 * CDXC:SidebarV2GroupedProjectUX 2026-07-30:
 * Reordering a Sidebar V2 grouped project row is not the same operation V1
 * performs, and this module exists because of that difference.
 *
 * V1 drags ONE physical project group inside ONE machine's list, so a drop is a
 * single reordered id list posted through `syncGroupOrder`. A V2 grouped row is
 * a LOGICAL project: it can merge several physical checkouts, on several
 * machines (see `shared/sidebar-v2-logical-project.ts`). There is no single list
 * to reorder, and `syncGroupOrder` rejects a mixed local/remote id list outright
 * (gxserver-runtime.ts refuses to persist an order that spans machines, because
 * each machine owns its own project order).
 *
 * So a logical drop has to be PROJECTED: work out the row's new index among the
 * logical rows, then express that same intent inside each participating
 * machine's own list, and post one `syncGroupOrder` per machine.
 *
 * Two deliberate properties:
 *
 * 1. Only the dragged row's members MOVE. Every other id on a machine keeps its
 *    position. Rewriting a machine's whole order to match the merged logical
 *    order would be wrong: the logical order is a MERGE of several machines'
 *    orders, so it does not equal any one machine's saved order, and a single
 *    drag would silently reshuffle projects the user never touched on machines
 *    they were not even looking at.
 *
 * 2. A row's members move as a BLOCK, in that machine's existing relative
 *    order. Under the default "repository" grouping a row legitimately holds
 *    several checkouts on the SAME machine (a project plus its worktrees, which
 *    share a git origin), and those have to land together — which is also what
 *    V1's worktree-family move achieves for the physical case.
 */

/** One rendered V2 grouped row, reduced to what an order projection needs. */
export type SidebarV2GroupOrderRow = {
  /** The REPRESENTATIVE host group id, i.e. the id the header renders with and
      the id a drop target names. */
  groupId: string;
  /** Every host group id merged into this row, representative first. */
  memberGroupIds: readonly string[];
};

export type SidebarV2GroupOrderDropTarget = {
  /** Representative id of the row the insertion boundary belongs to. */
  groupId: string;
  position: "after" | "before";
};

export type SidebarV2GroupOrderProjectionInput = {
  /**
   * Each machine's OWN ordered host group ids, keyed by machine id. The local
   * daemon uses whatever key the caller reserves for it; this module never
   * interprets the keys, it only keeps each list's ids together in one message.
   */
  groupIdsByMachineId: Readonly<Record<string, readonly string[]>>;
  /** The rendered rows in their CURRENT displayed order. */
  rows: readonly SidebarV2GroupOrderRow[];
  /** Representative id of the dragged row. */
  sourceGroupId: string;
  target: SidebarV2GroupOrderDropTarget;
};

/**
 * The next order for the LOGICAL rows, or `undefined` when the drop changes
 * nothing (an adjacent before/after boundary around the row's own slot, an
 * unknown source or target, or a target that is the source itself).
 */
export function moveSidebarV2GroupRows(
  rows: readonly SidebarV2GroupOrderRow[],
  sourceGroupId: string,
  target: SidebarV2GroupOrderDropTarget,
): SidebarV2GroupOrderRow[] | undefined {
  if (target.groupId === sourceGroupId) {
    return undefined;
  }
  const sourceIndex = rows.findIndex((row) => row.groupId === sourceGroupId);
  if (sourceIndex < 0) {
    return undefined;
  }
  const withoutSource = rows.filter((row) => row.groupId !== sourceGroupId);
  const anchorIndex = withoutSource.findIndex((row) => row.groupId === target.groupId);
  if (anchorIndex < 0) {
    return undefined;
  }
  const insertionIndex = target.position === "before" ? anchorIndex : anchorIndex + 1;
  const next = [...withoutSource];
  next.splice(insertionIndex, 0, rows[sourceIndex]);
  return next.every((row, index) => row.groupId === rows[index]?.groupId) ? undefined : next;
}

/**
 * Projects a logical-row reorder onto every machine that owns members of the
 * dragged row.
 *
 * Returns one entry per machine whose list actually changed, so the caller can
 * post exactly that many `syncGroupOrder` messages and no redundant ones. A
 * machine with no member of the dragged row is never touched: the drag says
 * nothing about that machine's order.
 */
export function projectSidebarV2GroupOrderByMachine(
  input: SidebarV2GroupOrderProjectionInput,
): Record<string, string[]> {
  const nextRows = moveSidebarV2GroupRows(input.rows, input.sourceGroupId, input.target);
  if (!nextRows) {
    return {};
  }
  const sourceRow = input.rows.find((row) => row.groupId === input.sourceGroupId);
  if (!sourceRow) {
    return {};
  }
  const nextSourceIndex = nextRows.findIndex((row) => row.groupId === input.sourceGroupId);

  const projected: Record<string, string[]> = {};
  for (const [machineId, machineGroupIds] of Object.entries(input.groupIdsByMachineId)) {
    const machineIdSet = new Set(machineGroupIds);
    /*
     * The block keeps the MACHINE's relative order, not the row's member order:
     * the member list is representative-first (a cross-machine convention), and
     * imposing it here would reorder a project against its own worktrees.
     */
    const movingIds = machineGroupIds.filter((groupId) =>
      sourceRow.memberGroupIds.includes(groupId),
    );
    if (movingIds.length === 0) {
      continue;
    }
    const movingIdSet = new Set(movingIds);
    const remaining = machineGroupIds.filter((groupId) => !movingIdSet.has(groupId));

    /*
     * The insertion point is expressed against this machine's own list: take
     * every row that now sits ABOVE the dragged one, and land the block just
     * after the lowest of their members as THIS machine orders them. That is
     * what makes the projection stable when a machine's order disagrees with the
     * merged logical order — the block lands relative to real neighbours rather
     * than at an index borrowed from another list.
     */
    let insertionIndex = 0;
    for (const row of nextRows.slice(0, nextSourceIndex)) {
      for (const memberGroupId of row.memberGroupIds) {
        if (!machineIdSet.has(memberGroupId) || movingIdSet.has(memberGroupId)) {
          continue;
        }
        const memberIndex = remaining.indexOf(memberGroupId);
        if (memberIndex >= 0) {
          insertionIndex = Math.max(insertionIndex, memberIndex + 1);
        }
      }
    }

    const next = [...remaining];
    next.splice(insertionIndex, 0, ...movingIds);
    if (next.every((groupId, index) => groupId === machineGroupIds[index])) {
      continue;
    }
    projected[machineId] = next;
  }
  return projected;
}
