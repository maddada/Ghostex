import { afterAll, beforeAll, describe, expect, test } from 'vitest';
import { readFileSync } from 'node:fs';
import {
  formatProjectEditorDiffStatsLabel,
  formatProjectTooltipGitStats,
  getEmptyProjectNewSessionButtonLabel,
  getGroupContextMenuItemCount,
  getPinnedSessionDropGapKey,
  getSidebarSessionGapContextMenuTarget,
  PINNED_SESSION_DROP_GAP_AFTER_LAST,
  shouldPreventGroupDragActivation,
  shouldTreatProjectAsEmptySessionGroup,
  shouldShowProjectEditorDiffStats,
} from './session-group-section';

const sessionGroupSectionSource = readFileSync(new URL('./session-group-section.tsx', import.meta.url), 'utf8');
const sessionGroupStylesSource = readFileSync(new URL('./styles/groups.css', import.meta.url), 'utf8');
const groupPanelStylesSource = readFileSync(new URL('./styles/group-panels.css', import.meta.url), 'utf8');
const sidebarAppSource = readFileSync(new URL('./sidebar-app.tsx', import.meta.url), 'utf8');

const originalElement = globalThis.Element;
const hadOriginalElement = 'Element' in globalThis;

class FakeElement extends EventTarget {
  public readonly children: FakeElement[] = [];
  public readonly attributes = new Map<string, string>();
  public readonly classNames = new Set<string>();
  public parentElement: FakeElement | undefined;

  constructor(public readonly tagName: string) {
    super();
  }

  public append(...children: FakeElement[]): void {
    for (const child of children) {
      child.parentElement = this;
      this.children.push(child);
    }
  }

  public addClass(className: string): void {
    this.classNames.add(className);
  }

  public setAttribute(name: string, value: string): void {
    this.attributes.set(name, value);
  }

  public contains(target: FakeElement | null): boolean {
    let current = target;
    while (current) {
      if (current === this) {
        return true;
      }
      current = current.parentElement ?? null;
    }
    return false;
  }

  public closest(selector: string): FakeElement | null {
    let current: FakeElement | undefined = this;
    while (current) {
      if (matchesGroupDragSelector(current, selector)) {
        return current;
      }
      current = current.parentElement;
    }
    return null;
  }
}

beforeAll(() => {
  Object.defineProperty(globalThis, 'Element', {
    configurable: true,
    value: FakeElement,
  });
});

afterAll(() => {
  if (!hadOriginalElement) {
    delete (globalThis as { Element?: typeof Element }).Element;
    return;
  }

  Object.defineProperty(globalThis, 'Element', {
    configurable: true,
    value: originalElement,
  });
});

function createFakeElement(tagName: string, className?: string): FakeElement {
  const element = new FakeElement(tagName);
  if (className) {
    element.addClass(className);
  }
  return element;
}

function matchesGroupDragSelector(element: FakeElement, selector: string): boolean {
  return selector
    .split(',')
    .map((part) => part.trim())
    .some((part) => {
      if (part.startsWith('.')) {
        return element.classNames.has(part.slice(1));
      }
      if (part === "[contenteditable='true']") {
        return element.attributes.get('contenteditable') === 'true';
      }
      return element.tagName.toLowerCase() === part;
    });
}

describe('shouldTreatProjectAsEmptySessionGroup', () => {
  test('identifies an empty project group so it can render a new-session row', () => {
    expect(
      shouldTreatProjectAsEmptySessionGroup({
        hasProjectContext: true,
        sessionCount: 0,
      })
    ).toBe(true);
  });

  test('does not treat non-project or non-empty groups as empty project groups', () => {
    expect(
      shouldTreatProjectAsEmptySessionGroup({
        hasProjectContext: false,
        sessionCount: 0,
      })
    ).toBe(false);
    expect(
      shouldTreatProjectAsEmptySessionGroup({
        hasProjectContext: true,
        sessionCount: 1,
      })
    ).toBe(false);
  });
});

