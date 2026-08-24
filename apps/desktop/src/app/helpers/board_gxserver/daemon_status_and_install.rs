// C1 wave-1 deferred split: apps/desktop/src/app/helpers/board_gxserver.rs
// (~4.3k lines) further divided into responsibility-scoped submodules (pure
// move, no logic changes). This file holds gxserver daemon-status titlebar reporting plus the macOS
// on-demand remote gxserver archive download/install helpers.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::path::{Path, PathBuf};

use futures::channel::mpsc;

use crate::app::helpers::*;
use crate::*;

pub(crate) fn gpui_titlebar_gxserver_daemon_status() -> serde_json::Value {
    match gpui_probe_local_gxserver_health() {
        GpuiLocalGxserverHealthState::Healthy { tools_available } => {
            let mut status = serde_json::json!({
                "ok": tools_available,
                "state": "running",
            });
            if !tools_available {
                status["message"] =
                    serde_json::json!("gxserver is running, but zmx/bd are unavailable.");
            }
            status
        }
        GpuiLocalGxserverHealthState::ProtocolMismatch { reported } => serde_json::json!({
            "message": gpui_gxserver_protocol_mismatch_message(reported),
            "ok": false,
            "state": "protocolMismatch",
        }),
        GpuiLocalGxserverHealthState::BuildMismatch => serde_json::json!({
            "message": "gxserver belongs to a different Ghostex build and must be restarted.",
            "ok": false,
            "state": "buildMismatch",
        }),
        GpuiLocalGxserverHealthState::Unreachable => serde_json::json!({
            "ok": false,
            "state": "stopped",
        }),
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_on_demand_gxserver_asset_key(
    target: &GpuiRemoteInstallTarget,
) -> Option<&'static str> {
    if target.normalized_os() != "linux" {
        return None;
    }
    match target.normalized_arch().as_str() {
        "x64" => Some("gxserver-linux-x64"),
        "arm64" => Some("gxserver-linux-arm64"),
        _ => None,
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_on_demand_gxserver_archive(
    target: &GpuiRemoteInstallTarget,
    progress_tx: Option<&mpsc::UnboundedSender<GpuiRemoteGxserverConnectProgress>>,
) -> Result<PathBuf, GpuiOnDemandArchiveFailure> {
    let Some(asset_key) = gpui_on_demand_gxserver_asset_key(target) else {
        return Err(GpuiOnDemandArchiveFailure {
            message: gpui_unsupported_remote_package_message(target),
            state: GpuiRemoteGxserverConnectState::UnsupportedRemotePlatform,
        });
    };
    let Some(resources_dir) = gpui_app_bundle_resources_dir() else {
        return Err(GpuiOnDemandArchiveFailure {
            message: "Could not locate the app's sealed on-demand resource manifest.".to_string(),
            state: GpuiRemoteGxserverConnectState::InstallFailed,
        });
    };
    let manifest_path = resources_dir.join("Web/on-demand-resources.json");
    let manifest = component_store::OnDemandManifest::load(&manifest_path).map_err(|message| {
        GpuiOnDemandArchiveFailure {
            message,
            state: GpuiRemoteGxserverConnectState::InstallFailed,
        }
    })?;
    if !manifest.assets.contains_key(asset_key) {
        return Err(GpuiOnDemandArchiveFailure {
            message: gpui_unsupported_remote_package_message(target),
            state: GpuiRemoteGxserverConnectState::UnsupportedRemotePlatform,
        });
    }
    let store = component_store::ComponentStore::from_manifest(manifest).map_err(|message| {
        GpuiOnDemandArchiveFailure {
            message,
            state: GpuiRemoteGxserverConnectState::InstallFailed,
        }
    })?;
    let mut report_progress = |event: component_store::ComponentStoreProgress| {
        if matches!(
            event.phase,
            component_store::ComponentStoreProgressPhase::Downloading
        ) {
            if let Some(progress_tx) = progress_tx {
                let _ = progress_tx.unbounded_send(GpuiRemoteGxserverConnectProgress {
                    state: GpuiRemoteGxserverConnectState::DownloadingRemoteServerPackage,
                });
            }
        }
        support_logs::append(
            support_logs::GpuiSupportLog::RemoteGxserverInstall,
            "gpui.remoteGxserver.install.onDemand.progress",
            serde_json::json!({
                "asset": event.component,
                "assetBytes": event.size_bytes,
                "phase": event.phase.as_str(),
            }),
        );
    };
    store
        .download_release_asset(asset_key, &mut report_progress)
        .map_err(|message| GpuiOnDemandArchiveFailure {
            message,
            state: GpuiRemoteGxserverConnectState::InstallFailed,
        })
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_install_gxserver_archive_and_read_token(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
    archive_path: &Path,
) -> GpuiRemoteProcessResult {
    // Stream the archive through the remote shell so its GHOSTEX_HOME/XDG
    // environment determines the destination on Linux, macOS, and WSL alike.
    let upload_result = gpui_run_remote_ssh_with_stdin_file_in_execution_target(
        config,
        execution_target,
        concat!(
            "set -eu; umask 077; ",
            "case \"${GHOSTEX_HOME:-}\" in /*) ghostex_data_dir=\"$GHOSTEX_HOME\";; ",
            "*) case \"${XDG_DATA_HOME:-}\" in /*) ghostex_data_dir=\"${XDG_DATA_HOME%/}/ghostex\";; *) ghostex_data_dir=\"$HOME/.local/share/ghostex\";; esac;; esac; ",
            "install_root=\"$ghostex_data_dir/gxserver\"; mkdir -p \"$install_root\"; ",
            "cat > \"$install_root/gxserver-upload.tar.gz\""
        ),
        archive_path,
        GPUI_REMOTE_GXSERVER_UPLOAD_TIMEOUT,
    );
    if upload_result.exit_code != 0 {
        return GpuiRemoteProcessResult {
            exit_code: upload_result.exit_code,
            stderr: "Could not upload gxserver package over SSH.".to_string(),
            stdout: String::new(),
        };
    }
    let release_uuid = match gpui_random_uuid_string() {
        Ok(value) => value,
        Err(_) => {
            return GpuiRemoteProcessResult {
                exit_code: 126,
                stderr: "Could not prepare gxserver install release id.".to_string(),
                stdout: String::new(),
            };
        }
    };
    let release_id = format!("release-{release_uuid}");
    let install_command = gpui_remote_gxserver_install_command(release_id.as_str());
    gpui_run_remote_ssh_in_execution_target(
        config,
        execution_target,
        install_command.as_str(),
        GPUI_REMOTE_GXSERVER_INSTALL_TIMEOUT,
    )
}
