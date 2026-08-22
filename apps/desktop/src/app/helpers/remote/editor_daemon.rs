// C1 wave-1 deferred split: apps/desktop/src/app/helpers/remote.rs (~8.2k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds the standalone GhostexEditor daemon
// resolution and front/prewarm integration. See
// docs/2026-08-22/repo-restructure/SPLITS.md C1.
#![allow(dead_code)]

use std::{
    env,
    path::PathBuf,
    process::{Command, Stdio},
    time::Duration,
};

use crate::app::helpers::*;
use crate::*;

pub(crate) fn gpui_prompt_editor_backend_setting_is_monaco() -> bool {
    shared_settings::shared_sidebar_settings_snapshot()
        .object()
        .get("promptEditorBackend")
        .and_then(serde_json::Value::as_str)
        == Some("monaco")
}

pub(crate) fn gpui_current_zmx_prompt_editor_attach_mode_is_monaco() -> bool {
    gpui_prompt_editor_backend_setting_is_monaco()
        && gpui_resolved_ghostex_editor_executable().is_some()
}

pub(crate) fn gpui_resolved_ghostex_editor_executable() -> Option<PathBuf> {
    env::var_os("GHOSTEX_EDITOR_APP")
        .and_then(|value| gpui_ghostex_editor_executable_candidate(PathBuf::from(value)))
        .or_else(gpui_default_ghostex_editor_executable)
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_default_ghostex_editor_executable() -> Option<PathBuf> {
    /*
    Packaged builds ship the daemon at Contents/Resources/GhostexEditor.app, so
    prefer the app-owned copy: it is version-matched and signed with the same
    team as the app that launches it. Standalone installs and the source
    checkout stay as fallbacks for development bundles that ship no helper.
    */
    gpui_app_bundle_resources_dir()
        .and_then(|resources_dir| {
            gpui_ghostex_editor_executable_candidate(resources_dir.join("GhostexEditor.app"))
        })
        .or_else(|| {
            gpui_ghostex_editor_executable_candidate(
                gpui_home_dir().join("Applications/GhostexEditor.app"),
            )
        })
        .or_else(|| {
            gpui_ghostex_editor_executable_candidate(PathBuf::from(
                "/Applications/GhostexEditor.app",
            ))
        })
        .or_else(|| {
            gpui_ghostex_editor_executable_candidate(
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../editor/dist/GhostexEditor.app"),
            )
        })
}

#[cfg(target_os = "linux")]
pub(crate) fn gpui_default_ghostex_editor_executable() -> Option<PathBuf> {
    gpui_ghostex_editor_executable_candidate(gpui_home_dir().join(".local/bin/ghostex-editor"))
        .or_else(|| {
            gpui_ghostex_editor_executable_candidate(PathBuf::from("/usr/local/bin/ghostex-editor"))
        })
        .or_else(|| {
            gpui_ghostex_editor_executable_candidate(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../editor/dist/desktop/ghostex-editor"),
            )
        })
}

#[cfg(target_os = "windows")]
pub(crate) fn gpui_default_ghostex_editor_executable() -> Option<PathBuf> {
    env::var_os("LOCALAPPDATA")
        .and_then(|value| {
            gpui_ghostex_editor_executable_candidate(
                PathBuf::from(value).join("Ghostex/GhostexEditor/GhostexEditor.exe"),
            )
        })
        .or_else(|| {
            gpui_ghostex_editor_executable_candidate(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../editor/dist/desktop/GhostexEditor.exe"),
            )
        })
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub(crate) fn gpui_default_ghostex_editor_executable() -> Option<PathBuf> {
    None
}

pub(crate) fn gpui_ghostex_editor_executable_candidate(candidate: PathBuf) -> Option<PathBuf> {
    let executable = if gpui_is_dir(&candidate)
        && candidate
            .extension()
            .and_then(|extension| extension.to_str())
            == Some("app")
    {
        candidate.join("Contents/MacOS/GhostexEditor")
    } else {
        candidate
    };
    gpui_is_executable_file(&executable).then_some(executable)
}

/// Socket resolution mirror of the daemon's `resolveSocketPath`
/// (apps/editor/macos DaemonSupport.swift ↔ scripts/ghostex-cli.mjs): env
/// override, then the shared Ghostex runtime directory.
#[cfg(unix)]
pub(crate) fn gpui_ghostex_editor_socket_path() -> PathBuf {
    if let Some(path) = env::var_os("GHOSTEX_EDITOR_SOCKET")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
    {
        return path;
    }
    shared_settings::ghostex_storage_paths()
        .runtime_dir
        .join("ghostex-editor.sock")
}

#[cfg(unix)]
pub(crate) fn gpui_ghostex_editor_daemon_request(request: &serde_json::Value) -> Option<serde_json::Value> {
    use std::io::{BufRead as _, BufReader, Write as _};
    let mut stream =
        std::os::unix::net::UnixStream::connect(gpui_ghostex_editor_socket_path()).ok()?;
    let timeout = Some(Duration::from_millis(750));
    stream.set_read_timeout(timeout).ok()?;
    stream.set_write_timeout(timeout).ok()?;
    let mut line = serde_json::to_string(request).ok()?;
    line.push('\n');
    stream.write_all(line.as_bytes()).ok()?;
    let mut response_line = String::new();
    BufReader::new(stream).read_line(&mut response_line).ok()?;
    serde_json::from_str(response_line.trim()).ok()
}

pub(crate) fn gpui_ghostex_editor_daemon_open_count() -> u64 {
    #[cfg(unix)]
    {
        gpui_ghostex_editor_daemon_request(
            &serde_json::json!({"v": GHOSTEX_EDITOR_PROTOCOL_VERSION, "type": "ping"}),
        )
        .and_then(|response| {
            response
                .get("openCount")
                .and_then(serde_json::Value::as_u64)
        })
        .unwrap_or(0)
    }
    #[cfg(not(unix))]
    {
        0
    }
}

pub(crate) fn gpui_ghostex_editor_daemon_bring_to_front() {
    let _ = gpui_ghostex_editor_daemon_front(None);
}

pub(crate) fn gpui_ghostex_editor_daemon_front(originating_session_id: Option<&str>) -> bool {
    #[cfg(unix)]
    {
        let mut request = serde_json::json!({
            "v": GHOSTEX_EDITOR_PROTOCOL_VERSION,
            "type": "front",
        });
        if let Some(originating_session_id) = originating_session_id {
            request["originatingSessionId"] =
                serde_json::Value::String(originating_session_id.to_string());
        }
        return gpui_ghostex_editor_daemon_request(&request).is_some_and(|response| {
            response.get("type").and_then(serde_json::Value::as_str) == Some("fronted")
                && response
                    .get("frontedCount")
                    .and_then(serde_json::Value::as_u64)
                    .is_some_and(|count| count > 0)
        });
    }
    #[cfg(not(unix))]
    {
        let _ = originating_session_id;
        false
    }
}

pub(crate) fn gpui_prewarm_ghostex_editor_daemon() {
    let Some(executable) = gpui_resolved_ghostex_editor_executable() else {
        return;
    };
    let mut command = Command::new(executable);
    command
        .arg("--daemon")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        command
            .arg("--socket")
            .arg(gpui_ghostex_editor_socket_path());
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        const DETACHED_PROCESS: u32 = 0x00000008;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW);
    }
    let _ = command.spawn();
}

