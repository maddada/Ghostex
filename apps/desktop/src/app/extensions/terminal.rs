use std::process::{Command, Stdio};
use std::time::Duration;

use crate::*;

#[derive(Clone, Copy)]
enum GpuiTerminalExtensionPlacement {
    SplitRight,
    Tab,
}

struct GpuiTerminalExtensionSpec {
    command: String,
    cwd: Option<String>,
    placement: GpuiTerminalExtensionPlacement,
    requires: Vec<String>,
}

impl GhostexGpuiApp {
    pub(crate) fn launch_terminal_pane_extension(
        &mut self,
        id: ExtensionId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(extension) = self
            .extensions_snapshot
            .installed
            .get(id.as_str())
            .filter(|extension| extension.enabled && extension.terminal_pane)
            .cloned()
        else {
            return false;
        };
        if !extension
            .declared_permissions
            .contains(&GpuiExtensionPermission::Exec)
            || !extension
                .granted_permissions
                .contains(&GpuiExtensionPermission::Exec)
        {
            self.dispatch_gpui_workspace_action_toast(
                "warning",
                "Extension permission required",
                &format!(
                    "Grant command execution permission to {} before launching it.",
                    extension.title
                ),
                cx,
            );
            return true;
        }
        let Some(project_id) = self.gpui_app_modal_active_project_id() else {
            self.dispatch_gpui_workspace_action_toast(
                "warning",
                "Extension unavailable",
                "Select a project before launching a terminal extension.",
                cx,
            );
            return true;
        };
        if gpui_remote_project_reference_from_project_id(&project_id).is_some() {
            self.dispatch_gpui_workspace_action_toast(
                "warning",
                "Extension unavailable",
                "Terminal extensions currently run only for local projects.",
                cx,
            );
            return true;
        }

        let launch_context = self.extension_launch_context_value();
        let requested_pane_id = match self.shell_focus {
            ShellFocusTarget::AgentsPane(pane_id) => pane_id,
            _ => self.agents_workspace.focused_pane,
        };
        let extension_id = id.as_str().to_string();
        let extension_title = extension.title;
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let title_for_probe = extension_title.clone();
            let result = background
                .spawn(async move {
                    let spec = read_terminal_extension_spec(&extension_id)?;
                    let spec = resolve_terminal_extension_spec(spec, &launch_context)?;
                    let missing = missing_terminal_extension_binaries(&spec.requires)?;
                    if !missing.is_empty() {
                        return Err(format!(
                            "Install {} to use {title_for_probe}.",
                            human_join(&missing)
                        ));
                    }
                    Ok(spec)
                })
                .await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(spec) => {
                    let placement = match spec.placement {
                        GpuiTerminalExtensionPlacement::SplitRight => {
                            AgentsWorkspaceNewTerminalPlacement::SplitRight
                        }
                        GpuiTerminalExtensionPlacement::Tab => {
                            AgentsWorkspaceNewTerminalPlacement::Tab
                        }
                    };
                    this.create_registered_agents_extension_terminal(
                        requested_pane_id,
                        placement,
                        extension_title,
                        spec.cwd,
                        gpui_debug_command_action_initial_input(&spec.command),
                        cx,
                    );
                }
                Err(message) => this.dispatch_gpui_workspace_action_toast(
                    "warning",
                    "Extension unavailable",
                    &message,
                    cx,
                ),
            });
        })
        .detach();
        true
    }
}

