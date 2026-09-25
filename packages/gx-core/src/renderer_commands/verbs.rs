//! What a renderer command asks for, validated. The host performs it.
//!
//! CDXC:CefRuntime 2026-09-25 WHY:
//! gxserver sends the CLI's renderer commands to the first socket that registered for them. That
//! was the QuickJS app runtime's socket, which answered 11 of the 25 actions gxserver allows and
//! failed the rest with "Unsupported renderer command." The runtime was removed, so the
//! desktop's own gx-client socket registers instead and this file is the validation the runtime
//! did. The verbs whose feature no longer exists in the app were taken out of gxserver's forward
//! list (`RENDERER_COMMAND_ACTIONS`) in the same change: `setVisibleCount` and `setViewMode` (the
//! GPUI app has no visible-session count or grid view mode), `saveAgent` (the renderer-era
//! agent-button writer), `sendMessage` to an agent id (a session selector goes to
//! `/api/sendSessionMessage` directly), and `assertSidebarCard` / `waitFor` (card assertions of the
//! React sidebar deleted on 2026-09-24). The CLI answers those verbs with a retired error; here they
//! answer "Unsupported renderer command." if an older daemon still sends them.
//! SEE-ALSO: server/src/server/mod.rs (`RENDERER_COMMAND_ACTIONS`),
//! apps/desktop/src/app/gx_store/renderer_commands/, packages/gx-client/src/worker.rs.

use serde_json::{Map, Value};

use super::errors::RendererCommandError;
use super::project_step::ProjectStepDirection;
use super::targets::{
    known_project_id, local_project_at_path, local_project_named, resolve_renderer_group_project,
    resolve_renderer_session, RendererSession,
};
use crate::core::Core;
use crate::keys::MachineId;
use crate::sidebar_view::text::{js_trim, utf16_len, utf16_prefix};

/// `SETTINGS_MODAL_NAVIGATION_TABS`, the tabs `ghostex settings open --tab` may name.
/// SEE-ALSO: packages/shared/ghostex-settings/settings-modal-navigation.ts (keep in lockstep).
pub const SETTINGS_MODAL_TABS: [&str; 14] = [
    "settings",
    "theme",
    "integrations",
    "extensions",
    "osIntegration",
    "remote",
    "projects",
    "agents",
    "accounts",
    "actions",
    "openTargets",
    "hotkeys",
    "debugging",
    "about",
];

const RENAME_TITLE_MAX_UTF16: usize = 120;
const OPEN_BROWSER_URL_MAX_UTF16: usize = 16 * 1024;
const SETTINGS_SEARCH_MAX_UTF16: usize = 200;
const SETTINGS_PATCH_MAX_KEYS: usize = 50;
/// Far above the ~100 hotkey ids; bounds what one `ghostex settings hotkeys` save can carry.
const SETTINGS_PATCH_MAX_HOTKEYS: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenPathsMode {
    Open,
    Edit,
}

/// One `ghostex open` / `edit` target: an absolute path, and for a file the `file:line:column`
/// position the CLI parsed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenPathTarget {
    pub path: String,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