describe('empty project new-session row', () => {
  test('uses the requested visible button label', () => {
    expect(getEmptyProjectNewSessionButtonLabel()).toBe('New Session');
  });

  test('renders as a sleeping session-shaped button without a last-active timestamp', () => {
    /*
     * CDXC:ProjectGroups 2026-06-15-20:14:
     * Closing the last terminal leaves an empty project row. Source coverage
     * keeps that row session-shaped, timestamp-free, and wired to create a new
     * terminal instead of restoring the closed session.
     */
    const rowStart = sessionGroupSectionSource.indexOf("data-empty-project-new-session-row='true'");
    expect(rowStart).toBeGreaterThan(-1);
    const rowSource = sessionGroupSectionSource.slice(Math.max(0, rowStart - 600), rowStart + 1400);

    expect(rowSource).toContain('group-empty-project-session-button');
    expect(rowSource).toContain("data-sleeping='true'");
    expect(rowSource).toContain("data-title-full-width='true'");
    expect(rowSource).toContain('getEmptyProjectNewSessionButtonLabel()');
    expect(rowSource).toContain('requestCreateProjectTerminal();');
    expect(rowSource).not.toContain('session-last-interaction-time');
  });
});

describe('formatProjectEditorDiffStatsLabel', () => {
  test('formats the compact changed-lines summary by default', () => {
    expect(
      formatProjectEditorDiffStatsLabel({
        additions: 9,
        deletions: 11,
        files: 1,
        isLoading: false,
        isRepo: true,
      })
    ).toBe('+9 -11');
  });

  test('caps the compact project diff counts for stable sidebar width', () => {
    expect(
      formatProjectEditorDiffStatsLabel({
        additions: 12000,
        deletions: 10001,
        files: 120,
        isLoading: false,
        isRepo: true,
      })
    ).toBe('+9999 -9999');
  });

  test('includes the capped file count when enabled', () => {
    expect(
      formatProjectEditorDiffStatsLabel(
        {
          additions: 12000,
          deletions: 10001,
          files: 120,
          isLoading: false,
          isRepo: true,
        },
        true
      )
    ).toBe('99 +9999 -9999');
  });
});

describe('formatProjectTooltipGitStats', () => {
  test('pluralizes changed files and changed lines in project and worktree tooltips', () => {
    expect(
      formatProjectTooltipGitStats({
        additions: 2,
        deletions: 6,
        files: 2,
        isLoading: false,
        isRepo: true,
      })
    ).toBe('2 files changed  +2  -6 lines');
  });

  test('uses singular copy for one changed file and one added line', () => {
    expect(
      formatProjectTooltipGitStats({
        additions: 1,
        deletions: 0,
        files: 1,
        isLoading: false,
        isRepo: true,
      })
    ).toBe('1 file changed  +1  -0 line');
  });

  test('uses singular line copy when the only changed line is a deletion', () => {
    expect(
      formatProjectTooltipGitStats({
        additions: 0,
        deletions: 1,
        files: 4,
        isLoading: false,
        isRepo: true,
      })
    ).toBe('4 files changed  +0  -1 line');
  });
});

describe('shouldShowProjectEditorDiffStats', () => {
  test('hides the project git status when additions and deletions are both zero', () => {
    expect(
      shouldShowProjectEditorDiffStats({
        additions: 0,
        deletions: 0,
        files: 0,
        isLoading: false,
        isRepo: true,
      })
    ).toBe(false);
  });

  test('shows the project git status when additions are nonzero', () => {
    expect(
      shouldShowProjectEditorDiffStats({
        additions: 1,
        deletions: 0,
        files: 1,
        isLoading: false,
        isRepo: true,
      })
    ).toBe(true);
  });

  test('shows the project git status when deletions are nonzero', () => {
    expect(
      shouldShowProjectEditorDiffStats({
        additions: 0,
        deletions: 1,
        files: 1,
        isLoading: false,
        isRepo: true,
      })
    ).toBe(true);
  });
});

