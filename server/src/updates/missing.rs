//! The "Not installed" part of Settings > Updates: the agent CLIs, account switchers and tools this
//! computer doesn't have that Ghostex knows how to install here. Each is an item with
//! `state: "notInstalled"`, and Install sends its id through `/api/updates`' `install`, which runs
//! the install its own Settings row runs (install.rs: Settings > Agents' first usable method, the
//! Accounts page's Install, Integrations > Tools' Install).
//!
//! CDXC:Updates 2026-11-07 DECISION:
//! User: "We also need the ability to install agents/etc that aren't installed already on each
//! machine through update page". Every computer's group lists what it lacks in a quieter Not
//! installed part, with Install running on that computer. These items install only on a click:
//! they are never `canUpdate` (Update all and "Update automatically" skip them) and never `topBar`
//! (the top bar's count stays real updates).
use serde_json::{json, Value};

use super::items::{agent_icon, definition, method_label};

fn base(id: String, kind: &str, group: &str, name: String, icon: &str, detail: String) -> Value {
    json!({
        "id": id,
        "kind": kind,
        "group": group,
        "name": name,
        "icon": icon,
        "detail": detail,
        "installed": null,
        "latest": null,
        "major": false,
        "state": "notInstalled",
        "canInstall": true,
        "canUpdate": false,
        "canAuto": false,
        "auto": false,
        "topBar": false,
        "inUse": false,
    })
}

/// While its install job runs the item shows its step; a failed install says why on the row.
fn with_job(mut item: Value, active: bool, queued: bool, failed: Option<&str>) -> Value {
    if active {
        item["state"] = json!("updating");
        item["progress"] = json!(if queued {
            "Waiting…"
        } else {
            "Installing…"
        });
    } else if let Some(error) = failed {
        item["error"] = json!(error);
    }
    item
}

/// An agent CLI from the Agents list (`agent_cli` `list`) that isn't installed, when one of its
/// install methods can run on this computer.
pub(super) fn missing_agent_item(agent: &Value) -> Option<Value> {
    if agent["executablePath"].is_string() {
        return None;
    }
    let id = agent["agentId"].as_str()?;
    let definition = definition(id)?;
    let method = agent["methods"].as_array()?.iter().find(|method| {
        method["unavailableReason"].is_null()
            && method["id"].as_str().is_some_and(|id| id != "manual")
    })?;
    let name = agent["name"]
        .as_str()
        .unwrap_or(&definition.binary)
        .to_string();
    let item = base(
        format!("agent:{id}"),
        "agent",
        "agents",
        name,
        agent_icon(id),
        format!(
            "Agent · {}",
            method_label(method["id"].as_str().unwrap_or_default())
        ),
    );
    let job = &agent["job"];
    let installing = job["operation"] == "install";
    let failed = (installing && job["status"] == "failed")
        .then(|| job["error"].as_str().unwrap_or("The install failed."));
    Some(with_job(
        item,
        installing && matches!(job["status"].as_str(), Some("queued" | "running")),
        job["status"] == "queued",
        failed,
    ))
}

/// Claude Swap or Codex Swap from the Accounts page's helper list, when it can be installed here.
pub(super) fn missing_helper_item(tool: &Value) -> Option<Value> {
    if tool["installed"] == true || !offers(tool, "install") {
        return None;
    }
    let provider = tool["provider"].as_str()?;
    let (name, icon) = match provider {
        "claude" => ("Claude Swap", "claude-swap"),
        "codex" => ("Codex Swap", "codex-swap"),
        _ => return None,
    };
    let item = base(
        format!("helper:{provider}"),
        "helper",
        "tools",
        name.to_string(),
        icon,
        "Account switcher".to_string(),
    );
    let job = &tool["job"];
    let installing = job["action"] == "install";
    let failed = (installing && job["status"] == "failed")
        .then(|| job["error"].as_str().unwrap_or("The install failed."));
    Some(with_job(
        item,
        installing && job["status"] == "running",
        false,
        failed,
    ))
}

/// A tool from Integrations > Tools that isn't installed and that Ghostex installs itself (not one
/// it can only hand a terminal command for). `listed_elsewhere`: Hex while Dictate uses it, which
/// the updates list already offers.
pub(super) fn missing_tool_item(tool: &Value, listed_elsewhere: bool) -> Option<Value> {
    let id = tool["id"].as_str()?;
    if listed_elsewhere
        || tool["installed"] == true
        || tool["supported"] != true
        || !offers(tool, "install")
        || tool["terminalCommand"].is_string()
        || tool["notAnswering"].is_string()
    {
        return None;
    }
    let detail = match id {
        "trycua" => "Tool · cua-driver",
        "hex" => "Dictation app",
        _ => "Tool",
    };
    let mut item = base(
        format!("tool:{id}"),
        "tool",
        "tools",
        tool["label"].as_str().unwrap_or(id).to_string(),
        id,
        detail.to_string(),
    );
    if let Some(latest) = tool["latestVersion"].as_str() {
        item["latest"] = json!(latest);
    }
    if tool["needsPassword"] == true {
        item["note"] = json!("Asks for your password.");
    }
    let job = &tool["job"];
    let installing = job["operation"] == "install";
    let failed = (installing && job["status"] == "failed")
        .then(|| job["error"].as_str().unwrap_or("The install failed."));
    Some(with_job(
        item,
        installing && matches!(job["status"].as_str(), Some("queued" | "running")),
        job["status"] == "queued",
        failed,
    ))
}

