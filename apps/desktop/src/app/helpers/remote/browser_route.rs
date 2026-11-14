//! Which addresses a remote project's Browser reaches through that computer, and which it opens
//! from this one.
//!
//! CDXC:Browser 2026-11-14 DECISION:
//! User (Q3200, picked "A. Send only that computer's own and private-network addresses through
//! it ... public sites like github.com load over this Mac's network"): a remote project's Browser
//! sends only the computer's own and private-network addresses through the remote route
//! (localhost, 127.x, ::1, 10.x, 172.16-31.x, 192.168.x, the 100.64/10 tailnet range, *.local and
//! the computer's own host names); every other address opens from this computer, on every remote
//! route (SSH, Easy Connect, Windows). Measured on an Easy Connect link that day: the relay carried
//! about 12.7 KB/s, so github.com never finished loading through it, and the page load held the
//! shared link long enough for the health check to drop the computer, which closed the page.
//! The toolbar's route chip says "From this Mac" while the page is one this computer opens, so a
//! name only the remote's network knows that fails here is not mistaken for a remote failure.
//! Supersedes CDXC:Browser 2026-09-05 (every page of a remote tab through the machine's SOCKS).
//!
//! The rule decides by the address as written, never by resolving a name here: a name that is not
//! private by the rule opens from this computer.
//!
//! WHY a SOCKS proxy of the app's own instead of a PAC script: under a PAC script Chromium applies
//! its implicit bypass rules and opens localhost and 127.0.0.1 directly whatever the script says
//! (checked with headless Chromium on 2026-11-14: the remote's localhost page came back empty), so
//! the browser keeps one fixed SOCKS proxy with loopback included and this rule runs inside that
//! proxy (`browser_tunnel/tcp_proxy.rs`).

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use url::Url;

use crate::app::helpers::*;
use crate::shared_settings;

/// Name endings that never name a public site: loopback (`.localhost`), multicast DNS (`.local`)
/// and the names reserved for private networks.
const PRIVATE_NAME_SUFFIXES: &[&str] = &[".localhost", ".local", ".internal", ".home.arpa"];

/// The addresses a remote computer's Browser tabs reach through that computer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RemoteBrowserRoute {
    /// The computer's own host names (lower case): its SSH host when that is a name, and its
    /// saved name.
    machine_hosts: Vec<String>,
}

impl RemoteBrowserRoute {
    pub(crate) fn new(machine_hosts: impl IntoIterator<Item = String>) -> Self {
        let mut hosts: Vec<String> = machine_hosts
            .into_iter()
            .map(|host| normalize_host(&host))
            .filter(|host| !host.is_empty() && !host.contains(char::is_whitespace))
            .collect();
        hosts.sort();
        hosts.dedup();
        Self {
            machine_hosts: hosts,
        }
    }

    /// Whether `host` (a URL host or a SOCKS destination, IPv6 with or without brackets) is
    /// reached through the remote computer.
    pub(crate) fn host_goes_through_machine(&self, host: &str) -> bool {
        let host = normalize_host(host);
        if host.is_empty() {
            return false;
        }
        if let Ok(ip) = host.parse::<IpAddr>() {
            return ip_is_private(ip);
        }
        // A plain name (no dot) is the computer itself or a name of its own network.
        !host.contains('.')
            || host == "localhost"
            || PRIVATE_NAME_SUFFIXES
                .iter()
                .any(|suffix| host.ends_with(suffix))
            || self.machine_hosts.iter().any(|name| *name == host)
    }

    /// Whether `url`'s page loads through the remote computer: `None` for an address without a
    /// network host (about:blank, data:, an empty New Tab).
    pub(crate) fn url_goes_through_machine(&self, url: &str) -> Option<bool> {
        let parsed = Url::parse(url.trim()).ok()?;
        if !matches!(parsed.scheme(), "http" | "https" | "ws" | "wss") {
            return None;
        }
        Some(self.host_goes_through_machine(parsed.host_str()?))
    }
}

/// The route of the saved remote machine `remote_machine_id`, from Settings: its SSH host (an
/// Easy Connect machine's is this computer's loopback forwarder, which says nothing about it) and
/// its saved name.
pub(crate) fn gpui_remote_browser_route_for_machine(remote_machine_id: &str) -> RemoteBrowserRoute {
    let settings = shared_settings::shared_sidebar_settings_snapshot();
    let machine = settings
        .object()
        .get("remoteMachines")
        .and_then(serde_json::Value::as_array)
        .and_then(|machines| {
            machines.iter().find_map(|machine| {
                (gpui_remote_machine_id_from_value(machine).as_deref() == Some(remote_machine_id))
                    .then(|| machine.as_object().cloned())
                    .flatten()
            })
        });
    let Some(machine) = machine else {
        return RemoteBrowserRoute::default();
    };
    RemoteBrowserRoute::new(
        ["sshHost", "name"]
            .into_iter()
            .filter_map(|key| gpui_remote_machine_string_field(&machine, key)),
    )
}

fn normalize_host(host: &str) -> String {
    host.trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim_end_matches('.')
        .to_ascii_lowercase()
}

fn ip_is_private(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ipv4_is_private(ip),
        IpAddr::V6(ip) => {
            if let Some(mapped) = ip.to_ipv4_mapped() {
                return ipv4_is_private(mapped);
            }
            let first = ip.segments()[0];
            ip == Ipv6Addr::LOCALHOST
                || ip.is_unspecified()
                // Unique local (fc00::/7, the IPv6 private range; a tailnet's fd7a:115c:a1e0::/48).
                || (first & 0xfe00) == 0xfc00
                // Link-local (fe80::/10).
                || (first & 0xffc0) == 0xfe80
        }
    }
}

fn ipv4_is_private(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_private()
        || ip.is_link_local()
        // The shared address space (100.64.0.0/10) that tailnets use.
        || (a == 100 && (64..=127).contains(&b))
}
