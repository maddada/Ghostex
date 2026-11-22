use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::agent_hooks::config::{
    HookDefinition, HookFormat, HookPaths, all_hook_events, hook_format, hook_marker,
    nested_event_timeout, pi_extension_path_is_loader_visible,
};
use crate::agent_hooks::plugin_sources::{
    command_for_agent, current_plugin_marker, yaml_double_quote,
};
use crate::agent_hooks::probing::{path_string, read_file_text};
use crate::agent_hooks::resolution::{
    is_ghostex_owned_hook_command, text_contains_ghostex_owned_hook_command,
};

use super::*;

pub(crate) struct HookInspection {
    pub(crate) current_hook_installed: bool,
    pub(crate) ghostex_hook_present: bool,
}

/// Plugin files (JSON string literals) and Kimi's TOML basic strings store the hook path with each backslash doubled, so a Windows path only matches in that escaped form.
fn text_contains_hook_path(text: &str, path: &str) -> bool {
    text.contains(path) || text.contains(&path.replace('\\', "\\\\"))
}

/// CDXC:AgentHooks 2026-09-27 WHY:
/// Windows paths and commands contain backslashes and quotes. Compare decoded JSON commands and serialized JavaScript path literals so a fresh install is not reported as stale.
pub(crate) fn inspect_agent_hook_installation(
    definition: &HookDefinition,
    hook_paths: &HookPaths,
    config_paths: &[PathBuf],
) -> HookInspection {
    // A portable copy treats another Ghostex's hooks as its own current install (foreign_owner.rs).
    if crate::agent_hooks::foreign_owner::hooks_belong_to_another_ghostex(definition, config_paths)
    {
        return HookInspection {
            current_hook_installed: true,
            ghostex_hook_present: true,
        };
    }
    let command = command_for_agent(definition, &hook_paths.notify_hook_path);
    match hook_format(definition.agent_id) {
        HookFormat::Opencode => match config_paths.get(1).and_then(|path| path.parent()) {
            Some(directory) => inspect_opencode_hook(hook_paths, directory),
            None => HookInspection {
                current_hook_installed: false,
                ghostex_hook_present: false,
            },
        },
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
                        && text_contains_hook_path(
                            &text,
                            &path_string(&hook_paths.notify_hook_path),
                        );
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
        // Every Hermes profile config must carry the hooks for the install to
        // count as current; the single-file providers reduce to their one file.
        HookFormat::MarkedYaml | HookFormat::TomlMarked => {
            let marker = format!("ghostex hooks {} begin", definition.agent_id);
            let notify_hook_path = path_string(&hook_paths.notify_hook_path);
            // A Hermes block naming the right hook path can still run it the wrong
            // way (the `sh -c` wrapper Windows got before 2026-10-16), so its
            // entries must carry exactly the command this install writes.
            let hermes_entry = (definition.agent_id == "hermes-agent").then(|| {
                format!(
                    "- command: {}",
                    yaml_double_quote(&hermes_hook_shell_command(&command))
                )
            });
            let texts = config_paths
                .iter()
                .map(|path| read_file_text(path))
                .collect::<Vec<_>>();
            let current = !texts.is_empty()
                && texts.iter().all(|text| {
                    text.contains(&marker)
                        && text_contains_hook_path(text, &notify_hook_path)
                        && hermes_entry
                            .as_ref()
                            .is_none_or(|entry| text.contains(entry))
                });
            HookInspection {
                current_hook_installed: current,
                ghostex_hook_present: texts.iter().any(|text| {
                    text.contains(&marker) || text_contains_ghostex_owned_hook_command(text)
                }),
            }
        }
        HookFormat::Antigravity => {
            let text = config_paths
                .first()
                .map(|path| read_file_text(path))
                .unwrap_or_default();
            let data = read_json_object(&text);
            let current = json_contains_hook_command(&data, &command)
                && json_hook_event_coverage_is_current(
                    &data,
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
        HookFormat::RootFlatJson
        | HookFormat::FlatJson
        | HookFormat::KiroJson
        | HookFormat::NestedJson
        | HookFormat::NestedEventsJson => {
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
                .map(|path| inspect_json_hook_config(path, definition, &command))
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
    command: &str,
) -> HookInspection {
    let data = read_agent_json_object(definition.agent_id, &read_file_text(config_path));
    let stale_ghostex_hook_present = json_contains_stale_ghostex_owned_hook_command(&data, command);
    let timeout_current =
        !matches!(
            definition.agent_id,
            "codex" | "grok" | "claude" | "openclaude"
        ) || nested_hook_timeouts_are_current(&data, definition.agent_id, command);
    // The Claude and Cursor statusLine and Codex's footer item no longer make a hook stale: Read
    // live status from Claude Code and Cursor keeps them (statusline_setting.rs).
    HookInspection {
        current_hook_installed: json_contains_hook_command(&data, command)
            && !stale_ghostex_hook_present
            && json_hook_event_coverage_is_current(
                &data,
                definition.agent_id,
                command,
                hook_format(definition.agent_id),
            )
            && timeout_current
            && (definition.agent_id != "zcode"
                || data.pointer("/hooks/enabled") == Some(&json!(true))),
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
    let event_groups = if format == HookFormat::RootFlatJson {
        data.as_object()
    } else {
        if format == HookFormat::NestedEventsJson {
            data.pointer("/hooks/events").and_then(Value::as_object)
        } else {
            data.get(container_key).and_then(Value::as_object)
        }
    };
    let Some(event_groups) = event_groups else {
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

fn nested_hook_timeouts_are_current(data: &Value, agent: &str, command: &str) -> bool {
    all_hook_events(agent).iter().all(|event| {
        data.get("hooks")
            .and_then(|hooks| hooks.get(*event))
            .and_then(Value::as_array)
            .is_some_and(|entries| {
                !hook_entries_contain(entries, &|hook| {
                    is_hook_command(hook, command)
                        && hook.get("timeout").and_then(Value::as_i64)
                            != nested_event_timeout(agent, event)
                })
            })
    })
}

/// Applies `predicate` to hook objects of every JSON hook shape Ghostex writes:
/// the flat/Kiro/Antigravity "direct entry" shape and the nested
/// `{ matcher, hooks: [...] }` group shape.
fn hook_entries_contain(entries: &[Value], predicate: &dyn Fn(&Value) -> bool) -> bool {
    entries.iter().any(|entry| match entry.get("hooks") {
        Some(Value::Array(hooks)) => hooks.iter().any(predicate),
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
        // CDXC:AgentHooks 2026-09-03 WHY: Claude's `statusLine` names the
        // Ghostex statusline script, which is Ghostex-owned but not a hook;
        // Read live status from Claude Code and Cursor keeps it (statusline_setting.rs).
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
        // CDXC:AgentHooks 2026-09-03 WHY: Claude's `statusLine` names the
        // Ghostex statusline script, which is Ghostex-owned but not a hook;
        // Read live status from Claude Code and Cursor keeps it (statusline_setting.rs).
        return object
            .iter()
            .filter(|(key, _)| key.as_str() != "statusLine")
            .any(|(_, item)| json_contains_ghostex_owned_hook_command(item, command));
    }
    false
}

pub(crate) fn read_json_object(text: &str) -> Value {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    if text.trim().is_empty() {
        return json!({});
    }
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(object)) => Value::Object(object),
        _ => json!({}),
    }
}

/*
CDXC:AgentHooks 2026-09-03:
An installed hook is only useful when its CLI will RUN it, and two chat agents
carry a switch, outside the hook entries themselves, that silently turns the
Ghostex hooks off while every file-shape check still passes:

  * Antigravity: `"enabled": false` on the `ghostex` named hook in
    `~/.gemini/config/hooks.json` disables all of its handlers (agy's own
    hooks.json spec).
  * Claude / OpenClaude: `"disableAllHooks": true` in settings.json switches
    off every hook, Ghostex's included.

Both read as "update required" so Settings and the launch guard stop
reporting a hook that never fires. The Antigravity switch sits on a
Ghostex-owned object, so an explicit Install/Update lifts it; the Claude
switch is the user's global choice over ALL their hooks and is only reported,
never rewritten.
*/
pub(crate) fn antigravity_ghostex_hook_disabled(text: &str) -> bool {
    read_json_object(text)
        .get("ghostex")
        .and_then(|hook| hook.get("enabled"))
        .and_then(Value::as_bool)
        == Some(false)
}

pub(crate) fn claude_all_hooks_disabled(text: &str) -> bool {
    read_json_object(text)
        .get("disableAllHooks")
        .and_then(Value::as_bool)
        == Some(true)
}

/// The agents whose JSON config may open with `//` comment lines: Copilot CLI heads its own
/// `config.json` with two, and Devin and OpenCode read JSONC. Another agent's commented config
/// reads as `{}` (no hook) and is never rewritten (json_merge.rs `read_json_config`).
pub(crate) const COMMENTED_JSON_AGENTS: &[&str] = &["copilot", "devin", "opencode"];

/// `agent_id`'s JSON config as an object, past its `//` header when that agent allows one.
pub(crate) fn read_agent_json_object(agent_id: &str, text: &str) -> Value {
    if COMMENTED_JSON_AGENTS.contains(&agent_id) {
        read_json_object(split_comment_header(text).1)
    } else {
        read_json_object(text)
    }
}

/// The `//` comment lines that open a JSON config (Copilot CLI heads its `config.json` with two),
/// and the JSON after them. A leading UTF-8 byte order mark is not part of either.
pub(crate) fn split_comment_header(text: &str) -> (&str, &str) {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let body_start = text
        .split_inclusive('\n')
        .take_while(|line| line.trim_start().starts_with("//"))
        .map(str::len)
        .sum::<usize>();
    text.split_at(body_start)
}
