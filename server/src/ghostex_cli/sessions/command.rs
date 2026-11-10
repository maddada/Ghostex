use serde_json::{json, Map, Value};

use crate::ghostex_cli::args::{parse_args, FlagValue, Flags};
use crate::ghostex_cli::output::{is_failed_cli_result, print_json, render_json};
use crate::ghostex_cli::rpc::{call_gxserver_rpc, CliError, CliResult};
use crate::ghostex_cli::{actions, sessions_brief, set_exit_code};

use super::*;

// ---------------------------------------------------------------------------
// sessions command
// ---------------------------------------------------------------------------

pub fn sessions_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let flags = &parsed.flags;
    /*
     * CDXC:AgentLauncher 2026-07-12-00:00:
     * The mobile summary is the one poll React Native Android makes, so it also carries
     * the gxserver-owned agent launcher rows and per-project quick actions.
     * HUD fetch failures must never break the session list; mobile simply
     * hides the launcher rows until the next poll.
     */
    if flags.truthy("json") && flags.truthy("mobileSummary") {
        // A repeat within two seconds is answered from the last run's printed text (summary_memo.rs).
        let memo_key = summary_memo_key(args);
        if let Some(printed) = read_summary_memo(&memo_key) {
            print!("{printed}");
            return Ok(());
        }
        let started_ms = now_ms();
        let result = fetch_session_list_result(flags, true)?;
        let settings = crate::ghostex_cli::settings::read_settings_file().unwrap_or_default();
        // The phone lists no Actions while the Actions extension is off on this computer.
        let actions_on = ghostex_settings_catalog::built_in_extensions::enabled(
            &settings,
            ghostex_settings_catalog::built_in_extensions::ACTIONS,
        );
        let hud = fetch_mobile_sidebar_hud(flags, actions_on).ok();
        let mut merged = match to_mobile_session_list(&result, &settings) {
            Value::Object(map) => map,
            _ => Map::new(),
        };
        if let Some(Value::Object(hud_map)) = hud {
            for (key, value) in hud_map {
                merged.insert(key, value);
            }
        }
        if actions_on {
            insert_global_quick_actions_capability(&mut merged);
        }
        insert_built_in_features(&mut merged);
        let printed = format!("{}\n", render_json(&Value::Object(merged)));
        write_summary_memo(&memo_key, started_ms, &printed);
        print!("{printed}");
        return Ok(());
    }
    let result = fetch_session_list_result(flags, true)?;
    if flags.truthy("json") {
        if flags.truthy("full") {
            print_json(&result);
        } else {
            print_json(&sessions_brief::brief_session_list(&result, flags));
        }
        return Ok(());
    }
    let empty = Vec::new();
    let sessions = result
        .get("sessions")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let grouped = !strict_true(flags, "ungrouped") && !strict_true(flags, "u");
    print_session_list(sessions, grouped);
    Ok(())
}

fn fetch_mobile_sidebar_hud(flags: &Flags, actions_on: bool) -> CliResult<Value> {
    let hud = call_gxserver_rpc(
        "/api/readSidebarHud",
        &json!({ "includeAllProjectCommands": true }),
        flags,
    )?;
    let mut agents: Vec<Value> = Vec::new();
    if let Some(list) = hud.get("agents").and_then(Value::as_array) {
        for agent in list {
            let has_agent_id =
                matches!(agent.get("agentId"), Some(Value::String(text)) if !text.is_empty());
            if !has_agent_id {
                continue;
            }
            let mut map = Map::new();
            insert_present(&mut map, "agentId", agent.get("agentId"));
            insert_present(&mut map, "icon", agent.get("icon"));
            insert_present(&mut map, "name", agent.get("name"));
            agents.push(Value::Object(map));
        }
    }
    let mut quick_actions_by_project = Map::new();
    if let Some(by_project) = hud
        .get("commandsByProject")
        .and_then(Value::as_object)
        .filter(|_| actions_on)
    {
        for (project_id, commands) in by_project {
            let mut actions_list: Vec<Value> = Vec::new();
            if let Some(commands) = commands.as_array() {
                for command in commands {
                    if !is_configured_mobile_quick_action(command) {
                        continue;
                    }
                    actions_list.push(mobile_quick_action(command));
                }
            }
            if !actions_list.is_empty() {
                quick_actions_by_project.insert(project_id.clone(), Value::Array(actions_list));
            }
        }
    }
    // Already in the computer's saved order; every project's menu lists the same Global Actions.
    let global_quick_actions: Vec<Value> = hud
        .get("globalCommands")
        .and_then(Value::as_array)
        .filter(|_| actions_on)
        .into_iter()
        .flatten()
        .filter(|command| is_configured_mobile_quick_action(command))
        .map(mobile_quick_action)
        .collect();
    Ok(json!({
        "agents": agents,
        "globalQuickActions": global_quick_actions,
        "quickActionsByProject": quick_actions_by_project,
    }))
}

