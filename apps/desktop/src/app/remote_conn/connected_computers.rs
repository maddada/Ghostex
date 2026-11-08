// Tells this computer's gxserver which machines the app is connected to, and tells each machine
// how its agents reach this computer back, so agents on either side can use the other.

use std::collections::{HashMap, HashSet};
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

/// One machine's registration, kept per machine: each window holds its own connections, so one
/// window's renewal or close must not decide for another's.
#[derive(Default)]
struct MachineRegistration {
    last_renewal: Option<Instant>,
    /// The server id it was registered under, to unregister it when its last connection ends.
    server_id: Option<String>,
    /// The connections (by their tunnel's local port) that still reach it, across windows.
    holders: HashSet<u16>,
    /// When the way back may be tried again after the machine refused it.
    reverse_retry_at: Option<Instant>,
}

fn state() -> MutexGuard<'static, HashMap<String, MachineRegistration>> {
    static STATE: OnceLock<Mutex<HashMap<String, MachineRegistration>>> = OnceLock::new();
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
    /// The live way back: its port on the machine and the listener here it lands on.
    reverse: Option<(u16, u16)>,
    may_open_reverse: bool,
    /// Mint new tokens on both sides: the way back is about to be (re)opened.
    rotate: bool,
}

struct RegistrationOutcome {
    machine_id: String,
    local_port: u16,
    opened_reverse: Option<GpuiRemoteReverseTunnel>,
    reverse_failed: bool,
    /// The machine's own switch is off for this computer: the way back closes here too.
    close_reverse: bool,
}

/// The gxserver server id of a connected remote machine, once its registration learned it.
pub(crate) fn gpui_remote_machine_server_id(remote_machine_id: &str) -> Option<String> {
    state()
        .get(remote_machine_id)
        .and_then(|machine| machine.server_id.clone())
}

/// The connected remote machine whose gxserver has `server_id`, the way gxserver names another
/// computer's session (`serverId:projectId:sessionId`).
pub(crate) fn gpui_remote_machine_for_server_id(server_id: &str) -> Option<String> {
    state()
        .iter()
        .find(|(_, machine)| machine.server_id.as_deref() == Some(server_id))
        .map(|(machine_id, _)| machine_id.clone())
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
    /// each machine), each at most once a minute unless `force`: a new connection, a changed Agent
    /// access switch and a way back that died register at once.
    pub(crate) fn renew_connected_computers(&mut self, force: bool, cx: &mut gpui::Context<Self>) {
        let now = Instant::now();
        let mut jobs = Vec::new();
        for (machine_id, connection) in self.remote_gxserver_connections.iter_mut() {
            if connection.this_computer {
                continue;
            }
            let agent_access = gpui_remote_machine_agent_access(machine_id);
            // A dead way back is dropped here and reopened below with new tokens; with Agent
            // access off the way back is closed at once.
            let mut reverse_changed = false;
            if connection
                .reverse_tunnel
                .as_mut()
                .is_some_and(|reverse| !agent_access || !reverse.is_alive())
            {
                if let Some(mut reverse) = connection.reverse_tunnel.take() {
                    reverse.terminate();
                }
                reverse_changed = true;
            }
            let mut registration = state();
            let machine = registration.entry(machine_id.clone()).or_default();
            // A connection not seen before is a (re)connect: its tokens are new either way.
            let new_connection = machine.holders.insert(connection.local_port);
            if !renewal_due(machine, force || reverse_changed || new_connection, now) {
                continue;
            }
            machine.last_renewal = Some(now);
            let may_open_reverse = agent_access
                && connection.reverse_tunnel.is_none()
                && machine.reverse_retry_at.is_none_or(|at| at <= now);
            jobs.push(RegistrationJob {
                machine_id: machine_id.clone(),
                name: gpui_remote_machine_name_from_settings(machine_id)
                    .unwrap_or_else(|| machine_id.clone()),
                agent_access,
                target: connection.request_target(),
                reverse: connection
                    .reverse_tunnel
                    .as_ref()
                    .map(|reverse| (reverse.remote_port, reverse.peer_port)),
                may_open_reverse,
                rotate: may_open_reverse || new_connection,
            });
        }
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
                    if outcome.close_reverse {
                        if let Some(connection) = this
                            .remote_gxserver_connections
                            .get_mut(outcome.machine_id.as_str())
                        {
                            if let Some(mut reverse) = connection.reverse_tunnel.take() {
                                reverse.terminate();
                            }
                        }
                    }
                    if outcome.reverse_failed {
                        if let Some(machine) = state().get_mut(&outcome.machine_id) {
                            machine.reverse_retry_at = Some(Instant::now() + REVERSE_TUNNEL_RETRY);
                        }
                    }
                    let Some(mut reverse) = outcome.opened_reverse else {
                        continue;
                    };
                    // The connection it was opened for may have been replaced or closed meanwhile;
                    // a way back that replaced an older one (this computer's gxserver restarted on
                    // a new port) closes the old one.
                    match this
                        .remote_gxserver_connections
                        .get_mut(outcome.machine_id.as_str())
                        .filter(|connection| connection.local_port == outcome.local_port)
                    {
                        Some(connection) => {
                            if let Some(mut old) = connection.reverse_tunnel.replace(reverse) {
                                old.terminate();
                            }
                        }
                        None => reverse.terminate(),
                    }
                }
            });
        })
        .detach();
    }

    /// The connection on `local_port` to a machine ended. The machine is forgotten (so this
    /// computer's agents stop reaching it, and its agents stop reaching this computer) only when no
    /// other window still connects to it; otherwise that window registers its own connection again
    /// on its next tick.
    pub(crate) fn unregister_connected_computer(
        &mut self,
        remote_machine_id: &str,
        local_port: u16,
    ) {
        let Some(server_id) = release_connection(&mut state(), remote_machine_id, local_port)
        else {
            return;
        };
        let _ = std::thread::Builder::new()
            .name("ghostex-unregister-computer".into())
            .spawn(move || {
                for path in [
                    "/api/unregisterConnectedComputer",
                    "/api/revokeComputerGrant",
                ] {
                    let _ = gxserver_post_typed_operation(
                        path,
                        &json!({ "serverId": server_id }),
                        RPC_TIMEOUT,
                    );
                }
            });
    }
}

