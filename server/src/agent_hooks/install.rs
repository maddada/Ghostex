use std::{
    env, fs,
    path::{Path, PathBuf},
};

use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;

use super::config::{
    all_hook_events, command_agent, hook_format, hook_marker, nested_timeout, HookDefinition,
    HookFormat, HookPaths, NOTIFY_HOOK_MARKER, NOTIFY_HOOK_VERSION, OPENCODE_PLUGIN_MARKER,
    OPENCODE_PLUGIN_SPEC,
};
use super::plugin_sources::{
    build_notify_hook_script, build_opencode_plugin_source, build_plugin_file_source,
    command_for_agent, current_plugin_marker, shell_quote, yaml_double_quote,
};
use super::probing::{
    io_error, json_error, path_string, push_unique_path, read_file_text, temp_path_for,
};
use super::resolution::{
    is_ghostex_owned_hook_command, is_opencode_session_plugin_registration, provider_hook_paths,
    text_contains_ghostex_owned_hook_command,
};

pub(crate) fn uninstall_agent_hook(
    definition: &HookDefinition,
    hook_paths: &HookPaths,
) -> Result<Vec<String>, DomainStateError> {
    if hook_format(definition.agent_id) == HookFormat::Opencode {
        return uninstall_opencode_hook(hook_paths);
    }
    let config_paths = provider_hook_paths(definition.agent_id, hook_paths);
    let command = command_for_agent(definition, &hook_paths.notify_hook_path);
    match hook_format(definition.agent_id) {
        HookFormat::PluginFile => uninstall_plugin_file_hook(definition, config_paths),
        HookFormat::MarkedYaml => uninstall_marked_yaml_hook(definition, config_paths),
        HookFormat::Antigravity
        | HookFormat::FlatJson
        | HookFormat::KiroJson
        | HookFormat::NestedJson => {
            let mut removed_paths = Vec::new();
            for config_path in config_paths {
                if remove_json_hook(&config_path, definition, &command)? {
                    removed_paths.push(path_string(&config_path));
                }
            }
            Ok(removed_paths)
        }
        HookFormat::Opencode => Ok(Vec::new()),
    }
}

pub(crate) fn uninstall_plugin_file_hook(
    definition: &HookDefinition,
    config_paths: Vec<PathBuf>,
) -> Result<Vec<String>, DomainStateError> {
    let mut removed_paths = Vec::new();
    for config_path in config_paths {
        let text = read_file_text(&config_path);
        if !text_contains_ghostex_owned_hook_command(&text)
            && hook_marker(definition.agent_id)
                .map(|marker| !text.contains(marker))
                .unwrap_or(true)
        {
            continue;
        }
        if remove_file_if_exists(&config_path)? {
            push_unique_path(&mut removed_paths, path_string(&config_path));
        }
    }
    Ok(removed_paths)
}

pub(crate) fn uninstall_marked_yaml_hook(
    definition: &HookDefinition,
    config_paths: Vec<PathBuf>,
) -> Result<Vec<String>, DomainStateError> {
    let Some(config_path) = config_paths.into_iter().next() else {
        return Ok(Vec::new());
    };
    let begin_marker = format!("# ghostex hooks {} begin", definition.agent_id);
    let end_marker = format!("# ghostex hooks {} end", definition.agent_id);
    let current_text = read_file_text(&config_path);
    let normalized_text = current_text.replace("\r\n", "\n");
    let lines = normalized_text
        .split('\n')
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let next_lines = without_marked_block(&lines, &begin_marker, &end_marker);
    let next_text = format!("{}\n", next_lines.join("\n").trim_end_matches('\n'));
    if current_text == next_text {
        return Ok(Vec::new());
    }
    fs::write(&config_path, next_text).map_err(io_error)?;
    Ok(vec![path_string(&config_path)])
}

