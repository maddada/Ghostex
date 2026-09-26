//! Sleep and Wake on a USER-MADE session group (a `gpui-wsg:` group id), which the bulk planner
//! refuses because the set is the group's members rather than a project's rows.
//!
//! The set is the runtime's `setGroupSleeping`: the members in the group's own order, of those the
//! ones whose row in that machine's LIVE presentation can move (running to sleep, sleeping to
//! wake). The host sends it through the ordinary `setSessionsSleeping`, so sleeps go out 350 ms
//! apart and wakes together. The Chats group has no members and moves nothing, as before.
//!
//! SEE-ALSO: apps/desktop/sidebar/gxserver-runtime/auto-sleep.ts (`setGroupSleeping`, deleted with
//! this port), packages/gx-core/src/sidebar_actions/bulk.rs.

use ghostex_gx_protocol::LifecycleState;
use serde_json::Value;

use crate::core::Core;
use crate::keys::{parse_workspace_subgroup_id, SessionKey};

use super::resolve::text_field;

/// `(sleeping, sidebar session ids)` for a `setGroupSleeping` on a user-made group, or `None`
/// when the payload is not one.
pub fn plan_group_sleep(core: &Core, message: &Value) -> Option<(bool, Vec<String>)> {
    if text_field(message, "type")? != "setGroupSleeping" {
        return None;
    }
    let sleeping = message.get("sleeping")?.as_bool()?;
    let (project, group_id) = parse_workspace_subgroup_id(text_field(message, "groupId")?)?;
    let members = core
        .presentation()
        .user_groups_of_project(&project)
        .iter()
        .find(|group| group.group_id == group_id)
        .map(|group| group.session_ids.clone())
        .unwrap_or_default();
    let wanted = match sleeping {
        true => LifecycleState::Running,
        false => LifecycleState::Sleeping,
    };
    let loaded = core.presentation().loaded_live(&project.machine);
    let ids = members
        .into_iter()
        .filter(|session_id| {
            loaded
                .and_then(|loaded| loaded.server_session(&project.project_id, session_id))
                .is_some_and(|row| row.lifecycle_state == wanted)
        })
        .map(|session_id| {
            SessionKey {
                machine: project.machine.clone(),
                project_id: project.project_id.clone(),
                session_id,
            }
            .to_sidebar_session_id()
        })
        .collect();
    Some((sleeping, ids))
}
