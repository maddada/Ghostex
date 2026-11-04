// Records of the SSH tunnels this app spawns, so the next launch can stop the ones a crashed or
// force-quit Ghostex left running.

use std::{fs, path::PathBuf};

use crate::app::helpers::*;

/// CDXC:RemoteMachines 2026-10-26 WHY:
/// A tunnel `ssh -N` is the app's own child, stopped when its connection ends, when its window closes and on quit. When Ghostex dies without running that code (a crash, or `cargo xtask start` escalating a slow quit to SIGTERM/SIGKILL), the child is reparented and keeps its forward and its login to the remote machine open forever: five such tunnels from Oct 25 were still connected to the user's Windows PC a day later. Each spawned tunnel is recorded here with its owner's pid and its exact forward, and the next launch stops a recorded tunnel only when that process still carries the recorded forward and its parent is no longer the Ghostex that started it (a parent only changes when it dies). A live Ghostex's tunnels, and any ssh nobody recorded, are never touched.
/// SEE-ALSO: tunnel_and_auth.rs and browser_tunnel.rs (record and forget), main.rs (the launch reap).
fn gpui_remote_tunnel_registry_dir() -> PathBuf {
    ghostex_state_root().join("remote-tunnels")
}

fn gpui_remote_tunnel_record_path(pid: u32) -> PathBuf {
    gpui_remote_tunnel_registry_dir().join(format!("{pid}.json"))
}

/// `forward` is the `-L`/`-D` argument exactly as passed to ssh. Only Unix launches reap, so only
/// Unix records.
pub(crate) fn gpui_record_remote_tunnel(pid: u32, forward: &str) {
    if cfg!(not(unix)) {
        return;
    }
    if fs::create_dir_all(gpui_remote_tunnel_registry_dir()).is_err() {
        return;
    }
    let record = serde_json::json!({
        "pid": pid,
        "ownerPid": std::process::id(),
        "forward": forward,
    });
    let _ = fs::write(gpui_remote_tunnel_record_path(pid), record.to_string());
}

