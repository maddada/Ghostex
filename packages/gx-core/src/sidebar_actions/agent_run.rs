//! The agent launcher's run (`projectAction: agent`): the project header's "Create <agent>" half,
//! the launcher menu's Configure row, and every account row of the launcher's account page.
//!
//! CDXC:AgentLauncher 2026-09-21 WHY:
//! Followed to its last function, `runNativeProjectAction`'s `agent` arm is one of two things.
//! With no agent it opens the Configure Agents dialog (`openAppModal`, no close first, the bridge's
//! `open` arm). With an agent it calls `writePrimaryAgentLauncherId(agentId)` and then posts
//! `runSidebarAgent { groupId, agentId, accountId }` to the runtime's `handleSidebarMessage`, whose
//! arm is `requestAgentSessionLaunch`. The runtime already has ONE entry that does exactly that
//! pair: `onSidebarHostMessage`, whose `runSidebarAgent` branch writes the primary agent id and
//! then calls `handleSidebarMessage` with the same message (it is how the New Thread picker
//! launches). So the store sends the message there and the launch, the storage write and the
//! `primaryAgentLauncherChanged` echo are the same functions they were, reached from Rust instead
//! of from the sidebar page. The launch itself (the hook check, the create call, the focus) stays
//! the runtime's until the create surface moves; that is `runSidebarAgent`'s own row in the
//! PROGRESS E list, not this payload's.
//!
//! `accountId` is absent rather than null when the row carries none: the TypeScript's `undefined`
//! is a missing key once the message is JSON, and `requestAgentSessionLaunch` reads it the same
//! either way.
//!
//! Ported from the sidebar page's `runNativeProjectAction` (frozen in the deleted
//! `tooling/gx-core/sidebar-page-frozen/project-actions.ts`) and `installGpuiBridgeCallbacks` /
//! `onSidebarHostMessage` in the deleted `gxserver-runtime/core.ts` (see git history).
//!
//! SEE-ALSO: apps/desktop/src/app/sidebar_agent_launch_placeholder.rs,
//! apps/desktop/src/app/gx_store/sidebar_state_actions.rs.

use serde_json::{json, Map, Value};

use crate::sidebar_view::SidebarView;

use super::plan::{ActionEffect, SidebarActionPlan};
use super::resolve::text_field;

/// Whether this renderer command is the launcher's run, without resolving anything.
pub fn owns_agent_run_command(command: &Value) -> bool {
    text_field(command, "type") == Some("projectAction")
        && text_field(command, "action") == Some("agent")
}

/// What the run does. `Some(nothing)` when the group is not a drawn project, which is the
/// TypeScript's own early return (`if (!group || !project) return`), the same answer Add Worktree
/// and History give.
pub fn plan_agent_run(view: &SidebarView, command: &Value) -> Option<SidebarActionPlan> {
    if !owns_agent_run_command(command) {
        return None;
    }
    let group_id = text_field(command, "groupId")?;
    let Some(group) = view.group(group_id) else {
        return Some(SidebarActionPlan::nothing());
    };
    if group.core.project_context.is_none() {
        return Some(SidebarActionPlan::nothing());
    }
    // `if (!command.agentId)`: an empty string is falsy too.
    let Some(agent_id) = text_field(command, "agentId").filter(|agent_id| !agent_id.is_empty())
    else {
        return Some(SidebarActionPlan::one(ActionEffect::OpenAppModal {
            payload: json!({ "type": "open", "modal": "configureAgents" }),
        }));
    };
    let mut message = Map::new();
    message.insert(
        "type".to_string(),
        Value::String("runSidebarAgent".to_string()),
    );
    // `group.groupId`, which is the id the command carried: the lookup is by that id.
    message.insert("groupId".to_string(), Value::String(group_id.to_string()));
    message.insert("agentId".to_string(), Value::String(agent_id.to_string()));
    // Whatever the row carried, passed through: a missing key stays missing.
    if let Some(account_id) = command.get("accountId") {
        message.insert("accountId".to_string(), account_id.clone());
    }
    Some(SidebarActionPlan::one(ActionEffect::SidebarHostMessage {
        message: Value::Object(message),
    }))
}
