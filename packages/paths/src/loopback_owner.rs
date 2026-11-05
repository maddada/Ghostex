//! Whether the process listening on a loopback port belongs to this account, checked before a
//! client sends it the gxserver token.
//!
//! CDXC:ServerDaemon 2026-11-05 DECISION:
//! User (security audit, Q17 "check the port's owner first"): every gxserver client sent its
//! permanent token to whatever answered on 127.0.0.1:<port>, and any account can listen on a port
//! above 1024 while gxserver is not running (after a reboot, during a restart, on a shared Linux
//! host before the user's gxserver starts), so another account could collect the token by taking
//! the port first. Before a token goes to a loopback address, the client now checks that a process
//! of this account owns the listening socket, and refuses with a clear error when another account
//! does. Linux reads the socket's owner from /proc/net/tcp; macOS looks through this account's own
//! processes for the listener (gxserver's pid from its runtime file first), since another account's
//! sockets cannot be read without root and the kernel never lets two accounts share a listening
//! port; Windows maps the port to its process and compares the process's user with ours. On Linux
//! and Windows, root and SYSTEM count as ours: they can read the token file anyway, and rootful
//! Docker's `docker-proxy` holds a local Cloud Box's port. macOS cannot name another account's (or
//! root's) listener without privileges (`netstat` and the socket-table sysctl come back empty for a
//! process macOS does not give network visibility), so there a listener outside this account's
//! processes is refused; Docker Desktop's ports belong to the user there.
//! When nothing listens, the request goes ahead and fails on its own. No protocol change, so every
//! gxserver version still works.
//! The check runs on every call (review 2026-11-05): an earlier 5-second trust let a reconnect
//! during a gxserver restart skip it, the very moment another account can take the port. Only the
//! pid found last time is remembered, and it is checked first (about 0.1 ms on macOS).
//! SEE-ALSO: the clients that call `check_url` / `check_host_port` (server/src/http_client.rs,
//! server/src/ghostex_cli/rpc.rs, packages/gx-client, packages/gx-chat-client, the desktop's
//! typed operations and tunnels).

use std::collections::HashMap;
use std::fmt;
use std::sync::{Mutex, OnceLock};

/// A loopback port whose listener belongs to another account on this computer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForeignListener {
    pub port: u16,
}

/// How a `ForeignListener` refusal starts, so a client that only has the text can tell it apart.
const FOREIGN_LISTENER_PREFIX: &str = "Another account on this computer is listening on";

/// The code a client gives this refusal where errors carry one, so a chat shows its words instead of
/// its generic "Couldn't reach Ghostex" (gx-chat-core `is_unreachable` treats a codeless refusal as
/// the service not answering).
pub const FOREIGN_LISTENER_CODE: &str = "foreignListener";

/// Whether an error message is a `ForeignListener` refusal.
pub fn is_foreign_listener_message(message: &str) -> bool {
    message.starts_with(FOREIGN_LISTENER_PREFIX)
}

impl fmt::Display for ForeignListener {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{FOREIGN_LISTENER_PREFIX} 127.0.0.1:{port}, where Ghostex's service should be, so Ghostex did not send it the access token. Stop that program (an administrator can find it with `sudo lsof -nP -iTCP:{port} -sTCP:LISTEN`), then start Ghostex again.",
            port = self.port
        )
    }
}

impl std::error::Error for ForeignListener {}

/// Who listens on a loopback port.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Owner {
    /// A process of this account (its pid, when the platform names it).
    Ours(Option<u32>),
    /// Another account (or the system).
    Other,
    /// Nothing listens.
    Nobody,
}

/// The pid last found listening on each port, checked first next time. Never a verdict.
fn known_pids() -> &'static Mutex<HashMap<u16, u32>> {
    static PIDS: OnceLock<Mutex<HashMap<u16, u32>>> = OnceLock::new();
    PIDS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// `check_host_port` for an `http://`, `https://`, `ws://` or `wss://` URL; any other URL, or one
/// without a port, passes.
pub fn check_url(url: &str) -> Result<(), ForeignListener> {
    let Some((host, port)) = url_host_port(url) else {
        return Ok(());
    };
    check_host_port(&host, port)
}

