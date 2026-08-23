// C1 wave-1 deferred split: apps/desktop/src/app/helpers/project.rs (~4.3k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds terminal search/attachment state types,
// workspace open-target detection, keep-awake types, and the add-project
// dialog operation type. See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::{
    collections::{HashMap, HashSet},
    io::Read,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crate::app::helpers::*;
use crate::*;

/// Mirrors the macOS host's per-surface `GhostexGhosttySearchState`: created by
/// START_SEARCH, updated by SEARCH_TOTAL/SEARCH_SELECTED, cleared by
/// END_SEARCH. Negative totals/selections from Ghostty mean "unknown".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GpuiTerminalSearchState {
    pub(crate) needle: String,
    pub(crate) total: Option<u64>,
    pub(crate) selected: Option<u64>,
}

/// Which pane family a GPUI-engine terminal view reports events for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiEngineTerminalEventTarget {
    Agents(TerminalSessionId),
    Command(CommandSessionId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GpuiTerminalAttachmentTarget {
    Terminal(GpuiEngineTerminalEventTarget),
    // Companion can display a shell id borrowed from a project-local remote
    // namespace. Retain the full shared workspace identity across the native
    // picker and upload so parked projects with the same numeric id cannot
    // redirect either the upload or its eventual paste.
    ProjectEditorCompanion {
        slot_id: ProjectEditorCompanionTerminalBodyMountSlotId,
        session_key: GpuiWorkspaceTerminalSessionKey,
    },
}

impl GpuiTerminalAttachmentTarget {
    pub(crate) fn engine_target(&self) -> GpuiEngineTerminalEventTarget {
        match self {
            Self::Terminal(target) => *target,
            Self::ProjectEditorCompanion { slot_id, .. } => {
                GpuiEngineTerminalEventTarget::Agents(slot_id.session_id)
            }
        }
    }
}

pub(crate) fn gpui_workspace_open_target_detection_script() -> String {
    // Same probe script macOS builds in createWorkspaceOpenTargetDetectionScript
    // (native/sidebar/native-sidebar.tsx): a login shell resolves the user's
    // real PATH for `command -v`, and app bundles are checked by directory
    // before falling back to one mdfind query per name.
    let mut lines = vec![
        "set +e".to_string(),
        "ghostex_app_exists() {".to_string(),
        "  local app_name=\"$1\"".to_string(),
        "  local app_bundle=\"${app_name}.app\"".to_string(),
        "  local base".to_string(),
        "  for base in /Applications \"$HOME/Applications\" /System/Applications; do".to_string(),
        "    if [ -d \"$base/$app_bundle\" ]; then return 0; fi".to_string(),
        "  done".to_string(),
        "  local found".to_string(),
        "  found=$(/usr/bin/mdfind \"kMDItemFSName == '$app_bundle'cd && kMDItemContentType == 'com.apple.application-bundle'\" 2>/dev/null | /usr/bin/head -n 1)".to_string(),
        "  [ -n \"$found\" ]".to_string(),
        "}".to_string(),
    ];
    for definition in GPUI_BUILT_IN_OPEN_TARGETS {
        for command in definition.commands {
            lines.push(format!(
                "if command -v {} >/dev/null 2>&1; then printf 'command\\t%s\\t%s\\n' {} {}; fi",
                gpui_shell_single_quote(command),
                gpui_shell_single_quote(definition.id),
                gpui_shell_single_quote(command),
            ));
        }
    }
    for definition in GPUI_BUILT_IN_OPEN_TARGETS {
        for app_name in definition.macos_app_names {
            lines.push(format!(
                "if ghostex_app_exists {} ; then printf 'app\\t%s\\t%s\\n' {} {}; fi",
                gpui_shell_single_quote(app_name),
                gpui_shell_single_quote(definition.id),
                gpui_shell_single_quote(app_name),
            ));
        }
    }
    lines.join("\n")
}

pub(crate) fn gpui_detect_workspace_open_target_availability() -> Option<GpuiDetectedOpenTargetAvailability> {
    // None mirrors the macOS catch path (spawn failure or timeout: keep the
    // stored availability). A scan that ran but failed still returns the
    // finder-only result, exactly like the exitCode gate on macOS.
    let mut child = Command::new("/bin/zsh")
        .arg("-lc")
        .arg(gpui_workspace_open_target_detection_script())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + GPUI_OPEN_TARGET_DETECTION_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    let mut stdout = String::new();
    if let Some(mut pipe) = child.stdout.take() {
        let _ = pipe.read_to_string(&mut stdout);
    }

    let built_in_ids = gpui_built_in_open_target_ids();
    let mut available_ids: HashSet<String> = HashSet::from(["finder".to_string()]);
    let mut resolved_commands = HashMap::new();
    let mut resolved_app_names = HashMap::new();
    if status.code() == Some(0) {
        for line in stdout.split('\n') {
            let mut parts = line.split('\t');
            let (Some(kind), Some(target_id), Some(value)) =
                (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            if target_id.is_empty() || value.is_empty() || !built_in_ids.contains(target_id) {
                continue;
            }
            available_ids.insert(target_id.to_string());
            if kind == "command" && !resolved_commands.contains_key(target_id) {
                resolved_commands.insert(target_id.to_string(), value.to_string());
            }
            if kind == "app" && !resolved_app_names.contains_key(target_id) {
                resolved_app_names.insert(target_id.to_string(), value.to_string());
            }
        }
    }
    let available_target_ids = GPUI_BUILT_IN_OPEN_TARGETS
        .iter()
        .filter(|definition| available_ids.contains(definition.id))
        .map(|definition| definition.id.to_string())
        .collect();
    Some(GpuiDetectedOpenTargetAvailability {
        available_target_ids,
        resolved_commands,
        resolved_app_names,
    })
}

pub(crate) fn gpui_detected_open_target_availability_matches_stored(
    detected: &GpuiDetectedOpenTargetAvailability,
    stored: &GpuiOpenTargetAvailability,
) -> bool {
    // checkedAtMs is deliberately outside the comparison, matching
    // workspaceOpenTargetAvailabilityEquals on macOS, so an unchanged machine
    // never rewrites settings at startup.
    detected.available_target_ids.len() == stored.available_ids.len()
        && detected
            .available_target_ids
            .iter()
            .all(|id| stored.available_ids.contains(id))
        && detected.resolved_commands == stored.resolved_commands
        && detected.resolved_app_names == stored.resolved_app_names
}

pub(crate) fn gpui_workspace_open_target_availability_settings_value(
    detected: &GpuiDetectedOpenTargetAvailability,
) -> serde_json::Value {
    let checked_at_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0);
    let mut resolved_commands = serde_json::Map::new();
    let mut resolved_app_names = serde_json::Map::new();
    for definition in GPUI_BUILT_IN_OPEN_TARGETS {
        if let Some(command) = detected.resolved_commands.get(definition.id) {
            resolved_commands.insert(definition.id.to_string(), serde_json::json!(command));
        }
        if let Some(app_name) = detected.resolved_app_names.get(definition.id) {
            resolved_app_names.insert(definition.id.to_string(), serde_json::json!(app_name));
        }
    }
    serde_json::json!({
        "availableTargetIds": detected.available_target_ids,
        "checkedAtMs": checked_at_ms,
        "resolvedAppNames": resolved_app_names,
        "resolvedCommands": resolved_commands,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GpuiKeepAwakePowerSnapshot {
    pub(crate) battery_percent: Option<f64>,
    pub(crate) external_display_connected: bool,
    pub(crate) low_power_mode: Option<bool>,
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GpuiKeepAwakePowerProbeOptions {
    pub(crate) include_battery: bool,
    pub(crate) include_external_display: bool,
    pub(crate) include_low_power_mode: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GpuiKeepAwakeWorkingSessionGraceState {
    pub(crate) previous_working_session_count: usize,
    pub(crate) grace_until: Option<Instant>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum GpuiWorkspaceTerminalSessionKey {
    Local(GpuiLocalWorkspaceSessionKey),
    Remote(GpuiRemoteAttachSessionKey),
}

impl GpuiWorkspaceTerminalSessionKey {
    pub(crate) fn scoped_project_id(&self) -> String {
        match self {
            Self::Local(key) => key.project_id.clone(),
            Self::Remote(key) => gpui_remote_scoped_project_id(
                key.remote_machine_id.as_str(),
                key.project_id.as_str(),
            ),
        }
    }

    pub(crate) fn as_local(&self) -> Option<&GpuiLocalWorkspaceSessionKey> {
        match self {
            Self::Local(key) => Some(key),
            Self::Remote(_) => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GpuiLocalWorkspaceAttachTerminalPlan {
    pub(crate) agent_icon: Option<&'static str>,
    pub(crate) attach_command: String,
    pub(crate) command_id: Option<String>,
    pub(crate) persistence_session_created: Option<bool>,
    pub(crate) startup_text: Option<String>,
    pub(crate) startup_text_disposition: Option<String>,
    pub(crate) title: String,
    pub(crate) working_directory: Option<String>,
    pub(crate) zmx_name: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiAddProjectDialogOperation {
    Add,
    Browse,
    CancelCloneJob,
    CreateDirectory,
    DiscoverSourceControl,
    ListMachines,
    LookupRepository,
    PreviewClone,
    ReadCloneJob,
    StartClone,
}

impl GpuiAddProjectDialogOperation {
    pub(crate) fn from_wire(value: &str) -> Option<Self> {
        match value {
            "add" => Some(Self::Add),
            "browse" => Some(Self::Browse),
            "cancelCloneJob" => Some(Self::CancelCloneJob),
            "createDirectory" => Some(Self::CreateDirectory),
            "discoverSourceControl" => Some(Self::DiscoverSourceControl),
            "listMachines" => Some(Self::ListMachines),
            "lookupRepository" => Some(Self::LookupRepository),
            "previewClone" => Some(Self::PreviewClone),
            "readCloneJob" => Some(Self::ReadCloneJob),
            "startClone" => Some(Self::StartClone),
            _ => None,
        }
    }

    pub(crate) fn as_wire(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Browse => "browse",
            Self::CancelCloneJob => "cancelCloneJob",
            Self::CreateDirectory => "createDirectory",
            Self::DiscoverSourceControl => "discoverSourceControl",
            Self::ListMachines => "listMachines",
            Self::LookupRepository => "lookupRepository",
            Self::PreviewClone => "previewClone",
            Self::ReadCloneJob => "readCloneJob",
            Self::StartClone => "startClone",
        }
    }

    pub(crate) fn endpoint(self) -> Option<&'static str> {
        match self {
            Self::Add => Some("/api/addProjectPath"),
            Self::Browse => Some("/api/browseProjectDirectories"),
            Self::CancelCloneJob => Some("/api/cancelRepositoryCloneJob"),
            Self::CreateDirectory => Some("/api/createProjectDirectory"),
            Self::DiscoverSourceControl => Some("/api/discoverSourceControl"),
            Self::ListMachines => None,
            Self::LookupRepository => Some("/api/lookupRepository"),
            Self::PreviewClone => Some("/api/previewRepositoryClone"),
            Self::ReadCloneJob => Some("/api/readRepositoryCloneJob"),
            Self::StartClone => Some("/api/startRepositoryClone"),
        }
    }

    pub(crate) fn timeout(self) -> Duration {
        match self {
            Self::Add | Self::StartClone => GPUI_ADD_PROJECT_DIALOG_ADD_TIMEOUT,
            Self::Browse => GPUI_ADD_PROJECT_DIALOG_BROWSE_TIMEOUT,
            Self::DiscoverSourceControl => GPUI_ADD_PROJECT_DIALOG_DISCOVERY_TIMEOUT,
            Self::LookupRepository | Self::PreviewClone => GPUI_ADD_PROJECT_DIALOG_LOOKUP_TIMEOUT,
            Self::CancelCloneJob
            | Self::CreateDirectory
            | Self::ReadCloneJob
            | Self::ListMachines => GPUI_ADD_PROJECT_DIALOG_JOB_TIMEOUT,
        }
    }
}