describe('project row Actions', () => {
  test('renders Global Actions flagged for the project row, not only project ones', () => {
    /*
     * CDXC:GlobalActions 2026-08-07:
     * Global Actions live in their own HUD list, so a row that reads only
     * commandsByProject leaves the "Show on the project's sidebar row" toggle
     * dead for every global. Merge both lists, globals first like Settings.
     */
    const rowCommandsStart = sessionGroupSectionSource.indexOf('const projectRowCommands = useMemo(');
    const rowCommandsSource = sessionGroupSectionSource.slice(rowCommandsStart, rowCommandsStart + 600);

    expect(rowCommandsStart).toBeGreaterThan(-1);
    expect(sessionGroupSectionSource).toContain(
      'const globalCommands = useSidebarStore((state) => state.hud.globalCommands);'
    );
    expect(rowCommandsSource).toContain(
      "...(globalCommands ?? []).map((command) => ({ command, scope: 'global' }) as const)"
    );
    expect(rowCommandsSource).toContain(
      "...(projectCommands ?? []).map((command) => ({ command, scope: 'project' }) as const)"
    );
    expect(rowCommandsSource).toContain('.filter((entry) => entry.command.showOnProjectRow)');
    expect(rowCommandsSource.indexOf('globalCommands ?? []')).toBeLessThan(
      rowCommandsSource.indexOf('projectCommands ?? []')
    );
  });

  test('keys and runs row Actions by scope so the two id spaces cannot collide', () => {
    /*
     * CDXC:GlobalActions 2026-08-07:
     * Global and project Action ids are separate spaces, so the click must name
     * the list to resolve against and the React key must stay unique when both
     * lists hold the same id. The group id keeps naming the project to run in.
     */
    const runStart = sessionGroupSectionSource.indexOf('const requestRunProjectRowCommand = (');
    const runSource = sessionGroupSectionSource.slice(runStart, runStart + 900);
    const buttonStart = sessionGroupSectionSource.indexOf('projectRowCommands.map(({ command, scope }) => {');
    const buttonSource = sessionGroupSectionSource.slice(buttonStart, buttonStart + 900);

    expect(runStart).toBeGreaterThan(-1);
    expect(buttonStart).toBeGreaterThan(-1);
    expect(runSource).toContain('(command: SidebarCommandButton, scope: SidebarCommandScope) =>');
    expect(runSource).toContain('groupId: group.groupId,');
    expect(runSource).toContain('scope,');
    expect(buttonSource).toContain('key={`${scope}:${command.commandId}`}');
    expect(buttonSource).toContain('requestRunProjectRowCommand(command, scope);');
  });
});

describe('project diff stats refresh triggers', () => {
  test('does not refresh project git stats from header hover', () => {
    /*
     * CDXC:ProjectDiffStats 2026-06-30-19:13:
     * Project-header Git stats are background data. Hovering the project header
     * must not post refreshWorkspaceProjectDiffForGroup or otherwise tie Git
     * probes to pointer movement.
     */
    const headerStart = sessionGroupSectionSource.indexOf("className='group-head'");
    const headerSource = sessionGroupSectionSource.slice(headerStart, headerStart + 900);

    expect(headerStart).toBeGreaterThan(-1);
    expect(headerSource).not.toContain('onMouseEnter');
    expect(sessionGroupSectionSource).not.toContain('const refreshProjectDiffStats = () => {');
    expect(sessionGroupSectionSource).not.toContain("type: 'refreshWorkspaceProjectDiffForGroup'");
  });
});