/// Whether a machine's registration is renewed this tick: always when `now_needed`, else once a
/// minute, each machine on its own clock (one window's renewal never stands in for another's).
fn renewal_due(machine: &MachineRegistration, now_needed: bool, now: Instant) -> bool {
    now_needed
        || machine
            .last_renewal
            .is_none_or(|at| now.saturating_duration_since(at) >= RENEW_EVERY)
}

/// Lets go of the connection on `local_port`. Returns the server id to forget when it was the
/// machine's last connection in any window; otherwise the machine is renewed on the next tick, so
/// the window still connected registers its own tunnel again.
fn release_connection(
    registration: &mut HashMap<String, MachineRegistration>,
    machine_id: &str,
    local_port: u16,
) -> Option<String> {
    let machine = registration.get_mut(machine_id)?;
    machine.holders.remove(&local_port);
    if !machine.holders.is_empty() {
        machine.last_renewal = None;
        return None;
    }
    registration
        .remove(machine_id)
        .and_then(|machine| machine.server_id)
}

/// A grant minted by `issueComputerGrant`: the token, the minting gxserver's way-back port, and
/// who minted it.
struct Grant {
    token: String,
    peer_port: Option<u16>,
    server_id: String,
    name: String,
}

fn grant_from(result: &Value) -> Option<Grant> {
    Some(Grant {
        token: result.get("peerToken")?.as_str()?.to_string(),
        peer_port: result
            .get("peerPort")
            .and_then(Value::as_u64)
            .and_then(|port| u16::try_from(port).ok()),
        server_id: result.get("serverId")?.as_str()?.to_string(),
        name: result
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("This computer")
            .to_string(),
    })
}

enum Grants {
    Issued(Grant, Grant),
    /// The machine's own Agent access switch is off for this computer.
    DeniedThere,
    /// One side's gxserver predates connected computers.
    Unsupported,
}

/// The tokens each side gives the other, minted (or renewed) on this computer (as its own app,
/// `owner`, which lifts a deny this computer's switch set) and on the machine (as another
/// computer's app, which the machine refuses while its own switch is off).
fn issue_grants(job: &RegistrationJob, remote_server_id: &str, rotate: bool) -> Grants {
    let Some(local) = post_local(
        "/api/issueComputerGrant",
        json!({ "serverId": remote_server_id, "name": job.name, "rotate": rotate, "owner": true, "ttlMs": REGISTRATION_TTL_MS }),
    )
    .as_ref()
    .and_then(grant_from) else {
        return Grants::Unsupported;
    };
    let Some(remote) = post_remote(
        &job.target,
        "/api/issueComputerGrant",
        json!({ "serverId": local.server_id, "name": local.name, "rotate": rotate, "ttlMs": REGISTRATION_TTL_MS }),
    ) else {
        return Grants::Unsupported;
    };
    if remote.get("denied").and_then(Value::as_bool) == Some(true) {
        return Grants::DeniedThere;
    }
    match grant_from(&remote) {
        Some(remote) => Grants::Issued(local, remote),
        None => Grants::Unsupported,
    }
}

