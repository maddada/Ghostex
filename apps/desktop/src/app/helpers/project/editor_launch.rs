// C1 wave-1 deferred split: apps/desktop/src/app/helpers/project.rs (~4.3k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds workspace editor launch target types
// and the default/custom editor launch helpers. See
// docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::path::{Path, PathBuf};

use crate::app::helpers::*;
use crate::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiWorkspaceEditorLaunchKind {
    DirectPath,
    VscodeCompatible,
    ZedCompatible,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GpuiWorkspaceEditorTarget {
    pub(crate) command: &'static str,
    pub(crate) app_names: &'static [&'static str],
    pub(crate) launch_kind: GpuiWorkspaceEditorLaunchKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiCustomWorkspaceEditorCommand {
    pub(crate) executable: GpuiCustomWorkspaceEditorExecutable,
    pub(crate) args: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GpuiCustomWorkspaceEditorExecutable {
    AbsolutePath(PathBuf),
    PathSearch(String),
}

pub(crate) const GPUI_WORKSPACE_EDITOR_VSCODE_APP_NAMES: &[&str] = &["Visual Studio Code"];
pub(crate) const GPUI_WORKSPACE_EDITOR_VSCODE_INSIDERS_APP_NAMES: &[&str] =
    &["Visual Studio Code - Insiders"];
pub(crate) const GPUI_WORKSPACE_EDITOR_CODIUM_APP_NAMES: &[&str] = &["VSCodium"];
pub(crate) const GPUI_WORKSPACE_EDITOR_CURSOR_APP_NAMES: &[&str] = &["Cursor"];
pub(crate) const GPUI_WORKSPACE_EDITOR_WINDSURF_APP_NAMES: &[&str] = &["Windsurf"];
pub(crate) const GPUI_WORKSPACE_EDITOR_ZED_APP_NAMES: &[&str] = &["Zed", "Zed Preview"];
pub(crate) const GPUI_WORKSPACE_EDITOR_SUBLIME_APP_NAMES: &[&str] = &["Sublime Text"];

pub(crate) const GPUI_WORKSPACE_EDITOR_VSCODE_TARGET: GpuiWorkspaceEditorTarget =
    GpuiWorkspaceEditorTarget {
        command: "code",
        app_names: GPUI_WORKSPACE_EDITOR_VSCODE_APP_NAMES,
        launch_kind: GpuiWorkspaceEditorLaunchKind::VscodeCompatible,
    };

pub(crate) const GPUI_WORKSPACE_EDITOR_ZED_TARGET: GpuiWorkspaceEditorTarget =
    GpuiWorkspaceEditorTarget {
        command: "zed",
        app_names: GPUI_WORKSPACE_EDITOR_ZED_APP_NAMES,
        launch_kind: GpuiWorkspaceEditorLaunchKind::ZedCompatible,
    };

pub(crate) fn gpui_open_project_path_for_native_ide_action(
    action: GpuiSidebarNativeProjectPathAction,
    project_path: &Path,
) -> Result<(), String> {
    match action {
        GpuiSidebarNativeProjectPathAction::OpenActiveWorkspaceProjectInVscode => {
            gpui_open_project_path_in_editor_target(
                GPUI_WORKSPACE_EDITOR_VSCODE_TARGET,
                project_path,
            )
        }
        GpuiSidebarNativeProjectPathAction::OpenActiveWorkspaceProjectInZed => {
            gpui_open_project_path_in_editor_target(GPUI_WORKSPACE_EDITOR_ZED_TARGET, project_path)
        }
        GpuiSidebarNativeProjectPathAction::OpenWorkspaceProjectInIde => {
            gpui_open_project_path_in_default_editor(project_path)
        }
        _ => Err("Configured editor is not available for GPUI project open.".to_string()),
    }
}

pub(crate) fn gpui_open_project_path_in_default_editor(project_path: &Path) -> Result<(), String> {
    /*
    CDXC:Projects 2026-06-24-13:49:
    Generic GPUI project IDE opens are native-owned Settings behavior. The sidebar action supplies only a gxserver project id; this launcher supports the normalized built-in default editor commands with fixed argv or fixed macOS app names, suppresses stdio, and reports only generic failure text.

    CDXC:Projects 2026-06-24-13:57:
    Custom default editor command support is intentionally narrower than a shell: parse Settings-owned text into literal argv, reject shell syntax/placeholders, require an executable found by PATH or absolute executable path, append the gxserver-resolved project path as argv, suppress child stdio, and return generic UI failures without exposing command text or paths.
    */
    let settings = shared_settings::shared_sidebar_settings_snapshot().external_editor_settings();
    if settings.default_editor_command() == shared_settings::SharedDefaultEditorCommand::Other
        && settings.editor_command().trim() != shared_settings::DEFAULT_DEFAULT_EDITOR_COMMAND
    {
        return gpui_open_project_path_in_custom_default_editor(
            settings.editor_command(),
            project_path,
        );
    }
    let target = gpui_workspace_editor_target_from_settings(&settings)?;
    gpui_open_project_path_in_editor_target(target, project_path)
}

pub(crate) fn gpui_open_project_path_in_editor_target(
    target: GpuiWorkspaceEditorTarget,
    project_path: &Path,
) -> Result<(), String> {
    if gpui_command_exists_on_path(target.command) {
        return gpui_spawn_workspace_editor_command(target, project_path);
    }
    for app_name in target.app_names {
        if gpui_macos_named_app_exists(app_name) {
            return gpui_spawn_open_target_app_name(app_name, project_path)
                .map_err(|_| "Configured editor could not open that project.".to_string());
        }
    }
    Err("Configured editor is not available for GPUI project open.".to_string())
}

pub(crate) fn gpui_workspace_editor_target_from_settings(
    settings: &shared_settings::SharedDefaultEditorSettings,
) -> Result<GpuiWorkspaceEditorTarget, String> {
    match settings.default_editor_command() {
        shared_settings::SharedDefaultEditorCommand::Code => {
            Ok(GPUI_WORKSPACE_EDITOR_VSCODE_TARGET)
        }
        shared_settings::SharedDefaultEditorCommand::CodeInsiders => {
            Ok(GpuiWorkspaceEditorTarget {
                command: "code-insiders",
                app_names: GPUI_WORKSPACE_EDITOR_VSCODE_INSIDERS_APP_NAMES,
                launch_kind: GpuiWorkspaceEditorLaunchKind::VscodeCompatible,
            })
        }
        shared_settings::SharedDefaultEditorCommand::Codium => Ok(GpuiWorkspaceEditorTarget {
            command: "codium",
            app_names: GPUI_WORKSPACE_EDITOR_CODIUM_APP_NAMES,
            launch_kind: GpuiWorkspaceEditorLaunchKind::VscodeCompatible,
        }),
        shared_settings::SharedDefaultEditorCommand::Cursor => Ok(GpuiWorkspaceEditorTarget {
            command: "cursor",
            app_names: GPUI_WORKSPACE_EDITOR_CURSOR_APP_NAMES,
            launch_kind: GpuiWorkspaceEditorLaunchKind::VscodeCompatible,
        }),
        shared_settings::SharedDefaultEditorCommand::Windsurf => Ok(GpuiWorkspaceEditorTarget {
            command: "windsurf",
            app_names: GPUI_WORKSPACE_EDITOR_WINDSURF_APP_NAMES,
            launch_kind: GpuiWorkspaceEditorLaunchKind::VscodeCompatible,
        }),
        shared_settings::SharedDefaultEditorCommand::Zed => Ok(GPUI_WORKSPACE_EDITOR_ZED_TARGET),
        shared_settings::SharedDefaultEditorCommand::Zeditor => Ok(GpuiWorkspaceEditorTarget {
            command: "zeditor",
            app_names: GPUI_WORKSPACE_EDITOR_ZED_APP_NAMES,
            launch_kind: GpuiWorkspaceEditorLaunchKind::ZedCompatible,
        }),
        shared_settings::SharedDefaultEditorCommand::Subl => Ok(GpuiWorkspaceEditorTarget {
            command: "subl",
            app_names: GPUI_WORKSPACE_EDITOR_SUBLIME_APP_NAMES,
            launch_kind: GpuiWorkspaceEditorLaunchKind::DirectPath,
        }),
        shared_settings::SharedDefaultEditorCommand::Other
            if settings.editor_command().trim()
                == shared_settings::DEFAULT_DEFAULT_EDITOR_COMMAND =>
        {
            Ok(GPUI_WORKSPACE_EDITOR_VSCODE_TARGET)
        }
        shared_settings::SharedDefaultEditorCommand::Other => {
            Err("Configured editor is not available for GPUI project open.".to_string())
        }
    }
}

pub(crate) fn gpui_open_project_path_in_custom_default_editor(
    editor_command: &str,
    project_path: &Path,
) -> Result<(), String> {
    let command = gpui_parse_custom_workspace_editor_command(editor_command)?;
    gpui_spawn_custom_workspace_editor_command(&command, project_path)
}

pub(crate) fn gpui_parse_custom_workspace_editor_command(
    editor_command: &str,
) -> Result<GpuiCustomWorkspaceEditorCommand, String> {
    /*
    CDXC:Projects 2026-06-24-13:57:
    This parser is not a shell compatibility layer. It accepts only Settings-owned argv text, uses quotes/backslashes only to form literal tokens, and rejects shell control or expansion syntax so GPUI project opens never execute arbitrary custom command snippets.
    */
    let trimmed = editor_command.trim();
    if trimmed.is_empty()
        || gpui_custom_workspace_editor_command_has_unsupported_shell_syntax(trimmed)
    {
        return Err(
            "Custom default editor command is not supported for GPUI project open.".to_string(),
        );
    }
    let mut argv = gpui_split_custom_workspace_editor_argv(trimmed)?;
    let executable = argv.remove(0);
    if executable.trim().is_empty()
        || executable.contains('\\')
        || executable.chars().any(char::is_control)
    {
        return Err(
            "Custom default editor command is not supported for GPUI project open.".to_string(),
        );
    }

    let executable_path = PathBuf::from(&executable);
    let executable = if executable_path.is_absolute() {
        if !gpui_is_executable_file(&executable_path) {
            return Err("Configured editor is not available for GPUI project open.".to_string());
        }
        GpuiCustomWorkspaceEditorExecutable::AbsolutePath(executable_path)
    } else {
        if executable.contains('/') || executable.chars().any(char::is_whitespace) {
            return Err(
                "Custom default editor command is not supported for GPUI project open.".to_string(),
            );
        }
        if !gpui_command_exists_on_path(&executable) {
            return Err("Configured editor is not available for GPUI project open.".to_string());
        }
        GpuiCustomWorkspaceEditorExecutable::PathSearch(executable)
    };

    Ok(GpuiCustomWorkspaceEditorCommand {
        executable,
        args: argv,
    })
}

pub(crate) fn gpui_custom_workspace_editor_command_has_unsupported_shell_syntax(
    value: &str,
) -> bool {
    value.chars().any(|character| {
        character.is_control()
            || matches!(
                character,
                '|' | '&'
                    | ';'
                    | '<'
                    | '>'
                    | '`'
                    | '$'
                    | '*'
                    | '?'
                    | '%'
                    | '['
                    | ']'
                    | '{'
                    | '}'
                    | '#'
            )
    })
}

pub(crate) fn gpui_split_custom_workspace_editor_argv(
    command: &str,
) -> Result<Vec<String>, String> {
    let mut argv = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut token_started = false;
    let mut chars = command.chars().peekable();

    while let Some(character) = chars.next() {
        match quote {
            Some('\'') => {
                if character == '\'' {
                    quote = None;
                } else {
                    current.push(character);
                }
            }
            Some('"') => {
                if character == '"' {
                    quote = None;
                } else if character == '\\' {
                    let Some(next) = chars.next() else {
                        return Err(
                            "Custom default editor command is not supported for GPUI project open."
                                .to_string(),
                        );
                    };
                    if next == '"' || next == '\\' {
                        current.push(next);
                    } else {
                        current.push('\\');
                        current.push(next);
                    }
                } else {
                    current.push(character);
                }
            }
            Some(_) => unreachable!("custom editor parser uses only quote delimiters"),
            None => {
                if character.is_whitespace() {
                    if token_started {
                        argv.push(std::mem::take(&mut current));
                        token_started = false;
                    }
                } else if character == '\'' || character == '"' {
                    quote = Some(character);
                    token_started = true;
                } else if character == '\\' {
                    let Some(next) = chars.next() else {
                        return Err(
                            "Custom default editor command is not supported for GPUI project open."
                                .to_string(),
                        );
                    };
                    current.push(next);
                    token_started = true;
                } else {
                    current.push(character);
                    token_started = true;
                }
            }
        }
    }

    if quote.is_some() {
        return Err(
            "Custom default editor command is not supported for GPUI project open.".to_string(),
        );
    }
    if token_started {
        argv.push(current);
    }
    if argv.is_empty() {
        return Err(
            "Custom default editor command is not supported for GPUI project open.".to_string(),
        );
    }
    Ok(argv)
}