/// Refuses when `host` is a loopback address and another account listens on `port`.
pub fn check_host_port(host: &str, port: u16) -> Result<(), ForeignListener> {
    if !is_loopback_host(host) {
        return Ok(());
    }
    let known = known_pids()
        .lock()
        .ok()
        .and_then(|pids| pids.get(&port).copied());
    let owner = listener_owner(port, known);
    if let Ok(mut pids) = known_pids().lock() {
        match owner {
            Owner::Ours(Some(pid)) => pids.insert(port, pid),
            _ => pids.remove(&port),
        };
    }
    match owner {
        Owner::Other => Err(ForeignListener { port }),
        Owner::Ours(_) | Owner::Nobody => Ok(()),
    }
}

/// `host[:port]` of a URL, the port defaulting to nothing (no check without one).
fn url_host_port(url: &str) -> Option<(String, u16)> {
    let rest = ["http://", "https://", "ws://", "wss://"]
        .iter()
        .find_map(|scheme| {
            url.get(..scheme.len())
                .filter(|prefix| prefix.eq_ignore_ascii_case(scheme))
                .map(|_| &url[scheme.len()..])
        })?;
    let authority = rest.split(['/', '?', '#']).next()?;
    let authority = authority.rsplit('@').next()?;
    let (host, port) = if let Some(bracketed) = authority.strip_prefix('[') {
        let (host, after) = bracketed.split_once(']')?;
        (host, after.strip_prefix(':')?)
    } else {
        authority.rsplit_once(':')?
    };
    Some((host.to_string(), port.parse().ok()?))
}

fn is_loopback_host(host: &str) -> bool {
    let host = host.trim_start_matches('[').trim_end_matches(']');
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

/// Something accepts connections on 127.0.0.1:`port`. Nothing is sent.
#[cfg(target_os = "macos")]
fn something_listens(port: u16) -> bool {
    std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        std::time::Duration::from_millis(300),
    )
    .is_ok()
}

/// A listener of this uid may receive the token: this account's own, or root's (which can read
/// the token file anyway).
#[cfg(any(target_os = "linux", test))]
fn trusted_uid(listener_uid: u32, own_uid: u32) -> bool {
    listener_uid == own_uid || listener_uid == 0
}

/// Who listens on `port` according to one /proc/net/tcp or /proc/net/tcp6 table.
#[cfg(any(target_os = "linux", test))]
fn owner_in_proc_net_tcp(table: &str, port: u16, own_uid: u32) -> Owner {
    let mut owner = Owner::Nobody;
    for line in table.lines().skip(1) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        // sl local_address rem_address st tx:rx tr:when retrnsmt uid ...
        let (Some(local), Some(state), Some(owner_uid)) =
            (fields.get(1), fields.get(3), fields.get(7))
        else {
            continue;
        };
        if *state != "0A" {
            continue;
        }
        let Some(local_port) = local
            .rsplit_once(':')
            .and_then(|(_, hex)| u16::from_str_radix(hex, 16).ok())
        else {
            continue;
        };
        if local_port != port {
            continue;
        }
        match owner_uid.parse::<u32>() {
            Ok(listener_uid) if trusted_uid(listener_uid, own_uid) => owner = Owner::Ours(None),
            _ => return Owner::Other,
        }
    }
    owner
}

#[cfg(target_os = "linux")]
fn listener_owner(port: u16, _known_pid: Option<u32>) -> Owner {
    // SAFETY: geteuid has no preconditions.
    let uid = unsafe { libc::geteuid() };
    let mut owner = Owner::Nobody;
    for table in ["/proc/net/tcp", "/proc/net/tcp6"] {
        match std::fs::read_to_string(table).map(|text| owner_in_proc_net_tcp(&text, port, uid)) {
            Ok(Owner::Other) => return Owner::Other,
            Ok(Owner::Ours(pid)) => owner = Owner::Ours(pid),
            _ => {}
        }
    }
    owner
}

