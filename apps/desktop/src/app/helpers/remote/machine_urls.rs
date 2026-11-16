//! Where an address a remote machine's session hands this computer opens.
use crate::app::helpers::*;
use crate::*;
use gpui::http_client::Url;

/// How this computer reaches an address a remote machine's session printed.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum GpuiRemoteMachineUrl {
    /// A route of the machine's own gxserver (`http://127.0.0.1:<its port>/…`, such as an agent's
    /// `ghostex show` page), rewritten onto this computer's tunnel to that gxserver.
    Gxserver(String),
    /// Another service on the machine's loopback (a dev server, a `*.localhost` name): only the
    /// machine's own network reaches it, which is the Browser's remote context.
    MachineLoopback,
    /// Not the machine's loopback: the address opens as written.
    AsWritten,
}

/// The line the Windows token read prints when that computer's gxserver moved off 58746
/// (`gpui_remote_windows_token_read_command`).
pub(crate) const GPUI_REMOTE_WINDOWS_GXSERVER_PORT_MARKER: &str =
    "__GHOSTEX_REMOTE_GXSERVER_PORT__";

/// The port each native Windows machine's gxserver reported on its last connect, by machine id.
static GPUI_REMOTE_WINDOWS_GXSERVER_PORTS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<String, u16>>,
> = std::sync::LazyLock::new(Default::default);

/// CDXC:ServerDaemon 2026-11-17 WHY:
/// A Windows computer's gxserver leaves 58746 while Windows reserves it (GitHub #639,
/// packages/paths/src/windows_gxserver_port.rs) and records the port it moved to. The token read
/// that every connect runs prints that record, so the tunnel's far end and the machine's own links
/// follow it; a computer that prints none (not moved, or a Ghostex from before the move) is on 58746.
pub(crate) fn gpui_remote_note_windows_gxserver_port(machine_id: &str, token_read_stdout: &str) {
    let port = token_read_stdout.lines().find_map(|line| {
        line.trim()
            .strip_prefix(GPUI_REMOTE_WINDOWS_GXSERVER_PORT_MARKER)?
            .trim()
            .parse::<u16>()
            .ok()
            .filter(|port| *port != 0)
    });
    let mut ports = GPUI_REMOTE_WINDOWS_GXSERVER_PORTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    match port {
        Some(port) => ports.insert(machine_id.to_string(), port),
        None => ports.remove(machine_id),
    };
}

/// The port the machine's gxserver listens on, on its own loopback: the far end of the SSH tunnel
/// every request for the machine goes through. Native Windows and WSL can run side by side, so
/// their ports differ.
pub(crate) fn gpui_remote_gxserver_port(
    machine_id: &str,
    execution_target: &GpuiRemoteExecutionTarget,
) -> u16 {
    match execution_target {
        GpuiRemoteExecutionTarget::WindowsPowerShell => GPUI_REMOTE_WINDOWS_GXSERVER_PORTS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(machine_id)
            .copied()
            .unwrap_or(58_746),
        GpuiRemoteExecutionTarget::PosixHost | GpuiRemoteExecutionTarget::WindowsWsl { .. } => {
            GPUI_GXSERVER_LOCAL_API_PORT
        }
    }
}

/// CDXC:RemoteMachines 2026-10-16 DECISION:
/// User: "i want a fix please so it uses the tailscale or the easy connect instead of trying to open 127.0.0.1 when i open inline html in a remote machine ... check other similar issues in the app and fix them please". A loopback address a remote machine's session prints names that machine, never this computer. A route of its own gxserver (an agent's `ghostex show` page, which posts its answers and draft back to the same origin) opens through the tunnel this computer already holds to that gxserver, the one every other request for the machine uses whether it is reached over Tailscale, Easy Connect or plain SSH. Any other loopback port opens in the Browser on the machine's own network (`try_open_remote_loopback_link`). Nothing tries this computer's 127.0.0.1 first.
/// SEE-ALSO: apps/mobile/app/src/webPreview/ (the phone's routing of the same addresses), server/src/visual_pages/serve.rs (the page's content policy follows the Host it was asked under, so the tunnel's address keeps its answers allowed).
pub(crate) fn gpui_remote_machine_url(
    url: &str,
    target: &GpuiRemoteGxserverRequestTarget,
) -> GpuiRemoteMachineUrl {
    if !gpui_remote_link_is_loopback(url) {
        return GpuiRemoteMachineUrl::AsWritten;
    }
    let Some(mut parsed) = normalize_address(url).and_then(|url| Url::parse(&url).ok()) else {
        return GpuiRemoteMachineUrl::AsWritten;
    };
    let host = parsed
        .host_str()
        .unwrap_or_default()
        .trim_end_matches('.')
        .trim_matches(['[', ']'])
        .to_ascii_lowercase();
    // `*.localhost` names are the machine's routed services (portless), never gxserver itself.
    let gxserver_host = host == "localhost"
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback() || ip.is_unspecified());
    if parsed.scheme() != "http"
        || !gxserver_host
        || parsed.port_or_known_default()
            != Some(gpui_remote_gxserver_port(
                &target.machine_id,
                &target.execution_target,
            ))
    {
        return GpuiRemoteMachineUrl::MachineLoopback;
    }
    if parsed.set_host(Some("127.0.0.1")).is_err()
        || parsed.set_port(Some(target.local_port)).is_err()
    {
        return GpuiRemoteMachineUrl::MachineLoopback;
    }
    GpuiRemoteMachineUrl::Gxserver(parsed.to_string())
}
