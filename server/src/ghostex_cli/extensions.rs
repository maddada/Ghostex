use std::path::PathBuf;

use serde_json::{json, Map, Value};

use crate::ghostex_cli::{
    args::{parse_args, Flags},
    output::print_json,
    rpc::{call_gxserver_rpc, CliError, CliResult},
};

pub fn extensions_command(args: &[String]) -> CliResult<()> {
    match args.first().map(String::as_str) {
        Some("list") => list_command(&args[1..]),
        Some("catalog") => catalog_command(&args[1..]),
        Some("install") => install_command(&args[1..]),
        Some("install-local") => install_local_command(&args[1..]),
        Some("uninstall") => uninstall_command(&args[1..]),
        Some("state") => state_command(&args[1..]),
        Some("panel") => crate::ghostex_cli::extension_panel::panel_command(&args[1..]),
        Some("message") => crate::ghostex_cli::extension_panel::message_command(&args[1..]),
        Some("open") => open_command(&args[1..]),
        None | Some("help") | Some("-h") | Some("--help") => {
            println!("{}", crate::ghostex_cli::usage::extensions_usage());
            Ok(())
        }
        Some(other) => Err(CliError::Other(format!(
            "Unknown extensions command: {other}\n\n{}",
            crate::ghostex_cli::usage::extensions_usage()
        ))),
    }
}

fn list_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    ensure_no_positional_args("extensions list", &parsed.rest)?;
    let result = call_gxserver_rpc("/api/listExtensions", &json!({}), &parsed.flags)?;
    if parsed.flags.truthy("json") {
        print_json(&result);
        return Ok(());
    }
    let extensions = result
        .get("extensions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if extensions.is_empty() {
        println!("No extensions installed.");
        return Ok(());
    }
    for extension in extensions {
        let id = extension
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let state = extension.get("state").unwrap_or(&Value::Null);
        let version = state
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let enabled = state
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let placement = state
            .get("placement")
            .and_then(Value::as_str)
            .unwrap_or("terminal-pane");
        println!(
            "{id}\t{version}\t{placement}\t{}",
            if enabled { "enabled" } else { "disabled" }
        );
    }
    Ok(())
}

/// `extensions open <id> [--session <ref>] [--placement view] [--background]`: the extension's view
/// in the side panel of the session's project, the calling session's by default
/// (server/src/extensions/open_view.rs decides whether it shows, asks first or opens in the
/// background).
fn open_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    if parsed.flags.truthy("help") || parsed.flags.truthy("h") {
        println!("{}", crate::ghostex_cli::usage::extensions_usage());
        return Ok(());
    }
    let id = one_required_arg("extensions open", &parsed.rest)?;
    if let Some(placement) = parsed.flags.text("placement") {
        if placement.trim() != "view" {
            return Err(CliError::Other(format!(
                "--placement {placement} can't be opened: only --placement view (the side panel) can."
            )));
        }
    }
    let mut payload = Map::new();
    payload.insert("id".to_string(), Value::String(id.clone()));
    if parsed.flags.truthy("background") {
        payload.insert("background".to_string(), Value::Bool(true));
    }
    let selector = parsed
        .flags
        .text("session")
        .or_else(|| parsed.flags.text("sessionId"))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let (project_id, session_id) = match &selector {
        Some(selector) => {
            let mut flags = parsed.flags.clone();
            flags.0.remove("projectId");
            let session =
                crate::ghostex_cli::selector::resolve_cli_session_selector(selector, &flags)?;
            let text = |key: &str| {
                session
                    .get(key)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
            };
            let session_id = text("sessionId");
            let project_id = text("projectId").or_else(|| {
                let global_ref = text("globalRef")?;
                crate::agent_hooks::probing::parse_global_session_ref(&global_ref).0
            });
            match (project_id, session_id) {
                (Some(project_id), Some(session_id)) => (Some(project_id), Some(session_id)),
                _ => {
                    return Err(CliError::Other(format!(
                        "Session \"{selector}\" does not report a project id."
                    )))
                }
            }
        }
        None => match session_reference_from_environment() {
            Some((project_id, session_id)) => (Some(project_id), Some(session_id)),
            None => (None, None),
        },
    };
    if let (Some(project_id), Some(session_id)) = (project_id, session_id) {
        payload.insert("projectId".to_string(), Value::String(project_id));
        payload.insert("sessionId".to_string(), Value::String(session_id));
    }
    let result = call_gxserver_rpc(
        "/api/openExtensionView",
        &Value::Object(payload),
        &parsed.flags,
    )?;
    if parsed.flags.truthy("json") {
        print_json(&result);
        return Ok(());
    }
    let flag = |key: &str| result.get(key) == Some(&Value::Bool(true));
    if flag("toastShown") {
        println!(
            "Asked the user: Ghostex shows an Open button for {id}. It opens when they click it."
        );
    } else if flag("alreadyShowing") {
        println!("{id} is already showing in the side panel.");
    } else if flag("background") {
        println!("Added {id} to the project's side panel without showing it.");
    } else {
        println!("Opened {id} in the side panel.");
    }
    Ok(())
}

