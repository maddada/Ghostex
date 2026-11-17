//! Whether a request to gxserver was made by a process inside an SSH login to this computer
//! (`/api/proveOwnerHandover`, `ghostex computer-code --prove-handover`).
//!
//! CDXC:Security 2026-11-17 DECISION:
//! The coordinator, for the user's Q2 ("Build it") on the review of 8abb0a8db: a computer the
//! terminal let in becomes this computer's owner only if it "has actually logged in over SSH with
//! real credentials", which an agent's made-up computer can't. Its app reaches gxserver with this
//! computer's token file, the same one any agent here can read, so the token proves nothing; the
//! process does. The joining app runs `computer-code --prove-handover` through its own SSH login,
//! and gxserver traces the process at the other end of that very loopback connection in the
//! kernel's socket table (never the request or the environment: `SSH_CONNECTION` is the caller's to
//! set), then walks its parents: within three steps (`MAX_HOPS`: the command, the login shell, the
//! SSH server's per-login process) every one must run as this user until a root process named
//! `sshd-session` or `sshd`, the SSH server that let the login in. A process an agent starts runs
//! under Ghostex's session hosts, which detach from whatever started them, so its parents never
//! reach the SSH server, and it can't make a root process its parent. Unlike `tailnet_owner.rs`,
//! the process traced is the command itself, not the SSH server's per-login process, so it works on
//! Linux too (Linux hides that sshd process's sockets from the user, but not a plain command's).
//! Anything that can't be traced (Windows, an unreadable socket table) refuses.
//! Known limits, all the same-user limit: a program running as the user that can sign in here over
//! SSH itself (the user's key in `authorized_keys` and a private key it can read) passes, as does
//! one that injects code into a process already in the user's SSH login (ptrace, where the system
//! allows it); either could already add its own SSH key without gxserver.
//! SEE-ALSO: security/policy.rs (`terminal_may_approve`), remote_access/tailnet_owner.rs.

use std::net::SocketAddr;

/// How many parents up the SSH server may be: the command, then its shell, then the SSH
/// server's per-login process running as the user, whose parent is the root SSH server.
const MAX_HOPS: usize = 3;

/// One process's parent, owner and name.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ProcessFacts {
    ppid: u32,
    uid: u32,
    /// The executable's file name (`sshd-session`), without its folder.
    name: String,
}

/// `Ok` when the request from `client` to this gxserver's `server_port` came from a process
/// inside an SSH login here; otherwise why not, in plain words.
pub fn check(client: Option<SocketAddr>, server_port: u16) -> Result<(), String> {
    if !cfg!(any(target_os = "macos", target_os = "linux")) {
        return Err("this computer can't trace an SSH login".to_string());
    }
    let client = client.ok_or("the request's address is unknown")?;
    if !client.ip().is_loopback() {
        return Err("the request didn't come from this computer".to_string());
    }
    let pid = caller_pid(client, server_port)?;
    walk_to_ssh_server(pid, own_uid(), process_facts)
}

/// Walks up from `pid`: `Ok` once a root `sshd-session`/`sshd` is reached within [`MAX_HOPS`]
/// with every process below it running as `uid`.
fn walk_to_ssh_server(
    pid: u32,
    uid: u32,
    facts: impl Fn(u32) -> Result<ProcessFacts, String>,
) -> Result<(), String> {
    let mut current = facts(pid)?;
    if current.uid != uid {
        return Err("the request came from another account".to_string());
    }
    for _ in 0..MAX_HOPS {
        if current.ppid <= 1 {
            return Err(format!("{} was not started by an SSH login", current.name));
        }
        let parent = facts(current.ppid)?;
        if parent.uid == 0 {
            return if crate::remote_access::tailnet_owner::SSH_SESSION_COMMANDS
                .contains(&parent.name.as_str())
            {
                Ok(())
            } else {
                Err(format!(
                    "it was started by {}, not by the SSH server",
                    parent.name
                ))
            };
        }
        if parent.uid != uid {
            return Err("a process of another account started it".to_string());
        }
        current = parent;
    }
    Err("it wasn't started by an SSH login".to_string())
}

