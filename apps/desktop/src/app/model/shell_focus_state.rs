// C1 wave-3 re-cluster: shell-state persistence for the saved workspace/command focus target, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).

use crate::*;

pub(crate) fn shell_focus_to_shell_state_json(focus: ShellFocusTarget) -> serde_json::Value {
    match focus {
        ShellFocusTarget::AgentsPane(pane_id) => {
            serde_json::json!({ "type": "agents-pane", "paneId": pane_id.0 })
        }
        ShellFocusTarget::CommandPane => serde_json::json!({ "type": "command-pane" }),
        ShellFocusTarget::BrowserSurface => serde_json::json!({ "type": "browser-surface" }),
        ShellFocusTarget::BrowserPane(pane_id) => {
            serde_json::json!({ "type": "browser-pane", "paneId": pane_id.0 })
        }
        ShellFocusTarget::ProjectEditorSurface(mode) => serde_json::json!({
            "type": "project-editor-surface",
            "mode": mode.element_slug(),
        }),
        ShellFocusTarget::ProjectEditorCompanion(mode) => serde_json::json!({
            "type": "project-editor-companion",
            "mode": mode.element_slug(),
        }),
    }
}

pub(crate) fn shell_focus_from_shell_state(value: &serde_json::Value) -> Option<ShellFocusTarget> {
    let object = value.as_object()?;
    match json_string_field(object, "type")? {
        "agents-pane" => Some(ShellFocusTarget::AgentsPane(WorkspacePaneId(
            json_u64_field(object, "paneId")?,
        ))),
        "command-pane" => Some(ShellFocusTarget::CommandPane),
        "browser-surface" => Some(ShellFocusTarget::BrowserSurface),
        "browser-pane" => Some(ShellFocusTarget::BrowserPane(BrowserPaneId(
            json_u64_field(object, "paneId")?,
        ))),
        "project-editor-surface" => json_string_field(object, "mode")
            .and_then(TitlebarMode::from_slug)
            .map(ShellFocusTarget::ProjectEditorSurface),
        "project-editor-companion" => json_string_field(object, "mode")
            .and_then(TitlebarMode::from_slug)
            .map(ShellFocusTarget::ProjectEditorCompanion),
        _ => None,
    }
}

pub(crate) fn valid_shell_focus_or_default_with_browser_tabs(
    focus: ShellFocusTarget,
    active_mode: TitlebarMode,
    agents_workspace: &WorkspaceModel,
    command_pane: &CommandPaneModel,
    project_editor_shell: &ProjectEditorShellModel,
    browser_tabs: &BrowserTabModel,
) -> ShellFocusTarget {
    if let Some(focus) = valid_non_command_shell_focus_with_browser_tabs(
        focus,
        active_mode,
        agents_workspace,
        project_editor_shell,
        browser_tabs,
    ) {
        return focus;
    }

    match focus {
        ShellFocusTarget::CommandPane if command_pane.has_sessions() => focus,
        _ => default_shell_focus_for_mode(active_mode, agents_workspace, project_editor_shell),
    }
}

pub(crate) fn valid_non_command_shell_focus_with_browser_tabs(
    focus: ShellFocusTarget,
    active_mode: TitlebarMode,
    agents_workspace: &WorkspaceModel,
    project_editor_shell: &ProjectEditorShellModel,
    browser_tabs: &BrowserTabModel,
) -> Option<ShellFocusTarget> {
    match focus {
        ShellFocusTarget::AgentsPane(pane_id)
            if active_mode == TitlebarMode::Agents
                && agents_workspace.find_leaf(pane_id).is_some() =>
        {
            Some(focus)
        }
        ShellFocusTarget::BrowserSurface if active_mode == TitlebarMode::Browser => Some(focus),
        ShellFocusTarget::BrowserPane(pane_id)
            if active_mode == TitlebarMode::Browser
                && browser_tabs.find_leaf(pane_id).is_some() =>
        {
            Some(focus)
        }
        ShellFocusTarget::ProjectEditorSurface(mode)
            if active_mode == mode
                && matches!(
                    mode,
                    TitlebarMode::Source
                        | TitlebarMode::Kanban
                        | TitlebarMode::Automate
                        | TitlebarMode::Manage
                ) =>
        {
            Some(focus)
        }
        ShellFocusTarget::ProjectEditorCompanion(mode)
            if active_mode == mode
                && project_editor_shell.left_companion_visible
                && matches!(
                    mode,
                    TitlebarMode::Source
                        | TitlebarMode::Browser
                        | TitlebarMode::Kanban
                        | TitlebarMode::Automate
                        | TitlebarMode::Manage
                ) =>
        {
            Some(focus)
        }
        ShellFocusTarget::CommandPane
        | ShellFocusTarget::AgentsPane(_)
        | ShellFocusTarget::BrowserSurface
        | ShellFocusTarget::BrowserPane(_)
        | ShellFocusTarget::ProjectEditorSurface(_)
        | ShellFocusTarget::ProjectEditorCompanion(_) => None,
    }
}

pub(crate) fn default_shell_focus_for_mode(
    active_mode: TitlebarMode,
    agents_workspace: &WorkspaceModel,
    _project_editor_shell: &ProjectEditorShellModel,
) -> ShellFocusTarget {
    match active_mode {
        TitlebarMode::Agents => ShellFocusTarget::AgentsPane(agents_workspace.focused_pane),
        TitlebarMode::Browser => ShellFocusTarget::BrowserSurface,
        TitlebarMode::Source
        | TitlebarMode::Kanban
        | TitlebarMode::Automate
        | TitlebarMode::Manage
        | TitlebarMode::Extension(_) => ShellFocusTarget::ProjectEditorSurface(active_mode),
    }
}

pub(crate) fn restored_non_command_shell_focus_or_default_with_browser_tabs(
    previous_non_command_focus: Option<ShellFocusTarget>,
    active_mode: TitlebarMode,
    agents_workspace: &WorkspaceModel,
    project_editor_shell: &ProjectEditorShellModel,
    browser_tabs: &BrowserTabModel,
) -> ShellFocusTarget {
    /*
    CDXC:GPUIKeyboardFocus 2026-06-25-19:27:
    Command-panel collapse may restore only a currently valid non-command focus target. Stale panes, hidden companions, wrong workarea modes, and command-pane focus fall back through the active shell mode instead of persisting unusable keyboard ownership.
    */
    previous_non_command_focus
        .and_then(|focus| {
            valid_non_command_shell_focus_with_browser_tabs(
                focus,
                active_mode,
                agents_workspace,
                project_editor_shell,
                browser_tabs,
            )
        })
        .unwrap_or_else(|| {
            default_shell_focus_for_mode(active_mode, agents_workspace, project_editor_shell)
        })
}