fn uninstall_opencode_hook(hook_paths: &HookPaths) -> Result<Vec<String>, DomainStateError> {
    let paths = provider_hook_paths("opencode", hook_paths);
    let Some(plugin_path) = paths.first() else {
        return Ok(Vec::new());
    };
    let Some(config_path) = paths.get(1) else {
        return Ok(Vec::new());
    };
    uninstall_opencode_hook_paths(plugin_path, config_path)
}

pub(crate) fn uninstall_opencode_hook_paths(
    plugin_path: &Path,
    config_path: &Path,
) -> Result<Vec<String>, DomainStateError> {
    let mut removed_paths = Vec::new();
    let config_text = read_file_text(config_path);
    if !config_text.trim().is_empty() {
        let mut data = read_json_object(&config_text);
        if let Some(object) = data.as_object_mut() {
            if let Some(plugins) = object.get_mut("plugin").and_then(Value::as_array_mut) {
                let next_plugins = plugins
                    .iter()
                    .filter(|plugin| !is_opencode_session_plugin_registration(plugin))
                    .cloned()
                    .collect::<Vec<_>>();
                if next_plugins.len() != plugins.len() {
                    *plugins = next_plugins;
                    write_json_file(config_path, &data)?;
                    push_unique_path(&mut removed_paths, path_string(config_path));
                }
            }
        }
    }
    let plugin_text = read_file_text(plugin_path);
    if (plugin_text.contains(OPENCODE_PLUGIN_MARKER)
        || text_contains_ghostex_owned_hook_command(&plugin_text))
        && remove_file_if_exists(plugin_path)?
    {
        push_unique_path(&mut removed_paths, path_string(plugin_path));
    }
    Ok(removed_paths)
}

pub(crate) fn remove_json_hook(
    config_path: &Path,
    definition: &HookDefinition,
    command: &str,
) -> Result<bool, DomainStateError> {
    let current_text = read_file_text(config_path);
    if current_text.trim().is_empty() {
        return Ok(false);
    }
    let mut data = read_json_object(&current_text);
    let changed = remove_owned_json_hooks(&mut data, hook_format(definition.agent_id), command);
    if !changed {
        return Ok(false);
    }
    write_json_file(config_path, &data)?;
    Ok(true)
}

fn remove_owned_json_hooks(data: &mut Value, format: HookFormat, command: &str) -> bool {
    let event_groups = if format == HookFormat::Antigravity {
        data.get_mut("ghostex").and_then(Value::as_object_mut)
    } else {
        data.get_mut("hooks").and_then(Value::as_object_mut)
    };
    let Some(event_groups) = event_groups else {
        return false;
    };

    let mut changed = false;
    let mut emptied_events = Vec::new();
    for (event_name, entries) in event_groups
        .iter_mut()
        .filter_map(|(event_name, value)| value.as_array_mut().map(|entries| (event_name, entries)))
    {
        let next_entries = match format {
            HookFormat::Antigravity => remove_antigravity_entries(entries, command),
            HookFormat::FlatJson | HookFormat::KiroJson => entries
                .iter()
                .filter(|entry| !is_ghostex_owned_hook_command(entry, command))
                .cloned()
                .collect::<Vec<_>>(),
            HookFormat::NestedJson => remove_nested_hook_groups(entries, command),
            HookFormat::Opencode | HookFormat::PluginFile | HookFormat::MarkedYaml => continue,
        };
        let event_changed = next_entries != *entries;
        if event_changed && next_entries.is_empty() {
            emptied_events.push(event_name.clone());
        }
        changed = changed || event_changed;
        *entries = next_entries;
    }
    for event_name in emptied_events {
        event_groups.remove(&event_name);
    }
    changed
}

