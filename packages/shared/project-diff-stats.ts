export type SidebarProjectDiffStats = {
  additions: number;
  deletions: number;
  files: number;
  isLoading: boolean;
  isRepo: boolean;
};

/**
 * CDXC:ProjectDiffStats 2026-05-06-14:21
 * Project editor buttons must show a compact file/addition/deletion summary
 * for each project. Keep the parsing pure so native sidebar process output can
 * be tested without touching the filesystem or shelling out from tests.
 */
export function createDefaultSidebarProjectDiffStats(
  isLoading = false,
): SidebarProjectDiffStats {
  return {
    additions: 0,
    deletions: 0,
    files: 0,
    isLoading,
    isRepo: false,
  };
}

export function parseGitNumstatDiffStats(stdout: string): SidebarProjectDiffStats {
  const totals = stdout
    .trim()
    .split("\n")
    .filter(Boolean)
    .reduce(
      (stats, line) => {
        const [additions, deletions] = line.split(/\s+/);
        return {
          additions: stats.additions + normalizeGitNumstatValue(additions),
          deletions: stats.deletions + normalizeGitNumstatValue(deletions),
          files: stats.files + 1,
        };
      },
      { additions: 0, deletions: 0, files: 0 },
    );

  return {
    ...totals,
    isLoading: false,
    isRepo: true,
  };
}

export function mergeSidebarProjectDiffStats(
  left: SidebarProjectDiffStats,
  right: SidebarProjectDiffStats,
): SidebarProjectDiffStats {
  return {
    additions: left.additions + right.additions,
    deletions: left.deletions + right.deletions,
    files: left.files + right.files,
    isLoading: left.isLoading || right.isLoading,
    isRepo: left.isRepo || right.isRepo,
  };
}

/**
 * CDXC:ProjectDiffStats 2026-05-27-09:25:
 * Project-header git stats should match Starship-style tracked diffs
 * (`git diff --numstat HEAD`) by default. Only fold untracked `wc -l` totals in
 * when the user enables the opt-in setting and there are no tracked line changes.
 */
export function resolveSidebarProjectDiffStats({
  showUntrackedWhenNoTrackedChanges,
  trackedStats,
  untrackedStats,
}: {
  showUntrackedWhenNoTrackedChanges: boolean;
  trackedStats: SidebarProjectDiffStats;
  untrackedStats: SidebarProjectDiffStats;
}): SidebarProjectDiffStats {
  const hasTrackedLineChanges = trackedStats.additions > 0 || trackedStats.deletions > 0;
  if (hasTrackedLineChanges || !showUntrackedWhenNoTrackedChanges) {
    return trackedStats;
  }

  return mergeSidebarProjectDiffStats(trackedStats, untrackedStats);
}

export function parseGitZeroDelimitedPaths(stdout: string): string[] {
  return stdout.split("\0").filter((path) => path.length > 0);
}

function normalizeGitNumstatValue(value: string | undefined): number {
  if (!value || value === "-") {
    return 0;
  }
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : 0;
}