/// The calling session's `(projectId, sessionId)`, from the ids every Ghostex pane exports.
fn session_reference_from_environment() -> Option<(String, String)> {
    let text = |key: &str| {
        std::env::var(key)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    };
    if let Some(global_ref) = text("GHOSTEX_GLOBAL_SESSION_REF") {
        if let (Some(project_id), Some(session_id)) =
            crate::agent_hooks::probing::parse_global_session_ref(&global_ref)
        {
            return Some((project_id, session_id));
        }
    }
    let native = text("GHOSTEX_NATIVE_SESSION_ID")?;
    let (project_id, session_id) = native.split_once(':')?;
    let (project_id, session_id) = (project_id.trim(), session_id.trim());
    (!project_id.is_empty() && !session_id.is_empty())
        .then(|| (project_id.to_string(), session_id.to_string()))
}

fn catalog_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    ensure_no_positional_args("extensions catalog", &parsed.rest)?;
    let flags = long_operation_flags(&parsed.flags);
    let result = call_gxserver_rpc("/api/extensionsCatalog", &json!({}), &flags)?;
    if parsed.flags.truthy("json") {
        print_json(&result);
        return Ok(());
    }
    let source = result
        .get("source")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let entries = result
        .get("catalog")
        .and_then(|catalog| catalog.get("extensions"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    println!("Extension catalog ({source}):");
    for entry in entries {
        let id = entry
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let version = entry
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let title = entry.get("title").and_then(Value::as_str).unwrap_or(id);
        println!("{id}\t{version}\t{title}");
    }
    Ok(())
}

fn install_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let id = one_required_arg("extensions install", &parsed.rest)?;
    let flags = long_operation_flags(&parsed.flags);
    let result = call_gxserver_rpc("/api/installExtension", &json!({ "id": id }), &flags)?;
    print_mutation_result(&result, &parsed.flags, "Installed")
}

fn install_local_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let path = one_required_arg("extensions install-local", &parsed.rest)?;
    let path = std::fs::canonicalize(PathBuf::from(path)).map_err(|error| {
        CliError::Other(format!("Could not resolve local extension folder: {error}"))
    })?;
    let flags = long_operation_flags(&parsed.flags);
    let result = call_gxserver_rpc(
        "/api/installExtension",
        &json!({ "localPath": path.to_string_lossy() }),
        &flags,
    )?;
    print_mutation_result(&result, &parsed.flags, "Installed")
}

fn uninstall_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let id = one_required_arg("extensions uninstall", &parsed.rest)?;
    let result = call_gxserver_rpc(
        "/api/uninstallExtension",
        &json!({ "id": id }),
        &parsed.flags,
    )?;
    if parsed.flags.truthy("json") {
        print_json(&result);
    } else {
        println!("Uninstalled {id}.");
    }
    Ok(())
}

fn state_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let id = one_required_arg("extensions state", &parsed.rest)?;
    let patch = parse_state_patch(args)?;
    let result = call_gxserver_rpc(
        "/api/updateExtensionState",
        &json!({ "id": id, "patch": patch }),
        &parsed.flags,
    )?;
    if parsed.flags.truthy("json") {
        print_json(&result);
    } else if let Some(state) = result.get("extension").and_then(|value| value.get("state")) {
        print_json(state);
    }
    Ok(())
}