/// One Action as the phone reads it: no command text, and a URL only for a Browser Action.
fn mobile_quick_action(command: &Value) -> Value {
    let mut map = Map::new();
    insert_present(&mut map, "actionType", command.get("actionType"));
    insert_present(&mut map, "commandId", command.get("commandId"));
    insert_present(&mut map, "icon", command.get("icon"));
    insert_present(&mut map, "name", command.get("name"));
    if command.get("actionType").and_then(Value::as_str) == Some("browser") {
        insert_present(&mut map, "url", command.get("url"));
    }
    Value::Object(map)
}

/// CDXC:AgentLauncher 2026-10-31 WHY:
/// `capabilities.globalQuickActions` tells the phone that the `ghostex` it runs over SSH lists `globalQuickActions` and honours `run-action --scope global`. The CLI sets it, not gxserver: an older CLI ignores `--scope` and would start a project Action with the same id, and it forwards the daemon's capabilities unchanged, so only this binary can promise the flag. It follows the Actions switch like the lists, and joins the daemon's capabilities instead of replacing them.
fn insert_global_quick_actions_capability(summary: &mut Map<String, Value>) {
    if let Value::Object(capabilities) = summary.entry("capabilities").or_insert_with(|| json!({}))
    {
        capabilities.insert("globalQuickActions".to_string(), json!(true));
    }
}

pub(super) fn is_configured_mobile_quick_action(command: &Value) -> bool {
    let has_command_id =
        matches!(command.get("commandId"), Some(Value::String(text)) if !text.is_empty());
    if !has_command_id {
        return false;
    }
    if command.get("actionType").and_then(Value::as_str) == Some("browser") {
        return matches!(command.get("url"), Some(Value::String(text)) if !text.trim().is_empty());
    }
    matches!(command.get("command"), Some(Value::String(text)) if !text.trim().is_empty())
}

