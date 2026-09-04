use std::{fs, sync::Mutex, time::SystemTime};

#[cfg(unix)]
use std::{
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use serde_json::{json, Map, Value};

use crate::{
    domain::{DomainRepository, DomainStateError},
    logging::{GxserverLogInput, GxserverLogger, LogLevel},
};

use super::*;

/*
CDXC:ZmxWireGeneration 2026-08-23:
zmx's IPC tags are a wire contract between the bundled binary and the daemons
it spawned, and that contract is deliberately broken from time to time — the
Ghostex fork's five tags moved from 14-18 to 19-23 when upstream claimed 14-18
for labels and Send, and an earlier upstream Resize change had the same effect.
A daemon keeps running the code of the binary that STARTED it, so after an app
or remote-package update the freshly installed client and the surviving daemons
cannot talk at all: `zmx attach` renders a blank pane and every new-tag request
is ignored. Replacing the binary in place (the rsync install, `gxserver setup`
on a remote) makes this the normal upgrade path, not an edge case.

Cycling a daemon kills the agent running inside it: the PTY hangs up, the
agent CLI dies mid-turn, and only its conversation comes back through the
saved resume command — background subagents and jobs do not.

CDXC:ZmxWireGeneration 2026-09-03:
The first version of this pass stamped the IDENTITY of the spawning binary
(cdhash on macOS, sha256 elsewhere) and cycled on any difference. That
conflates "the code changed" with "the wire broke": on 2026-09-03 three zmx
rebuilds that only ADDED tags (Visibility=26, GridInfo=27, which old daemons
drop through their `_` arm) cycled 57 sessions in one day, several with agents
mid-task. Widening the Visibility byte from `hidden: bool` to visible/chat/parked
on 2026-09-05 also stayed generation 1: 8.8.0 shipped the boolean, but the payload
is still 9 bytes and an old daemon reads chat and parked as hidden, which is the
resting-grid behaviour it already had.
The stamp is now the explicit wire-generation number zmx declares
(`WIRE_GENERATION` in `.dependencies/zmx/src/ipc.zig`, printed by
`zmx version` as `wire_generation\t<n>`). A daemon is cycled only when the
generation it was spawned with differs from the bundled binary's, so an
additive or internal zmx change installs without touching live sessions, and
the rules for when to bump the number live next to the constant.

Migration: a session stamped by the binary-identity scheme (`zmxBinaryStamp`)
was spawned by a binary from 2026-08-23 or later, and every such binary speaks
generation 1, so it counts as generation 1 rather than being cycled again. A
live daemon with NO stamp at all predates the renumbering and is cycled once,
as before.

Detection never probes the daemon: a busy daemon that misses an IPC timeout
must not be mistaken for an incompatible one. Comparing recorded and current
generations also handles the reverse skew (an older gxserver meeting daemons a
newer one spawned) without either side knowing which is newer.

Cycling leaves the session exactly as an auto-sleep would, so the existing
restore machinery — the wake-on-open pass, `/api/wakeSession` spawning the
provider server-side, the saved agent resume commands in the launch script —
brings it back with its conversation intact. Nothing is eagerly respawned here.
*/

/// `providerState` key holding the wire generation of the zmx binary that
/// spawned the live daemon behind this session.
pub(crate) const ZMX_WIRE_GENERATION_KEY: &str = "zmxWireGeneration";

/// `providerState` key of the retired binary-identity stamp. Still read so a
/// daemon stamped by the old scheme is recognised as generation 1, and removed
/// whenever a session's provider state is rewritten.
pub(crate) const LEGACY_ZMX_BINARY_STAMP_KEY: &str = "zmxBinaryStamp";

/// The generation every binary carrying a legacy identity stamp speaks. See
/// `WIRE_GENERATION` in `.dependencies/zmx/src/ipc.zig`.
const LEGACY_STAMP_WIRE_GENERATION: u32 = 1;

/// Line key `zmx version` prints the generation under.
const ZMX_VERSION_WIRE_GENERATION_KEY: &str = "wire_generation";

/// `zmx version` is a local, non-blocking print; anything slower is a wedged
/// shell, not a slow binary.
const ZMX_VERSION_TIMEOUT_MS: u64 = 5_000;

/// How long a directly signalled daemon gets to exit before it is escalated,
/// and again before the cycle is reported as failed.
#[cfg(unix)]
const ZMX_DAEMON_TERMINATION_GRACE: Duration = Duration::from_millis(2_000);

struct ZmxWireGenerationCacheEntry {
    executable_path: String,
    length: u64,
    modified: Option<SystemTime>,
    wire_generation: u32,
}

static ZMX_WIRE_GENERATION_CACHE: Mutex<Option<ZmxWireGenerationCacheEntry>> = Mutex::new(None);

/// Wire generation declared by the zmx binary at `zmx_executable_path`, cached
/// until the file changes. `None` when the binary cannot be run or predates
/// the `wire_generation` line, in which case no caller may conclude anything
/// about a daemon's compatibility.
pub(crate) fn current_zmx_wire_generation(zmx_executable_path: &str) -> Option<u32> {
    let metadata = fs::metadata(zmx_executable_path).ok()?;
    let length = metadata.len();
    let modified = metadata.modified().ok();
    let mut cache = ZMX_WIRE_GENERATION_CACHE.lock().ok()?;
    if let Some(cached) = cache.as_ref() {
        if cached.executable_path == zmx_executable_path
            && cached.length == length
            && cached.modified == modified
        {
            return Some(cached.wire_generation);
        }
    }
    let wire_generation = read_zmx_wire_generation(zmx_executable_path)?;
    *cache = Some(ZmxWireGenerationCacheEntry {
        executable_path: zmx_executable_path.to_string(),
        length,
        modified,
        wire_generation,
    });
    Some(wire_generation)
}

/// Runs `zmx version` through the same profile-free shell spawn as the probe
/// reads, so a Windows host reaches its WSL-side binary the same way.
fn read_zmx_wire_generation(zmx_executable_path: &str) -> Option<u32> {
    let script = format!(
        "unset ZMX_SESSION ZMX_SESSION_PREFIX\nexec {} version",
        shell_quote(zmx_executable_path)
    );
    let result = run_zmx_probe_script(
        script,
        ZmxCommandOptions {
            timeout_ms: Some(ZMX_VERSION_TIMEOUT_MS),
            ..ZmxCommandOptions::default()
        },
    )
    .ok()?;
    if result.exit_code != 0 {
        return None;
    }
    parse_zmx_wire_generation(&result.stdout)
}

/// Extracts `wire_generation\t<n>` from `zmx version` output. Missing line or
/// non-numeric value means the binary predates the generation scheme.
pub(crate) fn parse_zmx_wire_generation(version_output: &str) -> Option<u32> {
    version_output.lines().find_map(|line| {
        let (key, value) = line.split_once('\t')?;
        if key.trim() != ZMX_VERSION_WIRE_GENERATION_KEY {
            return None;
        }
        value.trim().parse::<u32>().ok()
    })
}

/// Generation recorded on a session, or `None` for a daemon started before any
/// stamp existed. A legacy binary-identity stamp resolves to generation 1.
fn recorded_zmx_wire_generation(session: &Value) -> Option<u32> {
    let provider_state = session.get("providerState").and_then(Value::as_object)?;
    if let Some(generation) = provider_state
        .get(ZMX_WIRE_GENERATION_KEY)
        .and_then(Value::as_u64)
    {
        return u32::try_from(generation).ok();
    }
    provider_state
        .get(LEGACY_ZMX_BINARY_STAMP_KEY)
        .and_then(Value::as_str)
        .filter(|stamp| !stamp.is_empty())
        .map(|_| LEGACY_STAMP_WIRE_GENERATION)
}

/// Provider state for a session whose daemon this gxserver just started, with
/// the spawning binary's wire generation recorded. Every provider start must go
/// through here: an unstamped live daemon is cycled on the next startup.
pub(crate) fn started_provider_state_patch(
    session: &Value,
    probe: &ProviderProbe,
    zmx_executable_path: &str,
) -> Result<Map<String, Value>, DomainStateError> {
    let mut provider_state = provider_state_patch(session, probe)?;
    provider_state.remove(LEGACY_ZMX_BINARY_STAMP_KEY);
    match current_zmx_wire_generation(zmx_executable_path) {
        Some(wire_generation) => {
            provider_state.insert(
                ZMX_WIRE_GENERATION_KEY.to_string(),
                Value::from(wire_generation),
            );
        }
        None => {
            provider_state.remove(ZMX_WIRE_GENERATION_KEY);
        }
    }
    Ok(provider_state)
}

/// Terminates every live daemon this gxserver owns that a zmx of a different
/// wire generation spawned, and leaves its session sleeping so the ordinary
/// wake path restores it. Runs before the listener starts serving so no client can attach to a
/// daemon that is about to be cycled.
pub(crate) fn cycle_wire_incompatible_zmx_session_daemons(
    repository: &DomainRepository<'_>,
    logger: &GxserverLogger,
    server_id: &str,
) {
    let Ok(zmx) = require_zmx() else {
        return;
    };
    let Some(current_generation) = current_zmx_wire_generation(&zmx.executable_path) else {
        let _ = logger.log(GxserverLogInput {
            level: LogLevel::Warn,
            event: "zmxWireGenerationUnreadable".to_string(),
            server_id: Some(server_id.to_string()),
            request_id: None,
            client: None,
            duration_ms: None,
            error: Some(
                "The bundled zmx binary did not report a wire generation, so daemons spawned by an incompatible zmx cannot be identified and none were cycled.".to_string(),
            ),
            details: Some(json!({ "zmxExecutablePath": zmx.executable_path })),
        });
        return;
    };
    let Ok(sessions) = repository.list_sessions(None) else {
        return;
    };
    for session in sessions {
        let (Some(project_id), Some(session_id)) = (
            string_field(&session, "projectId"),
            string_field(&session, "sessionId"),
        ) else {
            continue;
        };
        let Ok(zmx_name) = provider_zmx_session_name(&session) else {
            continue;
        };
        // Liveness is the socket file, not an IPC probe: an incompatible daemon
        // cannot answer, and a busy compatible one must never be mistaken for a
        // dead one.
        if zmx_session_daemon_socket_present(&zmx_name) != Some(true) {
            continue;
        }
        let recorded_generation = recorded_zmx_wire_generation(&session);
        if recorded_generation == Some(current_generation) {
            continue;
        }
        // A closed session keeps its terminal state; only a session that can be
        // woken becomes sleeping.
        let target_lifecycle =
            if string_field(&session, "lifecycleState").as_deref() == Some("stopped") {
                "stopped"
            } else {
                "sleeping"
            };
        let lifecycle = LifecycleParams {
            project_id: project_id.clone(),
            session_id: session_id.clone(),
        };
        let outcome = cycle_wire_incompatible_zmx_session_daemon(
            repository,
            &lifecycle,
            &zmx_name,
            target_lifecycle,
        );
        let details = json!({
            "currentZmxWireGeneration": current_generation,
            "lifecycleState": target_lifecycle,
            "projectId": project_id,
            "providerSessionId": zmx_name,
            "recordedZmxWireGeneration": recorded_generation,
            "sessionId": session_id,
            "termination": outcome.as_deref().unwrap_or("failed"),
        });
        let _ = logger.log(GxserverLogInput {
            level: LogLevel::Warn,
            event: match outcome {
                Ok(_) => "zmxIncompatibleSessionDaemonCycled",
                Err(_) => "zmxIncompatibleSessionDaemonCycleFailed",
            }
            .to_string(),
            server_id: Some(server_id.to_string()),
            request_id: None,
            client: None,
            duration_ms: None,
            error: match &outcome {
                Ok(_) => Some(format!(
                    "This session's terminal was started by a zmx that speaks a different wire generation than the one Ghostex now bundles (generation {current_generation}), so the two cannot talk. It was put to sleep and will be restored the next time it is opened. ({})",
                    recorded_generation
                        .map(|generation| format!("started with generation {generation}"))
                        .unwrap_or_else(|| "started before Ghostex recorded which zmx built a session".to_string()),
                )),
                Err(error) => Some(format!(
                    "This session's terminal was started by a zmx that speaks a different wire generation than the one Ghostex now bundles (generation {current_generation}) and could not be stopped, so it may render blank until it is closed: {error}"
                )),
            },
            details: Some(details),
        });
    }
}

/// Terminates one incompatible daemon and records the ordinary sleeping state.
/// Returns how the daemon was stopped.
fn cycle_wire_incompatible_zmx_session_daemon(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
    zmx_name: &str,
    target_lifecycle: &str,
) -> Result<String, String> {
    /*
    `zmx kill` rides the frozen upstream Kill tag and only needs the session's
    socket file, so it reaches pre-wire-break daemons and leaves exactly the
    state an auto-sleep would. It is still not guaranteed: it blocks on the
    daemon's hangup, so a daemon wedged in an older event loop trips the
    lifecycle command timeout instead of dying.
    */
    let killed = kill_and_cache_session_provider(repository, lifecycle, target_lifecycle)
        .map(|(kill, _)| kill.killed)
        .map_err(zmx_endpoint_error_message)?;
    if killed && zmx_session_daemon_socket_present(zmx_name) != Some(true) {
        return Ok("killed".to_string());
    }
    terminate_zmx_session_daemon_process(zmx_name)?;
    apply_cycled_session_provider_state(repository, lifecycle, target_lifecycle)
        .map_err(|error| error.message)?;
    Ok("terminated".to_string())
}

/// Rewrites the row a failed `zmx kill` left in `unknown`, so a cycled session
/// is indistinguishable from one the user slept.
fn apply_cycled_session_provider_state(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
    target_lifecycle: &str,
) -> Result<Value, DomainStateError> {
    let session = require_session(repository, lifecycle)?;
    let provider_state = missing_provider_state_patch(&session, &now_iso())?;
    let mut update = Map::new();
    update.insert("projectId".to_string(), json!(lifecycle.project_id));
    update.insert("sessionId".to_string(), json!(lifecycle.session_id));
    update.insert("lifecycleState".to_string(), json!(target_lifecycle));
    update.insert("providerState".to_string(), Value::Object(provider_state));
    repository.update_session_for_lifecycle(&update)
}

/*
A daemon that will not answer the frozen Kill tag is signalled directly. Its
own argv (`<zmx> run <name> -d …`) survives daemonization, so `ps` identifies
the process without asking the daemon anything. zmx frees a session's socket
only on its own graceful shutdown, so the socket is unlinked afterwards or the
name stays unusable and the restored session cannot claim it back.
*/
#[cfg(unix)]
fn terminate_zmx_session_daemon_process(zmx_name: &str) -> Result<(), String> {
    let ps_output = read_process_snapshot()?;
    let Some(process_id) = find_zmx_daemon_process_id(&ps_output, zmx_name) else {
        // Nothing owns the name any more; the socket is all that is left.
        remove_zmx_session_socket(zmx_name);
        return Ok(());
    };
    unsafe { libc::kill(process_id as libc::pid_t, libc::SIGTERM) };
    if !wait_for_process_exit(process_id, ZMX_DAEMON_TERMINATION_GRACE) {
        unsafe { libc::kill(process_id as libc::pid_t, libc::SIGKILL) };
        if !wait_for_process_exit(process_id, ZMX_DAEMON_TERMINATION_GRACE) {
            return Err(format!(
                "the zmx daemon process {process_id} did not exit after SIGTERM and SIGKILL"
            ));
        }
    }
    remove_zmx_session_socket(zmx_name);
    Ok(())
}

/*
Windows gxserver runs its daemons inside WSL, whose socket namespace it cannot
reach, so `zmx_session_daemon_socket_present` never reports a live daemon there
and this is unreachable. It stays explicit rather than silently succeeding.
*/
#[cfg(not(unix))]
fn terminate_zmx_session_daemon_process(_zmx_name: &str) -> Result<(), String> {
    Err("zmx daemons cannot be signalled directly on this platform".to_string())
}

#[cfg(unix)]
fn wait_for_process_exit(process_id: i64, grace: Duration) -> bool {
    let deadline = Instant::now() + grace;
    loop {
        if unsafe { libc::kill(process_id as libc::pid_t, 0) } != 0 {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// Raw process table, read without a shell so a wedged daemon cannot be reached
/// through `zmx list` on the way.
#[cfg(unix)]
fn read_process_snapshot() -> Result<String, String> {
    let output = Command::new("ps")
        .args(["-axo", "pid=,ppid=,command="])
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("ps failed: {error}"))?;
    if !output.status.success() {
        return Err(format!("ps exited {}", output.status.code().unwrap_or(-1)));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn zmx_endpoint_error_message(error: ZmxEndpointError) -> String {
    match error {
        ZmxEndpointError::DependencyUnavailable(message) => message,
        ZmxEndpointError::Domain(error) => error.message,
    }
}