/// One validated renderer command.
#[derive(Clone, Debug, PartialEq)]
pub enum RendererVerb {
    /// `focusSession` (`ghostex focus`, `focus-session`).
    FocusSession(RendererSession),
    /// `renameCommand`: type the agent's own rename command (`/rename`, `/name` for Pi, `/title`
    /// for Hermes Agent) with a real Enter.
    RenameCommand {
        session: RendererSession,
        title: String,
        /// `rename`, `name` or `title`.
        command: &'static str,
    },
    /// `runCommand` and `clickButton` with `kind: command`: a project Action by id.
    RunCommand {
        command_id: String,
    },
    ReadResourcesSnapshot,
    /// `ghostex settings set` / `reset`.
    UpdateSettingsPatch {
        patch: Map<String, Value>,
        keys: Vec<String>,
    },
    /// `ghostex settings open`.
    OpenSettings {
        tab: String,
        search_query: Option<String>,
    },
    /// `openBrowser` / `openBrowserPane`.
    OpenBrowser {
        project_id: Option<String>,
        reuse: &'static str,
        url: String,
    },
    /// `restartSession` and `fullReloadSession`, which are the same Full Reload.
    ReloadSession {
        session: RendererSession,
        message_type: &'static str,
    },
    ToggleCloseAfterDone(RendererSession),
    /// `switchProject` and `focusGroup`: activate a project the way the menu bar does.
    ActivateProject {
        project_id: String,
    },
    /// `moveProject`: one step up or down in this computer's sidebar order.
    MoveProject {
        project_id: String,
        direction: ProjectStepDirection,
    },
    ToggleSidebarCollapsed,
    /// `openPaths` (`ghostex open`, `ghostex edit`, `ghostex <path>`).
    OpenPaths {
        mode: OpenPathsMode,
        targets: Vec<OpenPathTarget>,
    },
}

fn string<'a>(payload: &'a Value, key: &str) -> Option<&'a str> {
    payload.get(key)?.as_str()
}

/// `readGpuiRecordString(payload, key)?.trim()` as a non-empty value.
fn trimmed(payload: &Value, key: &str) -> Option<String> {
    let value = js_trim(string(payload, key)?);
    (!value.is_empty()).then(|| value.to_string())
}

/// `gpuiWorkspaceTerminalTitleCommandForAgent`.
///
/// CDXC:Sessions 2026-07-28:
/// Pi names its session with `/name <title>` and Hermes Agent uses `/title <title>` instead of
/// `/rename <title>`, so the payload carries a fixed command selector resolved from the session's
/// own agent identity. Rust still owns turning that selector into the actual terminal input.
fn title_command_for_agent(agent: &str) -> &'static str {
    match js_trim(agent).to_lowercase().as_str() {
        "pi" | "π" => "name",
        "hermes" | "hermes agent" | "hermes-agent" => "title",
        _ => "rename",
    }
}

/// `normalizeGpuiRendererCommandRenameTitle`.
fn rename_title(payload: &Value) -> Option<String> {
    let raw = string(payload, "title")?;
    if raw
        .chars()
        .any(|character| matches!(character, '\u{0000}'..='\u{001f}' | '\u{007f}'..='\u{009f}'))
    {
        return None;
    }
    let title = js_trim(raw);
    (!title.is_empty() && utf16_len(title) <= RENAME_TITLE_MAX_UTF16).then(|| title.to_string())
}

