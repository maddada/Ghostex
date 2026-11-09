//! A project's Clone on…: clone its git remote onto a connected machine that does not have it yet.
//!
//! CDXC:BackgroundJobs 2026-11-09 DECISION:
//! User: "if there's a project that's only on another machine, please allow me to clone it on any machine i want from my connected machines (in the context menu)". The one-list Project sidebar knows which machines have each project (`GroupCore::machine_names`, `merged.rs`), so the project menu offers Clone on… with this computer and every saved machine that lacks it, a project here included; a machine that is not connected is listed greyed with "Not connected", and a project with no git remote gets the row greyed with "No git remote". The clone goes to that machine's usual projects folder and shows in the header's jobs button (apps/desktop/src/app/background_jobs/clone_on.rs). Separate by machine keeps each machine's projects apart, so it has no Clone on….

use serde_json::json;

use crate::sidebar_view::{GroupView, SidebarInputs, LOCAL_MACHINE_ID};

use super::commands::MenuCommand;
use super::item::MenuItem;

/// The top-level command a Clone on… row sends; the desktop answers it
/// (`clone_project_on_machine`).
pub const CLONE_PROJECT_ON_MACHINE: &str = "cloneProjectOnMachine";

/// The Clone on… row for `group`, or `None` when every known machine has the project (or the list
/// does not know which machines have it).
pub(super) fn clone_on_menu(group: &GroupView, inputs: &SidebarInputs) -> Option<MenuItem> {
    if inputs.settings.separate_by_machine {
        return None;
    }
    let project = group.core.project_context.as_ref()?;
    if project.worktree.is_some()
        || project.bot_profile.is_some()
        || project.is_home
        || project.gbot.is_some()
    {
        return None;
    }
    // No chips: a project only this computer has.
    let has = |machine_id: &str| {
        if group.core.machine_names.is_empty() {
            machine_id == LOCAL_MACHINE_ID
        } else {
            group
                .core
                .machine_names
                .iter()
                .any(|chip| chip.machine_id == machine_id)
        }
    };
    let mut missing: Vec<_> = inputs
        .host
        .machines
        .iter()
        .filter(|tab| !tab.this_computer && !has(&tab.machine_id))
        .collect();
    if missing.is_empty() {
        return None;
    }
    // This computer first, then the others in their tabs' order.
    missing.sort_by_key(|tab| !tab.is_local());
    let Some(remote_url) = project
        .git_remote_origin_url
        .as_deref()
        .map(str::trim)
        .filter(|url| !url.is_empty())
    else {
        return Some(MenuItem {
            label: Some("Clone on…".to_string()),
            icon: Some("copy-plus".to_string()),
            suffix: Some("No git remote".to_string()),
            disabled: true,
            ..MenuItem::default()
        });
    };
    let rows = missing
        .into_iter()
        .map(|tab| {
            let label = if tab.is_local() {
                "This computer".to_string()
            } else {
                tab.label.clone()
            };
            let connected = tab.is_local() || tab.is_connected();
            MenuItem {
                label: Some(label.clone()),
                icon: Some(
                    if tab.is_local() {
                        "device-desktop"
                    } else {
                        "server"
                    }
                    .to_string(),
                ),
                suffix: (!connected).then(|| "Not connected".to_string()),
                disabled: !connected,
                command: Some(MenuCommand::host(json!({
                    "type": CLONE_PROJECT_ON_MACHINE,
                    "machineId": tab.machine_id,
                    "machineName": label,
                    "remoteUrl": remote_url,
                    "projectName": group.core.title,
                }))),
                ..MenuItem::default()
            }
        })
        .collect();
    Some(MenuItem::submenu("Clone on…", "copy-plus", rows))
}
