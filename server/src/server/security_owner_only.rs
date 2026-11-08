//! What only the owner's own devices may turn on or set up (GitHub #495 follow-up, "settings
//! only"): agents, coordinators, threads, Chief, the token file and connected computers are
//! refused, and the refusal says where in Settings the owner does it.
//!
//! | Route | Owner-only when |
//! | --- | --- |
//! | `/api/installExtension` | always (local folder, catalog id, zip URL) |
//! | `/api/updateExtensionState` | it turns an extension on, or grants a permission it doesn't have |
//! | `/api/saveSecretProvider`, `/api/removeSecretProvider` | always |
//! | `/api/joinTeamSync`, `/api/createTeamSyncInvite`, `/api/setTeamLinearKey`, `/api/setOwnLinearKey`, `/api/setSlackFlowSettings` | always (leaving a team stays open) |
//! | `/api/updateAgentSettings` | it turns `agentAcceptAllEnabled` on |
//! | `/api/setWorkSource` | it sets a new ticket command for a workspace that has credentials stored (route_http/work_source.rs, which knows the workspace) |
//!
//! The always-owner-only routes are declared in `security::route_access`
//! (`Capability::OwnerSetup`); this file words their refusal and checks the rest.
//!
//! CDXC:Security 2026-11-08 DECISION:
//! User (#495 follow-up: Q1860 extensions, Q1864 secret providers, Q1865 teams, Q1866 the loosening settings keys, all "settings only"): "agents explain owner-only things and point to Settings, but they can't turn them on." Turning something off, leaving a team and uninstalling stay open to everyone.
//! SEE-ALSO: server/src/security/capability.rs (`route_access`), server/src/settings_agent_writable.rs and packages/settings-catalog/src/help/mod.rs (the settings keys), server/src/extensions/registry.rs (`start` refuses a payload that was never installed).

use serde_json::{Map, Value};

use crate::security::{Capability, Principal};

use super::AppState;

/// Where the owner does what an owner-only route does.
pub(crate) fn owner_setup_message(endpoint_path: &str) -> &'static str {
    match endpoint_path {
        "/api/installExtension" => {
            "Only you can install extensions. Install it in Settings > Extensions."
        }
        "/api/updateExtensionState" => {
            "Only you can turn on an extension or give it permissions. Do it in Settings > Extensions."
        }
        "/api/saveSecretProvider" | "/api/removeSecretProvider" => {
            "Only you can add, change or remove a secret provider. Do it in Settings > Secrets."
        }
        "/api/joinTeamSync"
        | "/api/createTeamSyncInvite"
        | "/api/setTeamLinearKey"
        | "/api/setOwnLinearKey"
        | "/api/setSlackFlowSettings" => {
            "Only you can join a team, make invites, or set the team's Linear keys and its Slack flow. Do it in Settings > Workspaces."
        }
        "/api/updateAgentSettings" => {
            "Only you can turn on \"Agent approvals: accept all\". Turn it on in Settings > Agents."
        }
        _ => "Only your own computer or phone can do this.",
    }
}

/// `Some(message)` when this call is owner-only for its params and the caller isn't the owner.
pub(crate) fn owner_only_refusal(
    state: &AppState,
    caller: &Principal,
    endpoint_path: &str,
    params: &Map<String, Value>,
) -> Option<&'static str> {
    if caller.has(Capability::OwnerSetup) {
        return None;
    }
    let refused = match endpoint_path {
        "/api/updateExtensionState" => extension_state_widens(state, params),
        "/api/updateAgentSettings" => {
            params.get("agentAcceptAllEnabled").and_then(Value::as_bool) == Some(true)
                && !accept_all_on(state)
        }
        _ => false,
    };
    refused.then(|| owner_setup_message(endpoint_path))
}

/// Whether an `updateExtensionState` patch turns the extension on or grants it a permission it
/// doesn't hold now. Pins, placements, preferences and turning it off stay open.
fn extension_state_widens(state: &AppState, params: &Map<String, Value>) -> bool {
    let Some(patch) = params.get("patch").and_then(Value::as_object) else {
        return false;
    };
    let turns_on = patch.get("enabled").and_then(Value::as_bool) == Some(true);
    let grants = patch
        .get("grantedPermissions")
        .and_then(Value::as_array)
        .filter(|granted| !granted.is_empty());
    if !turns_on && grants.is_none() {
        return false;
    }
    let id = params
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    // An extension that can't be read is refused: nothing an agent changes there is safe to guess.
    let Some(installed) = state
        .extension_registry
        .list()
        .ok()
        .and_then(|extensions| extensions.into_iter().find(|extension| extension.id == id))
    else {
        return true;
    };
    if turns_on && !installed.state.enabled {
        return true;
    }
    grants.is_some_and(|granted| {
        granted.iter().any(|permission| {
            serde_json::from_value(permission.clone())
                .map(|permission| !installed.state.granted_permissions.contains(&permission))
                .unwrap_or(true)
        })
    })
}

/// Whether "Agent approvals: accept all" is on now, so sending it on again changes nothing.
fn accept_all_on(state: &AppState) -> bool {
    crate::storage::open_gxserver_database(&state.paths)
        .ok()
        .and_then(|db| crate::agents::settings::read_agent_settings(&db).ok())
        .and_then(|settings| {
            settings
                .get("agentAcceptAllEnabled")
                .and_then(Value::as_bool)
        })
        .unwrap_or(false)
}
