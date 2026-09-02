use std::{
    env, fs,
    path::{Path, PathBuf},
};

use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;

use super::codex_trust::{
    ghostex_codex_hook_trust_entries, remove_codex_hook_trust_entries, trust_ghostex_codex_hooks,
    CodexTrustWriteMode,
};
use super::config::{
    all_hook_events, command_agent, hook_format, hook_marker, nested_event_timeout, nested_timeout,
    pi_extension_path_is_loader_visible, HookDefinition, HookFormat, HookPaths,
    CODEX_INTERRUPT_HOOK_TIMEOUT_SECONDS, NOTIFY_HOOK_MARKER, NOTIFY_HOOK_VERSION,
    OPENCODE_PLUGIN_MARKER, OPENCODE_PLUGIN_SPEC,
};
use super::plugin_sources::{
    build_notify_hook_script, build_opencode_plugin_source, build_plugin_file_source,
    command_for_agent, current_plugin_marker, shell_quote, toml_basic_quote, yaml_double_quote,
};
use super::probing::{
    io_error, json_error, path_string, push_unique_path, read_file_text, temp_path_for,
};
use super::resolution::{
    is_ghostex_owned_hook_command, is_opencode_session_plugin_registration, provider_hook_paths,
    text_contains_ghostex_owned_hook_command,
};
use super::statusline::{
    claude_statusline_is_current, register_claude_statusline, unregister_claude_statusline,
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
        HookFormat::MarkedYaml | HookFormat::TomlMarked => {
            uninstall_marked_yaml_hook(definition, config_paths)
        }
        HookFormat::Antigravity
        | HookFormat::FlatJson
        | HookFormat::KiroJson
        | HookFormat::NestedJson => {
            let mut removed_paths = Vec::new();
            for config_path in config_paths {
                // CDXC:CodexHookTrust 2026-09-02: state keys are positional, so
                // they must be read before the hooks leave the file.
                let codex_trust_keys = (definition.agent_id == "codex")
                    .then(|| {
                        ghostex_codex_hook_trust_entries(&config_path, hook_paths, &command)
                            .into_iter()
                            .map(|entry| entry.key)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                if remove_json_hook(&config_path, definition, &command)? {
                    removed_paths.push(path_string(&config_path));
                    if remove_codex_hook_trust_entries(&config_path, &codex_trust_keys)? {
                        let config_toml =
                            super::codex_trust::codex_config_path_for_hooks(&config_path);
                        removed_paths.push(path_string(&config_toml));
                    }
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

/// Removes the `# ghostex hooks <agent> begin/end` block from a marked config.
/// Shared by [`HookFormat::MarkedYaml`] and [`HookFormat::TomlMarked`]: the
/// marker comments are byte-identical in YAML and TOML, and only the block body
/// differs between the two.
pub(crate) fn uninstall_marked_yaml_hook(
    definition: &HookDefinition,
    config_paths: Vec<PathBuf>,
) -> Result<Vec<String>, DomainStateError> {
    let Some(config_path) = config_paths.into_iter().next() else {
        return Ok(Vec::new());
    };
    // Withdraw the recorded approvals together with the hook entries they
    // covered, even when the marked block itself is already gone.
    if definition.agent_id == "hermes-agent" {
        update_hermes_shell_hook_allowlist(&config_path, None)?;
    }
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
    ensure_json_config_is_rewritable(definition.agent_id, config_path, &current_text)?;
    let mut data = read_json_object(&current_text);
    let mut changed = remove_owned_json_hooks(&mut data, hook_format(definition.agent_id), command);
    // CDXC:ClaudeStatusline 2026-09-03: the statusLine Ghostex registered (or
    // wrapped) leaves with the hooks, restoring the user's own command.
    if definition.agent_id == "claude" {
        changed |= unregister_claude_statusline(&mut data);
    }
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
            HookFormat::Opencode
            | HookFormat::PluginFile
            | HookFormat::MarkedYaml
            | HookFormat::TomlMarked => continue,
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
                    // A pi extension outside the loader-visible agent
                    // directory exists but never runs, so however fresh its
                    // marker is it cannot count as a current install — only
                    // as a present (stale) one for the repair pass to migrate.
                    let loader_visible = definition.agent_id != "pi"
                        || pi_extension_path_is_loader_visible(
                            &hook_paths.home_dir,
                            hook_paths.respect_config_environment,
                            path,
                        );
                    let current = loader_visible
                        && !marker.is_empty()
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
        HookFormat::MarkedYaml | HookFormat::TomlMarked => {
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
            let current = text.contains(&command)
                && json_hook_event_coverage_is_current(
                    &read_json_object(&text),
                    definition.agent_id,
                    &command,
                    HookFormat::Antigravity,
                );
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
                .map(|path| inspect_json_hook_config(path, definition, hook_paths, &command))
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

fn inspect_json_hook_config(
    config_path: &Path,
    definition: &HookDefinition,
    hook_paths: &HookPaths,
    command: &str,
) -> HookInspection {
    let data = read_json_object(&read_file_text(config_path));
    let stale_ghostex_hook_present = json_contains_stale_ghostex_owned_hook_command(&data, command);
    let codex_interrupt_timeout_current =
        definition.agent_id != "codex" || codex_interrupt_hook_timeout_is_current(&data, command);
    // CDXC:ClaudeStatusline 2026-09-03: a Claude install is only current once
    // its statusLine runs the Ghostex script, so an older install reads as
    // updateRequired and the Update Hooks button (or daemon repair) adds it.
    let claude_statusline_current =
        definition.agent_id != "claude" || claude_statusline_is_current(&data, hook_paths);
    HookInspection {
        current_hook_installed: json_contains_hook_command(&data, command)
            && !stale_ghostex_hook_present
            && json_hook_event_coverage_is_current(
                &data,
                definition.agent_id,
                command,
                hook_format(definition.agent_id),
            )
            && codex_interrupt_timeout_current
            && claude_statusline_current,
        ghostex_hook_present: json_contains_ghostex_owned_hook_command(&data, command),
    }
}

/*
CDXC:AgentHooks 2026-08-27:
An install is only "current" when the config carries EXACTLY the event catalog
Ghostex ships today: every event in `all_hook_events` must hold our command, and
no Ghostex-owned hook may sit under an event we no longer register. The second
half is what sweeps names a provider renamed out from under us (Gemini's
PreToolUse → BeforeTool) — `merge_json_hook` removes every owned hook before it
re-adds the current list, so flagging the drift here is all the repair pass
needs. Without this rule an event-list expansion never reached existing users:
their config already contained the command, so it looked installed forever.
*/
fn json_hook_event_coverage_is_current(
    data: &Value,
    agent_id: &str,
    command: &str,
    format: HookFormat,
) -> bool {
    let events = all_hook_events(agent_id);
    if events.is_empty() {
        return true;
    }
    let container_key = if format == HookFormat::Antigravity {
        "ghostex"
    } else {
        "hooks"
    };
    let Some(event_groups) = data.get(container_key).and_then(Value::as_object) else {
        return false;
    };
    for event_name in &events {
        let covered = event_groups
            .get(*event_name)
            .and_then(Value::as_array)
            .is_some_and(|entries| {
                hook_entries_contain(entries, &|hook| is_hook_command(hook, command))
            });
        if !covered {
            return false;
        }
    }
    !event_groups.iter().any(|(event_name, value)| {
        !events.iter().any(|event| event == event_name)
            && value.as_array().is_some_and(|entries| {
                hook_entries_contain(entries, &|hook| {
                    is_ghostex_owned_hook_command(hook, command)
                })
            })
    })
}

/// True when a Ghostex-owned Codex Interrupt hook already carries the clamped
/// 3s timeout Codex CLI enforces; a stale 5s entry must be rewritten so the CLI
/// stops printing its clamping warning.
fn codex_interrupt_hook_timeout_is_current(data: &Value, command: &str) -> bool {
    data.get("hooks")
        .and_then(Value::as_object)
        .and_then(|hooks| hooks.get("Interrupt"))
        .and_then(Value::as_array)
        .is_some_and(|entries| {
            hook_entries_contain(entries, &|hook| {
                is_hook_command(hook, command)
                    && hook.get("timeout").and_then(Value::as_i64)
                        == Some(CODEX_INTERRUPT_HOOK_TIMEOUT_SECONDS)
            })
        })
}

/// Applies `predicate` to hook objects of every JSON hook shape Ghostex writes:
/// the flat/Kiro/Antigravity "direct entry" shape and the nested
/// `{ matcher, hooks: [...] }` group shape.
fn hook_entries_contain(entries: &[Value], predicate: &dyn Fn(&Value) -> bool) -> bool {
    entries.iter().any(|entry| match entry.get("hooks") {
        Some(Value::Array(hooks)) => hooks.iter().any(|hook| predicate(hook)),
        _ => predicate(entry),
    })
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
        // CDXC:ClaudeStatusline 2026-09-03: Claude's `statusLine` names the
        // Ghostex statusline script, which is Ghostex-owned but not a hook;
        // its currency is judged by `claude_statusline_is_current` instead.
        return object
            .iter()
            .filter(|(key, _)| key.as_str() != "statusLine")
            .any(|(_, item)| json_contains_stale_ghostex_owned_hook_command(item, command));
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
        // CDXC:ClaudeStatusline 2026-09-03: Claude's `statusLine` names the
        // Ghostex statusline script, which is Ghostex-owned but not a hook;
        // its currency is judged by `claude_statusline_is_current` instead.
        return object
            .iter()
            .filter(|(key, _)| key.as_str() != "statusLine")
            .any(|(_, item)| json_contains_ghostex_owned_hook_command(item, command));
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

pub(crate) fn write_json_file(path: &Path, data: &Value) -> Result<(), DomainStateError> {
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
        HookFormat::MarkedYaml | HookFormat::TomlMarked => {
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
                merge_json_hook(&config_path, definition, hook_paths, &command)?;
                installed_paths.push(path_string(&config_path));
                // CDXC:CodexHookTrust 2026-09-02: an explicit install is the
                // user's approval of the Ghostex hooks, so record it where Codex
                // reads it or Codex never runs them.
                if definition.agent_id == "codex"
                    && trust_ghostex_codex_hooks(
                        &config_path,
                        hook_paths,
                        &command,
                        CodexTrustWriteMode::Explicit,
                    )?
                {
                    let config_toml = super::codex_trust::codex_config_path_for_hooks(&config_path);
                    installed_paths.push(path_string(&config_toml));
                }
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
            // The stale copies handed in may sit in locations the agent no
            // longer loads from (pi's pre-2026-08 root extensions directory),
            // so refreshing them in place would repair a file the agent never
            // reads. Reinstall at the canonical loader-visible path instead,
            // and remove the Ghostex-owned copies left elsewhere.
            let source =
                build_plugin_file_source(definition.agent_id, &hook_paths.notify_hook_path);
            let Some(canonical) = provider_hook_paths(definition.agent_id, hook_paths)
                .into_iter()
                .next()
            else {
                return Ok(Vec::new());
            };
            if let Some(parent) = canonical.parent() {
                fs::create_dir_all(parent).map_err(io_error)?;
            }
            fs::write(&canonical, &source).map_err(io_error)?;
            let mut repaired_paths = vec![path_string(&canonical)];
            for config_path in config_paths {
                if config_path == canonical {
                    continue;
                }
                match fs::remove_file(&config_path) {
                    Ok(()) => repaired_paths.push(path_string(&config_path)),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(io_error(error)),
                }
            }
            Ok(repaired_paths)
        }
        HookFormat::MarkedYaml | HookFormat::TomlMarked => {
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
                merge_json_hook(&config_path, definition, hook_paths, &command)?;
                repaired_paths.push(path_string(&config_path));
                // CDXC:CodexHookTrust 2026-09-02: a repaired command changes the
                // hash Codex checks, so a slot the user already approved is
                // re-approved; a slot Codex never saw approved stays untrusted
                // until the user presses Update Hooks.
                if definition.agent_id == "codex"
                    && trust_ghostex_codex_hooks(
                        &config_path,
                        hook_paths,
                        &command,
                        CodexTrustWriteMode::RefreshExisting,
                    )?
                {
                    let config_toml = super::codex_trust::codex_config_path_for_hooks(&config_path);
                    repaired_paths.push(path_string(&config_toml));
                }
            }
            Ok(repaired_paths)
        }
    }
}

fn merge_json_hook(
    config_path: &Path,
    definition: &HookDefinition,
    hook_paths: &HookPaths,
    command: &str,
) -> Result<(), DomainStateError> {
    let current_text = read_file_text(config_path);
    ensure_json_config_is_rewritable(definition.agent_id, config_path, &current_text)?;
    let mut data = read_json_object(&current_text);
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
    // CDXC:ClaudeStatusline 2026-09-03: the same settings file carries the
    // statusLine command, registered (or re-pointed) alongside the hooks.
    if definition.agent_id == "claude" {
        register_claude_statusline(&mut data, hook_paths);
    }
    write_json_file(config_path, &data)
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
fn ensure_json_config_is_rewritable(
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
    if agent_id == "kimi" {
        lines.push(begin_marker.clone());
        lines.extend(kimi_hook_block_body(command));
        lines.push(end_marker);
    } else if agent_id == "hermes-agent" {
        let shell_command = hermes_hook_shell_command(command);
        merge_hermes_hook_block(&mut lines, &begin_marker, &end_marker, &shell_command);
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
    .map_err(io_error)?;
    if agent_id == "hermes-agent" {
        update_hermes_shell_hook_allowlist(config_path, Some(&hermes_hook_shell_command(command)))?;
    }
    Ok(())
}

/*
The agent consents to each configured shell hook per (event, command) pair: an
unapproved pair triggers a TTY prompt on every interactive launch, and is
silently skipped on non-interactive ones — which would mean ten prompts per
fresh machine, and no session-state events at all from gateway or automation
runs. Approvals live in `<config dir>/shell-hooks-allowlist.json` keyed by the
exact event name and command string, so installing the hooks and approving
them are one act of the same consent: the user already authorized Ghostex's
hook integration.
*/
const HERMES_HOOK_EVENTS: [(&str, u32); 10] = [
    ("on_session_start", 5),
    ("pre_llm_call", 5),
    ("post_llm_call", 5),
    ("pre_approval_request", 5),
    ("post_approval_response", 5),
    ("on_session_end", 5),
    ("on_session_finalize", 5),
    ("on_session_reset", 5),
    ("pre_tool_call", 120),
    ("post_tool_call", 120),
];

const HERMES_ALLOWLIST_FILE: &str = "shell-hooks-allowlist.json";

/*
The config's `hooks:` section may already belong to the user. YAML keeps only
the last duplicate of a mapping key, so appending a second top-level `hooks:`
block would silently drop either the user's hooks or ours depending on file
order. Instead the entries merge into the existing section: events the user
already configures get our list items inserted inside a marked block under
their event line, and the rest land as new event sections in one marked block
directly under `hooks:`. Marked blocks are removed by `without_marked_block`
wherever they sit, so uninstall stays a pure block deletion.
*/
fn merge_hermes_hook_block(
    lines: &mut Vec<String>,
    begin_marker: &str,
    end_marker: &str,
    shell_command: &str,
) {
    let hook_entry_lines = |item_indent: &str, timeout: u32| {
        vec![
            format!(
                "{item_indent}- command: {}",
                yaml_double_quote(shell_command)
            ),
            format!("{item_indent}  timeout: {timeout}"),
        ]
    };

    let Some(hooks_index) = hermes_hooks_line_index(lines) else {
        if lines.last().map(|line| !line.trim().is_empty()) == Some(true) {
            lines.push(String::new());
        }
        lines.push(begin_marker.to_string());
        lines.push("hooks:".to_string());
        for (event, timeout) in HERMES_HOOK_EVENTS {
            lines.push(format!("  {event}:"));
            lines.extend(hook_entry_lines("    ", timeout));
        }
        lines.push(end_marker.to_string());
        return;
    };

    // `hooks: {}` / `hooks: []` mean "no hooks", which is also what the bare
    // key means once the block entries below it exist.
    lines[hooks_index] = "hooks:".to_string();
    let child_indent = hermes_hooks_child_indent(lines, hooks_index);
    /*
    An entry of ours that is no longer inside a marked block — written by an
    older installer, or left behind when a marker was edited away — would
    otherwise be joined by the marked copy added below and run the hook twice
    per event. Ownership is decided by the command, so a hook the user wrote
    is never touched.
    */
    remove_unmarked_hermes_hook_entries(lines, hooks_index, &child_indent);
    let existing_events = hermes_direct_event_line_indexes(lines, hooks_index, &child_indent);

    let mut missing_events: Vec<(&str, u32)> = Vec::new();
    let mut matched_events: Vec<(u32, usize)> = Vec::new();
    for (event, timeout) in HERMES_HOOK_EVENTS {
        match existing_events.get(event) {
            Some(&event_index) => matched_events.push((timeout, event_index)),
            None => missing_events.push((event, timeout)),
        }
    }

    // Bottom-up so the earlier indexes stay valid across insertions.
    matched_events.sort_by(|left, right| right.1.cmp(&left.1));
    for (timeout, event_index) in matched_events {
        if let Some(colon) = lines[event_index].find(':') {
            lines[event_index].truncate(colon + 1);
        }
        let item_indent = format!("{child_indent}  ");
        let mut block = vec![format!("{item_indent}{begin_marker}")];
        block.extend(hook_entry_lines(&item_indent, timeout));
        block.push(format!("{item_indent}{end_marker}"));
        lines.splice(event_index + 1..event_index + 1, block);
    }

    if !missing_events.is_empty() {
        let mut block = vec![format!("{child_indent}{begin_marker}")];
        for (event, timeout) in missing_events {
            block.push(format!("{child_indent}{event}:"));
            block.extend(hook_entry_lines(&format!("{child_indent}  "), timeout));
        }
        block.push(format!("{child_indent}{end_marker}"));
        lines.splice(hooks_index + 1..hooks_index + 1, block);
    }
}

/// Drops every list item under `hooks:` that runs one of our hook commands,
/// leaving the user's own entries and the event keys themselves in place. An
/// item is its `- ` line plus every deeper-indented line that follows it.
fn remove_unmarked_hermes_hook_entries(
    lines: &mut Vec<String>,
    hooks_index: usize,
    child_indent: &str,
) {
    let mut index = hooks_index + 1;
    while index < lines.len() {
        let line = &lines[index];
        if !line.trim().is_empty() && !line.starts_with(child_indent) {
            break;
        }
        let indent: String = line.chars().take_while(|ch| ch.is_whitespace()).collect();
        if !line.trim_start().starts_with("- ") || indent.len() <= child_indent.len() {
            index += 1;
            continue;
        }
        let mut end = index + 1;
        while end < lines.len() {
            let next = &lines[end];
            let next_indent: String = next.chars().take_while(|ch| ch.is_whitespace()).collect();
            if next.trim().is_empty()
                || next.trim_start().starts_with("- ")
                || next_indent.len() <= indent.len()
            {
                break;
            }
            end += 1;
        }
        if text_contains_ghostex_owned_hook_command(&lines[index..end].join("\n")) {
            lines.drain(index..end);
            continue;
        }
        index = end;
    }
}

/// The top-level `hooks:` key: unindented, with nothing after the colon except
/// an inline empty value or a comment.
fn hermes_hooks_line_index(lines: &[String]) -> Option<usize> {
    lines.iter().position(|line| {
        let Some(suffix) = line.strip_prefix("hooks:") else {
            return false;
        };
        hermes_suffix_is_inline_empty(suffix)
    })
}

fn hermes_suffix_is_inline_empty(suffix: &str) -> bool {
    let uncommented = suffix.split('#').next().unwrap_or("").trim();
    uncommented.is_empty() || uncommented == "{}" || uncommented == "[]"
}

/// Indentation of the section's direct children, read from the first child
/// rather than assumed, so a config indented with four spaces keeps one
/// consistent sibling indent after the merge.
fn hermes_hooks_child_indent(lines: &[String], hooks_index: usize) -> String {
    for line in lines.iter().skip(hooks_index + 1) {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent: String = line.chars().take_while(|ch| ch.is_whitespace()).collect();
        if indent.is_empty() {
            break;
        }
        return indent;
    }
    "  ".to_string()
}

/// Direct children of `hooks:` whose value is a block (or inline empty), keyed
/// by event name. An event holding a non-empty inline value cannot take merged
/// list items, so it is deliberately absent here.
fn hermes_direct_event_line_indexes(
    lines: &[String],
    hooks_index: usize,
    child_indent: &str,
) -> std::collections::HashMap<String, usize> {
    let mut indexes = std::collections::HashMap::new();
    for (offset, line) in lines.iter().enumerate().skip(hooks_index + 1) {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if !line.starts_with(child_indent) {
            break;
        }
        let indent: String = line.chars().take_while(|ch| ch.is_whitespace()).collect();
        if indent != child_indent {
            continue;
        }
        let Some(colon) = trimmed.find(':') else {
            continue;
        };
        if hermes_suffix_is_inline_empty(&trimmed[colon + 1..]) {
            indexes.insert(trimmed[..colon].to_string(), offset);
        }
    }
    indexes
}

/// The exact command string the agent parses back out of the YAML entry; the
/// allowlist matches on it byte-for-byte.
fn hermes_hook_shell_command(command: &str) -> String {
    format!("sh -c {}", shell_quote(command))
}

/// Reconcile the approvals file with what is installed. `installed_command` is
/// the one command every event entry runs; `None` means uninstall. Foreign
/// approvals are preserved verbatim; stale Ghostex-owned approvals (an older
/// hook path, a removed event) are pruned so the file tracks exactly the
/// installed set. The read-modify-write is serialized against the agent's own
/// writers through the sibling `.lock` file, and the rewrite lands via
/// temp-file + rename so a concurrent reader never sees a torn file.
fn update_hermes_shell_hook_allowlist(
    config_path: &Path,
    installed_command: Option<&str>,
) -> Result<(), DomainStateError> {
    let Some(config_dir) = config_path.parent() else {
        return Ok(());
    };
    let allowlist_path = config_dir.join(HERMES_ALLOWLIST_FILE);
    if installed_command.is_none() && !allowlist_path.is_file() {
        return Ok(());
    }
    fs::create_dir_all(config_dir).map_err(io_error)?;

    let lock_path = config_dir.join(format!("{HERMES_ALLOWLIST_FILE}.lock"));
    let lock_file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&lock_path)
        .map_err(io_error)?;
    let _lock = HermesAllowlistLock::acquire(&lock_file)?;

    let mut root = fs::read(&allowlist_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|value| match value {
            Value::Object(map) => Some(map),
            _ => None,
        })
        .unwrap_or_default();
    let approvals = match root.get("approvals") {
        Some(Value::Array(entries)) => entries.clone(),
        _ => Vec::new(),
    };

    let installed: Vec<(&str, &str)> = installed_command
        .map(|command| {
            HERMES_HOOK_EVENTS
                .iter()
                .map(|(event, _)| (*event, command))
                .collect()
        })
        .unwrap_or_default();
    let mut next: Vec<Value> = approvals
        .into_iter()
        .filter(|entry| {
            let event = entry.get("event").and_then(Value::as_str);
            let command = entry.get("command").and_then(Value::as_str);
            let (Some(event), Some(command)) = (event, command) else {
                return true;
            };
            if installed
                .iter()
                .any(|(installed_event, installed_command)| {
                    *installed_event == event && *installed_command == command
                })
            {
                return true;
            }
            !text_contains_ghostex_owned_hook_command(command)
        })
        .collect();
    let approved_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    for (event, command) in &installed {
        let already_present = next.iter().any(|entry| {
            entry.get("event").and_then(Value::as_str) == Some(*event)
                && entry.get("command").and_then(Value::as_str) == Some(*command)
        });
        if !already_present {
            next.push(json!({
                "event": event,
                "command": command,
                "approved_at": approved_at,
            }));
        }
    }
    root.insert("approvals".to_string(), Value::Array(next));

    let serialized = serde_json::to_string_pretty(&Value::Object(root)).map_err(json_error)?;
    let temp_path = temp_path_for(&allowlist_path);
    fs::write(&temp_path, format!("{serialized}\n")).map_err(io_error)?;
    fs::rename(&temp_path, &allowlist_path).map_err(io_error)
}

/// Exclusive advisory lock on the allowlist's sibling lock file, released on
/// drop. On non-Unix targets the atomic rename alone carries the write.
struct HermesAllowlistLock<'a> {
    #[cfg_attr(not(unix), allow(dead_code))]
    file: &'a fs::File,
}

impl<'a> HermesAllowlistLock<'a> {
    fn acquire(file: &'a fs::File) -> Result<Self, DomainStateError> {
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
            if result != 0 {
                return Err(io_error(std::io::Error::last_os_error()));
            }
        }
        Ok(Self { file })
    }
}

#[cfg(unix)]
impl Drop for HermesAllowlistLock<'_> {
    fn drop(&mut self) {
        use std::os::unix::io::AsRawFd;
        unsafe { libc::flock(self.file.as_raw_fd(), libc::LOCK_UN) };
    }
}

/// The TOML body Ghostex writes between Kimi Code's marked-block markers: one
/// `[[hooks]]` array-of-tables entry per registered event, separated by a blank
/// line. No `matcher` key is written — Kimi treats `matcher` as a regex, and an
/// absent one already matches every tool.
fn kimi_hook_block_body(command: &str) -> Vec<String> {
    let mut lines = Vec::new();
    for event_name in all_hook_events("kimi") {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push("[[hooks]]".to_string());
        lines.push(format!("event = {}", toml_basic_quote(event_name)));
        lines.push(format!("command = {}", toml_basic_quote(command)));
        lines.push("timeout = 10".to_string());
    }
    lines
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

pub(crate) fn write_executable_notify_hook(
    path: &Path,
    contents: &str,
) -> Result<(), DomainStateError> {
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