fn remove_antigravity_entries(entries: &[Value], command: &str) -> Vec<Value> {
    let mut next_entries = Vec::new();
    for entry in entries {
        if is_ghostex_owned_hook_command(entry, command) {
            continue;
        }
        let Some(object) = entry.as_object() else {
            next_entries.push(entry.clone());
            continue;
        };
        let Some(hooks) = object.get("hooks").and_then(Value::as_array) else {
            next_entries.push(entry.clone());
            continue;
        };
        let next_hooks = hooks
            .iter()
            .filter(|hook| !is_ghostex_owned_hook_command(hook, command))
            .cloned()
            .collect::<Vec<_>>();
        if !next_hooks.is_empty() {
            let mut next_entry = object.clone();
            next_entry.insert("hooks".to_string(), Value::Array(next_hooks));
            next_entries.push(Value::Object(next_entry));
        }
    }
    next_entries
}

fn remove_nested_hook_groups(groups: &[Value], command: &str) -> Vec<Value> {
    let mut next_groups = Vec::new();
    for group in groups {
        let Some(object) = group.as_object() else {
            next_groups.push(group.clone());
            continue;
        };
        let Some(hooks) = object.get("hooks").and_then(Value::as_array) else {
            next_groups.push(group.clone());
            continue;
        };
        let next_hooks = hooks
            .iter()
            .filter(|hook| !is_ghostex_owned_hook_command(hook, command))
            .cloned()
            .collect::<Vec<_>>();
        if !next_hooks.is_empty() {
            let mut next_group = object.clone();
            next_group.insert("hooks".to_string(), Value::Array(next_hooks));
            next_groups.push(Value::Object(next_group));
        }
    }
    next_groups
}

fn without_marked_block(lines: &[String], begin_marker: &str, end_marker: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        if lines[index].trim() != begin_marker {
            result.push(lines[index].clone());
            index += 1;
            continue;
        }
        while index < lines.len() && lines[index].trim() != end_marker {
            index += 1;
        }
        if index < lines.len() {
            index += 1;
        }
    }
    while result.last().map(|line| line.trim().is_empty()) == Some(true) {
        result.pop();
    }
    result
}

pub(crate) struct HookInspection {
    pub(crate) current_hook_installed: bool,
    pub(crate) ghostex_hook_present: bool,
}

