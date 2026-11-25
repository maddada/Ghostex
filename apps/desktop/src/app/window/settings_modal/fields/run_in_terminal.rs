//! Run in terminal beside a failed install (cua-driver, SpaceO, Hex, the agent CLIs): the same
//! button the Linux update card shows (tabs/updates/card.rs), asking that computer's gxserver for a
//! Main Hub terminal with the install command typed in and not run.
//!
//! SEE-ALSO: server/src/shown_commands/installs.rs (the ids and the CDXC:ManagedTools 2026-11-25 decision), apps/desktop/src/app/gx_store/create/shown_command_terminal.rs (what the click opens).
use super::super::palette::SettingsPalette;
use super::super::store::{SettingsStore, post_store_message};
use super::controls::{ButtonSize, ButtonVariant, settings_button_sized};
use gpui::{AnyElement, Context, ElementId, Entity};
use serde_json::{Value, json};

/// The id of an install this computer's desktop runs itself (`trycua`, `spaceo`, `hex`), which
/// this computer's gxserver resolves to the same official command; `None` on Windows, where the
/// installers are PowerShell commands (installs.rs).
pub(crate) fn local_tool_install_id(tool: &str) -> Option<String> {
    (!cfg!(target_os = "windows")).then(|| format!("toolInstall:{tool}"))
}

/// The button: `command_id` is one gxserver advertised on the failed job (or
/// [`local_tool_install_id`]), `machine_id` a saved remote machine's id, `None` for this computer.
pub(crate) fn run_in_terminal_button<V: 'static>(
    p: &SettingsPalette,
    id: impl Into<ElementId>,
    store: &Entity<SettingsStore>,
    command_id: String,
    machine_id: Option<String>,
    cx: &mut Context<V>,
) -> AnyElement {
    run_in_terminal_button_with_args(p, id, store, command_id, machine_id, None, cx)
}

/// [`run_in_terminal_button`] for an id that takes arguments (`teamDeploy`'s `workspaceId`), which
/// gxserver checks again before it types anything.
pub(crate) fn run_in_terminal_button_with_args<V: 'static>(
    p: &SettingsPalette,
    id: impl Into<ElementId>,
    store: &Entity<SettingsStore>,
    command_id: String,
    machine_id: Option<String>,
    args: Option<Value>,
    cx: &mut Context<V>,
) -> AnyElement {
    let store = store.clone();
    let mut message = json!({
        "type": "openShownCommandTerminal",
        "commandId": command_id,
        "machineId": machine_id,
    });
    if let Some(args) = args {
        message["args"] = args;
    }
    settings_button_sized(
        p,
        id,
        "Run in terminal",
        None,
        ButtonVariant::Default,
        ButtonSize::Xs,
        false,
        None,
        move |_view: &mut V, _window, cx| post_store_message(&store, message.clone(), cx),
        cx,
    )
}