#[cfg(target_os = "macos")]
fn listener_owner(port: u16, known_pid: Option<u32>) -> Owner {
    let mut hints: Vec<u32> = known_pid.into_iter().collect();
    hints.extend(macos::gxserver_pid(port));
    for pid in &hints {
        if macos::pid_listens_on(*pid as libc::c_int, port) {
            return Owner::Ours(Some(*pid));
        }
    }
    for pid in macos::own_pids() {
        if hints.contains(&(pid as u32)) {
            continue;
        }
        if macos::pid_listens_on(pid, port) {
            return Owner::Ours(Some(pid as u32));
        }
    }
    // Not one of this account's processes: if anything answers, it belongs to another account
    // (or root, which macOS does not let us tell apart).
    if something_listens(port) {
        Owner::Other
    } else {
        Owner::Nobody
    }
}

#[cfg(windows)]
fn listener_owner(port: u16, _known_pid: Option<u32>) -> Owner {
    match windows::listening_pid(port) {
        Some(pid) if windows::trusted_user(pid) => Owner::Ours(Some(pid)),
        Some(_) => Owner::Other,
        // No IPv4 listener there, or the table could not be read: the request goes ahead.
        None => Owner::Nobody,
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn listener_owner(_port: u16, _known_pid: Option<u32>) -> Owner {
    Owner::Ours(None)
}

#[cfg(target_os = "macos")]
mod macos {
    use libc::{c_int, c_void};

    /// `PROC_UID_ONLY` (sys/proc_info.h).
    const PROC_UID_ONLY: u32 = 4;
    /// `PROC_PIDFDSOCKETINFO`.
    const PROC_PIDFDSOCKETINFO: c_int = 3;
    /// `SOCKINFO_TCP` and `TSI_S_LISTEN`.
    const SOCKINFO_TCP: i32 = 2;
    const TSI_S_LISTEN: i32 = 1;
    /// `sizeof(struct socket_fdinfo)` and the offsets of `psi.soi_kind`,
    /// `psi.soi_proto.pri_tcp.tcpsi_ini.insi_lport` and `psi.soi_proto.pri_tcp.tcpsi_state`
    /// (measured with offsetof against the macOS SDK's sys/proc_info.h).
    const SOCKET_FDINFO_SIZE: usize = 792;
    const SOI_KIND: usize = 256;
    const INSI_LPORT: usize = 268;
    const TCPSI_STATE: usize = 344;

    /// The pid in gxserver's runtime file, when that gxserver serves `port`. The file sits in this
    /// account's private state folder, so only this account can have written it.
    pub(super) fn gxserver_pid(port: u16) -> Option<u32> {
        let file = crate::GhostexPaths::resolve()
            .gxserver_state_dir()
            .join("runtime")
            .join("server.json");
        let metadata: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(file).ok()?).ok()?;
        (metadata.get("port")?.as_u64()? == u64::from(port))
            .then(|| metadata.get("pid")?.as_u64())
            .flatten()
            .and_then(|pid| u32::try_from(pid).ok())
    }

    /// Every process of this account.
    pub(super) fn own_pids() -> Vec<c_int> {
        // SAFETY: getuid has no preconditions.
        let uid = unsafe { libc::getuid() };
        // SAFETY: a null buffer asks for the size needed.
        let bytes = unsafe { libc::proc_listpids(PROC_UID_ONLY, uid, std::ptr::null_mut(), 0) };
        if bytes <= 0 {
            return Vec::new();
        }
        let mut pids = vec![0 as c_int; bytes as usize / std::mem::size_of::<c_int>() + 64];
        // SAFETY: the buffer holds `pids.len()` c_ints.
        let filled = unsafe {
            libc::proc_listpids(
                PROC_UID_ONLY,
                uid,
                pids.as_mut_ptr() as *mut c_void,
                (pids.len() * std::mem::size_of::<c_int>()) as c_int,
            )
        };
        pids.truncate(filled.max(0) as usize / std::mem::size_of::<c_int>());
        pids.retain(|pid| *pid > 0);
        pids
    }

    /// `pid` holds a TCP socket listening on `port`.
    pub(super) fn pid_listens_on(pid: c_int, port: u16) -> bool {
        let entry = std::mem::size_of::<libc::proc_fdinfo>();
        // SAFETY: a null buffer asks for the size needed.
        let bytes =
            unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDLISTFDS, 0, std::ptr::null_mut(), 0) };
        if bytes <= 0 {
            return false;
        }
        let mut fds: Vec<libc::proc_fdinfo> = Vec::with_capacity(bytes as usize / entry + 16);
        // SAFETY: the buffer has room for `capacity` entries; the call says how many it wrote.
        let filled = unsafe {
            libc::proc_pidinfo(
                pid,
                libc::PROC_PIDLISTFDS,
                0,
                fds.as_mut_ptr() as *mut c_void,
                (fds.capacity() * entry) as c_int,
            )
        };
        if filled <= 0 {
            return false;
        }
        // SAFETY: proc_pidinfo initialized `filled` bytes of whole entries.
        unsafe { fds.set_len(filled as usize / entry) };
        let mut info = [0u64; SOCKET_FDINFO_SIZE / 8];
        fds.iter()
            .filter(|fd| fd.proc_fdtype == libc::PROX_FDTYPE_SOCKET as u32)
            .any(|fd| {
                // SAFETY: `info` is SOCKET_FDINFO_SIZE bytes, 8-byte aligned.
                let written = unsafe {
                    libc::proc_pidfdinfo(
                        pid,
                        fd.proc_fd,
                        PROC_PIDFDSOCKETINFO,
                        info.as_mut_ptr() as *mut c_void,
                        SOCKET_FDINFO_SIZE as c_int,
                    )
                };
                if written < SOCKET_FDINFO_SIZE as c_int {
                    return false;
                }
                // SAFETY: `info` is SOCKET_FDINFO_SIZE bytes, so every offset read is in bounds.
                let bytes = unsafe {
                    std::slice::from_raw_parts(info.as_ptr() as *const u8, SOCKET_FDINFO_SIZE)
                };
                let int_at = |offset: usize| {
                    i32::from_ne_bytes(bytes[offset..offset + 4].try_into().unwrap_or([0; 4]))
                };
                int_at(SOI_KIND) == SOCKINFO_TCP
                    && int_at(TCPSI_STATE) == TSI_S_LISTEN
                    // The port is kept in network byte order in the int's low 16 bits.
                    && u16::from_be(int_at(INSI_LPORT) as u16) == port
            })
    }
}

