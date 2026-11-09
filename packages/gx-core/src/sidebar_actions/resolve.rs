//! Turning the ids a command carries into the things the daemon and the native bridge accept.
//!
//! Ported from `resolveProjectIdForGroup` (`gxserver-runtime/projects-and-commands.ts`),
//! `postNativeProjectPathAction` (`gxserver-runtime/app-shot-and-misc.ts`) and
//! `gxserver-runtime/constants.ts`, all deleted with QuickJS on 2026-09-25 (see git history).

use serde_json::{json, Value};

use crate::core::Core;
use crate::keys::{MachineId, ProjectKey};
use crate::sidebar_view::projects::build_project_meta;
use crate::sidebar_view::text::js_trim;
use crate::sidebar_view::view::{ProjectContextView, RemoteMachineView, SessionRow, SidebarView};
use crate::sidebar_view::SidebarInputs;

use super::plan::ActionEffect;

/// `GPUI_SIDEBAR_NATIVE_PROJECT_PATH_ACTION_MESSAGE_TYPE`.
pub const NATIVE_PROJECT_PATH_ACTION_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.nativeProjectPathAction";
/// `GPUI_SIDEBAR_NATIVE_PROJECT_PATH_ACTION_MESSAGE_VERSION`.
pub const NATIVE_PROJECT_PATH_ACTION_MESSAGE_VERSION: i64 = 1;

/// `normalizeNonEmptyString`: the ORIGINAL string when it has a non-blank trim, else nothing.
/// It does not return the trimmed value, and two call sites of this port depend on that (the text
/// that reaches the clipboard is the text the menu built).
pub(crate) fn non_empty(value: Option<&str>) -> Option<&str> {
    value.filter(|text| !js_trim(text).is_empty())
}

/// `postNativeProjectPathAction` without its options: the trimmed project id, or nothing when it
/// trims to empty, which is that function's `handleUnsupportedSidebarMessage` leg.
pub(crate) fn native_project_path_action(action: &str, project_id: &str) -> Option<ActionEffect> {
    let project_id = js_trim(project_id);
    if project_id.is_empty() {
        return None;
    }
    Some(ActionEffect::NativeProjectPathAction {
        payload: json!({
            "action": action,
            "projectId": project_id,
            "type": NATIVE_PROJECT_PATH_ACTION_MESSAGE_TYPE,
            "version": NATIVE_PROJECT_PATH_ACTION_MESSAGE_VERSION,
        }),
    })
}

/// `resolveProjectIdForGroup`: the raw project id of a LOCAL project group the sidebar has a group
/// for, and nothing for every other string.
///
/// The TypeScript asks `latestGroups`, the full projected inventory, which is not the drawn list:
/// it holds every group the projection built, filters and Spaces and collapse included. The same
/// question here is asked of the project facts (`build_project_meta`), whose `grouped_project_ids`
/// (projects, then bots) is exactly the set of projects that get a `combined-project:` group:
/// parked, recent and locally hidden projects get none at all.
/// Asking the drawn `SidebarView` instead would answer `None` for a project the user has filtered
/// off screen, which would turn Copy Path on a hidden project into a silent no-op.
pub fn local_project_group_project_id(
    core: &Core,
    inputs: &SidebarInputs,
    group_id: &str,
) -> Option<String> {
    let key = ProjectKey::parse_sidebar_group_id(group_id)?;
    if !key.machine.is_local() {
        return None;
    }
    let machine = core.presentation().machine(&MachineId::Local)?;
    let meta = build_project_meta(
        machine,
        machine
            .side_state()
            .workspace_groups
            .as_ref()
            .map(|groups| groups.project_order.as_slice())
            .unwrap_or_default(),
        inputs.host.parked_project_ids(&MachineId::Local),
    );
    let grouped = meta
        .grouped_project_ids()
        .any(|project_id| *project_id == key.project_id);
    grouped.then_some(key.project_id)
}

/// The string field of a command payload, without the JSON indexing that panics on a non-object.
pub(crate) fn text_field<'a>(message: &'a Value, key: &str) -> Option<&'a str> {
    message.get(key).and_then(Value::as_str)
}

/// The row as the list draws it, which is where the two renderer-side actions look it up:
/// `runNativeSessionAction` reads `sidebarStore.getState().sessionsById[...]` and gives up when it
/// is not there. Shared by `modals.rs` and `snooze.rs` because both are that one function's arms.
pub(super) fn drawn_row<'a>(
    view: &'a SidebarView,
    sidebar_session_id: &str,
) -> Option<&'a SessionRow> {
    view.groups
        .iter()
        .flat_map(|group| group.core.sessions.iter())
        .find(|session| session.row.sidebar_session_id == sidebar_session_id)
        .map(|session| session.row.as_ref())
}

/// The project a header action names, as `plan_project_action` and `plan_agent_run` read it.
pub(super) struct ActionProject<'a> {
    /// The drawn entry's name.
    pub(super) title: &'a str,
    pub(super) project: &'a ProjectContextView,
    pub(super) remote_machine: Option<&'a RemoteMachineView>,
}

/// The project `group_id` names: its drawn group, or else the copy a merged entry's header actions
/// go to (`GroupCore::action_target`), which has no drawn group of its own. `None` for any other id,
/// or a drawn group with no project, which is the TypeScript's `if (!group || !project) return`.
///
/// CDXC:RemoteMachines 2026-11-09 WHY:
/// The one list merges copies of a project (another computer's, or a second clone here) into one entry whose New Terminal, Create Agent and ⋯ actions go to the copy a session last started in (Q1353). That copy is not drawn, so a lookup of the drawn group alone answered nothing and Add Worktree, History, Create Agent, the launcher's agents and Configure did nothing on such an entry.
pub(super) fn action_project<'a>(
    view: &'a SidebarView,
    group_id: &str,
) -> Option<ActionProject<'a>> {
    if let Some(group) = view.group(group_id) {
        return Some(ActionProject {
            title: &group.core.title,
            project: group.core.project_context.as_ref()?,
            remote_machine: group.core.remote_machine.as_ref(),
        });
    }
    view.groups.iter().find_map(|group| {
        let target = group
            .core
            .action_target
            .as_ref()
            .filter(|target| target.group_id == group_id)?;
        Some(ActionProject {
            title: &group.core.title,
            project: &target.project_context,
            remote_machine: target.remote_machine.as_ref(),
        })
    })
}