/// Validates a renderer command and resolves what it names against the store.
pub fn plan_renderer_command(
    core: &Core,
    action: &str,
    payload: &Value,
) -> Result<RendererVerb, RendererCommandError> {
    match action {
        "focusSession" => resolve_renderer_session(core, payload, false)
            .map(RendererVerb::FocusSession)
            .ok_or(RendererCommandError::NoMatchingSession),
        "renameCommand" => {
            let session = resolve_renderer_session(core, payload, false)
                .ok_or(RendererCommandError::NoMatchingSession)?;
            let title = rename_title(payload).ok_or(RendererCommandError::InvalidTitle)?;
            let agent = core
                .presentation()
                .loaded(&MachineId::Local)
                .and_then(|loaded| {
                    loaded.server_session(&session.key.project_id, &session.key.session_id)
                })
                .and_then(|row| row.agent_id.clone().or_else(|| row.agent_name.clone()))
                .unwrap_or_default();
            Ok(RendererVerb::RenameCommand {
                session,
                title,
                command: title_command_for_agent(&agent),
            })
        }
        "runCommand" => run_command(payload, "commandId"),
        "clickButton" => {
            if string(payload, "kind").map(js_trim) != Some("command") {
                return Err(RendererCommandError::Unsupported);
            }
            run_command(payload, "id")
        }
        "readResourcesSnapshot" => Ok(RendererVerb::ReadResourcesSnapshot),
        "updateSettingsPatch" => settings_patch(payload),
        "openSettings" => {
            let tab = trimmed(payload, "tab").unwrap_or_else(|| "settings".to_string());
            if !SETTINGS_MODAL_TABS.contains(&tab.as_str()) {
                return Err(RendererCommandError::InvalidSettingsTab);
            }
            let search_query = string(payload, "searchQuery")
                .map(|query| utf16_prefix(js_trim(query), SETTINGS_SEARCH_MAX_UTF16).to_string())
                .filter(|query| !query.is_empty());
            Ok(RendererVerb::OpenSettings { tab, search_query })
        }
        "openBrowser" | "openBrowserPane" => open_browser(core, payload),
        "restartSession" | "fullReloadSession" => {
            let session = resolve_renderer_session(core, payload, true)
                .ok_or(RendererCommandError::NoMatchingSession)?;
            Ok(RendererVerb::ReloadSession {
                session,
                message_type: if action == "restartSession" {
                    "restartSession"
                } else {
                    "fullReloadSession"
                },
            })
        }
        "toggleCloseAfterDone" => resolve_renderer_session(core, payload, true)
            .map(RendererVerb::ToggleCloseAfterDone)
            .ok_or(RendererCommandError::NoMatchingSession),
        "switchProject" => {
            let project_id = if let Some(project_id) = trimmed(payload, "projectId") {
                known_project_id(core, &project_id)
            } else if let Some(path) = trimmed(payload, "path") {
                local_project_at_path(core, &path)
            } else {
                trimmed(payload, "name").and_then(|name| local_project_named(core, &name))
            };
            project_id
                .map(|project_id| RendererVerb::ActivateProject { project_id })
                .ok_or(RendererCommandError::NoMatchingProject)
        }
        "focusGroup" => trimmed(payload, "groupId")
            .and_then(|group_id| resolve_renderer_group_project(&group_id))
            .and_then(|project_id| known_project_id(core, &project_id))
            .map(|project_id| RendererVerb::ActivateProject { project_id })
            .ok_or(RendererCommandError::NoMatchingProject),
        "moveProject" => {
            let direction = match trimmed(payload, "direction").map(|value| value.to_lowercase()) {
                Some(value) if value == "up" => ProjectStepDirection::Up,
                Some(value) if value == "down" => ProjectStepDirection::Down,
                _ => return Err(RendererCommandError::InvalidDirection),
            };
            let project_id = trimmed(payload, "projectId")
                .filter(|project_id| {
                    core.presentation()
                        .machine(&MachineId::Local)
                        .is_some_and(|machine| machine.domain_project(project_id).is_some())
                })
                .ok_or(RendererCommandError::NoMatchingProject)?;
            Ok(RendererVerb::MoveProject {
                project_id,
                direction,
            })
        }
        "toggleSidebarCollapsed" => Ok(RendererVerb::ToggleSidebarCollapsed),
        "openPaths" => open_paths(payload),
        _ => Err(RendererCommandError::Unsupported),
    }
}

fn run_command(payload: &Value, key: &str) -> Result<RendererVerb, RendererCommandError> {
    trimmed(payload, key)
        .map(|command_id| RendererVerb::RunCommand { command_id })
        .ok_or(RendererCommandError::Unsupported)
}

