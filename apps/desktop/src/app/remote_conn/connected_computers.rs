// Tells this computer's gxserver which machines the app is connected to, and tells each machine
// how its agents reach this computer back, so agents on either side can use the other.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::app::helpers::*;
use crate::*;

/// How often a connected machine is registered again; the registration lasts three times as long,
/// so a missed renewal never cuts an agent off.
const RENEW_EVERY: Duration = Duration::from_secs(60);
const REGISTRATION_TTL_MS: u64 = 180_000;
/// After a machine refused the way back (an sshd without remote forwarding), it is tried again
/// this much later instead of on every renewal.
const REVERSE_TUNNEL_RETRY: Duration = Duration::from_secs(10 * 60);
const RPC_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Default)]
struct RegistrationState {
    last_renewal: Option<Instant>,
    /// machine id → the server id it was registered under, to unregister it when it disconnects.
    server_ids: HashMap<String, String>,
    /// machine id → when the way back may be tried again.
    reverse_retry_at: HashMap<String, Instant>,
}

fn state() -> MutexGuard<'static, RegistrationState> {
    static STATE: OnceLock<Mutex<RegistrationState>> = OnceLock::new();
    STATE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

struct RegistrationJob {
    machine_id: String,
    name: String,
    agent_access: bool,
    target: GpuiRemoteGxserverRequestTarget,
    reverse_port: Option<u16>,
    open_reverse: bool,
}

struct RegistrationOutcome {
    machine_id: String,
    local_port: u16,
    opened_reverse: Option<GpuiRemoteReverseTunnel>,
    reverse_failed: bool,
}

/// The Agent access switch of a machine in Settings > Remote > Remote machines (on unless the
/// user turned it off).
pub(crate) fn gpui_remote_machine_agent_access(remote_machine_id: &str) -> bool {
    shared_settings::shared_sidebar_settings_snapshot()
        .object()
        .get("remoteMachines")
        .and_then(Value::as_array)
        .and_then(|machines| {
            machines.iter().find(|machine| {
                gpui_remote_machine_id_from_value(machine).as_deref() == Some(remote_machine_id)
            })
        })
        .and_then(|machine| machine.get("agentAccess"))
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

/// A machine's Agent access switch differs between two saved settings.
pub(crate) fn gpui_remote_machine_agent_access_changed(
    previous: &serde_json::Map<String, Value>,
    next: &serde_json::Map<String, Value>,
) -> bool {
    let switches = |settings: &serde_json::Map<String, Value>| {
        settings
            .get("remoteMachines")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|machine| {
                Some((
                    gpui_remote_machine_id_from_value(machine)?,
                    machine
                        .get("agentAccess")
                        .and_then(Value::as_bool)
                        .unwrap_or(true),
                ))
            })
            .collect::<HashMap<_, _>>()
    };
    switches(previous) != switches(next)
}

impl GhostexGpuiApp {
    /// Registers every connected machine with this computer's gxserver (and this computer with
    /// each machine), at most once a minute unless `force`: a new connection and a changed Agent
    /// access switch register at once.
    pub(crate) fn renew_connected_computers(&mut self, force: bool, cx: &mut gpui::Context<Self>) {
        {
            let mut registration = state();
            if !force
                && registration
                    .last_renewal
                    .is_some_and(|at| at.elapsed() < RENEW_EVERY)
            {
                return;
            }
            registration.last_renewal = Some(Instant::now());
        }
        let now = Instant::now();
        let jobs: Vec<RegistrationJob> = self
            .remote_gxserver_connections
            .iter()
            .filter(|(_, connection)| !connection.this_computer)
            .map(|(machine_id, connection)| {
                let agent_access = gpui_remote_machine_agent_access(machine_id);
                RegistrationJob {
                    machine_id: machine_id.clone(),
                    name: gpui_remote_machine_name_from_settings(machine_id)
                        .unwrap_or_else(|| machine_id.clone()),
                    agent_access,
                    target: connection.request_target(),
                    reverse_port: connection
                        .reverse_tunnel
                        .as_ref()
                        .map(|reverse| reverse.remote_port),
                    open_reverse: agent_access
                        && connection.reverse_tunnel.is_none()
                        && state()
                            .reverse_retry_at
                            .get(machine_id)
                            .is_none_or(|at| *at <= now),
                }
            })
            .collect();
        if jobs.is_empty() {
            return;
        }
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let outcomes = background
                .spawn(async move { jobs.into_iter().map(register_machine).collect::<Vec<_>>() })
                .await;
            let _ = this.update(cx, |this, _cx| {
                for outcome in outcomes {
                    if outcome.reverse_failed {
                        state().reverse_retry_at.insert(
                            outcome.machine_id.clone(),
                            Instant::now() + REVERSE_TUNNEL_RETRY,
                        );
                    }
                    let Some(mut reverse) = outcome.opened_reverse else {
                        continue;
                    };
                    // The connection it was opened for may have been replaced or closed meanwhile.
                    match this
                        .remote_gxserver_connections
                        .get_mut(outcome.machine_id.as_str())
                        .filter(|connection| {
                            connection.local_port == outcome.local_port
                                && connection.reverse_tunnel.is_none()
                        }) {
                        Some(connection) => connection.reverse_tunnel = Some(reverse),
                        None => reverse.terminate(),
                    }
                }
            });
        })
        .detach();
    }

    /// Forgets a machine whose connection ended, so this computer's agents stop reaching it and
    /// its agents stop reaching this computer at once.
    pub(crate) fn unregister_connected_computer(&mut self, remote_machine_id: &str) {
        let Some(server_id) = state().server_ids.remove(remote_machine_id) else {
            return;
        };
        let _ = std::thread::Builder::new()
            .name("ghostex-unregister-computer".into())
            .spawn(move || {
                let _ = gxserver_post_typed_operation(
                    "/api/unregisterConnectedComputer",
                    &json!({ "serverId": server_id }),
                    RPC_TIMEOUT,
                );
            });
    }
}