pub fn run_quick_action_command(args: &[String]) -> CliResult<()> {
    /*
     * CDXC:Mobile 2026-07-12-00:00:
     * Mobile quick actions cannot reuse `run-command`: that routes through
     * gxserver renderer commands into the desktop app's command pane, which
     * the phone cannot see. `run-action` resolves the trusted HUD command on
     * the Mac (mobile sends only ids) and materializes a normal gxserver
     * terminal session running it, so the phone can attach like any other
     * session. Browser actions return the URL for the phone to open locally.
     */
    let parsed = parse_args(args);
    let flags = &parsed.flags;
    let command_id = flags
        .text("commandId")
        .or_else(|| parsed.rest.first().cloned())
        .unwrap_or_default()
        .trim()
        .to_string();
    // A bare `--project-id` names no project; `Flags::text` would read it as "true".
    let project_id = normalize_required_project_id(
        flags.string_value("projectId").map(str::to_string),
        "run-action",
    )?;
    if command_id.is_empty() {
        return Err(CliError::Other(
            "run-action requires a command id.".to_string(),
        ));
    }
    let global = quick_action_scope_is_global(flags)?;
    let hud = call_gxserver_rpc(
        "/api/readSidebarHud",
        &json!({ "activeProjectId": project_id }),
        flags,
    )?;
    let action = hud
        .get(if global { "globalCommands" } else { "commands" })
        .and_then(Value::as_array)
        .and_then(|commands| {
            commands.iter().find(|command| {
                command.get("commandId").and_then(Value::as_str) == Some(command_id.as_str())
            })
        });
    let action = match action {
        Some(action) if is_configured_mobile_quick_action(action) => action,
        _ => {
            let error = if global {
                "Global quick action is not configured."
            } else {
                "Quick action is not configured for this project."
            };
            print_json(&json!({
                "commandId": command_id,
                "error": error,
                "ok": false,
            }));
            set_exit_code(1);
            return Ok(());
        }
    };
    if action.get("actionType").and_then(Value::as_str) == Some("browser") {
        let mut map = Map::new();
        map.insert("actionType".to_string(), json!("browser"));
        map.insert("commandId".to_string(), json!(command_id));
        insert_present(&mut map, "name", action.get("name"));
        map.insert("ok".to_string(), json!(true));
        insert_present(&mut map, "url", action.get("url"));
        print_json(&Value::Object(map));
        return Ok(());
    }
    let payload = {
        let mut map = Map::new();
        insert_present(&mut map, "command", action.get("command"));
        map.insert("projectId".to_string(), json!(project_id));
        map.insert("start".to_string(), json!(true));
        map.insert(
            "title".to_string(),
            match action.get("name") {
                Some(name) if js_truthy(Some(name)) => name.clone(),
                _ => json!("Action"),
            },
        );
        Value::Object(map)
    };
    let created = match actions::send_gxserver_cli_action("createSession", &payload, flags) {
        Ok(created) => created,
        Err(error) => {
            print_json(&json!({
                "actionType": "terminal",
                "commandId": command_id,
                "error": error.to_string(),
                "ok": false,
            }));
            set_exit_code(1);
            return Ok(());
        }
    };
    let session_id = created
        .get("session")
        .and_then(|session| session.get("sessionId"));
    if is_failed_cli_result(&created) || !js_truthy(session_id) {
        print_json(&json!({
            "actionType": "terminal",
            "commandId": command_id,
            "error": "Could not start the quick action session.",
            "ok": false,
        }));
        set_exit_code(1);
        return Ok(());
    }
    let mut merged = created.as_object().cloned().unwrap_or_default();
    merged.insert("actionType".to_string(), json!("terminal"));
    merged.insert("commandId".to_string(), json!(command_id));
    merged.insert("ok".to_string(), json!(true));
    print_json(&Value::Object(merged));
    Ok(())
}

/// `run-action --scope`: absent or `project` looks the id up among the project's Actions, `global`
/// among the Global Actions; the project still says where a Terminal Action runs.
///
/// CDXC:AgentLauncher 2026-10-31 WHY: a Global Action and a project Action can share an id, so any other value, an empty one or a bare `--scope` is refused before anything is looked up or started instead of reading as the project list, which would run an Action nobody picked. A global id that is not among the Global Actions is reported as not configured and never falls back to the project's list.
/// SEE-ALSO: packages/gx-core/src/sidebar_command_run.rs (the desktop's Action run applies the same scope rule).
fn quick_action_scope_is_global(flags: &Flags) -> CliResult<bool> {
    match flags.0.get("scope") {
        None => Ok(false),
        Some(FlagValue::Text(scope)) if scope == "project" => Ok(false),
        Some(FlagValue::Text(scope)) if scope == "global" => Ok(true),
        Some(_) => Err(CliError::Other(
            "run-action --scope must be project or global.".to_string(),
        )),
    }
}

fn normalize_required_project_id(value: Option<String>, command_name: &str) -> CliResult<String> {
    let project_id = value.unwrap_or_default().trim().to_string();
    if project_id.is_empty() {
        return Err(CliError::Other(format!(
            "{command_name} requires --project-id until gxserver active-project routing lands."
        )));
    }
    Ok(project_id)
}
