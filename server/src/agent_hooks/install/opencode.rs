//! CDXC:AgentProviders 2026-10-17 DECISION:
//! User: "i dont want any support for opencode 1 in the app or on my pc i dont care about it only support 2 pleae and focus on 2 and make the code not worry about 1 at all". Ghostex installs only the OpenCode 2 TUI plugin and never checks which OpenCode version is installed.
//!
//! CDXC:AgentHooks 2026-10-17 WHY:
//! OpenCode 2 auto-loads every `*.js` in its config folder's `plugin/` and `plugins/` as a server plugin and reads opencode.json's `plugin` list as file plugins, so the `ghostex-session.js` an older Ghostex wrote there makes OpenCode 2 warn on every start (GitHub #244). Installing the TUI plugin removes those files and entries once; nothing else of the old plugin is kept.

use std::{fs, path::Path};

use serde_json::{json, Value};

use crate::domain::DomainStateError;

use crate::agent_hooks::config::{HookPaths, OPENCODE_PLUGIN_MARKER};
use crate::agent_hooks::probing::{io_error, path_string, push_unique_path, read_file_text};
use crate::agent_hooks::resolution::{
    provider_hook_paths, text_contains_ghostex_owned_hook_command,
};

use super::*;

const PLUGIN_DIRECTORY: &str = "plugins/ghostex-v2";

fn plugin_source(hooks: &HookPaths) -> Result<String, DomainStateError> {
    let executable = std::env::current_exe().map_err(io_error)?;
    Ok(include_str!("opencode_tui.js")
        .replace("__GXSERVER__", &json!(path_string(&executable)).to_string())
        .replace(
            "__NOTIFY__",
            &json!(path_string(&hooks.notify_hook_path)).to_string(),
        ))
}

fn plugin_spec(config_dir: &Path) -> Result<String, DomainStateError> {
    url::Url::from_directory_path(config_dir.join(PLUGIN_DIRECTORY))
        .map(|url| url.to_string())
        .map_err(|_| io_error(std::io::Error::other("Invalid OpenCode plugin path")))
}

fn is_plugin_spec(value: &Value, spec: &str) -> bool {
    value.as_str() == Some(spec) || value["package"].as_str() == Some(spec)
}

fn read_cli_config(path: &Path) -> Result<Value, DomainStateError> {
    let text = read_file_text(path);
    if text.trim().is_empty() {
        Ok(json!({}))
    } else {
        crate::session_chat_opencode::read_config(&text)
    }
}

/// The plugin files and opencode.json entries an older Ghostex wrote for OpenCode 1.
fn retired_plugin_paths(config_dir: &Path) -> [std::path::PathBuf; 2] {
    [
        config_dir.join("plugin").join("ghostex-session.js"),
        config_dir.join("plugins").join("ghostex-session.js"),
    ]
}

fn is_retired_plugin_file(path: &Path) -> bool {
    let text = read_file_text(path);
    text.contains(OPENCODE_PLUGIN_MARKER) || text_contains_ghostex_owned_hook_command(&text)
}

fn is_retired_plugin_registration(value: &Value) -> bool {
    let plugin = value
        .as_str()
        .or_else(|| value.as_array().and_then(|items| items.first()?.as_str()))
        .unwrap_or_default();
    plugin == "ghostex-session" || plugin.ends_with("/ghostex-session.js")
}

fn retired_plugin_present(config_dir: &Path) -> bool {
    retired_plugin_paths(config_dir)
        .iter()
        .any(|path| is_retired_plugin_file(path))
        || read_json_object(&read_file_text(&config_dir.join("opencode.json")))
            .get("plugin")
            .and_then(Value::as_array)
            .is_some_and(|plugins| plugins.iter().any(is_retired_plugin_registration))
}

