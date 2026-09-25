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
use crate::sidebar_view::view::{SessionRow, SidebarView};
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
/// question here is asked of the project facts (`build_project_meta`), whose `project_order` is
/// exactly the set of projects that get a `combined-project:` group: chat and quick projects go
/// into the Chats group instead and parked, recent and locally hidden projects get none at all.
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
    meta.project_order
        .iter()
        .any(|project_id| *project_id == key.project_id)
        .then_some(key.project_id)
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
