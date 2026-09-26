use super::{
    config::HookPaths,
    probing::{io_error, path_string, read_file_text},
};
use crate::domain::DomainStateError;
use serde_json::{Value, json};
use std::{fs, path::Path};

pub(crate) fn installed(hooks: &HookPaths) -> bool {
    let Some(executable) = super::probing::resolve_cli_command("opencode", &hooks.home_dir) else {
        return false;
    };
    let mut command = std::process::Command::new(executable);
    command.arg("--version");
    super::probing::run_command_stdout_with_timeout(command, std::time::Duration::from_secs(3))
        .is_some_and(|version| {
            version
                .trim()
                .trim_start_matches("opencode ")
                .trim_start_matches('v')
                .starts_with("2.")
        })
}

fn source(hooks: &HookPaths) -> Result<String, DomainStateError> {
    let executable = std::env::current_exe().map_err(io_error)?;
    Ok(include_str!("opencode_v2_tui.js")
        .replace("__GXSERVER__", &json!(path_string(&executable)).to_string())
        .replace(
            "__NOTIFY__",
            &json!(path_string(&hooks.notify_hook_path)).to_string(),
        ))
}

pub(crate) fn inspect(hooks: &HookPaths, directory: &Path) -> super::install::HookInspection {
    let plugin = directory.join("plugins/ghostex-v2/tui.js");
    let text = read_file_text(&plugin);
    let config = read_file_text(&directory.join("cli.json"));
    super::install::HookInspection {
        ghostex_hook_present: text.contains("ghostex-opencode-session-plugin-marker")
            || config.contains("plugins/ghostex-v2"),
        current_hook_installed: source(hooks).is_ok_and(|expected| expected == text)
            && config.contains("plugins/ghostex-v2"),
    }
}

pub(crate) fn uninstall(directory: &Path) -> Result<Vec<String>, DomainStateError> {
    let config_path = directory.join("cli.json");
    let plugin_dir = directory.join("plugins/ghostex-v2");
    let spec = url::Url::from_directory_path(&plugin_dir)
        .map_err(|_| io_error(std::io::Error::other("Invalid OpenCode plugin path")))?
        .to_string();
    let mut removed = Vec::new();
    if config_path.is_file() {
        let mut config: Value =
            crate::session_chat_opencode::read_config(&read_file_text(&config_path))?;
        if let Some(plugins) = config.get_mut("plugins").and_then(Value::as_array_mut) {
            let before = plugins.len();
            plugins.retain(|p| p.as_str() != Some(&spec) && p["package"].as_str() != Some(&spec));
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
    let plugin = plugin_dir.join("tui.js");
    if read_file_text(&plugin).contains("ghostex-opencode-session-plugin-marker") {
        fs::remove_file(&plugin).map_err(io_error)?;
        removed.push(path_string(&plugin));
    }
    Ok(removed)
}

pub(crate) fn install(
    hooks: &HookPaths,
    config_dir: &Path,
) -> Result<Vec<String>, DomainStateError> {
    let plugin_dir = config_dir.join("plugins/ghostex-v2");
    let source = source(hooks)?;
    let plugin_path = plugin_dir.join("tui.js");
    let config_path = config_dir.join("cli.json");
    let text = read_file_text(&config_path);
    let mut config: Value = if text.trim().is_empty() {
        json!({})
    } else {
        crate::session_chat_opencode::read_config(&text)?
    };
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
    let spec = url::Url::from_directory_path(&plugin_dir)
        .map_err(|_| io_error(std::io::Error::other("Invalid OpenCode plugin path")))?
        .to_string();
    if !plugins
        .iter()
        .any(|p| p.as_str() == Some(&spec) || p["package"].as_str() == Some(&spec))
    {
        plugins.push(json!(spec));
    }
    fs::create_dir_all(&plugin_dir).map_err(io_error)?;
    fs::write(&plugin_path, source).map_err(io_error)?;
    crate::session_chat_opencode::patch_config(&config_path, "plugins", &config["plugins"])?;
    Ok(vec![path_string(&plugin_path)])
}
