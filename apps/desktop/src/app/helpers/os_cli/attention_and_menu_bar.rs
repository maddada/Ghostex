use std::{
    collections::{HashMap, HashSet},
    env,
    path::{Path, PathBuf},
    time::Instant,
};

use anyhow::Result;

use crate::app::helpers::*;
use crate::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiSidebarNativeAppShotPromptMessage {
    pub(crate) prompt: String,
    pub(crate) session_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiSessionAttentionNotificationCandidate {
    pub(crate) body: String,
    pub(crate) icon_data_url: Option<String>,
    pub(crate) session_id: String,
    pub(crate) title: String,
}

#[derive(Debug, Default)]
pub(crate) struct GpuiSessionAttentionNotificationRateLimiter {
    pub(crate) global_window_count: usize,
    pub(crate) global_window_started_at: Option<Instant>,
    pub(crate) session_last_sent_at: HashMap<String, Instant>,
}

impl GpuiSessionAttentionNotificationRateLimiter {
    pub(crate) fn consume(&mut self, session_id: &str, now: Instant) -> bool {
        if self
            .session_last_sent_at
            .get(session_id)
            .is_some_and(|previous| {
                now.duration_since(*previous) < GPUI_SESSION_ATTENTION_NOTIFICATION_SESSION_COOLDOWN
            })
        {
            return false;
        }

        if self.global_window_started_at.is_none_or(|started_at| {
            now.duration_since(started_at) >= GPUI_SESSION_ATTENTION_NOTIFICATION_GLOBAL_WINDOW
        }) {
            self.global_window_started_at = Some(now);
            self.global_window_count = 0;
        }
        if self.global_window_count >= GPUI_SESSION_ATTENTION_NOTIFICATION_GLOBAL_LIMIT {
            return false;
        }

        self.global_window_count += 1;
        self.session_last_sent_at
            .insert(session_id.to_string(), now);
        true
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct GpuiMenuBarStatusItemState {
    pub(crate) attention_count: u64,
    pub(crate) available_count: u64,
    pub(crate) working_count: u64,
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct GpuiMenuBarStatusNativeSessionEntry {
    pub(crate) session_id: *const std::ffi::c_char,
    pub(crate) title: *const std::ffi::c_char,
    pub(crate) last_active_at: *const std::ffi::c_char,
    pub(crate) status: i32,
    pub(crate) order: u64,
}

pub(crate) fn gpui_session_attention_notification_candidates(
    previous: &GpuiSidebarSessionStatusIndicatorsState,
    next: &GpuiSidebarSessionStatusIndicatorsState,
) -> Vec<GpuiSessionAttentionNotificationCandidate> {
    /*
    CDXC:Notifications 2026-06-26-06:56:
    Attention notification detection is an edge detector over the sanitized status model, not a payload replay or count watcher. A row is eligible only when its bounded session id was absent from the previous attention set and the next row itself carries a bounded title/project title already accepted by the parser.
    */
    let previous_attention_session_ids = previous
        .projects
        .iter()
        .flat_map(|project| project.sessions.iter())
        .filter(|session| session.status == GpuiStatusIndicatorStatus::Attention)
        .map(|session| session.session_id.as_str())
        .collect::<HashSet<_>>();
    let mut emitted_session_ids = HashSet::new();
    let mut candidates = Vec::new();
    for project in &next.projects {
        for session in &project.sessions {
            if session.status != GpuiStatusIndicatorStatus::Attention
                || previous_attention_session_ids.contains(session.session_id.as_str())
                || !emitted_session_ids.insert(session.session_id.as_str())
            {
                continue;
            }
            candidates.push(GpuiSessionAttentionNotificationCandidate {
                body: if project.title.trim().is_empty() {
                    "Ghostex".to_string()
                } else {
                    project.title.clone()
                },
                icon_data_url: project.icon_data_url.clone(),
                session_id: session.session_id.clone(),
                title: session.title.clone(),
            });
        }
    }
    candidates
}

pub(crate) fn gpui_menu_bar_status_item_visible_state(
    state: &GpuiSidebarSessionStatusIndicatorsState,
) -> Option<GpuiMenuBarStatusItemState> {
    /*
    CDXC:StatusPet 2026-06-26-05:42:
    Match the macOS menu-bar visibility rule in pure Rust before calling AppKit: the saved hideMenuBarSessionStatusIndicators setting removes the status item, attention/working counts suppress the available count, and an idle available badge appears only when no action-state count is visible.

    CDXC:StatusPet 2026-06-26-05:44:
    Current macOS parity keeps the menu-bar item visible when the saved hide setting is false, even with zero sessions, because the button is the Running Agents dropdown target. Represent the empty case as an available-style count of 0.
    */
    if state.hide_menu_bar_indicators {
        return None;
    }
    if state.attention_count > 0 || state.working_count > 0 {
        return Some(GpuiMenuBarStatusItemState {
            attention_count: state.attention_count,
            available_count: 0,
            working_count: state.working_count,
        });
    }
    Some(GpuiMenuBarStatusItemState {
        attention_count: 0,
        available_count: state.available_count,
        working_count: 0,
    })
}

pub(crate) fn gpui_sidebar_native_app_shot_prompt_from_json(
    text: &str,
) -> Result<GpuiSidebarNativeAppShotPromptMessage, ()> {
    /*
    CDXC:AppShots 2026-06-25-23:28:
    App Shot prompt insertion is a strictly allowlisted session contract. Accept only version/type, one bounded gxserver presentation session id, and the already formatted prompt string; reject generic action names, paths as separate fields, command/stdout/stderr data, NULs, and oversized payloads before terminal ownership is consulted.

    CDXC:AppShots 2026-06-26-04:27:
    Remote App Shot insertion may identify only a machine-scoped `remote:<machine>:session:<project>:<session>` row. The parser must still reject malformed remote ids and any renderer-provided path, SSH, URL, token, command, output, or terminal text fields.
    */
    let value = serde_json::from_str::<serde_json::Value>(text).map_err(|_| ())?;
    let object = value.as_object().ok_or(())?;
    if object
        .keys()
        .any(|key| !["version", "type", "sessionId", "prompt"].contains(&key.as_str()))
    {
        return Err(());
    }
    if object.get("version").and_then(serde_json::Value::as_u64)
        != Some(GPUI_SIDEBAR_NATIVE_APP_SHOT_PROMPT_MESSAGE_VERSION)
        || object.get("type").and_then(serde_json::Value::as_str)
            != Some(GPUI_SIDEBAR_NATIVE_APP_SHOT_PROMPT_MESSAGE_TYPE)
    {
        return Err(());
    }
    let session_id = object
        .get("sessionId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|session_id| gpui_sidebar_gxserver_presentation_session_id_allowed(session_id))
        .ok_or(())?
        .to_string();
    let prompt = object
        .get("prompt")
        .and_then(serde_json::Value::as_str)
        .filter(|prompt| {
            !prompt.trim().is_empty()
                && prompt.chars().count() <= GPUI_NATIVE_APP_SHOT_PROMPT_MAX_CHARS
                && !prompt.contains('\0')
        })
        .ok_or(())?
        .to_string();
    Ok(GpuiSidebarNativeAppShotPromptMessage { prompt, session_id })
}

pub(crate) fn gpui_spawn_custom_workspace_editor_command(
    command: &GpuiCustomWorkspaceEditorCommand,
    project_path: &Path,
) -> Result<(), String> {
    let mut process = match &command.executable {
        GpuiCustomWorkspaceEditorExecutable::AbsolutePath(path) => std::process::Command::new(path),
        GpuiCustomWorkspaceEditorExecutable::PathSearch(executable) => {
            let mut process = std::process::Command::new("/usr/bin/env");
            process.arg(executable);
            process
        }
    };
    for arg in &command.args {
        process.arg(arg);
    }
    process
        .arg(project_path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|_| "Configured editor could not open that project.".to_string())
}

pub(crate) fn gpui_spawn_workspace_editor_command(
    target: GpuiWorkspaceEditorTarget,
    project_path: &Path,
) -> Result<(), String> {
    let mut command = std::process::Command::new("/usr/bin/env");
    command.arg(target.command);
    match target.launch_kind {
        GpuiWorkspaceEditorLaunchKind::DirectPath => {
            command.arg(project_path);
        }
        GpuiWorkspaceEditorLaunchKind::VscodeCompatible => {
            command.arg(project_path).arg("--reuse-window");
        }
        GpuiWorkspaceEditorLaunchKind::ZedCompatible => {
            command.arg(project_path).arg("--existing");
        }
    }
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|_| "Configured editor could not open that project.".to_string())
}

pub(crate) fn gpui_command_exists_on_path(command: &str) -> bool {
    if command.is_empty()
        || command.contains('/')
        || command.contains('\\')
        || command.chars().any(char::is_whitespace)
    {
        return false;
    }
    let Some(path_value) = env::var_os("PATH") else {
        return false;
    };
    env::split_paths(&path_value).any(|directory| {
        let candidate = directory.join(command);
        gpui_is_executable_file(&candidate)
    })
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_macos_named_app_exists(app_name: &str) -> bool {
    let bundle_name = format!("{app_name}.app");
    [
        Some(PathBuf::from("/Applications")),
        env::var_os("HOME").map(|home| PathBuf::from(home).join("Applications")),
        Some(PathBuf::from("/System/Applications")),
    ]
    .into_iter()
    .flatten()
    .any(|directory| directory.join(&bundle_name).is_dir())
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn gpui_macos_named_app_exists(_app_name: &str) -> bool {
    false
}