pub(crate) fn inspect_agent_hook_installation(
    definition: &HookDefinition,
    hook_paths: &HookPaths,
    config_paths: &[PathBuf],
) -> HookInspection {
    let command = command_for_agent(definition, &hook_paths.notify_hook_path);
    match hook_format(definition.agent_id) {
        HookFormat::Opencode => {
            let plugin_text = config_paths
                .first()
                .map(|path| read_file_text(path))
                .unwrap_or_default();
            let config_text = config_paths
                .get(1)
                .map(|path| read_file_text(path))
                .unwrap_or_default();
            let current = plugin_text.contains(&current_plugin_marker(OPENCODE_PLUGIN_MARKER))
                && plugin_text.contains(&path_string(&hook_paths.notify_hook_path))
                && config_text.contains(OPENCODE_PLUGIN_SPEC);
            HookInspection {
                current_hook_installed: current,
                ghostex_hook_present: current
                    || plugin_text.contains(OPENCODE_PLUGIN_MARKER)
                    || config_text.contains(OPENCODE_PLUGIN_SPEC),
            }
        }
        HookFormat::PluginFile => {
            let marker = hook_marker(definition.agent_id).unwrap_or_default();
            let inspections = config_paths
                .iter()
                .map(|path| {
                    let text = read_file_text(path);
                    let current = !marker.is_empty()
                        && text.contains(&current_plugin_marker(marker))
                        && text.contains(&path_string(&hook_paths.notify_hook_path));
                    HookInspection {
                        current_hook_installed: current,
                        ghostex_hook_present: current
                            || (!marker.is_empty() && text.contains(marker))
                            || text_contains_ghostex_owned_hook_command(&text),
                    }
                })
                .collect::<Vec<_>>();
            HookInspection {
                current_hook_installed: inspections
                    .iter()
                    .any(|inspection| inspection.current_hook_installed),
                ghostex_hook_present: inspections
                    .iter()
                    .any(|inspection| inspection.ghostex_hook_present),
            }
        }
        HookFormat::MarkedYaml => {
            let text = config_paths
                .first()
                .map(|path| read_file_text(path))
                .unwrap_or_default();
            let marker = format!("ghostex hooks {} begin", definition.agent_id);
            let current =
                text.contains(&marker) && text.contains(&path_string(&hook_paths.notify_hook_path));
            HookInspection {
                current_hook_installed: current,
                ghostex_hook_present: current
                    || text.contains(&marker)
                    || text_contains_ghostex_owned_hook_command(&text),
            }
        }
        HookFormat::Antigravity => {
            let text = config_paths
                .first()
                .map(|path| read_file_text(path))
                .unwrap_or_default();
            let current = text.contains(&command);
            HookInspection {
                current_hook_installed: current,
                ghostex_hook_present: current
                    || text.contains("\"ghostex\"")
                    || text_contains_ghostex_owned_hook_command(&text),
            }
        }
        HookFormat::FlatJson | HookFormat::KiroJson | HookFormat::NestedJson => {
            let existing_paths = config_paths
                .iter()
                .filter(|path| !read_file_text(path).trim().is_empty())
                .cloned()
                .collect::<Vec<_>>();
            let should_inspect_all =
                matches!(definition.agent_id, "codex" | "claude") && !existing_paths.is_empty();
            let paths_to_check = if should_inspect_all {
                existing_paths
            } else {
                config_paths.iter().take(1).cloned().collect()
            };
            if paths_to_check.is_empty() {
                return HookInspection {
                    current_hook_installed: false,
                    ghostex_hook_present: false,
                };
            }
            let inspections = paths_to_check
                .iter()
                .map(|path| inspect_json_hook_config(path, &command))
                .collect::<Vec<_>>();
            let installed_inspections = inspections
                .iter()
                .filter(|inspection| inspection.ghostex_hook_present)
                .collect::<Vec<_>>();
            HookInspection {
                current_hook_installed: !installed_inspections.is_empty()
                    && installed_inspections
                        .iter()
                        .all(|inspection| inspection.current_hook_installed),
                ghostex_hook_present: !installed_inspections.is_empty(),
            }
        }
    }
}

fn inspect_json_hook_config(config_path: &Path, command: &str) -> HookInspection {
    let data = read_json_object(&read_file_text(config_path));
    let stale_ghostex_hook_present = json_contains_stale_ghostex_owned_hook_command(&data, command);
    HookInspection {
        current_hook_installed: json_contains_hook_command(&data, command)
            && !stale_ghostex_hook_present,
        ghostex_hook_present: json_contains_ghostex_owned_hook_command(&data, command),
    }
}

fn json_contains_stale_ghostex_owned_hook_command(value: &Value, command: &str) -> bool {
    if is_ghostex_owned_hook_command(value, command) && !is_hook_command(value, command) {
        return true;
    }
    if let Some(array) = value.as_array() {
        return array
            .iter()
            .any(|item| json_contains_stale_ghostex_owned_hook_command(item, command));
    }
    if let Some(object) = value.as_object() {
        return object
            .values()
            .any(|item| json_contains_stale_ghostex_owned_hook_command(item, command));
    }
    false
}

pub(crate) fn json_contains_hook_command(value: &Value, command: &str) -> bool {
    if is_hook_command(value, command) {
        return true;
    }
    if let Some(array) = value.as_array() {
        return array
            .iter()
            .any(|item| json_contains_hook_command(item, command));
    }
    if let Some(object) = value.as_object() {
        return object
            .values()
            .any(|item| json_contains_hook_command(item, command));
    }
    false
}

fn json_contains_ghostex_owned_hook_command(value: &Value, command: &str) -> bool {
    if is_ghostex_owned_hook_command(value, command) {
        return true;
    }
    if let Some(array) = value.as_array() {
        return array
            .iter()
            .any(|item| json_contains_ghostex_owned_hook_command(item, command));
    }
    if let Some(object) = value.as_object() {
        return object
            .values()
            .any(|item| json_contains_ghostex_owned_hook_command(item, command));
    }
    false
}