pub(crate) fn gpui_forget_remote_tunnel(pid: u32) {
    let _ = fs::remove_file(gpui_remote_tunnel_record_path(pid));
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum GpuiRemoteTunnelRecordFate {
    /// A running Ghostex still owns it.
    Keep,
    /// The process is gone, or the pid now belongs to something else.
    Forget,
    /// The recorded tunnel outlived the Ghostex that started it.
    Stop,
}

/// `ps_line` is `ps -o ppid= -o command=` for the recorded pid, `None` when no such process exists.
pub(crate) fn gpui_remote_tunnel_record_fate(
    owner_pid: u32,
    current_pid: u32,
    forward: &str,
    ps_line: Option<&str>,
) -> GpuiRemoteTunnelRecordFate {
    if owner_pid == current_pid {
        return GpuiRemoteTunnelRecordFate::Keep;
    }
    let Some((parent, command)) =
        ps_line.and_then(|line| line.trim().split_once(char::is_whitespace))
    else {
        return GpuiRemoteTunnelRecordFate::Forget;
    };
    if parent.trim().parse::<u32>().ok() == Some(owner_pid) {
        return GpuiRemoteTunnelRecordFate::Keep;
    }
    let mut arguments = command.split_whitespace();
    let is_ssh = arguments
        .next()
        .is_some_and(|executable| executable.ends_with("/ssh") || executable == "ssh");
    let arguments = arguments.collect::<Vec<_>>();
    // A WSL tunnel carries its owner command instead of `-N`, so match the forward itself.
    let carries_forward = arguments
        .windows(2)
        .any(|pair| matches!(pair[0], "-L" | "-D") && pair[1] == forward);
    if is_ssh && !forward.is_empty() && carries_forward {
        GpuiRemoteTunnelRecordFate::Stop
    } else {
        GpuiRemoteTunnelRecordFate::Forget
    }
}

/// Runs once per launch, off the main thread.
pub(crate) fn gpui_reap_orphaned_remote_tunnels() {
    #[cfg(unix)]
    {
        let Ok(entries) = fs::read_dir(gpui_remote_tunnel_registry_dir()) else {
            return;
        };
        let current_pid = std::process::id();
        for entry in entries.flatten() {
            let path = entry.path();
            let record = fs::read_to_string(&path)
                .ok()
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());
            let field = |name: &str| {
                record
                    .as_ref()
                    .and_then(|record| record.get(name))
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok())
            };
            let (Some(pid), Some(owner_pid)) = (field("pid"), field("ownerPid")) else {
                let _ = fs::remove_file(&path);
                continue;
            };
            let forward = record
                .as_ref()
                .and_then(|record| record.get("forward"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let ps_line = gpui_remote_tunnel_ps_line(pid);
            match gpui_remote_tunnel_record_fate(
                owner_pid,
                current_pid,
                forward,
                ps_line.as_deref(),
            ) {
                GpuiRemoteTunnelRecordFate::Keep => {}
                GpuiRemoteTunnelRecordFate::Forget => {
                    let _ = fs::remove_file(&path);
                }
                GpuiRemoteTunnelRecordFate::Stop => {
                    if let Ok(pid) = i32::try_from(pid) {
                        unsafe {
                            libc::kill(pid, libc::SIGTERM);
                        }
                    }
                    let _ = fs::remove_file(&path);
                }
            }
        }
    }
}

#[cfg(unix)]
fn gpui_remote_tunnel_ps_line(pid: u32) -> Option<String> {
    let output = std::process::Command::new("/bin/ps")
        .args([
            "-ww",
            "-o",
            "ppid=",
            "-o",
            "command=",
            "-p",
            &pid.to_string(),
        ])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    let line = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (output.status.success() && !line.is_empty()).then_some(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FORWARD: &str = "58365:127.0.0.1:58746";
    const TUNNEL: &str = "/usr/bin/ssh -N -o ConnectTimeout=8 -o ExitOnForwardFailure=yes -L 58365:127.0.0.1:58746 madda@100.105.82.19";

    #[test]
    fn a_tunnel_whose_ghostex_died_is_stopped() {
        assert_eq!(
            gpui_remote_tunnel_record_fate(17059, 74421, FORWARD, Some(&format!("1 {TUNNEL}"))),
            GpuiRemoteTunnelRecordFate::Stop
        );
        // Linux hands orphans to a subreaper rather than pid 1.
        assert_eq!(
            gpui_remote_tunnel_record_fate(
                17059,
                74421,
                FORWARD,
                Some(&format!("   2210 {TUNNEL}"))
            ),
            GpuiRemoteTunnelRecordFate::Stop
        );
    }

    #[test]
    fn a_live_ghostex_keeps_its_tunnels() {
        assert_eq!(
            gpui_remote_tunnel_record_fate(74421, 74421, FORWARD, Some(&format!("74421 {TUNNEL}"))),
            GpuiRemoteTunnelRecordFate::Keep
        );
        // Another Ghostex (a dev copy) that is still running.
        assert_eq!(
            gpui_remote_tunnel_record_fate(5000, 74421, FORWARD, Some(&format!("5000 {TUNNEL}"))),
            GpuiRemoteTunnelRecordFate::Keep
        );
    }

    #[test]
    fn a_gone_or_reused_pid_only_drops_the_record() {
        assert_eq!(
            gpui_remote_tunnel_record_fate(17059, 74421, FORWARD, None),
            GpuiRemoteTunnelRecordFate::Forget
        );
        assert_eq!(
            gpui_remote_tunnel_record_fate(17059, 74421, FORWARD, Some("1 /usr/sbin/cupsd -l")),
            GpuiRemoteTunnelRecordFate::Forget
        );
        // The forward must be an ssh forward, not some other argument.
        assert_eq!(
            gpui_remote_tunnel_record_fate(
                17059,
                74421,
                FORWARD,
                Some("1 /usr/bin/ssh -N -o 58365:127.0.0.1:58746 madda@100.105.82.19")
            ),
            GpuiRemoteTunnelRecordFate::Forget
        );
        // Another ssh -N with a different forward is not ours.
        assert_eq!(
            gpui_remote_tunnel_record_fate(
                17059,
                74421,
                FORWARD,
                Some("1 /usr/bin/ssh -N -L 41000:127.0.0.1:58746 madda@100.105.82.19")
            ),
            GpuiRemoteTunnelRecordFate::Forget
        );
        assert_eq!(
            gpui_remote_tunnel_record_fate(17059, 74421, "", Some(&format!("1 {TUNNEL}"))),
            GpuiRemoteTunnelRecordFate::Forget
        );
    }

    #[test]
    fn a_browser_socks_tunnel_matches_its_dynamic_forward() {
        assert_eq!(
            gpui_remote_tunnel_record_fate(
                17059,
                74421,
                "127.0.0.1:61000",
                Some(
                    "1 /usr/bin/ssh -o ControlMaster=no -N -o ExitOnForwardFailure=yes -D 127.0.0.1:61000 me@host"
                )
            ),
            GpuiRemoteTunnelRecordFate::Stop
        );
    }
}
