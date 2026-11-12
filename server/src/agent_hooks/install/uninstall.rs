use std::{
    fs,
    path::{Path, PathBuf},
};

use serde_json::Value;

use crate::domain::DomainStateError;

use crate::agent_hooks::codex_trust::{
    ghostex_codex_hook_trust_entries, remove_codex_hook_trust_entries,
};
use crate::agent_hooks::config::{hook_format, hook_marker, HookDefinition, HookFormat, HookPaths};
use crate::agent_hooks::plugin_sources::command_for_agent;
use crate::agent_hooks::probing::{io_error, path_string, push_unique_path, read_file_text};
use crate::agent_hooks::resolution::{
    is_ghostex_owned_hook_command, provider_hook_paths, text_contains_ghostex_owned_hook_command,
};
use crate::agent_hooks::statusline::unregister_statusline;

use super::*;

pub(crate) fn uninstall_agent_hook(
    definition: &HookDefinition,
    hook_paths: &HookPaths,
) -> Result<Vec<String>, DomainStateError> {
    uninstall_agent_hook_with(definition, hook_paths, false)
}

/// Removes only the agent's hook entries: Claude's `statusLine` stays (hook_free_cleanup.rs).
pub(crate) fn uninstall_agent_hook_entries(
    definition: &HookDefinition,
    hook_paths: &HookPaths,
) -> Result<Vec<String>, DomainStateError> {
    uninstall_agent_hook_with(definition, hook_paths, true)
}

fn uninstall_agent_hook_with(
    definition: &HookDefinition,
    hook_paths: &HookPaths,
    keep_statusline: bool,
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
        | HookFormat::RootFlatJson
        | HookFormat::FlatJson
        | HookFormat::KiroJson
        | HookFormat::NestedJson
        | HookFormat::NestedEventsJson => {
            let mut removed_paths = Vec::new();
            for config_path in config_paths {
                // CDXC:AgentHooks 2026-09-02: state keys are positional, so
                // they must be read before the hooks leave the file.
                let codex_trust_keys = if definition.agent_id == "codex" {
                    {
                        ghostex_codex_hook_trust_entries(&config_path, hook_paths, &command)
                            .into_iter()
                            .map(|entry| entry.key)
                            .collect::<Vec<_>>()
                    }
                } else {
                    Default::default()
                };
                // CDXC:AgentHooks 2026-09-24 WHY: Cursor's statusLine lives in
                // its own file and leaves with the hooks, restoring the user's.
                if definition.agent_id == "cursor" {
                    if let Some(path) =
                        super::cursor_statusline::remove_cursor_statusline(&config_path)?
                    {
                        removed_paths.push(path_string(&path));
                    }
                }
                if remove_json_hook_with(&config_path, definition, &command, keep_statusline)? {
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
    let begin_marker = format!("# ghostex hooks {} begin", definition.agent_id);
    let end_marker = format!("# ghostex hooks {} end", definition.agent_id);
    let mut removed_paths = Vec::new();
    for config_path in config_paths {
        // Withdraw the recorded approvals together with the hook entries they
        // covered, even when the marked block itself is already gone.
        if definition.agent_id == "hermes-agent" {
            update_hermes_shell_hook_allowlist(&config_path, None)?;
        }
        // A config that isn't there has no block to remove; writing the "\n" the removal leaves
        // failed in a folder that doesn't exist either, which stopped Uninstall all on a computer
        // without Rovo Dev, Hermes or Kimi.
        if !config_path.is_file() {
            continue;
        }
        let current_text = read_file_text(&config_path);
        let normalized_text = current_text.replace("\r\n", "\n");
        let lines = normalized_text
            .split('\n')
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let next_lines = without_marked_block(&lines, &begin_marker, &end_marker);
        let next_text = format!("{}\n", next_lines.join("\n").trim_end_matches('\n'));
        if current_text == next_text {
            continue;
        }
        fs::write(&config_path, next_text).map_err(io_error)?;
        removed_paths.push(path_string(&config_path));
    }
    Ok(removed_paths)
}

#[cfg(test)]
pub(crate) fn remove_json_hook(
    config_path: &Path,
    definition: &HookDefinition,
    command: &str,
) -> Result<bool, DomainStateError> {
    remove_json_hook_with(config_path, definition, command, false)
}

fn remove_json_hook_with(
    config_path: &Path,
    definition: &HookDefinition,
    command: &str,
    keep_statusline: bool,
) -> Result<bool, DomainStateError> {
    let current_text = read_file_text(config_path);
    if current_text.trim().is_empty() {
        return Ok(false);
    }
    ensure_json_config_is_rewritable(definition.agent_id, config_path, &current_text)?;
    let mut data = read_json_object(&current_text);
    let mut changed = remove_owned_json_hooks(&mut data, hook_format(definition.agent_id), command);
    // CDXC:AgentHooks 2026-09-03 WHY: the statusLine Ghostex registered (or
    // wrapped) leaves with the hooks, restoring the user's own command.
    if definition.agent_id == "claude" && !keep_statusline {
        changed |= unregister_statusline(&mut data);
    }
    if !changed {
        return Ok(false);
    }
    write_json_file(config_path, &data)?;
    Ok(true)
}

pub(super) fn remove_owned_json_hooks(data: &mut Value, format: HookFormat, command: &str) -> bool {
    let event_groups = if format == HookFormat::Antigravity {
        data.get_mut("ghostex").and_then(Value::as_object_mut)
    } else if format == HookFormat::NestedEventsJson {
        data.get_mut("hooks")
            .and_then(|hooks| hooks.get_mut("events"))
            .and_then(Value::as_object_mut)
    } else if format == HookFormat::RootFlatJson {
        data.as_object_mut()
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
            HookFormat::RootFlatJson | HookFormat::FlatJson | HookFormat::KiroJson => entries
                .iter()
                .filter(|entry| !is_ghostex_owned_hook_command(entry, command))
                .cloned()
                .collect::<Vec<_>>(),
            HookFormat::NestedJson | HookFormat::NestedEventsJson => {
                remove_nested_hook_groups(entries, command)
            }
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

pub(super) fn without_marked_block(
    lines: &[String],
    begin_marker: &str,
    end_marker: &str,
) -> Vec<String> {
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