pub(crate) fn read_json_object(text: &str) -> Value {
    if text.trim().is_empty() {
        return json!({});
    }
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(object)) => Value::Object(object),
        _ => json!({}),
    }
}

fn write_json_file(path: &Path, data: &Value) -> Result<(), DomainStateError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    let text = format!(
        "{}\n",
        serde_json::to_string_pretty(data).map_err(json_error)?
    );
    fs::write(path, text).map_err(io_error)
}

fn remove_file_if_exists(path: &Path) -> Result<bool, DomainStateError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(io_error(error)),
    }
}

pub(crate) fn install_agent_hook(
    definition: &HookDefinition,
    hook_paths: &HookPaths,
) -> Result<Vec<String>, DomainStateError> {
    if hook_format(definition.agent_id) == HookFormat::Opencode {
        return install_opencode_hook(hook_paths);
    }
    let config_paths = provider_hook_paths(definition.agent_id, hook_paths);
    let command = command_for_agent(definition, &hook_paths.notify_hook_path);
    match hook_format(definition.agent_id) {
        HookFormat::PluginFile => {
            let Some(config_path) = config_paths.first() else {
                return Ok(Vec::new());
            };
            if let Some(parent) = config_path.parent() {
                fs::create_dir_all(parent).map_err(io_error)?;
            }
            fs::write(
                config_path,
                build_plugin_file_source(definition.agent_id, &hook_paths.notify_hook_path),
            )
            .map_err(io_error)?;
            Ok(vec![path_string(config_path)])
        }
        HookFormat::MarkedYaml => {
            let Some(config_path) = config_paths.first() else {
                return Ok(Vec::new());
            };
            install_marked_yaml_hook(config_path, definition.agent_id, &command)?;
            Ok(vec![path_string(config_path)])
        }
        HookFormat::Antigravity
        | HookFormat::FlatJson
        | HookFormat::KiroJson
        | HookFormat::NestedJson => {
            let mut installed_paths = Vec::new();
            for config_path in config_paths {
                merge_json_hook(&config_path, definition, &command)?;
                installed_paths.push(path_string(&config_path));
            }
            Ok(installed_paths)
        }
        HookFormat::Opencode => Ok(Vec::new()),
    }
}

pub(crate) fn repair_agent_hook_paths(
    definition: &HookDefinition,
    hook_paths: &HookPaths,
    config_paths: Vec<PathBuf>,
) -> Result<Vec<String>, DomainStateError> {
    match hook_format(definition.agent_id) {
        HookFormat::Opencode => install_opencode_hook(hook_paths),
        HookFormat::PluginFile => {
            let source =
                build_plugin_file_source(definition.agent_id, &hook_paths.notify_hook_path);
            let mut repaired_paths = Vec::new();
            for config_path in config_paths {
                if let Some(parent) = config_path.parent() {
                    fs::create_dir_all(parent).map_err(io_error)?;
                }
                fs::write(&config_path, &source).map_err(io_error)?;
                repaired_paths.push(path_string(&config_path));
            }
            Ok(repaired_paths)
        }
        HookFormat::MarkedYaml => {
            let command = command_for_agent(definition, &hook_paths.notify_hook_path);
            let mut repaired_paths = Vec::new();
            for config_path in config_paths {
                install_marked_yaml_hook(&config_path, definition.agent_id, &command)?;
                repaired_paths.push(path_string(&config_path));
            }
            Ok(repaired_paths)
        }
        HookFormat::Antigravity
        | HookFormat::FlatJson
        | HookFormat::KiroJson
        | HookFormat::NestedJson => {
            let command = command_for_agent(definition, &hook_paths.notify_hook_path);
            let mut repaired_paths = Vec::new();
            for config_path in config_paths {
                merge_json_hook(&config_path, definition, &command)?;
                repaired_paths.push(path_string(&config_path));
            }
            Ok(repaired_paths)
        }
    }
}