/// One machine's registration, run on the background executor. Never logs or returns a token.
///
/// CDXC:RemoteMachines 2026-11-03 WHY:
/// Each side mints its own token for the other (`issueComputerGrant`), limited to the agent allowlist, so this computer's gxserver never holds the machine's full token and its relay cannot reach the machine's settings or controls. The way back is opened only after the machine answered, so an older gxserver there gets no tunnel it cannot use; it is closed (and both tokens dropped) when Agent access goes off on either computer, and every new way back gets new tokens. Calls to this computer's own gxserver say `owner`: only they set or lift this computer's deny, so when two computers connect to each other, one app's renewals never undo the other's switch.
fn register_machine(job: RegistrationJob) -> RegistrationOutcome {
    let mut outcome = RegistrationOutcome {
        machine_id: job.machine_id.clone(),
        local_port: job.target.local_port,
        opened_reverse: None,
        reverse_failed: false,
        close_reverse: false,
    };
    let Some(remote_server_id) = remote_server_id(&job.target) else {
        return outcome;
    };
    if let Some(machine) = state().get_mut(&job.machine_id) {
        machine.server_id = Some(remote_server_id.clone());
    }
    if !job.agent_access {
        // Registered with access off so refusals on either side can name it; no tokens remain,
        // and the deny keeps the machine's own renewals from minting one here again.
        let _ = post_local(
            "/api/revokeComputerGrant",
            json!({ "serverId": remote_server_id, "deny": true, "ttlMs": REGISTRATION_TTL_MS }),
        );
        let _ = post_local(
            "/api/registerConnectedComputer",
            json!({ "serverId": remote_server_id, "name": job.name, "agentAccess": false, "owner": true, "ttlMs": REGISTRATION_TTL_MS }),
        );
        if let Some(local) = post_local("/api/listConnectedComputers", json!({})) {
            let local_server_id = local
                .get("serverId")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let local_name = local
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("This computer");
            let _ = post_remote(
                &job.target,
                "/api/revokeComputerGrant",
                json!({ "serverId": local_server_id }),
            );
            let _ = post_remote(
                &job.target,
                "/api/registerConnectedComputer",
                json!({ "serverId": local_server_id, "name": local_name, "direction": "inbound", "agentAccess": false, "ttlMs": REGISTRATION_TTL_MS }),
            );
        }
        return outcome;
    }
    let (mut local, mut remote) = match issue_grants(&job, &remote_server_id, job.rotate) {
        Grants::Issued(local, remote) => (local, remote),
        Grants::DeniedThere => {
            off_on_other_computer(&remote_server_id, &job.name);
            outcome.close_reverse = true;
            return outcome;
        }
        Grants::Unsupported => {
            // One side predates connected computers: agents can't use this machine, and nothing is
            // left registered that could suggest they can.
            let _ = post_local(
                "/api/unregisterConnectedComputer",
                json!({ "serverId": remote_server_id }),
            );
            let _ = post_local(
                "/api/revokeComputerGrant",
                json!({ "serverId": remote_server_id }),
            );
            return outcome;
        }
    };
    // The way back: kept while it lands on this gxserver's current way-back listener, else
    // (re)opened, with new tokens on both sides when it was not opened with them already.
    let mut reverse_port = job
        .reverse
        .filter(|(_, peer_port)| Some(*peer_port) == local.peer_port)
        .map(|(remote_port, _)| remote_port);
    if reverse_port.is_none() && (job.may_open_reverse || job.reverse.is_some()) {
        if !job.rotate {
            match issue_grants(&job, &remote_server_id, true) {
                Grants::Issued(rotated_local, rotated_remote) => {
                    (local, remote) = (rotated_local, rotated_remote);
                }
                Grants::DeniedThere => {
                    off_on_other_computer(&remote_server_id, &job.name);
                    outcome.close_reverse = true;
                    return outcome;
                }
                Grants::Unsupported => {}
            }
        }
        match local
            .peer_port
            .and_then(|peer_port| open_reverse_tunnel(&job.machine_id, &job.target, peer_port))
        {
            Some(reverse) => {
                reverse_port = Some(reverse.remote_port);
                outcome.opened_reverse = Some(reverse);
            }
            None => outcome.reverse_failed = true,
        }
    }
    let _ = post_local(
        "/api/registerConnectedComputer",
        json!({
            "serverId": remote_server_id,
            "name": job.name,
            "baseUrl": format!("http://127.0.0.1:{}", job.target.local_port),
            "token": remote.token,
            "owner": true,
            "ttlMs": REGISTRATION_TTL_MS,
            // A message held for this machine while it was offline names it by this id when the
            // app had not learned its server id yet (server/src/connected_computers/held_sends.rs).
            "machineId": job.machine_id,
        }),
    );
    if let Some(reverse_port) = reverse_port {
        let _ = post_remote(
            &job.target,
            "/api/registerConnectedComputer",
            json!({
                "serverId": local.server_id,
                "name": local.name,
                "baseUrl": format!("http://127.0.0.1:{reverse_port}"),
                "token": local.token,
                "direction": "inbound",
                "ttlMs": REGISTRATION_TTL_MS,
            }),
        );
    }
    outcome
}