fn read_terminal_extension_spec(id: &str) -> Result<GpuiTerminalExtensionSpec, String> {
    let result = gpui_gxserver_rpc_result(
        "/api/listExtensions",
        &serde_json::json!({}),
        Duration::from_secs(5),
    )?;
    let extension = result
        .get("extensions")
        .and_then(serde_json::Value::as_array)
        .and_then(|extensions| {
            extensions.iter().find(|extension| {
                extension.get("id").and_then(serde_json::Value::as_str) == Some(id)
            })
        })
        .ok_or_else(|| "The terminal extension is no longer installed.".to_string())?;
    let manifest = extension
        .get("manifest")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "The terminal extension manifest is invalid.".to_string())?;
    let state = extension
        .get("state")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "The terminal extension state is invalid.".to_string())?;
    if state.get("enabled").and_then(serde_json::Value::as_bool) != Some(true)
        || manifest.get("kind").and_then(serde_json::Value::as_str) != Some("terminal-pane")
    {
        return Err("The terminal extension is disabled or unavailable.".to_string());
    }
    let terminal = manifest
        .get("terminal")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "The terminal extension command is missing.".to_string())?;
    let command = required_terminal_text(terminal.get("command"), "command")?;
    let cwd = terminal
        .get("cwd")
        .map(|value| required_terminal_text(Some(value), "working directory"))
        .transpose()?;
    let requires = terminal
        .get("requires")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .map(|value| required_terminal_text(Some(value), "required binary"))
        .collect::<Result<Vec<_>, _>>()?;
    let placement = match state
        .get("terminalPlacement")
        .and_then(serde_json::Value::as_str)
    {
        Some("splitRight") => GpuiTerminalExtensionPlacement::SplitRight,
        Some("tab") => GpuiTerminalExtensionPlacement::Tab,
        _ => return Err("The terminal extension placement is invalid.".to_string()),
    };
    Ok(GpuiTerminalExtensionSpec {
        command,
        cwd,
        placement,
        requires,
    })
}

fn required_terminal_text(
    value: Option<&serde_json::Value>,
    field: &str,
) -> Result<String, String> {
    value
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("The terminal extension {field} is invalid."))
}

fn resolve_terminal_extension_spec(
    mut spec: GpuiTerminalExtensionSpec,
    context: &serde_json::Value,
) -> Result<GpuiTerminalExtensionSpec, String> {
    spec.command = resolve_terminal_extension_placeholders(&spec.command, context, false)?;
    spec.cwd = spec
        .cwd
        .map(|cwd| resolve_terminal_extension_placeholders(&cwd, context, true))
        .transpose()?;
    Ok(spec)
}

fn resolve_terminal_extension_placeholders(
    template: &str,
    context: &serde_json::Value,
    is_cwd: bool,
) -> Result<String, String> {
    let project_path = context_text(context, "projectPath");
    if template.contains("{projectPath}") && project_path.is_empty() {
        return Err("The active project does not have a local path.".to_string());
    }
    let worktree = if context
        .get("worktree")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        "1"
    } else {
        "0"
    };
    let resolved = template
        .replace("{projectPath}", project_path)
        .replace("{projectName}", context_text(context, "projectName"))
        .replace("{sessionId}", context_text(context, "sessionId"))
        .replace("{worktree}", worktree)
        .replace("{worktreeBranch}", context_text(context, "worktreeBranch"));
    if resolved.trim().is_empty() {
        return Err(if is_cwd {
            "The terminal extension working directory resolved to an empty path.".to_string()
        } else {
            "The terminal extension command resolved to an empty value.".to_string()
        });
    }
    Ok(resolved)
}

fn context_text<'a>(context: &'a serde_json::Value, key: &str) -> &'a str {
    context
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
}

fn missing_terminal_extension_binaries(requires: &[String]) -> Result<Vec<String>, String> {
    let mut missing = Vec::new();
    for binary in requires {
        if !terminal_extension_binary_available(binary)? {
            missing.push(binary.clone());
        }
    }
    Ok(missing)
}

#[cfg(not(target_os = "windows"))]
fn terminal_extension_binary_available(binary: &str) -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    let shell = "/bin/zsh";
    #[cfg(not(target_os = "macos"))]
    let shell = "/bin/sh";
    Command::new(shell)
        .arg("-lc")
        .arg(format!(
            "command -v {} >/dev/null 2>&1",
            shell_quote(binary)
        ))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .map_err(|_| {
            "Ghostex could not check the terminal extension's required binaries.".to_string()
        })
}

#[cfg(target_os = "windows")]
fn terminal_extension_binary_available(binary: &str) -> Result<bool, String> {
    let (program, args) = crate::windows_terminal_backend::terminal_invocation(
        Some(format!(
            "command -v {} >/dev/null 2>&1",
            shell_quote(binary)
        )),
        None,
    );
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .map_err(|_| {
            "Ghostex could not check the terminal extension's required binaries.".to_string()
        })
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn human_join(values: &[String]) -> String {
    match values {
        [] => String::new(),
        [value] => value.clone(),
        [left, right] => format!("{left} and {right}"),
        _ => format!(
            "{}, and {}",
            values[..values.len() - 1].join(", "),
            values.last().expect("non-empty list")
        ),
    }
}
