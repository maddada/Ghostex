import { readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";

const titlebarHostSource = readFileSync(new URL("./titlebar-host.tsx", import.meta.url), "utf8");
const sidebarGitSource = readFileSync(new URL("../../shared/sidebar-git.ts", import.meta.url), "utf8");

describe("native titlebar Git source", () => {
  test("labels Git metadata rows before their values in the titlebar Git menu", () => {
    /*
     * CDXC:TitlebarGit 2026-06-16-13:31:
     * macOS titlebar Git metadata rows must put Branch, Changes, and Commits
     * before their values, with inherited row typography so these labels match
     * the surrounding menu text.
     *
     * CDXC:TitlebarGit 2026-06-16-15:11:
     * The three prefix labels must share one fixed width so their following
     * values align vertically instead of drifting by label length.
     *
     * CDXC:TitlebarGit 2026-06-16-15:15:
     * The status rows should sit under a Status section label, and runnable Git
     * commands should sit under an Actions section label.
     *
     * CDXC:TitlebarGit 2026-06-16-18:41:
     * Branch, Changes, and Commits labels should not include trailing colons.
     *
     * CDXC:TitlebarGit 2026-06-16-19:03:
     * Branch, Changes, and Commits labels stay left-aligned while their values
     * share a right-aligned edge.
     *
     * CDXC:TitlebarGit 2026-06-16-19:10:
     * Changes and Commits number pairs should use a tighter gap while the whole
     * value group remains right-aligned.
     *
     * CDXC:TitlebarGit 2026-06-16-19:19:
     * The changed-files stat label should read Changes instead of Lines.
     */
    expect(titlebarHostSource).toContain("CDXC:TitlebarGit 2026-06-16-13:31:");
    expect(titlebarHostSource).toContain("CDXC:TitlebarGit 2026-06-16-15:11:");
    expect(titlebarHostSource).toContain("CDXC:TitlebarGit 2026-06-16-15:15:");
    expect(titlebarHostSource).toContain("CDXC:TitlebarGit 2026-06-16-18:41:");
    expect(titlebarHostSource).toContain("CDXC:TitlebarGit 2026-06-16-19:03:");
    expect(titlebarHostSource).toContain("CDXC:TitlebarGit 2026-06-16-19:10:");
    expect(titlebarHostSource).toContain("CDXC:TitlebarGit 2026-06-16-19:19:");
    expect(titlebarHostSource).toMatch(
      /<div className="titlebar-menu-section-label">Status<\/div>\s*<AppTooltip/,
    );
    expect(titlebarHostSource).toMatch(
      /<TitlebarPanelMenuSeparator \/>\s*<div className="titlebar-menu-section-label">Actions<\/div>\s*\{gitItems\.map/,
    );
    expect(titlebarHostSource).toContain("TITLEBAR_DROPDOWN_MENU_LABEL_HEIGHT * 2");
    expect(titlebarHostSource).toContain('<span className="titlebar-git-meta-label">Branch</span>');
    expect(titlebarHostSource).not.toContain('<span className="titlebar-git-meta-label">Branch:</span>');
    expect(titlebarHostSource).toMatch(
      /\.titlebar-git-stat-pair \{\s*align-items: center;\s*display: grid;\s*gap: 12px;\s*grid-column: 2;\s*grid-template-columns: 62px minmax\(0, 1fr\);\s*width: 100%;\s*\}/,
    );
    expect(titlebarHostSource).toMatch(
      /\.titlebar-git-stat-values \{\s*display: inline-flex;\s*\/\*[\s\S]*?CDXC:TitlebarGit 2026-06-16-19:10:[\s\S]*?\*\/\s*gap: 4px;\s*justify-self: end;/,
    );
    expect(titlebarHostSource).toMatch(
      /\.titlebar-git-stat \{\s*font: inherit;\s*min-width: 48px;\s*text-align: right;\s*\}/,
    );
    expect(titlebarHostSource).not.toMatch(
      /\.titlebar-git-stat \{\s*font: inherit;\s*min-width: 38px;\s*text-align: left;\s*\}/,
    );
    expect(titlebarHostSource).toMatch(
      /<TitlebarGitStatPair\s+firstCount=\{git\.additions\}\s+label="Changes"\s+secondCount=\{git\.deletions\}\s+\/>/,
    );
    expect(titlebarHostSource).not.toContain('label="Lines"');
    expect(titlebarHostSource).toMatch(
      /<span className="titlebar-git-meta-label">\{label\}<\/span>\s*<span className="titlebar-git-stat-values">/,
    );
    expect(titlebarHostSource).not.toMatch(
      /<span className="titlebar-git-meta-label">\{label\}:<\/span>/,
    );
    expect(titlebarHostSource).toMatch(
      /\.titlebar-git-meta-label \{\s*color: var\(--titlebar-git-value-color\);\s*font: inherit;\s*min-width: 62px;\s*text-align: left;\s*white-space: nowrap;\s*width: 62px;\s*\}/,
    );
    expect(titlebarHostSource).not.toContain("titlebar-git-stat-label");
  });

  test("shows neutral commit counts for the zero remote delta state", () => {
    /*
     * CDXC:TitlebarGit 2026-06-16-19:03:
     * The remote-sync status row should keep the Commits label and show
     * ↑ahead/↓behind counts even when both values are zero. Do not tint the
     * zero state green or replace the counts with synced copy.
     */
    expect(titlebarHostSource).toContain("hasSidebarGitRemoteCommitDelta");
    expect(sidebarGitSource).toContain("function hasSidebarGitRemoteCommitDelta");
    expect(titlebarHostSource).toMatch(
      /<TitlebarGitStatPair\s+firstCount=\{git\.aheadCount\}\s+firstPrefix="↑"\s+label="Commits"\s+secondCount=\{git\.behindCount\}\s+secondPrefix="↓"\s+tone="commits"\s+\/>/,
    );
    expect(titlebarHostSource).not.toContain("TitlebarGitRemoteCommitStatus");
    expect(titlebarHostSource).not.toContain("titlebar-git-synced-label");
    expect(titlebarHostSource).not.toContain("rgba(218, 245, 226, 0.78)");
    expect(titlebarHostSource).not.toContain("Synced with Remote");
  });

  test("copies the full branch row and explains the branch tooltip action", () => {
    /*
     * CDXC:TitlebarGit 2026-06-16-19:10:
     * The full Branch row should be clickable to copy the full branch name, and
     * its hover tooltip should explain the click action while capping at 250px.
     */
    expect(titlebarHostSource).toContain("const gitBranchLabel = titlebarGitBranchLabel(git.branch);");
    expect(titlebarHostSource).toContain('contentClassName="titlebar-git-branch-tooltip whitespace-normal text-left"');
    expect(titlebarHostSource).toContain("Click to copy branch name");
    expect(titlebarHostSource).toContain("void navigator.clipboard.writeText(gitBranchLabel);");
    expect(titlebarHostSource).toMatch(
      /<button\s+aria-label=\{`Copy branch \$\{gitBranchLabel\}`\}\s+className="titlebar-open-menu-item titlebar-git-meta-row titlebar-git-copy-branch-row"/,
    );
    expect(titlebarHostSource).toMatch(
      /\.titlebar-git-branch-tooltip \{[\s\S]*max-width: 250px;/,
    );
    expect(titlebarHostSource).toMatch(
      /\.titlebar-git-branch-name \{[\s\S]*justify-self: end;[\s\S]*text-align: right;/,
    );
    expect(titlebarHostSource).toMatch(
      /\.titlebar-git-copy-branch-row \{\s*cursor: copy !important;\s*\}/,
    );
  });

  test("opens the commit screen from the files row and explains the click target", () => {
    /*
     * CDXC:TitlebarGit 2026-06-16-19:10:
     * Clicking the Changes/files status row should open the same commit review
     * screen as the Commit action, with a tooltip explaining the behavior.
     */
    expect(titlebarHostSource).toContain('content="Open commit screen"');
    expect(titlebarHostSource).toMatch(
      /<TitlebarPanelMenuItem onClick=\{\(\) => closeAfter\(\(\) => onRunGitAction\("commit"\)\)\}>[\s\S]*<IconCode aria-hidden="true" className="titlebar-git-icon" size=\{15\} stroke=\{1\.8\} \/>[\s\S]*<TitlebarGitStatPair firstCount=\{git\.additions\} label="Changes" secondCount=\{git\.deletions\} \/>[\s\S]*<\/TitlebarPanelMenuItem>/,
    );
  });

  test("keeps the zero-delta remote row inert and refreshes stats before opening", () => {
    /*
     * CDXC:TitlebarGit 2026-06-16-18:41:
     * If the Commits row has no remote delta, the titlebar should not allow
     * the sync action to run. Opening the Git dropdown by click, double-click,
     * or context menu should request the latest Git state first.
     */
    expect(titlebarHostSource).toContain('| { type: "refreshGitState" }');
    expect(titlebarHostSource).toContain('postTitlebarSidebarCommand({ type: "refreshGitState" });');
    expect(titlebarHostSource).toMatch(
      /onClick=\{openGitMenuFromTitlebar\}[\s\S]*onContextMenu=\{\(event\) => \{[\s\S]*openGitMenuFromTitlebar\(event\);[\s\S]*onDoubleClick=\{openGitMenuFromTitlebar\}/,
    );
    expect(titlebarHostSource).toMatch(
      /if \(action === "syncRemote" && !hasSidebarGitRemoteCommitDelta\(projectState\.git\)\) \{\s*return;\s*\}/,
    );
    expect(titlebarHostSource).toMatch(
      /function titlebarGitRemoteSyncDisabledReason\(state: SidebarGitState\): string \| undefined \{[\s\S]*No remote commits to sync\./,
    );
  });

  test("hydrates transient refresh state from cached Git metadata", () => {
    /*
     * CDXC:TitlebarGit 2026-06-16-19:19:
     * The titlebar Git dropdown should reuse the last cached Git snapshot for
     * the active project while refresh is still publishing its busy/default
     * state, so Branch does not flash as detached before the branch probe
     * finishes.
     */
    expect(titlebarHostSource).toContain(
      'const TITLEBAR_GIT_STATE_CACHE_STORAGE_PREFIX = "ghostex.titlebar.gitState."',
    );
    expect(titlebarHostSource).toContain("function resolveTitlebarGitStateForMerge(");
    expect(titlebarHostSource).toContain("shouldHydrateMissingTitlebarGitStateFromCache(current, cached)");
    expect(titlebarHostSource).toMatch(
      /incoming\.isBusy &&\s*incoming\.branch === null &&\s*\(cached\.branch !== null \|\| cached\.isRepo\)/,
    );
    expect(titlebarHostSource).toContain("readCachedTitlebarGitState(projectIdentity)");
    expect(titlebarHostSource).toContain("cacheTitlebarGitState(next);");
    expect(titlebarHostSource).toContain("cacheTitlebarGitState(mergedState);");
    expect(titlebarHostSource).toContain("localStorage.setItem(cacheKey, JSON.stringify(state.git));");
  });

  test("uses a distinct code icon for change stats", () => {
    /*
     * CDXC:TitlebarGit 2026-06-16-15:15:
     * The Changes metadata row should use a code icon instead of the compare icon
     * used by the remote-sync action, so status and action rows are visually
     * distinguishable.
     */
    expect(titlebarHostSource).toMatch(
      /<IconCode aria-hidden="true" className="titlebar-git-icon" size=\{15\} stroke=\{1\.8\} \/>\s*<TitlebarGitStatPair firstCount=\{git\.additions\} label="Changes" secondCount=\{git\.deletions\} \/>/,
    );
    expect(titlebarHostSource).toMatch(
      /getTitlebarGitActionIcon\("syncRemote"\)/,
    );
  });

  test("uses neutral arrow commit stats for remote sync", () => {
    /*
     * CDXC:TitlebarGit 2026-06-16-13:31:
     * The titlebar remote-sync row should visually match the changed-file stats
     * row spacing while using neutral down/up arrows, a Commits prefix label,
     * no slash divider, and the direct remote push/pull action.
     */
    expect(titlebarHostSource).toMatch(
      /<TitlebarGitStatPair\s+firstCount=\{git\.aheadCount\}\s+firstPrefix="↑"\s+label="Commits"\s+secondCount=\{git\.behindCount\}\s+secondPrefix="↓"\s+tone="commits"\s+\/>/,
    );
    expect(titlebarHostSource).toContain('data-tone={tone}');
    expect(titlebarHostSource).toMatch(
      /\.titlebar-git-stat-pair\[data-tone="commits"\] \.titlebar-git-stat \{\s*color: var\(--titlebar-git-value-color\);/,
    );
    expect(titlebarHostSource).toContain('onRunGitAction("syncRemote")');
    expect(titlebarHostSource).not.toContain("titlebarGitSyncMainLabel");
    expect(titlebarHostSource).not.toContain(" / +");
  });
});
