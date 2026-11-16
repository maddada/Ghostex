use std::path::Path;

use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;

use crate::agent_hooks::config::{
    all_hook_events, command_agent, hook_format, nested_event_timeout, nested_timeout,
    HookDefinition, HookFormat, HookPaths,
};
use crate::agent_hooks::probing::{path_string, read_file_text};
use crate::agent_hooks::resolution::is_ghostex_owned_hook_command;
use crate::agent_hooks::statusline::register_claude_statusline;

use super::*;

/// Merges Ghostex's hook into `config_path` and returns the object it read before the edit: Codex
/// trust keys are positions, so the trust pass after the edit compares the two
/// (codex_trust.rs `follow_codex_hook_trust`).
pub(super) fn merge_json_hook(
    config_path: &Path,
    definition: &HookDefinition,
    hook_paths: &HookPaths,
    command: &str,
) -> Result<Value, DomainStateError> {
    if definition.agent_id == "zcode" {
        super::zcode::ensure_zcode_config(config_path, hook_paths)?;
    }
    let current_text = read_file_text(config_path);
    ensure_json_config_is_rewritable(definition.agent_id, config_path, &current_text)?;
    let mut data = read_json_object(&current_text);
    let before = data.clone();
    let events = all_hook_events(definition.agent_id);
    let format = hook_format(definition.agent_id);
    remove_owned_json_hooks(&mut data, format, command);
    match format {
        HookFormat::Antigravity => {
            let object = ensure_json_object(&mut data);
            let ghostex = ensure_object_property(object, "ghostex");
            for event_name in events {
                ghostex.insert(
                    event_name.to_string(),
                    Value::Array(vec![antigravity_hook_entry(command, event_name)]),
                );
            }
        }
        HookFormat::RootFlatJson => {
            let hooks = ensure_json_object(&mut data);
            for event_name in events {
                let entries = hooks
                    .get(event_name)
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                hooks.insert(
                    event_name.to_string(),
                    Value::Array(merge_flat_hook_entries(
                        &entries,
                        command,
                        Some(json!({ "type": "command", "timeout": 5000 })),
                    )),
                );
            }
        }
        HookFormat::FlatJson => {
            let object = ensure_json_object(&mut data);
            object.entry("version".to_string()).or_insert(json!(1));
            let hooks = ensure_object_property(object, "hooks");
            for event_name in events {
                let entries = hooks
                    .get(event_name)
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                hooks.insert(
                    event_name.to_string(),
                    Value::Array(merge_flat_hook_entries(&entries, command, None)),
                );
            }
        }
        HookFormat::KiroJson => {
            let object = ensure_json_object(&mut data);
            object.entry("name".to_string()).or_insert(json!("ghostex"));
            object
                .entry("description".to_string())
                .or_insert(json!("Ghostex notification hooks for Kiro CLI."));
            object.entry("tools".to_string()).or_insert(json!(["*"]));
            let hooks = ensure_object_property(object, "hooks");
            for event_name in events {
                let entries = hooks
                    .get(event_name)
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                hooks.insert(
                    event_name.to_string(),
                    Value::Array(merge_flat_hook_entries(
                        &entries,
                        command,
                        Some(json!({ "timeout_ms": 5000 })),
                    )),
                );
            }
        }
        HookFormat::NestedJson | HookFormat::NestedEventsJson => {
            let object = ensure_json_object(&mut data);
            let hooks = ensure_object_property(object, "hooks");
            let hooks = if format == HookFormat::NestedEventsJson {
                hooks.entry("enabled").or_insert(json!(true));
                ensure_object_property(hooks, "events")
            } else {
                hooks
            };
            for event_name in events {
                let groups = hooks
                    .get(event_name)
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let mut next_groups = groups;
                if !next_groups
                    .iter()
                    .any(|group| group_contains_hook_command(group, command))
                {
                    let mut group = Map::new();
                    let mut hook = Map::new();
                    hook.insert("type".to_string(), json!("command"));
                    hook.insert("command".to_string(), json!(command));
                    if command_agent(definition.agent_id).is_some()
                        || nested_timeout(definition.agent_id).is_some()
                    {
                        hook.insert(
                            "timeout".to_string(),
                            json!(nested_event_timeout(definition.agent_id, event_name)
                                .unwrap_or(5000)),
                        );
                    }
                    group.insert("hooks".to_string(), Value::Array(vec![Value::Object(hook)]));
                    if let Some(matcher) = nested_hook_matcher(definition.agent_id, event_name) {
                        group.insert("matcher".to_string(), json!(matcher));
                    }
                    next_groups.push(Value::Object(group));
                }
                hooks.insert(event_name.to_string(), Value::Array(next_groups));
            }
        }
        HookFormat::Opencode
        | HookFormat::PluginFile
        | HookFormat::MarkedYaml
        | HookFormat::TomlMarked => {}
    }
    // CDXC:AgentHooks 2026-09-03 WHY: the same settings file carries the
    // statusLine command, registered (or re-pointed) alongside the hooks.
    if definition.agent_id == "claude" {
        register_claude_statusline(&mut data, hook_paths);
        super::claude_retention::ensure_claude_transcript_retention(&mut data);
    }
    write_json_file(config_path, &data)?;
    Ok(before)
}