/// One machine's registration, run on the background executor. Never logs or returns a token.
fn register_machine(job: RegistrationJob) -> RegistrationOutcome {
    let mut outcome = RegistrationOutcome {
        machine_id: job.machine_id.clone(),
        local_port: job.target.local_port,
        opened_reverse: None,
        reverse_failed: false,
    };
    let Some(remote_server_id) = remote_server_id(&job.target) else {
        return outcome;
    };
    let Some(local) = post_local(
        "/api/registerConnectedComputer",
        json!({
            "serverId": remote_server_id,
            "name": job.name,
            "baseUrl": format!("http://127.0.0.1:{}", job.target.local_port),
            "token": job.target.token,
            "agentAccess": job.agent_access,
            "ttlMs": REGISTRATION_TTL_MS,
        }),
    ) else {
        // An older gxserver here has no registry: nothing to tell the machine either.
        return outcome;
    };
    state()
        .server_ids
        .insert(job.machine_id.clone(), remote_server_id.clone());
    let local_server_id = local
        .get("serverId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let local_name = local
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("This computer")
        .to_string();
    let Some(peer_token) = local.get("peerToken").and_then(Value::as_str) else {
        return outcome;
    };
    let mut reverse_port = job.reverse_port;
    if reverse_port.is_none() && job.open_reverse {
        match open_reverse_tunnel(&job.machine_id, &job.target) {
            Some(reverse) => {
                reverse_port = Some(reverse.remote_port);
                outcome.opened_reverse = Some(reverse);
            }
            None => outcome.reverse_failed = true,
        }
    }
    let Some(reverse_port) = reverse_port else {
        return outcome;
    };
    // A machine whose gxserver predates connected computers answers notFound; it keeps working
    // for this computer's agents, its own just cannot reach back until it is updated.
    let _ = gpui_remote_gxserver_post_typed_operation(
        &job.target,
        "/api/registerConnectedComputer",
        &json!({
            "serverId": local_server_id,
            "name": local_name,
            "baseUrl": format!("http://127.0.0.1:{reverse_port}"),
            "token": peer_token,
            "direction": "inbound",
            "agentAccess": job.agent_access,
            "ttlMs": REGISTRATION_TTL_MS,
        }),
        RPC_TIMEOUT,
    );
    outcome
}

fn open_reverse_tunnel(
    machine_id: &str,
    target: &GpuiRemoteGxserverRequestTarget,
) -> Option<GpuiRemoteReverseTunnel> {
    let settings = shared_settings::shared_sidebar_settings_snapshot();
    let config = gpui_remote_machine_config_from_settings(settings.object(), machine_id)?;
    gpui_open_remote_reverse_tunnel(&config, &target.execution_target)
}

fn post_local(path: &str, params: Value) -> Option<Value> {
    let (status, body) = gxserver_post_typed_operation(path, &params, RPC_TIMEOUT).ok()?;
    if !(200..300).contains(&status) {
        return None;
    }
    serde_json::from_str::<Value>(&body)
        .ok()?
        .get("result")
        .cloned()
}

/// The machine's server id, from its `/api/health/server` answer through the tunnel.
fn remote_server_id(target: &GpuiRemoteGxserverRequestTarget) -> Option<String> {
    let address = format!("127.0.0.1:{}", target.local_port);
    let mut stream = TcpStream::connect(address.as_str()).ok()?;
    stream.set_read_timeout(Some(RPC_TIMEOUT)).ok()?;
    stream.set_write_timeout(Some(RPC_TIMEOUT)).ok()?;
    let request = format!(
        "GET /api/health/server HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nAuthorization: Bearer {}\r\n{GPUI_GXSERVER_PROTOCOL_HEADER}: {GPUI_GXSERVER_PROTOCOL_VERSION}\r\n\r\n",
        target.token
    );
    stream.write_all(request.as_bytes()).ok()?;
    let mut response = String::new();
    stream.read_to_string(&mut response).ok()?;
    let (headers, body) = response.split_once("\r\n\r\n")?;
    let body = gxserver_http_response_body(headers, body).ok()?;
    serde_json::from_str::<Value>(&body)
        .ok()?
        .get("serverId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|id| !id.is_empty())
}