#[cfg(unix)]
fn own_uid() -> u32 {
    // SAFETY: getuid has no preconditions and can't fail.
    unsafe { libc::getuid() }
}

#[cfg(not(unix))]
fn own_uid() -> u32 {
    u32::MAX
}

/// The file name of an executable path or a process name.
fn base_name(name: &str) -> String {
    name.trim()
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_string()
}

// macOS: `lsof` and `ps`, by absolute path (never PATH).

#[cfg(target_os = "macos")]
fn caller_pid(client: SocketAddr, server_port: u16) -> Result<u32, String> {
    use crate::remote_access::tailnet_owner::{client_process, parse_lsof};
    let listed = run(
        "/usr/sbin/lsof",
        &[
            "-nP",
            &format!("-iTCP:{}", client.port()),
            "-sTCP:ESTABLISHED",
            "-F",
            "pcfn",
        ],
    )?;
    let processes = parse_lsof(&listed);
    let holder = client_process(&processes, std::process::id(), client, server_port)
        .ok_or("no other process holds the request's connection")?;
    // A socket two processes share (one handed it to the other) names neither for sure.
    let holders = processes
        .iter()
        .filter(|process| {
            process.pid != std::process::id()
                && process
                    .connections
                    .iter()
                    .any(|(local, remote)| *local == client && remote.port() == server_port)
        })
        .count();
    if holders != 1 {
        return Err("more than one process holds the request's connection".to_string());
    }
    Ok(holder.pid)
}

#[cfg(target_os = "macos")]
fn process_facts(pid: u32) -> Result<ProcessFacts, String> {
    // The real uid (a setuid program's effective uid is root's) and the accounting name, which
    // the SSH server's retitling ("sshd-session: madda@notty") doesn't change.
    let line = run(
        "/bin/ps",
        &["-o", "ppid=,ruid=,ucomm=", "-p", &pid.to_string()],
    )?;
    let mut fields = line.split_whitespace();
    let ppid = fields.next().and_then(|value| value.parse::<u32>().ok());
    let uid = fields.next().and_then(|value| value.parse::<u32>().ok());
    let name = fields.collect::<Vec<_>>().join(" ");
    match (ppid, uid) {
        (Some(ppid), Some(uid)) if !name.is_empty() => Ok(ProcessFacts {
            ppid,
            uid,
            name: base_name(&name),
        }),
        _ => Err("a process in the chain ended".to_string()),
    }
}

