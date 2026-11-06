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
/// reads, using the native wmx invocation on Windows.
fn read_zmx_wire_generation(zmx_executable_path: &str) -> Option<u32> {
    #[cfg(windows)]
    let script = super::scripts_windows::version_command(zmx_executable_path);
    #[cfg(not(windows))]
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

/// Provider state for a session whose daemon this gxserver just started, with
/// the spawning binary's wire generation recorded. Every provider start must go
/// through here: an unstamped live daemon is cycled on the next startup.
pub(crate) fn started_provider_state_patch(
    session: &Value,
    probe: &ProviderProbe,
    zmx_executable_path: &str,
) -> Result<Map<String, Value>, DomainStateError> {
    let mut provider_state = provider_state_patch(session, probe)?;
    #[cfg(windows)]
    {
        provider_state.insert("terminalBackend".into(), json!("nativeWindows"));
        provider_state.insert("nativeSessionProtocol".into(), json!(1));
    }
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

/// What the wire cycle reads of one session row.
struct WireCycleRow {
    project_id: String,
    session_id: String,
    stopped: bool,
    recorded_generation: Option<u32>,
}

/// Every session row's ids, whether it is closed, and its recorded wire generation, in
/// `list_sessions` order, read without hydrating any row.
///
/// CDXC:ZmxWireGeneration 2026-10-23 WHY:
/// The pass hydrated every session row (about 148 MB for 10,900 rows) at the same moment as the other startup passes, which made gxserver's startup peak 300 to 650 MB, to read three provider-state keys and the lifecycle of each. SQLite now reads just those, and the generation is decided exactly as the hydrated row's `providerState` decided it (`stored_zmx_wire_generation`; its zmx name is the derived `create_zmx_session_name`, as hydration makes it). Every row is still a candidate, closed ones included.
fn list_wire_cycle_rows(
    repository: &DomainRepository<'_>,
) -> Result<Vec<WireCycleRow>, DomainStateError> {
    let mut statement = repository
        .db
        .prepare(&format!(
            "SELECT projectId, sessionId, lifecycleState,
                    json_type(providerStateJson, '$.{ZMX_WIRE_GENERATION_KEY}'),
                    CASE WHEN json_type(providerStateJson, '$.{ZMX_WIRE_GENERATION_KEY}') = 'integer'
                         THEN json_extract(providerStateJson, '$.{ZMX_WIRE_GENERATION_KEY}') END,
                    CASE WHEN json_type(providerStateJson, '$.{LEGACY_ZMX_BINARY_STAMP_KEY}') = 'text'
                         THEN json_extract(providerStateJson, '$.{LEGACY_ZMX_BINARY_STAMP_KEY}') END,
                    CASE WHEN json_type(providerStateJson, '$.nativeSessionProtocol') = 'integer'
                         THEN json_extract(providerStateJson, '$.nativeSessionProtocol') END
             FROM sessions
             ORDER BY updatedAt DESC, projectId ASC, sessionId ASC"
        ))
        .map_err(crate::domain::sql_error)?;
    let rows = statement
        .query_map([], |row| {
            let generation_type: Option<String> = row.get(3)?;
            let generation: Option<i64> = row.get(4)?;
            let legacy_stamp: Option<String> = row.get(5)?;
            let native_protocol: Option<i64> = row.get(6)?;
            Ok(WireCycleRow {
                project_id: row.get(0)?,
                session_id: row.get(1)?,
                stopped: row.get::<_, String>(2)? == "stopped",
                recorded_generation: stored_zmx_wire_generation(
                    generation_type.as_deref(),
                    generation,
                    legacy_stamp.as_deref(),
                    native_protocol,
                ),
            })
        })
        .map_err(crate::domain::sql_error)?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(crate::domain::sql_error)
}

/// Generation recorded on a session, or `None` for a daemon started before any stamp existed,
/// from its stored provider-state values: a non-negative integer generation wins (or is unusable
/// past `u32`), then Windows' native protocol 1, then a non-empty legacy binary-identity stamp,
/// which resolves to generation 1.
fn stored_zmx_wire_generation(
    generation_type: Option<&str>,
    generation: Option<i64>,
    legacy_stamp: Option<&str>,
    native_protocol: Option<i64>,
) -> Option<u32> {
    if generation_type == Some("integer") {
        if let Some(generation) = generation.filter(|generation| *generation >= 0) {
            return u32::try_from(generation).ok();
        }
    }
    // CDXC:ZmxWireGeneration 2026-09-14 WHY:
    // The extracted Windows host and its predecessor share generation 1. Import the native protocol stamp so installing wmx preserves running agents.
    #[cfg(windows)]
    if native_protocol == Some(1) {
        return Some(1);
    }
    #[cfg(not(windows))]
    let _ = native_protocol;
    legacy_stamp
        .filter(|stamp| !stamp.is_empty())
        .map(|_| LEGACY_STAMP_WIRE_GENERATION)
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
    let Ok(rows) = list_wire_cycle_rows(repository) else {
        return;
    };
    // A session stamped with the current generation is compatible whether or not its daemon lives, so only the others are probed.
    let candidates: Vec<_> = rows
        .into_iter()
        .filter(|row| row.recorded_generation != Some(current_generation))
        .map(|row| {
            let zmx_name =
                crate::ids::create_zmx_session_name(server_id, &row.project_id, &row.session_id);
            (
                row.project_id,
                row.session_id,
                zmx_name,
                row.recorded_generation,
                row.stopped,
            )
        })
        .collect();
    let live = zmx_session_daemons_present(
        &candidates
            .iter()
            .map(|(_, _, zmx_name, _, _)| zmx_name.as_str())
            .collect::<Vec<_>>(),
    );
    let live_candidates: Vec<_> = candidates
        .into_iter()
        .zip(live)
        .filter_map(|(candidate, live)| live.then_some(candidate))
        .collect();
    let outcomes = cycle_wire_incompatible_zmx_session_daemon_batch(
        repository,
        &live_candidates
            .iter()
            .map(|(project_id, session_id, zmx_name, _, stopped)| {
                (
                    LifecycleParams {
                        project_id: project_id.clone(),
                        session_id: session_id.clone(),
                    },
                    zmx_name.as_str(),
                    wire_cycle_target_lifecycle(*stopped),
                )
            })
            .collect::<Vec<_>>(),
    );
    for ((project_id, session_id, zmx_name, recorded_generation, stopped), outcome) in
        live_candidates.into_iter().zip(outcomes)
    {
        let target_lifecycle = wire_cycle_target_lifecycle(stopped);
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

/// Whether each named daemon's socket is present. Liveness is the socket, not
/// an IPC probe: an incompatible daemon cannot answer, and a busy compatible
/// one must never be mistaken for a dead one.
///
/// CDXC:ZmxWireGeneration 2026-10-12 WHY:
/// On Windows each check starts a `wmx inspect` process (0.3–0.8 s on the user's machine), and this pass ran it for every one of 1,112 stored sessions one after another before checking the recorded generation, so gxserver held its port without serving for about 150 s on every start: health probes timed out, the app showed its startup diagnostics, and CLI calls failed. Only sessions whose stamp differs are checked now, several at a time.
fn zmx_session_daemons_present(zmx_names: &[&str]) -> Vec<bool> {
    const PARALLEL_CHECKS: usize = 16;
    let mut present = Vec::with_capacity(zmx_names.len());
    for chunk in zmx_names.chunks(PARALLEL_CHECKS) {
        std::thread::scope(|scope| {
            let checks: Vec<_> = chunk
                .iter()
                .map(|zmx_name| {
                    scope.spawn(move || zmx_session_daemon_socket_present(zmx_name) == Some(true))
                })
                .collect();
            present.extend(
                checks
                    .into_iter()
                    .map(|check| check.join().unwrap_or(false)),
            );
        });
    }
    present
}

/// A closed session keeps its terminal state; only a session that can be woken
/// becomes sleeping.
fn wire_cycle_target_lifecycle(stopped: bool) -> &'static str {
    if stopped {
        "stopped"
    } else {
        "sleeping"
    }
}

/// Ends every incompatible daemon in one go and records each session's sleeping
/// (or closed) state. Returns how each daemon was stopped, in order.
///
/// CDXC:ZmxWireGeneration 2026-10-25 WHY:
/// The pass used to send each daemon `zmx kill`, trusting the Kill tag to be frozen across generations. Generation 3 changed the IPC header itself (protocol version and reserved bytes before the length), so a generation-2 daemon reads a new client's Kill as a header asking for one more payload byte and waits for it: every kill sat out the 5 s command timeout, then a `zmx list` probe, then fell back to SIGTERM, about 8 s per session one after another while gxserver answered "starting". After the 10.33.0 update 40 sessions held gxserver in "starting" for about 270 s, past the app's 180 s patience, so it showed that gxserver failed to start, and a reporter restarted Ghostex four or five times before their sessions came back. A daemon of another generation is now never spoken to: SIGTERM is the daemon's own graceful shutdown (socket unlinked, SIGHUP then SIGKILL to the shell, as `Kill` does) in every zmx generation, so all of them are signalled at once from one process snapshot and the pass takes about a second however many sessions there are.
#[cfg(unix)]
fn cycle_wire_incompatible_zmx_session_daemon_batch(
    repository: &DomainRepository<'_>,
    daemons: &[(LifecycleParams, &str, &str)],
) -> Vec<Result<String, String>> {
    let terminations = terminate_zmx_session_daemon_processes(
        &daemons
            .iter()
            .map(|(_, zmx_name, _)| *zmx_name)
            .collect::<Vec<_>>(),
    );
    daemons
        .iter()
        .zip(terminations)
        .map(|((lifecycle, _, target_lifecycle), termination)| {
            termination?;
            apply_cycled_session_provider_state(repository, lifecycle, target_lifecycle)
                .map_err(|error| error.message)?;
            Ok("terminated".to_string())
        })
        .collect()
}

#[cfg(windows)]
fn cycle_wire_incompatible_zmx_session_daemon_batch(
    repository: &DomainRepository<'_>,
    daemons: &[(LifecycleParams, &str, &str)],
) -> Vec<Result<String, String>> {
    daemons
        .iter()
        .map(|(lifecycle, zmx_name, target_lifecycle)| {
            cycle_wire_incompatible_zmx_session_daemon(
                repository,
                lifecycle,
                zmx_name,
                target_lifecycle,
            )
        })
        .collect()
}

/// Terminates one incompatible Windows session host and records the ordinary
/// sleeping state. Returns how it was stopped.
#[cfg(windows)]
fn cycle_wire_incompatible_zmx_session_daemon(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
    zmx_name: &str,
    target_lifecycle: &str,
) -> Result<String, String> {
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
Daemons are signalled, never asked over IPC. A daemon's own argv
(`<zmx> run <name> -d …`) survives daemonization, so `ps` identifies the
process without asking the daemon anything. SIGTERM runs zmx's graceful
shutdown; a daemon still alive after the grace gets SIGKILL, which skips it, so
the socket is unlinked afterwards or the name stays unusable and the restored
session cannot claim it back. The session's launchd job is retired as
`zmx kill` callers do.
*/
#[cfg(unix)]
pub(super) fn terminate_zmx_session_daemon_processes(
    zmx_names: &[&str],
) -> Vec<Result<(), String>> {
    let ps_output = match read_process_snapshot() {
        Ok(ps_output) => ps_output,
        Err(error) => return zmx_names.iter().map(|_| Err(error.clone())).collect(),
    };
    let process_ids: Vec<Option<i64>> = zmx_names
        .iter()
        .map(|zmx_name| find_zmx_daemon_process_id(&ps_output, zmx_name))
        .collect();
    let mut running: Vec<i64> = process_ids.iter().flatten().copied().collect();
    for process_id in &running {
        unsafe { libc::kill(*process_id as libc::pid_t, libc::SIGTERM) };
    }
    running = wait_for_processes_exit(running, ZMX_DAEMON_TERMINATION_GRACE);
    for process_id in &running {
        unsafe { libc::kill(*process_id as libc::pid_t, libc::SIGKILL) };
    }
    running = wait_for_processes_exit(running, ZMX_DAEMON_TERMINATION_GRACE);
    zmx_names
        .iter()
        .zip(process_ids)
        .map(|(zmx_name, process_id)| match process_id {
            Some(process_id) if running.contains(&process_id) => Err(format!(
                "the zmx daemon process {process_id} did not exit after SIGTERM and SIGKILL"
            )),
            // Exited, or nothing owned the name any more and the socket is all that is left.
            _ => {
                remove_zmx_session_socket(zmx_name);
                #[cfg(target_os = "macos")]
                cleanup_macos_zmx_launchd_job(zmx_name);
                Ok(())
            }
        })
        .collect()
}

#[cfg(windows)]
fn terminate_zmx_session_daemon_process(zmx_name: &str) -> Result<(), String> {
    let zmx = require_zmx().map_err(zmx_endpoint_error_message)?;
    let result = run_zmx_probe_script(
        super::scripts_windows::force_kill_command(&zmx.executable_path, zmx_name),
        ZmxCommandOptions {
            timeout_ms: Some(10_000),
            ..ZmxCommandOptions::default()
        },
    )?;
    if result.exit_code != 0 {
        return Err("The incompatible Windows session host could not be stopped.".into());
    }
    Ok(())
}

/// The processes still alive once they have all exited or `grace` has passed.
#[cfg(unix)]
fn wait_for_processes_exit(mut process_ids: Vec<i64>, grace: Duration) -> Vec<i64> {
    let deadline = Instant::now() + grace;
    loop {
        process_ids.retain(|process_id| unsafe { libc::kill(*process_id as libc::pid_t, 0) } == 0);
        if process_ids.is_empty() || Instant::now() >= deadline {
            return process_ids;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// Raw process table, read without a shell so a wedged daemon cannot be reached
/// through `zmx list` on the way.
#[cfg(unix)]
pub(super) fn read_process_snapshot() -> Result<String, String> {
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

#[cfg(windows)]
fn zmx_endpoint_error_message(error: ZmxEndpointError) -> String {
    match error {
        ZmxEndpointError::DependencyUnavailable(message) => message,
        ZmxEndpointError::Domain(error) => error.message,
    }
}

/// Ends every live session daemon named for this gxserver that no session row owns, before any client can attach. Runs beside the wire-generation pass at startup.
///
/// CDXC:Resources 2026-10-24 WHY:
/// The user's rule is "we shouldn't have orphaned ones" (`resource_session_owners`). A daemon whose row is gone (a create the previous gxserver died in the middle of, or a row removed while gxserver was down) was never ended: nothing restores it and nothing lists it but Resources, so it kept its agent, its endpoint record and, on Windows, its `windows-sessions` lock forever, and the reporter of GitHub #289 had to end them by hand. Only daemons with this server's name prefix are touched, a name counts as owned when it is a row's derived name, its stored `zmxName` or its provider state's `zmxName`, and the pass reads the live list once, so a row that exists is never ended.
pub(crate) fn end_unowned_zmx_session_daemons(
    repository: &DomainRepository<'_>,
    logger: &GxserverLogger,
    server_id: &str,
) {
    // A shared socket folder holds other profiles' daemons, which may carry this server's id (CDXC:ServerDaemon 2026-11-06 in screen_capture.rs).
    #[cfg(unix)]
    if super::zmx_socket_directory_is_shared() {
        return;
    }
    let Ok(live) = read_zmx_existing_session_names() else {
        return;
    };
    let Ok(owned) = owned_zmx_session_names(repository, server_id) else {
        return;
    };
    let unowned = unowned_zmx_session_names(live, &owned, server_id);
    if unowned.is_empty() {
        return;
    }
    // Signalled like the wire cycle's daemons: an unowned daemon may be of an older generation too.
    #[cfg(unix)]
    let outcomes: Vec<Result<String, String>> = terminate_zmx_session_daemon_processes(
        &unowned.iter().map(String::as_str).collect::<Vec<_>>(),
    )
    .into_iter()
    .map(|termination| termination.map(|()| "terminated".to_string()))
    .collect();
    #[cfg(windows)]
    let outcomes: Vec<Result<String, String>> = {
        let Ok(zmx) = require_zmx() else {
            return;
        };
        unowned
            .iter()
            .map(|zmx_name| {
                let kill = kill_zmx_session(zmx_name, &zmx.executable_path);
                if kill.killed && zmx_session_daemon_socket_present(zmx_name) != Some(true) {
                    Ok("killed".to_string())
                } else {
                    terminate_zmx_session_daemon_process(zmx_name)
                        .map(|()| "terminated".to_string())
                }
            })
            .collect()
    };
    for (zmx_name, outcome) in unowned.into_iter().zip(outcomes) {
        let _ = logger.log(GxserverLogInput {
            level: LogLevel::Warn,
            event: match outcome {
                Ok(_) => "unownedSessionDaemonEnded",
                Err(_) => "unownedSessionDaemonEndFailed",
            }
            .to_string(),
            server_id: Some(server_id.to_string()),
            request_id: None,
            client: None,
            duration_ms: None,
            error: Some(match &outcome {
                Ok(_) => "A session terminal was still running for a session Ghostex no longer has, so it was ended.".to_string(),
                Err(error) => format!("A session terminal is still running for a session Ghostex no longer has and could not be ended: {error}"),
            }),
            details: Some(json!({
                "providerSessionId": zmx_name,
                "termination": outcome.as_deref().unwrap_or("failed"),
            })),
        });
    }
}

/// Every zmx name a session row of this server can own.
fn owned_zmx_session_names(
    repository: &DomainRepository<'_>,
    server_id: &str,
) -> Result<std::collections::HashSet<String>, DomainStateError> {
    let mut statement = repository
        .db
        .prepare(
            "SELECT projectId, sessionId, zmxName,
                    CASE WHEN json_type(providerStateJson, '$.zmxName') = 'text'
                         THEN json_extract(providerStateJson, '$.zmxName') END
             FROM sessions",
        )
        .map_err(crate::domain::sql_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })
        .map_err(crate::domain::sql_error)?;
    let mut owned = std::collections::HashSet::new();
    for row in rows {
        let (project_id, session_id, stored, provider_state) =
            row.map_err(crate::domain::sql_error)?;
        owned.insert(crate::ids::create_zmx_session_name(
            server_id,
            &project_id,
            &session_id,
        ));
        owned.extend(stored.into_iter().chain(provider_state));
    }
    Ok(owned)
}

/// The live names with this server's prefix that no row owns, sorted.
pub(crate) fn unowned_zmx_session_names(
    live: impl IntoIterator<Item = String>,
    owned: &std::collections::HashSet<String>,
    server_id: &str,
) -> Vec<String> {
    let prefix = format!("{server_id}-");
    let mut unowned: Vec<String> = live
        .into_iter()
        .map(|name| name.trim().to_string())
        .filter(|name| name.len() > prefix.len() && name.starts_with(&prefix))
        .filter(|name| !owned.contains(name))
        .collect();
    unowned.sort();
    unowned.dedup();
    unowned
}

#[cfg(test)]
mod unowned_daemon_tests {
    use super::*;

    #[test]
    fn only_this_servers_daemons_without_a_row_are_ended() {
        let owned: std::collections::HashSet<String> =
            ["S1b-P1-G1".to_string(), "S1b-P1-G2".to_string()].into();
        let live = [
            "S1b-P1-G1",
            "S1b-P1-G9",
            "S2c-P1-G9",
            "S1b-",
            "S1bx-P1-G3",
            " S1b-P2-G4 ",
            "S1b-P1-G9",
        ]
        .map(str::to_string);
        assert_eq!(
            unowned_zmx_session_names(live, &owned, "S1b"),
            vec!["S1b-P1-G9".to_string(), "S1b-P2-G4".to_string()]
        );
    }

    #[test]
    fn a_stored_name_keeps_its_daemon() {
        let db = rusqlite::Connection::open_in_memory().expect("db");
        db.execute_batch(
            "CREATE TABLE sessions (projectId TEXT, sessionId TEXT, zmxName TEXT, providerStateJson TEXT);
             INSERT INTO sessions VALUES ('P1', 'G1', 'S1b-P0-G1', '{}');
             INSERT INTO sessions VALUES ('P2', 'G2', '', '{\"zmxName\":\"S1b-P9-G2\"}');",
        )
        .expect("rows");
        let repository = DomainRepository::new(&db, "S1b");
        let owned = owned_zmx_session_names(&repository, "S1b").expect("owned");
        let live = [
            "S1b-P1-G1",
            "S1b-P0-G1",
            "S1b-P9-G2",
            "S1b-P2-G2",
            "S1b-P3-G3",
        ]
        .map(str::to_string);
        assert_eq!(
            unowned_zmx_session_names(live, &owned, "S1b"),
            vec!["S1b-P3-G3".to_string()]
        );
    }
}