fn merge_json_hook(
    config_path: &Path,
    definition: &HookDefinition,
    command: &str,
) -> Result<(), DomainStateError> {
    let mut data = read_json_object(&read_file_text(config_path));
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
        HookFormat::NestedJson => {
            let object = ensure_json_object(&mut data);
            let hooks = ensure_object_property(object, "hooks");
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
                            json!(nested_timeout(definition.agent_id).unwrap_or(5000)),
                        );
                    }
                    group.insert("hooks".to_string(), Value::Array(vec![Value::Object(hook)]));
                    if definition.agent_id == "claude" {
                        group.insert("matcher".to_string(), json!("*"));
                    }
                    next_groups.push(Value::Object(group));
                }
                hooks.insert(event_name.to_string(), Value::Array(next_groups));
            }
        }
        HookFormat::Opencode | HookFormat::PluginFile | HookFormat::MarkedYaml => {}
    }
    write_json_file(config_path, &data)
}

fn ensure_json_object(value: &mut Value) -> &mut Map<String, Value> {
    if !value.is_object() {
        *value = json!({});
    }
    value.as_object_mut().expect("json object")
}

fn ensure_object_property<'a>(
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

fn is_hook_command(value: &Value, command: &str) -> bool {
    value.get("command").and_then(Value::as_str) == Some(command)
}

fn install_marked_yaml_hook(
    config_path: &Path,
    agent_id: &str,
    command: &str,
) -> Result<(), DomainStateError> {
    let begin_marker = format!("# ghostex hooks {agent_id} begin");
    let end_marker = format!("# ghostex hooks {agent_id} end");
    let current_text = read_file_text(config_path);
    let current_lines = current_text
        .replace("\r\n", "\n")
        .split('\n')
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let mut lines = without_marked_block(&current_lines, &begin_marker, &end_marker);
    if lines.last().map(|line| !line.trim().is_empty()) == Some(true) {
        lines.push(String::new());
    }
    if agent_id == "hermes-agent" {
        let shell_command = format!("sh -c {}", shell_quote(command));
        lines.extend([
            begin_marker.clone(),
            "hooks:".to_string(),
            "  on_session_start:".to_string(),
            format!("    - command: {}", yaml_double_quote(&shell_command)),
            "      timeout: 5".to_string(),
            "  pre_llm_call:".to_string(),
            format!("    - command: {}", yaml_double_quote(&shell_command)),
            "      timeout: 5".to_string(),
            "  post_llm_call:".to_string(),
            format!("    - command: {}", yaml_double_quote(&shell_command)),
            "      timeout: 5".to_string(),
            "  pre_approval_request:".to_string(),
            format!("    - command: {}", yaml_double_quote(&shell_command)),
            "      timeout: 5".to_string(),
            "  post_approval_response:".to_string(),
            format!("    - command: {}", yaml_double_quote(&shell_command)),
            "      timeout: 5".to_string(),
            "  on_session_end:".to_string(),
            format!("    - command: {}", yaml_double_quote(&shell_command)),
            "      timeout: 5".to_string(),
            "  on_session_finalize:".to_string(),
            format!("    - command: {}", yaml_double_quote(&shell_command)),
            "      timeout: 5".to_string(),
            "  on_session_reset:".to_string(),
            format!("    - command: {}", yaml_double_quote(&shell_command)),
            "      timeout: 5".to_string(),
            "  pre_tool_call:".to_string(),
            format!("    - command: {}", yaml_double_quote(&shell_command)),
            "      timeout: 120".to_string(),
            "  post_tool_call:".to_string(),
            format!("    - command: {}", yaml_double_quote(&shell_command)),
            "      timeout: 120".to_string(),
            end_marker,
        ]);
    } else {
        lines.extend([
            begin_marker.clone(),
            "eventHooks:".to_string(),
            "  events:".to_string(),
            "    - name: on_complete".to_string(),
            "      commands:".to_string(),
            format!("        - command: {}", yaml_double_quote(command)),
            "    - name: on_error".to_string(),
            "      commands:".to_string(),
            format!("        - command: {}", yaml_double_quote(command)),
            "    - name: on_tool_permission".to_string(),
            "      commands:".to_string(),
            format!("        - command: {}", yaml_double_quote(command)),
            end_marker,
        ]);
    }
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    fs::write(
        config_path,
        format!("{}\n", lines.join("\n").trim_end_matches('\n')),
    )
    .map_err(io_error)
}