fn parse_state_patch(args: &[String]) -> CliResult<Value> {
    let assignments = set_assignments(args)?;
    let mut patch = Map::new();
    let mut preferences = Map::new();
    for assignment in assignments {
        let (key, raw_value) = assignment.split_once('=').ok_or_else(|| {
            CliError::Other(format!(
                "Extension state assignment must use k=v: {assignment:?}."
            ))
        })?;
        if key.is_empty() {
            return Err(CliError::Other(
                "Extension state assignment key must not be empty.".to_string(),
            ));
        }
        let value = serde_json::from_str(raw_value)
            .unwrap_or_else(|_| Value::String(raw_value.to_string()));
        if let Some(name) = key.strip_prefix("preferences.") {
            if name.is_empty() {
                return Err(CliError::Other(
                    "Preference assignment requires preferences.<name>.".to_string(),
                ));
            }
            preferences.insert(name.to_string(), value);
            continue;
        }
        match key {
            "enabled" | "pinned" | "sidebarModeHidden" | "sidebarSectionsHidden" => {
                if !value.is_boolean() {
                    return Err(CliError::Other(format!("{key} must be true or false.")));
                }
                patch.insert(key.to_string(), value);
            }
            "placement" | "terminalPlacement" => {
                if !value.is_string() {
                    return Err(CliError::Other(format!("{key} must be a string.")));
                }
                patch.insert(key.to_string(), value);
            }
            "preferences" => {
                let object = value.as_object().ok_or_else(|| {
                    CliError::Other("preferences must be a JSON object.".to_string())
                })?;
                preferences.extend(object.clone());
            }
            "grantedPermissions" => {
                let permissions = match value {
                    Value::Array(values) => Value::Array(values),
                    Value::String(text) => Value::Array(
                        text.split(',')
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .map(|value| Value::String(value.to_string()))
                            .collect(),
                    ),
                    _ => {
                        return Err(CliError::Other(
                            "grantedPermissions must be a JSON array or comma-separated string."
                                .to_string(),
                        ));
                    }
                };
                patch.insert(key.to_string(), permissions);
            }
            _ => {
                return Err(CliError::Other(format!(
                    "Unknown extension state key: {key}."
                )));
            }
        }
    }
    if !preferences.is_empty() {
        patch.insert("preferences".to_string(), Value::Object(preferences));
    }
    Ok(Value::Object(patch))
}

fn set_assignments(args: &[String]) -> CliResult<Vec<String>> {
    let mut assignments = Vec::new();
    let mut index = 0;
    while index < args.len() {
        if let Some(value) = args[index].strip_prefix("--set=") {
            assignments.push(value.to_string());
        } else if args[index] == "--set" {
            let value = args
                .get(index + 1)
                .ok_or_else(|| CliError::Other("--set requires a k=v assignment.".to_string()))?;
            assignments.push(value.clone());
            index += 1;
        }
        index += 1;
    }
    Ok(assignments)
}

fn print_mutation_result(result: &Value, flags: &Flags, action: &str) -> CliResult<()> {
    if flags.truthy("json") {
        print_json(result);
        return Ok(());
    }
    let extension = result
        .get("extension")
        .ok_or_else(|| CliError::Other("gxserver returned no installed extension.".to_string()))?;
    let id = extension
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("extension");
    let version = extension
        .get("state")
        .and_then(|state| state.get("version"))
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    println!("{action} {id} {version}.");
    Ok(())
}

fn long_operation_flags(flags: &Flags) -> Flags {
    let mut flags = flags.clone();
    if !flags.contains("timeout") && !flags.contains("timeoutMs") {
        flags.insert_text("timeout", "180000");
    }
    flags
}

fn one_required_arg(command: &str, rest: &[String]) -> CliResult<String> {
    if rest.len() != 1 || rest[0].trim().is_empty() {
        return Err(CliError::Other(format!(
            "{command} requires exactly one argument."
        )));
    }
    Ok(rest[0].trim().to_string())
}

fn ensure_no_positional_args(command: &str, rest: &[String]) -> CliResult<()> {
    if rest.is_empty() {
        Ok(())
    } else {
        Err(CliError::Other(format!(
            "{command} does not accept positional arguments."
        )))
    }
}