/// The `matcher` a nested-JSON provider expects on one event group, if any.
///
/// Claude and OpenClaude read `matcher` as a glob and want `"*"` on every
/// group. Command Code reads it as a REGEX and only accepts it on the two tool
/// events, so it gets `".*"` there and nothing on Stop. Devin also treats the
/// field as a regex but matches everything when it is absent, so Ghostex omits
/// it entirely rather than writing a pattern that could filter events out.
fn nested_hook_matcher(agent_id: &str, event_name: &str) -> Option<&'static str> {
    match agent_id {
        "claude" | "openclaude" => Some("*"),
        "command-code" if matches!(event_name, "PreToolUse" | "PostToolUse") => Some(".*"),
        _ => None,
    }
}

/*
CDXC:AgentHooks 2026-08-27:
Devin's `~/.config/devin/config.json` is JSONC — comments are legal and users
have them. `read_json_object` degrades ANY unparsable text to `{}`, so merging
into it would replace the user's whole commented config with a bare hooks
object, silently destroying their settings. Refuse the rewrite instead, on both
the install and the uninstall path, and tell the user what to do.
*/
pub(super) fn ensure_json_config_is_rewritable(
    agent_id: &str,
    config_path: &Path,
    text: &str,
) -> Result<(), DomainStateError> {
    if agent_id != "devin" || text.trim().is_empty() {
        return Ok(());
    }
    if serde_json::from_str::<Value>(text).is_ok() {
        return Ok(());
    }
    Err(DomainStateError::corrupt_state(format!(
        "{} contains JSONC comments or other non-JSON syntax that Ghostex will not rewrite. \
         Edit the Devin hooks entry by hand, or remove the comments and run Install Hooks again.",
        path_string(config_path)
    )))
}

pub(super) fn ensure_json_object(value: &mut Value) -> &mut Map<String, Value> {
    if !value.is_object() {
        *value = json!({});
    }
    value.as_object_mut().expect("json object")
}

pub(super) fn ensure_object_property<'a>(
    object: &'a mut Map<String, Value>,
    key: &str,
) -> &'a mut Map<String, Value> {
    if !object.get(key).map(Value::is_object).unwrap_or(false) {
        object.insert(key.to_string(), json!({}));
    }
    object
        .get_mut(key)
        .and_then(Value::as_object_mut)
        .expect("object property")
}

fn antigravity_hook_entry(command: &str, event_name: &str) -> Value {
    let hook = json!({ "type": "command", "command": command, "timeout": 10 });
    if matches!(event_name, "PreToolUse" | "PostToolUse") {
        json!({ "matcher": "*", "hooks": [hook] })
    } else {
        hook
    }
}

fn merge_flat_hook_entries(entries: &[Value], command: &str, extra: Option<Value>) -> Vec<Value> {
    let mut next = entries
        .iter()
        .filter(|entry| !is_ghostex_owned_hook_command(entry, command))
        .cloned()
        .collect::<Vec<_>>();
    let mut entry = Map::new();
    entry.insert("command".to_string(), json!(command));
    if let Some(extra) = extra.and_then(|value| value.as_object().cloned()) {
        for (key, value) in extra {
            entry.insert(key, value);
        }
    }
    next.push(Value::Object(entry));
    next
}

fn group_contains_hook_command(group: &Value, command: &str) -> bool {
    group
        .get("hooks")
        .and_then(Value::as_array)
        .map(|hooks| hooks.iter().any(|hook| is_hook_command(hook, command)))
        .unwrap_or(false)
}

pub(super) fn is_hook_command(value: &Value, command: &str) -> bool {
    value.get("command").and_then(Value::as_str) == Some(command)
}