fn install_opencode_hook(hook_paths: &HookPaths) -> Result<Vec<String>, DomainStateError> {
    let paths = provider_hook_paths("opencode", hook_paths);
    let Some(plugin_path) = paths.first() else {
        return Ok(Vec::new());
    };
    let Some(config_path) = paths.get(1) else {
        return Ok(Vec::new());
    };
    if let Some(parent) = plugin_path.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    fs::write(
        plugin_path,
        build_opencode_plugin_source(&hook_paths.notify_hook_path),
    )
    .map_err(io_error)?;
    update_opencode_config_plugin_registration(config_path)?;
    Ok(vec![path_string(plugin_path)])
}

fn update_opencode_config_plugin_registration(config_path: &Path) -> Result<(), DomainStateError> {
    let mut data = read_json_object(&read_file_text(config_path));
    let object = ensure_json_object(&mut data);
    let plugins = object
        .get("plugin")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut next_plugins = plugins
        .into_iter()
        .filter(|plugin| !is_opencode_session_plugin_registration(plugin))
        .collect::<Vec<_>>();
    next_plugins.push(json!(OPENCODE_PLUGIN_SPEC));
    object.insert("plugin".to_string(), Value::Array(next_plugins));
    write_json_file(config_path, &data)
}

pub(crate) fn install_notify_hook(hook_paths: &HookPaths) -> Result<(), DomainStateError> {
    if let Some(parent) = hook_paths.notify_hook_path.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    fs::create_dir_all(&hook_paths.hook_state_directory).map_err(io_error)?;
    let executable = env::current_exe()
        .ok()
        .map(|path| path_string(&path))
        .unwrap_or_else(|| "gxserver".to_string());
    let script = build_notify_hook_script(&executable, &hook_paths.hook_state_directory);
    write_executable_notify_hook(&hook_paths.notify_hook_path, &script)?;
    Ok(())
}

fn write_executable_notify_hook(path: &Path, contents: &str) -> Result<(), DomainStateError> {
    let temp_path = temp_path_for(path);
    fs::write(&temp_path, contents).map_err(io_error)?;
    set_executable_permissions(&temp_path).map_err(io_error)?;
    remove_macos_notify_hook_execution_attributes(&temp_path);
    fs::rename(&temp_path, path).map_err(io_error)?;
    set_executable_permissions(path).map_err(io_error)?;
    remove_macos_notify_hook_execution_attributes(path);
    Ok(())
}

#[cfg(unix)]
pub(crate) fn set_executable_permissions(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
}

#[cfg(not(unix))]
pub(crate) fn set_executable_permissions(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(unix)]
pub(crate) fn parent_process_id() -> u32 {
    unsafe { libc::getppid() as u32 }
}

#[cfg(windows)]
pub(crate) fn parent_process_id() -> u32 {
    use std::mem::{size_of, zeroed};
    use windows_sys::Win32::{
        Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
        System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        },
    };

    let current_pid = std::process::id();
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return 0;
        }
        let mut entry: PROCESSENTRY32W = zeroed();
        entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        let mut found = Process32FirstW(snapshot, &mut entry) != 0;
        let mut parent_pid = 0;
        while found {
            if entry.th32ProcessID == current_pid {
                parent_pid = entry.th32ParentProcessID;
                break;
            }
            found = Process32NextW(snapshot, &mut entry) != 0;
        }
        CloseHandle(snapshot);
        parent_pid
    }
}