/// Removes what `retired_plugin_present` reports and returns the files it changed.
pub(crate) fn remove_retired_opencode_plugin(
    config_dir: &Path,
) -> Result<Vec<String>, DomainStateError> {
    let mut removed = Vec::new();
    let config_path = config_dir.join("opencode.json");
    let config_text = read_file_text(&config_path);
    if !config_text.trim().is_empty() {
        let mut data = read_json_object(&config_text);
        if let Some(plugins) = data.get_mut("plugin").and_then(Value::as_array_mut) {
            let before = plugins.len();
            plugins.retain(|plugin| !is_retired_plugin_registration(plugin));
            if plugins.len() != before {
                write_json_file(&config_path, &data)?;
                push_unique_path(&mut removed, path_string(&config_path));
            }
        }
    }
    for path in retired_plugin_paths(config_dir) {
        if is_retired_plugin_file(&path) && remove_file_if_exists(&path)? {
            push_unique_path(&mut removed, path_string(&path));
        }
    }
    Ok(removed)
}

/// `provider_hook_paths("opencode")` is `[plugins/ghostex-v2/tui.js, cli.json]`.
fn opencode_config_dir(hook_paths: &HookPaths) -> Option<std::path::PathBuf> {
    provider_hook_paths("opencode", hook_paths)
        .get(1)
        .and_then(|path| path.parent())
        .map(Path::to_path_buf)
}

/// The plugin file that carries Ghostex's gxserver and notify paths.
pub(crate) fn opencode_plugin_file(config_dir: &Path) -> std::path::PathBuf {
    config_dir.join(PLUGIN_DIRECTORY).join("tui.js")
}

pub(crate) fn inspect_opencode_hook(hooks: &HookPaths, config_dir: &Path) -> HookInspection {
    let text = read_file_text(&opencode_plugin_file(config_dir));
    let config = read_file_text(&config_dir.join("cli.json"));
    let retired = retired_plugin_present(config_dir);
    HookInspection {
        ghostex_hook_present: text.contains(OPENCODE_PLUGIN_MARKER)
            || config.contains(PLUGIN_DIRECTORY)
            || retired,
        current_hook_installed: plugin_source(hooks).is_ok_and(|expected| expected == text)
            && config.contains(PLUGIN_DIRECTORY)
            && !retired
            && ghostex_theme_current(config_dir),
    }
}

const GHOSTEX_THEME: &str = include_str!("opencode_ghostex_theme.json");
const GHOSTEX_THEME_NAME: &str = "ghostex";

/// OpenCode 2 lists every `themes/*.json` in its config folder by file name.
pub(crate) fn ghostex_theme_file(config_dir: &Path) -> std::path::PathBuf {
    config_dir
        .join("themes")
        .join(format!("{GHOSTEX_THEME_NAME}.json"))
}

/// Whether cli.json names a theme. OpenCode 2 saves a theme picked with Ctrl+P as `theme.name`
/// there, and starts on its own `opencode` theme when none is named.
fn theme_chosen(config: &Value) -> bool {
    let name = match config.get("theme") {
        Some(Value::Object(theme)) => theme.get("name").and_then(Value::as_str),
        Some(Value::String(name)) => Some(name.as_str()),
        _ => None,
    };
    name.is_some_and(|name| !name.trim().is_empty())
}

/// An unreadable cli.json counts as current: installing would refuse it the same way.
fn ghostex_theme_current(config_dir: &Path) -> bool {
    read_file_text(&ghostex_theme_file(config_dir)) == GHOSTEX_THEME
        && read_cli_config(&config_dir.join("cli.json"))
            .map_or(true, |config| theme_chosen(&config))
}

