//! `ghostex box boot --ticket <ticket>`: start the box's gxserver (and, without tailcat, the
//! relay Docker publishes) and report how to reach it.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use ghostex_gx_protocol::cloud_box::{BoxBootResult, BoxBootTicket};
use serde_json::Value;

const ENDPOINT_WAIT: Duration = Duration::from_secs(60);

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

fn identity_file() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/root"));
    home.join(".ghostex-box").join("identity.json")
}

fn read_or_reset_identity(box_id: &str) -> Result<(BoxIdentity, bool), String> {
    let file = identity_file();
    if let Some(identity) = std::fs::read_to_string(&file)
        .ok()
        .and_then(|text| serde_json::from_str::<BoxIdentity>(&text).ok())
        .filter(|identity| identity.box_id == box_id)
    {
        return Ok((identity, false));
    }
    let existed = file.is_file();
    let mut entropy = [0u8; 16];
    rand::Rng::fill_bytes(&mut rand::rng(), &mut entropy);
    let identity = BoxIdentity {
        box_id: box_id.to_string(),
        machine_id: ghostex_gx_cloud::ids::gx_id_from_entropy(&entropy),
    };
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Can't write the box identity: {error}"))?;
    }
    std::fs::write(&file, serde_json::to_vec(&identity).unwrap_or_default())
        .map_err(|error| format!("Can't write the box identity: {error}"))?;
    Ok((identity, existed))
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

/// Starts gxserver unless it runs, and waits for its endpoint (port and token).
fn start_gxserver(bin: &Path) -> Result<Value, String> {
    let gxserver = bin.join("gxserver");
    if let Ok(endpoint) = run_json(&gxserver, &["endpoint"]) {
        if endpoint.get("ok").and_then(Value::as_bool) == Some(true) {
            return Ok(endpoint);
        }
    }
    run_json(&gxserver, &["start", "--json"])?;
    let started = Instant::now();
    loop {
        match run_json(&gxserver, &["endpoint"]) {
            Ok(endpoint) if endpoint.get("ok").and_then(Value::as_bool) == Some(true) => {
                return Ok(endpoint)
            }
            result if started.elapsed() >= ENDPOINT_WAIT => {
                return Err(format!(
                    "gxserver did not start on the box within {} seconds ({}).",
                    ENDPOINT_WAIT.as_secs(),
                    result.err().unwrap_or_default()
                ))
            }
            _ => std::thread::sleep(Duration::from_millis(250)),
        }
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

pub(crate) fn run_boot(ticket: &BoxBootTicket) -> Result<BoxBootResult, String> {
    if !ghostex_gx_cloud::ids::is_gx_id(&ticket.box_id) {
        return Err(format!("\"{}\" is not a box id.", ticket.box_id));
    }
    let bin = bin_dir()?;
    let (identity, identity_reset) = read_or_reset_identity(&ticket.box_id)?;
    let endpoint = start_gxserver(&bin)?;
    let gxserver_port = endpoint
        .get("port")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    if !ticket.tailcat {
        start_relay(&bin, u16::try_from(gxserver_port).unwrap_or_default())?;
    }
    Ok(BoxBootResult {
        box_id: ticket.box_id.clone(),
        machine_id: identity.machine_id,
        identity_reset,
        tailcat_address: None,
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
    })
}
