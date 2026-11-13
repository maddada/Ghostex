//! Whether a phone asking to pair over its own SSH login is the owner's own Tailscale device, so
//! its Easy Connect pairing needs no click (`/api/pairOverSsh` with Easy Connect keys).
//!
//! CDXC:RemotePairing 2026-11-13 DECISION:
//! Supersedes 2026-11-12 (Q2716 = A, Q2719: "a phone that connects over Tailscale also gets Easy
//! Connect, inside the same pairing", asked by the phone at its first sign-in), after an SSH-only
//! iPhone was added this way: Easy Connect is strictly opt-in, and nothing moves a phone onto it
//! without the person's own ask; only the route they chose is used. A phone paired over SSH asks for Easy Connect only when the person taps Add Easy
//! Connect on it (capability `pairOverSshEasyConnectOnRequest`; the old `pairOverSshEasyConnect`
//! that 11.6.0 phones asked by themselves on is no longer offered), and only while Easy Connect is
//! on here (refused otherwise, `EASY_CONNECT_OFF_REFUSAL`). That tap is the person's own ask, so
//! the 2026-11-12 "no click needed when the phone is the user's own Tailscale device" still holds
//! for it: gxserver approves it by itself only when both hold: the SSH session the request came through is from a
//! tailnet address (100.64.0.0/10 or fd7a:115c:a1e0::/48) that is not this computer's own address
//! or loopback, and Tailscale's whois says that node belongs to the same Tailscale user as this
//! computer (not tagged, not shared in from another tailnet). Any check that fails or can't be
//! made asks for the usual click with the three words instead.
//!
//! CDXC:RemotePairing 2026-11-12 WHY:
//! The request reaches gxserver through the phone's SSH port forward, so gxserver sees only a
//! loopback client, and the token file it carries is the same one an agent on this computer can
//! read. The source address is therefore never taken from the request or the environment
//! (`SSH_CONNECTION` is the caller's to set); it is traced in the kernel's socket table: the
//! process holding the other end of this very loopback connection must be the SSH server's
//! per-login process (`sshd-session` or `sshd`, a root-owned binary, started by a root process),
//! and that process's one established tailnet socket names the phone. An agent here can't make
//! sshd hold a forward for it except by signing in itself, and then the source is this computer's
//! own address or loopback, both refused. Tailscale is asked only through root-owned binaries, so
//! a user-writable `tailscale` earlier on PATH can't answer whois. This works on macOS, where the
//! per-login sshd process runs as the user and `lsof` sees its sockets; Linux hides a non-dumpable
//! sshd's descriptors from the user, Tailscale SSH holds the connection in tailscaled itself, and
//! Windows has no such trace here, so those ask for the click. A process running as the user can
//! already write `authorized_keys` and gxserver's database, so it is outside what this guards.
//! SEE-ALSO: server/src/remote_access/pair_device.rs (`begin_ssh_pairing`, `pairing_status`),
//! apps/mobile/app/src/machines/sshPairing.ts (the phone's side).

use std::{
    net::{IpAddr, SocketAddr},
    path::Path,
    process::Stdio,
};

use serde_json::Value;

/// What the check decided.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TailnetOwnerCheck {
    /// The request came through an SSH login from the owner's own tailnet device.
    Owned {
        peer_ip: IpAddr,
        /// The node's tailnet name, for the security log.
        node_name: String,
    },
    /// Ask for the click; `reason` says why, for the security log and diagnostics.
    Ask { reason: String },
}

impl TailnetOwnerCheck {
    fn ask(reason: impl Into<String>) -> Self {
        Self::Ask {
            reason: reason.into(),
        }
    }
}

/// One process in `lsof -F pcn` output: its pid, command name and the TCP endpoints it holds
/// (`local`, `remote`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LsofProcess {
    pub pid: u32,
    pub command: String,
    pub connections: Vec<(SocketAddr, SocketAddr)>,
    /// `txt` file names (the executable first).
    pub files: Vec<String>,
}

/// What the check needs from the operating system and Tailscale; [`SystemProbe`] is the real one.
pub trait TailnetProbe {
    /// `lsof -F pcfn` of every process with an established TCP connection on `port`.
    fn processes_on_port(&self, port: u16) -> Result<String, String>;
    /// `lsof -F pcfn` of `pid`'s established TCP connections and its executable.
    fn process_files(&self, pid: u32) -> Result<String, String>;
    /// Whether `path` is owned by root and not writable by anyone else.
    fn root_owned(&self, path: &Path) -> bool;
    /// The uid of `pid`'s parent process.
    fn parent_uid(&self, pid: u32) -> Result<u32, String>;
    /// `tailscale status --json` and `tailscale whois --json <ip>` from the same root-owned CLI
    /// whose backend is running.
    fn tailscale(&self, peer_ip: IpAddr) -> Result<(Value, Value), String>;
}