describe('getGroupContextMenuItemCount', () => {
  test('counts compact worktree project actions with copy path instead of open', () => {
    expect(
      getGroupContextMenuItemCount({
        canFullReloadGroup: true,
        hasProjectContext: true,
        isWorktreeProject: true,
      })
    ).toBe(5);
  });

  test('worktree rows offer Rename Worktree and not the dead label-only Rename', () => {
    /*
     * CDXC:WorktreeRename 2026-08-10:
     * Two invariants in one place, because they are the same mistake.
     *
     * The label-only Rename posts `renameWorkspaceProjectForGroup`, which the
     * GPUI runtime has no case for — on the desktop app it did nothing, sitting
     * directly above a Rename Worktree that does. It must not come back for
     * worktree rows, while ordinary project rows keep theirs.
     *
     * And getGroupContextMenuItemCount counts the items between these anchors,
     * which drives viewport clamping. Anchoring on POSITION rather than mere
     * presence is the point: a test that only checked the item exists would pass
     * while the count drifted and the last menu item opened off-screen.
     */
    const menuStart = sessionGroupSectionSource.indexOf('CDXC:WorktreeDelete 2026-05-28-07:46');
    // The slice has to reach the END of the worktree branch, not its first
    // shared item: anchored on "Add to project group" it stopped short of Delete
    // Worktree and Remove Worktree, so a reintroduced label-only Rename placed
    // below that item would have passed the negative assertions untouched.
    const menuEnd = sessionGroupSectionSource.indexOf('Remove Worktree', menuStart);
    const collectionsAnchor = sessionGroupSectionSource.indexOf('Add to project group', menuStart);
    expect(menuStart).toBeGreaterThanOrEqual(0);
    expect(collectionsAnchor).toBeGreaterThan(menuStart);
    expect(menuEnd).toBeGreaterThan(collectionsAnchor);
    const worktreeMenuSource = sessionGroupSectionSource.slice(menuStart, menuEnd);

    expect(worktreeMenuSource).toContain('Rename Worktree');
    expect(worktreeMenuSource).toContain('promptRenameWorktree');
    expect(worktreeMenuSource).not.toContain('IconPencil');
    expect(worktreeMenuSource).not.toContain('setIsEditing(true)');
    expect(sessionGroupSectionSource).toContain("type: 'promptRenameWorktreeForGroup'");
    // Ordinary project rows are untouched — they still rename their label.
    expect(sessionGroupSectionSource.slice(menuEnd)).toContain('IconPencil');
  });

  test('counts normal project and group actions separately', () => {
    expect(
      getGroupContextMenuItemCount({
        canFullReloadGroup: true,
        hasProjectContext: true,
        isWorktreeProject: false,
      })
    ).toBe(6);
    expect(
      getGroupContextMenuItemCount({
        canFullReloadGroup: false,
        hasProjectContext: false,
        isWorktreeProject: false,
      })
    ).toBe(3);
  });

  test('counts the Hide item both project menus render', () => {
    /*
     * CDXC:SidebarContextMenu 2026-08-10:
     * Hide/Unhide is rendered by the worktree AND the repository project menu
     * whenever onHideGroup is supplied, and the sidebar always supplies it, so
     * leaving it out measured every project menu one row short. The group menu
     * has no Hide item, so it must not gain one here.
     */
    expect(
      getGroupContextMenuItemCount({
        canFullReloadGroup: false,
        canHideGroup: true,
        hasProjectContext: true,
        isWorktreeProject: true,
      })
    ).toBe(6);
    expect(
      getGroupContextMenuItemCount({
        canFullReloadGroup: true,
        canHideGroup: true,
        hasProjectContext: true,
        isWorktreeProject: false,
      })
    ).toBe(7);
    expect(
      getGroupContextMenuItemCount({
        canFullReloadGroup: true,
        canHideGroup: true,
        hasProjectContext: false,
        isWorktreeProject: false,
      })
    ).toBe(4);
  });
});

