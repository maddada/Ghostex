import { IconGitBranch } from '@tabler/icons-react';

/**
 * CDXC:SessionFork 2026-08-28:
 * The one branch marker for session rows. gxserver collapses a closed ancestor
 * once something continues from it, so any row that still says it has branches
 * is telling the truth about LIVING siblings the user can switch between. The
 * count is the number of visible branches, this row included, which is why it
 * only ever renders at two or more.
 */
export function SessionForkBranchBadge({ branchCount }: { branchCount?: number }) {
  if (typeof branchCount !== 'number' || !Number.isFinite(branchCount) || branchCount < 2) {
    return null;
  }

  const count = Math.floor(branchCount);
  const tooltip = `This session has ${count} branches that share earlier history.`;

  return (
    <span aria-label={tooltip} className='session-fork-branch-badge' title={tooltip}>
      <IconGitBranch aria-hidden='true' size={11} stroke={2} />
      {count}
    </span>
  );
}
