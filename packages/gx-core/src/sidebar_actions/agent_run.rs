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
use super::resolve::{action_project, text_field};

/// Whether this renderer command is the launcher's run or a bot's "+", without resolving anything.
pub fn owns_agent_run_command(command: &Value) -> bool {
    text_field(command, "type") == Some("projectAction")
        && matches!(text_field(command, "action"), Some("agent" | "bot"))
}

/// What the run does. `Some(nothing)` when the group is neither a drawn project nor the copy a
/// merged entry acts on (`action_project`), which is the TypeScript's own early return
/// (`if (!group || !project) return`), the same answer Add Worktree and History give.
pub fn plan_agent_run(view: &SidebarView, command: &Value) -> Option<SidebarActionPlan> {
    if !owns_agent_run_command(command) {
        return None;
    }
    let group_id = text_field(command, "groupId")?;
    // A merged entry's Create Agent and launcher name the copy a session last started in.
    let Some(project) = action_project(view, group_id).map(|target| target.project) else {
        return Some(SidebarActionPlan::nothing());
    };
    if text_field(command, "action") == Some("bot") {
        // The bot's own agent, never the launcher's: nothing here reads or writes the primary one.
        // Only the group travels; the host reads the profile off the list it draws.
        if project.bot_profile.is_none() {
            return Some(SidebarActionPlan::nothing());
        }
        return Some(SidebarActionPlan::one(ActionEffect::SidebarHostMessage {
            message: json!({ "type": "runSidebarBot", "groupId": group_id }),
        }));
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
    // A Run in a Box row's location (gx-core agentbox.rs), passed on the same way.
    if let Some(run_location) = command.get("runLocation") {
        message.insert("runLocation".to_string(), run_location.clone());
    }
    Some(SidebarActionPlan::one(ActionEffect::SidebarHostMessage {
        message: Value::Object(message),
    }))
}