describe('getSidebarSessionGapContextMenuTarget', () => {
  const sessionRows = [
    { bottom: 34, element: 'session-1', top: 0 },
    { bottom: 69, element: 'session-2', top: 36 },
    { bottom: 104, element: 'session-3', top: 71 },
  ] as const;

  test('routes a session-to-session gap to the row above it', () => {
    expect(
      getSidebarSessionGapContextMenuTarget({
        clientY: 35,
        sessionRows,
      })
    ).toBe('session-1');

    expect(
      getSidebarSessionGapContextMenuTarget({
        clientY: 70,
        sessionRows,
      })
    ).toBe('session-2');
  });

  test('does not route row interiors or outer project body space', () => {
    expect(
      getSidebarSessionGapContextMenuTarget({
        clientY: 20,
        sessionRows,
      })
    ).toBeUndefined();
    expect(
      getSidebarSessionGapContextMenuTarget({
        clientY: -1,
        sessionRows,
      })
    ).toBeUndefined();
    expect(
      getSidebarSessionGapContextMenuTarget({
        clientY: 110,
        sessionRows,
      })
    ).toBeUndefined();
  });

  test('keeps the project context menu attached to the header instead of the group body', () => {
    /*
     * CDXC:SidebarContextMenu 2026-06-19-10:46:
     * Right-clicking project body gaps must not open the project context menu.
     * The group body owns only session-gap retargeting, while the header owns
     * the project menu.
     */
    const sectionStart = sessionGroupSectionSource.indexOf("<section\n        className='group'");
    const groupHeadStart = sessionGroupSectionSource.indexOf("className='group-head'", sectionStart);
    const groupSessionsStart = sessionGroupSectionSource.indexOf(
      "className='group-sessions sidebar-collapse-content'",
      groupHeadStart
    );

    expect(sectionStart).toBeGreaterThan(-1);
    expect(groupHeadStart).toBeGreaterThan(sectionStart);
    expect(groupSessionsStart).toBeGreaterThan(groupHeadStart);
    expect(sessionGroupSectionSource.slice(sectionStart, groupHeadStart)).not.toContain('onContextMenu=');
    expect(sessionGroupSectionSource.slice(groupHeadStart, groupSessionsStart)).toContain(
      'onContextMenu={handleGroupHeaderContextMenu}'
    );
    expect(sessionGroupSectionSource.slice(groupSessionsStart, groupSessionsStart + 600)).toContain(
      'onContextMenu={handleGroupSessionsContextMenu}'
    );
  });
});

