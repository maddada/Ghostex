//! The app's settings file copied into the WSL distribution, where the WSL gxserver and the
//! `ghostex` CLI there read it.
//!
//! CDXC:Settings 2026-11-26 WHY:
//! GitHub #732: GBots turned on in the Windows app stayed off for the WSL gxserver (`/api/gbots` answered 409, `ghostex settings get gbotsEnabled` in WSL said false). The app saves `%APPDATA%\Ghostex\native-sidebar-settings.json`, while the WSL gxserver and CLI read `native-sidebar-settings.json` in the distribution's own Ghostex config folder, which nothing wrote, so every setting gxserver reads (extension switches, telemetry, auto sleep, hooks) stayed at its default there. On macOS, Linux and native Windows the app and gxserver share one file; here the app stays its one writer and copies the whole file in before it starts the WSL gxserver and after every save it makes, and gxserver's settings watch picks the copy up. Pointing the WSL gxserver at the Windows file instead would leave the CLI in a plain WSL shell reading its own empty file.
//! SEE-ALSO: apps/desktop/src/shared_settings/service.rs (`observe_settings_saved`), server/src/shared_settings_file.rs (the reader), server/src/headless_settings.rs (stops saving once the file is the app's copy), packages/paths/src/lib.rs (the config folder this script resolves).

use std::{io::Write as _, process::Stdio, sync::Mutex};

use super::*;

/// Resolves the config folder the way Linux gxserver does (`GhostexPaths::resolve`: `GHOSTEX_HOME`,
/// else `XDG_CONFIG_HOME/ghostex`, else `~/.config/ghostex`) and replaces the settings file there
/// with standard input, through a temporary file so a reader never sees half of it. An unchanged
/// file is left alone so gxserver's watch does not fire for nothing.
const MIRROR_SCRIPT: &str = r#"set -eu
case "${GHOSTEX_HOME:-}" in
    /*) config_dir="$GHOSTEX_HOME" ;;
    *)
        case "${XDG_CONFIG_HOME:-}" in
            /*) config_dir="${XDG_CONFIG_HOME%/}/ghostex" ;;
            *) config_dir="$HOME/.config/ghostex" ;;
        esac
        ;;
esac
umask 077
mkdir -p "$config_dir"
target="$config_dir/native-sidebar-settings.json"
incoming="$target.windows-app.$$"
trap 'rm -f "$incoming"' EXIT
cat > "$incoming"
if [ -f "$target" ] && cmp -s "$incoming" "$target"; then
    exit 0
fi
mv -f "$incoming" "$target"
"#;

/// Copies the app's settings file into `distribution`. Nothing is copied while the app has no
/// settings file yet: gxserver then reads its defaults, as the app does.
pub(in crate::windows_terminal_backend) fn mirror_settings_into(
    distribution: &str,
) -> Result<(), String> {
    let path = crate::shared_settings::shared_sidebar_settings_path();
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("Could not read the settings file: {error}")),
    };
    let mut child = hidden_command("wsl.exe")
        .args([
            "--distribution",
            distribution,
            "--exec",
            "sh",
            "-lc",
            MIRROR_SCRIPT,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Could not run wsl.exe: {error}"))?;
    let written = child
        .stdin
        .take()
        .ok_or_else(|| "wsl.exe gave no standard input".to_string())
        .and_then(|mut stdin| {
            stdin
                .write_all(&bytes)
                .map_err(|error| format!("Could not send the settings to WSL: {error}"))
        });
    let output = child
        .wait_with_output()
        .map_err(|error| format!("wsl.exe did not finish: {error}"))?;
    written?;
    if output.status.success() {
        return Ok(());
    }
    let detail = String::from_utf8_lossy(&output.stderr)
        .trim()
        .chars()
        .take(1024)
        .collect::<String>();
    Err(format!(
        "Could not save the settings in WSL ({}): {detail}",
        output.status
    ))
}

/// Saves made while a copy is running, so a burst of saves ends in one more copy of the latest
/// file instead of one copy each.
struct MirrorQueue {
    running: bool,
    pending: bool,
}

static QUEUE: Mutex<MirrorQueue> = Mutex::new(MirrorQueue {
    running: false,
    pending: false,
});

/// The settings service's save hook (`observe_settings_saved`): copies the saved file into the WSL
/// distribution in the background, since each copy runs `wsl.exe` and a save must not wait on it.
pub(crate) fn settings_file_saved() {
    {
        let mut queue = QUEUE.lock().unwrap_or_else(|error| error.into_inner());
        queue.pending = true;
        if queue.running {
            return;
        }
        queue.running = true;
    }
    let spawned = std::thread::Builder::new()
        .name("ghostex-wsl-settings-mirror".to_string())
        .spawn(|| {
            loop {
                {
                    let mut queue = QUEUE.lock().unwrap_or_else(|error| error.into_inner());
                    if !queue.pending {
                        queue.running = false;
                        return;
                    }
                    queue.pending = false;
                }
                mirror_into_current_distribution();
            }
        });
    if spawned.is_err() {
        QUEUE
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .running = false;
    }
}

/// Copies into the distribution the WSL gxserver runs in. Before the backend has picked one there
/// is nothing to copy into: `prepare_gxserver` copies the file before it starts gxserver.
fn mirror_into_current_distribution() {
    if super::current_preference() != WindowsTerminalBackendPreference::Wsl {
        return;
    }
    let Some(distribution) = state()
        .lock()
        .ok()
        .and_then(|state| state.distribution.clone())
    else {
        return;
    };
    if let Err(error) = mirror_settings_into(&distribution) {
        log::warn!("WSL settings copy failed: {error}");
    }
}
