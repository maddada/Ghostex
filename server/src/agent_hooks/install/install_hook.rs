use std::{
    fs,
    path::{Path, PathBuf},
};

use serde_json::{json, Value};

use crate::domain::DomainStateError;

use crate::agent_hooks::codex_status_line::ensure_codex_status_line_names_model;
use crate::agent_hooks::codex_trust::{follow_codex_hook_trust, CodexTrustWriteMode};
#[cfg(test)]
use crate::agent_hooks::config::all_hook_events;
use crate::agent_hooks::config::{hook_format, HookDefinition, HookFormat, HookPaths};
use crate::agent_hooks::plugin_sources::{build_plugin_file_source, command_for_agent};
use crate::agent_hooks::probing::{
    io_error, json_error, path_string, push_unique_path, read_file_text,
};
use crate::agent_hooks::resolution::provider_hook_paths;

use super::*;

fn enable_antigravity_ghostex_hook(
    config_path: &Path,
    definition: &HookDefinition,
) -> Result<bool, DomainStateError> {
    let current_text = read_file_text(config_path);
    if !antigravity_ghostex_hook_disabled(&current_text) {
        return Ok(false);
    }
    ensure_json_config_is_rewritable(definition.agent_id, config_path, &current_text)?;
    let mut data = read_json_object(&current_text);
    let Some(hook) = data.get_mut("ghostex").and_then(Value::as_object_mut) else {
        return Ok(false);
    };
    hook.remove("enabled");
    write_json_file(config_path, &data)?;
    Ok(true)
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

pub(super) fn remove_file_if_exists(path: &Path) -> Result<bool, DomainStateError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(io_error(error)),
    }
}