describe('reference sidebar group spacing styles', () => {
  test('keeps project headers normal-flow for fast sidebar scrolling', () => {
    /*
     * CDXC:SidebarScroll 2026-06-30-01:59:
     * Sidebar scrolling must prioritize throughput over sticky project context.
     * Keep project headers in normal flow and do not keep the fixed-gradient
     * geometry observer or sticky background contract that made scroll paint
     * compositor-bound.
     */
    const projectHeaderStart = sessionGroupStylesSource.indexOf(
      ".sidebar-reference-layout[data-reference-sidebar='true'] .group[data-project-group='true'] .group-head {"
    );
    const projectHeaderSource = sessionGroupStylesSource.slice(projectHeaderStart, projectHeaderStart + 5200);

    expect(projectHeaderStart).toBeGreaterThan(-1);
    expect(projectHeaderSource).toContain('position: relative;');
    expect(projectHeaderSource).not.toContain('position: sticky;');
    expect(projectHeaderSource).not.toContain('background-attachment:');
    expect(projectHeaderSource).not.toContain('background-position:');
    expect(projectHeaderSource).not.toContain('background-size:');
    expect(groupPanelStylesSource).toContain('--reference-sidebar-background: var(--app-background);');
    expect(groupPanelStylesSource).not.toContain('--reference-sidebar-background-attachment:');
    expect(groupPanelStylesSource).not.toContain('--reference-sidebar-gradient-top');
    expect(groupPanelStylesSource).not.toContain('--reference-sidebar-gradient-height');
    expect(sidebarAppSource).not.toContain('referenceSidebarLayoutRef');
    expect(sidebarAppSource).not.toContain('--reference-sidebar-gradient-top');
    expect(sidebarAppSource).not.toContain('--reference-sidebar-gradient-height');
    expect(sessionGroupSectionSource).not.toContain('updateStickyHeaderClip');
    expect(sessionGroupSectionSource).not.toContain('reference-project-session-clip-top');
    expect(sessionGroupStylesSource).not.toContain('reference-project-session-clip-top');
    expect(sessionGroupStylesSource).not.toContain('clip-path: inset(var(--reference-project');
  });

  test('does not force the project to the top when Show less is selected', () => {
    /*
     * CDXC:ProjectSessionLists 2026-06-25-22:28:
     * The project session-list toggle should preserve the outer sidebar scroll
     * viewport. A local Show less state change must not call scrollIntoView and
     * snap the project header to the top of the sidebar.
     */
    const toggleStart = sessionGroupSectionSource.indexOf('const toggleProjectSessionListCollapsed = () => {');
    const toggleSource = sessionGroupSectionSource.slice(toggleStart, toggleStart + 1200);

    expect(toggleStart).toBeGreaterThan(-1);
    expect(toggleSource).toContain('onProjectSessionListCollapsedChange?.(');
    expect(toggleSource).not.toContain('scrollIntoView');
  });

  test('keeps expanded project session lists as plain bounded scroll surfaces', () => {
    /*
     * CDXC:ProjectSessionLists 2026-06-25-12:20:
     * The Show more state should keep rendering all project sessions, but the
     * expanded body must become a bounded inner scroll area using plain vertical
     * overflow instead of scroll masks.
     *
     * CDXC:ProjectSessionLists 2026-06-29-17:53:
     * Inner project scrolling should chain to the main sidebar when the nested
     * list reaches an edge, so this rule must not contain overscroll.
     *
     * CDXC:SidebarScroll 2026-06-30-01:59:
     * The fast sidebar path removes scroll masks and per-scroll glow state from
     * expanded project bodies.
     */
    expect(sessionGroupSectionSource).toContain('shouldScrollExpandedProjectSessionList');
    expect(sessionGroupSectionSource).toContain('getExpandedProjectSessionListScrollHeight');
    expect(sessionGroupSectionSource).toContain(
      'const projectSessionListRenderedSessionIdsKey = shouldClipProjectSessionList'
    );
    expect(sessionGroupSectionSource).not.toContain('setExpandedProjectSessionListScrollHeight');
    expect(sessionGroupSectionSource).not.toContain('projectSessionListScrollBoundarySessionId');
    expect(sessionGroupSectionSource).not.toContain('vertical-scroll-fade-mask');
    expect(sessionGroupSectionSource).not.toContain('data-scroll-glow');
    expect(sessionGroupSectionSource).toContain(
      'data-project-session-list-scrollable={String(shouldScrollExpandedProjectSessionList)}'
    );

    /*
     * CDXC:SidebarCollapseAnimation 2026-08-19:
     * The bounded inner scroll surface is scoped to the expanded body, so the
     * collapsed rule's max-height is not overridden by a later rule at the same
     * specificity. Match that exact selector; the unscoped prefix also appears
     * on the drag-lock and ::-webkit-scrollbar rules below it.
     */
    const scrollableRuleStart = groupPanelStylesSource.indexOf(
      ".group-sessions-shell[data-project-session-list-scrollable='true'][data-collapsed='false'] {"
    );
    const scrollableRuleEnd = groupPanelStylesSource.indexOf('\n}\n', scrollableRuleStart);
    const scrollableRuleSource = groupPanelStylesSource.slice(scrollableRuleStart, scrollableRuleEnd);

    expect(scrollableRuleStart).toBeGreaterThan(-1);
    expect(scrollableRuleEnd).toBeGreaterThan(scrollableRuleStart);
    expect(scrollableRuleSource).toContain('overflow-x: hidden;');
    expect(scrollableRuleSource).toContain('overflow-y: auto;');
    expect(scrollableRuleSource).toContain('overscroll-behavior: none;');
    expect(scrollableRuleSource).not.toContain('--edge-fade-distance:');
    expect(groupPanelStylesSource).not.toContain('--top-fade: var(--edge-fade-distance);');
    expect(groupPanelStylesSource).not.toContain('--bottom-fade: var(--edge-fade-distance);');
  });

  test('does not slice below-session arrays in the project row render loop', () => {
    /*
     * CDXC:SidebarContextMenu 2026-06-30-02:45:
     * Large project lists should keep manual drag ordering mounted, but row
     * rendering must not build a per-card below-session slice. Pass a shared
     * visible-id source and the row's next index so context-menu actions can
     * materialize the slice only when the menu opens.
     */
    const rowLoopStart = sessionGroupSectionSource.indexOf('{renderedSessionIds.map((sessionId, sessionIndex) => {');
    const rowLoopEnd = sessionGroupSectionSource.indexOf('{projectSessionListHiddenCount > 0 ? (', rowLoopStart);
    const rowLoopSource = sessionGroupSectionSource.slice(rowLoopStart, rowLoopEnd);

    expect(rowLoopStart).toBeGreaterThan(-1);
    expect(rowLoopEnd).toBeGreaterThan(rowLoopStart);
    expect(rowLoopSource).toContain('sessionIdsBelowSource={visibleSessionIds}');
    expect(rowLoopSource).toContain('sessionIdsBelowStartIndex={sessionIdsBelowStartIndex}');
    expect(rowLoopSource).not.toContain('visibleSessionIds.indexOf(sessionId)');
    expect(rowLoopSource).not.toContain('visibleSessionIds.slice(');
  });

  test('uses row-owned padding instead of blank gaps between project headers and sessions', () => {
    /*
     * CDXC:ReferenceSidebar 2026-06-19-10:52:
     * Sidebar project headers and session rows should not have empty visual
     * gaps between buttons. Keep project-header breathing room inside the
     * clickable row and leave session drag indicators row-owned.
     */
    const projectListStart = sessionGroupStylesSource.indexOf(
      ".sidebar-reference-layout[data-reference-sidebar='true'] .reference-project-group-list {"
    );
    const projectHeaderStart = sessionGroupStylesSource.indexOf(
      ".sidebar-reference-layout[data-reference-sidebar='true'] .group[data-project-group='true'] .group-head {",
      projectListStart
    );
    const projectHeaderSource = sessionGroupStylesSource.slice(projectHeaderStart, projectHeaderStart + 5200);
    const groupSessionsStart = sessionGroupStylesSource.indexOf(
      ".sidebar-reference-layout[data-reference-sidebar='true'] .group-sessions {",
      projectHeaderStart
    );
    const groupSessionsSource = sessionGroupStylesSource.slice(groupSessionsStart, groupSessionsStart + 500);

    expect(projectListStart).toBeGreaterThan(-1);
    expect(projectHeaderStart).toBeGreaterThan(projectListStart);
    expect(groupSessionsStart).toBeGreaterThan(projectHeaderStart);
    expect(sessionGroupStylesSource.slice(projectListStart, projectHeaderStart)).toContain('row-gap: 0;');
    expect(projectHeaderSource).toContain('padding-bottom: 5.5px;');
    expect(projectHeaderSource).toContain('padding-top: 5.5px;');
    expect(groupSessionsSource).toContain('gap: 0;');
    expect(groupSessionsSource).not.toContain('gap: 1px;');
  });

  test('keeps session drag feedback to insertion lines instead of project highlights', () => {
    /*
     * CDXC:SidebarDragDrop 2026-06-19-11:12:
     * Dropping sessions onto projects should not tint the project body. The
     * only visible destination cue should be the insertion line.
     */
    const emptyTargetStart = sessionGroupStylesSource.indexOf(
      ".group-empty-drop-target[data-drop-target='true'] .group-empty-state {"
    );
    const emptyTargetSource = sessionGroupStylesSource.slice(emptyTargetStart, emptyTargetStart + 700);

    expect(sessionGroupStylesSource).not.toContain('.group-sessions[data-drop-target="true"] {\n  background:');
    expect(sessionGroupStylesSource).not.toContain(".group:has(.session[data-drop-target='true']) .group-sessions");
    expect(emptyTargetStart).toBeGreaterThan(-1);
    expect(emptyTargetSource).toContain('background: transparent;');
    expect(emptyTargetSource).toContain('box-shadow: none;');
  });
});