/// The machine turned Agent access off for this computer, so the pair is off here too: drop the
/// token this computer gave it (with no deny of this computer's own) and register it off, naming
/// the machine's switch.
fn off_on_other_computer(remote_server_id: &str, name: &str) {
    let _ = post_local(
        "/api/revokeComputerGrant",
        json!({ "serverId": remote_server_id }),
    );
    let _ = post_local(
        "/api/registerConnectedComputer",
        json!({
            "serverId": remote_server_id,
            "name": name,
            "agentAccess": false,
            "owner": true,
            "offOnOtherComputer": true,
            "ttlMs": REGISTRATION_TTL_MS,
        }),
    );
}

fn open_reverse_tunnel(
    machine_id: &str,
    target: &GpuiRemoteGxserverRequestTarget,
    peer_port: u16,
) -> Option<GpuiRemoteReverseTunnel> {
    let settings = shared_settings::shared_sidebar_settings_snapshot();
    let config = gpui_remote_machine_config_from_settings(settings.object(), machine_id)?;
    gpui_open_remote_reverse_tunnel(&config, &target.execution_target, peer_port)
}

fn post_local(path: &str, params: Value) -> Option<Value> {
    let (status, body) = gxserver_post_typed_operation(path, &params, RPC_TIMEOUT).ok()?;
    result_of(status, &body)
}

fn post_remote(
    target: &GpuiRemoteGxserverRequestTarget,
    path: &str,
    params: Value,
) -> Option<Value> {
    let (status, body) =
        gpui_remote_gxserver_post_typed_operation(target, path, &params, RPC_TIMEOUT).ok()?;
    result_of(status, &body)
}

fn result_of(status: u16, body: &str) -> Option<Value> {
    if !(200..300).contains(&status) {
        return None;
    }
    serde_json::from_str::<Value>(body)
        .ok()?
        .get("result")
        .cloned()
}

/// The machine's server id, from its `/api/health/server` answer through the tunnel.
fn remote_server_id(target: &GpuiRemoteGxserverRequestTarget) -> Option<String> {
    let address = format!("127.0.0.1:{}", target.local_port);
    ghostex_paths::loopback_owner::check_host_port("127.0.0.1", target.local_port).ok()?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_machine_is_forgotten_only_when_its_last_window_lets_go() {
        let mut registration = HashMap::new();
        let machine: &mut MachineRegistration =
            registration.entry("omarchy".to_string()).or_default();
        machine.server_id = Some("S4k".into());
        machine.holders.extend([50001, 50002]);
        machine.last_renewal = Some(Instant::now());
        assert_eq!(
            release_connection(&mut registration, "omarchy", 50001),
            None
        );
        let kept = &registration["omarchy"];
        assert!(kept.holders.contains(&50002));
        assert!(
            kept.last_renewal.is_none(),
            "the other window re-registers on its next tick"
        );
        assert_eq!(
            release_connection(&mut registration, "omarchy", 50002).as_deref(),
            Some("S4k")
        );
        assert!(registration.is_empty());
        assert_eq!(
            release_connection(&mut registration, "omarchy", 50002),
            None
        );
    }

    #[test]
    fn a_machine_and_its_gxserver_server_id_name_each_other() {
        state()
            .entry("remote-voice-test".to_string())
            .or_default()
            .server_id = Some("S9voice".into());
        assert_eq!(
            gpui_remote_machine_server_id("remote-voice-test").as_deref(),
            Some("S9voice")
        );
        assert_eq!(
            gpui_remote_machine_for_server_id("S9voice").as_deref(),
            Some("remote-voice-test")
        );
        assert_eq!(gpui_remote_machine_for_server_id("S9unknown"), None);
        state().remove("remote-voice-test");
    }

    #[test]
    fn each_machine_renews_on_its_own_clock() {
        let now = Instant::now();
        let fresh = MachineRegistration {
            last_renewal: Some(now),
            ..Default::default()
        };
        let stale = MachineRegistration {
            last_renewal: now.checked_sub(RENEW_EVERY + Duration::from_secs(1)),
            ..Default::default()
        };
        assert!(!renewal_due(&fresh, false, now));
        assert!(
            renewal_due(&fresh, true, now),
            "a new connection or a dead way back renews now"
        );
        assert!(renewal_due(&stale, false, now));
        assert!(renewal_due(&MachineRegistration::default(), false, now));
    }
}