/// CDXC:AgentProviders 2026-11-02 DECISION:
/// User: "is there any actual way to make the bg of opencode transparent? (like claude code and codex are)", and chose "Yes, a Ghostex theme with transparent backgrounds". Ghostex installs a `ghostex` OpenCode theme (the screen, panels and diff context are `transparent` so the terminal's own background and glass show; the colours are the GitHub Dark and GitHub Light palettes Ghostex's terminal uses, picked by OpenCode from the terminal's light or dark mode) and names it in cli.json only when no theme is named there, so a theme the user picked with Ctrl+P or wrote by hand is never replaced and the rest of cli.json is kept.
///
/// CDXC:AgentProviders 2026-11-02 WHY:
/// The theme is OpenCode 2's own document format rather than the older flat one: an older-format theme with every background transparent leaves dialogs (Ctrl+P, model picker) without a fill, so the text under them reads through, and only the newer format can give `@dialog` a solid background. OpenCode blends the session tab strip into an opaque colour taken from the transparent colour's own RGB, so selected tabs get a solid fill and light mode uses a transparent white (`#ffffff00`): the strip then matches a black or white terminal instead of drawing a black band in light mode. A per-launch override (`OPENCODE_CLI_CONFIG_CONTENT`) was not used: it is merged over cli.json on every read, so it would beat the user's Ctrl+P pick.
pub(crate) fn install_ghostex_opencode_theme(config_dir: &Path) -> Result<(), DomainStateError> {
    let theme_path = ghostex_theme_file(config_dir);
    if read_file_text(&theme_path) != GHOSTEX_THEME {
        crate::session_chat_opencode::write_config_file(&theme_path, GHOSTEX_THEME)?;
    }
    let config_path = config_dir.join("cli.json");
    let config = read_cli_config(&config_path)?;
    if theme_chosen(&config) {
        return Ok(());
    }
    let mut theme = config
        .get("theme")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    theme.insert("name".into(), json!(GHOSTEX_THEME_NAME));
    crate::session_chat_opencode::patch_config(&config_path, "theme", &Value::Object(theme))
}

pub(super) fn uninstall_opencode_hook(
    hook_paths: &HookPaths,
) -> Result<Vec<String>, DomainStateError> {
    let Some(config_dir) = opencode_config_dir(hook_paths) else {
        return Ok(Vec::new());
    };
    let config_path = config_dir.join("cli.json");
    let spec = plugin_spec(&config_dir)?;
    let mut removed = Vec::new();
    if config_path.is_file() {
        let mut config = read_cli_config(&config_path)?;
        if let Some(plugins) = config.get_mut("plugins").and_then(Value::as_array_mut) {
            let before = plugins.len();
            plugins.retain(|plugin| !is_plugin_spec(plugin, &spec));
            if before != plugins.len() {
                crate::session_chat_opencode::patch_config(
                    &config_path,
                    "plugins",
                    &config["plugins"],
                )?;
                removed.push(path_string(&config_path));
            }
        }
    }
    let plugin = config_dir.join(PLUGIN_DIRECTORY).join("tui.js");
    if read_file_text(&plugin).contains(OPENCODE_PLUGIN_MARKER) {
        fs::remove_file(&plugin).map_err(io_error)?;
        removed.push(path_string(&plugin));
        // Its folder was Ghostex's too; anything else put there keeps it.
        if let Some(folder) = plugin.parent() {
            let _ = fs::remove_dir(folder);
        }
    }
    for path in remove_retired_opencode_plugin(&config_dir)? {
        push_unique_path(&mut removed, path);
    }
    Ok(removed)
}

pub(super) fn install_opencode_hook(
    hook_paths: &HookPaths,
) -> Result<Vec<String>, DomainStateError> {
    let Some(config_dir) = opencode_config_dir(hook_paths) else {
        return Ok(Vec::new());
    };
    let plugin_dir = config_dir.join(PLUGIN_DIRECTORY);
    let plugin_path = plugin_dir.join("tui.js");
    let config_path = config_dir.join("cli.json");
    let mut config = read_cli_config(&config_path)?;
    let object = config.as_object_mut().ok_or_else(|| DomainStateError {
        code: "invalidParams",
        message: "OpenCode cli.json must contain an object.".into(),
    })?;
    let plugins = object
        .entry("plugins")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or_else(|| DomainStateError {
            code: "invalidParams",
            message: "OpenCode's plugins setting must be a list.".into(),
        })?;
    let spec = plugin_spec(&config_dir)?;
    if !plugins.iter().any(|plugin| is_plugin_spec(plugin, &spec)) {
        plugins.push(json!(spec));
    }
    fs::create_dir_all(&plugin_dir).map_err(io_error)?;
    fs::write(&plugin_path, plugin_source(hook_paths)?).map_err(io_error)?;
    crate::session_chat_opencode::patch_config(&config_path, "plugins", &config["plugins"])?;
    install_ghostex_opencode_theme(&config_dir)?;
    remove_retired_opencode_plugin(&config_dir)?;
    Ok(vec![path_string(&plugin_path)])
}