describe('shouldPreventGroupDragActivation', () => {
  test('allows drag activation from the project header surface and title', () => {
    const header = createFakeElement('div', 'group-head');
    const titleButton = createFakeElement('button', 'group-title-button');
    const titleText = createFakeElement('span', 'group-title');
    const spacer = createFakeElement('div', 'group-title-spacer');
    titleButton.append(titleText);
    header.append(titleButton, spacer);

    expect(shouldPreventGroupDragActivation(titleText as unknown as EventTarget, header as unknown as Element)).toBe(
      false
    );
    expect(shouldPreventGroupDragActivation(spacer as unknown as EventTarget, header as unknown as Element)).toBe(
      false
    );
  });

  test('keeps project header controls out of drag activation', () => {
    const header = createFakeElement('div', 'group-head');
    const actionCluster = createFakeElement('div', 'group-header-actions');
    const actionButton = createFakeElement('button', 'group-add-button');
    const titleInput = createFakeElement('input', 'group-title-input');
    actionCluster.append(actionButton);
    header.append(actionCluster, titleInput);

    expect(shouldPreventGroupDragActivation(actionButton as unknown as EventTarget, header as unknown as Element)).toBe(
      true
    );
    expect(shouldPreventGroupDragActivation(titleInput as unknown as EventTarget, header as unknown as Element)).toBe(
      true
    );
  });

  test('ignores blocked-looking targets outside the drag surface', () => {
    const header = createFakeElement('div', 'group-head');
    const externalActionCluster = createFakeElement('div', 'group-header-actions');
    const externalActionButton = createFakeElement('button', 'group-add-button');
    externalActionCluster.append(externalActionButton);

    expect(
      shouldPreventGroupDragActivation(externalActionButton as unknown as EventTarget, header as unknown as Element)
    ).toBe(false);
  });
});

