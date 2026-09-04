export type SidebarSplitSessionRightMessage = {
  /**
   * CDXC:Workarea 2026-09-04 DECISION:
   * User: with the tabs bar hidden on unsplit workspaces, the sidebar
   * session menu (Advanced > Split Right) is how a pane gets split: open
   * this session in a new pane to the right of the focused agents pane.
   */
  type: 'splitSessionRight';
  sessionId: string;
};
