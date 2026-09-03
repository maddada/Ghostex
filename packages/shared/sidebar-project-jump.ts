export const SIDEBAR_PROJECT_JUMP_EVENT = 'ghostex-sidebar-project-jump';

export type SidebarProjectJumpEventDetail = {
  expandCollapsedProject: boolean;
  groupId: string;
  projectId: string;
  /**
   * CDXC:Sessions 2026-06-16-07:55:
   * Add Project and Add Worktree launch flows need the same Projects-area reveal
   * event as project jumps, plus an explicit retry of the focused-session row
   * scroll after React expands the target project.
   */
  revealFocusedSession?: boolean;
  showLessAfterExpand: boolean;
};