fn remove_macos_notify_hook_execution_attributes(path: &Path) {
    if std::env::consts::OS != "macos" {
        return;
    }
    for attribute in ["com.apple.quarantine", "com.apple.provenance"] {
        let _ = std::process::Command::new("/usr/bin/xattr")
            .arg("-d")
            .arg(attribute)
            .arg(path)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

pub(crate) fn notify_hook_state_directory(contents: &str) -> Option<PathBuf> {
    let value = contents
        .lines()
        .find_map(|line| line.strip_prefix("DEFAULT_HOOK_STATE_DIR="))?;
    let inner = value.strip_prefix('\'')?.strip_suffix('\'')?;
    let path = PathBuf::from(inner.replace("'\\''", "'"));
    (path.is_absolute() && path.file_name().and_then(|name| name.to_str()) == Some("agent-hooks"))
        .then_some(path)
}

pub(crate) fn migrate_hook_session_sidecars(
    source_directory: &Path,
    destination_directory: &Path,
) -> Result<Vec<String>, DomainStateError> {
    if source_directory == destination_directory || !source_directory.is_dir() {
        return Ok(Vec::new());
    }
    let mut migrated_paths = Vec::new();
    for entry in fs::read_dir(source_directory).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        if !entry.file_type().map_err(io_error)?.is_file() {
            continue;
        }
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        if !file_name.ends_with("-hook-sessions.json") {
            continue;
        }
        let source_data = read_json_object(&read_file_text(&entry.path()));
        let Some(source_sessions) = source_data.get("sessions").and_then(Value::as_object) else {
            continue;
        };
        let destination_path = destination_directory.join(file_name);
        let mut destination_data = read_json_object(&read_file_text(&destination_path));
        let destination_object = destination_data.as_object_mut().expect("JSON object");
        let destination_sessions = destination_object
            .entry("sessions".to_string())
            .or_insert_with(|| json!({}));
        if !destination_sessions.is_object() {
            *destination_sessions = json!({});
        }
        let destination_sessions = destination_sessions
            .as_object_mut()
            .expect("sessions object");
        let mut changed = false;
        for (session_id, source_session) in source_sessions {
            let source_updated_at = source_session
                .get("updatedAt")
                .and_then(Value::as_f64)
                .unwrap_or_default();
            let destination_updated_at = destination_sessions
                .get(session_id)
                .and_then(|session| session.get("updatedAt"))
                .and_then(Value::as_f64)
                .unwrap_or_default();
            if !destination_sessions.contains_key(session_id)
                || source_updated_at > destination_updated_at
            {
                destination_sessions.insert(session_id.clone(), source_session.clone());
                changed = true;
            }
        }
        if changed {
            write_json_file(&destination_path, &destination_data)?;
            migrated_paths.push(path_string(&destination_path));
        }
    }
    Ok(migrated_paths)
}

pub(crate) fn is_notify_hook_current(hook_paths: &HookPaths, contents: &str) -> bool {
    /*
    CDXC:AgentHooks 2026-06-22-08:23:
    Area 27 parity requires Rust status/install/uninstall to treat the TypeScript gxserver v6 hook marker as the shared notify-hook currency contract. Do not require Rust-only helper text here; existing gxserver-owned v6 hooks should stay installed instead of forcing a needless updateRequired state.

    The marker alone is not enough when Ghostex's resolved state directory
    changes (for example, after moving from the macOS Application Support
    layout to XDG state). The hook embeds that directory at install time, so a
    marker-current script pointing at a different directory is stale and must
    be rewritten before live Codex identity repair can consume its sidecar.
    */
    let state_directory_assignment = format!(
        "DEFAULT_HOOK_STATE_DIR={}",
        shell_quote(&path_string(&hook_paths.hook_state_directory))
    );
    contents.contains(&format!("{NOTIFY_HOOK_MARKER} v{NOTIFY_HOOK_VERSION}"))
        && contents
            .lines()
            .any(|line| line == state_directory_assignment)
}