/// A Codex `hooks.json` as it was before Ghostex edits it: trust keys are positions, so the
/// trust pass after the edit needs both versions (codex_trust.rs `follow_codex_hook_trust`).
pub(super) fn codex_hooks_before_edit(definition: &HookDefinition, config_path: &Path) -> Value {
    if definition.agent_id == "codex" {
        read_json_object(&read_file_text(config_path))
    } else {
        Value::Null
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
            install_marked_yaml_hooks(&config_paths, definition.agent_id, &command)
        }
        HookFormat::Antigravity
        | HookFormat::RootFlatJson
        | HookFormat::FlatJson
        | HookFormat::KiroJson
        | HookFormat::NestedJson
        | HookFormat::NestedEventsJson => {
            let mut installed_paths = Vec::new();
            for config_path in config_paths {
                let codex_before = codex_hooks_before_edit(definition, &config_path);
                merge_json_hook(&config_path, definition, hook_paths, &command)?;
                installed_paths.push(path_string(&config_path));
                if definition.agent_id == "zcode" {
                    let mut data = read_json_object(&read_file_text(&config_path));
                    ensure_object_property(ensure_json_object(&mut data), "hooks")
                        .insert("enabled".into(), json!(true));
                    write_json_file(&config_path, &data)?;
                }
                // CDXC:AgentHooks 2026-09-03: the same explicit click
                // lifts an `"enabled": false` the user put on the Ghostex
                // named hook in Antigravity; startup repair never does.
                if definition.agent_id == "antigravity" {
                    enable_antigravity_ghostex_hook(&config_path, definition)?;
                }
                // CDXC:AgentHooks 2026-09-02: an explicit install is the
                // user's approval of the Ghostex hooks, so record it where Codex
                // reads it or Codex never runs them.
                if definition.agent_id == "codex"
                    && follow_codex_hook_trust(
                        &config_path,
                        hook_paths,
                        &command,
                        &codex_before,
                        Some(CodexTrustWriteMode::Explicit),
                    )?
                {
                    let config_toml = super::codex_trust::codex_config_path_for_hooks(&config_path);
                    installed_paths.push(path_string(&config_toml));
                }
                // CDXC:AgentHooks 2026-09-03 WHY: the footer must name
                // the model for the chat pills to have a pre-turn source.
                if definition.agent_id == "codex" {
                    let config_toml = super::codex_trust::codex_config_path_for_hooks(&config_path);
                    if ensure_codex_status_line_names_model(&config_toml)? {
                        push_unique_path(&mut installed_paths, path_string(&config_toml));
                    }
                }
                if definition.agent_id == "cursor" {
                    if let Some(path) = super::cursor_statusline::ensure_cursor_statusline(
                        &config_path,
                        hook_paths,
                    )? {
                        push_unique_path(&mut installed_paths, path_string(&path));
                    }
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
        HookFormat::MarkedYaml | HookFormat::TomlMarked => install_marked_yaml_hooks(
            &config_paths,
            definition.agent_id,
            &command_for_agent(definition, &hook_paths.notify_hook_path),
        ),
        HookFormat::Antigravity
        | HookFormat::RootFlatJson
        | HookFormat::FlatJson
        | HookFormat::KiroJson
        | HookFormat::NestedJson
        | HookFormat::NestedEventsJson => {
            let command = command_for_agent(definition, &hook_paths.notify_hook_path);
            let mut repaired_paths = Vec::new();
            for config_path in config_paths {
                let codex_before = codex_hooks_before_edit(definition, &config_path);
                merge_json_hook(&config_path, definition, hook_paths, &command)?;
                repaired_paths.push(path_string(&config_path));
                // CDXC:AgentHooks 2026-09-02: a repaired command changes the
                // hash Codex checks, so a slot the user already approved is
                // re-approved; a slot Codex never saw approved stays untrusted
                // until the user presses Update Hooks.
                if definition.agent_id == "codex"
                    && follow_codex_hook_trust(
                        &config_path,
                        hook_paths,
                        &command,
                        &codex_before,
                        Some(CodexTrustWriteMode::RefreshExisting),
                    )?
                {
                    let config_toml = super::codex_trust::codex_config_path_for_hooks(&config_path);
                    repaired_paths.push(path_string(&config_toml));
                }
                if definition.agent_id == "codex" {
                    let config_toml = super::codex_trust::codex_config_path_for_hooks(&config_path);
                    if ensure_codex_status_line_names_model(&config_toml)? {
                        push_unique_path(&mut repaired_paths, path_string(&config_toml));
                    }
                }
                if definition.agent_id == "cursor" {
                    if let Some(path) = super::cursor_statusline::ensure_cursor_statusline(
                        &config_path,
                        hook_paths,
                    )? {
                        push_unique_path(&mut repaired_paths, path_string(&path));
                    }
                }
            }
            Ok(repaired_paths)
        }
    }
}

#[cfg(test)]
mod zcode_tests {
    use super::*;

    #[test]
    fn zcode_hooks_preserve_user_config_and_respect_disabled_hooks_during_repair() {
        let temp = tempfile::tempdir().unwrap();
        let paths = HookPaths::new(temp.path().to_path_buf());
        let definition = HookDefinition {
            agent_id: "zcode",
            cli_command: "zcode",
        };
        let path = provider_hook_paths("zcode", &paths).remove(0);
        let user_hook = json!({"hooks":[{"type":"command","command":"my-hook","timeout":3}]});
        let original = json!({"model":{"main":"zai/glm-5.2"},"provider":{"zai":{"options":{"apiKey":"test-only"}}},"hooks":{"enabled":false,"events":{"Stop":[user_hook.clone()]}}});
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        write_json_file(&path, &original).unwrap();
        let command = command_for_agent(&definition, &paths.notify_hook_path);
        merge_json_hook(&path, &definition, &paths, &command).unwrap();
        let repaired = read_json_object(&read_file_text(&path));
        assert_eq!(repaired.pointer("/hooks/enabled"), Some(&json!(false)));
        assert_eq!(repaired.get("provider"), original.get("provider"));
        install_agent_hook(&definition, &paths).unwrap();
        install_agent_hook(&definition, &paths).unwrap();
        let installed = read_json_object(&read_file_text(&path));
        assert_eq!(installed.pointer("/hooks/enabled"), Some(&json!(true)));
        for event in all_hook_events("zcode") {
            let groups = installed["hooks"]["events"][event].as_array().unwrap();
            assert_eq!(groups.len(), if event == "Stop" { 2 } else { 1 });
        }
        uninstall_agent_hook(&definition, &paths).unwrap();
        let uninstalled = read_json_object(&read_file_text(&path));
        assert_eq!(uninstalled.get("provider"), original.get("provider"));
        assert_eq!(
            uninstalled.pointer("/hooks/events/Stop"),
            Some(&json!([user_hook]))
        );
        assert!(!json_contains_hook_command(&uninstalled, &command));
    }
}
