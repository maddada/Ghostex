/**
 * CDXC:Workarea 2026-09-04 DECISION:
 * User: Advanced > Split Right opens a sidebar session in a pane to the right
 * of the focused agents pane. The workspace focus bridge carries it as an
 * optional `placement`; absent means the ordinary tab placement.
 * SEE-ALSO: `gpui_sidebar_workspace_terminal_focus_from_value` in
 * apps/desktop/src/app/helpers/sidebar/workspace_terminal_actions.rs.
 */
export type GpuiWorkspaceTerminalFocusPlacement = 'splitRight';