/// Names the SSH server's per-login process goes by (OpenSSH 9.8 and later split it out).
const SSH_SESSION_COMMANDS: &[&str] = &["sshd-session", "sshd"];

/// Whether `ip` is a Tailscale address: 100.64.0.0/10 or fd7a:115c:a1e0::/48.
pub fn is_tailnet_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            a == 100 && (64..128).contains(&b)
        }
        IpAddr::V6(v6) => {
            let segments = v6.segments();
            segments[0] == 0xfd7a && segments[1] == 0x115c && segments[2] == 0xa1e0
        }
    }
}

/// Reads `lsof -F pcfn` output (fields one per line: `p` pid, `c` command, `f` descriptor, `n`
/// name). TCP names look like `127.0.0.1:5000->127.0.0.1:58744` or `[::1]:5000->[::1]:58744`;
/// other names (the executable after `ftxt`) go to `files`.
pub fn parse_lsof(text: &str) -> Vec<LsofProcess> {
    let mut processes: Vec<LsofProcess> = Vec::new();
    let mut descriptor = String::new();
    for line in text.lines() {
        let Some(field) = line.chars().next() else {
            continue;
        };
        let value = &line[field.len_utf8()..];
        match field {
            'p' => {
                let Ok(pid) = value.parse() else {
                    continue;
                };
                processes.push(LsofProcess {
                    pid,
                    command: String::new(),
                    connections: Vec::new(),
                    files: Vec::new(),
                });
                descriptor.clear();
            }
            'c' => {
                if let Some(process) = processes.last_mut() {
                    process.command = value.to_string();
                }
            }
            'f' => descriptor = value.to_string(),
            'n' => {
                let Some(process) = processes.last_mut() else {
                    continue;
                };
                if let Some((local, remote)) = value.split_once("->") {
                    if let (Ok(local), Ok(remote)) = (local.parse(), remote.parse()) {
                        process.connections.push((local, remote));
                    }
                } else if descriptor == "txt" {
                    process.files.push(value.to_string());
                }
            }
            _ => {}
        }
    }
    processes
}

/// The process (other than `own_pid`) holding the client end of the loopback connection
/// `client` made to this gxserver's `server_port`.
fn client_process<'a>(
    processes: &'a [LsofProcess],
    own_pid: u32,
    client: SocketAddr,
    server_port: u16,
) -> Option<&'a LsofProcess> {
    processes.iter().find(|process| {
        process.pid != own_pid
            && process.connections.iter().any(|(local, remote)| {
                *local == client && remote.ip().is_loopback() && remote.port() == server_port
            })
    })
}

/// The one tailnet address `connections` reach from this computer, or why there isn't one.
fn tailnet_peer(connections: &[(SocketAddr, SocketAddr)]) -> Result<IpAddr, String> {
    let mut peers: Vec<IpAddr> = connections
        .iter()
        .map(|(_, remote)| remote.ip())
        .filter(|ip| is_tailnet_ip(*ip))
        .collect();
    peers.sort();
    peers.dedup();
    match peers.as_slice() {
        [peer] => Ok(*peer),
        [] => Err("the SSH login did not come from a tailnet address".to_string()),
        _ => Err("the SSH login holds connections to several tailnet addresses".to_string()),
    }
}

fn json_i64(value: &Value) -> Option<i64> {
    value.as_i64().or_else(|| value.as_u64().map(|n| n as i64))
}

fn non_empty_list(value: &Value) -> bool {
    value.as_array().is_some_and(|list| !list.is_empty())
}

