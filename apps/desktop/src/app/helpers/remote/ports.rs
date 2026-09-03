// C1 wave-1 deferred split: apps/desktop/src/app/helpers/remote.rs (~8.2k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds the remote listening-ports discovery
// and the local browser page rendered for them. See
// docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::{fs, time::Duration};

use crate::app::helpers::*;
use crate::*;

/*
CDXC:GPUIRemotePortsBrowser 2026-07-30:
The remote Browser default page lists the machine's listening TCP sockets so a
locally running app there is one click away. Discovery runs over the saved SSH
configuration (`ss` on Linux/WSL remotes, `netstat` elsewhere) — no gxserver
endpoint and no renderer-supplied hosts, ports, paths, or command text. The
rendered page is written under the local state directory and only ever
contains the saved ssh host plus parsed port/address/process fields.
*/
pub(crate) const GPUI_REMOTE_LISTENING_PORTS_COMMAND: &str = "if command -v ss >/dev/null 2>&1; then ss -tlnp 2>/dev/null || ss -tln; else netstat -an 2>/dev/null | grep LISTEN; fi";

pub(crate) struct GpuiRemoteListeningPort {
    pub(crate) address: String,
    pub(crate) port: u16,
    pub(crate) process: Option<String>,
    pub(crate) remotely_reachable: bool,
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_prepare_remote_ports_browser_page(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
) -> Result<String, String> {
    let result = gpui_run_remote_ssh_in_execution_target(
        config,
        execution_target,
        GPUI_REMOTE_LISTENING_PORTS_COMMAND,
        Duration::from_secs(12),
    );
    if result.exit_code != 0 {
        return Err("Listing the remote machine's ports over SSH failed.".to_string());
    }
    let mut ports = gpui_parse_remote_listening_ports(result.stdout.as_str());
    /*
    CDXC:RemotePairing 2026-09-03:
    Over Easy Connect the saved host is this app's loopback forwarder, so a
    `http://127.0.0.1:<port>/` link would open the local computer, not the
    remote one. No port on such a machine is directly reachable from here;
    every one is listed as reach-via-port-forward, under the machine's name.
    */
    let via_easy_connect = config.uses_easy_connect();
    let page_host = if via_easy_connect {
        for entry in &mut ports {
            entry.remotely_reachable = false;
        }
        gpui_remote_machine_name_from_settings(config.remote_machine_id.as_str())
            .unwrap_or_else(|| "Easy Connect machine".to_string())
    } else {
        config.ssh_host.trim().to_string()
    };
    let html = gpui_remote_ports_page_html(page_host.as_str(), &ports, via_easy_connect);
    let directory = shared_settings::ghostex_storage_paths()
        .state_dir
        .join("remote-ports");
    fs::create_dir_all(&directory)
        .map_err(|_| "Could not prepare the remote ports page.".to_string())?;
    let path = directory.join(format!("{}.html", config.remote_machine_id));
    fs::write(&path, html).map_err(|_| "Could not write the remote ports page.".to_string())?;
    Ok(format!("file://{}", gpui_path_string(&path)))
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn gpui_prepare_remote_ports_browser_page(
    _config: &GpuiRemoteMachineConfig,
    _execution_target: &GpuiRemoteExecutionTarget,
) -> Result<String, String> {
    Err("Remote port discovery is unavailable on this platform.".to_string())
}

pub(crate) fn gpui_parse_remote_listening_ports(stdout: &str) -> Vec<GpuiRemoteListeningPort> {
    let mut by_port: std::collections::BTreeMap<u16, GpuiRemoteListeningPort> =
        std::collections::BTreeMap::new();
    for line in stdout.lines() {
        let tokens = line.split_whitespace().collect::<Vec<_>>();
        if tokens.len() < 4 {
            continue;
        }
        let (local, process) = if tokens[0].eq_ignore_ascii_case("LISTEN") {
            // ss: State Recv-Q Send-Q Local:Port Peer [users:(("name",…))]
            (tokens[3], gpui_remote_ss_process_name(line))
        } else if tokens[0].starts_with("tcp")
            && tokens
                .last()
                .is_some_and(|token| token.eq_ignore_ascii_case("LISTEN"))
        {
            // netstat: Proto Recv-Q Send-Q Local Foreign State
            (tokens[3], None)
        } else {
            continue;
        };
        let Some((address, port)) = gpui_remote_split_listening_local_address(local) else {
            continue;
        };
        let remotely_reachable = gpui_remote_listen_address_is_remotely_reachable(address.as_str());
        match by_port.get_mut(&port) {
            Some(existing) => {
                if remotely_reachable && !existing.remotely_reachable {
                    existing.address = address;
                    existing.remotely_reachable = true;
                }
                if existing.process.is_none() {
                    existing.process = process;
                }
            }
            None => {
                by_port.insert(
                    port,
                    GpuiRemoteListeningPort {
                        address,
                        port,
                        process,
                        remotely_reachable,
                    },
                );
            }
        }
    }
    by_port.into_values().collect()
}

pub(crate) fn gpui_remote_split_listening_local_address(value: &str) -> Option<(String, u16)> {
    // ss prints `address:port` (`[::]:80`, `*:80`); macOS netstat prints
    // `address.port` (`127.0.0.1.58744`, `*.58744`).
    let (address, port) = value.rsplit_once(':').or_else(|| value.rsplit_once('.'))?;
    let port = port.parse::<u16>().ok()?;
    let address = address.trim_matches(['[', ']']).to_string();
    Some((address, port))
}

pub(crate) fn gpui_remote_listen_address_is_remotely_reachable(address: &str) -> bool {
    let normalized = address.trim().to_ascii_lowercase();
    !(normalized.starts_with("127.") || normalized == "::1" || normalized == "localhost")
}

pub(crate) fn gpui_remote_ss_process_name(line: &str) -> Option<String> {
    let marker = "users:((\"";
    let start = line.find(marker)? + marker.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    let name = rest[..end].trim();
    (!name.is_empty()).then(|| name.to_string())
}

pub(crate) fn gpui_remote_ports_page_html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub(crate) fn gpui_remote_ports_link_host(ssh_host: &str) -> String {
    if ssh_host.contains(':') {
        format!("[{ssh_host}]")
    } else {
        ssh_host.to_string()
    }
}

pub(crate) fn gpui_remote_ports_page_html(
    ssh_host: &str,
    ports: &[GpuiRemoteListeningPort],
    via_easy_connect: bool,
) -> String {
    let escaped_host = gpui_remote_ports_page_html_escape(ssh_host);
    let link_host = gpui_remote_ports_page_html_escape(&gpui_remote_ports_link_host(ssh_host));
    let mut reachable_rows = String::new();
    let mut loopback_rows = String::new();
    for entry in ports {
        let process = entry
            .process
            .as_deref()
            .map(gpui_remote_ports_page_html_escape)
            .unwrap_or_default();
        let address = gpui_remote_ports_page_html_escape(entry.address.as_str());
        if entry.remotely_reachable {
            reachable_rows.push_str(&format!(
                "<a class=\"row\" href=\"http://{link_host}:{port}/\">\
                 <span class=\"port\">{port}</span>\
                 <span class=\"process\">{process}</span>\
                 <span class=\"address\">{address}</span>\
                 <span class=\"go\">http://{link_host}:{port}/ →</span></a>\n",
                port = entry.port,
            ));
        } else {
            loopback_rows.push_str(&format!(
                "<div class=\"row loopback\">\
                 <span class=\"port\">{port}</span>\
                 <span class=\"process\">{process}</span>\
                 <span class=\"address\">{address}</span>\
                 <span class=\"go\">localhost-only on the remote</span></div>\n",
                port = entry.port,
            ));
        }
    }
    if reachable_rows.is_empty() {
        reachable_rows =
            "<div class=\"empty\">No externally reachable listening TCP ports were found.</div>\n"
                .to_string();
    }
    let loopback_section = if loopback_rows.is_empty() {
        String::new()
    } else if via_easy_connect {
        format!(
            "<h2>Listening on the remote</h2>\
             <p class=\"hint\">This machine is reached through Easy Connect, so none of its ports can be opened directly from here; reach them with an SSH port forward.</p>\n{loopback_rows}"
        )
    } else {
        format!(
            "<h2>Bound to localhost on the remote</h2>\
             <p class=\"hint\">These only answer on the remote machine itself; reach them with an SSH port forward.</p>\n{loopback_rows}"
        )
    };
    format!(
        "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\">\
         <title>Ports on {escaped_host}</title>\
         <style>\
         body{{margin:0;padding:32px;background:#101418;color:#d7dde3;font:14px/1.5 -apple-system,'Segoe UI',sans-serif;}}\
         h1{{font-size:20px;margin:0 0 4px;}}\
         h2{{font-size:14px;margin:28px 0 4px;color:#9aa7b2;}}\
         p.hint{{margin:0 0 12px;color:#77828c;font-size:12px;}}\
         .row{{display:flex;align-items:baseline;gap:16px;padding:10px 14px;margin:6px 0;border-radius:8px;background:#1a2027;text-decoration:none;color:inherit;}}\
         a.row:hover{{background:#232b35;}}\
         .port{{font-size:16px;font-weight:600;min-width:64px;color:#e8eef4;}}\
         .process{{min-width:160px;color:#a9c7e8;}}\
         .address{{min-width:120px;color:#77828c;font-family:ui-monospace,monospace;font-size:12px;}}\
         .go{{margin-left:auto;color:#6fa8dc;font-family:ui-monospace,monospace;font-size:12px;}}\
         .loopback{{opacity:.55;}}\
         .empty{{padding:24px;color:#77828c;}}\
         </style></head><body>\
         <h1>Ports on {escaped_host}</h1>\
         <p class=\"hint\">Click a port to open the app running on the remote machine. Click the project's Browser button again to refresh this list.</p>\n\
         {reachable_rows}{loopback_section}</body></html>"
    )
}