describe('getPinnedSessionDropGapKey', () => {
  const visibleSessionIds = ['first', 'second', 'third'];

  test('maps before the first pinned target to the first visible gap', () => {
    expect(
      getPinnedSessionDropGapKey({
        dropTarget: {
          groupId: 'project',
          kind: 'session',
          position: 'before',
          sessionId: 'first',
        },
        groupId: 'project',
        visibleSessionIds,
      })
    ).toBe('before:first');
  });

  test('maps after a row to the next visible gap instead of a row pseudo-element', () => {
    expect(
      getPinnedSessionDropGapKey({
        dropTarget: {
          groupId: 'project',
          kind: 'session',
          position: 'after',
          sessionId: 'first',
        },
        groupId: 'project',
        visibleSessionIds,
      })
    ).toBe('before:second');
  });

  test('maps after the final row to the stable trailing gap', () => {
    expect(
      getPinnedSessionDropGapKey({
        dropTarget: {
          groupId: 'project',
          kind: 'session',
          position: 'after',
          sessionId: 'third',
        },
        groupId: 'project',
        visibleSessionIds,
      })
    ).toBe(PINNED_SESSION_DROP_GAP_AFTER_LAST);
  });

  test('ignores targets for another group', () => {
    expect(
      getPinnedSessionDropGapKey({
        dropTarget: {
          groupId: 'other',
          kind: 'session',
          position: 'before',
          sessionId: 'first',
        },
        groupId: 'project',
        visibleSessionIds,
      })
    ).toBeUndefined();
  });
});