/// `applyRendererSettingsPatch`'s checks: 1 to 50 keys, each a boolean, string or finite number.
/// CDXC:Hotkeys 2026-09-25 WHY:
/// `hotkeys` is the one structured value allowed: `ghostex settings hotkeys` sends the complete
/// action id -> keys map the Hotkeys page saves, so it may only be an object of strings.
/// SEE-ALSO: server/src/ghostex_cli/settings_hotkeys.rs.
fn settings_patch(payload: &Value) -> Result<RendererVerb, RendererCommandError> {
    let patch = payload
        .get("patch")
        .and_then(Value::as_object)
        .ok_or(RendererCommandError::InvalidSettingsPatch)?;
    if patch.is_empty() || patch.len() > SETTINGS_PATCH_MAX_KEYS {
        return Err(RendererCommandError::InvalidSettingsPatch);
    }
    let allowed = |key: &String, value: &Value| match value {
        Value::Bool(_) | Value::String(_) => true,
        Value::Number(number) => number.as_f64().is_some_and(f64::is_finite),
        Value::Object(hotkeys) if key == "hotkeys" => {
            hotkeys.len() <= SETTINGS_PATCH_MAX_HOTKEYS && hotkeys.values().all(Value::is_string)
        }
        _ => false,
    };
    if !patch.iter().all(|(key, value)| allowed(key, value)) {
        return Err(RendererCommandError::InvalidSettingsPatch);
    }
    Ok(RendererVerb::UpdateSettingsPatch {
        keys: patch.keys().cloned().collect(),
        patch: patch.clone(),
    })
}

/// `openEmbeddedBrowserFromRendererCommand`: the address, the reuse rule and the project the
/// selectors name. A selector that names nothing is refused rather than opened in the active
/// project.
fn open_browser(core: &Core, payload: &Value) -> Result<RendererVerb, RendererCommandError> {
    let url = string(payload, "url").map(js_trim).unwrap_or_default();
    if utf16_len(url) > OPEN_BROWSER_URL_MAX_UTF16 {
        return Err(RendererCommandError::Failed);
    }
    let reuse = match string(payload, "reuse")
        .map(|value| js_trim(value).to_lowercase())
        .as_deref()
    {
        Some("exact") => "exact",
        Some("none") => "none",
        _ => "similar",
    };
    let group_id = trimmed(payload, "groupId");
    let requested_project_id = trimmed(payload, "projectId");
    let project_path = trimmed(payload, "projectPath");
    let project_id = if let Some(group_id) = &group_id {
        resolve_renderer_group_project(group_id)
            .and_then(|project_id| known_project_id(core, &project_id))
    } else if let Some(project_id) = &requested_project_id {
        known_project_id(core, project_id)
    } else {
        project_path
            .as_deref()
            .and_then(|path| local_project_at_path(core, path))
    };
    let named = group_id.is_some() || requested_project_id.is_some() || project_path.is_some();
    if named && project_id.is_none() {
        return Err(RendererCommandError::NoMatchingProject);
    }
    Ok(RendererVerb::OpenBrowser {
        project_id,
        reuse,
        url: url.to_string(),
    })
}

/// `ghostex open` / `edit`: the resolved target paths. `wait` has no answer in this app.
fn open_paths(payload: &Value) -> Result<RendererVerb, RendererCommandError> {
    if payload.get("wait").and_then(Value::as_bool) == Some(true) {
        return Err(RendererCommandError::WaitUnsupported);
    }
    let mode = match string(payload, "mode") {
        Some("edit") => OpenPathsMode::Edit,
        _ => OpenPathsMode::Open,
    };
    let position = |target: &Value, key: &str| {
        target
            .get(key)
            .and_then(Value::as_u64)
            .filter(|value| *value > 0)
            .and_then(|value| u32::try_from(value).ok())
    };
    let targets: Vec<OpenPathTarget> = payload
        .get("targets")
        .and_then(Value::as_array)
        .map(|targets| {
            targets
                .iter()
                .filter_map(|target| {
                    let path = js_trim(target.get("path")?.as_str()?);
                    (!path.is_empty()).then(|| OpenPathTarget {
                        path: path.to_string(),
                        line: position(target, "line"),
                        column: position(target, "column"),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    if targets.is_empty() {
        return Err(RendererCommandError::NoPaths);
    }
    Ok(RendererVerb::OpenPaths { mode, targets })
}
