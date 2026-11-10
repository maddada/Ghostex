use std::{
    net::TcpListener,
    path::{Path, PathBuf},
    sync::Arc,
};

use ghostex_paths::tailcat_forward::{
    ClientEnd, ForwardEvent, ForwardProbe, ForwardSpec, TailcatForward,
};

use serde_json::Value;

use crate::ghostex_cli::rpc::{CliError, CliResult, GXSERVER_LOCAL_API_HOST};

/*
CDXC:RemotePairing 2026-09-01:
The CLI's ssh transport forwards a port with `ssh -N -L`. The forwarder here
is the tailcat equivalent: one loopback listener on the plan's port, relaying
every accepted connection to the tunnel's one `tailcat forward` client
(`ghostex_paths::tailcat_forward`, which the desktop app's forwarder runs too). That keeps the
rest of the RPC layer talking to a plain `http://127.0.0.1:<port>` base URL exactly like the ssh
arm does.
*/

#[derive(Clone, Debug)]
pub struct TailcatForwardPlan {
    pub address_blob: String,
    pub base_url: String,
    pub local_port: u16,
    pub remote_port: u16,
    /// A key of this computer's own for that computer (`computer_link_keys.rs`); `None` dials with
    /// tailcat's `client-default`, the key it paired with.
    pub client_key: Option<PathBuf>,
}

pub fn create_cli_tailcat_forward_plan(
    profile: &Value,
    local_port: u16,
    remote_port: u16,
) -> CliResult<TailcatForwardPlan> {
    let address_blob = profile
        .get("tailcatToken")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    if address_blob.is_empty() {
        let label = profile
            .get("name")
            .or_else(|| profile.get("id"))
            .and_then(Value::as_str)
            .unwrap_or("undefined");
        return Err(CliError::Other(format!(
            "gxserver profile \"{label}\" is missing tailcatToken. Copy the remote's tailcat address from its Ghostex tailcat settings."
        )));
    }
    Ok(TailcatForwardPlan {
        address_blob,
        base_url: format!("http://{GXSERVER_LOCAL_API_HOST}:{local_port}"),
        local_port,
        remote_port,
        client_key: None,
    })
}

/// Handle for a running loopback forwarder. Dropping it does nothing: the
/// tunnel registry in `rpc` owns the lifetime and calls `stop` from the same
/// idle-kill path that kills ssh tunnel children.
pub struct TailcatTunnel {
    forward: TailcatForward,
}

impl TailcatTunnel {
    pub fn stop(&self) {
        self.forward.stop();
    }
}

/// What a tunnel's own check asks the computer's gxserver: it answers without a token.
const TUNNEL_PROBE_PATH: &str = "/api/health";

pub fn start_tailcat_tunnel(
    binary_path: &Path,
    plan: &TailcatForwardPlan,
) -> CliResult<TailcatTunnel> {
    let listener = TcpListener::bind((GXSERVER_LOCAL_API_HOST, plan.local_port)).map_err(
        |error| {
            CliError::Connection(format!(
                "Could not bind the tailcat gxserver tunnel to {GXSERVER_LOCAL_API_HOST}:{}: {error}",
                plan.local_port
            ))
        },
    )?;
    let spec = ForwardSpec {
        binary_path: binary_path.to_path_buf(),
        address_blob: plan.address_blob.clone(),
        remote_port: plan.remote_port,
        probe: ForwardProbe::HttpGet(TUNNEL_PROBE_PATH),
    };
    let client_key = plan.client_key.clone();
    let forward = TailcatForward::start(
        listener,
        spec,
        Arc::new(move || client_key.clone()),
        Arc::new(log_tunnel_event),
    )?;
    Ok(TailcatTunnel { forward })
}

/// One line on stderr (gxserver's launch log, or the terminal of a `ghostex` command) when a
/// tunnel's tailcat client ends for a reason of its own.
fn log_tunnel_event(event: ForwardEvent) {
    let ForwardEvent::ClientEnded {
        reason,
        uptime,
        connections_cut,
    } = event
    else {
        return;
    };
    if matches!(reason, ClientEnd::Stopped | ClientEnd::Refreshed) {
        return;
    }
    eprintln!(
        "[gxserver] Easy Connect tunnel: its helper ended ({}) after {} s with {connections_cut} open connection(s); starting it again",
        reason.as_str(),
        uptime.as_secs()
    );
}