#[cfg(target_os = "macos")]
fn run(program: &str, args: &[&str]) -> Result<String, String> {
    let output = crate::platform::process::background_command(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .map_err(|error| format!("{program}: {error}"))?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

// Linux: `/proc`, which shows every process's parent, owner and name, and a plain command's own
// sockets (the SSH server's per-login process hides its own, which is why the command is traced).

#[cfg(target_os = "linux")]
fn caller_pid(client: SocketAddr, server_port: u16) -> Result<u32, String> {
    let inode = ["/proc/net/tcp", "/proc/net/tcp6"]
        .iter()
        .filter_map(|table| std::fs::read_to_string(table).ok())
        .find_map(|table| socket_inode(&table, client, server_port))
        .ok_or("the request's connection isn't in the socket table")?;
    let target = format!("socket:[{inode}]");
    let own = std::process::id();
    let mut holders = Vec::new();
    for entry in std::fs::read_dir("/proc").map_err(|error| format!("/proc: {error}"))? {
        let Some(pid) = entry
            .ok()
            .and_then(|entry| entry.file_name().to_str()?.parse::<u32>().ok())
        else {
            continue;
        };
        if pid == own {
            continue;
        }
        let Ok(descriptors) = std::fs::read_dir(format!("/proc/{pid}/fd")) else {
            continue;
        };
        if descriptors.flatten().any(|descriptor| {
            std::fs::read_link(descriptor.path())
                .is_ok_and(|link| link.to_str() == Some(target.as_str()))
        }) {
            holders.push(pid);
        }
    }
    match holders.as_slice() {
        [pid] => Ok(*pid),
        [] => Err("no process this account can see holds the request's connection".to_string()),
        _ => Err("more than one process holds the request's connection".to_string()),
    }
}

/// The inode of the established socket from `client` to `server_port` in a `/proc/net/tcp`
/// or `tcp6` table.
#[cfg(target_os = "linux")]
fn socket_inode(table: &str, client: SocketAddr, server_port: u16) -> Option<u64> {
    table.lines().skip(1).find_map(|line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let (local, remote, state, inode) = (
            fields.get(1)?,
            fields.get(2)?,
            fields.get(3)?,
            fields.get(9)?,
        );
        // 01: ESTABLISHED.
        if *state != "01" {
            return None;
        }
        let (local_ip, local_port) = proc_address(local)?;
        let (_, remote_port) = proc_address(remote)?;
        (local_port == client.port()
            && remote_port == server_port
            && same_ip(local_ip, client.ip()))
        .then(|| inode.parse::<u64>().ok())
        .flatten()
    })
}

/// `0100007F:D431` (IPv4) or 32 hex digits and a port (IPv6), each 32-bit word in the
/// kernel's byte order (little-endian on every Linux this ships on).
#[cfg(target_os = "linux")]
fn proc_address(text: &str) -> Option<(std::net::IpAddr, u16)> {
    let (ip, port) = text.split_once(':')?;
    let port = u16::from_str_radix(port, 16).ok()?;
    let words = (0..ip.len() / 8)
        .map(|index| u32::from_str_radix(ip.get(index * 8..index * 8 + 8)?, 16).ok())
        .collect::<Option<Vec<u32>>>()?;
    let bytes: Vec<u8> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
    let ip = match bytes.len() {
        4 => std::net::IpAddr::from(<[u8; 4]>::try_from(bytes).ok()?),
        16 => std::net::IpAddr::from(<[u8; 16]>::try_from(bytes).ok()?),
        _ => return None,
    };
    Some((ip, port))
}

/// The same address, an IPv4 one also matching its IPv6-mapped form.
#[cfg(target_os = "linux")]
fn same_ip(left: std::net::IpAddr, right: std::net::IpAddr) -> bool {
    let plain = |ip: std::net::IpAddr| match ip {
        std::net::IpAddr::V6(v6) => v6.to_ipv4_mapped().map(std::net::IpAddr::V4).unwrap_or(ip),
        other => other,
    };
    plain(left) == plain(right)
}

#[cfg(target_os = "linux")]
fn process_facts(pid: u32) -> Result<ProcessFacts, String> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status"))
        .map_err(|_| "a process in the chain ended".to_string())?;
    let field = |name: &str| {
        status
            .lines()
            .find_map(|line| line.strip_prefix(name))
            .and_then(|rest| rest.split_whitespace().next())
            .and_then(|value| value.parse::<u32>().ok())
    };
    let (Some(ppid), Some(uid)) = (field("PPid:"), field("Uid:")) else {
        return Err("a process in the chain can't be read".to_string());
    };
    let name = std::fs::read_to_string(format!("/proc/{pid}/comm"))
        .map_err(|_| "a process in the chain ended".to_string())?;
    Ok(ProcessFacts {
        ppid,
        uid,
        name: base_name(&name),
    })
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn caller_pid(_client: SocketAddr, _server_port: u16) -> Result<u32, String> {
    Err("this computer can't trace an SSH login".to_string())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn process_facts(_pid: u32) -> Result<ProcessFacts, String> {
    Err("this computer can't trace an SSH login".to_string())
}