/// Whether whois says `peer_ip` is a node of this computer's own Tailscale user: `Ok` with the
/// node's name, or why not. `status` is `tailscale status --json`, `whois` is `tailscale whois
/// --json <peer_ip>`.
pub fn judge_whois(peer_ip: IpAddr, status: &Value, whois: &Value) -> Result<String, String> {
    if status.get("BackendState").and_then(Value::as_str) != Some("Running") {
        return Err("Tailscale is not running on this computer".to_string());
    }
    let own = &status["Self"];
    let Some(own_user) = json_i64(&own["UserID"]).filter(|id| *id != 0) else {
        return Err("Tailscale did not name this computer's user".to_string());
    };
    if non_empty_list(&own["Tags"]) {
        return Err("this computer is a tagged Tailscale device".to_string());
    }
    let own_ip = |ip: IpAddr| {
        own["TailscaleIPs"].as_array().is_some_and(|ips| {
            ips.iter()
                .filter_map(Value::as_str)
                .filter_map(|text| text.parse::<IpAddr>().ok())
                .any(|own| own == ip)
        })
    };
    if own_ip(peer_ip) {
        return Err("the SSH login came from this computer's own Tailscale address".to_string());
    }
    let node = &whois["Node"];
    if !node.is_object() {
        return Err("Tailscale whois named no node".to_string());
    }
    // whois answers for the node that holds the address: check it is that address.
    let holds_peer = node["Addresses"].as_array().is_some_and(|addresses| {
        addresses.iter().filter_map(Value::as_str).any(|prefix| {
            prefix
                .split('/')
                .next()
                .and_then(|ip| ip.parse::<IpAddr>().ok())
                == Some(peer_ip)
        })
    });
    if !holds_peer {
        return Err("Tailscale whois answered for another address".to_string());
    }
    let own_stable_id = own["ID"].as_str().unwrap_or_default();
    if !own_stable_id.is_empty() && node["StableID"].as_str() == Some(own_stable_id) {
        return Err("the SSH login came from this computer itself".to_string());
    }
    if non_empty_list(&node["Tags"]) {
        return Err("the phone's node is a tagged Tailscale device".to_string());
    }
    if json_i64(&node["Sharer"]).is_some_and(|sharer| sharer != 0) {
        return Err("the phone's node is shared in from another tailnet".to_string());
    }
    if json_i64(&node["User"]) != Some(own_user) {
        return Err("the phone's node belongs to another Tailscale user".to_string());
    }
    if json_i64(&whois["UserProfile"]["ID"]) != Some(own_user) {
        return Err("Tailscale whois named another user".to_string());
    }
    let name = node["ComputedName"]
        .as_str()
        .or_else(|| node["Name"].as_str())
        .unwrap_or_default()
        .trim_end_matches('.')
        .to_string();
    Ok(if name.is_empty() {
        peer_ip.to_string()
    } else {
        name
    })
}

/// The whole check, through `probe`. `client` is the loopback address the request came from,
/// `server_port` the port it reached, `own_pid` this gxserver's.
pub fn check_with(
    probe: &dyn TailnetProbe,
    client: Option<SocketAddr>,
    server_port: u16,
    own_pid: u32,
) -> TailnetOwnerCheck {
    let Some(client) = client else {
        return TailnetOwnerCheck::ask("the request's address is unknown");
    };
    if !client.ip().is_loopback() {
        return TailnetOwnerCheck::ask("the request did not come through an SSH forward");
    }
    let listed = match probe.processes_on_port(client.port()) {
        Ok(listed) => parse_lsof(&listed),
        Err(error) => return TailnetOwnerCheck::ask(format!("could not list sockets: {error}")),
    };
    let Some(holder) = client_process(&listed, own_pid, client, server_port) else {
        return TailnetOwnerCheck::ask("no other process holds the request's connection");
    };
    if !SSH_SESSION_COMMANDS.contains(&holder.command.as_str()) {
        return TailnetOwnerCheck::ask(format!(
            "the request came from {}, not an SSH login",
            holder.command
        ));
    }
    let pid = holder.pid;
    let files = match probe.process_files(pid) {
        Ok(files) => parse_lsof(&files),
        Err(error) => {
            return TailnetOwnerCheck::ask(format!(
                "could not read the SSH login's sockets: {error}"
            ))
        }
    };
    let Some(session) = files.iter().find(|process| process.pid == pid) else {
        return TailnetOwnerCheck::ask("the SSH login ended");
    };
    match session.files.first() {
        Some(executable) if probe.root_owned(Path::new(executable)) => {}
        _ => return TailnetOwnerCheck::ask("the SSH login's program is not the system's"),
    }
    match probe.parent_uid(pid) {
        Ok(0) => {}
        Ok(_) => return TailnetOwnerCheck::ask("the SSH login was not started by the system"),
        Err(error) => {
            return TailnetOwnerCheck::ask(format!(
                "could not read the SSH login's parent: {error}"
            ))
        }
    }
    let peer_ip = match tailnet_peer(&session.connections) {
        Ok(ip) => ip,
        Err(reason) => return TailnetOwnerCheck::Ask { reason },
    };
    if peer_ip.is_loopback() {
        return TailnetOwnerCheck::ask("the SSH login came from this computer");
    }
    let (status, whois) = match probe.tailscale(peer_ip) {
        Ok(answers) => answers,
        Err(error) => return TailnetOwnerCheck::ask(format!("Tailscale could not tell: {error}")),
    };
    match judge_whois(peer_ip, &status, &whois) {
        Ok(node_name) => TailnetOwnerCheck::Owned { peer_ip, node_name },
        Err(reason) => TailnetOwnerCheck::Ask { reason },
    }
}

