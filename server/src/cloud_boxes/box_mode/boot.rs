//! `ghostex box boot --ticket <ticket>`: make the box this box (reset an identity copied from a
//! snapshot or another box), start its gxserver and, for a real provider, tailcat with the
//! allow-list of the user's devices; start the idle countdown; report how to reach it. Safe to
//! run again: a resume re-runs it.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use ghostex_gx_protocol::cloud_box::{BoxBootResult, BoxBootTicket};
use serde_json::{json, Value};

use super::{idle, state};
use crate::ghostex_cli::{args::Flags, rpc};

const ENDPOINT_WAIT: Duration = Duration::from_secs(60);
const TAILCAT_WAIT: Duration = Duration::from_secs(60);

/// The folder of the running `ghostex` (the package's `bin/`).
fn bin_dir() -> Result<PathBuf, String> {
    std::env::current_exe()
        .map_err(|error| format!("Can't find the ghostex program: {error}"))?
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "Can't find the ghostex program's folder.".to_string())
}

/// What the box remembers about itself: which box it is and its machine id. A different box id
/// means the disk was copied from a snapshot or another box.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct BoxIdentity {
    box_id: String,
    machine_id: String,
}

/// gxserver's identity (its server id), its API token and tailcat's key and address: everything
/// a client knows this box by. A copied set would make two boxes answer as one.
fn copied_identity_files() -> Vec<PathBuf> {
    let paths = crate::paths::get_gxserver_paths(None);
    vec![
        paths.identity_file.clone(),
        paths.auth_token_file.clone(),
        crate::tailcat::tailcat_key_file(&paths),
        crate::tailcat::tailcat_address_file(&paths),
    ]
}

/// Keeps this box's identity, or makes a new one when the disk came from another box (whose
/// gxserver and tailcat identity files are removed, so both make new ones when they start).
/// Returns the identity and whether a copied one was reset.
fn read_or_reset_identity(box_id: &str) -> Result<(BoxIdentity, bool), String> {
    let file = state::identity_file();
    let previous = state::read_json::<BoxIdentity>(&file);
    if let Some(identity) = previous
        .as_ref()
        .filter(|identity| identity.box_id == box_id)
    {
        return Ok((
            BoxIdentity {
                box_id: identity.box_id.clone(),
                machine_id: identity.machine_id.clone(),
            },
            false,
        ));
    }
    for copied in copied_identity_files() {
        match std::fs::remove_file(&copied) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "Can't reset the box identity ({}): {error}",
                    copied.display()
                ))
            }
        }
    }
    let mut entropy = [0u8; 16];
    rand::Rng::fill_bytes(&mut rand::rng(), &mut entropy);
    let identity = BoxIdentity {
        box_id: box_id.to_string(),
        machine_id: ghostex_gx_cloud::ids::gx_id_from_entropy(&entropy),
    };
    state::write_json(&file, &identity)?;
    Ok((identity, previous.is_some()))
}

fn run_json(program: &Path, args: &[&str]) -> Result<Value, String> {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("Can't run {}: {error}", program.display()))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    // `gxserver start --json` prints one pretty-printed object; `endpoint` prints one line.
    serde_json::from_str::<Value>(stdout.trim())
        .ok()
        .filter(Value::is_object)
        .or_else(|| {
            stdout.lines().rev().find_map(|line| {
                serde_json::from_str::<Value>(line.trim())
                    .ok()
                    .filter(Value::is_object)
            })
        })
        .ok_or_else(|| {
            let stderr = String::from_utf8_lossy(&output.stderr);
            format!(
                "{} {} failed: {}",
                program.display(),
                args.join(" "),
                stderr.trim()
            )
        })
}

/// The endpoint of a gxserver that answers. `gxserver endpoint` reads the runtime file the last
/// start wrote, which survives a restart of the box (a `local` or Boat resume), so its port must
/// also accept a connection.
fn running_endpoint(gxserver: &Path) -> Option<Value> {
    run_json(gxserver, &["endpoint"])
        .ok()
        .filter(|endpoint| endpoint.get("ok").and_then(Value::as_bool) == Some(true))
        .filter(|endpoint| {
            endpoint
                .get("port")
                .and_then(Value::as_u64)
                .and_then(|port| u16::try_from(port).ok())
                .is_some_and(|port| {
                    std::net::TcpStream::connect_timeout(
                        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
                        Duration::from_secs(2),
                    )
                    .is_ok()
                })
        })
}

