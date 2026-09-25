//! The active project context: which project the titlebar names, which views it offers, and the
//! surface ids of its Code, Kanban, Automate and Docs views. Built from the store's focus.
//!
//! CDXC:FocusRouting 2026-09-25 WHY:
//! The old runtime built this payload from its own sidebar projection (`active-project-context.ts`) and posted it on every publish, which made it the second owner of the active project beside the store. The store's focus is the only owner now, so the payload is derived from it here, with the same contract and the same rules: a real project needs its daemon row, a Chats collection and a missing or unloaded project give the Quick payload, and All Automations gives its own. One declared difference: a session in a user-made group used to give the Quick payload too (the runtime's subgroup rows carried no project context), which dropped the project's views while one of its own sessions was focused; it now gives its project's context.
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/focus_publish.rs (the host that applies it),
//! apps/desktop/src/app/helpers/sidebar/workspace_terminal_actions.rs (the contract parser).

use std::collections::BTreeSet;

use serde_json::{json, Value};

use crate::focus::{ActiveGroup, FocusState};
use crate::keys::ProjectKey;
use crate::presentation_store::PresentationStore;
use crate::sidebar_view::projects::build_project_meta;

/// `ghostex.gpui.sidebar.activeProjectContext`, version 1.
pub const ACTIVE_PROJECT_CONTEXT_MESSAGE_TYPE: &str = "ghostex.gpui.sidebar.activeProjectContext";
pub const ACTIVE_PROJECT_CONTEXT_MESSAGE_VERSION: u64 = 1;

/// The synthetic project the All Automations overview is.
const QUICK_AUTOMATIONS_PROJECT_ID: &str = crate::selectors::QUICK_AUTOMATIONS_PROJECT_ID;
const QUICK_AUTOMATIONS_DISPLAY_TITLE: &str = "All Automations";

/// The payload for the store's focus. `quick_automations_overview` is the All Automations
/// overview, which is the host's state rather than a focus the store can hold.
pub fn active_project_context_payload(
    store: &PresentationStore,
    focus: &FocusState,
    quick_automations_overview: bool,
) -> Value {
    if quick_automations_overview {
        return quick_automations_payload();
    }
    let (Some(project), Some(group)) = (&focus.active_project, &focus.active_group) else {
        return quick_projectless_payload();
    };
    project_payload(store, project, group).unwrap_or_else(quick_projectless_payload)
}

/// The context of `project` while `group` is the active group, or `None` when the Quick payload
/// applies: the Chats collection is active, or the project has no daemon row the store holds.
pub fn project_context_payload(
    store: &PresentationStore,
    project: &ProjectKey,
    group: &ActiveGroup,
) -> Option<Value> {
    project_payload(store, project, group)
}

fn project_payload(
    store: &PresentationStore,
    project: &ProjectKey,
    group: &ActiveGroup,
) -> Option<Value> {
    if matches!(group, ActiveGroup::Chats(_)) {
        return None;
    }
    let machine = store.machine(&project.machine)?;
    let loaded = machine.loaded()?;
    let row = loaded.project(&project.project_id)?;
    // Only the overlays are read, so the order and the parked set do not matter here.
    let meta = build_project_meta(machine, &[], &BTreeSet::new());
    let overlay = meta.overlay(&project.project_id);
    let editor_project_id = project.to_workspace_project_id();
    let selection_owner = overlay
        .and_then(|overlay| overlay.worktree.as_ref())
        .map(|worktree| worktree.parent_project_id.trim())
        .filter(|parent| !parent.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| editor_project_id.clone());
    let git_remote_origin_url = row
        .git_remote_origin_url
        .value()
        .map(|url| url.trim())
        .filter(|url| !url.is_empty())
        .map(str::to_string);
    let project_path = row
        .path
        .as_deref()
        .filter(|path| !path.trim().is_empty())
        .map(str::to_string);
    let icon_data_url = overlay.and_then(|overlay| overlay.icon_data_url.clone());
    Some(json!({
        "version": ACTIVE_PROJECT_CONTEXT_MESSAGE_VERSION,
        "type": ACTIVE_PROJECT_CONTEXT_MESSAGE_TYPE,
        "activeProject": {
            "activeProjectId": editor_project_id,
            "displayName": row.title,
            "gitRemoteOriginUrl": git_remote_origin_url,
            "projectIconDataUrl": icon_data_url,
            "projectPath": project_path,
            "selectionOwnerProjectId": selection_owner,
            "isQuickProjectless": false,
            "workareaAvailability": {
                "source": true,
                "browser": true,
                "kanban": true,
                "automate": true,
                "manage": true,
            },
            "surfaceIds": {
                "sourceWorkareaId": editor_project_id,
                "kanbanBoardId": project_editor_surface_id(&editor_project_id, "tasks"),
                "automateBoardId": project_editor_surface_id(&editor_project_id, "automate"),
                "manageWorkspaceId": project_editor_surface_id(&editor_project_id, "manage"),
            },
        },
    }))
}

/// CDXC:Automations 2026-07-08:
/// Mirror macOS `createQuickAutomationsProjectEditorUrl` and `focusQuickAutomationsProject`: All Automations publishes a project-scoped Automate surface id for `quick-automations`, but no Source, Browser, Kanban, Manage, icon, or project path.
pub fn quick_automations_payload() -> Value {
    json!({
        "version": ACTIVE_PROJECT_CONTEXT_MESSAGE_VERSION,
        "type": ACTIVE_PROJECT_CONTEXT_MESSAGE_TYPE,
        "activeProject": {
            "activeProjectId": QUICK_AUTOMATIONS_PROJECT_ID,
            "displayName": QUICK_AUTOMATIONS_DISPLAY_TITLE,
            "gitRemoteOriginUrl": null,
            "projectIconDataUrl": null,
            "projectPath": null,
            "selectionOwnerProjectId": QUICK_AUTOMATIONS_PROJECT_ID,
            "isQuickProjectless": false,
            "workareaAvailability": {
                "source": false,
                "browser": false,
                "kanban": false,
                "automate": true,
                "manage": false,
            },
            "surfaceIds": {
                "automateBoardId": project_editor_surface_id(QUICK_AUTOMATIONS_PROJECT_ID, "automate"),
            },
        },
    })
}

pub fn quick_projectless_payload() -> Value {
    json!({
        "version": ACTIVE_PROJECT_CONTEXT_MESSAGE_VERSION,
        "type": ACTIVE_PROJECT_CONTEXT_MESSAGE_TYPE,
        "activeProject": {
            "activeProjectId": null,
            "displayName": "Quick",
            "gitRemoteOriginUrl": null,
            "projectIconDataUrl": null,
            "projectPath": null,
            "selectionOwnerProjectId": null,
            "isQuickProjectless": true,
            "workareaAvailability": {
                "source": true,
                "browser": false,
                "kanban": false,
                "automate": false,
                "manage": false,
            },
            "surfaceIds": {},
        },
    })
}

/// `nativeProjectEditorSurfaceId`: `project-editor:<encodeURIComponent(projectId)>:<mode>`.
fn project_editor_surface_id(project_id: &str, mode: &str) -> String {
    format!(
        "project-editor:{}:{mode}",
        crate::keys::encode_uri_component(project_id)
    )
}
