//! The installed native Windows Ghostex's gxserver port: 58746, or the port gxserver moved to
//! while Windows reserves 58746.
//!
//! CDXC:ServerDaemon 2026-11-17 WHY:
//! Windows hands out excluded port ranges at every boot (Hyper-V/WinNAT for WSL and Docker,
//! `netsh interface ipv4 show excludedportrange protocol=tcp`), picked from the dynamic range that
//! 58746 sits in. When a range covers 58746, bind fails with WSAEACCES (10013) and gxserver could
//! never start (GitHub #639). gxserver now checks the port before it binds: while Windows forbids
//! it and nothing listens there, gxserver takes the first free port from a block below 49152,
//! where those ranges are not drawn from, and records it here; the app, the `ghostex` CLI and the
//! agents' hooks read the record, so they all reach the gxserver that actually runs. It goes back
//! to 58746 (and drops the record) at the first start where 58746 can be bound again, because other
//! computers reaching this one over SSH still expect 58746.
//! SEE-ALSO: server/src/config.rs (`read_selected_local_api_port`), server/src/ghostex_cli/rpc.rs
//! (`local_gxserver_api_port`), apps/desktop/src/app/helpers/os_cli/process_and_constants.rs
//! (`gpui_local_gxserver_api_port`), server/src/server/foreground.rs (the start that resolves it).
//! A portable copy (`portable`) and an explicit `GHOSTEX_GXSERVER_DEV_PORT` keep their own ports.

use std::{
    fs, io,
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    time::Duration,
};

use crate::GhostexPaths;

/// The port native Windows gxserver listens on whenever Windows allows it.
pub const DEFAULT_PORT: u16 = 58_746;
/// The block a reserved 58746 moves into: below Windows' dynamic range (49152 and up), which the
/// boot-time reservations come from, and below the portable copies' block (41000 and up).
const FALLBACK_FIRST: u16 = 38_746;
const FALLBACK_LAST: u16 = 40_745;
const RECORD_FILE: &str = "local-port";
/// WSAEACCES: the port is in an excluded range (or held with SO_EXCLUSIVEADDRUSE).
const WSAEACCES: i32 = 10_013;

/// Where gxserver records the port it moved to: its runtime folder, beside `server.json`.
pub fn record_file() -> PathBuf {
    GhostexPaths::resolve()
        .gxserver_state_dir()
        .join("runtime")
        .join(RECORD_FILE)
}

/// The port the local gxserver uses: the recorded one while 58746 is reserved, else 58746.
pub fn current() -> u16 {
    read_record(&record_file()).unwrap_or(DEFAULT_PORT)
}

/// A port gxserver itself may pick on Windows (58746 or one of its fallback block), so a
/// `config.json` written while it used one of them is not a user override.
pub fn is_own_port(port: u16) -> bool {
    port == DEFAULT_PORT || (FALLBACK_FIRST..=FALLBACK_LAST).contains(&port)
}

/// What a start chose.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StartPort {
    pub port: u16,
    /// Set when Windows forbids 58746 and gxserver uses `port` instead.
    pub reserved_default: bool,
}

/// Run by gxserver before anything reads its port: decides the port this start listens on and
/// records it (or drops the record when 58746 is usable again).
pub fn resolve_for_start() -> StartPort {
    resolve_with(&record_file(), probe)
}

/// The startup error for a port Windows refuses to let gxserver bind.
pub fn forbidden_port_message(port: u16) -> String {
    format!(
        "Windows does not allow Ghostex to use port {port} (os error 10013). It is usually in a port range Windows reserved for Hyper-V, WSL or Docker; `netsh interface ipv4 show excludedportrange protocol=tcp` lists them. Restart the computer or run `net stop winnat` and `net start winnat` as administrator to free it."
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Probe {
    /// Bindable right now.
    Free,
    /// Something listens there (a running gxserver, which the normal start reuses or replaces).
    InUse,
    /// Windows refuses the bind and nothing listens there: a reserved port.
    Forbidden,
    /// Any other bind error; the start keeps the port and reports the bind error as before.
    Other,
}

fn resolve_with(record: &Path, probe: impl Fn(u16) -> Probe) -> StartPort {
    let recorded = read_record(record).filter(|port| *port != DEFAULT_PORT);
    // A gxserver still running on the recorded port is the one this start reuses or replaces.
    if let Some(port) = recorded {
        if probe(port) == Probe::InUse {
            return StartPort {
                port,
                reserved_default: true,
            };
        }
    }
    if probe(DEFAULT_PORT) != Probe::Forbidden {
        if recorded.is_some() {
            let _ = fs::remove_file(record);
        }
        return StartPort {
            port: DEFAULT_PORT,
            reserved_default: false,
        };
    }
    let fallback = recorded
        .filter(|port| probe(*port) == Probe::Free)
        .or_else(|| (FALLBACK_FIRST..=FALLBACK_LAST).find(|port| probe(*port) == Probe::Free));
    let Some(port) = fallback else {
        // Nothing free in the block either: keep 58746 so the start reports the reserved port.
        return StartPort {
            port: DEFAULT_PORT,
            reserved_default: false,
        };
    };
    if recorded != Some(port) {
        if let Err(error) = write_record(record, port) {
            eprintln!(
                "gxserver could not record its port {port} in {}: {error}",
                record.display()
            );
            return StartPort {
                port: DEFAULT_PORT,
                reserved_default: false,
            };
        }
    }
    StartPort {
        port,
        reserved_default: true,
    }
}

fn probe(port: u16) -> Probe {
    match TcpListener::bind(("127.0.0.1", port)) {
        Ok(_) => Probe::Free,
        Err(error) if error.kind() == io::ErrorKind::AddrInUse => Probe::InUse,
        Err(error)
            if error.raw_os_error() == Some(WSAEACCES)
                || error.kind() == io::ErrorKind::PermissionDenied =>
        {
            let address = SocketAddr::from(([127, 0, 0, 1], port));
            if TcpStream::connect_timeout(&address, Duration::from_millis(300)).is_ok() {
                Probe::InUse
            } else {
                Probe::Forbidden
            }
        }
        Err(_) => Probe::Other,
    }
}

fn read_record(file: &Path) -> Option<u16> {
    fs::read_to_string(file)
        .ok()?
        .trim()
        .parse::<u16>()
        .ok()
        .filter(|port| *port != 0)
}

fn write_record(file: &Path, port: u16) -> io::Result<()> {
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)?;
    }
    let staged = file.with_extension(format!("{}.tmp", std::process::id()));
    fs::write(&staged, format!("{port}\n"))?;
    fs::rename(&staged, file).inspect_err(|_| {
        let _ = fs::remove_file(&staged);
    })
}
