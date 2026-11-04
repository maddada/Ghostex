#[cfg(not(target_os = "windows"))]
use std::process::Command;
use std::process::Stdio;
use std::time::Duration;

use crate::*;

#[derive(Clone, Copy)]
enum GpuiTerminalExtensionPlacement {
    SplitRight,
    Tab,
}

/// Why a terminal extension did not open.
enum GpuiTerminalExtensionLaunchError {
    Unavailable(String),
    /// Required commands are not on the login shell's PATH; `homebrew` is whether `brew` is.
    Missing {
        binaries: Vec<String>,
        homebrew: bool,
    },
}

impl From<String> for GpuiTerminalExtensionLaunchError {
    fn from(message: String) -> Self {
        Self::Unavailable(message)
    }
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
            let result = background
                .spawn(async move {
                    let spec = read_terminal_extension_spec(&extension_id)?;
                    let spec = resolve_terminal_extension_spec(spec, &launch_context)?;
                    let missing = missing_terminal_extension_binaries(&spec.requires)?;
                    if !missing.is_empty() {
                        // Homebrew is offered only on a Mac, where it is the usual way to get a CLI.
                        let homebrew = cfg!(target_os = "macos")
                            && terminal_extension_binary_available("brew").unwrap_or(false);
                        return Err(GpuiTerminalExtensionLaunchError::Missing {
                            binaries: missing,
                            homebrew,
                        });
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
                Err(GpuiTerminalExtensionLaunchError::Unavailable(message)) => this
                    .dispatch_gpui_workspace_action_toast(
                        "warning",
                        "Extension unavailable",
                        &message,
                        cx,
                    ),
                Err(GpuiTerminalExtensionLaunchError::Missing { binaries, homebrew }) => {
                    this.show_missing_terminal_extension_command_toast(
                        &extension_title,
                        &binaries,
                        homebrew,
                        requested_pane_id,
                        cx,
                    );
                }
            });
        })
        .detach();
        true
    }
}

impl GhostexGpuiApp {
    /// CDXC:Extensions 2026-10-27 DECISION:
    /// User: "i enabled lazygit extension and i dont see a way to actually use it pls fix". A terminal extension whose command is missing says which command, that it is not installed on this computer, and how to get it; on a Mac with Homebrew the toast's Install with Homebrew button runs `brew install <command>` in a terminal where the extension would have opened, so the user watches it finish and opens the extension again.
    fn show_missing_terminal_extension_command_toast(
        &mut self,
        title: &str,
        binaries: &[String],
        homebrew: bool,
        requested_pane_id: WorkspacePaneId,
        cx: &mut gpui::Context<Self>,
    ) {
        self.app_toast_id_counter = self.app_toast_id_counter.wrapping_add(1);
        let id = format!("gpui-app-toast-{}", self.app_toast_id_counter);
        self.upsert_gpui_app_toast(
            GpuiAppToast {
                id,
                copy_text: None,
                level: GpuiAppToastLevel::from_raw(Some("warning")),
                title: format!("{title} needs {}", human_join(binaries)),
                description: Some(missing_terminal_extension_command_message(
                    title, binaries, homebrew,
                )),
                loading: false,
                // The button has to wait for the user.
                persistent: homebrew,
                duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                action: homebrew.then(|| GpuiAppToastAction::RunInNewTerminal {
                    label: "Install with Homebrew".to_string(),
                    title: format!("Install {}", human_join(binaries)),
                    command: homebrew_install_command(binaries),
                    pane_id: requested_pane_id.0,
                }),
                epoch: 0,
            },
            cx,
        );
    }

    /// The Install with Homebrew button: a terminal split to the right of the pane the extension
    /// was opened from, running the install.
    pub(crate) fn run_toast_command_in_new_terminal(
        &mut self,
        title: &str,
        command: &str,
        pane_id: u64,
        cx: &mut gpui::Context<Self>,
    ) {
        self.create_registered_agents_extension_terminal(
            WorkspacePaneId(pane_id),
            AgentsWorkspaceNewTerminalPlacement::SplitRight,
            title.to_string(),
            None,
            gpui_debug_command_action_initial_input(command),
            cx,
        );
    }
}

/// What a terminal extension's missing-command toast says.
fn missing_terminal_extension_command_message(
    title: &str,
    binaries: &[String],
    homebrew: bool,
) -> String {
    let commands = human_join(binaries);
    let (it, command_word) = if binaries.len() == 1 {
        ("it", "command")
    } else {
        ("them", "commands")
    };
    let how = if homebrew {
        format!("Install {it} with Homebrew below")
    } else if cfg!(target_os = "macos") {
        format!(
            "Install {it} (with Homebrew: {})",
            homebrew_install_command(binaries)
        )
    } else {
        format!("Install {it} with your package manager")
    };
    format!(
        "{title} opens the {commands} {command_word} in a terminal, and this computer does not have {it}. {how}, then open {title} again."
    )
}

fn homebrew_install_command(binaries: &[String]) -> String {
    format!(
        "brew install {}",
        binaries
            .iter()
            .map(|binary| shell_quote_if_needed(binary))
            .collect::<Vec<_>>()
            .join(" ")
    )
}

/// A command name as typed at a prompt: bare when it is plain, quoted otherwise.
fn shell_quote_if_needed(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_.+@/".contains(character))
    {
        value.to_string()
    } else {
        shell_quote(value)
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
    crate::app::helpers::gpui_background_command(program)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_command_on_a_mac_with_homebrew_points_at_the_install_button() {
        assert_eq!(
            missing_terminal_extension_command_message("Lazygit", &["lazygit".to_string()], true),
            "Lazygit opens the lazygit command in a terminal, and this computer does not have it. Install it with Homebrew below, then open Lazygit again."
        );
        assert_eq!(
            homebrew_install_command(&["lazygit".to_string()]),
            "brew install lazygit"
        );
    }

    #[test]
    fn missing_commands_without_homebrew_still_say_how_to_get_them() {
        let message = missing_terminal_extension_command_message(
            "Tools",
            &["tig".to_string(), "delta".to_string()],
            false,
        );
        assert!(message.starts_with(
            "Tools opens the tig and delta commands in a terminal, and this computer does not have them."
        ));
        if cfg!(target_os = "macos") {
            assert!(message.contains("(with Homebrew: brew install tig delta)"));
        } else {
            assert!(message.contains("with your package manager"));
        }
    }

    #[test]
    fn an_unusual_command_name_is_quoted_for_the_shell() {
        assert_eq!(
            homebrew_install_command(&["odd name".to_string()]),
            "brew install 'odd name'"
        );
    }

    #[test]
    fn the_working_directory_resolves_to_the_active_project() {
        let spec = GpuiTerminalExtensionSpec {
            command: "lazygit".to_string(),
            cwd: Some("{projectPath}".to_string()),
            placement: GpuiTerminalExtensionPlacement::SplitRight,
            requires: vec!["lazygit".to_string()],
        };
        let resolved = resolve_terminal_extension_spec(
            spec,
            &serde_json::json!({ "projectPath": "/Users/me/code/app" }),
        )
        .unwrap_or_else(|_| panic!("resolves"));
        assert_eq!(resolved.cwd.as_deref(), Some("/Users/me/code/app"));
        assert_eq!(resolved.command, "lazygit");
        let no_project = GpuiTerminalExtensionSpec {
            command: "lazygit".to_string(),
            cwd: Some("{projectPath}".to_string()),
            placement: GpuiTerminalExtensionPlacement::SplitRight,
            requires: Vec::new(),
        };
        assert!(resolve_terminal_extension_spec(no_project, &serde_json::json!({})).is_err());
    }
}
