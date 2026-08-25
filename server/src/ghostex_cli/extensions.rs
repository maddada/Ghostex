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
            "enabled" | "pinned" => {
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