#[cfg(windows)]
mod windows {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, NO_ERROR};
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        GetExtendedTcpTable, MIB_TCPROW_OWNER_PID, TCP_TABLE_OWNER_PID_LISTENER,
    };
    use windows_sys::Win32::Networking::WinSock::AF_INET;
    use windows_sys::Win32::Security::{
        EqualSid, GetTokenInformation, IsWellKnownSid, TokenUser, WinLocalSystemSid, TOKEN_QUERY,
        TOKEN_USER,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    /// The pid listening on 127.0.0.1 or 0.0.0.0 at `port`, from the IPv4 listener table.
    pub(super) fn listening_pid(port: u16) -> Option<u32> {
        let mut size: u32 = 0;
        // SAFETY: a null table asks for the size needed.
        unsafe {
            GetExtendedTcpTable(
                std::ptr::null_mut(),
                &mut size,
                0,
                u32::from(AF_INET),
                TCP_TABLE_OWNER_PID_LISTENER,
                0,
            )
        };
        if size == 0 {
            return None;
        }
        let mut buffer = vec![0u64; size as usize / 8 + 1];
        // SAFETY: the buffer holds at least `size` bytes.
        let result = unsafe {
            GetExtendedTcpTable(
                buffer.as_mut_ptr() as *mut _,
                &mut size,
                0,
                u32::from(AF_INET),
                TCP_TABLE_OWNER_PID_LISTENER,
                0,
            )
        };
        if result != NO_ERROR {
            return None;
        }
        // SAFETY: the table starts with its entry count, followed by that many rows.
        unsafe {
            let base = buffer.as_ptr() as *const u8;
            let count = *(base as *const u32) as usize;
            let rows = base.add(std::mem::size_of::<u32>()) as *const MIB_TCPROW_OWNER_PID;
            (0..count)
                .map(|index| &*rows.add(index))
                .find(|row| u16::from_be(row.dwLocalPort as u16) == port)
                .map(|row| row.dwOwningPid)
        }
    }

    /// `pid` runs as the same user as this process, or as SYSTEM (which can read the token file
    /// anyway).
    pub(super) fn trusted_user(pid: u32) -> bool {
        // SAFETY: the handles opened here are closed before returning.
        unsafe {
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() {
                return false;
            }
            let theirs = token_user(process);
            CloseHandle(process);
            let ours = token_user(GetCurrentProcess());
            match (theirs, ours) {
                (Some(theirs), Some(ours)) => {
                    let theirs_user = theirs.as_ptr() as *const TOKEN_USER;
                    let ours_user = ours.as_ptr() as *const TOKEN_USER;
                    EqualSid((*theirs_user).User.Sid, (*ours_user).User.Sid) != 0
                        || IsWellKnownSid((*theirs_user).User.Sid, WinLocalSystemSid) != 0
                }
                _ => false,
            }
        }
    }

    /// The TOKEN_USER of a process, in a buffer that keeps its SID alive.
    unsafe fn token_user(process: HANDLE) -> Option<Vec<u64>> {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
            return None;
        }
        let mut size: u32 = 0;
        GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut size);
        let mut buffer = vec![0u64; size as usize / 8 + 1];
        let ok = GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr() as *mut _,
            size,
            &mut size,
        ) != 0;
        CloseHandle(token);
        ok.then_some(buffer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_host_port_reads_loopback_and_other_addresses() {
        assert_eq!(
            url_host_port("http://127.0.0.1:58744/api/health"),
            Some(("127.0.0.1".into(), 58744))
        );
        assert_eq!(
            url_host_port("WS://localhost:3785/x?y=1"),
            Some(("localhost".into(), 3785))
        );
        assert_eq!(
            url_host_port("ws://[::1]:58744/api/events"),
            Some(("::1".into(), 58744))
        );
        assert_eq!(
            url_host_port("http://user:secret@127.0.0.1:9/"),
            Some(("127.0.0.1".into(), 9))
        );
        // No port, another scheme, or a port that is not a number: no check applies.
        assert_eq!(url_host_port("https://example.com/x"), None);
        assert_eq!(url_host_port("file:///etc/hosts"), None);
        assert_eq!(url_host_port("http://127.0.0.1:http/"), None);
        assert!(is_loopback_host("localhost") && is_loopback_host("[::1]"));
        assert!(is_loopback_host("127.0.0.2"));
        assert!(!is_loopback_host("100.105.82.19") && !is_loopback_host("evil.example"));
        // A host that is not loopback is never checked.
        assert_eq!(check_url("ws://evil.example:22/api/cefDevtools/x"), Ok(()));
    }

    const PROC_NET_TCP: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 0100007F:E578 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 41771 1 0000000000000000 100 0 0 10 0
   1: 0100007F:0016 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 12345 1 0000000000000000 100 0 0 10 0
   2: 00000000:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1001        0 22222 1 0000000000000000 100 0 0 10 0
   3: 0100007F:E578 0100007F:A000 01 00000000:00000000 00:00000000 00000000  1001        0 33333 1 0000000000000000 100 0 0 10 0
";

    #[test]
    fn linux_table_names_the_listener_owner() {
        // 58744 (0xE578) listens as uid 1000; the connected row from uid 1001 is not a listener.
        assert_eq!(
            owner_in_proc_net_tcp(PROC_NET_TCP, 58744, 1000),
            Owner::Ours(None)
        );
        assert_eq!(
            owner_in_proc_net_tcp(PROC_NET_TCP, 58744, 1001),
            Owner::Other
        );
        // Root's listener (22) is trusted; another account's (8080 on any address) is not.
        assert_eq!(
            owner_in_proc_net_tcp(PROC_NET_TCP, 22, 1000),
            Owner::Ours(None)
        );
        assert_eq!(
            owner_in_proc_net_tcp(PROC_NET_TCP, 8080, 1000),
            Owner::Other
        );
        assert_eq!(
            owner_in_proc_net_tcp(PROC_NET_TCP, 3785, 1000),
            Owner::Nobody
        );
        let tcp6 = "  sl  local_address                         remote_address                        st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 00000000000000000000000001000000:E578 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000  1001        0 1 1 0 100 0 0 10 0
";
        assert_eq!(owner_in_proc_net_tcp(tcp6, 58744, 1000), Owner::Other);
    }

    #[test]
    fn root_and_this_account_are_trusted() {
        assert!(trusted_uid(501, 501) && trusted_uid(0, 501));
        assert!(!trusted_uid(502, 501));
    }
}