fn offers(tool: &Value, operation: &str) -> bool {
    tool["actions"]
        .as_array()
        .is_some_and(|actions| actions.iter().any(|action| action == operation))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_agent_with_a_usable_method_is_offered_for_install() {
        let agent = json!({
            "agentId": "codex", "name": "Codex",
            "methods": [
                { "id": "brew", "command": "brew install codex", "unavailableReason": "Homebrew isn't installed." },
                { "id": "npm", "command": "npm i -g @openai/codex", "unavailableReason": null },
            ],
        });
        let item = missing_agent_item(&agent).expect("listed");
        assert_eq!(item["id"], "agent:codex");
        assert_eq!(item["state"], "notInstalled");
        assert_eq!(item["detail"], "Agent · npm");
        assert_eq!(item["canInstall"], true);
        assert_eq!(item["canUpdate"], false, "Update all never installs");
        assert_eq!(item["canAuto"], false, "never installs on its own");
        assert_eq!(item["topBar"], false, "the top bar counts real updates");
    }

    #[test]
    fn an_installed_agent_or_one_without_a_usable_method_is_not_offered() {
        let installed = json!({ "agentId": "codex", "executablePath": "/bin/codex", "methods": [{ "id": "npm" }] });
        assert!(missing_agent_item(&installed).is_none());
        let blocked = json!({ "agentId": "codex", "methods": [{ "id": "brew", "unavailableReason": "No Homebrew." }] });
        assert!(missing_agent_item(&blocked).is_none());
        let unknown = json!({ "agentId": "not-an-agent", "methods": [{ "id": "npm" }] });
        assert!(missing_agent_item(&unknown).is_none());
    }

    #[test]
    fn a_running_or_failed_install_shows_on_the_row() {
        let running = json!({
            "agentId": "codex", "methods": [{ "id": "npm" }],
            "job": { "operation": "install", "status": "running" },
        });
        let item = missing_agent_item(&running).unwrap();
        assert_eq!(item["state"], "updating");
        assert_eq!(item["progress"], "Installing…");
        let failed = json!({
            "agentId": "codex", "methods": [{ "id": "npm" }],
            "job": { "operation": "install", "status": "failed", "error": "npm: EACCES" },
        });
        let item = missing_agent_item(&failed).unwrap();
        assert_eq!(item["state"], "notInstalled");
        assert_eq!(item["error"], "npm: EACCES");
    }

    #[test]
    fn missing_tools_are_offered_only_when_ghostex_installs_them_here() {
        let uv = json!({ "id": "uv", "label": "uv", "supported": true, "installed": false, "actions": ["install"] });
        let item = missing_tool_item(&uv, false).expect("listed");
        assert_eq!(item["id"], "tool:uv");
        assert_eq!(item["state"], "notInstalled");
        let terminal_only = json!({ "id": "gh", "supported": true, "installed": false, "actions": ["install"], "terminalCommand": "sudo apt install gh" });
        assert!(missing_tool_item(&terminal_only, false).is_none());
        let unsupported =
            json!({ "id": "hex", "supported": false, "installed": false, "actions": ["install"] });
        assert!(missing_tool_item(&unsupported, false).is_none());
        let hex = json!({ "id": "hex", "label": "Hex", "supported": true, "installed": false, "actions": ["install"] });
        assert!(
            missing_tool_item(&hex, true).is_none(),
            "Hex for Dictate is already listed as an update"
        );
        assert!(missing_tool_item(&hex, false).is_some());
        let installed =
            json!({ "id": "uv", "supported": true, "installed": true, "actions": ["update"] });
        assert!(missing_tool_item(&installed, false).is_none());
    }

    #[test]
    fn a_missing_account_switcher_is_offered_when_it_can_be_installed() {
        let swap = json!({ "provider": "claude", "installed": false, "actions": ["install"] });
        let item = missing_helper_item(&swap).expect("listed");
        assert_eq!(item["id"], "helper:claude");
        assert_eq!(item["name"], "Claude Swap");
        let blocked = json!({ "provider": "codex", "installed": false, "actions": [] });
        assert!(missing_helper_item(&blocked).is_none());
    }
}