/// The check against this computer.
pub fn check(client: Option<SocketAddr>, server_port: u16) -> TailnetOwnerCheck {
    if !cfg!(target_os = "macos") {
        return TailnetOwnerCheck::ask("this computer can't trace an SSH login's address");
    }
    check_with(&SystemProbe, client, server_port, std::process::id())
}

/// The real probe: `/usr/sbin/lsof`, `/bin/ps` and root-owned Tailscale CLIs, by absolute path.
pub struct SystemProbe;

/// Where a Tailscale CLI is installed by root: the Mac app, its `/usr/local/bin` wrapper, and the
/// Linux package. Never PATH: a user-made link there to a root-owned helper (`env`) would pass.
const TAILSCALE_CLIS: &[&str] = &[
    "/Applications/Tailscale.app/Contents/MacOS/Tailscale",
    "/usr/local/bin/tailscale",
    "/usr/bin/tailscale",
];

fn run(program: &str, args: &[&str]) -> Result<String, String> {
    let output = crate::platform::process::background_command(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .map_err(|error| format!("{program}: {error}"))?;
    // lsof exits 1 when one of several selections matched nothing; its output still counts.
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

impl TailnetProbe for SystemProbe {
    fn processes_on_port(&self, port: u16) -> Result<String, String> {
        run(
            "/usr/sbin/lsof",
            &[
                "-nP",
                &format!("-iTCP:{port}"),
                "-sTCP:ESTABLISHED",
                "-F",
                "pcfn",
            ],
        )
    }

    fn process_files(&self, pid: u32) -> Result<String, String> {
        let pid = pid.to_string();
        let executable = run(
            "/usr/sbin/lsof",
            &["-nP", "-a", "-p", &pid, "-d", "txt", "-F", "pcfn"],
        )?;
        let sockets = run(
            "/usr/sbin/lsof",
            &[
                "-nP",
                "-a",
                "-p",
                &pid,
                "-iTCP",
                "-sTCP:ESTABLISHED",
                "-F",
                "pcfn",
            ],
        )?;
        // One process: the executable's lines, then the sockets' under the same pid.
        let sockets: String = sockets
            .lines()
            .filter(|line| !line.starts_with('p') && !line.starts_with('c'))
            .map(|line| format!("{line}\n"))
            .collect();
        Ok(format!("{executable}{sockets}"))
    }

    fn root_owned(&self, path: &Path) -> bool {
        root_owned_file(path)
    }

    fn parent_uid(&self, pid: u32) -> Result<u32, String> {
        let ppid = run("/bin/ps", &["-o", "ppid=", "-p", &pid.to_string()])?;
        let ppid = ppid
            .trim()
            .parse::<u32>()
            .map_err(|_| "no parent process".to_string())?;
        run("/bin/ps", &["-o", "uid=", "-p", &ppid.to_string()])?
            .trim()
            .parse::<u32>()
            .map_err(|_| "the parent process ended".to_string())
    }

    fn tailscale(&self, peer_ip: IpAddr) -> Result<(Value, Value), String> {
        let mut errors = Vec::new();
        for binary in TAILSCALE_CLIS.iter().map(Path::new) {
            if !root_owned_file(binary) {
                continue;
            }
            let ask = |args: &[&str]| -> Result<Value, String> {
                let output = crate::platform::process::background_command(&binary)
                    .args(args)
                    .env("TAILSCALE_BE_CLI", "1")
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .output()
                    .map_err(|error| error.to_string())?;
                serde_json::from_slice(&output.stdout)
                    .map_err(|_| format!("{} answered no JSON", binary.display()))
            };
            let status = match ask(&["status", "--json"]) {
                Ok(status) => status,
                Err(error) => {
                    errors.push(error);
                    continue;
                }
            };
            if status.get("BackendState").and_then(Value::as_str) != Some("Running") {
                continue;
            }
            return ask(&["whois", "--json", &peer_ip.to_string()]).map(|whois| (status, whois));
        }
        Err(if errors.is_empty() {
            "no root-owned Tailscale CLI with a running backend".to_string()
        } else {
            errors.join("; ")
        })
    }
}

/// Whether `path` (followed through links) is a file owned by root that only root can change.
#[cfg(unix)]
fn root_owned_file(path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).is_ok_and(|metadata| {
        metadata.is_file() && metadata.uid() == 0 && metadata.mode() & 0o022 == 0
    })
}

#[cfg(not(unix))]
fn root_owned_file(_path: &Path) -> bool {
    false
}

#[cfg(test)]
#[path = "tailnet_owner_tests.rs"]
mod tests;
