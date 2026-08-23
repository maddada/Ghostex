use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt as _;

use anyhow::Result;

use crate::app::helpers::*;
use crate::*;

pub(crate) fn gpui_ghostex_folder_stats_message() -> serde_json::Value {
    /*
    CDXC:GPUISettingsStatusBridge 2026-06-24-11:36:
    Settings storage stats may inspect only the GPUI-resolved Ghostex home and its immediate child directories. Do not trust React-provided paths, follow symlink directories, write logs, or scan unrelated project/workspace trees.
    */
    let folder_path = shared_settings::ghostex_storage_paths().data_dir.clone();
    let folder_path_string = gpui_path_string(&folder_path);
    if !gpui_is_dir(&folder_path) {
        return serde_json::json!({
            "errorMessage": "Ghostex home is not available for GPUI folder stats.",
            "folderPath": folder_path_string,
            "folders": [],
            "generatedAt": gpui_status_generated_at(),
            "totalBytes": 0,
            "type": "ghostexFolderStats",
        });
    }

    let mut folders = fs::read_dir(&folder_path)
        .map(|children| {
            children
                .filter_map(Result::ok)
                .filter_map(|child| {
                    let metadata = fs::symlink_metadata(child.path()).ok()?;
                    if !metadata.is_dir() {
                        return None;
                    }
                    let path = child.path();
                    let name = child.file_name().to_string_lossy().to_string();
                    let size_bytes = gpui_directory_size_bytes(&path);
                    Some(serde_json::json!({
                        "name": name,
                        "path": gpui_path_string(&path),
                        "sizeBytes": size_bytes,
                    }))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    folders.sort_by(|left, right| {
        let left_size = left
            .get("sizeBytes")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let right_size = right
            .get("sizeBytes")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let left_name = left
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let right_name = right
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        right_size
            .cmp(&left_size)
            .then_with(|| left_name.cmp(right_name))
    });
    let total_bytes = folders
        .iter()
        .filter_map(|folder| folder.get("sizeBytes").and_then(serde_json::Value::as_u64))
        .sum::<u64>();

    serde_json::json!({
        "folderPath": folder_path_string,
        "folders": folders,
        "generatedAt": gpui_status_generated_at(),
        "totalBytes": total_bytes,
        "type": "ghostexFolderStats",
    })
}

/// Every queue row held for this session, plus how many of them are `failed`.
/// A `failed` row waits on the user rather than on the agent, but it still
/// counts: a queue stalled behind one has stopped dead, and hiding it would make
/// that look identical to no queue. This mirrors the sidebar badge's
/// `queuedPromptCount` / `queuedPromptFailedCount` exactly — one count, one
/// meaning, on every surface.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct GpuiSessionChatQueuedCounts {
    pub(crate) total: usize,
    pub(crate) failed: usize,
}

pub(crate) fn gpui_session_chat_queued_counts_from_result(
    value: &serde_json::Value,
) -> GpuiSessionChatQueuedCounts {
    let Some(rows) = value.get("queue").and_then(serde_json::Value::as_array) else {
        return GpuiSessionChatQueuedCounts::default();
    };
    GpuiSessionChatQueuedCounts {
        total: rows.len(),
        failed: rows
            .iter()
            .filter(|row| row.get("state").and_then(serde_json::Value::as_str) == Some("failed"))
            .count(),
    }
}

pub(crate) fn gpui_spawn_zmx_refresh_if_stale_process(
    session_name: Option<String>,
    grid_size: Option<(u16, u16)>,
    reason: &'static str,
) {
    let Some(session_name) = session_name.filter(|name| !name.trim().is_empty()) else {
        support_logs::append(
            support_logs::GpuiSupportLog::TerminalFocus,
            "gpui.zmxPersistenceViewportRefresh.ifStale",
            serde_json::json!({
                "didRequest": false,
                "reason": reason,
                "skipReason": "missingSessionName",
            }),
        );
        return;
    };
    let Some((rows, columns)) = grid_size.filter(|(rows, columns)| *rows > 0 && *columns > 0)
    else {
        support_logs::append(
            support_logs::GpuiSupportLog::TerminalFocus,
            "gpui.zmxPersistenceViewportRefresh.ifStale",
            serde_json::json!({
                "didRequest": false,
                "reason": reason,
                "skipReason": "invalidSurfaceSize",
            }),
        );
        return;
    };
    #[cfg(target_os = "windows")]
    {
        let Ok(windows_terminal_backend::ResolvedWindowsTerminalBackend::Wsl { distribution }) =
            windows_terminal_backend::resolve_current()
        else {
            support_logs::append(
                support_logs::GpuiSupportLog::TerminalFocus,
                "gpui.zmxPersistenceViewportRefresh.ifStale",
                serde_json::json!({
                    "didRequest": false,
                    "reason": reason,
                    "skipReason": "windowsPowerShellBackend",
                }),
            );
            return;
        };
        std::thread::spawn(move || {
            let spawned = windows_terminal_backend::spawn_zmx_refresh(
                distribution.as_str(),
                session_name.as_str(),
                rows,
                columns,
            );
            let Ok(child) = spawned else {
                gpui_record_zmx_refresh_launch_failure(rows, columns, reason);
                return;
            };
            gpui_monitor_zmx_refresh_process(child, rows, columns, reason);
        });
        return;
    }
    #[cfg(not(target_os = "windows"))]
    let Some(zmx_path) = gpui_resolve_local_zmx_binary() else {
        support_logs::append(
            support_logs::GpuiSupportLog::TerminalFocus,
            "gpui.zmxPersistenceViewportRefresh.ifStale",
            serde_json::json!({
                "didRequest": false,
                "reason": reason,
                "skipReason": "missingBundledZmx",
            }),
        );
        return;
    };
    #[cfg(not(target_os = "windows"))]
    std::thread::spawn(move || {
        let spawned = Command::new(&zmx_path)
            .args([
                "refresh-if-stale",
                session_name.as_str(),
                rows.to_string().as_str(),
                columns.to_string().as_str(),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        let Ok(child) = spawned else {
            gpui_record_zmx_refresh_launch_failure(rows, columns, reason);
            return;
        };
        gpui_monitor_zmx_refresh_process(child, rows, columns, reason);
    });
}

pub(crate) fn gpui_record_zmx_refresh_launch_failure(rows: u16, columns: u16, reason: &'static str) {
    support_logs::append(
        support_logs::GpuiSupportLog::TerminalFocus,
        "gpui.zmxPersistenceViewportRefresh.ifStale",
        serde_json::json!({
            "columns": columns,
            "didLaunch": false,
            "didRequest": true,
            "reason": reason,
            "rows": rows,
        }),
    );
}

pub(crate) fn gpui_monitor_zmx_refresh_process(
    mut child: std::process::Child,
    rows: u16,
    columns: u16,
    reason: &'static str,
) {
    let deadline = Instant::now() + Duration::from_secs(1);
    let mut timed_out = false;
    let mut exit_code: i32 = -1;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                exit_code = status.code().unwrap_or(-1);
                break;
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    timed_out = true;
                    let _ = child.kill();
                    let _ = child.wait();
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(_) => break,
        }
    }
    support_logs::append(
        support_logs::GpuiSupportLog::TerminalFocus,
        "gpui.zmxPersistenceViewportRefresh.ifStale",
        serde_json::json!({
            "columns": columns,
            "didLaunch": true,
            "didRequest": true,
            "exitCode": exit_code,
            "reason": reason,
            "rows": rows,
            "timedOut": timed_out,
        }),
    );
}

pub(crate) fn gpui_file_contains_ghostex_cli_wrapper_marker(path: &Path) -> bool {
    gpui_is_marked_ghostex_wrapper_file(path)
}

pub(crate) fn gpui_current_bundle_cli_dir_for_ownership_probe() -> Option<PathBuf> {
    let executable = env::current_exe().ok()?;
    let bundle_root = find_app_bundle_root(&executable)?;
    let cli_dir = bundle_root.join("Contents/Resources/CLI");
    gpui_is_dir(&cli_dir).then_some(cli_dir)
}

pub(crate) fn gpui_marked_ghostex_wrapper_content(content: &str) -> bool {
    /*
    CDXC:GhostexRustCli 2026-07-13:
    New wrappers exec the bundled native `Resources/CLI/ghostex` binary; the
    legacy `ghostex-cli.mjs` form stays recognized so repair can replace
    wrappers written by pre-cutover app builds.
    */
    content.contains(GPUI_GHOSTEX_CLI_WRAPPER_MARKER)
        && (content.contains("ghostex-cli.mjs") || content.contains("/Resources/CLI/ghostex"))
}

pub(crate) fn gpui_is_file(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.is_file())
        .unwrap_or(false)
}

pub(crate) fn gpui_is_dir(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false)
}

pub(crate) fn gpui_is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

pub(crate) fn gpui_directory_size_bytes(path: &Path) -> u64 {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return 0;
    };
    if metadata.is_file() {
        return metadata.len();
    }
    if !metadata.is_dir() {
        return 0;
    }
    fs::read_dir(path)
        .map(|children| {
            children
                .filter_map(Result::ok)
                .map(|child| gpui_directory_size_bytes(&child.path()))
                .sum()
        })
        .unwrap_or(0)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GpuiAppModalProductState {
    pub(crate) pinned_prompts: Vec<GpuiPinnedPrompt>,
    pub(crate) scratch_pad_content: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiPinnedPrompt {
    pub(crate) content: String,
    pub(crate) created_at: String,
    pub(crate) prompt_id: String,
    pub(crate) title: String,
    pub(crate) updated_at: String,
}