/// Starts gxserver unless it runs (restarting it when the identity was just reset, since a
/// running one still holds the copied identity), and waits for its endpoint (port and token).
fn start_gxserver(bin: &Path, identity_reset: bool) -> Result<Value, String> {
    let gxserver = bin.join("gxserver");
    if let Some(endpoint) = running_endpoint(&gxserver) {
        if !identity_reset {
            return Ok(endpoint);
        }
        let _ = Command::new(&gxserver)
            .arg("stop")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    run_json(&gxserver, &["start", "--json"])?;
    let started = Instant::now();
    loop {
        if let Some(endpoint) = running_endpoint(&gxserver) {
            return Ok(endpoint);
        }
        if started.elapsed() >= ENDPOINT_WAIT {
            return Err(format!(
                "gxserver did not start on the box within {} seconds.",
                ENDPOINT_WAIT.as_secs()
            ));
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// Starts `ghostex box relay` in the background unless something already listens on its port.
fn start_relay(bin: &Path, gxserver_port: u16) -> Result<(), String> {
    let port = ghostex_gx_cloud::local::RELAY_PORT;
    if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
        return Ok(());
    }
    let mut command = Command::new(bin.join("ghostex"));
    command
        .args([
            "box",
            "relay",
            "--listen",
            &format!("0.0.0.0:{port}"),
            "--to",
            &format!("127.0.0.1:{gxserver_port}"),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command
        .spawn()
        .map_err(|error| format!("Can't start the relay: {error}"))?;
    let started = Instant::now();
    while std::net::TcpStream::connect(("127.0.0.1", port)).is_err() {
        if started.elapsed() > Duration::from_secs(10) {
            return Err("The relay did not start.".to_string());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}

/// Turns on the box gxserver's tailcat (its own sshd and gxserver ports, the same Easy Connect
/// sidecar a remote computer runs) for the user's devices only, and waits for its address.
///
/// CDXC:CloudBoxes 2026-11-02 DECISION: the user chose private access only (Q1021 "private
/// only"): a box publishes no port and answers only the user's own devices. A boot that asks for
/// tailcat without device keys is refused: a box reachable by none of the user's devices is no use
/// (an empty allow-list starts the helper closed, `--allow=none`, since 2026-11-08).
///
/// CDXC:CloudBoxes 2026-11-21 FIX:
/// GitHub #698: since 11.7.0 (7be3fc18d) no key can be added while Easy Connect is off, and the
/// boot sent the devices first and turned Easy Connect on second, so every Instabox box failed its
/// boot. Turning it on first would have run the helper on the list already saved (a snapshot's or
/// the last boot's devices) until the new one landed. The boot now sends one `enableForBoxDevices`
/// update: gxserver reads the devices from this box's ticket and saves them with Easy Connect on in
/// one write, so this update never serves anyone the ticket doesn't name. That update is refused on
/// a computer that isn't a box, so the off rule holds for every other caller. A box whose saved
/// state already has Easy Connect on (a woken box, one made from a snapshot) no longer starts its
/// helper when gxserver starts: gxserver drops the saved devices the ticket no longer names and
/// waits for this update (`BOX_DEVICES_FROM_BOOT` in tailcat/status.rs), which sets exactly the
/// ticket's list and starts the helper.
fn start_tailcat(ticket: &BoxBootTicket) -> Result<String, String> {
    if ticket.allow_device_keys.is_empty() {
        return Err(
            "The boot ticket names no device allowed to reach the box, so tailcat was not started."
                .to_string(),
        );
    }
    let flags = Flags::default();
    rpc::call_gxserver_rpc(
        "/api/updateTailcatState",
        &json!({ "kind": "enableForBoxDevices" }),
        &flags,
    )
    .map_err(|error| format!("The box's gxserver did not set up tailcat: {error}"))?;
    let started = Instant::now();
    loop {
        let status = rpc::call_gxserver_rpc("/api/tailcatStatus", &json!({}), &flags).ok();
        let status = status.as_ref().and_then(|value| value.get("status"));
        if let Some(token) = status
            .and_then(|status| status.get("token"))
            .and_then(Value::as_str)
            .filter(|token| !token.is_empty())
        {
            return Ok(token.to_string());
        }
        if started.elapsed() >= TAILCAT_WAIT {
            let reason = status
                .and_then(|status| status.get("lastError"))
                .and_then(Value::as_str)
                .unwrap_or("no address yet");
            return Err(format!(
                "tailcat did not start on the box within {} seconds ({reason}).",
                TAILCAT_WAIT.as_secs()
            ));
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// The devices this box's boot ticket lets reach it, for gxserver's `enableForBoxDevices` update.
/// Refused on a computer that isn't a box, for a ticket that doesn't ask for tailcat (`local`) and
/// for one that names no device.
pub(crate) fn ticket_device_keys() -> Result<Vec<String>, String> {
    let ticket = state::read_ticket().ok_or_else(|| super::not_in_box_mode().message)?;
    if !ticket.tailcat {
        return Err("This Cloud Box's boot ticket doesn't ask for Easy Connect.".to_string());
    }
    let keys = named_device_keys(ticket);
    if keys.is_empty() {
        return Err(
            "This Cloud Box's boot ticket names no device allowed to reach it, so Easy Connect stays off."
                .to_string(),
        );
    }
    Ok(keys)
}

/// The ticket's device keys, trimmed, without blanks or the `none` placeholder; none for a ticket
/// that doesn't ask for tailcat.
fn named_device_keys(ticket: BoxBootTicket) -> Vec<String> {
    if !ticket.tailcat {
        return Vec::new();
    }
    ticket
        .allow_device_keys
        .into_iter()
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty() && key != crate::tailcat::TAILCAT_ALLOW_NONE)
        .collect()
}

/// On a box, the devices its ticket names (possibly none); `None` on a computer that isn't a box.
/// A ticket file that can't be read names nobody, so the helper starts closed rather than on the
/// old list. For gxserver's start, which only ever takes saved devices off with it.
pub(crate) fn devices_ticket_names() -> Option<Vec<String>> {
    if !state::ticket_file().is_file() {
        return None;
    }
    Some(
        state::read_ticket()
            .map(named_device_keys)
            .unwrap_or_default(),
    )
}

/// `ghostex box forget-devices --keys <key,key> --home <id>`: takes those devices out of this box's
/// ticket and has gxserver take every saved device the ticket no longer names off Easy Connect
/// (`pruneToBoxTicket`, removal only). Refused unless `home` is the computer the ticket names, so
/// another computer seeing this box on a shared provider account can't take its devices off. Run
/// again, it changes nothing.
pub(crate) fn run_forget_devices(keys: &[String], home: Option<&str>) -> Result<Value, String> {
    let mut ticket = state::read_ticket().ok_or_else(|| super::not_in_box_mode().message)?;
    if home.is_none() || ticket.home_machine_id.as_deref() != home {
        return Err(
            "This box belongs to another computer, so only that computer can take devices off it."
                .to_string(),
        );
    }
    let before = ticket.allow_device_keys.len();
    ticket
        .allow_device_keys
        .retain(|key| !keys.iter().any(|gone| gone.trim() == key.trim()));
    let removed = before - ticket.allow_device_keys.len();
    if removed > 0 {
        state::write_json(&state::ticket_file(), &ticket)?;
    }
    if ticket.tailcat {
        rpc::call_gxserver_rpc(
            "/api/updateTailcatState",
            &json!({ "kind": "pruneToBoxTicket" }),
            &Flags::default(),
        )
        .map_err(|error| format!("The box's gxserver did not take the devices off: {error}"))?;
    }
    Ok(json!({ "removedFromTicket": removed }))
}

pub(crate) fn run_boot(ticket: &BoxBootTicket) -> Result<BoxBootResult, String> {
    if !ghostex_gx_cloud::ids::is_gx_id(&ticket.box_id) {
        return Err(format!("\"{}\" is not a box id.", ticket.box_id));
    }
    let bin = bin_dir()?;
    let (identity, identity_reset) = read_or_reset_identity(&ticket.box_id)?;
    state::write_json(&state::ticket_file(), ticket)?;
    let private = state::make_private_dir(&ticket.provider)?;
    let endpoint = start_gxserver(&bin, identity_reset)?;
    let gxserver_port = endpoint
        .get("port")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let tailcat_address = if ticket.tailcat {
        Some(start_tailcat(ticket)?)
    } else {
        start_relay(&bin, u16::try_from(gxserver_port).unwrap_or_default())?;
        None
    };
    idle::restart_watcher(&bin)?;
    Ok(BoxBootResult {
        box_id: ticket.box_id.clone(),
        machine_id: identity.machine_id,
        identity_reset,
        tailcat_address,
        gxserver_port,
        gxserver_token: endpoint
            .get("authToken")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        gxserver_version: crate::constants::GXSERVER_VERSION.to_string(),
        protocol_version: endpoint
            .get("protocolVersion")
            .and_then(Value::as_u64)
            .unwrap_or(crate::constants::GXSERVER_PROTOCOL_VERSION),
        private_dir: Some(private.to_string_lossy().into_owned()),
    })
}
